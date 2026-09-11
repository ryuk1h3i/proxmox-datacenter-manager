//! Propagate a PBS datastore as a `pbs` storage onto the PVE remotes.
//!
//! PDM already holds the PBS host, fingerprint and API token, so it can write a
//! ready-to-use storage entry to every PVE remote. Note that the token secret
//! ends up in each remote's `storage.cfg`.

use anyhow::{Error, bail, format_err};
use serde_json::{Value, json};

use proxmox_client::HttpApiClient;
use proxmox_router::{Permission, Router};
use proxmox_schema::api;

use pdm_api_types::pbs::{PbsAttachResult, PbsPveStorageState};
use pdm_api_types::remotes::{REMOTE_ID_SCHEMA, RemoteType};
use pdm_api_types::{PRIV_RESOURCE_AUDIT, PRIV_RESOURCE_MANAGE, PVE_STORAGE_ID_SCHEMA};

pub const ROUTER: Router = Router::new()
    .get(&API_METHOD_PVE_STORAGE_STATE)
    .post(&API_METHOD_ATTACH_TO_PVE);

/// Split `host`, `host:port` and `[::1]:port` into address and port.
fn split_host_port(hostname: &str) -> (String, Option<u16>) {
    if let Some(rest) = hostname.strip_prefix('[') {
        if let Some((address, rest)) = rest.split_once(']') {
            let port = rest.strip_prefix(':').and_then(|port| port.parse().ok());
            return (address.to_string(), port);
        }
    }

    if let Some((address, port)) = hostname.rsplit_once(':') {
        if let Ok(port) = port.parse() {
            return (address.to_string(), Some(port));
        }
    }

    (hostname.to_string(), None)
}

struct PbsConnectionInfo {
    server: String,
    port: Option<u16>,
    fingerprint: Option<String>,
    username: String,
    password: String,
}

fn pbs_connection_info(remote: &str) -> Result<PbsConnectionInfo, Error> {
    let (remotes, _) = pdm_config::remotes::config()?;
    let entry = crate::pbs_client::get_remote(&remotes, remote)?;

    let node = entry
        .nodes
        .first()
        .ok_or_else(|| format_err!("remote '{remote}' has no node address configured"))?;

    let (server, port) = split_host_port(&node.hostname);

    // `remotes.cfg` only holds a placeholder, the real secret lives in `remotes.shadow`.
    let password = pdm_config::remotes::get_secret_token(entry)?;

    Ok(PbsConnectionInfo {
        server,
        port,
        fingerprint: node.fingerprint.clone(),
        username: entry.authid.to_string(),
        password,
    })
}

fn pve_remote_ids(selection: Option<Vec<String>>) -> Result<Vec<String>, Error> {
    let (remotes, _) = pdm_config::remotes::config()?;
    let all: Vec<String> = remotes
        .into_iter()
        .filter(|(_, remote)| remote.ty == RemoteType::Pve)
        .map(|(id, _)| id)
        .collect();

    Ok(match selection {
        Some(selection) => {
            for id in &selection {
                if !all.contains(id) {
                    bail!("'{id}' is not a PVE remote");
                }
            }
            selection
        }
        None => all,
    })
}

async fn list_pve_storages(remote: &str) -> Result<Vec<Value>, Error> {
    let client = crate::api::pve::raw_client_to_remote_by_id(remote)?;
    let data: Value = client
        .get("/api2/extjs/storage")
        .await?
        .expect_json()?
        .data;
    match data {
        Value::Array(storages) => Ok(storages),
        _ => bail!("unexpected response from the storage endpoint"),
    }
}

/// Find a `pbs` storage pointing at the given server and datastore.
fn find_matching_storage<'a>(
    storages: &'a [Value],
    server: &str,
    datastore: &str,
) -> Option<&'a Value> {
    storages.iter().find(|storage| {
        storage.get("type").and_then(Value::as_str) == Some("pbs")
            && storage.get("datastore").and_then(Value::as_str) == Some(datastore)
            && storage
                .get("server")
                .and_then(Value::as_str)
                .is_some_and(|value| value.eq_ignore_ascii_case(server))
    })
}

fn find_storage_by_id<'a>(storages: &'a [Value], id: &str) -> Option<&'a Value> {
    storages
        .iter()
        .find(|storage| storage.get("storage").and_then(Value::as_str) == Some(id))
}

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            datastore: { type: String, description: "The PBS datastore to look for." },
        },
    },
    returns: {
        description: "Per PVE remote state of the PBS storage.",
        type: Array,
        items: { type: PbsPveStorageState },
    },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}"], PRIV_RESOURCE_AUDIT, false),
    },
)]
/// Report which PVE remotes already have a storage for this PBS datastore.
pub async fn pve_storage_state(
    remote: String,
    datastore: String,
) -> Result<Vec<PbsPveStorageState>, Error> {
    let info = pbs_connection_info(&remote)?;

    let mut result = Vec::new();
    for pve_remote in pve_remote_ids(None)? {
        let mut state = PbsPveStorageState {
            remote: pve_remote.clone(),
            storage: None,
            error: None,
        };

        match list_pve_storages(&pve_remote).await {
            Ok(storages) => {
                state.storage = find_matching_storage(&storages, &info.server, &datastore)
                    .and_then(|storage| storage.get("storage"))
                    .and_then(Value::as_str)
                    .map(str::to_owned);
            }
            Err(err) => state.error = Some(format!("{err:#}")),
        }

        result.push(state);
    }

    Ok(result)
}

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            datastore: { type: String, description: "The PBS datastore to attach." },
            storage: { schema: PVE_STORAGE_ID_SCHEMA },
            "pve-remotes": {
                description: "PVE remotes to configure, defaults to all of them.",
                type: Array,
                optional: true,
                items: { schema: REMOTE_ID_SCHEMA },
            },
        },
    },
    returns: {
        description: "Per PVE remote outcome.",
        type: Array,
        items: { type: PbsAttachResult },
    },
    access: {
        permission: &Permission::Privilege(&["resource"], PRIV_RESOURCE_MANAGE, false),
    },
)]
/// Configure this PBS datastore as a storage on the selected PVE remotes.
pub async fn attach_to_pve(
    remote: String,
    datastore: String,
    storage: String,
    pve_remotes: Option<Vec<String>>,
) -> Result<Vec<PbsAttachResult>, Error> {
    let info = pbs_connection_info(&remote)?;

    let mut base = json!({
        "server": info.server,
        "datastore": datastore,
        "username": info.username,
        "password": info.password,
    });
    if let Some(port) = info.port {
        base["port"] = port.into();
    }
    if let Some(fingerprint) = &info.fingerprint {
        base["fingerprint"] = fingerprint.clone().into();
    }

    let mut result = Vec::new();
    for pve_remote in pve_remote_ids(pve_remotes)? {
        let outcome = attach_to_single_remote(&pve_remote, &storage, &datastore, &info, &base).await;

        result.push(match outcome {
            Ok((changed, message)) => PbsAttachResult {
                remote: pve_remote,
                changed,
                message,
                error: None,
            },
            Err(err) => PbsAttachResult {
                remote: pve_remote,
                changed: false,
                message: String::new(),
                error: Some(format!("{err:#}")),
            },
        });
    }

    Ok(result)
}

async fn attach_to_single_remote(
    pve_remote: &str,
    storage: &str,
    datastore: &str,
    info: &PbsConnectionInfo,
    base: &Value,
) -> Result<(bool, String), Error> {
    let storages = list_pve_storages(pve_remote).await?;
    let client = crate::api::pve::raw_client_to_remote_by_id(pve_remote)?;

    if find_storage_by_id(&storages, storage).is_some() {
        // only refresh the parts that may legitimately change
        let mut payload = json!({
            "username": info.username,
            "password": info.password,
        });
        if let Some(fingerprint) = &info.fingerprint {
            payload["fingerprint"] = fingerprint.clone().into();
        }
        let path = format!("/api2/extjs/storage/{storage}");
        client.put(&path, &payload).await?.nodata()?;
        return Ok((true, format!("updated existing storage '{storage}'")));
    }

    if let Some(existing) = find_matching_storage(&storages, &info.server, datastore) {
        let id = existing
            .get("storage")
            .and_then(Value::as_str)
            .unwrap_or("<unknown>");
        return Ok((
            false,
            format!("datastore already available as storage '{id}'"),
        ));
    }

    let mut payload = base.clone();
    payload["storage"] = storage.into();
    payload["type"] = "pbs".into();
    client
        .post("/api2/extjs/storage", &payload)
        .await?
        .nodata()?;

    Ok((true, format!("created storage '{storage}'")))
}
