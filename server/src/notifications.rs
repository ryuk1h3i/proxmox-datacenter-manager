//! Central notification dispatch for PDM, built on top of the shared `proxmox-notify` crate.
//!
//! Call [`init`] once at daemon startup (before any notification can be sent) to register the
//! PDM-specific notification [`Context`].

use std::collections::HashMap;

use proxmox_notify::context::Context;
use proxmox_notify::renderer::TemplateSource;
use proxmox_notify::{Notification, Severity};
use proxmox_rest_server::TaskState;

const TASK_RESULT_TEMPLATE: &str = "task-result";
const REMOTE_UNREACHABLE_TEMPLATE: &str = "remote-unreachable";

const TASK_RESULT_SUBJECT: &str = "{{job_type}} on {{hostname}}: {{status}}";

const TASK_RESULT_BODY: &str = r#"The scheduled task '{{job_type}}' finished with status: {{status}}

Job type:  {{job_type}}
Job ID:    {{job_id}}
Host:      {{hostname}}
"#;

const REMOTE_UNREACHABLE_SUBJECT: &str = "Remote '{{remote}}' is unreachable";

const REMOTE_UNREACHABLE_BODY: &str = r#"The remote '{{remote}}' could not be reached from {{hostname}}.

Remote:  {{remote}}
Host:    {{host}}
Error:   {{error}}
"#;

#[derive(Debug)]
struct PdmContext;

static PDM_CONTEXT: PdmContext = PdmContext;

impl Context for PdmContext {
    fn lookup_email_for_user(&self, _user: &str) -> Option<String> {
        // PDM does not manage per-user mail addresses, so targets must list recipients directly.
        None
    }

    fn default_sendmail_author(&self) -> String {
        format!("Proxmox Datacenter Manager - {}", proxmox_sys::nodename())
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
        // No built-in targets or matchers: without an explicit configuration nothing is sent.
        ""
    }

    /// Templates are compiled into the binary instead of being shipped as files, so there is
    /// nothing to look up for user overrides. `proxmox-notify` asks for the file name
    /// `<template>-<subject|body>.<txt|html>.hbs`; returning `None` for the HTML body makes it
    /// fall back to rendering the plaintext one.
    fn lookup_template(
        &self,
        filename: &str,
        _namespace: Option<&str>,
        source: TemplateSource,
    ) -> Result<Option<String>, proxmox_notify::Error> {
        if !matches!(source, TemplateSource::Vendor) || filename.ends_with(".html.hbs") {
            return Ok(None);
        }

        let template = if let Some((name, _)) = filename.split_once("-subject.") {
            match name {
                TASK_RESULT_TEMPLATE => Some(TASK_RESULT_SUBJECT),
                REMOTE_UNREACHABLE_TEMPLATE => Some(REMOTE_UNREACHABLE_SUBJECT),
                _ => None,
            }
        } else if let Some((name, _)) = filename.split_once("-body.") {
            match name {
                TASK_RESULT_TEMPLATE => Some(TASK_RESULT_BODY),
                REMOTE_UNREACHABLE_TEMPLATE => Some(REMOTE_UNREACHABLE_BODY),
                _ => None,
            }
        } else {
            None
        };

        Ok(template.map(ToString::to_string))
    }
}

/// Register the PDM notification context.
///
/// Must be called once at daemon startup, before any notification is sent.
pub fn init() {
    proxmox_notify::context::set_context(&PDM_CONTEXT);
}

/// Sends through every matcher that matches. Errors are logged, never propagated, so that a
/// broken notification target cannot fail the operation that triggered it.
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

/// Notify about the result of a finished locally scheduled task.
pub fn notify_task_result(jobtype: &str, jobname: &str, state: &TaskState) {
    let severity = match state {
        TaskState::OK { .. } => Severity::Info,
        TaskState::Warning { .. } => Severity::Warning,
        TaskState::Error { .. } => Severity::Error,
        TaskState::Unknown { .. } => Severity::Notice,
    };

    let hostname = proxmox_sys::nodename().to_string();
    let status = state.to_string();

    // Field keys use the dashed spelling that `match-field` rules are written against, while
    // the template data uses plain identifiers that handlebars can reference directly.
    let fields = HashMap::from([
        ("type".to_string(), "task-result".to_string()),
        ("job-type".to_string(), jobtype.to_string()),
        ("job-id".to_string(), jobname.to_string()),
        ("hostname".to_string(), hostname.clone()),
    ]);

    let data = serde_json::json!({
        "job_type": jobtype,
        "job_id": jobname,
        "hostname": hostname,
        "status": status,
    });

    send(Notification::from_template(
        severity,
        TASK_RESULT_TEMPLATE,
        data,
        fields,
    ));
}

/// Notify that a remote could not be reached any more.
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
        REMOTE_UNREACHABLE_TEMPLATE,
        data,
        fields,
    ));
}
