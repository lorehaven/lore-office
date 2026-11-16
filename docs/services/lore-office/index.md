---
title: lore-office
parent: Services
nav_order: 1
has_children: true
permalink: /services/lore-office/
---

`lore-office-server` (version 0.1.1, package `apps/lore-office/server`) serves the whole suite on one port: the launcher, the four apps, their assets, the sign-in routes, the cloud API and the health checks.

## Routes

Paths are relative to `BASE_PATH` (`/lore-office` in the estate; empty when `BASE_PATH` is unset or `/`).

| Path | Answers |
|---|---|
| `/` | the launcher |
| `/docs`, `/sheets`, `/notes`, `/pdf` | the app's page, rendered with the page context (see [Architecture](../../guides/architecture.md)) |
| `/_quench/...` | the hashed scripts and styles of the pages, from whichever app owns the hash |
| `/api/...` | the cloud API ([API](api.md)) |
| `/health`, `/health/<anything>` | `200 ok` (the overlay uses `/health/ready` and `/health/live`) |
| `/ui/login`, `/ui/auth/callback`, `/ui/logout`, `/ui/home` | sign-in ([Authentication](../../guides/authentication.md)); present only with `SERVICE_AUTH_ENABLED=true`, except `/ui/logout` and `/ui/home` |

Every response carries `x-content-type-options: nosniff`; API responses also carry `referrer-policy: no-referrer`. Anything else answers 404.

## Dependencies

| Needs | When |
|---|---|
| gatehouse | with `SERVICE_AUTH_ENABLED=true`: sign-in, and the keys that verify session tokens |
| warehouse | with `CLOUD_STORAGE=warehouse`: the files API, through the client `lore-office-warehouse` |
| a writable `DATA_DIR` | with `CLOUD_STORAGE=local` |

There is no database and no Redis.

## Pages of this section

* [API](api.md) and [`openapi.yaml`](openapi.yaml)
* [Configuration](configuration.md)
* [Data](data.md)
* [Operations](operations.md)
