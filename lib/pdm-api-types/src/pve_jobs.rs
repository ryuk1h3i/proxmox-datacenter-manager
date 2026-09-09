use serde::{Deserialize, Serialize};

use proxmox_schema::api;

/// A scheduled PVE vzdump job as returned by `cluster/backup`.
#[api]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct PveBackupJob {
    pub id: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schedule: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storage: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pool: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vmid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exclude: Option<String>,
    /// Back up all guests when set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub all: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compress: Option<String>,
    /// Optional bandwidth limit in KiB per second.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bwlimit: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mailto: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mailnotification: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prune_backups: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes_template: Option<String>,
}

/// Editable parameters for a scheduled PVE backup job.
#[api]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct PveBackupJobConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schedule: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storage: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pool: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vmid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exclude: Option<String>,
    /// Back up all guests when set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub all: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compress: Option<String>,
    /// Optional bandwidth limit in KiB per second.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bwlimit: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mailto: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mailnotification: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prune_backups: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes_template: Option<String>,
}

/// Parameters for an immediate PVE vzdump run.
#[api]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct PveVzdumpRequest {
    pub node: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vmid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pool: Option<String>,
    /// Back up all guests when set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub all: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storage: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compress: Option<String>,
    /// Optional bandwidth limit in KiB per second.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bwlimit: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prune_backups: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes_template: Option<String>,
}

/// A native intra-cluster PVE guest replication job.
#[api]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct PveReplicationJob {
    pub id: String,
    pub target: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schedule: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate: Option<f64>,
    /// Disable this replication job.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disable: Option<bool>,
    /// Optional job comment.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub job_type: Option<String>,
}

/// Editable parameters for a native PVE replication job.
#[api]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct PveReplicationJobConfig {
    pub id: String,
    pub target: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schedule: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate: Option<f64>,
    /// Disable this replication job.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disable: Option<bool>,
    /// Optional job comment.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub job_type: Option<String>,
}

/// Runtime state for a native PVE replication job.
#[api]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct PveReplicationStatus {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_sync: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_sync: Option<i64>,
    /// Duration of the previous replication run in seconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fail_count: Option<u64>,
    /// Error message from the previous replication run.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}
