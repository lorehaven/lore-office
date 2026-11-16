#!/bin/sh
# Logs the job's Docker CLI in to the registries it pushes images to (the daemon is the job's `dind` service, reached
# through DOCKER_HOST).
#
#   DOCKER_REGISTRIES         the registry hosts, separated by spaces (host and port, no path)
#   DOCKER_REGISTRY_USER,
#   DOCKER_REGISTRY_PASSWORD  the conveyor secrets holding one account's name and password (or token)
#
# The password goes in through standard input, so it is in no process list.
set -eu

: "${DOCKER_REGISTRIES:?DOCKER_REGISTRIES is not set: the registry hosts to log in to}"
: "${DOCKER_REGISTRY_USER:?the conveyor secret DOCKER_REGISTRY_USER is not set}"
: "${DOCKER_REGISTRY_PASSWORD:?the conveyor secret DOCKER_REGISTRY_PASSWORD is not set}"

tries=0
until docker info >/dev/null 2>&1; do
  tries=$((tries + 1))
  if [ "$tries" -ge 30 ]; then
    echo "no Docker daemon at ${DOCKER_HOST:-the default socket}: the job needs the privileged dind service" >&2
    exit 1
  fi
  sleep 2
done

for registry in $DOCKER_REGISTRIES; do
  printf '%s' "$DOCKER_REGISTRY_PASSWORD" | docker login "$registry" --username "$DOCKER_REGISTRY_USER" --password-stdin
done
