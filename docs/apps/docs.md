---
title: docs
parent: Apps
nav_order: 1
---

A word processor. The page is an editable element the browser may not change by itself: every typed character, deletion, paste and formatting command arrives as an event, is applied to the model (`docs-text`) and the changed paragraphs are drawn again.

## Editing

Typing in any script, Enter, Backspace and Delete (also by word), selecting across paragraphs, cut and paste; bold, italic, underline, strikethrough, superscript, subscript and highlight; typeface, size and text colour; alignment (left, centre, right, justify); Heading 1 to 3; bulleted and numbered lists (Enter continues one, Tab nests); indents and line spacing; links (Ctrl+K); tables with their rows, columns, widths and header row; pictures; page breaks; undo and redo (Ctrl+Z, Ctrl+Y, Ctrl+Shift+Z; typing is one step); find and replace (Ctrl+F, Ctrl+H).

The Typeface list has the three kinds (Sans, Serif, Mono) and 85 named typefaces in three groups. A name is drawn by the computer's own font of that name, else by a free font with the same widths where there is one (for example Liberation Sans for Arial), else by the kind. PDF and print use Helvetica, Times and Courier for all of them.

## Pages

The page is the paper the document is set for: A4 by default; the **Paper** menu (View tab, Page group) offers A4, A3, A5, US Letter and US Legal. The size is saved in `.docx` files. The editing view shows where each sheet ends: a foot margin, a band of the desk, the head margin of the next sheet. The break positions come from the layout that prints (`docs-print`), so a paragraph that would cross a page moves whole to the next sheet on screen while the PDF splits it by line. View ▸ Pages shows the document as it prints, read only. A header and a footer (Header and footer) apply to every page; `{page}` and `{pages}` stand for the numbers. Zoom sets the size of the page on screen.

## Files

| | |
|---|---|
| open | `.docx`, `.txt`, `.md` (also by dropping the file) |
| save | Word document (`.docx`, Ctrl+S), web page (`.html`), Markdown (`.md`), plain text (`.txt`) |
| export | PDF; Print… (Ctrl+P) opens the pages in a new tab |

An opened `.docx` is written back in its own package: styles, themes, headers, properties and page setup are kept, and only the body is replaced. Content the model does not hold is simplified, and the status line says so.

## The page in light colours

The page icon next to the theme menu shows the page in light colours while the rest of the screen keeps its theme.
