use anyhow::{Error, bail};

use proxmox_client::HttpApiClient;
use proxmox_router::{Permission, Router};
use proxmox_schema::api;

use pdm_api_types::pve_jobs::{PveBackupJob, PveBackupJobConfig, PveVzdumpRequest};
use pdm_api_types::remotes::REMOTE_ID_SCHEMA;
use pdm_api_types::{
    PRIV_RESOURCE_AUDIT, PRIV_RESOURCE_CREATE, PRIV_RESOURCE_DELETE, PRIV_RESOURCE_MANAGE,
    RemoteUpid,
};

use super::{new_remote_upid, raw_client_to_remote_by_id};

pub const ROUTER: Router = Router::new()
    .get(&API_METHOD_LIST_BACKUP_JOBS)
    .post(&API_METHOD_CREATE_BACKUP_JOB)
    .match_all("id", &ITEM_ROUTER);

const ITEM_ROUTER: Router = Router::new()
    .get(&API_METHOD_GET_BACKUP_JOB)
    .put(&API_METHOD_UPDATE_BACKUP_JOB)
    .delete(&API_METHOD_DELETE_BACKUP_JOB);

pub const VZDUMP_ROUTER: Router = Router::new().post(&API_METHOD_RUN_VZDUMP);

fn encode_id(id: &str) -> String {
    percent_encoding::percent_encode(id.as_bytes(), percent_encoding::NON_ALPHANUMERIC).to_string()
}

#[api(
    input: { properties: { remote: { schema: REMOTE_ID_SCHEMA } } },
    returns: { type: Array, items: { type: PveBackupJob } },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}"], PRIV_RESOURCE_AUDIT, false),
    },
)]
/// List scheduled backup jobs on a PVE remote.
pub async fn list_backup_jobs(remote: String) -> Result<Vec<PveBackupJob>, Error> {
    let client = raw_client_to_remote_by_id(&remote)?;
    Ok(client
        .get("/api2/extjs/cluster/backup")
        .await?
        .expect_json()?
        .data)
}

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            config: { type: PveBackupJobConfig, flatten: true },
        },
    },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}"], PRIV_RESOURCE_CREATE, false),
    },
)]
/// Create a scheduled backup job on a PVE remote.
pub async fn create_backup_job(
    remote: String,
    config: PveBackupJobConfig,
) -> Result<(), Error> {
    let client = raw_client_to_remote_by_id(&remote)?;
    client
        .post("/api2/extjs/cluster/backup", &config)
        .await?
        .nodata()?;
    Ok(())
}

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            id: { type: String },
        },
    },
    returns: { type: PveBackupJob },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}"], PRIV_RESOURCE_AUDIT, false),
    },
)]
/// Read one scheduled backup job.
pub async fn get_backup_job(remote: String, id: String) -> Result<PveBackupJob, Error> {
    let client = raw_client_to_remote_by_id(&remote)?;
    let path = format!("/api2/extjs/cluster/backup/{}", encode_id(&id));
    Ok(client.get(&path).await?.expect_json()?.data)
}

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            id: { type: String },
            config: { type: PveBackupJobConfig, flatten: true },
        },
    },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}"], PRIV_RESOURCE_MANAGE, false),
    },
)]
/// Update one scheduled backup job.
pub async fn update_backup_job(
    remote: String,
    id: String,
    mut config: PveBackupJobConfig,
) -> Result<(), Error> {
    config.id = None;
    let client = raw_client_to_remote_by_id(&remote)?;
    let path = format!("/api2/extjs/cluster/backup/{}", encode_id(&id));
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
/// Delete one scheduled backup job.
pub async fn delete_backup_job(remote: String, id: String) -> Result<(), Error> {
    let client = raw_client_to_remote_by_id(&remote)?;
    let path = format!("/api2/extjs/cluster/backup/{}", encode_id(&id));
    client.delete(&path).await?.nodata()?;
    Ok(())
}

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            request: { type: PveVzdumpRequest, flatten: true },
        },
    },
    returns: { type: RemoteUpid },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}"], PRIV_RESOURCE_MANAGE, false),
    },
)]
/// Start an immediate vzdump task on a PVE node.
pub async fn run_vzdump(
    remote: String,
    request: PveVzdumpRequest,
) -> Result<RemoteUpid, Error> {
    if request.vmid.is_none() && request.pool.is_none() && request.all != Some(true) {
        bail!("one of vmid, pool, or all must select backup guests");
    }
    let node = request.node.clone();
    let mut payload = serde_json::to_value(request)?;
    payload.as_object_mut().expect("request serializes as object").remove("node");
    let client = raw_client_to_remote_by_id(&remote)?;
    let path = format!("/api2/extjs/nodes/{node}/vzdump");
    let upid = client
        .post(&path, &payload)
        .await?
        .expect_json::<pve_api_types::PveUpid>()?
        .data;
    new_remote_upid(remote, upid).await
}