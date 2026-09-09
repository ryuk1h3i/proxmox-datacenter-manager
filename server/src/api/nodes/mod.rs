//! Server/Node Configuration and Administration

use proxmox_router::{Router, SubdirMap, list_subdirs_api_method};
use proxmox_sortable_macro::sortable;

pub mod certificates;
pub mod config;
pub mod sdn;
pub mod tasks;
pub mod vncwebsocket;

use anyhow::Error;
use serde_json::{Value, json};

use proxmox_schema::api;

#[api]
/// List Nodes (only for compatibility)
pub fn list_nodes() -> Result<Value, Error> {
    Ok(json!([ { "node": proxmox_sys::nodename().to_string() } ]))
}

pub const ROUTER: Router = Router::new()
    .get(&API_METHOD_LIST_NODES)
    .match_all("node", &ITEM_ROUTER);

pub const ITEM_ROUTER: Router = Router::new()
    .get(&list_subdirs_api_method!(SUBDIRS))
    .subdirs(SUBDIRS);

#[sortable]
pub const SUBDIRS: SubdirMap = &sorted!([
    ("certificates", &certificates::ROUTER),
    ("config", &config::ROUTER),
    ("sdn", &sdn::ROUTER),
    ("tasks", &tasks::ROUTER),
]);
