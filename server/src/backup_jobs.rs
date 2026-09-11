//! Materialization of unified backup jobs onto the PVE remotes.
//!
//! A [`BackupJobConfig`] is translated into one native `cluster/backup` job per
//! PVE remote that owns at least one of the selected guests. PVE keeps running
//! the schedule on its own, so backups survive a PDM outage.

use std::collections::{BTreeMap, HashMap, HashSet};

use anyhow::{Error, bail, format_err};
use serde_json::{Value, json};

use proxmox_client::HttpApiClient;

use pdm_api_types::RemoteUpid;
use pdm_api_types::backup_jobs::{
    BackupJobConfig, BackupJobConfigEntry, BackupJobRemoteStatus, BackupJobSyncState,
    derived_job_id,
};
use pdm_api_types::remotes::RemoteType;

use crate::api::pve::{new_remote_upid, raw_client_to_remote_by_id};
use crate::api::resources::{CachedGuest, cached_pve_guests};

/// Maximum age of the cached resource list used to resolve guests.
const GUEST_CACHE_MAX_AGE: u64 = 300;

/// What a job should look like on one PVE remote.
pub struct RemotePlan {
    /// Target storage, if one could be resolved.
    pub storage: Option<String>,
    /// Selected guests, sorted and deduplicated.
    pub vmids: Vec<u32>,
    /// Node each guest currently lives on, used for immediate runs.
    pub nodes: HashMap<u32, String>,
}

/// The remotes a job has to be materialized on.
pub type JobPlan = BTreeMap<String, RemotePlan>;

/// List all PVE remote ids.
fn pve_remote_ids() -> Result<Vec<String>, Error> {
    let (remotes, _) = pdm_config::remotes::config()?;
    Ok(remotes
        .into_iter()
        .filter(|(_, remote)| remote.ty == RemoteType::Pve)
        .map(|(id, _)| id)
        .collect())
}

/// Resolve the guest selection of a job into a per-remote plan.
pub async fn build_plan(job: &BackupJobConfig) -> Result<JobPlan, Error> {
    let guests = cached_pve_guests(GUEST_CACHE_MAX_AGE).await?;

    let mut by_vmid: HashMap<u32, Vec<&CachedGuest>> = HashMap::new();
    for guest in &guests {
        by_vmid.entry(guest.vmid).or_default().push(guest);
    }

    // remote -> selected vmids
    let mut selected: BTreeMap<String, HashSet<u32>> = BTreeMap::new();

    for entry in &job.guests {
        // A guest that was migrated to another remote keeps its vmid, so prefer the
        // location the resource cache reports when it is unambiguous.
        let remote = match by_vmid.get(&entry.vmid) {
            Some(found) if job.follows_migrations() && found.len() == 1 => found[0].remote.clone(),
            _ => entry.remote.clone(),
        };
        selected.entry(remote).or_default().insert(entry.vmid);
    }

    if !job.tag_filters.is_empty() {
        for guest in &guests {
            if guest.template {
                continue;
            }
            if !job.tag_filter_remotes.is_empty()
                && !job.tag_filter_remotes.iter().any(|r| *r == guest.remote)
            {
                continue;
            }
            if guest
                .tags
                .iter()
                .any(|tag| job.tag_filters.iter().any(|filter| filter == tag))
            {
                selected
                    .entry(guest.remote.clone())
                    .or_default()
                    .insert(guest.vmid);
            }
        }
    }

    let mut plan = JobPlan::new();
    for (remote, vmids) in selected {
        if vmids.is_empty() {
            continue;
        }
        let mut nodes = HashMap::new();
        for guest in &guests {
            if guest.remote == remote && vmids.contains(&guest.vmid) {
                nodes.insert(guest.vmid, guest.node.clone());
            }
        }
        let mut vmids: Vec<u32> = vmids.into_iter().collect();
        vmids.sort_unstable();

        plan.insert(
            remote.clone(),
            RemotePlan {
                storage: job.storage_for(&remote).map(str::to_owned),
                vmids,
                nodes,
            },
        );
    }

    Ok(plan)
}

/// Build the `cluster/backup` payload for one remote.
fn desired_payload(job: &BackupJobConfig, plan: &RemotePlan) -> Value {
    let vmid = plan
        .vmids
        .iter()
        .map(|vmid| vmid.to_string())
        .collect::<Vec<_>>()
        .join(",");

    let mut payload = json!({
        "vmid": vmid,
        "enabled": if job.disable.unwrap_or(false) { 0 } else { 1 },
        "comment": job.comment.clone().unwrap_or_else(|| format!("PDM job '{}'", job.id)),
    });

    if !job.schedule.is_empty() {
        payload["schedule"] = job.schedule.clone().into();
    }
    if let Some(storage) = &plan.storage {
        payload["storage"] = storage.clone().into();
    }
    for (key, value) in [
        ("mode", &job.mode),
        ("compress", &job.compress),
        ("prune-backups", &job.prune_backups),
        ("notes-template", &job.notes_template),
        ("mailto", &job.mailto),
        ("mailnotification", &job.mailnotification),
    ] {
        if let Some(value) = value {
            payload[key] = value.clone().into();
        }
    }
    if let Some(bwlimit) = job.bwlimit {
        payload["bwlimit"] = bwlimit.into();
    }

    payload
}

/// PVE returns booleans as 0/1 integers or strings, depending on the endpoint.
fn as_bool(value: Option<&Value>) -> Option<bool> {
    match value? {
        Value::Bool(value) => Some(*value),
        Value::Number(number) => Some(number.as_i64()? != 0),
        Value::String(value) => match value.as_str() {
            "1" | "true" | "yes" => Some(true),
            "0" | "false" | "no" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

fn as_string(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(value) => Some(value.clone()),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        _ => None,
    }
}

fn normalized_vmid_list(value: Option<&Value>) -> Vec<u32> {
    let mut list: Vec<u32> = as_string(value)
        .unwrap_or_default()
        .split(',')
        .filter_map(|entry| entry.trim().parse().ok())
        .collect();
    list.sort_unstable();
    list.dedup();
    list
}

/// Compare the job currently configured on PVE with what PDM wants it to be.
fn is_in_sync(existing: &Value, desired: &Value) -> bool {
    if normalized_vmid_list(existing.get("vmid")) != normalized_vmid_list(desired.get("vmid")) {
        return false;
    }

    // an absent `enabled` means enabled on PVE
    if as_bool(existing.get("enabled")).unwrap_or(true) != as_bool(desired.get("enabled")).unwrap_or(true)
    {
        return false;
    }

    for key in [
        "schedule",
        "storage",
        "mode",
        "compress",
        "prune-backups",
        "notes-template",
        "mailto",
        "mailnotification",
        "bwlimit",
    ] {
        let desired_value = as_string(desired.get(key));
        if desired_value.is_some() && desired_value != as_string(existing.get(key)) {
            return false;
        }
    }

    // a job PDM owns must never select guests through `all` or `pool`
    if as_bool(existing.get("all")).unwrap_or(false) || existing.get("pool").is_some() {
        return false;
    }

    true
}

fn encode_id(id: &str) -> String {
    percent_encoding::percent_encode(id.as_bytes(), percent_encoding::NON_ALPHANUMERIC).to_string()
}

/// Read the `cluster/backup` job list of a remote as raw JSON.
///
/// Raw values are used on purpose: PVE is inconsistent about the types of its
/// boolean and numeric fields.
async fn list_remote_jobs(remote: &str) -> Result<Vec<Value>, Error> {
    let client = raw_client_to_remote_by_id(remote)?;
    let data: Value = client
        .get("/api2/extjs/cluster/backup")
        .await?
        .expect_json()?
        .data;
    match data {
        Value::Array(jobs) => Ok(jobs),
        _ => bail!("unexpected response from cluster/backup"),
    }
}

fn find_job<'a>(jobs: &'a [Value], id: &str) -> Option<&'a Value> {
    jobs.iter()
        .find(|job| job.get("id").and_then(Value::as_str) == Some(id))
}

async fn apply_on_remote(
    remote: &str,
    job: &BackupJobConfig,
    plan: &RemotePlan,
) -> Result<BackupJobSyncState, Error> {
    let derived_id = derived_job_id(&job.id);
    let jobs = list_remote_jobs(remote).await?;
    let mut payload = desired_payload(job, plan);

    let client = raw_client_to_remote_by_id(remote)?;
    if find_job(&jobs, &derived_id).is_some() {
        let path = format!("/api2/extjs/cluster/backup/{}", encode_id(&derived_id));
        // `all` and `pool` would widen the selection behind PDM's back
        payload["delete"] = "all,pool,exclude,node".into();
        client.put(&path, &payload).await?.nodata()?;
    } else {
        payload["id"] = derived_id.into();
        client
            .post("/api2/extjs/cluster/backup", &payload)
            .await?
            .nodata()?;
    }

    Ok(BackupJobSyncState::Synced)
}

async fn remove_on_remote(remote: &str, job_id: &str) -> Result<(), Error> {
    let derived_id = derived_job_id(job_id);
    let jobs = list_remote_jobs(remote).await?;
    if find_job(&jobs, &derived_id).is_none() {
        return Ok(());
    }

    let client = raw_client_to_remote_by_id(remote)?;
    let path = format!("/api2/extjs/cluster/backup/{}", encode_id(&derived_id));
    client.delete(&path).await?.nodata()?;
    Ok(())
}

/// Materialize a job on every PVE remote, removing it from the remotes it no
/// longer covers.
pub async fn sync_job(job: &BackupJobConfig) -> Result<Vec<BackupJobRemoteStatus>, Error> {
    let plan = build_plan(job).await?;
    let derived_id = derived_job_id(&job.id);
    let mut status = Vec::new();

    for remote in pve_remote_ids()? {
        match plan.get(&remote) {
            Some(remote_plan) => {
                let (state, error) = match apply_on_remote(&remote, job, remote_plan).await {
                    Ok(state) => (state, None),
                    Err(err) => (BackupJobSyncState::Error, Some(format!("{err:#}"))),
                };
                status.push(BackupJobRemoteStatus {
                    remote: remote.clone(),
                    job_id: derived_id.clone(),
                    state,
                    guest_count: remote_plan.vmids.len() as u32,
                    storage: remote_plan.storage.clone(),
                    error,
                });
            }
            None => {
                if let Err(err) = remove_on_remote(&remote, &job.id).await {
                    log::warn!("could not clean up backup job '{derived_id}' on '{remote}': {err:#}");
                }
            }
        }
    }

    Ok(status)
}

/// Remove the materialized job from every PVE remote.
pub async fn remove_job(job_id: &str) -> Result<(), Error> {
    let mut errors = Vec::new();
    for remote in pve_remote_ids()? {
        if let Err(err) = remove_on_remote(&remote, job_id).await {
            errors.push(format!("{remote}: {err:#}"));
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        bail!("could not remove the job from some remotes - {}", errors.join(", "))
    }
}

/// Report how the job is currently materialized, without changing anything.
pub async fn job_status(job: &BackupJobConfig) -> Result<Vec<BackupJobRemoteStatus>, Error> {
    let plan = build_plan(job).await?;
    let derived_id = derived_job_id(&job.id);
    let mut status = Vec::new();

    for (remote, remote_plan) in &plan {
        let mut entry = BackupJobRemoteStatus {
            remote: remote.clone(),
            job_id: derived_id.clone(),
            state: BackupJobSyncState::Missing,
            guest_count: remote_plan.vmids.len() as u32,
            storage: remote_plan.storage.clone(),
            error: None,
        };

        match list_remote_jobs(remote).await {
            Ok(jobs) => {
                if let Some(existing) = find_job(&jobs, &derived_id) {
                    entry.state = if is_in_sync(existing, &desired_payload(job, remote_plan)) {
                        BackupJobSyncState::Synced
                    } else {
                        BackupJobSyncState::OutOfSync
                    };
                }
            }
            Err(err) => {
                entry.state = BackupJobSyncState::Error;
                entry.error = Some(format!("{err:#}"));
            }
        }

        status.push(entry);
    }

    // remotes that still carry the job but are no longer part of the plan
    for remote in pve_remote_ids()? {
        if plan.contains_key(&remote) {
            continue;
        }
        if let Ok(jobs) = list_remote_jobs(&remote).await {
            if find_job(&jobs, &derived_id).is_some() {
                status.push(BackupJobRemoteStatus {
                    remote,
                    job_id: derived_id.clone(),
                    state: BackupJobSyncState::OutOfSync,
                    guest_count: 0,
                    storage: None,
                    error: Some("stale job, no guest of this remote is selected".to_string()),
                });
            }
        }
    }

    Ok(status)
}

/// Start an immediate vzdump run for every remote and node of the job.
pub async fn run_job(job: &BackupJobConfig) -> Result<Vec<RemoteUpid>, Error> {
    let plan = build_plan(job).await?;
    if plan.is_empty() {
        bail!("job does not select any guest");
    }

    let mut upids = Vec::new();
    let mut errors = Vec::new();

    for (remote, remote_plan) in &plan {
        let mut per_node: BTreeMap<String, Vec<u32>> = BTreeMap::new();
        for vmid in &remote_plan.vmids {
            let node = remote_plan
                .nodes
                .get(vmid)
                .ok_or_else(|| format_err!("unknown node for guest {vmid} on remote {remote}"))?;
            per_node.entry(node.clone()).or_default().push(*vmid);
        }

        for (node, vmids) in per_node {
            let mut payload = json!({
                "vmid": vmids.iter().map(u32::to_string).collect::<Vec<_>>().join(","),
            });
            if let Some(storage) = &remote_plan.storage {
                payload["storage"] = storage.clone().into();
            }
            for (key, value) in [
                ("mode", &job.mode),
                ("compress", &job.compress),
                ("prune-backups", &job.prune_backups),
                ("notes-template", &job.notes_template),
                ("mailto", &job.mailto),
            ] {
                if let Some(value) = value {
                    payload[key] = value.clone().into();
                }
            }
            if let Some(bwlimit) = job.bwlimit {
                payload["bwlimit"] = bwlimit.into();
            }

            let result = async {
                let client = raw_client_to_remote_by_id(remote)?;
                let path = format!("/api2/extjs/nodes/{node}/vzdump");
                let upid = client
                    .post(&path, &payload)
                    .await?
                    .expect_json::<pve_api_types::PveUpid>()?
                    .data;
                new_remote_upid(remote.clone(), upid).await
            }
            .await;

            match result {
                Ok(upid) => upids.push(upid),
                Err(err) => errors.push(format!("{remote}/{node}: {err:#}")),
            }
        }
    }

    if upids.is_empty() && !errors.is_empty() {
        bail!("could not start any backup - {}", errors.join(", "));
    }
    for error in errors {
        log::error!("backup job '{}' partially failed to start: {error}", job.id);
    }

    Ok(upids)
}

/// Rewrite guest entries whose guest was migrated to a different remote.
///
/// Returns the ids of the jobs that were changed.
pub async fn follow_migrated_guests() -> Result<Vec<String>, Error> {
    let guests = cached_pve_guests(GUEST_CACHE_MAX_AGE).await?;

    let mut location: HashMap<u32, Vec<String>> = HashMap::new();
    for guest in &guests {
        let entry = location.entry(guest.vmid).or_default();
        if !entry.contains(&guest.remote) {
            entry.push(guest.remote.clone());
        }
    }

    let _lock = pdm_config::backup_jobs::lock_config()?;
    let (mut config, _) = pdm_config::backup_jobs::config()?;

    let mut changed = Vec::new();
    let job_ids: Vec<String> = config
        .iter()
        .map(|(id, _)| id.to_string())
        .collect();

    for job_id in job_ids {
        let Some(BackupJobConfigEntry::BackupJob(job)) = config.get_mut(&job_id) else {
            continue;
        };
        if !job.follows_migrations() {
            continue;
        }

        let mut job_changed = false;
        for guest in job.guests.iter_mut() {
            if let Some(remotes) = location.get(&guest.vmid) {
                if remotes.len() == 1 && remotes[0] != guest.remote {
                    log::info!(
                        "backup job '{}': guest {} moved from '{}' to '{}'",
                        job_id,
                        guest.vmid,
                        guest.remote,
                        remotes[0]
                    );
                    guest.remote = remotes[0].clone();
                    job_changed = true;
                }
            }
        }

        if job_changed {
            changed.push(job_id);
        }
    }

    if !changed.is_empty() {
        pdm_config::backup_jobs::save_config(&config)?;
    }

    Ok(changed)
}

/// Re-materialize every configured job.
pub async fn reconcile_all() -> Result<(), Error> {
    if let Err(err) = follow_migrated_guests().await {
        log::error!("could not update migrated guests in backup jobs: {err:#}");
    }

    let (config, _) = pdm_config::backup_jobs::config()?;
    for (_, entry) in config {
        let BackupJobConfigEntry::BackupJob(job) = entry;
        if let Err(err) = sync_job(&job).await {
            log::error!("could not sync backup job '{}': {err:#}", job.id);
        }
    }

    Ok(())
}
