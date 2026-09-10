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

Updates are deployed by rebuilding or pulling an image and recreating the container. Package,
network, DNS, time, power, shell, journal, system report, local host metrics, and local subscription
management are intentionally unavailable. ACME renewal is handled by the API daemon's internal
daily scheduler.
