//! Container image update status.

use anyhow::Error;

use proxmox_router::{Permission, Router};
use proxmox_schema::api;

use pdm_api_types::PRIV_SYS_AUDIT;
use pdm_api_types::update::ImageUpdateStatus;

pub const ROUTER: Router = Router::new().get(&API_METHOD_GET_UPDATE_STATUS);

/// Written by the periodic update check of the container entrypoint.
const STATUS_FILE: &str = concat!(pdm_buildcfg::PDM_RUN_DIR_M!(), "/update-status.json");

fn env_var(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.is_empty())
}

#[api(
    returns: { type: ImageUpdateStatus },
    access: {
        permission: &Permission::Privilege(&["system"], PRIV_SYS_AUDIT, false),
    },
)]
/// Report whether a newer container image was published on the watched tag.
pub fn get_update_status() -> Result<ImageUpdateStatus, Error> {
    let mut status: ImageUpdateStatus = match std::fs::read(STATUS_FILE) {
        Ok(data) => serde_json::from_slice(&data)?,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => ImageUpdateStatus {
            error: Some("the update check did not run yet".to_string()),
            ..Default::default()
        },
        Err(err) => return Err(err.into()),
    };

    if status.running_revision.is_none() {
        status.running_revision = env_var("PDM_IMAGE_REVISION");
    }
    if status.running_created.is_none() {
        status.running_created = env_var("PDM_IMAGE_CREATED");
    }

    Ok(status)
}
