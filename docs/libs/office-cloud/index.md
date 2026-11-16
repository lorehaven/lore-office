---
title: office-cloud
parent: Libraries
nav_order: 1
---

The cloud side of the suite, independent of any host: what a document in the cloud is, what a share link is, the storage behind them, and the rules (who may read, write and share what). Version 0.1.0.

```toml
[dependencies]
office-cloud = { workspace = true }                          # the types only (the pages in the browser)
office-cloud = { workspace = true, features = ["server"] }   # and the storage (the service)
```

## Features

| Feature | Default | Enables |
|---|---|---|
| `server` | yes (the workspace turns it off by default for the pages) | `LocalStore`, `WarehouseStore`, `Storage`, and the dependencies `quench-archive`, `quench-client` and `tokio` |

## Types

| Item | Meaning |
|---|---|
| `DEFAULT_QUOTA_BYTES` | 10 GiB |
| `DocMeta` | a document without its bytes: `id`, `name`, `kind`, `size`, `modified`, `etag` |
| `Rights` | `View` or `Edit`; `Rights::parse(&str)` (`"view"`, `"edit"`), `Rights::word()` |
| `Share` | a link: `token`, `owner`, `doc`, `name`, `kind`, `rights`, `created`, `expires`; `Share::expired(now)` |
| `Usage` | `used` and `quota` in bytes |
| `Error` | `NotFound`, `Conflict { etag }`, `Quota { used, quota }`, `Denied`, `Invalid(String)`, `Io(String)` |
| `now()`, `new_id(bytes)`, `safe_name(text)` | seconds since 1970; a random hexadecimal identifier; a name fit for a file or storage |

## Storage (feature `server`)

`Storage` is an async trait:

| Method | Does |
|---|---|
| `list(user)` | the person's documents |
| `get(user, id)` | a document's `DocMeta` and bytes |
| `put(user, id, name, kind, bytes, if_match)` | makes (`id` is `None`) or replaces a document; checks the kind, the name, the quota and the etag |
| `delete(user, id)` | deletes the document and its links |
| `usage(user)` | `Usage` |
| `put_share(share)`, `share(token)`, `shares_of(user)`, `delete_share(user, token)` | the links |

Two implementations: `LocalStore::new(root, quota)` / `LocalStore::from_env(root)` (a directory per person) and `WarehouseStore::from_env()` (a warehouse storage per person; `WarehouseStore::storage_of(user)` gives the storage's name). Two functions apply the rules of a link: `open_share(store, token)` and `save_share(store, token, bytes, if_match)` (only a link with edit rights may save; an ended link is `NotFound`).

The layouts are in [Data](../../services/lore-office/data.md); the environment in [Configuration](../../services/lore-office/configuration.md).
