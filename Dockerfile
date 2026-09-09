FROM debian:trixie AS builder

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

WORKDIR /source
COPY . .

RUN mk-build-deps --install --remove \
        --tool 'apt-get -y --no-install-recommends' debian/control \
    && mk-build-deps --install --remove \
        --tool 'apt-get -y --no-install-recommends' ui/debian/control \
    && DEB_BUILD_OPTIONS=nocheck make deb \
    && mkdir /packages \
    && cp ./*.deb /packages/

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

COPY --from=builder /packages /packages
RUN apt-get install -y --no-install-recommends \
        /packages/proxmox-datacenter-manager_*.deb \
        /packages/proxmox-datacenter-manager-docs_*.deb \
        /packages/proxmox-datacenter-manager-ui_*.deb \
    && rm -rf /packages /var/lib/apt/lists/*

COPY --chmod=0755 docker/entrypoint.sh /usr/local/bin/pdm-entrypoint

EXPOSE 8443
VOLUME ["/etc/proxmox-datacenter-manager", "/var/lib/proxmox-datacenter-manager", "/var/cache/proxmox-datacenter-manager", "/var/log/proxmox-datacenter-manager"]

ENTRYPOINT ["/usr/bin/tini", "-g", "--", "/usr/local/bin/pdm-entrypoint"]
