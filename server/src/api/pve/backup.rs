use anyhow::{Error, bail, format_err};
use serde_json::{Value, json};

use proxmox_client::HttpApiClient;
use proxmox_router::{Permission, Router};
use proxmox_schema::api;

use pdm_api_types::backup_restore::{BackupGuestType, PveBackupContent, PveRestoreRequest};
use pdm_api_types::pve_jobs::{PveBackupJob, PveBackupJobConfig, PveVzdumpRequest};
use pdm_api_types::remotes::REMOTE_ID_SCHEMA;
use pdm_api_types::{
    NODE_SCHEMA, PRIV_RESOURCE_AUDIT, PRIV_RESOURCE_CREATE, PRIV_RESOURCE_DELETE,
    PRIV_RESOURCE_MANAGE, PVE_STORAGE_ID_SCHEMA, RemoteUpid, VMID_SCHEMA,
};

use pve_api_types::ClusterNodeIndexResponseStatus;

use crate::api::pbs::storage_sync::{is_matching_storage, list_pve_storages, pbs_connection_info};

use super::{connect_to_remote_by_id, find_node_for_vm, new_remote_upid, raw_client_to_remote_by_id};

pub const ROUTER: Router = Router::new()
    .get(&API_METHOD_LIST_BACKUP_JOBS)
    .post(&API_METHOD_CREATE_BACKUP_JOB)
    .match_all("id", &ITEM_ROUTER);

const ITEM_ROUTER: Router = Router::new()
    .get(&API_METHOD_GET_BACKUP_JOB)
    .put(&API_METHOD_UPDATE_BACKUP_JOB)
    .delete(&API_METHOD_DELETE_BACKUP_JOB);

pub const VZDUMP_ROUTER: Router = Router::new().post(&API_METHOD_RUN_VZDUMP);

pub const CONTENT_ROUTER: Router = Router::new().get(&API_METHOD_LIST_BACKUP_CONTENT);

pub const RESTORE_ROUTER: Router = Router::new().post(&API_METHOD_RESTORE_BACKUP);

fn encode_id(id: &str) -> String {
    percent_encoding::percent_encode(id.as_bytes(), percent_encoding::NON_ALPHANUMERIC).to_string()
}

#[api(
    input: { properties: { remote: { schema: REMOTE_ID_SCHEMA } } },
    returns: {
        description: "Configured backup jobs.",
        type: Array,
        items: { type: PveBackupJob },
    },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}"], PRIV_RESOURCE_AUDIT, false),
    },
)]
/// List scheduled backup jobs on a PVE remote.
pub async fn list_backup_jobs(remote: String) -> Result<Vec<PveBackupJob>, Error> {
    let client = raw_client_to_remote_by_id(&remote)?;
    Ok(client
        .get("/api2/extjs/cluster/backup")
        .await?
        .expect_json()?
        .data)
}

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            config: { type: PveBackupJobConfig, flatten: true },
        },
    },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}"], PRIV_RESOURCE_CREATE, false),
    },
)]
/// Create a scheduled backup job on a PVE remote.
pub async fn create_backup_job(
    remote: String,
    config: PveBackupJobConfig,
) -> Result<(), Error> {
    let client = raw_client_to_remote_by_id(&remote)?;
    client
        .post("/api2/extjs/cluster/backup", &config)
        .await?
        .nodata()?;
    Ok(())
}

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            id: { description: "Backup job identifier.", type: String },
        },
    },
    returns: { type: PveBackupJob },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}"], PRIV_RESOURCE_AUDIT, false),
    },
)]
/// Read one scheduled backup job.
pub async fn get_backup_job(remote: String, id: String) -> Result<PveBackupJob, Error> {
    let client = raw_client_to_remote_by_id(&remote)?;
    let path = format!("/api2/extjs/cluster/backup/{}", encode_id(&id));
    Ok(client.get(&path).await?.expect_json()?.data)
}

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            id: { description: "Backup job identifier.", type: String },
            config: { type: PveBackupJobConfig, flatten: true },
        },
    },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}"], PRIV_RESOURCE_MANAGE, false),
    },
)]
/// Update one scheduled backup job.
pub async fn update_backup_job(
    remote: String,
    id: String,
    mut config: PveBackupJobConfig,
) -> Result<(), Error> {
    config.id = None;
    let client = raw_client_to_remote_by_id(&remote)?;
    let path = format!("/api2/extjs/cluster/backup/{}", encode_id(&id));
    client.put(&path, &config).await?.nodata()?;
    Ok(())
}

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            id: { description: "Backup job identifier.", type: String },
        },
    },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}"], PRIV_RESOURCE_DELETE, false),
    },
)]
/// Delete one scheduled backup job.
pub async fn delete_backup_job(remote: String, id: String) -> Result<(), Error> {
    let client = raw_client_to_remote_by_id(&remote)?;
    let path = format!("/api2/extjs/cluster/backup/{}", encode_id(&id));
    client.delete(&path).await?.nodata()?;
    Ok(())
}

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            request: { type: PveVzdumpRequest, flatten: true },
        },
    },
    returns: { type: RemoteUpid },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}"], PRIV_RESOURCE_MANAGE, false),
    },
)]
/// Start an immediate vzdump task on a PVE node.
pub async fn run_vzdump(
    remote: String,
    request: PveVzdumpRequest,
) -> Result<RemoteUpid, Error> {
    if request.vmid.is_none() && request.pool.is_none() && request.all != Some(true) {
        bail!("one of vmid, pool, or all must select backup guests");
    }
    let node = request.node.clone();
    let mut payload = serde_json::to_value(request)?;
    payload.as_object_mut().expect("request serializes as object").remove("node");
    let client = raw_client_to_remote_by_id(&remote)?;
    let path = format!("/api2/extjs/nodes/{node}/vzdump");
    let upid = client
        .post(&path, &payload)
        .await?
        .expect_json::<pve_api_types::PveUpid>()?
        .data;
    new_remote_upid(remote, upid).await
}

/// Node used to reach a storage: the one of the guest, otherwise any online one.
async fn pick_node(
    remote: &str,
    node: Option<String>,
    vmid: Option<u32>,
) -> Result<String, Error> {
    if let Some(node) = node {
        return Ok(node);
    }

    let pve = connect_to_remote_by_id(remote)?;
    if let Some(vmid) = vmid {
        if let Ok(node) = find_node_for_vm(None, vmid, pve.as_ref()).await {
            return Ok(node);
        }
    }

    pve.list_nodes()
        .await?
        .into_iter()
        .find(|node| node.status == ClusterNodeIndexResponseStatus::Online)
        .map(|node| node.node)
        .ok_or_else(|| format_err!("remote '{remote}' has no online node"))
}

fn value_string(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(value) if value.is_empty() => None,
        Value::String(value) => Some(value.clone()),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        Value::Object(map) => map
            .get("state")
            .and_then(Value::as_str)
            .map(str::to_owned),
        _ => None,
    }
}

fn value_bool(value: Option<&Value>) -> Option<bool> {
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

fn value_u64(value: Option<&Value>) -> Option<u64> {
    match value? {
        Value::Number(number) => number.as_u64(),
        Value::String(value) => value.parse().ok(),
        _ => None,
    }
}

/// Guest type of an archive, from the explicit subtype or from the volume ID.
fn guest_type_of(entry: &Value, volid: &str) -> Option<BackupGuestType> {
    match entry.get("subtype").and_then(Value::as_str) {
        Some("qemu") => return Some(BackupGuestType::Vm),
        Some("lxc") | Some("openvz") => return Some(BackupGuestType::Ct),
        _ => {}
    }

    if volid.contains("/vm/") || volid.contains("vzdump-qemu") {
        Some(BackupGuestType::Vm)
    } else if volid.contains("/ct/") || volid.contains("vzdump-lxc") || volid.contains("vzdump-openvz")
    {
        Some(BackupGuestType::Ct)
    } else {
        None
    }
}

/// Storages that can hold backups, optionally narrowed down to a single one.
fn backup_storage_ids(storages: &[Value], only: Option<&str>) -> Vec<String> {
    storages
        .iter()
        .filter(|storage| {
            if value_bool(storage.get("disable")).unwrap_or(false) {
                return false;
            }
            storage
                .get("content")
                .and_then(Value::as_str)
                .is_some_and(|content| content.split(',').any(|entry| entry.trim() == "backup"))
        })
        .filter_map(|storage| storage.get("storage").and_then(Value::as_str))
        .filter(|id| match only {
            Some(only) => only == *id,
            None => true,
        })
        .map(str::to_owned)
        .collect()
}

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            node: { schema: NODE_SCHEMA, optional: true },
            vmid: { schema: VMID_SCHEMA, optional: true },
            storage: { schema: PVE_STORAGE_ID_SCHEMA, optional: true },
        },
    },
    returns: {
        description: "Backup archives reachable from the remote.",
        type: Array,
        items: { type: PveBackupContent },
    },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}"], PRIV_RESOURCE_AUDIT, false),
    },
)]
/// List the backup archives a PVE remote can restore from.
pub async fn list_backup_content(
    remote: String,
    node: Option<String>,
    vmid: Option<u32>,
    storage: Option<String>,
) -> Result<Vec<PveBackupContent>, Error> {
    let storages = list_pve_storages(&remote).await?;
    let ids = backup_storage_ids(&storages, storage.as_deref());
    if ids.is_empty() {
        return Ok(Vec::new());
    }

    let node = pick_node(&remote, node, vmid).await?;
    let client = raw_client_to_remote_by_id(&remote)?;

    let mut result = Vec::new();
    for id in ids {
        let mut path = format!("/api2/extjs/nodes/{node}/storage/{id}/content?content=backup");
        if let Some(vmid) = vmid {
            path.push_str(&format!("&vmid={vmid}"));
        }

        let data: Value = match client.get(&path).await {
            // a storage can be restricted to other nodes or be temporarily offline
            Err(err) => {
                log::debug!("could not list backups of '{id}' on '{remote}': {err}");
                continue;
            }
            Ok(response) => match response.expect_json() {
                Ok(response) => response.data,
                Err(err) => {
                    log::debug!("unexpected content listing of '{id}' on '{remote}': {err}");
                    continue;
                }
            },
        };

        let Value::Array(entries) = data else {
            continue;
        };

        for entry in entries {
            let Some(volid) = entry.get("volid").and_then(Value::as_str) else {
                continue;
            };

            result.push(PveBackupContent {
                guest_type: guest_type_of(&entry, volid),
                volid: volid.to_string(),
                storage: id.clone(),
                vmid: value_u64(entry.get("vmid")).map(|vmid| vmid as u32),
                ctime: value_u64(entry.get("ctime")).map(|ctime| ctime as i64),
                size: value_u64(entry.get("size")),
                format: value_string(entry.get("format")),
                notes: value_string(entry.get("notes")),
                protected: value_bool(entry.get("protected")),
                verification: value_string(entry.get("verification")),
                encrypted: value_string(entry.get("encrypted")),
            });
        }
    }

    result.sort_by(|a, b| b.ctime.cmp(&a.ctime));

    Ok(result)
}

/// Volume ID of a PBS snapshot on the target remote.
///
/// The namespace is part of the storage configuration, so a storage only gives
/// access to the snapshots of its own namespace.
async fn pbs_snapshot_volid(remote: &str, request: &PveRestoreRequest) -> Result<String, Error> {
    let pbs_remote = request
        .pbs_remote
        .as_deref()
        .ok_or_else(|| format_err!("neither a volume ID nor a backup server was given"))?;
    let datastore = request
        .datastore
        .as_deref()
        .ok_or_else(|| format_err!("no datastore given for backup server '{pbs_remote}'"))?;
    let backup_id = request
        .backup_id
        .as_deref()
        .ok_or_else(|| format_err!("no backup ID given"))?;
    let backup_time = request
        .backup_time
        .ok_or_else(|| format_err!("no backup time given"))?;

    let wanted_namespace = request
        .namespace
        .as_deref()
        .map(str::trim)
        .filter(|namespace| !namespace.is_empty());

    let info = pbs_connection_info(pbs_remote)?;
    let storages = list_pve_storages(remote).await?;

    let storage = storages
        .iter()
        .filter(|storage| is_matching_storage(storage, &info.server, Some(datastore)))
        .find(|storage| {
            let namespace = storage
                .get("namespace")
                .and_then(Value::as_str)
                .filter(|namespace| !namespace.is_empty());
            namespace == wanted_namespace
        })
        .and_then(|storage| storage.get("storage"))
        .and_then(Value::as_str)
        .ok_or_else(|| {
            format_err!(
                "remote '{remote}' has no storage for datastore '{datastore}' of backup server \
                 '{pbs_remote}'{}, add it first",
                match wanted_namespace {
                    Some(namespace) => format!(" and namespace '{namespace}'"),
                    None => String::new(),
                }
            )
        })?;

    let time = proxmox_time::epoch_to_rfc3339_utc(backup_time)?;

    Ok(format!(
        "{storage}:backup/{}/{backup_id}/{time}",
        request.guest_type.as_str()
    ))
}

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            request: { type: PveRestoreRequest, flatten: true },
        },
    },
    returns: { type: RemoteUpid },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}"], PRIV_RESOURCE_CREATE, false),
    },
)]
/// Restore a backup archive into a guest of a PVE remote.
pub async fn restore_backup(
    remote: String,
    request: PveRestoreRequest,
) -> Result<RemoteUpid, Error> {
    let volid = match &request.volid {
        Some(volid) => volid.clone(),
        None => pbs_snapshot_volid(&remote, &request).await?,
    };

    let node = pick_node(&remote, request.node.clone(), Some(request.vmid)).await?;

    let force = u8::from(request.force.unwrap_or(false));
    let mut payload = json!({
        "vmid": request.vmid,
        "force": force,
    });
    if let Some(storage) = &request.storage {
        payload["storage"] = storage.clone().into();
    }
    if let Some(bwlimit) = request.bwlimit {
        payload["bwlimit"] = bwlimit.into();
    }

    let live_restore = request.live_restore.unwrap_or(false);
    let path = match request.guest_type {
        BackupGuestType::Vm => {
            payload["archive"] = volid.into();
            if request.unique.unwrap_or(false) {
                payload["unique"] = 1.into();
            }
            if live_restore {
                // PVE starts the guest itself while the restore is still running
                payload["live-restore"] = 1.into();
            }
            format!("/api2/extjs/nodes/{node}/qemu")
        }
        BackupGuestType::Ct => {
            payload["ostemplate"] = volid.into();
            payload["restore"] = 1.into();
            format!("/api2/extjs/nodes/{node}/lxc")
        }
    };
    if !live_restore {
        payload["start"] = u8::from(request.start.unwrap_or(false)).into();
    }

    let client = raw_client_to_remote_by_id(&remote)?;
    let upid = client
        .post(&path, &payload)
        .await?
        .expect_json::<pve_api_types::PveUpid>()?
        .data;

    new_remote_upid(remote, upid).await
}