---
title: notes
parent: Apps
nav_order: 3
---

Markdown notes with slide decks. A vault of plain notes, a source editor and a live preview side by side (or either alone), links between notes, and any note presented as a deck.

## Notes

A list of notes with search (names, titles and text) and tags. CommonMark with tables, strikethrough, task lists, footnotes, math (typeset) and callouts (`> [!tip] Title`); raw HTML in a note is never put on the page. Links between notes are `[[Note]]`, `[[Note#Heading]]` or `[[Note|alias]]`: a click opens the note or makes it; the right-hand panel lists backlinks, the links of the note and those not written yet; renaming a note points the links of the others at the new name. The ribbon has the Home, Insert and Deck tabs.

## Decks

Any note is a deck: slides are separated by a line of `---`, speaker notes follow a `???` or `Note:` line, and `<!-- layout: two-col -->` (also `center`, `title`) picks a layout. Front matter sets `theme:` (default, light, dark, night, paper, contrast) and `aspect:` (16:9, 16:10, 4:3, 1:1); the Deck tab sets them for you. Present with F5: arrow keys, space and page keys move, Home and End jump, N shows the speaker notes and the next slide, Escape leaves.

## Files

| | |
|---|---|
| open | `.md`, `.txt` (also by dropping the files), as notes |
| save | the open note as `.md` (Ctrl+S), all notes as a `.zip` |
| export | a note as a web page (`.html`), a deck as a slide show (`.html`, no network needed), a deck as a PDF with a page for each slide |

The vault is kept in the browser's storage after every change.
