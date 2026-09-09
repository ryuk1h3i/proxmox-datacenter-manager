use serde::{Deserialize, Serialize};

use proxmox_schema::api;

/// Scheduled PBS prune job.
#[api]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct PbsPruneJob {
    pub id: String,
    pub store: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ns: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schedule: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keep_last: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keep_hourly: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keep_daily: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keep_weekly: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keep_monthly: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keep_yearly: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_depth: Option<u64>,
}

/// Scheduled PBS verification job.
#[api]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct PbsVerifyJob {
    pub id: String,
    pub store: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ns: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schedule: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ignore_verified: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outdated_after: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_depth: Option<u64>,
}

/// Scheduled pull-style PBS synchronization job.
#[api]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct PbsSyncJob {
    pub id: String,
    pub store: String,
    pub remote: String,
    pub remote_store: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ns: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_ns: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schedule: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remove_vanished: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate_in: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_depth: Option<u64>,
}

/// Current garbage-collection state for a PBS datastore.
#[api]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct PbsGcStatus {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_run_endtime: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_run_upid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_run: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schedule: Option<String>,
}

/// Parameters for an on-demand datastore prune operation.
#[api]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct PbsPruneRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ns: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dry_run: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keep_last: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keep_hourly: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keep_daily: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keep_weekly: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keep_monthly: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keep_yearly: Option<u64>,
}

/// One snapshot decision returned by a PBS prune operation or dry-run.
#[api]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct PbsPruneResult {
    pub backup_type: String,
    pub backup_id: String,
    pub backup_time: i64,
    pub keep: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protected: Option<bool>,
}

/// Identifies one PBS backup snapshot.
#[api]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct PbsSnapshotRef {
    pub backup_type: String,
    pub backup_id: String,
    pub backup_time: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ns: Option<String>,
}

/// Update the protection flag of a PBS backup snapshot.
#[api]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct PbsSnapshotProtection {
    #[serde(flatten)]
    pub snapshot: PbsSnapshotRef,
    pub protected: bool,
}

/// Update notes attached to a PBS backup snapshot.
#[api]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct PbsSnapshotNotes {
    #[serde(flatten)]
    pub snapshot: PbsSnapshotRef,
    pub notes: String,
}
