FROM debian:trixie AS base

ARG DEBIAN_FRONTEND=noninteractive

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates \
        devscripts \
        equivs \
        lintian \
        make \
        patchelf \
        wget \
    && wget -qO /usr/share/keyrings/proxmox-archive-keyring.gpg \
        https://enterprise.proxmox.com/debian/proxmox-release-trixie.gpg \
    && printf '%s\n' \
        'Types: deb' \
        'URIs: http://download.proxmox.com/debian/pdm' \
        'Suites: trixie' \
        'Components: pdm-no-subscription' \
        'Signed-By: /usr/share/keyrings/proxmox-archive-keyring.gpg' \
        > /etc/apt/sources.list.d/proxmox.sources \
    && printf '%s\n' \
        'Types: deb' \
        'URIs: http://download.proxmox.com/debian/devel' \
        'Suites: trixie' \
        'Components: main' \
        'Signed-By: /usr/share/keyrings/proxmox-archive-keyring.gpg' \
        > /etc/apt/sources.list.d/proxmox-devel.sources \
    && apt-get update

# Skip the test suite and the debug symbol packages, which the runtime image
# never installs.
ENV DEB_BUILD_OPTIONS="nocheck noautodbgsym"

WORKDIR /source

# The build dependencies are derived from the control files alone, so resolving
# them in their own layer keeps that (slow) apt step cached when only sources
# change. Both sets go in together: the UI package also builds the shared lib/
# crates, whose dependencies are declared in the main control file only.
FROM base AS deps
COPY debian/control debian/control
COPY ui/debian/control ui/debian/control
RUN apt-get update \
    && mk-build-deps --install --remove \
        --tool 'apt-get -y --no-install-recommends' debian/control \
    && mk-build-deps --install --remove \
        --tool 'apt-get -y --no-install-recommends' ui/debian/control

FROM deps AS build-api
COPY . .
RUN make LINTIAN=true deb-api \
    && mkdir /packages \
    && cp ./*.deb /packages/

FROM deps AS build-ui
COPY . .
RUN make LINTIAN=true deb-ui \
    && mkdir /packages \
    && cp ./*.deb /packages/

# Export-only stages, so CI can pull the debs out without the build tree.
FROM scratch AS api-packages
COPY --from=build-api /packages/ /

FROM scratch AS ui-packages
COPY --from=build-ui /packages/ /

FROM debian:trixie-slim

ARG DEBIAN_FRONTEND=noninteractive

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates tini util-linux wget \
    && wget -qO /usr/share/keyrings/proxmox-archive-keyring.gpg \
        https://enterprise.proxmox.com/debian/proxmox-release-trixie.gpg \
    && printf '%s\n' \
        'Types: deb' \
        'URIs: http://download.proxmox.com/debian/pdm' \
        'Suites: trixie' \
        'Components: pdm-no-subscription' \
        'Signed-By: /usr/share/keyrings/proxmox-archive-keyring.gpg' \
        > /etc/apt/sources.list.d/proxmox.sources \
    && apt-get update

COPY --from=build-api /packages /packages
COPY --from=build-ui /packages /packages
RUN apt-get install -y --no-install-recommends \
        /packages/proxmox-datacenter-manager_*.deb \
        /packages/proxmox-datacenter-manager-docs_*.deb \
        /packages/proxmox-datacenter-manager-ui_*.deb \
    && rm -rf /packages /var/lib/apt/lists/*

COPY --chmod=0755 docker/entrypoint.sh /usr/local/bin/pdm-entrypoint

EXPOSE 8443
VOLUME ["/etc/proxmox-datacenter-manager", "/var/lib/proxmox-datacenter-manager", "/var/cache/proxmox-datacenter-manager", "/var/log/proxmox-datacenter-manager"]

ENTRYPOINT ["/usr/bin/tini", "-g", "--", "/usr/local/bin/pdm-entrypoint"]
