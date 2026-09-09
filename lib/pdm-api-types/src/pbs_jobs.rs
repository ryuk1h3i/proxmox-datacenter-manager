use serde::{Deserialize, Serialize};

use proxmox_schema::api;

/// Scheduled PBS prune job.
#[api]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct PbsPruneJob {
    /// Job identifier.
    pub id: String,
    /// Target datastore.
    pub store: String,
    /// Optional namespace.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ns: Option<String>,
    /// Calendar event schedule.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schedule: Option<String>,
    /// Disable this scheduled job.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disable: Option<bool>,
    /// Optional job comment.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    /// Number of newest backups to retain.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keep_last: Option<u64>,
    /// Number of hourly backups to retain.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keep_hourly: Option<u64>,
    /// Number of daily backups to retain.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keep_daily: Option<u64>,
    /// Number of weekly backups to retain.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keep_weekly: Option<u64>,
    /// Number of monthly backups to retain.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keep_monthly: Option<u64>,
    /// Number of yearly backups to retain.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keep_yearly: Option<u64>,
    /// Maximum namespace recursion depth.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_depth: Option<u64>,
}

/// Scheduled PBS verification job.
#[api]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct PbsVerifyJob {
    /// Job identifier.
    pub id: String,
    /// Target datastore.
    pub store: String,
    /// Optional namespace.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ns: Option<String>,
    /// Calendar event schedule.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schedule: Option<String>,
    /// Disable this scheduled job.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disable: Option<bool>,
    /// Optional job comment.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    /// Skip snapshots that have already been verified.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ignore_verified: Option<bool>,
    /// Reverify snapshots older than this many days.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outdated_after: Option<u64>,
    /// Maximum namespace recursion depth.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_depth: Option<u64>,
}

/// Scheduled pull-style PBS synchronization job.
#[api]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct PbsSyncJob {
    /// Job identifier.
    pub id: String,
    /// Target datastore.
    pub store: String,
    /// Source remote identifier.
    pub remote: String,
    /// Source datastore.
    pub remote_store: String,
    /// Optional target namespace.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ns: Option<String>,
    /// Optional source namespace.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_ns: Option<String>,
    /// Owner assigned to synchronized backups.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    /// Calendar event schedule.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schedule: Option<String>,
    /// Disable this scheduled job.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disable: Option<bool>,
    /// Remove snapshots vanished from the source.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remove_vanished: Option<bool>,
    /// Optional job comment.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    /// Incoming transfer rate limit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate_in: Option<u64>,
    /// Maximum namespace recursion depth.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_depth: Option<u64>,
}

/// Current garbage-collection state for a PBS datastore.
#[api]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct PbsGcStatus {
    /// Running garbage-collection task identifier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upid: Option<String>,
    /// Current garbage-collection status.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// End time of the previous garbage-collection run.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_run_endtime: Option<i64>,
    /// Task identifier of the previous garbage-collection run.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_run_upid: Option<String>,
    /// Scheduled time of the next run.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_run: Option<i64>,
    /// Calendar event schedule.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schedule: Option<String>,
}

/// Parameters for an on-demand datastore prune operation.
#[api]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct PbsPruneRequest {
    /// Optional namespace.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ns: Option<String>,
    /// Calculate the prune result without deleting snapshots.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dry_run: Option<bool>,
    /// Number of newest backups to retain.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keep_last: Option<u64>,
    /// Number of hourly backups to retain.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keep_hourly: Option<u64>,
    /// Number of daily backups to retain.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keep_daily: Option<u64>,
    /// Number of weekly backups to retain.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keep_weekly: Option<u64>,
    /// Number of monthly backups to retain.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keep_monthly: Option<u64>,
    /// Number of yearly backups to retain.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keep_yearly: Option<u64>,
}

/// One snapshot decision returned by a PBS prune operation or dry-run.
#[api]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct PbsPruneResult {
    /// Backup group type.
    pub backup_type: String,
    /// Backup group identifier.
    pub backup_id: String,
    /// Snapshot creation time as Unix epoch.
    pub backup_time: i64,
    /// Whether the prune policy retains this snapshot.
    pub keep: bool,
    /// Whether the snapshot is protected from deletion.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protected: Option<bool>,
}

/// Identifies one PBS backup snapshot.
#[api]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct PbsSnapshotRef {
    /// Backup group type.
    pub backup_type: String,
    /// Backup group identifier.
    pub backup_id: String,
    /// Snapshot creation time as Unix epoch.
    pub backup_time: i64,
    /// Optional namespace.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ns: Option<String>,
}

/// Update the protection flag of a PBS backup snapshot.
#[api(
    properties: {
        snapshot: {
            type: PbsSnapshotRef,
            flatten: true,
        },
    },
)]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct PbsSnapshotProtection {
    /// Snapshot to update.
    #[serde(flatten)]
    pub snapshot: PbsSnapshotRef,
    /// Whether the snapshot is protected from deletion.
    pub protected: bool,
}

/// Update notes attached to a PBS backup snapshot.
#[api(
    properties: {
        snapshot: {
            type: PbsSnapshotRef,
            flatten: true,
        },
    },
)]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct PbsSnapshotNotes {
    /// Snapshot to update.
    #[serde(flatten)]
    pub snapshot: PbsSnapshotRef,
    /// Notes to attach to the snapshot.
    pub notes: String,
}
