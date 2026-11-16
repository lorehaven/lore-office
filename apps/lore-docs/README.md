# lore-docs (spike)

A word processor in the browser, part of lore-office. A first complete version: type and format, tables, pictures, links, find and replace, a pages view, `.docx` / Markdown / text files, PDF export and print. Everything stays on your machine.

## Run

    cd apps/lore-docs
    quench build --release     # dist/lore-docs-server (module embedded) and dist/app.wasm
    PORT=3000 dist/lore-docs-server

## What it does

- **Editor.** The page is an editable element the browser may not change by itself: every typed character, deletion, paste, bold command and undo arrives as an event, is applied to the model (`docs-text`) and the changed paragraphs are drawn again. Typing (also accents and other scripts), Enter, Backspace and Delete (also by word), selecting across paragraphs, cut and paste, bold / italic / underline (Ctrl+B / I / U or the buttons), strikethrough, superscript, subscript and highlight, typeface (sans, serif, mono), size, text colour, alignment (left, centre, right, justify), Heading 1 to 3, bullets and numbered lists (Enter continues a list), undo and redo (Ctrl+Z, Ctrl+Y, Ctrl+Shift+Z; typing is one step). An input method (IME) composes inside the browser and is read back when it ends.
- **Pages.** `docs-print` lays the document out on A4 pages (72 pt margins, 12 pt text, page numbers at the foot): lines are broken with the PDF fonts' own widths, justified lines are stretched, list items are numbered (each run from 1), and the pages are written as a PDF. The screen is still one long sheet.

**Files.** File ▸ Open reads `.docx` (also by dropping a file on the page), `.txt` and `.md`; Save as `.docx` (Ctrl+S) or `.txt`, Export as PDF, and Print… (Ctrl+P: the pages open in a new tab, where the viewer prints them). A `.docx` that was opened is written back in its own package: styles, themes, headers, properties and page setup are kept, and only the body is replaced. A new document gets the heading and list styles it needs. LibreOffice opens the files it writes (headings, bold, italic, bullets). The document is kept in the browser's storage after each change and restored on the next visit.

## Limits

- The model holds paragraphs of runs (bold, italic, underline, strike, raised and lowered text, highlight, typeface, size, colour, links), headings 1 to 3, nested bullet and numbered items, alignment, indents, spacing and page breaks. Specific font names become one of three kinds (a file's "Calibri" is read as sans and written back as Arial); fonts, line spacing, indents, tabs, columns, styles, headers and footers are not in it. Tables (plain grids of up to 8 columns), pictures and links are; merged-cell and nested tables become tab-separated lines and pictures inside cells or drawn as shapes are dropped, and the status line says so. Saving such a file writes the simplified body.
- The PDF uses the standard PDF fonts; a string with characters they lack (Polish letters, Greek, Cyrillic...) is set in an embedded Noto Sans instead (sans only, bold and italic imitated, text stays searchable; CJK and the like show a box and add about 300 KB to the file). Pages are fixed to A4 for now; no page breaks of your own, no widow or orphan control, no headers or footers. Table columns are equal width and a cell is one line of text.
- Real input methods and phone keyboards are untested.

## Find and replace, pages view
- Ctrl+F (or Ctrl+H, or View ▸ Find and replace) opens a bar under the ribbon: find (Enter / Shift+Enter move between matches), replace with, Replace, Replace all (one undo step), Match case.
- View ▸ Pages shows the document as it prints (the same layout the PDF export uses, on white A4 sheets). It is read only; switch back to edit.

## Links
Select text, press Ctrl+K (or Home ▸ Insert ▸ Link), type an address (a bare `name@host` becomes `mailto:`, a bare host `https://`), Enter. Links are real `.docx` hyperlinks (checked with LibreOffice), show their address as a tooltip, and print blue and underlined.

## Tables
Insert ▸ Table (2×2 … 8×3) puts a table after the paragraph the caret is in; Insert ▸ Edit table adds or deletes rows and columns, or the whole table. A cell is one line of text: Tab / Shift+Tab move between cells (Tab in the last cell adds a row), Enter goes to the next cell, and nothing you type or delete joins or splits cells. Tables are real `.docx` tables (checked with LibreOffice) and print with borders; rows do not split across pages.
Tables in files you open are kept as tables when they are plain grids of up to 8 columns; merged cells, nested tables and wider tables are still flattened to tab-separated lines (and reported). A cell with several paragraphs becomes one, joined with spaces. Columns are always equal width.

## Pictures
Insert ▸ Picture… (or drop an image on the page) puts a PNG, JPEG, GIF or WebP picture in a paragraph of its own after the caret's paragraph, at its size (96 pixels to the inch) or the text width if that is less; WebP is kept as PNG. Click a picture to select it, then Insert ▸ Smaller / Larger; Backspace from the paragraph after it removes it, and centring/right-aligning the paragraph places the picture. Pictures are real `.docx` pictures (media parts, checked with LibreOffice), print and export to PDF, and show in the pages view. Pictures in files you open are kept when inline or anchored (an anchored one becomes a paragraph of its own); ones in table cells, in other formats (EMF, SVG) or drawn as shapes are still dropped and reported. The browser autosave keeps up to about 2.6 MB of file, so a large picture means saving to a file.

## Markdown and word count
File ▸ Save as Markdown writes headings, lists, bold, italic, strikethrough, links and pipe tables (pictures as `![image]`); opening a `.md` reads the same, plus code blocks and quotes, as a formatted document. View ▸ Word count says how many words and characters the document (or the selection) has. Links are real `.docx` hyperlinks and clickable boxes in the PDF export.

## Paragraphs, lists, pages
Home ▸ Spacing: indent more or less, line spacing (single, 1.15, 1.5, double); Insert ▸ Page break starts the paragraph on a new page. Lists nest: Tab and Shift+Tab move a list item in and out (nine levels; numbers go 1. a. i. 1. ..., bullets • ◦ ▪). Shift+Enter breaks the line inside a paragraph. View ▸ Header and footer sets text for the top and foot of every page (`{page}` and `{pages}` are the numbers; `|` splits lines), View ▸ A4 / Letter picks the paper, View ▸ Zoom scales the page. Typeface names from a file (Calibri, Georgia...) are kept when it is saved again, though the editor shows them in a similar sans, serif or mono face. All of this is in the `.docx` as real paragraph properties, numbering levels, headers and footers (checked by opening files in LibreOffice).
Tables also have column widths (Insert ▸ Edit table ▸ Wider / Narrower column) and a header row (bold, and set again at the top of each page a table runs over). A table with merged cells opens as a grid with the merged cells split, the text in the first of them (the status line says it was simplified).
File ▸ Save as web page (.html) writes the document, with its pictures, as one standalone page.
