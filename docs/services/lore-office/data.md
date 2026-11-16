---
title: Data
parent: lore-office
grand_parent: Services
nav_order: 4
---

The service keeps no database. A document is two files, its bytes and a small description; a share link is one file.

## Records

`DocMeta` (`crates/office-cloud/src/model.rs`), as JSON:

| Field | Type | |
|---|---|---|
| `id` | string | 16 hexadecimal characters, made when the document is first saved |
| `name` | string | trimmed; at most 200 bytes, not empty |
| `kind` | string | `docs`, `sheets`, `notes` or `pdf`: the app that opens it |
| `size` | integer | bytes |
| `modified` | integer | seconds since 1970 |
| `etag` | string | changes when the bytes do; `<crc32 in 8 hexadecimal digits>-<length>` in the local store |

`Share`:

| Field | Type | |
|---|---|---|
| `token` | string | 64 hexadecimal characters |
| `owner` | string | the person's name |
| `doc` | string | the document's `id` |
| `name`, `kind` | string | copied from the document when the link is made |
| `rights` | string | `View` or `Edit` |
| `created` | integer | seconds since 1970 |
| `expires` | integer or null | seconds since 1970; null: never |

`Usage`: `{"used": <bytes>, "quota": <bytes>}`.

## Local store (`CLOUD_STORAGE=local`)

```text
<DATA_DIR>/users/<safe name>-<crc32 of the name, 8 hex digits>/<id>.bin    the document
<DATA_DIR>/users/<safe name>-<crc32 of the name, 8 hex digits>/<id>.json   its `DocMeta`
<DATA_DIR>/shares/<token>.json                                             a `Share`
```

A safe name keeps letters, digits and `-_.`, is never empty and is at most 64 characters; the checksum of the real name keeps two names that look alike apart. A save writes the bytes to `<id>.tmp` first and renames it, so a failure part way leaves the old document whole. Usage is the sum of the `.bin` files of the person.

## Warehouse store (`CLOUD_STORAGE=warehouse`)

One warehouse storage per person, made the first time they save, named `lore-<name>-<crc32, 8 hex digits>` with `quota_bytes` from `USER_QUOTA_BYTES`. Warehouse allows only letters, digits, `-` and `_` in a storage name, so in `<name>` (at most 48 characters, lower case) every other character, a dot or an `@` for example, becomes `_`; the checksum of the real name keeps `ada.lovelace` and `ada_lovelace` apart. The storage's owner is the person's name, which must be a user warehouse knows (warehouse answers `400 no such user to own this storage` otherwise). Inside it:

```text
docs/<id>.bin
docs/<id>.json
```

A save first asks warehouse for the storage (`GET /api/v1/files/{storage}`) and makes it (`POST /api/v1/files`) only when that answers 404. A person's usage is the sizes of the files under `docs/` added up from the listing, since warehouse's answer for one storage carries no totals.

All share links are in one storage, `SHARES_STORAGE` (`lore-office-shares`), as `<token>.json`, so a link is found from its token.

The service uses warehouse's files API with a token it gets from gatehouse (`POST <GATEHOUSE_URL>/api/v1/token`, client credentials): `POST /api/v1/files` makes a storage (`name`, `owner`, `quota_bytes`), `PUT`, `GET` and `DELETE /api/v1/files/{storage}/file?path=` keep a file, `GET /api/v1/files/{storage}?prefix=&n=1000` lists (pages follow with `last`). A refused write over the quota (`413` or `507`) is a quota error to the person.

*Unverified:* the calls were written against the forge sources and tested against fakes; the first run against the live estate may need a small fix.

## Limits

* A person who has never saved has no storage; their usage is 0 and the panel shows "1 kB" (usage under 1 kB is rounded up in the panel).
* Deleting a document deletes its `.bin` and `.json` and its share links.
