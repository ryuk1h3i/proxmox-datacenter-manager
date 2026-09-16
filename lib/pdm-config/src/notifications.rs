//! Read/write the notification targets/matchers configuration (`notifications.cfg` /
//! `notifications-priv.cfg`), backed by the shared `proxmox-notify` crate.

use anyhow::Error;

use proxmox_config_digest::ConfigDigest;
use proxmox_notify::Config;
use proxmox_product_config::{ApiLockGuard, open_api_lockfile, replace_config, replace_privileged_config};
use proxmox_sys::fs::file_read_optional_string;

use pdm_buildcfg::configdir;

/// Configuration file location for notification targets/matchers.
pub const NOTIFICATION_CFG_FILENAME: &str = configdir!("/notifications.cfg");
/// Private configuration file location for secrets (only readable by `root`).
pub const NOTIFICATION_PRIV_CFG_FILENAME: &str = configdir!("/notifications-priv.cfg");
/// Lockfile to prevent concurrent write access.
pub const NOTIFICATION_CFG_LOCKFILE: &str = configdir!("/.notifications.lock");

/// Lock the notifications config.
pub fn lock_config() -> Result<ApiLockGuard, Error> {
    open_api_lockfile(NOTIFICATION_CFG_LOCKFILE, None, true)
}

/// Return the parsed notifications config, together with a digest of the public part (used for
/// optimistic-locking on updates).
pub fn config() -> Result<(Config, ConfigDigest), Error> {
    let content = file_read_optional_string(NOTIFICATION_CFG_FILENAME)?.unwrap_or_default();
    let priv_content =
        file_read_optional_string(NOTIFICATION_PRIV_CFG_FILENAME)?.unwrap_or_default();

    let digest = openssl::sha::sha256(content.as_bytes());
    let config = Config::new(&content, &priv_content)?;

    Ok((config, digest.into()))
}

/// Replace the currently persisted notifications config (public and private part).
pub fn save_config(config: Config) -> Result<(), Error> {
    let (raw, priv_raw) = config.write()?;

    // write the private file first, it contains the secrets and has stricter permissions
    replace_privileged_config(NOTIFICATION_PRIV_CFG_FILENAME, priv_raw.as_bytes())?;
    replace_config(NOTIFICATION_CFG_FILENAME, raw.as_bytes())?;

    Ok(())
}
