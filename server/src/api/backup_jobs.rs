//! Unified, cross-remote backup jobs.

use anyhow::Error;
use serde::{Deserialize, Serialize};

use proxmox_config_digest::ConfigDigest;
use proxmox_router::{
    Permission, Router, SubdirMap, http_bail, http_err, list_subdirs_api_method,
};
use proxmox_schema::{api, param_bail};
use proxmox_sortable_macro::sortable;

use pdm_api_types::backup_jobs::{
    BACKUP_JOB_ID_SCHEMA, BackupJobConfig, BackupJobConfigEntry, BackupJobConfigUpdater,
    BackupJobRemoteStatus,
};
use pdm_api_types::{PRIV_RESOURCE_AUDIT, PRIV_RESOURCE_MANAGE, RemoteUpid};

use crate::backup_jobs;

pub const ROUTER: Router = Router::new()
    .get(&API_METHOD_LIST_BACKUP_JOBS)
    .post(&API_METHOD_CREATE_BACKUP_JOB)
    .match_all("id", &ITEM_ROUTER);

const ITEM_ROUTER: Router = Router::new()
    .get(&list_subdirs_api_method!(ITEM_SUBDIRS))
    .delete(&API_METHOD_DELETE_BACKUP_JOB)
    .put(&API_METHOD_UPDATE_BACKUP_JOB)
    .subdirs(ITEM_SUBDIRS);

#[sortable]
const ITEM_SUBDIRS: SubdirMap = &sorted!([
    ("config", &Router::new().get(&API_METHOD_READ_BACKUP_JOB)),
    ("run", &Router::new().post(&API_METHOD_RUN_BACKUP_JOB)),
    ("status", &Router::new().get(&API_METHOD_BACKUP_JOB_STATUS)),
    ("sync", &Router::new().post(&API_METHOD_SYNC_BACKUP_JOB)),
]);

fn get_job(id: &str) -> Result<BackupJobConfig, Error> {
    let (config, _) = pdm_config::backup_jobs::config()?;
    match config.get(id) {
        Some(BackupJobConfigEntry::BackupJob(job)) => Ok(job.clone()),
        None => Err(http_err!(NOT_FOUND, "no such backup job '{id}'")),
    }
}

#[api(
    returns: {
        description: "List of unified backup jobs.",
        type: Array,
        items: { type: BackupJobConfig },
    },
    access: {
        permission: &Permission::Privilege(&["resource"], PRIV_RESOURCE_AUDIT, false),
    },
)]
/// List the configured unified backup jobs.
pub fn list_backup_jobs() -> Result<Vec<BackupJobConfig>, Error> {
    let (config, _) = pdm_config::backup_jobs::config()?;
    Ok(config
        .into_iter()
        .map(|(_, entry)| match entry {
            BackupJobConfigEntry::BackupJob(job) => job,
        })
        .collect())
}

#[api(
    input: {
        properties: {
            id: { schema: BACKUP_JOB_ID_SCHEMA },
        },
    },
    returns: { type: BackupJobConfig },
    access: {
        permission: &Permission::Privilege(&["resource"], PRIV_RESOURCE_AUDIT, false),
    },
)]
/// Read a single unified backup job.
pub fn read_backup_job(id: String) -> Result<BackupJobConfig, Error> {
    get_job(&id)
}

#[api(
    input: {
        properties: {
            job: { type: BackupJobConfig, flatten: true },
        },
    },
    access: {
        permission: &Permission::Privilege(&["resource"], PRIV_RESOURCE_MANAGE, false),
    },
)]
/// Create a unified backup job and materialize it on the involved remotes.
pub async fn create_backup_job(job: BackupJobConfig) -> Result<(), Error> {
    if job.schedule.is_empty() {
        param_bail!("schedule", "a schedule is required for job '{}'", job.id);
    }

    {
        let _lock = pdm_config::backup_jobs::lock_config()?;
        let (mut config, _) = pdm_config::backup_jobs::config()?;

        if config.contains_key(&job.id) {
            param_bail!("id", "backup job '{}' already exists.", job.id);
        }

        config.insert(
            job.id.clone(),
            BackupJobConfigEntry::BackupJob(job.clone()),
        );
        pdm_config::backup_jobs::save_config(&config)?;
    }

    if let Err(err) = backup_jobs::sync_job(&job).await {
        log::error!("could not materialize new backup job '{}': {err:#}", job.id);
    }

    Ok(())
}

#[api()]
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
/// Deletable property name
pub enum DeletableProperty {
    /// Delete the comment.
    Comment,
    /// Delete the disable flag.
    Disable,
    /// Delete the PBS remote reference.
    PbsRemote,
    /// Delete the default storage.
    DefaultStorage,
    /// Delete the guest selection.
    Guests,
    /// Delete the per-remote storage overrides.
    Targets,
    /// Delete the tag filters.
    TagFilters,
    /// Delete the tag filter remote restriction.
    TagFilterRemotes,
    /// Delete the backup mode.
    Mode,
    /// Delete the compression setting.
    Compress,
    /// Delete the bandwidth limit.
    Bwlimit,
    /// Delete the retention policy.
    PruneBackups,
    /// Delete the notes template.
    NotesTemplate,
    /// Delete the notification recipients.
    Mailto,
    /// Delete the notification policy.
    Mailnotification,
    /// Delete the migration following flag.
    FollowMigrations,
}

#[api(
    input: {
        properties: {
            id: { schema: BACKUP_JOB_ID_SCHEMA },
            update: { type: BackupJobConfigUpdater, flatten: true },
            delete: {
                description: "List of properties to delete.",
                type: Array,
                optional: true,
                items: { type: DeletableProperty },
            },
            digest: { type: ConfigDigest, optional: true },
        },
    },
    access: {
        permission: &Permission::Privilege(&["resource"], PRIV_RESOURCE_MANAGE, false),
    },
)]
/// Update a unified backup job and re-materialize it.
pub async fn update_backup_job(
    id: String,
    update: BackupJobConfigUpdater,
    delete: Option<Vec<DeletableProperty>>,
    digest: Option<ConfigDigest>,
) -> Result<(), Error> {
    let job = {
        let _lock = pdm_config::backup_jobs::lock_config()?;
        let (mut config, config_digest) = pdm_config::backup_jobs::config()?;

        config_digest.detect_modification(digest.as_ref())?;

        let Some(BackupJobConfigEntry::BackupJob(job)) = config.get_mut(&id) else {
            http_bail!(NOT_FOUND, "no such backup job '{id}'");
        };

        if let Some(delete) = delete {
            for delete_prop in delete {
                match delete_prop {
                    DeletableProperty::Comment => job.comment = None,
                    DeletableProperty::Disable => job.disable = None,
                    DeletableProperty::PbsRemote => job.pbs_remote = None,
                    DeletableProperty::DefaultStorage => job.default_storage = None,
                    DeletableProperty::Guests => job.guests = Vec::new(),
                    DeletableProperty::Targets => job.targets = Vec::new(),
                    DeletableProperty::TagFilters => job.tag_filters = Vec::new(),
                    DeletableProperty::TagFilterRemotes => job.tag_filter_remotes = Vec::new(),
                    DeletableProperty::Mode => job.mode = None,
                    DeletableProperty::Compress => job.compress = None,
                    DeletableProperty::Bwlimit => job.bwlimit = None,
                    DeletableProperty::PruneBackups => job.prune_backups = None,
                    DeletableProperty::NotesTemplate => job.notes_template = None,
                    DeletableProperty::Mailto => job.mailto = None,
                    DeletableProperty::Mailnotification => job.mailnotification = None,
                    DeletableProperty::FollowMigrations => job.follow_migrations = None,
                }
            }
        }

        if let Some(comment) = update.comment {
            job.comment = Some(comment);
        }
        if update.disable.is_some() {
            job.disable = update.disable;
        }
        if let Some(schedule) = update.schedule {
            job.schedule = schedule;
        }
        if let Some(pbs_remote) = update.pbs_remote {
            job.pbs_remote = Some(pbs_remote);
        }
        if let Some(default_storage) = update.default_storage {
            job.default_storage = Some(default_storage);
        }
        if let Some(guests) = update.guests {
            job.guests = guests;
        }
        if let Some(targets) = update.targets {
            job.targets = targets;
        }
        if let Some(tag_filters) = update.tag_filters {
            job.tag_filters = tag_filters;
        }
        if let Some(tag_filter_remotes) = update.tag_filter_remotes {
            job.tag_filter_remotes = tag_filter_remotes;
        }
        if let Some(mode) = update.mode {
            job.mode = Some(mode);
        }
        if let Some(compress) = update.compress {
            job.compress = Some(compress);
        }
        if update.bwlimit.is_some() {
            job.bwlimit = update.bwlimit;
        }
        if let Some(prune_backups) = update.prune_backups {
            job.prune_backups = Some(prune_backups);
        }
        if let Some(notes_template) = update.notes_template {
            job.notes_template = Some(notes_template);
        }
        if let Some(mailto) = update.mailto {
            job.mailto = Some(mailto);
        }
        if let Some(mailnotification) = update.mailnotification {
            job.mailnotification = Some(mailnotification);
        }
        if update.follow_migrations.is_some() {
            job.follow_migrations = update.follow_migrations;
        }

        let job = job.clone();
        pdm_config::backup_jobs::save_config(&config)?;
        job
    };

    if let Err(err) = backup_jobs::sync_job(&job).await {
        log::error!("could not materialize backup job '{id}': {err:#}");
    }

    Ok(())
}

#[api(
    input: {
        properties: {
            id: { schema: BACKUP_JOB_ID_SCHEMA },
            digest: { type: ConfigDigest, optional: true },
        },
    },
    access: {
        permission: &Permission::Privilege(&["resource"], PRIV_RESOURCE_MANAGE, false),
    },
)]
/// Delete a unified backup job and remove it from the involved remotes.
pub async fn delete_backup_job(id: String, digest: Option<ConfigDigest>) -> Result<(), Error> {
    // remove the derived jobs first so a failure here does not orphan them
    backup_jobs::remove_job(&id).await?;

    let _lock = pdm_config::backup_jobs::lock_config()?;
    let (mut config, config_digest) = pdm_config::backup_jobs::config()?;

    config_digest.detect_modification(digest.as_ref())?;

    if config.remove(&id).is_none() {
        http_bail!(NOT_FOUND, "backup job '{id}' does not exist.");
    }

    pdm_config::backup_jobs::save_config(&config)?;

    Ok(())
}

#[api(
    input: {
        properties: {
            id: { schema: BACKUP_JOB_ID_SCHEMA },
        },
    },
    returns: {
        description: "Per-remote materialization status.",
        type: Array,
        items: { type: BackupJobRemoteStatus },
    },
    access: {
        permission: &Permission::Privilege(&["resource"], PRIV_RESOURCE_AUDIT, false),
    },
)]
/// Report how a unified backup job is currently materialized.
pub async fn backup_job_status(id: String) -> Result<Vec<BackupJobRemoteStatus>, Error> {
    backup_jobs::job_status(&get_job(&id)?).await
}

#[api(
    input: {
        properties: {
            id: { schema: BACKUP_JOB_ID_SCHEMA },
        },
    },
    returns: {
        description: "Per-remote materialization status.",
        type: Array,
        items: { type: BackupJobRemoteStatus },
    },
    access: {
        permission: &Permission::Privilege(&["resource"], PRIV_RESOURCE_MANAGE, false),
    },
)]
/// Force a re-materialization of a unified backup job.
pub async fn sync_backup_job(id: String) -> Result<Vec<BackupJobRemoteStatus>, Error> {
    backup_jobs::sync_job(&get_job(&id)?).await
}

#[api(
    input: {
        properties: {
            id: { schema: BACKUP_JOB_ID_SCHEMA },
        },
    },
    returns: {
        description: "One task per involved remote and node.",
        type: Array,
        items: { type: RemoteUpid },
    },
    access: {
        permission: &Permission::Privilege(&["resource"], PRIV_RESOURCE_MANAGE, false),
    },
)]
/// Start an immediate run of a unified backup job.
pub async fn run_backup_job(id: String) -> Result<Vec<RemoteUpid>, Error> {
    backup_jobs::run_job(&get_job(&id)?).await
}
