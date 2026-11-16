---
title: Deployment
parent: Guides
nav_order: 6
---

The service is one image, released by the pipeline and run by the `lore-office` overlay of `homecloud` behind the estate's ingress.

## The image

`Dockerfile` has two stages. The builder (`rust:1.94-alpine`) adds the `wasm32-unknown-unknown` target, writes the cargo config for the `ennor` registry from the build arguments `CARGO_REGISTRIES_ENNOR_INDEX` (default `sparse+https://ennor.ddns.net/index/`) and `CARGO_REGISTRIES_ENNOR_TOKEN`, runs `bash scripts/web.sh` and `cargo build --release -p lore-office-server`. The runtime stage (`alpine:3.22`) has `ca-certificates`, the user `lore` (uid and gid 10001), the binary and `web/` in `/app`, an empty `/data` and these defaults:

```text
RUST_LOG=info  HOST=0.0.0.0  PORT=8080  WASM_DIR=/app/web  DATA_DIR=/data  CLOUD_STORAGE=local
```

It exposes 8080 and runs `/app/lore-office-server`.

## The pipeline

`.anvil.toml` defines one Docker module: the `Dockerfile`, the package `lore-office-server`, the image `lore-office`, pushed to the registries `ossiriand.arda:8080/forge` and `ennor.ddns.net/forge`. `anvil release --all` bumps the version of `lore-office-server` (its own `version` in `apps/lore-office/server/Cargo.toml`, not the workspace's) and tags `lore-office-server-v<version>`.

`.conveyor.toml` runs on every push to `master` and on every pull request:

| Stage | Jobs |
|---|---|
| `check` | `format` (`anvil format --check`), `lint` (`anvil lint --all-targets --deny-warnings`, after `scripts/ci-placeholders.sh`), `deps` (`cargo deny check`, `anvil machete`, `anvil audit`) |
| `build` | `server`: the wasm target, `scripts/web.sh`, `cargo build --release -p lore-office-server` |
| `test` | `unit`: `anvil test --all` |
| `release-image` | by hand only (`when = "event == 'release'"`): logs in to the registries (`scripts/ci-docker-login.sh`) and runs `anvil docker release lore-office-server`, with a privileged `docker:dind` service. Needs the secrets `DOCKER_REGISTRY_USER` and `DOCKER_REGISTRY_PASSWORD` on the repository |

```bash
conveyor run lorehaven/lore-office --stage release-image --wait
```

The repository is registered in conveyor under the project `lorehaven/lore-office`, with a push webhook to `https://ennor.ddns.net/conveyor/api/v1/webhooks/github` signed with the repository secret `WEBHOOK_SECRET`.

## The overlay

`homecloud/overlays/lore-office` renders a namespace `lore-office` with:

| File | Resource |
|---|---|
| `deployment.yaml.j2` | one replica of `ossiriand.arda:8080/forge/lore-office/lore-office:<version>` on port 8080; requests 50m CPU and 64Mi, limits 500m and 512Mi; readiness `/lore-office/health/ready`, liveness `/lore-office/health/live` |
| `service.yaml.j2` | service `lore-office`, port 80 to 8080 |
| `ingress.yaml.j2` | host `ennor.ddns.net`, paths `/lore-office` and `/_quench` to the service, TLS from `letsencrypt-prod`, HTTP redirected, `proxy-body-size: 256m` |
| `config.yaml.j2` | the ConfigMap `lore-office-config` (the values below) |
| `secret.yaml.j2` | the Secret `lore-office-secret` with `GATEHOUSE_CLIENT_SECRET` and `WAREHOUSE_CLIENT_SECRET` |
| `values.yaml`, `secrets.yaml` | plain values, and the age-encrypted secrets `CLIENT_SECRET_LORE_OFFICE` and `CLIENT_SECRET_LORE_OFFICE_WAREHOUSE` |

The image tag in `deployment.yaml.j2` and the version in `rivet.toml` change together when a new image is released. `homecloud`'s pipeline packs and publishes the overlay as a rivet package with the others; `riveter install lore-office` installs it, or `riveter --env lore-office apply` applies the working copy.

The pages' scripts load from `/_quench/`, so the ingress must route that path to the service too.

## Gatehouse

The overlay `forge` carries what lore-office needs from gatehouse: two clients (`lore-office`, redirect path `/ui/auth/callback`, scope `lore-office`; and `lore-office-warehouse`, scope `warehouse`), the service `lore-office` in the permission catalogue with `home = true` (its card on gatehouse's home page, which links to `LORE_OFFICE_UI_URL`, `https://ennor.ddns.net/lore-office/`) and the grants in the viewer and editor templates. Gatehouse serves the card's icon from a fixed list compiled into it (`ICONS` in `gatehouse-core`, the pictures in `services/gatehouse`); `lore-office` needs an entry there.

## Order

1. Apply the `forge` overlay (gatehouse learns the clients and the permission), and make sure gatehouse picked up the new config.
2. Release the image, set the tag in the overlay, apply or install the `lore-office` overlay.
3. Give people a role that carries the `lore-office` permission.
4. Open `/lore-office/`, sign in, save a document: the first save makes the person's warehouse storage.
