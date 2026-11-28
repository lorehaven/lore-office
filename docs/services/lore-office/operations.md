---
title: Operations
parent: lore-office
grand_parent: Services
nav_order: 5
---

Running the service, and what to look at when it misbehaves.

## Locally

`foreman` starts it with a gatehouse and a warehouse in Docker, on `http://localhost:8080/lore-office/` (see [Running locally](../../guides/running-locally.md)).

## In the cluster

The overlay `lore-office` runs one replica in the namespace `lore-office`; see [Deployment](../../guides/deployment.md) for the resources.

```bash
kubectl get all -n lore-office
kubectl logs -n lore-office deploy/lore-office
kubectl get ingress -n lore-office
```

| Check | Expect |
|---|---|
| `https://ennor.ddns.net/lore-office/health` | `200 ok` (the probes use `/lore-office/health/ready` and `/lore-office/health/live`) |
| `https://ennor.ddns.net/lore-office/` | the launcher |
| a page's scripts under `/_quench/` | `200`; a blank page usually means the ingress does not route this path |

## Logging

`RUST_LOG` sets the level (`info`); the log goes to the console. The service prints its address at start and logs a storage failure as `cloud storage: <reason>`, with warehouse's answer in it (the person gets `502 the storage did not answer`). It does not log every request.

## Failure modes

| Symptom | Likely cause |
|---|---|
| the Cloud panel says "Signed in as <name>: this account has no access to the cloud" | the account's token lacks the audience `lore-office`: give the person a role that carries the permission in gatehouse |
| the Cloud panel offers Sign in after signing in | the session cookie is not accepted: check `SERVICE_NAME`, that the pod reaches `GATEHOUSE_URL`, and the cookie names |
| sign-in ends at the launcher | no page was remembered (`lore_return` expired or blocked); the person opens the app again |
| a save says "The cloud said no (502)" | the storage did not answer: look for `cloud storage:` in the log; with warehouse, check the machine client's secret and its right to make a storage |
| a save says "There is no room left in your cloud storage" (`507`) | the person's quota is used up; raise `USER_QUOTA_BYTES` or delete documents |
| a save says "Someone else changed this document" (`409`) | the document was saved elsewhere since this page opened it |
| the service stops at start | `CLOUD_STORAGE=warehouse` without `WAREHOUSE_URL` or `GATEHOUSE_URL`, or a module missing in `WASM_DIR` ("run scripts/web.sh") |

## Limits

* One body is limited by `MAX_UPLOAD_BYTES` (256 MiB) and the ingress by `proxy-body-size` (256m): keep the two in step.
* The service keeps the five page modules in memory (about 9 MB).
* Documents are read and written whole; there is no partial update.

## Backups

The documents live in warehouse (or `DATA_DIR`). Back up the warehouse storages named `lore-*` and `lore-office-shares`, or the directory; the service holds no other state.
