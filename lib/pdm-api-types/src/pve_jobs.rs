use serde::{Deserialize, Serialize};

use proxmox_schema::api;

/// A scheduled PVE vzdump job as returned by `cluster/backup`.
#[api]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct PveBackupJob {
    /// Backup job identifier.
    pub id: String,

    /// Whether the job is enabled.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    /// Calendar event schedule.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schedule: Option<String>,
    /// Target backup storage.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storage: Option<String>,
    /// Backup mode.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    /// PVE node selection.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node: Option<String>,
    /// Guest pool selection.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pool: Option<String>,
    /// Comma-separated guest identifiers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vmid: Option<String>,
    /// Comma-separated guest identifiers to exclude.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exclude: Option<String>,
    /// Back up all guests when set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub all: Option<bool>,
    /// Compression mode.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compress: Option<String>,
    /// Optional bandwidth limit in KiB per second.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bwlimit: Option<u64>,
    /// Notification email recipients.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mailto: Option<String>,
    /// Notification delivery policy.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mailnotification: Option<String>,
    /// Backup retention policy.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prune_backups: Option<String>,
    /// Backup notes template.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes_template: Option<String>,
}

/// Editable parameters for a scheduled PVE backup job.
#[api]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct PveBackupJobConfig {
    /// Backup job identifier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Whether the job is enabled.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    /// Calendar event schedule.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schedule: Option<String>,
    /// Target backup storage.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storage: Option<String>,
    /// Backup mode.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    /// PVE node selection.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node: Option<String>,
    /// Guest pool selection.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pool: Option<String>,
    /// Comma-separated guest identifiers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vmid: Option<String>,
    /// Comma-separated guest identifiers to exclude.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exclude: Option<String>,
    /// Back up all guests when set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub all: Option<bool>,
    /// Compression mode.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compress: Option<String>,
    /// Optional bandwidth limit in KiB per second.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bwlimit: Option<u64>,
    /// Notification email recipients.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mailto: Option<String>,
    /// Notification delivery policy.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mailnotification: Option<String>,
    /// Backup retention policy.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prune_backups: Option<String>,
    /// Backup notes template.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes_template: Option<String>,
}

/// Parameters for an immediate PVE vzdump run.
#[api]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct PveVzdumpRequest {
    /// PVE node that executes the backup.
    pub node: String,
    /// Comma-separated guest identifiers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vmid: Option<String>,
    /// Guest pool selection.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pool: Option<String>,
    /// Back up all guests when set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub all: Option<bool>,
    /// Target backup storage.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storage: Option<String>,
    /// Backup mode.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    /// Compression mode.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compress: Option<String>,
    /// Optional bandwidth limit in KiB per second.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bwlimit: Option<u64>,
    /// Backup retention policy.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prune_backups: Option<String>,
    /// Backup notes template.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes_template: Option<String>,
}

/// A native intra-cluster PVE guest replication job.
#[api]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct PveReplicationJob {
    /// Replication job identifier.
    pub id: String,
    /// Target PVE node.
    pub target: String,
    /// Source PVE node.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// Calendar event schedule.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schedule: Option<String>,
    /// Replication bandwidth limit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate: Option<f64>,
    /// Disable this replication job.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disable: Option<bool>,
    /// Optional job comment.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    /// Replication job type.
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub job_type: Option<String>,
}

/// Editable parameters for a native PVE replication job.
#[api]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct PveReplicationJobConfig {
    /// Replication job identifier.
    pub id: String,
    /// Target PVE node.
    pub target: String,
    /// Calendar event schedule.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schedule: Option<String>,
    /// Replication bandwidth limit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate: Option<f64>,
    /// Disable this replication job.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disable: Option<bool>,
    /// Optional job comment.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    /// Replication job type.
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub job_type: Option<String>,
}

/// Runtime state for a native PVE replication job.
#[api]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct PveReplicationStatus {
    /// Replication job identifier.
    pub id: String,
    /// Target PVE node.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    /// Time of the previous replication run.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_sync: Option<i64>,
    /// Scheduled time of the next replication run.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_sync: Option<i64>,
    /// Duration of the previous replication run in seconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<f64>,
    /// Number of consecutive failures.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fail_count: Option<u64>,
    /// Error message from the previous replication run.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}
