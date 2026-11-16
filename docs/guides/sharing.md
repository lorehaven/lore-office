---
title: Sharing
parent: Guides
nav_order: 4
---

A document in the cloud can be shared by a link that anyone holding it can open, signed in or not.

## In the apps

File ▸ *Cloud…* opens the documents panel, 70% of the window wide and high:

* **Folders at the side**: *My documents*, the folders under it (nested), and *Shared with me*. Under them, who is signed in, Sign out, and the Server field with *Connect*.
* **Documents in the middle** of the app's kind, in the folder shown, with a breadcrumb above. They are shown as **cards or as a list** (the two buttons at the right of the toolbar; the choice is kept). A click picks a document (the box on it picks several), a double click opens it.
* **The toolbar** acts on what is picked: *Open*, *Move to…* (a menu of the top and every folder), *Share…* and *Delete*; a field and *Create* make a folder inside the folder shown; *Save here* saves the open document in the folder shown; the space used is at the right. An empty folder has a delete button. *Save to the cloud* saves over the open document; *Save a copy to the cloud* saves it as a new one.

*Share…* offers two ways:

* **A link** that anyone holding it can open: whether it lets people change the document (otherwise they only view), and after how many days it ends (empty: never). *Make a link* shows the address; the panel lists the links of the document, each with Withdraw.
* **A person**: type the name they sign in with, choose whether they may change it, *Share with them*. It stays in your cloud (under your quota) and shows in their panel under *Shared with me*, where they open it and, with edit rights, save it back into your copy. It is kept in step like any document in the cloud (see below). The list under the field shows who has it, each with Withdraw. The name is not checked: a typo shares with nobody.

## Saving, and staying in step

When you are signed in, the cloud is where **Ctrl+S** saves (docs, sheets and notes): the first time it makes the document in the cloud, afterwards it saves over it. The File menu still has *Save as…* items for a file on your computer, and the Ctrl+S hint moves to *Save to the cloud*. On a page opened by an edit link Ctrl+S saves through the link; with a view link, or when nobody is signed in, it saves a file as before.

A document that is in the cloud is kept up to date in both directions:

* **docs and sheets send your changes by themselves**, two seconds after the last change. The Cloud button shows a dot: filled when the cloud has your latest version, hollow while changes are still to be sent. notes and pdf save when you ask.
* **Every five seconds the page asks whether someone else saved** (`GET /api/docs/{id}/meta`, or `/api/s/{token}/meta` through a link; a few hundred bytes). If so and you have changed nothing since, their version replaces the one on the screen ("Updated: someone else changed this document"), so there is no need to reload the page.
* **If you have changes of your own** when theirs arrives, nothing is replaced: a banner offers *Load their version* (yours is discarded) or *Keep mine* (the next save goes over theirs). Sending changes by themselves stops until you choose.

This is whole-document sync, not live co-editing: the page that saves last, after choosing, wins, and an update replaces the document, so the caret and the undo history start again.

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
