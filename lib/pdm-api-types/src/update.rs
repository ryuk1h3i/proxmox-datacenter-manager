//! Types for the container image update check.

use serde::{Deserialize, Serialize};

use proxmox_schema::api;

#[api]
#[derive(Default, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
/// State of the container image the manager runs from.
pub struct ImageUpdateStatus {
    /// Whether the watched tag carries a newer image than the running one.
    pub update_available: bool,

    /// The image reference that is watched for updates.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,

    /// Source revision the running image was built from.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub running_revision: Option<String>,

    /// Build time of the running image.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub running_created: Option<String>,

    /// Source revision of the image published on the watched tag.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub available_revision: Option<String>,

    /// Build time of the image published on the watched tag.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub available_created: Option<String>,

    /// When the last check ran, in seconds since the UNIX epoch.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_checked: Option<i64>,

    /// Why the last check did not produce a result.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}
