#!/usr/bin/env bash
# The forge services lore-office is part of, as Docker containers: foundry installs their schemas, then gatehouse and
# warehouse start. foreman runs `up` before lore-office (foreman.toml, which sets every variable used here) and `down`
# is `foreman run forge-down`.
set -euo pipefail
cd "$(dirname "$0")/.."

gatehouse=lore-office-gatehouse
warehouse=lore-office-warehouse

down() {
  docker rm -f "$gatehouse" "$warehouse" >/dev/null 2>&1 || true
}

wait_for() { # name url
  for _ in $(seq 1 60); do
    if curl -fs -o /dev/null "$2"; then
      echo "$1 is ready"
      return 0
    fi
    if [ -z "$(docker ps -q -f "name=^$1\$")" ]; then
      break
    fi
    sleep 1
  done
  echo "$1 did not become ready; its log:" >&2
  docker logs --tail 40 "$1" >&2 || true
  return 1
}

up() {
  docker run --rm --network host \
    -e DATABASE_URL -e FOUNDRY_INSTALL=gatehouse,warehouse -e RUST_LOG -e LOG_DIR=/tmp/logs \
    "$FOUNDRY_IMAGE"

  # Made again on every start, so a changed setting or image takes effect.
  down

  # On the host's network, listening on this machine only.
  docker run -d --name "$gatehouse" --network host \
    -v "$PWD/local/gatehouse:/config:ro" \
    -e DATABASE_URL -e REDIS_URL -e RUST_LOG -e SERVICE_REALM \
    -e SERVER_ADDR=127.0.0.1:5443 -e BASE_PATH=/gatehouse -e SERVICE_NAME=gatehouse \
    -e SERVICE_AUTH_ENABLED=true -e AUTH_DB_SCHEMA=auth \
    -e AUTH_BOOTSTRAP=true -e SERVICE_USERNAME -e SERVICE_PASSWORD \
    -e GATEHOUSE_KEY_ENCRYPTION_KEY \
    -e CLIENTS_CONFIG=/config/clients.toml -e PERMISSIONS_CONFIG=/config/permissions.toml \
    -e CLIENT_SECRET_WAREHOUSE -e CLIENT_SECRET_LORE_OFFICE -e CLIENT_SECRET_LORE_OFFICE_WAREHOUSE \
    -e AUTH_REDIRECT_HOSTS="$PUBLIC_URL" \
    -e WAREHOUSE_UI_URL="$PUBLIC_URL/warehouse/ui/home" \
    -e WAREHOUSE_API_URL="$PUBLIC_URL/warehouse" \
    -e LORE_OFFICE_UI_URL="$PUBLIC_URL/lore-office/" \
    "$GATEHOUSE_IMAGE" >/dev/null
  wait_for "$gatehouse" http://127.0.0.1:5443/gatehouse/health/ready

  docker run -d --name "$warehouse" --network host \
    -v lore-office-warehouse:/data \
    -e DATABASE_URL -e REDIS_URL -e RUST_LOG -e SERVICE_REALM \
    -e SERVER_ADDR=127.0.0.1:6443 -e BASE_PATH=/warehouse -e SERVICE_NAME=warehouse \
    -e SERVICE_AUTH_ENABLED=true -e AUTH_DB_SCHEMA=auth -e DB_SCHEMA=warehouse \
    -e GATEHOUSE_URL="$PUBLIC_URL/gatehouse" -e GATEHOUSE_CLIENT_ID=warehouse \
    -e GATEHOUSE_CLIENT_SECRET="$CLIENT_SECRET_WAREHOUSE" \
    -e PUBLIC_URL="$PUBLIC_URL/warehouse" -e REGISTRY_BASE_URL="$PUBLIC_URL/warehouse" \
    -e FEATURE_FILES_ENABLED=true -e FEATURE_CRATES_ENABLED=false -e FEATURE_DOCKER_ENABLED=false \
    -e FEATURE_APK_ENABLED=false -e FEATURE_RIVETS_ENABLED=false \
    -e DYNAMIC_STORAGE_ROOT=/data/dynamic \
    -e DOCKER_TOKEN_SECRET \
    "$WAREHOUSE_IMAGE" >/dev/null
  wait_for "$warehouse" http://127.0.0.1:6443/warehouse/health/ready
}

case "${1:-}" in
up) up ;;
down) down ;;
*)
  echo "usage: $0 up|down" >&2
  exit 2
  ;;
esac
