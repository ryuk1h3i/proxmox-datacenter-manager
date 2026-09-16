//! Central notification dispatch for PDM, built on top of the shared `proxmox-notify` crate.
//!
//! Call [`init`] once at daemon startup (before any notification can be sent) to register the
//! PDM-specific notification [`Context`].

use std::collections::HashMap;

use proxmox_notify::context::Context;
use proxmox_notify::renderer::TemplateSource;
use proxmox_notify::{Notification, Severity};
use proxmox_rest_server::TaskState;

#[derive(Debug)]
struct PdmContext;

static PDM_CONTEXT: PdmContext = PdmContext;

impl Context for PdmContext {
    fn lookup_email_for_user(&self, _user: &str) -> Option<String> {
        // PDM does not (yet) manage a separate user email directory.
        None
    }

    fn default_sendmail_author(&self) -> String {
        format!(
            "Proxmox Datacenter Manager - {}",
            proxmox_sys::nodename()
        )
    }

    fn default_sendmail_from(&self) -> String {
        pdm_config::node::config()
            .ok()
            .and_then(|(config, _digest)| config.email_from)
            .unwrap_or_else(|| String::from("root"))
    }

    fn http_proxy_config(&self) -> Option<String> {
        pdm_config::node::config()
            .ok()
            .and_then(|(config, _digest)| config.http_proxy)
    }

    fn default_config(&self) -> &'static str {
        // No built-in targets/matchers for now - an empty config is valid and matches the
        // previous behavior of a not-yet-existing notifications.cfg.
        ""
    }

    fn lookup_template(
        &self,
        _filename: &str,
        _namespace: Option<&str>,
        _source: TemplateSource,
    ) -> Result<Option<String>, proxmox_notify::Error> {
        // PDM does not (yet) ship custom notification templates, so fall back to the
        // defaults built into `proxmox-notify`.
        Ok(None)
    }
}

/// Register the PDM notification context.
///
/// Must be called once at daemon startup, before any notification is sent.
pub fn init() {
    proxmox_notify::context::set_context(&PDM_CONTEXT);
}

fn send(notification: Notification) {
    match pdm_config::notifications::config() {
        Ok((config, _digest)) => {
            if let Err(err) = proxmox_notify::api::common::send(&config, &notification) {
                log::warn!("could not send notification: {err}");
            }
        }
        Err(err) => log::warn!("could not load notification config: {err:#}"),
    }
}

/// Notify about the result of a finished job (e.g. a scheduled backup/sync job).
pub fn notify_task_result(jobtype: &str, jobname: &str, state: &TaskState) {
    let severity = match state {
        TaskState::OK { .. } => Severity::Info,
        TaskState::Warning { .. } => Severity::Notice,
        TaskState::Error { .. } => Severity::Error,
        TaskState::Unknown { .. } => Severity::Notice,
    };

    let hostname = proxmox_sys::nodename().to_string();

    let fields = HashMap::from([
        ("type".to_string(), "task".to_string()),
        ("job-type".to_string(), jobtype.to_string()),
        ("job-id".to_string(), jobname.to_string()),
        ("hostname".to_string(), hostname.clone()),
    ]);

    let data = serde_json::json!({
        "job-type": jobtype,
        "job-id": jobname,
        "hostname": hostname,
        "status": state.to_string(),
    });

    send(Notification::from_template(
        severity,
        "task-notification",
        data,
        fields,
    ));
}

/// Notify that a remote could not be reached.
pub fn notify_remote_unreachable(remote: &str, host: &str, error: &str) {
    let hostname = proxmox_sys::nodename().to_string();

    let fields = HashMap::from([
        ("type".to_string(), "remote-unreachable".to_string()),
        ("remote".to_string(), remote.to_string()),
        ("hostname".to_string(), hostname.clone()),
    ]);

    let data = serde_json::json!({
        "remote": remote,
        "host": host,
        "hostname": hostname,
        "error": error,
    });

    send(Notification::from_template(
        Severity::Error,
        "remote-unreachable",
        data,
        fields,
    ));
}
