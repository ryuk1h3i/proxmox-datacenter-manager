use serde::{Deserialize, Serialize};

use proxmox_schema::api;

/// Parameters used to create a QEMU virtual machine on PVE.
#[api]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct CreateQemu {
    pub vmid: u32,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub cores: Option<u64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub sockets: Option<u64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory: Option<u64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub ostype: Option<String>,

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
    pub vmid: u32,

    pub ostemplate: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub hostname: Option<String>,

    /// Optional container description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssh_public_keys: Option<String>,

    /// Number of virtual CPU cores.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cores: Option<u64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory: Option<u64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub swap: Option<u64>,

    /// Native PVE root filesystem property string.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rootfs: Option<String>,

    /// Native PVE network property string.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub net0: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub unprivileged: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub features: Option<String>,
}

/// Commonly edited QEMU configuration properties.
#[api]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct UpdateQemu {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cores: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sockets: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scsi0: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ide2: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub net0: Option<String>,
    /// Native PVE boot-order property string.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub boot: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub onboot: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub startup: Option<String>,
    /// Native PVE guest-agent property string.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delete: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,
}

/// Commonly edited LXC configuration properties.
#[api]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct UpdateLxc {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hostname: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Number of virtual CPU cores.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cores: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub swap: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rootfs: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub net0: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub features: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub onboot: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub startup: Option<String>,
    /// Comma-separated configuration properties to delete.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delete: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,
}

/// Parameters for cloning a QEMU VM or template.
#[api]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct CloneQemu {
    pub newid: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub full: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storage: Option<String>,
    /// Target disk format.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    /// Optional description for the cloned VM.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pool: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snapname: Option<String>,
}

/// Parameters for cloning an LXC container or template.
#[api]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct CloneLxc {
    pub newid: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hostname: Option<String>,
    /// Create a full clone instead of a linked clone.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub full: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storage: Option<String>,
    /// Optional description for the cloned container.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pool: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snapname: Option<String>,
}