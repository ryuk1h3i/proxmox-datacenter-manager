# Proxmox Datacenter Manager

A stand-alone API + GUI product with the following main features for multiple instances of Proxmox
VE and Proxmox Backup Server in one central place.

## Feature Overview

- Connect & display an arbitrary amount of independent nodes or clusters ("Remotes")
- View the status and load of all resources, which includes nodes, virtual guests, storages,
  datastores and so on. Proxmox Datacenter Manager provides a dashboard that tries to present
  information such that potential problematic outliers can be found easily.
- Customizable dashboards ("views") showing a configurable subset of resources
- Central inventory and management of QEMU virtual machines and LXC containers
  - Resource graphs
  - Create VM and container workflows
  - Start, shutdown, stop, reboot, reset, suspend, resume, clone, template conversion and delete
  - Configuration, snapshots and migration
- URL-only ISO and LXC template catalog
  - PDM stores metadata and checksums, never image payloads
  - PVE downloads media directly from the source URL into the selected storage
- Scheduled PVE backup job management and on-demand vzdump tasks
- Native PVE intra-cluster guest replication management and run-now tasks
- PBS prune, verification and synchronization job management
- PBS datastore prune preview/execution and garbage collection
- Remote shell for Proxmox VE and Proxmox Backup Server remotes
- Global overview over available system updates for managed remotes
- Firewall overview for all managed remotes
- Basic SDN overview for all managed remotes
- Remote migration of virtual guests between different datacenters
  Advertising use of ZFS & Ceph backed replication for quicker transfer on actual migration
- View configuration health state (subscription, APT repositories, pending updates, ...)
- User management / access control
  - Users/API token
  - Support for LDAP and Active Directory
  - Support for OpenID Connect
  - Support for complex Two-Factor Authentication
- ACME/Let's Encrypt

- A non-exhaustive list of features planned for future releases is:
  - Management of more configuration (e.g. notification policies and package repositories)
  - Integration of other projects, like Proxmox Mail Gateway, and potentially also Proxmox Offline Mirror.
  - ... to be determined from user feedback and feature requests.

## Deployment and scope

This variant is designed to run as a containerized control plane. PVE and PBS remain authoritative
for guest data, schedules and task execution. PDM does not administer its Docker host and does not
provide local PAM, package, subscription, shell or host-metrics features.

High availability and automatic failover are intentionally out of scope. Replication support is
limited to the native PVE intra-cluster guest replication API; PBS synchronization remains a
separate backup-server workflow. Media is referenced by URL and transferred directly by PVE, so
the PDM container does not require an object store or persistent image repository.

## Container image

The ready-to-run image is published to GitHub Container Registry:

```text
ghcr.io/ryuk1h3i/proxmox-datacenter-manager
```

The [container workflow](.github/workflows/container-image.yml) runs for every pushed commit on
every branch. It also validates pull requests without publishing an image and can be started
manually from the Actions page. Images are currently built for `linux/amd64`.

Published tags are:

- `latest`: the latest commit on the repository's default branch.
- `<branch>`: the latest commit on that branch, with Docker-safe normalization.
- `sha-<short-sha>`: an immutable tag identifying the exact commit.
- `vX.Y.Z`, `X.Y.Z`, and `X.Y`: tags generated for a pushed semantic version tag such as `v1.2.3`.

The workflow authenticates with the automatically provided `GITHUB_TOKEN`; no registry password
or personal access token needs to be stored as a repository secret. The workflow requires the
standard `packages: write` permission declared in the workflow. If the package is private, clients
must authenticate to `ghcr.io` with a GitHub token that has `read:packages` permission before
pulling it.

### Run with Docker Compose

Copy `.env.example` to `.env` and set the initial password for `admin@pdm`:

```sh
cp .env.example .env
$EDITOR .env
chmod 600 .env
docker compose pull
docker compose up -d
```

The service is available at `https://localhost:8443`. Compose passes `PDM_ADMIN_PASSWORD` to the
container as the Docker secret `/run/secrets/pdm-admin-password`, which is read only to bootstrap
`admin@pdm`; change the password from the web interface afterwards. Configuration, state, cache and
task logs are kept in named Docker volumes.

Defining the secret from an environment variable requires Docker Compose v2.24 or later. The value
is stored in clear text in `.env` and is visible through `docker compose config`, so restrict access
to that file.

To deploy a specific immutable build, set `PDM_IMAGE` in `.env`:

```text
PDM_IMAGE=ghcr.io/ryuk1h3i/proxmox-datacenter-manager:sha-0123456
```

To build the current checkout locally instead of pulling GHCR, run `docker compose build` followed
by `docker compose up -d`. The same multi-stage [Dockerfile](Dockerfile) is used locally and by
GitHub Actions.

### Publishing behavior

Pushes to branches and version tags build and publish the image. Pull requests execute the complete
container build but set `push: false`, preventing unmerged code from being uploaded to GHCR.
BuildKit's GitHub Actions cache is reused between runs, and concurrent runs for the same Git ref are
cancelled when a newer commit arrives.

## Technology Overview

### Backend
- Implemented in the Rust programming language, reusing code from Proxmox Backup Server where possible
- A for Proxmox projects standard dual-stack of API daemons. One as main API daemon running as
  unprivileged users and one privileged daemon running as root. Contrary to other projects the
  privileged daemon exclusively listens on a file based UNIX socket, thus restricting attack surface
  even further.
- The backend listens on port 8443 (TLS only)
- The code for the backend server is located in the `server/` directory.

### Frontend

- The Web UI communicates with the backend server via a JSON-based REST API.
- The UI is implemented in Rust, using [Yew](https://yew.rs/) and the 
  [proxmox-yew-widget-toolkit](https://git.proxmox.com/?p=ui/proxmox-yew-widget-toolkit.git;a=summary).
  The Rust code is compiled to WebAssembly.
- The code for the UI is located in the `ui/` directory.

### CLI tools

There are two CLI tools to manage Proxmox Datacenter Manager.
- `proxmox-datacenter-manager-client`: client using the PDM API, can be used to
  control local or remote PDM instances
- `proxmox-datacenter-manager-admin`: root-only, local administration tool

Their implementation can be found in `cli/admin` and `cli/client`, respectively.


## Documentation

Documentation (user-facing as well as developer-facing) can be found in `docs/`.
