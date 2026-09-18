#!/bin/sh
# Asks the container registry whether the watched tag carries a newer image than
# the running one and records the answer for the API daemon, which only reads
# the result file. Never fails the caller: problems are reported inside the file.
set -u

STATUS_FILE=/run/proxmox-datacenter-manager/update-status.json

REGISTRY=${PDM_IMAGE_REGISTRY:-ghcr.io}
REPOSITORY=$(printf '%s' "${PDM_IMAGE_REPOSITORY:-}" | tr '[:upper:]' '[:lower:]')
CHANNEL=${PDM_UPDATE_CHANNEL:-latest}
RUNNING_REVISION=${PDM_IMAGE_REVISION:-}
RUNNING_CREATED=${PDM_IMAGE_CREATED:-}

ACCEPT='application/vnd.oci.image.index.v1+json, application/vnd.oci.image.manifest.v1+json, application/vnd.docker.distribution.manifest.list.v2+json, application/vnd.docker.distribution.manifest.v2+json'

available_revision=
available_created=

write_status() {
    update_available=$1
    error=$2

    tmp="$STATUS_FILE.tmp"
    if ! jq -n \
        --argjson update_available "$update_available" \
        --argjson last_checked "$(date -u +%s)" \
        --arg image "$REGISTRY/$REPOSITORY:$CHANNEL" \
        --arg running_revision "$RUNNING_REVISION" \
        --arg running_created "$RUNNING_CREATED" \
        --arg available_revision "$available_revision" \
        --arg available_created "$available_created" \
        --arg error "$error" \
        '{
            "update-available": $update_available,
            "last-checked": $last_checked,
            "image": $image,
            "running-revision": $running_revision,
            "running-created": $running_created,
            "available-revision": $available_revision,
            "available-created": $available_created,
            "error": $error
        } | with_entries(select(.value != ""))' > "$tmp"
    then
        rm -f "$tmp"
        return 1
    fi

    chmod 0644 "$tmp"
    mv "$tmp" "$STATUS_FILE"
}

registry_get() {
    # $1: url, $2: value of the Accept header
    wget -q -O - --timeout=15 --tries=2 \
        --header="Authorization: Bearer $token" \
        --header="Accept: $2" \
        "$1"
}

# Everything interpolated into a registry URL is restricted to the characters a
# repository path or a tag may legally contain.
validate() {
    printf '%s' "$1" | grep -Eq "$2"
}

check() {
    if [ -z "$REPOSITORY" ]; then
        echo "PDM_IMAGE_REPOSITORY is not set" >&2
        return 1
    fi
    if ! validate "$REPOSITORY" '^[a-z0-9]+([._-][a-z0-9]+)*(/[a-z0-9]+([._-][a-z0-9]+)*)*$'; then
        echo "refusing to query registry: invalid repository '$REPOSITORY'" >&2
        return 1
    fi
    if ! validate "$CHANNEL" '^[A-Za-z0-9_][A-Za-z0-9._-]*$'; then
        echo "refusing to query registry: invalid tag '$CHANNEL'" >&2
        return 1
    fi
    if ! validate "$REGISTRY" '^[A-Za-z0-9.-]+(:[0-9]+)?$'; then
        echo "refusing to query registry: invalid registry '$REGISTRY'" >&2
        return 1
    fi

    token=$(wget -q -O - --timeout=15 --tries=2 \
        "https://$REGISTRY/token?scope=repository:$REPOSITORY:pull&service=$REGISTRY" \
        | jq -r '.token // .access_token // empty')
    if [ -z "$token" ]; then
        echo "could not obtain a pull token for $REPOSITORY" >&2
        return 1
    fi

    manifest=$(registry_get "https://$REGISTRY/v2/$REPOSITORY/manifests/$CHANNEL" "$ACCEPT")
    if [ -z "$manifest" ]; then
        echo "could not fetch the manifest of $REPOSITORY:$CHANNEL" >&2
        return 1
    fi

    # Multi-platform tags point at an index; follow it to the linux/amd64 image.
    digest=$(printf '%s' "$manifest" | jq -r '
        (.manifests // [])
        | map(select(.platform.os == "linux" and .platform.architecture == "amd64"))
        | first | .digest // empty')
    if [ -n "$digest" ]; then
        if ! validate "$digest" '^sha256:[a-f0-9]{64}$'; then
            echo "registry returned a malformed manifest digest" >&2
            return 1
        fi
        manifest=$(registry_get "https://$REGISTRY/v2/$REPOSITORY/manifests/$digest" "$ACCEPT")
    fi

    config_digest=$(printf '%s' "$manifest" | jq -r '.config.digest // empty')
    if ! validate "$config_digest" '^sha256:[a-f0-9]{64}$'; then
        echo "registry returned a malformed config digest" >&2
        return 1
    fi

    config=$(registry_get "https://$REGISTRY/v2/$REPOSITORY/blobs/$config_digest" 'application/json')
    available_revision=$(printf '%s' "$config" \
        | jq -r '.config.Labels["org.opencontainers.image.revision"] // empty')
    available_created=$(printf '%s' "$config" | jq -r '.created // empty')

    if [ -z "$available_revision" ] && [ -z "$available_created" ]; then
        echo "published image carries neither a revision label nor a build date" >&2
        return 1
    fi

    return 0
}

# check() runs in this shell, not in a command substitution, so that the values
# it collects survive; its diagnostics are captured to be stored in the result.
error_file=$(mktemp)
if check 2>"$error_file"; then
    error=
else
    error=$(cat "$error_file")
    [ -n "$error" ] || error="update check failed"
    echo "pdm-update-check: $error" >&2
fi
rm -f "$error_file"

if [ -n "$error" ]; then
    write_status false "$error"
    exit 0
fi

update_available=false
if [ -n "$RUNNING_REVISION" ] && [ -n "$available_revision" ]; then
    if [ "$RUNNING_REVISION" != "$available_revision" ]; then
        update_available=true
    fi
elif [ -n "$RUNNING_CREATED" ] && [ -n "$available_created" ]; then
    # Both timestamps are RFC 3339 in UTC, so sorting them as text orders them.
    newest=$(printf '%s\n%s\n' "$RUNNING_CREATED" "$available_created" | sort | tail -n 1)
    if [ "$newest" = "$available_created" ] && [ "$newest" != "$RUNNING_CREATED" ]; then
        update_available=true
    fi
fi

write_status "$update_available" ""
