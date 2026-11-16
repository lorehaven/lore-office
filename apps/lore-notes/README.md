# lore-notes

Markdown notes in the browser, part of lore-office. A vault of plain notes with a source editor and live preview, links between notes, and any note presented as a deck. Everything stays on your machine: no account, no sync.

## Run

    cd apps/lore-notes
    quench dev                 # development, reloads on change
    quench build --release     # dist/lore-notes-server (module embedded) and dist/app.wasm
    PORT=3000 dist/lore-notes-server

## What it does

**Notes.** A list of notes with search (names, titles and text) and tags; a source editor and a live preview side by side (or either alone); CommonMark with tables, strikethrough, task lists, footnotes, math (typeset) and callouts (`> [!tip] Title`). Raw HTML in a note is never put on the page. Links between notes are `[[Note]]`, `[[Note#Heading]]` or `[[Note|alias]]`: a click opens the note (or makes it when it does not exist), the right-hand panel lists backlinks, the links of this note and those not written yet; renaming a note points the links of the others at the new name. A new vault starts with a short Welcome note.

**Decks.** Any note is a deck: slides are separated by a line of `---`, speaker notes follow a `???` or `Note:` line, `<!-- layout: two-col -->` (also `center`, `title`) picks a layout. Front matter sets `theme:` (default, light, dark, night, paper, contrast) and `aspect:` (16:9, 16:10, 4:3, 1:1); the Deck tab sets them for you. Present (F5, or a slide in the right-hand list): arrow keys, space and page keys move, Home and End jump, N shows the speaker notes and the next slide, Escape leaves; there is a timer.

**Editing.** The ribbon's Format tools (bold, italic, code, link, heading, quote, list) work on the selection or the lines the caret is in, and the Insert tab puts tables, code blocks, callouts, math and slide breaks at the caret.

**Files.** Open `.md` and `.txt` files (also by dropping them on the page) as notes; save the open note as `.md` (Ctrl+S) or the whole vault as a `.zip`; export a note as a standalone web page, a deck as a standalone slide show (arrow keys, no network), or a deck as a PDF with a page for each slide (`notes-pdf`: the stage's size and the deck's theme colours (Deck ▸ Theme: `default` follows the app on screen and prints light; `light` and `dark` are fixed on both; also night, paper, high contrast); text in the standard PDF fonts, and any string with characters they lack (Polish letters, Greek, Cyrillic...) in an embedded Noto Sans (sans only, bold and italic imitated; characters Noto lacks, such as CJK, show a box); `two-col` is set in one column).

**Look and language.** Light, dark or the system's, six accent palettes (`office-theme`), and five languages (English, German, Spanish, French, Polish) picked at the top right; both are kept in the browser. The vault is kept in the browser's storage after every change.

## Keys

| Key | |
|---|---|
| Ctrl+O | go to a note by name |
| Alt+N | new note |
| Ctrl+S | save the note as `.md` |
| F5 | present |
| Ctrl+B / Ctrl+I | bold / italic around the selection (again takes it off) |
| Tab / Shift+Tab | indent / outdent the lines |

## Limits

- The editor is a plain text field: no syntax colouring, no auto-continue of lists, no multi-cursor.
- The vault lives in the browser's storage (about 5 MB); folders on disk need the File System Access API and are not used yet.
- Math is typeset in the app (and the deck), but exports (web page, PDF) still carry its TeX. Images: only `data:` and web addresses; `![[file]]` embeds show a placeholder.
- No second-window audience view. A slide with `<!-- fragments -->` shows its list items one at a time (Right/Space); this is for the presenter only, not in the exported slide show or PDF.
