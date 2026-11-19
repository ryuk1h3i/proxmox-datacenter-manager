//! Central notification dispatch for PDM, built on top of the shared `proxmox-notify` crate.
//!
//! Notifications use an explicit PDM context and preserve the existing matcher-based routing.

use std::collections::HashMap;
use std::sync::OnceLock;

use anyhow::{Error, bail};
use proxmox_notify::api_types::{MatcherReceiptStatus, Receipt, TargetReceiptStatus};
use proxmox_notify::context::TemplateSource;
use proxmox_notify::{
    Config, ConfigEntity, Context, ContextImpl, Notification, Recipient, Severity,
};
use proxmox_rest_server::TaskState;
use proxmox_section_config::typed::SectionConfigData;

const TASK_RESULT_TEMPLATE: &str = "task-result";
const REMOTE_UNREACHABLE_TEMPLATE: &str = "remote-unreachable";
const TEST_TEMPLATE: &str = "test-notification";

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

type ContextError = Box<dyn std::error::Error + Send + Sync>;

impl ContextImpl for PdmContext {
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

    fn proxy_config(&self) -> Result<Option<proxmox_http::ProxyConfig>, ContextError> {
        pdm_config::node::config()
            .ok()
            .and_then(|(config, _digest)| config.http_proxy)
            .map(|url| proxmox_http::ProxyConfig::parse_proxy_url(&url))
            .transpose()
            .map_err(Into::into)
    }

    fn default_config(&self) -> &'static SectionConfigData<ConfigEntity> {
        static CONFIG: OnceLock<SectionConfigData<ConfigEntity>> = OnceLock::new();
        CONFIG.get_or_init(SectionConfigData::default)
    }

    fn read_config(&self) -> Result<Config, ContextError> {
        pdm_config::notifications::config()
            .map(|(config, _digest)| config)
            .map_err(Into::into)
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
    ) -> Result<Option<String>, ContextError> {
        if !matches!(source, TemplateSource::Vendor) || filename.ends_with(".html.hbs") {
            return Ok(None);
        }

        let template = if let Some((name, _)) = filename.split_once("-subject.") {
            match name {
                TASK_RESULT_TEMPLATE => Some(TASK_RESULT_SUBJECT),
                REMOTE_UNREACHABLE_TEMPLATE => Some(REMOTE_UNREACHABLE_SUBJECT),
                TEST_TEMPLATE => Some("Test notification from {{hostname}}"),
                _ => None,
            }
        } else if let Some((name, _)) = filename.split_once("-body.") {
            match name {
                TASK_RESULT_TEMPLATE => Some(TASK_RESULT_BODY),
                REMOTE_UNREACHABLE_TEMPLATE => Some(REMOTE_UNREACHABLE_BODY),
                TEST_TEMPLATE => Some(
                    "This is a test notification from Proxmox Datacenter Manager on {{hostname}}.\n",
                ),
                _ => None,
            }
        } else {
            None
        };

        Ok(template.map(ToString::to_string))
    }
}

fn check_delivery(receipt: &Receipt) -> Result<(), Error> {
    for target in receipt.target_receipts() {
        if let TargetReceiptStatus::Error { error_message } = target.status() {
            bail!(
                "notification target '{}': {error_message}",
                target.target().unwrap_or("email")
            );
        }
    }
    for matcher in receipt.matcher_receipts() {
        if let MatcherReceiptStatus::Error { error_message } = matcher.status() {
            bail!(
                "notification matcher '{}': {error_message}",
                matcher.matcher()
            );
        }
    }
    Ok(())
}

fn test_notification(name: &str) -> Result<Notification, proxmox_notify::Error> {
    Notification::template_builder(
        TEST_TEMPLATE,
        serde_json::json!({ "hostname": proxmox_sys::nodename() }),
    )
    .severity(Severity::Info)
    .recipient(Recipient::Target(name.to_string()))
    .build()
}

pub fn test_target(config: &Config, name: &str) -> Result<(), Error> {
    let target = proxmox_notify::api::get_targets(config)?
        .into_iter()
        .find(|target| target.name == name)
        .ok_or_else(|| anyhow::format_err!("notification target '{name}' does not exist"))?;
    if target.disable == Some(true) {
        bail!("notification target '{name}' is disabled");
    }
    let notification = test_notification(name)?;
    let receipt = proxmox_notify::send(&Context::new(PdmContext), config, &notification)?;
    check_delivery(&receipt)?;
    if !receipt.target_receipts().iter().any(|target| {
        target.target() == Some(name) && matches!(target.status(), TargetReceiptStatus::Delivered)
    }) {
        bail!("notification target '{name}' did not confirm delivery");
    }
    Ok(())
}

/// Sends through every matcher that matches. Errors are logged, never propagated, so that a
/// broken notification target cannot fail the operation that triggered it.
fn send(notification: Result<Notification, proxmox_notify::Error>) {
    let notification = match notification {
        Ok(notification) => notification,
        Err(err) => {
            log::warn!("could not build notification: {err}");
            return;
        }
    };
    match pdm_config::notifications::config() {
        Ok((config, _digest)) => {
            let result = proxmox_notify::send(&Context::new(PdmContext), &config, &notification)
                .map_err(Error::from)
                .and_then(|receipt| check_delivery(&receipt));
            if let Err(err) = result {
                log::warn!("could not send notification: {err}");
            }
        }
        Err(err) => log::warn!("could not load notification config: {err:#}"),
    }
}

/// Notify about the result of a finished locally scheduled task.
pub fn notify_task_result(jobtype: &str, jobname: &str, state: &TaskState) {
    send(task_result_notification(jobtype, jobname, state));
}

fn task_result_notification(
    jobtype: &str,
    jobname: &str,
    state: &TaskState,
) -> Result<Notification, proxmox_notify::Error> {
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

    Notification::template_builder(TASK_RESULT_TEMPLATE, data)
        .severity(severity)
        .fields(fields)
        .build()
}

/// Notify that a remote could not be reached any more.
pub fn notify_remote_unreachable(remote: &str, host: &str, error: &str) {
    send(remote_unreachable_notification(remote, host, error));
}

fn remote_unreachable_notification(
    remote: &str,
    host: &str,
    error: &str,
) -> Result<Notification, proxmox_notify::Error> {
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

    Notification::template_builder(REMOTE_UNREACHABLE_TEMPLATE, data)
        .severity(Severity::Error)
        .fields(fields)
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notification_test_bypasses_matchers() -> Result<(), Error> {
        let notification = test_notification("selected-target")?;
        assert_eq!(
            notification.recipient(),
            &Recipient::Target("selected-target".into())
        );
        assert_eq!(notification.severity(), Severity::Info);
        Ok(())
    }

    #[test]
    fn notification_missing_and_disabled_targets_fail() -> Result<(), Error> {
        let empty = Config::parse_section_config("", "", None)?;
        assert!(test_target(&empty, "missing").is_err());
        let disabled = Config::parse_section_config(
            "sendmail: disabled-target\n\tmailto admin@example.com\n\tdisable true\n",
            "",
            None,
        )?;
        assert!(
            test_target(&disabled, "disabled-target")
                .unwrap_err()
                .to_string()
                .contains("disabled")
        );
        Ok(())
    }

    #[test]
    fn notification_delivery_errors_are_not_success() -> Result<(), Error> {
        let receipt: Receipt = serde_json::from_value(serde_json::json!({
            "target-receipts": [{
                "target": "smtp-target", "status": "error",
                "error-message": "delivery failed", "timestamp": 0,
            }],
            "matcher-receipts": [],
        }))?;
        assert!(
            check_delivery(&receipt)
                .unwrap_err()
                .to_string()
                .contains("delivery failed")
        );
        let receipt: Receipt = serde_json::from_value(serde_json::json!({
            "target-receipts": [],
            "matcher-receipts": [{
                "matcher": "broken-matcher", "status": "error",
                "error-message": "evaluation failed",
            }],
        }))?;
        assert!(check_delivery(&receipt).is_err());
        Ok(())
    }

    #[test]
    fn notification_empty_config_sends_nothing() -> Result<(), Error> {
        let config = Config::parse_section_config("", "", None)?;
        let receipt = proxmox_notify::send(
            &Context::new(PdmContext),
            &config,
            &test_notification("missing")?,
        )?;
        assert!(check_delivery(&receipt).is_err());
        let notification = remote_unreachable_notification("remote", "host", "offline")?;
        let receipt = proxmox_notify::send(&Context::new(PdmContext), &config, &notification)?;
        assert!(receipt.target_receipts().is_empty());
        assert!(check_delivery(&receipt).is_ok());
        Ok(())
    }

    #[test]
    fn notification_events_preserve_metadata() -> Result<(), Error> {
        let task =
            task_result_notification("backup", "nightly", &TaskState::Unknown { endtime: 0 })?;
        assert_eq!(task.recipient(), &Recipient::Matcher);
        assert_eq!(
            task.fields().get("type").map(String::as_str),
            Some("task-result")
        );
        assert_eq!(
            task.fields().get("job-type").map(String::as_str),
            Some("backup")
        );
        assert_eq!(
            task.fields().get("job-id").map(String::as_str),
            Some("nightly")
        );
        let remote = remote_unreachable_notification("site-a", "host", "offline")?;
        assert_eq!(remote.severity(), Severity::Error);
        assert_eq!(remote.recipient(), &Recipient::Matcher);
        assert_eq!(
            remote.fields().get("remote").map(String::as_str),
            Some("site-a")
        );
        Ok(())
    }

    #[test]
    fn notification_templates_remain_available() -> Result<(), Error> {
        for template in [
            TASK_RESULT_TEMPLATE,
            REMOTE_UNREACHABLE_TEMPLATE,
            TEST_TEMPLATE,
        ] {
            for suffix in ["subject.txt.hbs", "body.txt.hbs"] {
                let filename = format!("{template}-{suffix}");
                assert!(
                    PdmContext
                        .lookup_template(&filename, None, TemplateSource::Vendor)
                        .map_err(|err| anyhow::format_err!("{err}"))?
                        .is_some()
                );
            }
        }
        assert!(
            PdmContext
                .lookup_template("task-result-body.html.hbs", None, TemplateSource::Vendor)
                .map_err(|err| anyhow::format_err!("{err}"))?
                .is_none()
        );
        Ok(())
    }
}
