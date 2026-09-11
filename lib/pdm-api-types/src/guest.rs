use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use proxmox_schema::api;

/// Parameters used to create a QEMU virtual machine on PVE.
#[api]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct CreateQemu {
    /// Guest identifier.
    pub vmid: u32,

    /// Guest name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    /// Optional guest description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Number of virtual CPU cores.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cores: Option<u64>,

    /// Number of virtual CPU sockets.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sockets: Option<u64>,

    /// Guest memory in MiB.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory: Option<u64>,

    /// Guest operating-system type.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ostype: Option<String>,

    /// SCSI controller model.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scsihw: Option<String>,

    /// Native PVE disk property string.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scsi0: Option<String>,

    /// Native PVE CD-ROM property string, such as `store:iso/file.iso,media=cdrom`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ide2: Option<String>,

    /// Native PVE network property string.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub net0: Option<String>,

    /// Native PVE boot-order property string.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub boot: Option<String>,

    /// Start the guest after creation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<bool>,

    /// Native PVE guest-agent property string.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
}

/// Parameters used to create an LXC container on PVE.
#[api]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct CreateLxc {
    /// Guest identifier.
    pub vmid: u32,

    /// PVE volume identifier of the container template.
    pub ostemplate: String,

    /// Container hostname.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hostname: Option<String>,

    /// Optional container description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Initial root password.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,

    /// Authorized SSH public keys.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssh_public_keys: Option<String>,

    /// Number of virtual CPU cores.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cores: Option<u64>,

    /// Container memory in MiB.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory: Option<u64>,

    /// Container swap in MiB.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub swap: Option<u64>,

    /// Native PVE root filesystem property string.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rootfs: Option<String>,

    /// Native PVE network property string.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub net0: Option<String>,

    /// Create an unprivileged container.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unprivileged: Option<bool>,

    /// Start the container after creation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<bool>,

    /// Native PVE container feature string.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub features: Option<String>,
}

/// Commonly edited QEMU configuration properties.
#[api(
    properties: {},
    additional_properties: "extra",
)]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct UpdateQemu {
    /// Guest name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Optional guest description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Number of virtual CPU cores.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cores: Option<u64>,
    /// Number of virtual CPU sockets.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sockets: Option<u64>,
    /// Guest memory in MiB.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory: Option<u64>,
    /// Native PVE disk property string.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scsi0: Option<String>,
    /// Native PVE CD-ROM property string.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ide2: Option<String>,
    /// Native PVE network property string.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub net0: Option<String>,
    /// Native PVE boot-order property string.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub boot: Option<String>,
    /// Start the guest during node boot.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub onboot: Option<bool>,
    /// Native PVE startup-order property string.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub startup: Option<String>,
    /// Native PVE guest-agent property string.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    /// Comma-separated configuration properties to delete.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delete: Option<String>,
    /// Configuration digest used for optimistic locking.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,

    /// Any other native PVE property, forwarded to the remote verbatim.
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

/// Commonly edited LXC configuration properties.
#[api(
    properties: {},
    additional_properties: "extra",
)]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct UpdateLxc {
    /// Container hostname.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hostname: Option<String>,
    /// Optional container description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Number of virtual CPU cores.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cores: Option<u64>,
    /// Container memory in MiB.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory: Option<u64>,
    /// Container swap in MiB.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub swap: Option<u64>,
    /// Native PVE root filesystem property string.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rootfs: Option<String>,
    /// Native PVE network property string.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub net0: Option<String>,
    /// Native PVE container feature string.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub features: Option<String>,
    /// Start the container during node boot.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub onboot: Option<bool>,
    /// Native PVE startup-order property string.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub startup: Option<String>,
    /// Comma-separated configuration properties to delete.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delete: Option<String>,
    /// Configuration digest used for optimistic locking.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,

    /// Any other native PVE property, forwarded to the remote verbatim.
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

/// Parameters for cloning a QEMU VM or template.
#[api]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct CloneQemu {
    /// Identifier assigned to the cloned guest.
    pub newid: u32,
    /// Name assigned to the cloned guest.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Create a full clone instead of a linked clone.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub full: Option<bool>,
    /// Target PVE node.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    /// Target storage.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storage: Option<String>,
    /// Target disk format.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    /// Optional description for the cloned VM.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Target resource pool.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pool: Option<String>,
    /// Source snapshot name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snapname: Option<String>,
}

/// Parameters for cloning an LXC container or template.
#[api]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct CloneLxc {
    /// Identifier assigned to the cloned container.
    pub newid: u32,
    /// Hostname assigned to the cloned container.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hostname: Option<String>,
    /// Create a full clone instead of a linked clone.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub full: Option<bool>,
    /// Target PVE node.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    /// Target storage.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storage: Option<String>,
    /// Optional description for the cloned container.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Target resource pool.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pool: Option<String>,
    /// Source snapshot name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snapname: Option<String>,
}