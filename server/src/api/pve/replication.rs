use anyhow::Error;

use proxmox_client::HttpApiClient;
use proxmox_router::{Permission, Router, SubdirMap, list_subdirs_api_method};
use proxmox_schema::api;
use proxmox_sortable_macro::sortable;

use pdm_api_types::pve_jobs::{
    PveReplicationJob, PveReplicationJobConfig, PveReplicationStatus,
};
use pdm_api_types::remotes::REMOTE_ID_SCHEMA;
use pdm_api_types::{
    NODE_SCHEMA, PRIV_RESOURCE_AUDIT, PRIV_RESOURCE_CREATE, PRIV_RESOURCE_DELETE,
    PRIV_RESOURCE_MANAGE, RemoteUpid,
};

use super::{new_remote_upid, raw_client_to_remote_by_id};

pub const ROUTER: Router = Router::new()
    .get(&API_METHOD_LIST_REPLICATION_JOBS)
    .post(&API_METHOD_CREATE_REPLICATION_JOB)
    .match_all("id", &ITEM_ROUTER);

const ITEM_ROUTER: Router = Router::new()
    .get(&list_subdirs_api_method!(ITEM_SUBDIRS))
    .put(&API_METHOD_UPDATE_REPLICATION_JOB)
    .delete(&API_METHOD_DELETE_REPLICATION_JOB)
    .subdirs(ITEM_SUBDIRS);

#[sortable]
const ITEM_SUBDIRS: SubdirMap = &sorted!([
    (
        "config",
        &Router::new().get(&API_METHOD_GET_REPLICATION_JOB)
    ),
    ("run", &Router::new().post(&API_METHOD_RUN_REPLICATION_JOB)),
    (
        "status",
        &Router::new().get(&API_METHOD_GET_REPLICATION_STATUS)
    ),
]);

fn encode_id(id: &str) -> String {
    percent_encoding::percent_encode(id.as_bytes(), percent_encoding::NON_ALPHANUMERIC).to_string()
}

#[api(
    input: { properties: { remote: { schema: REMOTE_ID_SCHEMA } } },
    returns: { type: Array, items: { type: PveReplicationJob } },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}"], PRIV_RESOURCE_AUDIT, false),
    },
)]
/// List native intra-cluster PVE replication jobs.
pub async fn list_replication_jobs(remote: String) -> Result<Vec<PveReplicationJob>, Error> {
    let client = raw_client_to_remote_by_id(&remote)?;
    Ok(client
        .get("/api2/extjs/cluster/replication")
        .await?
        .expect_json()?
        .data)
}

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            id: { type: String },
        },
    },
    returns: { type: PveReplicationJob },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}"], PRIV_RESOURCE_AUDIT, false),
    },
)]
/// Read one native PVE replication job.
pub async fn get_replication_job(remote: String, id: String) -> Result<PveReplicationJob, Error> {
    let client = raw_client_to_remote_by_id(&remote)?;
    let path = format!("/api2/extjs/cluster/replication/{}", encode_id(&id));
    Ok(client.get(&path).await?.expect_json()?.data)
}

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            config: { type: PveReplicationJobConfig, flatten: true },
        },
    },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}"], PRIV_RESOURCE_CREATE, false),
    },
)]
/// Create a native PVE replication job.
pub async fn create_replication_job(
    remote: String,
    config: PveReplicationJobConfig,
) -> Result<(), Error> {
    let client = raw_client_to_remote_by_id(&remote)?;
    client
        .post("/api2/extjs/cluster/replication", &config)
        .await?
        .nodata()?;
    Ok(())
}

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            id: { type: String },
            config: { type: PveReplicationJobConfig, flatten: true },
        },
    },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}"], PRIV_RESOURCE_MANAGE, false),
    },
)]
/// Update a native PVE replication job.
pub async fn update_replication_job(
    remote: String,
    id: String,
    config: PveReplicationJobConfig,
) -> Result<(), Error> {
    let client = raw_client_to_remote_by_id(&remote)?;
    let path = format!("/api2/extjs/cluster/replication/{}", encode_id(&id));
    client.put(&path, &config).await?.nodata()?;
    Ok(())
}

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            id: { type: String },
        },
    },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}"], PRIV_RESOURCE_DELETE, false),
    },
)]
/// Delete a native PVE replication job.
pub async fn delete_replication_job(remote: String, id: String) -> Result<(), Error> {
    let client = raw_client_to_remote_by_id(&remote)?;
    let path = format!("/api2/extjs/cluster/replication/{}", encode_id(&id));
    client.delete(&path).await?.nodata()?;
    Ok(())
}

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            id: { type: String },
            node: { schema: NODE_SCHEMA },
        },
    },
    returns: { type: PveReplicationStatus },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}"], PRIV_RESOURCE_AUDIT, false),
    },
)]
/// Read runtime state for a native PVE replication job.
pub async fn get_replication_status(
    remote: String,
    id: String,
    node: String,
) -> Result<PveReplicationStatus, Error> {
    let client = raw_client_to_remote_by_id(&remote)?;
    let path = format!(
        "/api2/extjs/nodes/{node}/replication/{}/status",
        encode_id(&id)
    );
    Ok(client.get(&path).await?.expect_json()?.data)
}

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            id: { type: String },
            node: { schema: NODE_SCHEMA },
        },
    },
    returns: { type: RemoteUpid },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}"], PRIV_RESOURCE_MANAGE, false),
    },
)]
/// Schedule a native PVE replication job to run immediately.
pub async fn run_replication_job(
    remote: String,
    id: String,
    node: String,
) -> Result<RemoteUpid, Error> {
    let client = raw_client_to_remote_by_id(&remote)?;
    let path = format!(
        "/api2/extjs/nodes/{node}/replication/{}/schedule_now",
        encode_id(&id)
    );
    let upid = client
        .post(&path, &serde_json::json!({}))
        .await?
        .expect_json::<pve_api_types::PveUpid>()?
        .data;
    new_remote_upid(remote, upid).await
}