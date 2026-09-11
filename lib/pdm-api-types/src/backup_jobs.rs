//! Unified, cross-remote backup jobs.
//!
//! PDM cannot run backups itself, so a job defined here is *materialized* into a
//! native `cluster/backup` job on every PVE remote that owns at least one of the
//! selected guests. The derived job keeps the id [`derived_job_id`] so PDM can
//! recognize and reconcile the jobs it owns.

use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use proxmox_schema::property_string::PropertyString;
use proxmox_schema::{ApiType, Schema, StringSchema, Updater, api};
use proxmox_section_config::typed::ApiSectionDataEntry;
use proxmox_section_config::{SectionConfig, SectionConfigPlugin};

use crate::remotes::REMOTE_ID_SCHEMA;
use crate::{PVE_STORAGE_ID_SCHEMA, SINGLE_LINE_COMMENT_SCHEMA, VMID_SCHEMA};

pub const BACKUP_JOB_ID_SCHEMA: Schema = StringSchema::new("Unified backup job ID.")
    .format(&crate::PROXMOX_SAFE_ID_FORMAT)
    .min_length(2)
    .max_length(32)
    .schema();

/// Prefix of the PVE-side jobs owned by PDM.
pub const DERIVED_JOB_PREFIX: &str = "pdm-";

/// ID of the `cluster/backup` job that a unified job materializes into.
pub fn derived_job_id(job_id: &str) -> String {
    format!("{DERIVED_JOB_PREFIX}{job_id}")
}

/// Reverse of [`derived_job_id`].
pub fn job_id_from_derived(derived: &str) -> Option<&str> {
    derived.strip_prefix(DERIVED_JOB_PREFIX)
}

#[api(
    properties: {
        remote: { schema: REMOTE_ID_SCHEMA },
        vmid: { schema: VMID_SCHEMA },
    },
    default_key: "remote",
)]
/// One guest selected by a unified backup job.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct BackupJobGuest {
    /// Remote the guest was last seen on.
    pub remote: String,
    /// Guest ID.
    pub vmid: u32,
}

#[api(
    properties: {
        remote: { schema: REMOTE_ID_SCHEMA },
        storage: { schema: PVE_STORAGE_ID_SCHEMA },
    },
    default_key: "remote",
)]
/// Overrides the target storage for a single remote.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct BackupJobTarget {
    /// PVE remote this override applies to.
    pub remote: String,
    /// Storage ID on that remote.
    pub storage: String,
}

#[api(
    properties: {
        id: { schema: BACKUP_JOB_ID_SCHEMA },
        comment: { schema: SINGLE_LINE_COMMENT_SCHEMA, optional: true },
        "default-storage": { schema: PVE_STORAGE_ID_SCHEMA, optional: true },
        "pbs-remote": { schema: REMOTE_ID_SCHEMA, optional: true },
        guests: {
            type: Array,
            optional: true,
            items: {
                type: String,
                description: "A guest selection entry, e.g. 'pve1,vmid=100'.",
            },
        },
        targets: {
            type: Array,
            optional: true,
            items: {
                type: String,
                description: "A per-remote storage override, e.g. 'pve1,storage=pbs01'.",
            },
        },
        "tag-filters": {
            type: Array,
            optional: true,
            items: {
                type: String,
                description: "A guest tag that automatically includes matching guests.",
            },
        },
        "tag-filter-remotes": {
            type: Array,
            optional: true,
            items: { schema: REMOTE_ID_SCHEMA },
        },
    },
)]
/// A unified backup job spanning guests of multiple remotes.
#[derive(Clone, Debug, Default, Deserialize, Serialize, Updater, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct BackupJobConfig {
    /// Job name.
    #[updater(skip)]
    pub id: String,

    /// Optional comment.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[updater(serde(skip_serializing_if = "Option::is_none"))]
    pub comment: Option<String>,

    /// Disable the job without deleting it.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[updater(serde(skip_serializing_if = "Option::is_none"))]
    pub disable: Option<bool>,

    /// Calendar event, evaluated by the PVE remotes themselves.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    #[updater(serde(skip_serializing_if = "Option::is_none"))]
    pub schedule: String,

    /// PBS remote whose datastore is the backup target.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[updater(serde(skip_serializing_if = "Option::is_none"))]
    pub pbs_remote: Option<String>,

    /// Storage used on every remote without an explicit target override.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[updater(serde(skip_serializing_if = "Option::is_none"))]
    pub default_storage: Option<String>,

    /// Explicitly selected guests.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[updater(serde(skip_serializing_if = "Option::is_none"))]
    pub guests: Vec<PropertyString<BackupJobGuest>>,

    /// Per-remote storage overrides.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[updater(serde(skip_serializing_if = "Option::is_none"))]
    pub targets: Vec<PropertyString<BackupJobTarget>>,

    /// Guests carrying one of these tags are included automatically.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[updater(serde(skip_serializing_if = "Option::is_none"))]
    pub tag_filters: Vec<String>,

    /// Restricts tag matching to these remotes (empty means every PVE remote).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[updater(serde(skip_serializing_if = "Option::is_none"))]
    pub tag_filter_remotes: Vec<String>,

    /// Backup mode (snapshot, suspend or stop).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[updater(serde(skip_serializing_if = "Option::is_none"))]
    pub mode: Option<String>,

    /// Compression mode.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[updater(serde(skip_serializing_if = "Option::is_none"))]
    pub compress: Option<String>,

    /// Bandwidth limit in KiB per second.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[updater(serde(skip_serializing_if = "Option::is_none"))]
    pub bwlimit: Option<u64>,

    /// Backup retention policy.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[updater(serde(skip_serializing_if = "Option::is_none"))]
    pub prune_backups: Option<String>,

    /// Backup notes template.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[updater(serde(skip_serializing_if = "Option::is_none"))]
    pub notes_template: Option<String>,

    /// Notification email recipients.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[updater(serde(skip_serializing_if = "Option::is_none"))]
    pub mailto: Option<String>,

    /// Notification delivery policy.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[updater(serde(skip_serializing_if = "Option::is_none"))]
    pub mailnotification: Option<String>,

    /// Follow guests that were migrated to another remote.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[updater(serde(skip_serializing_if = "Option::is_none"))]
    pub follow_migrations: Option<bool>,
}

impl BackupJobConfig {
    /// Storage to use on the given remote.
    pub fn storage_for(&self, remote: &str) -> Option<&str> {
        self.targets
            .iter()
            .find(|target| target.remote == remote)
            .map(|target| target.storage.as_str())
            .or(self.default_storage.as_deref())
    }

    /// Whether guests migrated to another remote should be tracked by vmid.
    pub fn follows_migrations(&self) -> bool {
        self.follow_migrations.unwrap_or(true)
    }
}

const BACKUP_JOB_SECTION_NAME: &str = "backup-job";

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
/// Enum for the different sections in the 'backup-jobs.cfg' file.
pub enum BackupJobConfigEntry {
    /// 'backup-job' section
    BackupJob(BackupJobConfig),
}

impl ApiSectionDataEntry for BackupJobConfigEntry {
    fn section_config() -> &'static SectionConfig {
        static CONFIG: OnceLock<SectionConfig> = OnceLock::new();

        CONFIG.get_or_init(|| {
            let mut this = SectionConfig::new(&BACKUP_JOB_ID_SCHEMA);

            this.register_plugin(SectionConfigPlugin::new(
                BACKUP_JOB_SECTION_NAME.into(),
                Some("id".to_string()),
                BackupJobConfig::API_SCHEMA.unwrap_object_schema(),
            ));
            this
        })
    }

    fn section_type(&self) -> &'static str {
        match self {
            BackupJobConfigEntry::BackupJob(_) => BACKUP_JOB_SECTION_NAME,
        }
    }
}

#[api]
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
/// Materialization state of a unified backup job on a single PVE remote.
pub enum BackupJobSyncState {
    /// The derived PVE job matches the PDM definition.
    Synced,
    /// The derived PVE job exists but differs from the PDM definition.
    OutOfSync,
    /// The derived PVE job is missing on the remote.
    Missing,
    /// The remote could not be queried.
    Error,
}

#[api(
    properties: {
        remote: { schema: REMOTE_ID_SCHEMA },
        "job-id": { type: String, description: "ID of the derived job on the PVE remote." },
        storage: { schema: PVE_STORAGE_ID_SCHEMA, optional: true },
    },
)]
/// Per-remote materialization status of a unified backup job.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct BackupJobRemoteStatus {
    /// PVE remote.
    pub remote: String,
    /// ID of the derived job on that remote.
    pub job_id: String,
    /// Materialization state.
    pub state: BackupJobSyncState,
    /// Number of guests of this job living on the remote.
    pub guest_count: u32,
    /// Target storage on that remote.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storage: Option<String>,
    /// Error encountered while querying or updating the remote.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}
