# lore-pdf (first version)

Reads the **text** of a PDF in the browser, part of lore-office. Nothing leaves your machine.

    cd apps/lore-pdf && quench build --release && PORT=3000 dist/lore-pdf-server

- Open a PDF with File ▸ Open (or drop it on the page). The pages are listed on the left (page number and first line); the text of the page is shown at a zoom of 60 % to 250 % (Ctrl+plus, Ctrl+minus). Arrow keys, Page Up/Down, Home and End move between pages; a page number can be typed in.
- Search looks in every page (case does not matter), highlights the matches on the page, and steps through them (F3, Shift+F3), wrapping round.
- File ▸ Open in the browser's viewer hands the file to the browser's own PDF viewer in a new tab, which **draws the pages** (pictures, layout, forms).

## Limits

- Pages are drawn by an SVG renderer (see below), not from embedded font programs; shadings, patterns and annotations other than links are not drawn. Text that is a picture (a scan) is not text: it cannot be selected or searched.
- Encrypted files are refused. Text selection and copying work as in any web page, but not by page layout.
- Five languages (English, German, Spanish, French, Polish), picked at the top right.

## Pages as drawn
Pages are drawn by `quench-pdf` (`Document::render_page`: an SVG per page with paths, fills and strokes, clips, dashes, transparency, text, images with soft masks, forms and rotation) and shown as pictures at the zoom you pick. "Text only" in the ribbon (View) switches back to the extracted text, which is what search highlights. Text is set in the viewer's own fonts (Liberation/Arial/Times/Courier chosen by name and flags) with each glyph placed where the file puts it, so layout is faithful but letter shapes are those of a similar font. Not drawn: shadings and patterns, type 3 fonts, inline images, annotations, form fields. Invisible text (a scan's text layer) is skipped. Needs the `quench-pdf` in `../quench` (patched in `.cargo/config.toml` until it is released).

Over each drawn page lies its text (invisible, placed where the file puts it), so text can be selected and copied from the picture and a search marks the lines that hold it (the current match in orange). Text in a font the file gives no widths for (the standard fonts of older files) is flowed by the browser in a matching font, so spacing is right even then; the selection boxes of such lines are approximate.

## Outline, links and page operations
The file's own outline (bookmarks) fills the sidebar (else the first line of each page); links on pages (web, mail and to other pages) are clickable boxes over the drawing. The "Edit pages" tab turns a page a quarter turn, moves it earlier or later, deletes it, adds the pages of another PDF, and saves a copy of the file or only the page shown (`pdf-edit`: the pages' content, fonts and pictures are copied byte for byte; the outline, form fields and metadata are not carried over, so a saved copy has none).

## Marking up and filling in
The "Annotate" tab marks text: choose Highlight, Underline, Strike out or Note and click a line of text (a note takes the words typed in the box beside the buttons); "Type" puts words after a line (a form's blank follows its label), in the embedded Noto Sans, so Polish and other letters work. Markup is saved as real PDF annotations (highlights, underlines, strike-outs, notes: other viewers show them; checked with poppler) and typed words are put on the page itself; Undo takes back the last change, "Save a copy" saves the file. Dark pages turns the paper dark. This is not form-field filling: a file's own fields are not edited, and its outline, form fields and metadata are not carried into the saved copy.
