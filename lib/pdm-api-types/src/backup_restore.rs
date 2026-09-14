//! Browsing and restoring the backups a PVE remote can reach.

use serde::{Deserialize, Serialize};

use proxmox_schema::api;

use crate::remotes::REMOTE_ID_SCHEMA;
use crate::{NODE_SCHEMA, PVE_STORAGE_ID_SCHEMA, VMID_SCHEMA};

#[api]
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
/// Guest type of a backup archive.
pub enum BackupGuestType {
    /// A virtual machine.
    Vm,
    /// A container.
    Ct,
}

impl BackupGuestType {
    /// The name PVE uses in volume IDs and backup group IDs.
    pub fn as_str(self) -> &'static str {
        match self {
            BackupGuestType::Vm => "vm",
            BackupGuestType::Ct => "ct",
        }
    }
}

impl Default for BackupGuestType {
    fn default() -> Self {
        Self::Vm
    }
}

#[api(
    properties: {
        storage: { schema: PVE_STORAGE_ID_SCHEMA },
        vmid: { schema: VMID_SCHEMA, optional: true },
    },
)]
/// One backup archive as reported by the content listing of a PVE storage.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct PveBackupContent {
    /// Volume ID, used to restore the archive.
    pub volid: String,
    /// Storage holding the archive.
    pub storage: String,
    /// Guest the archive belongs to.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vmid: Option<u32>,
    /// Guest type of the archive.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guest_type: Option<BackupGuestType>,
    /// Creation time of the backup.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ctime: Option<i64>,
    /// Size in bytes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    /// Archive format.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    /// First line of the backup notes, usually the guest name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    /// Whether the archive is protected against pruning.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protected: Option<bool>,
    /// Verification state reported by the backup server.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification: Option<String>,
    /// Encryption state reported by the backup server.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encrypted: Option<String>,
}

#[api(
    properties: {
        node: { schema: NODE_SCHEMA, optional: true },
        vmid: { schema: VMID_SCHEMA },
        storage: { schema: PVE_STORAGE_ID_SCHEMA, optional: true },
        "pbs-remote": { schema: REMOTE_ID_SCHEMA, optional: true },
    },
)]
/// Restore a backup archive into a guest of a PVE remote.
///
/// The archive is either addressed directly by its volume ID or, when it is
/// browsed on a PBS remote, by datastore and snapshot. In the latter case PDM
/// resolves the storage that points at that datastore on the target remote.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct PveRestoreRequest {
    /// Node that runs the restore, defaults to the node of an existing guest.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node: Option<String>,
    /// ID of the restored guest.
    pub vmid: u32,
    /// Guest type of the archive.
    pub guest_type: BackupGuestType,

    /// Volume ID of the archive on the target remote.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volid: Option<String>,

    /// PBS remote the snapshot was browsed on.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pbs_remote: Option<String>,
    /// Datastore of that PBS remote.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub datastore: Option<String>,
    /// Namespace of the snapshot inside the datastore.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    /// Backup ID of the snapshot, usually the original guest ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backup_id: Option<String>,
    /// Backup time of the snapshot.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backup_time: Option<i64>,

    /// Storage the restored disks are written to.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storage: Option<String>,
    /// Overwrite an existing guest with the same ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub force: Option<bool>,
    /// Start the guest once the restore finished.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<bool>,
    /// Assign new MAC addresses to the restored VM.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unique: Option<bool>,
    /// Start the VM while it is still being restored.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub live_restore: Option<bool>,
    /// Restore bandwidth limit in KiB per second.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bwlimit: Option<u64>,
}
