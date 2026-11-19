//! Read/write the notification targets/matchers configuration (`notifications.cfg` /
//! `notifications-priv.cfg`), backed by the shared `proxmox-notify` crate.

use anyhow::Error;

use proxmox_config_digest::ConfigDigest;
use proxmox_notify::Config;
use proxmox_product_config::{
    ApiLockGuard, open_api_lockfile, replace_config, replace_privileged_config,
};
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
    let config = Config::parse_section_config(&content, &priv_content, None)?;

    Ok((config, digest.into()))
}

/// Replace the currently persisted notifications config (public and private part).
pub fn save_config(config: Config) -> Result<(), Error> {
    let (raw, priv_raw) = config.to_section_config()?;

    // write the private file first, it contains the secrets and has stricter permissions
    replace_privileged_config(NOTIFICATION_PRIV_CFG_FILENAME, priv_raw.as_bytes())?;
    replace_config(NOTIFICATION_CFG_FILENAME, raw.as_bytes())?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notification_config_preserves_legacy_matchers_and_secrets() -> Result<(), Error> {
        let public = "smtp: mail-server\n\tserver smtp.example.com\n\tfrom-address pdm@example.com\n\tmailto admin@example.com\n\tusername pdm\n\nmatcher: failures\n\tmatch-severity warning,error\n\tmatch-field exact:type=task-result\n\ttarget mail-server\n";
        let private = "smtp: mail-server\n\tpassword fake-test-password\n";
        let config = Config::parse_section_config(public, private, None)?;
        let (written, private_written) = config.to_section_config()?;
        assert!(!written.contains("fake-test-password"));
        assert!(private_written.contains("fake-test-password"));
        let restored = Config::parse_section_config(&written, &private_written, None)?;
        assert_eq!(
            serde_json::to_value(proxmox_notify::api::matcher::get_matcher(
                &config, "failures"
            )?)?,
            serde_json::to_value(proxmox_notify::api::matcher::get_matcher(
                &restored, "failures"
            )?)?,
        );
        assert_eq!(
            serde_json::to_value(proxmox_notify::api::smtp::get_endpoint(
                &config,
                "mail-server"
            )?)?,
            serde_json::to_value(proxmox_notify::api::smtp::get_endpoint(
                &restored,
                "mail-server"
            )?)?,
        );
        Ok(())
    }

    #[test]
    fn notification_config_empty_and_invalid_input() -> Result<(), Error> {
        let config = Config::parse_section_config("", "", None)?;
        assert!(proxmox_notify::api::get_targets(&config)?.is_empty());
        assert!(Config::parse_section_config("unknown: invalid\n", "", None).is_err());
        Ok(())
    }

    #[test]
    fn notification_config_rejects_stale_digest() -> Result<(), Error> {
        let mut config = Config::parse_section_config(
            "sendmail: mail-target\n\tmailto admin@example.com\n",
            "",
            None,
        )?;
        let update = serde_json::from_value(serde_json::json!({ "comment": "changed" }))?;
        assert!(
            proxmox_notify::api::sendmail::update_endpoint(
                &mut config,
                "mail-target",
                update,
                None,
                Some([0u8; 32].into()),
            )
            .is_err()
        );
        Ok(())
    }
}
