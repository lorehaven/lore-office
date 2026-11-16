---
title: API
parent: lore-office
grand_parent: Services
nav_order: 2
---

The cloud API under `<base path>/api`. The machine-readable description is [`openapi.yaml`](openapi.yaml) (OpenAPI 3.1); this page is the same in prose.

Calls act on the signed-in person's own documents. The exception is `s/{token}`, which needs no sign-in. A call from someone who is not accepted (see [Authentication](../../guides/authentication.md)) gets `401 sign in first` with `www-authenticate: Bearer`. A path or method that matches nothing gets `404 no such call`. Texts of errors are the bodies of the responses, as plain text.

## Status

`GET /api/status` answers `200` with `{"user": <name or null>, "login": "<where signing in starts>"}`. It needs no sign-in.

## Usage

`GET /api/usage` answers `200` with `{"used": <bytes>, "quota": <bytes>}`.

## Documents

| Call | Does |
|---|---|
| `GET /api/docs?kind=<kind>` | lists the person's documents (`DocMeta` array), newest first; `kind` (`docs`, `sheets`, `notes`, `pdf`) limits the list, empty or absent lists all |
| `PUT /api/docs?name=<name>&kind=<kind>` | makes a document from the request body; answers `200` with its `DocMeta` |
| `GET /api/docs/{id}` | the bytes, as `application/octet-stream`, with the headers below |
| `PUT /api/docs/{id}?name=<name>&kind=<kind>` | replaces the document; the header `If-Match: <etag>` makes it conditional |
| `GET /api/docs/{id}/meta` | the document's `DocMeta` without its bytes; what a page asks every few seconds to see whether someone else saved |
| `DELETE /api/docs/{id}` | deletes the document and its links; `204` |

`name` and `kind` are required on `PUT` (`400 name and kind are needed`); `kind` must be one of the four (`400`). The body is limited by `MAX_UPLOAD_BYTES` (`413 the file is too large`). A save over the quota answers `507`. An `If-Match` that is not the current etag answers `409 the document was changed by someone else` with the current etag in `x-etag`.

Headers of `GET /api/docs/{id}` and `GET /api/s/{token}`: `x-name` (the name, percent-encoded), `x-kind`, `x-etag`, `cache-control: no-store`; the share one adds `x-rights` (`view` or `edit`).

## Folders

| Call | Does |
|---|---|
| `GET /api/folders` | every folder the person has (the ones they made and the ones their documents are in, with those above them), sorted, as a JSON array of paths such as `Work/2026` |
| `POST /api/folders?path=<path>` | makes a folder (and those above it); answers with the list |
| `DELETE /api/folders?path=<path>` | takes away a folder; `400 the folder is not empty` when a document or a folder is in it; `204` |
| `PUT /api/docs/{id}/move?folder=<path>` | puts a document in a folder (empty `folder` is the top); the bytes and the etag stay; answers with the `DocMeta` |

A path is parts separated by `/`: not empty, not `.` or `..`, none of `\ : * ? " < > |`, at most 64 characters a part and 8 parts. `PUT /api/docs` also takes `folder=` and makes the new document there. A save over a document keeps its folder.

## Shared with a person

A document can be shared with one named person. It stays in the owner's cloud (and counts against the owner's quota) and shows in the other person's panel under "Shared with me".

| Call | Does |
|---|---|
| `POST /api/docs/{id}/grant?user=<name>&rights=<view\|edit>` | shares the owner's document with `user`; answers with the `Grant`. `400` for oneself or an empty name |
| `GET /api/docs/{id}/grants` | who the owner shared it with |
| `DELETE /api/docs/{id}/grant?user=<name>` | stops sharing it with them; `204` |
| `GET /api/grants` | what others shared with the signed-in person (`Grant` array, newest first) |
| `GET /api/shared/{owner}/{id}` | opens it: the bytes, with `x-rights` (`view` or `edit`) |
| `GET /api/shared/{owner}/{id}/meta` | its `DocMeta`, with `x-rights` |
| `PUT /api/shared/{owner}/{id}` | saves it, with `If-Match`; only with edit rights (`403` otherwise), into the owner's copy |

Someone nothing was shared with gets `404`. The service does not check that `user` exists anywhere: the name must be the one the person signs in with.

## Links

| Call | Does |
|---|---|
| `POST /api/docs/{id}/share?rights=<view\|edit>&days=<n>` | makes a link; answers `200` with the `Share` and the header `x-link` (the address). `rights` defaults to `view`; `days` (a positive integer) sets the end, otherwise never |
| `GET /api/shares` | the person's links, newest first |
| `DELETE /api/shares/{token}` | withdraws a link; `204` |
| `GET /api/s/{token}` | opens the document of a link; no sign-in. `404` when the link is unknown, withdrawn or ended |
| `GET /api/s/{token}/meta` | the `DocMeta` of the link's document, without its bytes; no sign-in; the header `x-rights` says what the link allows |
| `PUT /api/s/{token}` | saves through an edit link; `If-Match` as above. `403 this link only lets you view` for a view link |

## Errors

| Status | When |
|---|---|
| `400` | missing or invalid `name`, `kind` or `rights`; an id that is not hexadecimal |
| `401` | not signed in (or not accepted) |
| `403` | a view link used to save |
| `404` | no such document, link or call |
| `405` | `s/{token}` with a method other than `GET` or `PUT` |
| `409` | a save over a newer version |
| `413` | a body over `MAX_UPLOAD_BYTES` |
| `502` | the storage did not answer (the cause is in the log) |
| `507` | no room left in the person's quota |

The apps show these in words: "Sign in first", "This link only lets you view", "Not found", "Someone else changed this document: open it again, or save it as a new one", "There is no room left in your cloud storage", "The cloud cannot be reached".
