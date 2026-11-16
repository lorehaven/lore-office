---
title: sheets
parent: Apps
nav_order: 2
---

A spreadsheet. The grid is a native scroll area over a box as big as the sheet; one CSS grid holds the visible rows and columns, so a large sheet stays fast.

## Files

| | |
|---|---|
| open | `.xlsx`, `.ods`, `.xls`, `.csv`, `.tsv` (also by dropping the file) |
| save | Excel workbook (`.xlsx`), OpenDocument spreadsheet (`.ods`), CSV or tab separated for the current sheet, PDF |
| print | Page setup…, Print… |

An opened `.xlsx` is patched, not rewritten: the parts the model does not hold are kept.

## Editing

Undo and redo; copy, cut and paste (also with other spreadsheets) and Paste special (values only, formulas without formats, formats only, transposed); fill by dragging the handle; find and replace across sheets; insert and delete rows and columns; hide, group, freeze panes; merged cells; borders, fonts, number formats, conditional formats; data validation with drop-down lists; named ranges; sort and filters; tables (Ctrl+T); charts. Dragging a selection past the edge of the grid keeps growing it and scrolls the grid after it. Right-click on cells, or on a row or column heading, for a menu of what can be done with them.

## Formulas

About 180 functions (math, statistics, text, dates, lookup, financial, and `FILTER`, `SORT`, `UNIQUE`, `SEQUENCE`, `XLOOKUP`, `LET`, `INDIRECT`, `OFFSET` with spilling results), incremental recalculation, circular reference detection. A function the app does not have shows the result the file stored until the cell is edited.

## The sheet in light colours

The page icon next to the theme menu shows the grid and its headings in light colours while the rest of the screen keeps its theme.
