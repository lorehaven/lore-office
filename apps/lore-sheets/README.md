# lore-sheets

Spreadsheet in the browser, part of lore-office. Everything stays on your machine: no account, no sync.

## Run

    cd apps/lore-sheets
    quench dev                 # development, reloads on change
    quench build --release     # dist/lore-sheets-server (module embedded) and dist/app.wasm
    PORT=3000 dist/lore-sheets-server

## What it does

**Files.** Opens and saves `.xlsx` (an existing file is patched, not rewritten: parts we do not model are kept), `.csv` and `.tsv`; opens and saves OpenDocument `.ods`; opens old `.xls`. Export to PDF and print (page setup, print area, repeated heading rows). Drag a file onto the page to open it. A big file shows "Opening…" before it is read. The workbook is kept in the browser's storage after each change and restored on the next visit (up to about 2.6 MB; bigger ones must be saved).

**Editing.** Undo and redo, copy/cut/paste (also with other spreadsheets) and Paste special, fill by dragging the handle, find and replace across sheets (also regular expressions), insert and delete rows and columns, hide, group and outline, freeze panes, merged cells, borders, fonts and sizes, number formats, conditional formats, data validation (drop-down lists), notes, named ranges with completion, sheet tabs (double-click renames), sheet protection with optional password and locked/unlocked cells.

**Data.** Sort, filters with value lists, and **tables**: Ctrl+T (or Data ▸ Table, or the right-click menu) turns the cells into a named table with sort and filter arrows in its heading, banded rows and a look of your choice (plain, six colours, or your own three); typing under a table extends it; saved as real `.xlsx` tables. Summary (pivot) tables, charts and sparklines.

**Formulas.** About 180 functions (math, statistics, text, dates, lookup, financial, and `FILTER`, `SORT`, `UNIQUE`, `SEQUENCE`, `XLOOKUP`, `LET`, `INDIRECT`, `OFFSET` with spilling results), incremental recalculation, circular reference detection, `NOW`/`TODAY`/`RAND` in local time. A function we do not have shows the result the file stored, until the cell is edited.

**Menus.** A ribbon of two lines: File and the tabs on top, the tools of the chosen tab under them. Right-click on cells, or on a row or column heading, for a menu of what can be done with them.

**Look and language.** Light, dark or the system's, six accent palettes (`office-theme`), and five languages (English, German, Spanish, French, Polish) picked at the top right; the choice is kept in the browser.

## Keys

| Key | |
|---|---|
| Ctrl+Z / Y | undo / redo |
| Ctrl+B / I / U | bold / italic / underline |
| Ctrl+S / O / P | save `.xlsx` / open / print |
| Ctrl+T | make a table |
| Ctrl+F / H | find / replace (F3 and Shift+F3 step through matches) |
| Ctrl+D / R | fill down / right |
| Ctrl++ / Ctrl+- | insert / delete rows or columns |
| Ctrl+arrows | jump to the edge of a block (with Shift: select) |
| Ctrl+Home / End | first cell / last used cell |
| F2 | edit the cell |

## Limits

- `.ods` and `.xls` do not keep charts, validation, conditional formats or notes; an `.xls` formula becomes its value, and `.xls` cannot be saved.
- `LET` is written without the `_xlpm` prefixes; sparklines are not printed; modern password hashes are kept but cannot be checked.
- Formulas cannot refer to a table by name (`Table1[Qty]`) and tables have no totals row.
- Texts made of a number or a cell reference (status lines with a count, for example) are in English.
