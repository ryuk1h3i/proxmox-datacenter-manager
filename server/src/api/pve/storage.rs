use anyhow::Error;

use proxmox_client::HttpApiClient;
use proxmox_router::{Permission, Router, SubdirMap, list_subdirs_api_method};
use proxmox_schema::api;
use proxmox_sortable_macro::sortable;

use pdm_api_types::media::{MediaContentType, PveDownloadUrl, PveStorageContent};
use pdm_api_types::remotes::REMOTE_ID_SCHEMA;
use pdm_api_types::{
    NODE_SCHEMA, PRIV_RESOURCE_AUDIT, PRIV_RESOURCE_MANAGE, PVE_STORAGE_ID_SCHEMA, RemoteUpid,
};

use super::{connect_to_remote_by_id, get_remote, new_remote_upid};

pub const ROUTER: Router = Router::new()
    .get(&list_subdirs_api_method!(STORAGE_SUBDIR))
    .subdirs(STORAGE_SUBDIR);

#[sortable]
const STORAGE_SUBDIR: SubdirMap = &sorted!([
    (
        "content",
        &Router::new().get(&API_METHOD_LIST_CONTENT)
    ),
    (
        "download-url",
        &Router::new().post(&API_METHOD_DOWNLOAD_URL)
    ),
    ("rrddata", &super::rrddata::STORAGE_RRD_ROUTER),
    ("status", &Router::new().get(&API_METHOD_GET_STATUS)),
]);

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            node: { schema: NODE_SCHEMA },
            storage: { schema: PVE_STORAGE_ID_SCHEMA },
            content: { type: MediaContentType },
        },
    },
    returns: {
        type: Array,
        items: { type: PveStorageContent },
    },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}", "storage", "{storage}"], PRIV_RESOURCE_AUDIT, false),
    },
)]
/// List ISO images or LXC templates available on a PVE storage.
pub async fn list_content(
    remote: String,
    node: String,
    storage: String,
    content: MediaContentType,
) -> Result<Vec<PveStorageContent>, Error> {
    let (remotes, _) = pdm_config::remotes::config()?;
    let remote_config = get_remote(&remotes, &remote)?;
    let client = crate::connection::make_raw_client(remote_config)?;
    let path = format!(
        "/api2/extjs/nodes/{node}/storage/{storage}/content?content={content}"
    );

    Ok(client.get(&path).await?.expect_json()?.data)
}

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            node: { schema: NODE_SCHEMA },
            storage: { schema: PVE_STORAGE_ID_SCHEMA },
            download: { type: PveDownloadUrl, flatten: true },
        },
    },
    returns: { type: RemoteUpid },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}", "storage", "{storage}"], PRIV_RESOURCE_MANAGE, false),
    },
)]
/// Ask PVE to download media directly from an external URL into storage.
pub async fn download_url(
    remote: String,
    node: String,
    storage: String,
    download: PveDownloadUrl,
) -> Result<RemoteUpid, Error> {
    let (remotes, _) = pdm_config::remotes::config()?;
    let remote_config = get_remote(&remotes, &remote)?;
    let client = crate::connection::make_raw_client(remote_config)?;
    let path = format!("/api2/extjs/nodes/{node}/storage/{storage}/download-url");
    let upid = client
        .post(&path, &download)
        .await?
        .expect_json::<pve_api_types::PveUpid>()?
        .data;

    new_remote_upid(remote, upid).await
}

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            node: { schema: NODE_SCHEMA, },
            storage: { schema: PVE_STORAGE_ID_SCHEMA, },
        },
    },
    returns: { type: pve_api_types::StorageStatus },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}", "storage", "{storage}"], PRIV_RESOURCE_AUDIT, false),
    },
)]
/// Get the status of a qemu VM from a remote. If a node is provided, the VM must be on that
/// node, otherwise the node is determined automatically.
pub async fn get_status(
    remote: String,
    node: String,
    storage: String,
) -> Result<pve_api_types::StorageStatus, Error> {
    let pve = connect_to_remote_by_id(&remote)?;

    Ok(pve.storage_status(&node, &storage).await?)
}
