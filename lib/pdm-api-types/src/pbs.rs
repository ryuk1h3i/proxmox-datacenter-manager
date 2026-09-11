use serde::{Deserialize, Serialize};

use proxmox_schema::api;

#[api]
#[derive(Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
/// Whether a task is still running.
pub enum IsRunning {
    /// Task is running.
    Running,
    /// Task is not running.
    Stopped,
}

// TODO: The pbs code should expose this via pbs-api-types!
#[api]
/// Status if a task.
#[derive(Debug, Deserialize, Serialize)]
pub struct TaskStatus {
    /// Exit status, if available.
    pub exitstatus: Option<String>,

    /// Task id.
    pub id: Option<String>,

    /// Node the task is running on.
    pub node: String,

    /// The Unix PID
    pub pid: i64,

    /// The task start time (Epoch)
    pub pstart: i64,

    /// The task's start time.
    pub starttime: i64,

    pub status: IsRunning,

    /// The task type.
    #[serde(rename = "type")]
    pub ty: String,

    /// The task's UPID.
    pub upid: String,

    /// The authenticated entity who started the task.
    pub user: String,
}

impl TaskStatus {
    /// Checks if the task is currently running.
    pub fn is_running(&self) -> bool {
        self.status == IsRunning::Running
    }
}

#[api(
    properties: {
        remote: { schema: crate::remotes::REMOTE_ID_SCHEMA },
        storage: { schema: crate::PVE_STORAGE_ID_SCHEMA, optional: true },
    },
)]
/// Whether a PVE remote already has a storage pointing at a PBS datastore.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct PbsPveStorageState {
    /// The PVE remote.
    pub remote: String,
    /// ID of the storage pointing at the datastore, if there is one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storage: Option<String>,
    /// Error encountered while querying the remote.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[api(
    properties: {
        remote: { schema: crate::remotes::REMOTE_ID_SCHEMA },
    },
)]
/// Outcome of attaching a PBS datastore to a single PVE remote.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct PbsAttachResult {
    /// The PVE remote.
    pub remote: String,
    /// Whether the storage was created or updated.
    pub changed: bool,
    /// Human readable outcome.
    pub message: String,
    /// Error encountered while configuring the remote.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}
