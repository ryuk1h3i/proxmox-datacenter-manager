use anyhow::Error;

use proxmox_product_config::{ApiLockGuard, open_api_lockfile, replace_config};
use proxmox_section_config::typed::{ApiSectionDataEntry, SectionConfigData};

use pdm_api_types::{ConfigDigest, backup_jobs::BackupJobConfigEntry};

use pdm_buildcfg::configdir;

const BACKUP_JOB_CFG_FILENAME: &str = configdir!("/backup-jobs.cfg");
const BACKUP_JOB_CFG_LOCKFILE: &str = configdir!("/.backup-jobs.lock");

/// Get the `backup-jobs.cfg` config file contents.
pub fn config() -> Result<(SectionConfigData<BackupJobConfigEntry>, ConfigDigest), Error> {
    let content =
        proxmox_sys::fs::file_read_optional_string(BACKUP_JOB_CFG_FILENAME)?.unwrap_or_default();

    let digest = openssl::sha::sha256(content.as_bytes());

    let data = BackupJobConfigEntry::parse_section_config(BACKUP_JOB_CFG_FILENAME, &content)?;
    Ok((data, digest.into()))
}

/// Get exclusive lock
pub fn lock_config() -> Result<ApiLockGuard, Error> {
    open_api_lockfile(BACKUP_JOB_CFG_LOCKFILE, None, true)
}

pub fn save_config(config: &SectionConfigData<BackupJobConfigEntry>) -> Result<(), Error> {
    let raw = BackupJobConfigEntry::write_section_config(BACKUP_JOB_CFG_FILENAME, config)?;
    replace_config(BACKUP_JOB_CFG_FILENAME, raw.as_bytes())?;
    Ok(())
}
