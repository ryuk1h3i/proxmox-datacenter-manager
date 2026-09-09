use anyhow::{Error, bail};
use serde::{Deserialize, Serialize};

use proxmox_config_digest::ConfigDigest;
use proxmox_router::{Permission, Router, RpcEnvironment, http_bail, http_err, param_bail};
use proxmox_schema::api;

use pdm_api_types::media::{
    MediaCatalogConfigEntry, MediaCatalogEntry, MediaCatalogEntryUpdater,
};
use pdm_api_types::{PRIV_RESOURCE_AUDIT, PRIV_RESOURCE_MODIFY, PROXMOX_SAFE_ID_SCHEMA};

const MEDIA_ROUTER: Router = Router::new()
    .get(&API_METHOD_READ_MEDIA)
    .put(&API_METHOD_UPDATE_MEDIA)
    .delete(&API_METHOD_REMOVE_MEDIA);

pub const ROUTER: Router = Router::new()
    .get(&API_METHOD_LIST_MEDIA)
    .post(&API_METHOD_ADD_MEDIA)
    .match_all("id", &MEDIA_ROUTER);

fn validate_checksum(checksum: Option<&str>, algorithm: Option<&str>) -> Result<(), Error> {
    match (checksum, algorithm) {
        (None, None) => Ok(()),
        (Some(_), Some("md5" | "sha1" | "sha224" | "sha256" | "sha384" | "sha512")) => {
            Ok(())
        }
        (Some(_), Some(algorithm)) => bail!("unsupported checksum algorithm '{algorithm}'"),
        _ => bail!("checksum and checksum-algorithm must be specified together"),
    }
}

#[api(
    returns: {
        type: Array,
        items: { type: MediaCatalogEntry },
    },
    access: {
        permission: &Permission::Privilege(&["resource"], PRIV_RESOURCE_AUDIT, false),
    },
)]
/// List URL-only media catalog entries.
pub fn list_media(rpcenv: &mut dyn RpcEnvironment) -> Result<Vec<MediaCatalogEntry>, Error> {
    let (config, digest) = pdm_config::media::config()?;
    rpcenv["digest"] = digest.to_hex().into();

    Ok(config
        .into_iter()
        .map(|(_, entry)| match entry {
            MediaCatalogConfigEntry::Media(entry) => entry,
        })
        .collect())
}

#[api(
    input: {
        properties: {
            entry: { type: MediaCatalogEntry, flatten: true },
            digest: { type: ConfigDigest, optional: true },
        },
    },
    access: {
        permission: &Permission::Privilege(&["resource"], PRIV_RESOURCE_MODIFY, false),
    },
)]
/// Add a URL-only media catalog entry.
pub fn add_media(entry: MediaCatalogEntry, digest: Option<ConfigDigest>) -> Result<(), Error> {
    validate_checksum(
        entry.checksum.as_deref(),
        entry.checksum_algorithm.as_deref(),
    )
    .map_err(|err| proxmox_router::ParameterError::from(("checksum", err)))?;

    let _lock = pdm_config::media::lock_config()?;
    let (mut config, config_digest) = pdm_config::media::config()?;
    config_digest.detect_modification(digest.as_ref())?;

    let id = entry.id.clone();
    if config
        .insert(id.clone(), MediaCatalogConfigEntry::Media(entry))
        .is_some()
    {
        param_bail!("id", "media entry '{id}' already exists");
    }

    pdm_config::media::save_config(&config)
}

#[api]
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
/// Optional media properties which can be cleared during an update.
pub enum DeletableMediaProperty {
    Version,
    Architecture,
    Checksum,
    ChecksumAlgorithm,
    Size,
}

#[api(
    input: {
        properties: {
            id: { schema: PROXMOX_SAFE_ID_SCHEMA },
            entry: { type: MediaCatalogEntryUpdater, flatten: true },
            delete: {
                type: Array,
                optional: true,
                items: { type: DeletableMediaProperty },
            },
            digest: { type: ConfigDigest, optional: true },
        },
    },
    access: {
        permission: &Permission::Privilege(&["resource"], PRIV_RESOURCE_MODIFY, false),
    },
)]
/// Update a URL-only media catalog entry.
pub fn update_media(
    id: String,
    entry: MediaCatalogEntryUpdater,
    delete: Option<Vec<DeletableMediaProperty>>,
    digest: Option<ConfigDigest>,
) -> Result<(), Error> {
    let _lock = pdm_config::media::lock_config()?;
    let (mut config, config_digest) = pdm_config::media::config()?;
    config_digest.detect_modification(digest.as_ref())?;

    let MediaCatalogConfigEntry::Media(current) = config
        .get_mut(&id)
        .ok_or_else(|| http_err!(NOT_FOUND, "no such media entry '{id}'"))?;

    if let Some(delete) = delete {
        for property in delete {
            match property {
                DeletableMediaProperty::Version => current.version = None,
                DeletableMediaProperty::Architecture => current.architecture = None,
                DeletableMediaProperty::Checksum => current.checksum = None,
                DeletableMediaProperty::ChecksumAlgorithm => current.checksum_algorithm = None,
                DeletableMediaProperty::Size => current.size = None,
            }
        }
    }

    if let Some(name) = entry.name {
        current.name = name;
    }
    if let Some(url) = entry.url {
        current.url = url;
    }
    if let Some(content) = entry.content {
        current.content = content;
    }
    if let Some(filename) = entry.filename {
        current.filename = filename;
    }
    if entry.version.is_some() {
        current.version = entry.version;
    }
    if entry.architecture.is_some() {
        current.architecture = entry.architecture;
    }
    if entry.checksum.is_some() {
        current.checksum = entry.checksum;
    }
    if entry.checksum_algorithm.is_some() {
        current.checksum_algorithm = entry.checksum_algorithm;
    }
    if entry.size.is_some() {
        current.size = entry.size;
    }

    validate_checksum(
        current.checksum.as_deref(),
        current.checksum_algorithm.as_deref(),
    )
    .map_err(|err| proxmox_router::ParameterError::from(("checksum", err)))?;

    pdm_config::media::save_config(&config)
}

#[api(
    input: {
        properties: {
            id: { schema: PROXMOX_SAFE_ID_SCHEMA },
            digest: { type: ConfigDigest, optional: true },
        },
    },
    access: {
        permission: &Permission::Privilege(&["resource"], PRIV_RESOURCE_MODIFY, false),
    },
)]
/// Remove a media catalog entry.
pub fn remove_media(id: String, digest: Option<ConfigDigest>) -> Result<(), Error> {
    let _lock = pdm_config::media::lock_config()?;
    let (mut config, config_digest) = pdm_config::media::config()?;
    config_digest.detect_modification(digest.as_ref())?;

    if config.remove(&id).is_none() {
        http_bail!(NOT_FOUND, "media entry '{id}' does not exist");
    }

    pdm_config::media::save_config(&config)
}

#[api(
    input: {
        properties: {
            id: { schema: PROXMOX_SAFE_ID_SCHEMA },
        },
    },
    returns: { type: MediaCatalogEntry },
    access: {
        permission: &Permission::Privilege(&["resource"], PRIV_RESOURCE_AUDIT, false),
    },
)]
/// Read a media catalog entry.
pub fn read_media(id: String, rpcenv: &mut dyn RpcEnvironment) -> Result<MediaCatalogEntry, Error> {
    let (config, digest) = pdm_config::media::config()?;
    rpcenv["digest"] = digest.to_hex().into();

    match config
        .get(&id)
        .ok_or_else(|| http_err!(NOT_FOUND, "no such media entry '{id}'"))?
    {
        MediaCatalogConfigEntry::Media(entry) => Ok(entry.clone()),
    }
}