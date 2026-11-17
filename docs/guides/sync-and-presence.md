---
title: Sync and presence
parent: Guides
nav_order: 5
---

How a document in the cloud is kept in step between hosts: a log of paragraph changes for docs, a log of cell changes for sheets, whole documents for notes and pdf, and who is editing where.

## Whole documents (notes, pdf; docs before the log)

* **Saving.** docs and sheets send the whole file one second after the last change, or eight seconds after the first unsaved change when the person keeps typing without a pause. Each save names the version it began from (`If-Match`); the service answers `409` when it is no longer the current one. notes and pdf save when asked.
* **Looking.** Every two seconds a page asks `GET /api/docs/{id}/meta` (through a link, `/api/s/{token}/meta`; for a shared document, `/api/shared/{owner}/{id}/meta`). When the version moved and the person has changed nothing since, the page loads the new file. With changes of their own it offers *Load their version* or *Keep mine*.
* **Cost of one save.** The browser sends the whole file to the service, which checks the version from the description file and writes `docs/<id>.bin` and `docs/<id>.json` into the person's warehouse storage. Warehouse hashes the bytes and stores them as a blob; the blob of the replaced content is deleted once the change is committed.
* **Limit.** Two people changing one document at once get the choice banner, however short the timers are. Whole-document replace cannot merge.

## Sheets: a log of cell changes (built)

A sheets document in the cloud is its last full `.xlsx` and a numbered log of the cell changes made since. The service numbers the changes; each page applies them in that order.

* **What is sent.** A step that writes cells (typing, paste, fill, clear, a number format, undo and redo among them) is sent as one change: for each cell its place (sheet, row, column), its content (a value or a formula) and its look as the style itself, not as an index into the workbook's styles, because the style tables of two pages need not agree. A change is a few hundred bytes. Anything a list of cells does not describe (rows or columns inserted or deleted, a sheet added or renamed, a width, sorting, tables) makes the page send the whole file instead. Tracking is conservative: the app marks the document as changed in a way no cell list describes whenever its workbook is borrowed for change outside the cell path, so a missed case sends the file rather than losing a change.
* **When.** After 0.4 s without a change, or at most 2 s after the first unsent one. The change names the number it began from (`POST .../ops?base=n`). A change that began from an old number is refused with the latest (`409`); the page then asks for what it missed, applies it, applies its own unsent changes again, and sends them from the new number. So other pages' changes come first and the page's own after: for the same cell the later change wins, which is the page's.
* **Looking.** Every 2 s a page asks `GET .../ops?after=n`. The changes it gets are written into the open document (the grid redraws; the selection, caret and undo history stay). A cell this page changed and has not sent yet keeps its value.
* **The whole file.** A page that is up to date sends the whole file as the new snapshot, `PUT ...?at=n`, in three cases: it made a change no cell list describes; the person pressed Ctrl+S; or 50 changes are beyond the stored file and the page has been idle for 3 s. The service accepts it only when `n` is still the latest number (else `409` and the page catches up first), then drops the changes up to `n`. A page whose number is older than the stored file's version gets `410` and fetches the file again.
* **First save.** The first save of a new document sends the file whole; the changes that were waiting are in it.
* **Not covered.** A document opened from a view link or a view share only looks. The cell's `base` (the cell format a loaded file gave it) is not sent: a cell written by another page loses the border detail of its original format. The remote changes do not enter the undo history.

## Docs: a log of paragraph changes (built)

Docs uses the same log, service and timing as sheets (see above); what differs is what a change is.

* **A change** replaces paragraphs: `{"at": 3, "del": 1, "ins": [paragraph, ...]}` means from paragraph 3, one paragraph is replaced by the ones listed. A paragraph is sent in full (kind, alignment, spacing and its runs with bold, italic, underline, strike, super/subscript, highlight, size, colour and family). The change is found by comparing the document with the last one the cloud is known to have and leaving out the paragraphs that are alike at the start and at the end, so editing steps are not tracked: typing in one paragraph sends that paragraph, a few hundred bytes.
* **The whole file instead** when a change cannot be carried: a paragraph that points into the document's lists (a font by name, a link, a picture, a table cell), or a change to the header, footer, paper size or tables.
* **Positions.** "Paragraph 3" means nothing once others' changes came in before it, so docs sends one change at a time, and when a change is refused because others came first, the page merges theirs into its document (theirs where it did not edit; its own where both changed the same paragraphs, which then stay and are sent over theirs), takes them as the cloud's state, and works its own change out again from the difference (`DeltaHooks::recompute`).
* **Not covered.** Others' changes do not enter the undo history, so undoing after someone else's edit in the same place sends the older paragraphs again. A caret inside a paragraph someone else changed is kept by the browser, not moved by the model.

## The design that came first

The design below was written before the code; sheets and docs follow it. What differs in the code is noted above.

A document in the cloud becomes its last full file plus a short, numbered log of changes.

```text
docs/<id>.bin             the last full file: a normal .docx / .xlsx, at most N changes behind
docs/<id>.json            its description, with the version of the snapshot
ops/<id>/<n>.json         change number n (n counts from the snapshot's version)
```

* **Deltas come from the model, not from the zip.** A byte difference of a `.docx` or `.xlsx` is nearly as large as the file, because one edit rewrites most of the compressed package. So each app describes its changes: docs by paragraph (a paragraph has a stable id: *changed*, *inserted after*, *deleted*), sheets by cell (address and new content), notes by the text of the changed note. pdf stays whole-file. A change the app cannot describe as a delta (inserting or deleting rows in sheets, say) sends a full snapshot instead; that fallback is part of the design from the start.
* **Order.** The service numbers the deltas of a document and refuses one built on an old version (`409` with the deltas since, or the instruction to fetch them). With one service replica a mutex per document is enough.
* **Reading.** A page asks "anything after version n?" and gets the new deltas, which it applies to the document on screen in place: the caret and the undo history stay. Opening a document means fetching the snapshot and the deltas after it and replaying them.
* **Compaction is the merge.** After about 50 deltas, or a few minutes of quiet, one page that has applied every delta uploads a new full file as the new snapshot, and the older deltas are deleted. Every page applies deltas in the order the service gave, so there is nothing else to merge.
* **Conflicts.** Changes to different paragraphs or cells combine cleanly. The same paragraph or cell changed by two people at once is resolved by order (the later delta wins), and the loser is told. Merging inside one paragraph (operational transform, or a CRDT) is deliberately out of scope.
* **Existing documents.** A stored file with no log is version 0.

Size of a save goes from the whole document to a few hundred bytes; warehouse files change rarely, and a check for news is a few bytes. A file with no log of its own always opens: the snapshot is a complete document.

## Presence and the held question (built)

Presence says who else has the document open and where. It is advisory: it enforces nothing, and it is kept in the service's memory only (a restart empties it and the pages say again where they are).

* **Where a page is.** A page sends `POST .../presence` with a client id, its name and colour, and a short text its app writes: sheets send the selected rectangle (`sheet:row:col:row:col`), docs the paragraph the caret is in (`p<index>`). It sends when the text changes (looked at every 0.4 s) and every 8 s otherwise. Leaving a document (opening another, signing out) sends `gone`.
* **The held question.** A page also keeps one `GET .../presence?client=c&seen=n&wait=20000` open. The service answers it when anything about the document's room changes from number `n`: a page arrives, moves or leaves (one that is not heard from for 12 s is dropped), or the document is saved, changed or deleted. The answer is the new number, how many times the document itself changed, and the others. A page that is told the document changed asks for it at once (its changes for sheets, its version for the rest). So an update reaches the others as fast as it is sent, and the 2 s polling is left as a safety net every 16 s while the held question works; when it fails (an older service, a network that cuts it) the page waits 3 s and asks again, and polls every 2 s as before.
* **Who people are.** A signed-in person shows under their user name. Anyone else gets a colour and an animal (`Teal Otter`, `Amber Heron`, 8 colours and 20 animals) made when the page opens, so a reload makes a new one. Two people with one name are told apart with a number on the screen (`Otter 2`).
* **What is drawn.** A strip of coloured initials next to the Cloud button lists who is here (hover for the name). sheets draw the other person's rectangle in their colour with the name on it, on the sheet they are on. docs mark the paragraph with a bar in the margin in their colour, because drawing a caret inside a paragraph needs the position of the text on screen, which `quench-ui` cannot measure. The paged view of docs does not mark paragraphs. notes and pdf show the strip only.
* **Rights.** Presence is read and sent with the same access as the document: the owner, a link (view or edit) or a share with the person.
* **Not covered.** Presence is not a lock: two people in one paragraph are only shown to each other.

## Replicas

The design assumes **one replica of the service**, which is how it runs (a local server; a single `lore-office` deployment). Three things rely on it:

| What | Why one replica is enough | What a second replica would need |
|---|---|---|
| **Order of deltas** | a mutex per document in the service gives each delta its number | warehouse has no compare-and-set on a file, so numbering needs one owner per document: route all of a document's requests to the same replica (consistent hashing on the document id, sticky routing at the ingress), or a store with real transactions (a database table of deltas) |
| **Presence** | kept in the service's memory | a shared store both replicas read and write (a small table, or a cache service); presence tolerates loss, so it needs no durability |
| **Autosave and polling** | each page talks to whichever replica the ingress picks, and each save is checked against the stored version | nothing extra for whole-document saves: the version check happens against warehouse's files on every request; the race between two replicas writing the same document at the same instant is the one a per-document owner removes |

Until then, scaling the deployment above one replica is not supported for documents that are being edited. Nothing breaks for reading, listing or opening, and the whole-document sync of today is safe with more replicas (it never relied on shared memory); it is the delta log and presence that are not.

## Build order

1. Whole-document sync with the faster timers and the cheaper save (done).
2. The delta log: sheets and docs (done).
3. Presence and the held question (done): who is here and where, and updates pushed as they happen.
4. Open: the same for notes (by note) and pdf (nothing to edit); more kinds of change in docs (pictures, tables, links) instead of the whole file.
