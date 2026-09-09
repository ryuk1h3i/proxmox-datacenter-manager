use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use proxmox_schema::{ApiType, Updater, api};
use proxmox_section_config::{SectionConfig, SectionConfigPlugin, typed::ApiSectionDataEntry};

use crate::PROXMOX_SAFE_ID_SCHEMA;

/// Media content supported by the centralized catalog.
#[api]
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MediaContentType {
    /// QEMU installation media.
    Iso,
    /// LXC container template.
    Vztmpl,
}

impl std::fmt::Display for MediaContentType {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Iso => "iso",
            Self::Vztmpl => "vztmpl",
        })
    }
}

/// An ISO image or container template available from a PVE storage.
#[api]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct PveStorageContent {
    /// PVE volume identifier.
    pub volid: String,

    /// Content type reported by PVE.
    pub content: MediaContentType,

    /// Storage format, when reported by the storage plugin.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,

    /// File size in bytes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,

    /// Creation time as Unix epoch.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ctime: Option<i64>,
}

/// Parameters for a native PVE download into storage.
#[api]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct PveDownloadUrl {
    /// External HTTP or HTTPS URL fetched directly by PVE.
    pub url: String,

    /// Destination filename on the PVE storage.
    pub filename: String,

    /// Destination content type.
    pub content: MediaContentType,

    /// Expected checksum.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checksum: Option<String>,

    /// Checksum algorithm used by PVE.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checksum_algorithm: Option<String>,

    /// Verify the TLS certificate of the source URL.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verify_certificates: Option<bool>,
}

/// URL-only catalog entry managed by PDM.
#[api(
    properties: {
        id: { schema: PROXMOX_SAFE_ID_SCHEMA },
    },
)]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Updater)]
#[serde(rename_all = "kebab-case")]
pub struct MediaCatalogEntry {
    /// Stable catalog identifier.
    #[updater(skip)]
    pub id: String,

    /// Human-readable name.
    pub name: String,

    /// External source URL. PDM never stores the referenced blob.
    pub url: String,

    /// Media content type.
    pub content: MediaContentType,

    /// Suggested destination filename.
    pub filename: String,

    /// Optional release or image version.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,

    /// Optional target architecture.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub architecture: Option<String>,

    /// Expected checksum.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checksum: Option<String>,

    /// Checksum algorithm.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checksum_algorithm: Option<String>,

    /// Expected size in bytes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
/// Section entry stored in `media.cfg`.
pub enum MediaCatalogConfigEntry {
    /// URL-only media catalog entry.
    Media(MediaCatalogEntry),
}

impl ApiSectionDataEntry for MediaCatalogConfigEntry {
    fn section_config() -> &'static SectionConfig {
        static CONFIG: OnceLock<SectionConfig> = OnceLock::new();

        CONFIG.get_or_init(|| {
            let mut config = SectionConfig::new(&PROXMOX_SAFE_ID_SCHEMA);
            config.register_plugin(SectionConfigPlugin::new(
                "media".into(),
                Some("id".to_string()),
                MediaCatalogEntry::API_SCHEMA.unwrap_object_schema(),
            ));
            config
        })
    }

    fn section_type(&self) -> &'static str {
        "media"
    }
}