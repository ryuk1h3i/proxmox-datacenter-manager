use anyhow::Error;

use proxmox_product_config::{ApiLockGuard, open_api_lockfile, replace_config};
use proxmox_section_config::typed::{ApiSectionDataEntry, SectionConfigData};

use pdm_api_types::{ConfigDigest, media::MediaCatalogConfigEntry};

use pdm_buildcfg::configdir;

const MEDIA_CFG_FILENAME: &str = configdir!("/media.cfg");
const MEDIA_CFG_LOCKFILE: &str = configdir!("/.media.lock");

/// Load the URL-only media catalog.
pub fn config() -> Result<(SectionConfigData<MediaCatalogConfigEntry>, ConfigDigest), Error> {
    let content =
        proxmox_sys::fs::file_read_optional_string(MEDIA_CFG_FILENAME)?.unwrap_or_default();
    let digest = openssl::sha::sha256(content.as_bytes());
    let data = MediaCatalogConfigEntry::parse_section_config(MEDIA_CFG_FILENAME, &content)?;
    Ok((data, digest.into()))
}

/// Acquire the exclusive media catalog lock.
pub fn lock_config() -> Result<ApiLockGuard, Error> {
    open_api_lockfile(MEDIA_CFG_LOCKFILE, None, true)
}

/// Atomically save the URL-only media catalog.
pub fn save_config(config: &SectionConfigData<MediaCatalogConfigEntry>) -> Result<(), Error> {
    let raw = MediaCatalogConfigEntry::write_section_config(MEDIA_CFG_FILENAME, config)?;
    replace_config(MEDIA_CFG_FILENAME, raw.as_bytes())?;
    Ok(())
}