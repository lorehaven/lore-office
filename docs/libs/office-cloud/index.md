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
| `Grant` | a document shared with a named person: `owner`, `doc`, `with`, `name`, `kind`, `rights`, `created` |
| `clean_folder(path)` | a folder path made fit to keep, or `Error::Invalid` |
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
| `meta(user, id)` | the `DocMeta` alone |
| `put_op`, `ops_after`, `ops_head`, `drop_ops`, `set_version` | the files of a document's log of changes |
| `folders(user)`, `make_folder`, `delete_folder`, `move_doc(user, id, folder)` | folders and putting a document in one |
| `put_grant`, `grants_to(user)`, `grants_of(owner, doc)`, `grant(owner, doc, with)`, `delete_grant` | documents shared with a named person |
| `put(user, id, name, kind, bytes, if_match)` | makes (`id` is `None`) or replaces a document; checks the kind, the name, the quota and the etag |
| `delete(user, id)` | deletes the document and its links |
| `usage(user)` | `Usage` |
| `put_share(share)`, `share(token)`, `shares_of(user)`, `delete_share(user, token)` | the links |

Two implementations: `LocalStore::new(root, quota)` / `LocalStore::from_env(root)` (a directory per person) and `WarehouseStore::from_env()` (a warehouse storage per person; `WarehouseStore::storage_of(user)` gives the storage's name). `Hub` keeps who has each document open (`update`, `wait`, `touch`; in memory, one lock) and holds a page's question until something changes; `Peer` is one page. `OpLog` numbers a document's changes (`append`, `since`, `snapshot`, `head`; one lock, the latest numbers in memory) and answers `Error::Behind { head }` to a change that began from an old number; `Since` is what a page asking for changes after some number is told. `grant_doc`, `open_granted`, `granted_meta` and `save_granted` apply the rules of sharing with a person (only a grant with edit rights may save; the save goes into the owner's copy). Three functions apply the rules of a link: `open_share(store, token)`, `share_meta(store, token)` (the same without the bytes) and `save_share(store, token, bytes, if_match)` (only a link with edit rights may save; an ended link is `NotFound`).

The layouts are in [Data](../../services/lore-office/data.md); the environment in [Configuration](../../services/lore-office/configuration.md).
