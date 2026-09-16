//! Notification targets (sendmail, smtp, gotify, webhook) and matchers configuration API.
//!
//! This is a thin wrapper around the shared `proxmox-notify` crate, which is also used by PVE
//! and PBS to implement the exact same notification system (targets + matchers).

use anyhow::Error;

use proxmox_router::SubdirMap;
use proxmox_router::{Router, list_subdirs_api_method};
use proxmox_schema::api;
use proxmox_sortable_macro::sortable;

use proxmox_notify::api::{gotify as gotify_api, matcher as matcher_api, sendmail as sendmail_api};
use proxmox_notify::api::{smtp as smtp_api, webhook as webhook_api};
use proxmox_notify::endpoints::gotify::{
    DeleteableGotifyProperty, GotifyConfig, GotifyConfigUpdater, GotifyPrivateConfig,
    GotifyPrivateConfigUpdater,
};
use proxmox_notify::endpoints::sendmail::{
    DeleteableSendmailProperty, SendmailConfig, SendmailConfigUpdater,
};
use proxmox_notify::endpoints::smtp::{
    DeleteableSmtpProperty, SmtpConfig, SmtpConfigUpdater, SmtpPrivateConfig,
    SmtpPrivateConfigUpdater,
};
use proxmox_notify::endpoints::webhook::{
    DeleteableWebhookProperty, KeyAndBase64Val, WebhookConfig, WebhookConfigUpdater,
    WebhookPrivateConfig,
};
use proxmox_notify::matcher::{DeleteableMatcherProperty, MatcherConfig, MatcherConfigUpdater};

use pdm_api_types::{ConfigDigest, PRIV_SYS_MODIFY};

use crate::api::Permission;

const NOTIFICATIONS_PRIVILEGE: Permission =
    Permission::Privilege(&["system", "notifications"], PRIV_SYS_MODIFY, false);

#[sortable]
const SUBDIRS: SubdirMap = &sorted!([
    ("gotify", &GOTIFY_ROUTER),
    ("matchers", &MATCHER_ROUTER),
    ("sendmail", &SENDMAIL_ROUTER),
    ("smtp", &SMTP_ROUTER),
    ("webhook", &WEBHOOK_ROUTER),
]);

pub(crate) const ROUTER: Router = Router::new()
    .get(&list_subdirs_api_method!(SUBDIRS))
    .subdirs(SUBDIRS);

const SENDMAIL_ROUTER: Router = Router::new()
    .get(&API_METHOD_LIST_SENDMAIL_ENDPOINTS)
    .post(&API_METHOD_ADD_SENDMAIL_ENDPOINT)
    .match_all("name", &SENDMAIL_ITEM_ROUTER);

const SENDMAIL_ITEM_ROUTER: Router = Router::new()
    .get(&API_METHOD_GET_SENDMAIL_ENDPOINT)
    .put(&API_METHOD_UPDATE_SENDMAIL_ENDPOINT)
    .delete(&API_METHOD_DELETE_SENDMAIL_ENDPOINT);

const SMTP_ROUTER: Router = Router::new()
    .get(&API_METHOD_LIST_SMTP_ENDPOINTS)
    .post(&API_METHOD_ADD_SMTP_ENDPOINT)
    .match_all("name", &SMTP_ITEM_ROUTER);

const SMTP_ITEM_ROUTER: Router = Router::new()
    .get(&API_METHOD_GET_SMTP_ENDPOINT)
    .put(&API_METHOD_UPDATE_SMTP_ENDPOINT)
    .delete(&API_METHOD_DELETE_SMTP_ENDPOINT);

const GOTIFY_ROUTER: Router = Router::new()
    .get(&API_METHOD_LIST_GOTIFY_ENDPOINTS)
    .post(&API_METHOD_ADD_GOTIFY_ENDPOINT)
    .match_all("name", &GOTIFY_ITEM_ROUTER);

const GOTIFY_ITEM_ROUTER: Router = Router::new()
    .get(&API_METHOD_GET_GOTIFY_ENDPOINT)
    .put(&API_METHOD_UPDATE_GOTIFY_ENDPOINT)
    .delete(&API_METHOD_DELETE_GOTIFY_ENDPOINT);

const WEBHOOK_ROUTER: Router = Router::new()
    .get(&API_METHOD_LIST_WEBHOOK_ENDPOINTS)
    .post(&API_METHOD_ADD_WEBHOOK_ENDPOINT)
    .match_all("name", &WEBHOOK_ITEM_ROUTER);

const WEBHOOK_ITEM_ROUTER: Router = Router::new()
    .get(&API_METHOD_GET_WEBHOOK_ENDPOINT)
    .put(&API_METHOD_UPDATE_WEBHOOK_ENDPOINT)
    .delete(&API_METHOD_DELETE_WEBHOOK_ENDPOINT);

const MATCHER_ROUTER: Router = Router::new()
    .get(&API_METHOD_LIST_MATCHERS)
    .post(&API_METHOD_ADD_MATCHER)
    .match_all("name", &MATCHER_ITEM_ROUTER);

const MATCHER_ITEM_ROUTER: Router = Router::new()
    .get(&API_METHOD_GET_MATCHER)
    .put(&API_METHOD_UPDATE_MATCHER)
    .delete(&API_METHOD_DELETE_MATCHER);

// --- sendmail -------------------------------------------------------------

#[api(
    access: { permission: &NOTIFICATIONS_PRIVILEGE },
    returns: {
        type: Array,
        items: { type: SendmailConfig },
        description: "List of sendmail notification targets.",
    },
)]
/// List sendmail notification targets.
pub fn list_sendmail_endpoints() -> Result<Vec<SendmailConfig>, Error> {
    let (config, _digest) = pdm_config::notifications::config()?;
    Ok(sendmail_api::get_endpoints(&config)?)
}

#[api(
    input: { properties: { name: { type: String } } },
    access: { permission: &NOTIFICATIONS_PRIVILEGE },
    returns: { type: SendmailConfig },
)]
/// Get a sendmail notification target.
pub fn get_sendmail_endpoint(name: String) -> Result<SendmailConfig, Error> {
    let (config, _digest) = pdm_config::notifications::config()?;
    Ok(sendmail_api::get_endpoint(&config, &name)?)
}

#[api(
    input: { properties: { config: { type: SendmailConfig, flatten: true } } },
    access: { permission: &NOTIFICATIONS_PRIVILEGE },
    protected: true,
)]
/// Add a new sendmail notification target.
pub fn add_sendmail_endpoint(config: SendmailConfig) -> Result<(), Error> {
    let _lock = pdm_config::notifications::lock_config()?;
    let (mut notify_config, _digest) = pdm_config::notifications::config()?;

    sendmail_api::add_endpoint(&mut notify_config, config)?;

    pdm_config::notifications::save_config(notify_config)
}

#[api(
    input: {
        properties: {
            name: { type: String },
            update: { type: SendmailConfigUpdater, flatten: true },
            delete: {
                description: "List of properties to delete.",
                type: Array,
                optional: true,
                items: { type: DeleteableSendmailProperty },
            },
            digest: { type: ConfigDigest, optional: true },
        },
    },
    access: { permission: &NOTIFICATIONS_PRIVILEGE },
    protected: true,
)]
/// Update a sendmail notification target.
pub fn update_sendmail_endpoint(
    name: String,
    update: SendmailConfigUpdater,
    delete: Option<Vec<DeleteableSendmailProperty>>,
    digest: Option<ConfigDigest>,
) -> Result<(), Error> {
    let _lock = pdm_config::notifications::lock_config()?;
    let (mut notify_config, config_digest) = pdm_config::notifications::config()?;

    config_digest.detect_modification(digest.as_ref())?;

    sendmail_api::update_endpoint(&mut notify_config, &name, update, delete.as_deref(), None)?;

    pdm_config::notifications::save_config(notify_config)
}

#[api(
    input: {
        properties: {
            name: { type: String },
            digest: { type: ConfigDigest, optional: true },
        },
    },
    access: { permission: &NOTIFICATIONS_PRIVILEGE },
    protected: true,
)]
/// Delete a sendmail notification target.
pub fn delete_sendmail_endpoint(name: String, digest: Option<ConfigDigest>) -> Result<(), Error> {
    let _lock = pdm_config::notifications::lock_config()?;
    let (mut notify_config, config_digest) = pdm_config::notifications::config()?;

    config_digest.detect_modification(digest.as_ref())?;

    sendmail_api::delete_endpoint(&mut notify_config, &name)?;

    pdm_config::notifications::save_config(notify_config)
}

// --- smtp -------------------------------------------------------------

#[api(
    access: { permission: &NOTIFICATIONS_PRIVILEGE },
    returns: {
        type: Array,
        items: { type: SmtpConfig },
        description: "List of SMTP notification targets.",
    },
)]
/// List SMTP notification targets.
pub fn list_smtp_endpoints() -> Result<Vec<SmtpConfig>, Error> {
    let (config, _digest) = pdm_config::notifications::config()?;
    Ok(smtp_api::get_endpoints(&config)?)
}

#[api(
    input: { properties: { name: { type: String } } },
    access: { permission: &NOTIFICATIONS_PRIVILEGE },
    returns: { type: SmtpConfig },
)]
/// Get an SMTP notification target.
pub fn get_smtp_endpoint(name: String) -> Result<SmtpConfig, Error> {
    let (config, _digest) = pdm_config::notifications::config()?;
    Ok(smtp_api::get_endpoint(&config, &name)?)
}

#[api(
    input: {
        properties: {
            config: { type: SmtpConfig, flatten: true },
            password: {
                description: "Password for authentication with the SMTP server.",
                type: String,
                optional: true,
            },
        },
    },
    access: { permission: &NOTIFICATIONS_PRIVILEGE },
    protected: true,
)]
/// Add a new SMTP notification target.
pub fn add_smtp_endpoint(config: SmtpConfig, password: Option<String>) -> Result<(), Error> {
    let _lock = pdm_config::notifications::lock_config()?;
    let (mut notify_config, _digest) = pdm_config::notifications::config()?;

    let private_config = SmtpPrivateConfig {
        name: config.name.clone(),
        password,
    };

    smtp_api::add_endpoint(&mut notify_config, config, private_config)?;

    pdm_config::notifications::save_config(notify_config)
}

#[api(
    input: {
        properties: {
            name: { type: String },
            update: { type: SmtpConfigUpdater, flatten: true },
            password: {
                description: "Password for authentication with the SMTP server.",
                type: String,
                optional: true,
            },
            delete: {
                description: "List of properties to delete.",
                type: Array,
                optional: true,
                items: { type: DeleteableSmtpProperty },
            },
            digest: { type: ConfigDigest, optional: true },
        },
    },
    access: { permission: &NOTIFICATIONS_PRIVILEGE },
    protected: true,
)]
/// Update an SMTP notification target.
#[allow(clippy::too_many_arguments)]
pub fn update_smtp_endpoint(
    name: String,
    update: SmtpConfigUpdater,
    password: Option<String>,
    delete: Option<Vec<DeleteableSmtpProperty>>,
    digest: Option<ConfigDigest>,
) -> Result<(), Error> {
    let _lock = pdm_config::notifications::lock_config()?;
    let (mut notify_config, config_digest) = pdm_config::notifications::config()?;

    config_digest.detect_modification(digest.as_ref())?;

    let private_update = SmtpPrivateConfigUpdater { password };

    smtp_api::update_endpoint(
        &mut notify_config,
        &name,
        update,
        private_update,
        delete.as_deref(),
        None,
    )?;

    pdm_config::notifications::save_config(notify_config)
}

#[api(
    input: {
        properties: {
            name: { type: String },
            digest: { type: ConfigDigest, optional: true },
        },
    },
    access: { permission: &NOTIFICATIONS_PRIVILEGE },
    protected: true,
)]
/// Delete an SMTP notification target.
pub fn delete_smtp_endpoint(name: String, digest: Option<ConfigDigest>) -> Result<(), Error> {
    let _lock = pdm_config::notifications::lock_config()?;
    let (mut notify_config, config_digest) = pdm_config::notifications::config()?;

    config_digest.detect_modification(digest.as_ref())?;

    smtp_api::delete_endpoint(&mut notify_config, &name)?;

    pdm_config::notifications::save_config(notify_config)
}

// --- gotify -------------------------------------------------------------

#[api(
    access: { permission: &NOTIFICATIONS_PRIVILEGE },
    returns: {
        type: Array,
        items: { type: GotifyConfig },
        description: "List of Gotify notification targets.",
    },
)]
/// List Gotify notification targets.
pub fn list_gotify_endpoints() -> Result<Vec<GotifyConfig>, Error> {
    let (config, _digest) = pdm_config::notifications::config()?;
    Ok(gotify_api::get_endpoints(&config)?)
}

#[api(
    input: { properties: { name: { type: String } } },
    access: { permission: &NOTIFICATIONS_PRIVILEGE },
    returns: { type: GotifyConfig },
)]
/// Get a Gotify notification target.
pub fn get_gotify_endpoint(name: String) -> Result<GotifyConfig, Error> {
    let (config, _digest) = pdm_config::notifications::config()?;
    Ok(gotify_api::get_endpoint(&config, &name)?)
}

#[api(
    input: {
        properties: {
            config: { type: GotifyConfig, flatten: true },
            token: {
                description: "Authentication token for the Gotify server.",
                type: String,
            },
        },
    },
    access: { permission: &NOTIFICATIONS_PRIVILEGE },
    protected: true,
)]
/// Add a new Gotify notification target.
pub fn add_gotify_endpoint(config: GotifyConfig, token: String) -> Result<(), Error> {
    let _lock = pdm_config::notifications::lock_config()?;
    let (mut notify_config, _digest) = pdm_config::notifications::config()?;

    let private_config = GotifyPrivateConfig {
        name: config.name.clone(),
        token,
    };

    gotify_api::add_endpoint(&mut notify_config, config, private_config)?;

    pdm_config::notifications::save_config(notify_config)
}

#[api(
    input: {
        properties: {
            name: { type: String },
            update: { type: GotifyConfigUpdater, flatten: true },
            token: {
                description: "Authentication token for the Gotify server.",
                type: String,
                optional: true,
            },
            delete: {
                description: "List of properties to delete.",
                type: Array,
                optional: true,
                items: { type: DeleteableGotifyProperty },
            },
            digest: { type: ConfigDigest, optional: true },
        },
    },
    access: { permission: &NOTIFICATIONS_PRIVILEGE },
    protected: true,
)]
/// Update a Gotify notification target.
#[allow(clippy::too_many_arguments)]
pub fn update_gotify_endpoint(
    name: String,
    update: GotifyConfigUpdater,
    token: Option<String>,
    delete: Option<Vec<DeleteableGotifyProperty>>,
    digest: Option<ConfigDigest>,
) -> Result<(), Error> {
    let _lock = pdm_config::notifications::lock_config()?;
    let (mut notify_config, config_digest) = pdm_config::notifications::config()?;

    config_digest.detect_modification(digest.as_ref())?;

    let private_update = GotifyPrivateConfigUpdater { token };

    gotify_api::update_endpoint(
        &mut notify_config,
        &name,
        update,
        private_update,
        delete.as_deref(),
        None,
    )?;

    pdm_config::notifications::save_config(notify_config)
}

#[api(
    input: {
        properties: {
            name: { type: String },
            digest: { type: ConfigDigest, optional: true },
        },
    },
    access: { permission: &NOTIFICATIONS_PRIVILEGE },
    protected: true,
)]
/// Delete a Gotify notification target.
pub fn delete_gotify_endpoint(name: String, digest: Option<ConfigDigest>) -> Result<(), Error> {
    let _lock = pdm_config::notifications::lock_config()?;
    let (mut notify_config, config_digest) = pdm_config::notifications::config()?;

    config_digest.detect_modification(digest.as_ref())?;

    gotify_api::delete_endpoint(&mut notify_config, &name)?;

    pdm_config::notifications::save_config(notify_config)
}

// --- webhook -------------------------------------------------------------
//
// A generic Webhook target can also be pointed at the Telegram Bot API
// (`https://api.telegram.org/bot<token>/sendMessage`) to get Telegram notifications without any
// dedicated PDM code - see the notifications documentation for a step-by-step guide.

#[api(
    access: { permission: &NOTIFICATIONS_PRIVILEGE },
    returns: {
        type: Array,
        items: { type: WebhookConfig },
        description: "List of webhook notification targets.",
    },
)]
/// List webhook notification targets.
pub fn list_webhook_endpoints() -> Result<Vec<WebhookConfig>, Error> {
    let (config, _digest) = pdm_config::notifications::config()?;
    Ok(webhook_api::get_endpoints(&config)?)
}

#[api(
    input: { properties: { name: { type: String } } },
    access: { permission: &NOTIFICATIONS_PRIVILEGE },
    returns: { type: WebhookConfig },
)]
/// Get a webhook notification target.
pub fn get_webhook_endpoint(name: String) -> Result<WebhookConfig, Error> {
    let (config, _digest) = pdm_config::notifications::config()?;
    Ok(webhook_api::get_endpoint(&config, &name)?)
}

#[api(
    input: {
        properties: {
            config: { type: WebhookConfig, flatten: true },
            secret: {
                description: "Secrets that can be referenced from the URL, header or body \
                    templates as `{{ secrets.<name> }}`.",
                type: Array,
                items: { type: KeyAndBase64Val },
                optional: true,
            },
        },
    },
    access: { permission: &NOTIFICATIONS_PRIVILEGE },
    protected: true,
)]
/// Add a new webhook notification target.
pub fn add_webhook_endpoint(
    config: WebhookConfig,
    secret: Option<Vec<KeyAndBase64Val>>,
) -> Result<(), Error> {
    let _lock = pdm_config::notifications::lock_config()?;
    let (mut notify_config, _digest) = pdm_config::notifications::config()?;

    let private_config = WebhookPrivateConfig {
        name: config.name.clone(),
        secret: secret
            .unwrap_or_default()
            .into_iter()
            .map(Into::into)
            .collect(),
    };

    webhook_api::add_endpoint(&mut notify_config, config, private_config)?;

    pdm_config::notifications::save_config(notify_config)
}

#[api(
    input: {
        properties: {
            name: { type: String },
            update: { type: WebhookConfigUpdater, flatten: true },
            delete: {
                description: "List of properties to delete.",
                type: Array,
                optional: true,
                items: { type: DeleteableWebhookProperty },
            },
            digest: { type: ConfigDigest, optional: true },
        },
    },
    access: { permission: &NOTIFICATIONS_PRIVILEGE },
    protected: true,
)]
/// Update a webhook notification target.
pub fn update_webhook_endpoint(
    name: String,
    update: WebhookConfigUpdater,
    delete: Option<Vec<DeleteableWebhookProperty>>,
    digest: Option<ConfigDigest>,
) -> Result<(), Error> {
    let _lock = pdm_config::notifications::lock_config()?;
    let (mut notify_config, config_digest) = pdm_config::notifications::config()?;

    config_digest.detect_modification(digest.as_ref())?;

    webhook_api::update_endpoint(&mut notify_config, &name, update, delete.as_deref(), None)?;

    pdm_config::notifications::save_config(notify_config)
}

#[api(
    input: {
        properties: {
            name: { type: String },
            digest: { type: ConfigDigest, optional: true },
        },
    },
    access: { permission: &NOTIFICATIONS_PRIVILEGE },
    protected: true,
)]
/// Delete a webhook notification target.
pub fn delete_webhook_endpoint(name: String, digest: Option<ConfigDigest>) -> Result<(), Error> {
    let _lock = pdm_config::notifications::lock_config()?;
    let (mut notify_config, config_digest) = pdm_config::notifications::config()?;

    config_digest.detect_modification(digest.as_ref())?;

    webhook_api::delete_endpoint(&mut notify_config, &name)?;

    pdm_config::notifications::save_config(notify_config)
}

// --- matchers -------------------------------------------------------------

#[api(
    access: { permission: &NOTIFICATIONS_PRIVILEGE },
    returns: {
        type: Array,
        items: { type: MatcherConfig },
        description: "List of notification matchers.",
    },
)]
/// List notification matchers.
pub fn list_matchers() -> Result<Vec<MatcherConfig>, Error> {
    let (config, _digest) = pdm_config::notifications::config()?;
    Ok(matcher_api::get_matchers(&config)?)
}

#[api(
    input: { properties: { name: { type: String } } },
    access: { permission: &NOTIFICATIONS_PRIVILEGE },
    returns: { type: MatcherConfig },
)]
/// Get a notification matcher.
pub fn get_matcher(name: String) -> Result<MatcherConfig, Error> {
    let (config, _digest) = pdm_config::notifications::config()?;
    Ok(matcher_api::get_matcher(&config, &name)?)
}

#[api(
    input: { properties: { config: { type: MatcherConfig, flatten: true } } },
    access: { permission: &NOTIFICATIONS_PRIVILEGE },
    protected: true,
)]
/// Add a new notification matcher.
pub fn add_matcher(config: MatcherConfig) -> Result<(), Error> {
    let _lock = pdm_config::notifications::lock_config()?;
    let (mut notify_config, _digest) = pdm_config::notifications::config()?;

    matcher_api::add_matcher(&mut notify_config, config)?;

    pdm_config::notifications::save_config(notify_config)
}

#[api(
    input: {
        properties: {
            name: { type: String },
            update: { type: MatcherConfigUpdater, flatten: true },
            delete: {
                description: "List of properties to delete.",
                type: Array,
                optional: true,
                items: { type: DeleteableMatcherProperty },
            },
            digest: { type: ConfigDigest, optional: true },
        },
    },
    access: { permission: &NOTIFICATIONS_PRIVILEGE },
    protected: true,
)]
/// Update a notification matcher.
pub fn update_matcher(
    name: String,
    update: MatcherConfigUpdater,
    delete: Option<Vec<DeleteableMatcherProperty>>,
    digest: Option<ConfigDigest>,
) -> Result<(), Error> {
    let _lock = pdm_config::notifications::lock_config()?;
    let (mut notify_config, config_digest) = pdm_config::notifications::config()?;

    config_digest.detect_modification(digest.as_ref())?;

    matcher_api::update_matcher(&mut notify_config, &name, update, delete.as_deref(), None)?;

    pdm_config::notifications::save_config(notify_config)
}

#[api(
    input: {
        properties: {
            name: { type: String },
            digest: { type: ConfigDigest, optional: true },
        },
    },
    access: { permission: &NOTIFICATIONS_PRIVILEGE },
    protected: true,
)]
/// Delete a notification matcher.
pub fn delete_matcher(name: String, digest: Option<ConfigDigest>) -> Result<(), Error> {
    let _lock = pdm_config::notifications::lock_config()?;
    let (mut notify_config, config_digest) = pdm_config::notifications::config()?;

    config_digest.detect_modification(digest.as_ref())?;

    matcher_api::delete_matcher(&mut notify_config, &name)?;

    pdm_config::notifications::save_config(notify_config)
}
