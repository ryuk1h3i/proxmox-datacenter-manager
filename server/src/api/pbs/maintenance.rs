use anyhow::Error;

use proxmox_router::{Permission, Router, SubdirMap, list_subdirs_api_method};
use proxmox_schema::api;
use proxmox_sortable_macro::sortable;

use pdm_api_types::pbs_jobs::{
    PbsGcStatus, PbsPruneJob, PbsPruneRequest, PbsPruneResult, PbsSnapshotNotes,
    PbsSnapshotProtection, PbsSnapshotRef, PbsSyncJob, PbsVerifyJob,
};
use pdm_api_types::remotes::REMOTE_ID_SCHEMA;
use pdm_api_types::{PRIV_RESOURCE_AUDIT, PRIV_RESOURCE_MANAGE, RemoteUpid};

use crate::pbs_client;

use super::new_remote_upid;

pub const PRUNE_JOBS_ROUTER: Router = Router::new()
    .get(&API_METHOD_LIST_PRUNE_JOBS)
    .post(&API_METHOD_CREATE_PRUNE_JOB)
    .match_all("id", &PRUNE_JOB_ROUTER);
pub const VERIFY_JOBS_ROUTER: Router = Router::new()
    .get(&API_METHOD_LIST_VERIFY_JOBS)
    .post(&API_METHOD_CREATE_VERIFY_JOB)
    .match_all("id", &VERIFY_JOB_ROUTER);
pub const SYNC_JOBS_ROUTER: Router = Router::new()
    .get(&API_METHOD_LIST_SYNC_JOBS)
    .post(&API_METHOD_CREATE_SYNC_JOB)
    .match_all("id", &SYNC_JOB_ROUTER);

const PRUNE_JOB_ROUTER: Router = Router::new()
    .put(&API_METHOD_UPDATE_PRUNE_JOB)
    .delete(&API_METHOD_DELETE_PRUNE_JOB)
    .subdirs(&PRUNE_JOB_SUBDIRS);
const VERIFY_JOB_ROUTER: Router = Router::new()
    .put(&API_METHOD_UPDATE_VERIFY_JOB)
    .delete(&API_METHOD_DELETE_VERIFY_JOB)
    .subdirs(&VERIFY_JOB_SUBDIRS);
const SYNC_JOB_ROUTER: Router = Router::new()
    .put(&API_METHOD_UPDATE_SYNC_JOB)
    .delete(&API_METHOD_DELETE_SYNC_JOB)
    .subdirs(&SYNC_JOB_SUBDIRS);

#[sortable]
const PRUNE_JOB_SUBDIRS: SubdirMap = &sorted!([
    ("config", &Router::new().get(&API_METHOD_GET_PRUNE_JOB)),
    ("run", &Router::new().post(&API_METHOD_RUN_PRUNE_JOB)),
]);
#[sortable]
const VERIFY_JOB_SUBDIRS: SubdirMap = &sorted!([
    ("config", &Router::new().get(&API_METHOD_GET_VERIFY_JOB)),
    ("run", &Router::new().post(&API_METHOD_RUN_VERIFY_JOB)),
]);
#[sortable]
const SYNC_JOB_SUBDIRS: SubdirMap = &sorted!([
    ("config", &Router::new().get(&API_METHOD_GET_SYNC_JOB)),
    ("run", &Router::new().post(&API_METHOD_RUN_SYNC_JOB)),
]);

macro_rules! job_get {
    ($name:ident, $ty:ty, $method:ident) => {
        #[api(
            input: { properties: { remote: { schema: REMOTE_ID_SCHEMA }, id: { type: String } } },
            returns: { type: $ty },
            access: { permission: &Permission::Privilege(&["resource", "{remote}"], PRIV_RESOURCE_AUDIT, false) },
        )]
        pub async fn $name(remote: String, id: String) -> Result<$ty, Error> {
            Ok(pbs_client::connect_to_remote_by_id(&remote)?.$method(&id).await?)
        }
    };
}

job_get!(get_prune_job, PbsPruneJob, get_prune_job);
job_get!(get_verify_job, PbsVerifyJob, get_verify_job);
job_get!(get_sync_job, PbsSyncJob, get_sync_job);

pub const DATASTORE_MAINTENANCE_ROUTER: Router = Router::new()
    .get(&list_subdirs_api_method!(DATASTORE_MAINTENANCE_SUBDIRS))
    .subdirs(DATASTORE_MAINTENANCE_SUBDIRS);

pub const SNAPSHOT_ACTIONS_ROUTER: Router = Router::new()
    .get(&list_subdirs_api_method!(SNAPSHOT_ACTION_SUBDIRS))
    .subdirs(SNAPSHOT_ACTION_SUBDIRS);

#[sortable]
const SNAPSHOT_ACTION_SUBDIRS: SubdirMap = &sorted!([
    ("forget", &Router::new().delete(&API_METHOD_FORGET_SNAPSHOT)),
    ("notes", &Router::new().put(&API_METHOD_SET_SNAPSHOT_NOTES)),
    (
        "protected",
        &Router::new().put(&API_METHOD_SET_SNAPSHOT_PROTECTION)
    ),
    ("verify", &Router::new().post(&API_METHOD_VERIFY_SNAPSHOT)),
]);

#[sortable]
const DATASTORE_MAINTENANCE_SUBDIRS: SubdirMap = &sorted!([
    (
        "gc",
        &Router::new()
            .get(&API_METHOD_GET_GC_STATUS)
            .post(&API_METHOD_RUN_GC)
    ),
    ("prune", &Router::new().post(&API_METHOD_PRUNE_DATASTORE)),
]);

macro_rules! job_crud {
    ($list:ident, $create:ident, $update:ident, $delete:ident, $run:ident, $ty:ty,
     $list_method:ident, $create_method:ident, $update_method:ident, $delete_method:ident, $run_method:ident) => {
        #[api(
            input: { properties: { remote: { schema: REMOTE_ID_SCHEMA } } },
            returns: { type: Array, items: { type: $ty } },
            access: { permission: &Permission::Privilege(&["resource", "{remote}"], PRIV_RESOURCE_AUDIT, false) },
        )]
        pub async fn $list(remote: String) -> Result<Vec<$ty>, Error> {
            pbs_client::connect_to_remote_by_id(&remote)?.$list_method().await.map_err(Into::into)
        }

        #[api(
            input: { properties: { remote: { schema: REMOTE_ID_SCHEMA }, job: { type: $ty, flatten: true } } },
            access: { permission: &Permission::Privilege(&["resource", "{remote}"], PRIV_RESOURCE_MANAGE, false) },
        )]
        pub async fn $create(remote: String, job: $ty) -> Result<(), Error> {
            pbs_client::connect_to_remote_by_id(&remote)?.$create_method(&job).await.map_err(Into::into)
        }

        #[api(
            input: { properties: { remote: { schema: REMOTE_ID_SCHEMA }, id: { type: String }, job: { type: $ty, flatten: true } } },
            access: { permission: &Permission::Privilege(&["resource", "{remote}"], PRIV_RESOURCE_MANAGE, false) },
        )]
        pub async fn $update(remote: String, id: String, job: $ty) -> Result<(), Error> {
            pbs_client::connect_to_remote_by_id(&remote)?.$update_method(&id, &job).await.map_err(Into::into)
        }

        #[api(
            input: { properties: { remote: { schema: REMOTE_ID_SCHEMA }, id: { type: String } } },
            access: { permission: &Permission::Privilege(&["resource", "{remote}"], PRIV_RESOURCE_MANAGE, false) },
        )]
        pub async fn $delete(remote: String, id: String) -> Result<(), Error> {
            pbs_client::connect_to_remote_by_id(&remote)?.$delete_method(&id).await.map_err(Into::into)
        }

        #[api(
            input: { properties: { remote: { schema: REMOTE_ID_SCHEMA }, id: { type: String } } },
            returns: { type: RemoteUpid },
            access: { permission: &Permission::Privilege(&["resource", "{remote}"], PRIV_RESOURCE_MANAGE, false) },
        )]
        pub async fn $run(remote: String, id: String) -> Result<RemoteUpid, Error> {
            let upid = pbs_client::connect_to_remote_by_id(&remote)?.$run_method(&id).await?;
            new_remote_upid(remote, upid).await
        }
    };
}

job_crud!(
    list_prune_jobs, create_prune_job, update_prune_job, delete_prune_job, run_prune_job,
    PbsPruneJob, list_prune_jobs, create_prune_job, update_prune_job, delete_prune_job,
    run_prune_job
);
job_crud!(
    list_verify_jobs, create_verify_job, update_verify_job, delete_verify_job, run_verify_job,
    PbsVerifyJob, list_verify_jobs, create_verify_job, update_verify_job, delete_verify_job,
    run_verify_job
);
job_crud!(
    list_sync_jobs, create_sync_job, update_sync_job, delete_sync_job, run_sync_job, PbsSyncJob,
    list_sync_jobs, create_sync_job, update_sync_job, delete_sync_job, run_sync_job
);

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            datastore: { schema: pbs_api_types::DATASTORE_SCHEMA },
        },
    },
    returns: { type: PbsGcStatus },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}", "datastore", "{datastore}"], PRIV_RESOURCE_AUDIT, false),
    },
)]
pub async fn get_gc_status(remote: String, datastore: String) -> Result<PbsGcStatus, Error> {
    Ok(pbs_client::connect_to_remote_by_id(&remote)?
        .datastore_gc_status(&datastore)
        .await?)
}

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            datastore: { schema: pbs_api_types::DATASTORE_SCHEMA },
        },
    },
    returns: { type: RemoteUpid },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}", "datastore", "{datastore}"], PRIV_RESOURCE_MANAGE, false),
    },
)]
pub async fn run_gc(remote: String, datastore: String) -> Result<RemoteUpid, Error> {
    let upid = pbs_client::connect_to_remote_by_id(&remote)?
        .run_datastore_gc(&datastore)
        .await?;
    new_remote_upid(remote, upid).await
}

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            datastore: { schema: pbs_api_types::DATASTORE_SCHEMA },
            request: { type: PbsPruneRequest, flatten: true },
        },
    },
    returns: { type: Array, items: { type: PbsPruneResult } },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}", "datastore", "{datastore}"], PRIV_RESOURCE_MANAGE, false),
    },
)]
pub async fn prune_datastore(
    remote: String,
    datastore: String,
    request: PbsPruneRequest,
) -> Result<Vec<PbsPruneResult>, Error> {
    Ok(pbs_client::connect_to_remote_by_id(&remote)?
        .prune_datastore(&datastore, &request)
        .await?)
}

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            datastore: { schema: pbs_api_types::DATASTORE_SCHEMA },
            request: { type: PbsSnapshotProtection, flatten: true },
        },
    },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}", "datastore", "{datastore}"], PRIV_RESOURCE_MANAGE, false),
    },
)]
pub async fn set_snapshot_protection(
    remote: String,
    datastore: String,
    request: PbsSnapshotProtection,
) -> Result<(), Error> {
    pbs_client::connect_to_remote_by_id(&remote)?
        .set_snapshot_protection(&datastore, &request)
        .await?;
    Ok(())
}

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            datastore: { schema: pbs_api_types::DATASTORE_SCHEMA },
            request: { type: PbsSnapshotNotes, flatten: true },
        },
    },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}", "datastore", "{datastore}"], PRIV_RESOURCE_MANAGE, false),
    },
)]
pub async fn set_snapshot_notes(
    remote: String,
    datastore: String,
    request: PbsSnapshotNotes,
) -> Result<(), Error> {
    pbs_client::connect_to_remote_by_id(&remote)?
        .set_snapshot_notes(&datastore, &request)
        .await?;
    Ok(())
}

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            datastore: { schema: pbs_api_types::DATASTORE_SCHEMA },
            snapshot: { type: PbsSnapshotRef, flatten: true },
        },
    },
    returns: { type: RemoteUpid },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}", "datastore", "{datastore}"], PRIV_RESOURCE_MANAGE, false),
    },
)]
pub async fn verify_snapshot(
    remote: String,
    datastore: String,
    snapshot: PbsSnapshotRef,
) -> Result<RemoteUpid, Error> {
    let upid = pbs_client::connect_to_remote_by_id(&remote)?
        .verify_snapshot(&datastore, &snapshot)
        .await?;
    new_remote_upid(remote, upid).await
}

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            datastore: { schema: pbs_api_types::DATASTORE_SCHEMA },
            snapshot: { type: PbsSnapshotRef, flatten: true },
        },
    },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}", "datastore", "{datastore}"], PRIV_RESOURCE_MANAGE, false),
    },
)]
pub async fn forget_snapshot(
    remote: String,
    datastore: String,
    snapshot: PbsSnapshotRef,
) -> Result<(), Error> {
    pbs_client::connect_to_remote_by_id(&remote)?
        .forget_snapshot(&datastore, &snapshot)
        .await?;
    Ok(())
}