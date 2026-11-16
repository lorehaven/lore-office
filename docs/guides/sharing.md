---
title: Sharing
parent: Guides
nav_order: 4
---

A document in the cloud can be shared by a link that anyone holding it can open, signed in or not.

## In the apps

File ▸ *Cloud…* opens the panel: the account line, a Server field with Connect, and the list of the person's documents of this app, each with Open, Share… and Delete. *Save to the cloud* saves over the open document; *Save a copy to the cloud* saves it as a new one.

*Share…* offers: whether the link lets people change the document (otherwise they only view), and after how many days it ends (empty: never). *Make a link* shows the address; the panel lists the links of the document, each with Withdraw.

## What a link is

A link holds a random token of 32 bytes (64 hexadecimal characters). The address is `<scheme>://<host><base path>/<app>?share=<token>`, built from `x-forwarded-proto` (https if absent) and `x-forwarded-host` (or `host`). Whoever has the token has the rights of the link on that one document:

| Rights | Opening | Saving |
|---|---|---|
| `view` | yes | `403 this link only lets you view` |
| `edit` | yes | yes, into the owner's copy, under the owner's quota |

A link that was withdrawn, has run out, or whose document was deleted answers `404`. Deleting a document removes its links.

A page opened by a link tells the person so ("Opened from a link: you can edit" or "view only") and loads the document. An edit link saves through the link with the version the page began from (`If-Match`), so a concurrent change is a conflict, not an overwrite.

## Versions

Every document has an `etag` that changes with its bytes. A save that names an etag which is no longer current gets `409` with the current etag in `x-etag`; the apps say "Someone else changed this document: open it again, or save it as a new one".
