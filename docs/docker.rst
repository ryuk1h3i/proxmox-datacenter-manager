Docker Deployment
=================

This variant runs Proxmox Datacenter Manager as an application container. It manages remote Proxmox
VE and Proxmox Backup Server systems, but does not administer the operating system hosting the
container.

Set the initial administrator password in ``PDM_ADMIN_PASSWORD`` before invoking Docker Compose,
for example by copying ``.env.example`` to ``.env``. Compose exposes the value to the container as
the secret ``/run/secrets/pdm-admin-password``, which is read during the first setup. Existing
password data is never replaced on subsequent starts.

Build and start the service with Docker Compose. The web interface is available over HTTPS on port
8443. The initial account is ``admin@pdm``.

The Compose definition persists configuration, application state, cache, and task logs in named
volumes. Runtime socket and PID data use an in-memory filesystem. Application logs are written to
standard error and can be consumed by the Docker logging driver.

Package, network, DNS, time, power, shell, journal, system report, local host metrics, and local
subscription management are intentionally unavailable. ACME renewal is handled by the API daemon's
internal daily scheduler.

Updates
-------

A container is not updated in place: ``apt`` inside the container is not a supported path, because
every change is discarded when the container is recreated. Updates replace the image instead. All
state lives in the named volumes and survives the replacement.

:Deploy an update: ``docker compose pull`` followed by ``docker compose up -d``. Back up the
   ``pdm-config`` volume first, see below.
:Pin a version: set ``PDM_IMAGE`` to a ``vX.Y.Z`` or ``sha-<short-sha>`` tag in production and keep
   ``latest`` for test deployments. The ``sha-`` tags always refer to the exact build they name.
:Roll back: point ``PDM_IMAGE`` at the previous tag and recreate the container. This is only safe as
   long as the newer release did not migrate the configuration, so restore the ``pdm-config`` volume
   from the backup taken before the upgrade if the older image refuses to start.
:Base system fixes: a scheduled workflow rebuilds and republishes the image every week with the
   current Debian security updates, so pulling regularly is what replaces ``apt upgrade``.
:Application releases: the packages inside the image are built from this repository, so an upstream
   Proxmox release reaches the image by merging it into this fork and letting the build pipeline
   publish a new image.

Update Notification
-------------------

Once a day the container asks the registry whether the watched tag carries an image newer than the
running one and stores the answer in ``/run/proxmox-datacenter-manager/update-status.json``. The web
interface shows an indicator next to the product name when an update is available, and
``/update-status`` exposes the same information to users with ``Sys.Audit`` privileges. The check is
read-only: the container never updates itself and has no access to the Docker socket.

``PDM_UPDATE_CHANNEL`` selects the tag to watch and should match the tag ``PDM_IMAGE`` is pinned to.
``PDM_UPDATE_CHECK=off`` disables the check entirely, which also removes the only outgoing
connection the container makes to the registry.

