//! The document, the selection, and what each input event does to them.

use crate::i18n::tr;
use docs_text::{Align, Doc, Family, Kind, Mark, Pos, Style};
use office_fmt::Package;
use quench_ui::platform::{self, PickedFile};
use quench_ui::prelude::*;
use quench_ui::{BeforeInputEvent, BlockPoint};
use std::cell::RefCell;
use std::rc::Rc;

/// A selection: where it started and where the caret is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Range {
    pub anchor: Pos,
    pub focus: Pos,
}

impl Range {
    /// The two ends in document order.
    #[must_use]
    pub fn ordered(self) -> (Pos, Pos) {
        if self.anchor <= self.focus {
            (self.anchor, self.focus)
        } else {
            (self.focus, self.anchor)
        }
    }
}

const fn pos_of(p: BlockPoint) -> Pos {
    Pos {
        para: p.block,
        offset: p.offset,
    }
}

const MAX_UNDO: usize = 200;

#[derive(Clone, Default)]
struct History {
    undo: Vec<(Doc, Range)>,
    redo: Vec<(Doc, Range)>,
    /// The kind of the last change, so typing is one step.
    last: &'static str,
}

/// The document and everything around it.
/// The width of the text of a page: 6 inches, in points.
const MAX_PICTURE_WIDTH: f64 = 432.0;

#[derive(Clone)]
pub struct DocState {
    doc: Rc<RefCell<Doc>>,
    history: Rc<RefCell<History>>,
    /// Bumped when the document changes.
    pub rev: Signal<u32>,
    /// Where the selection is, as the browser reported it.
    pub selection: Signal<Range>,
    /// Where the browser is to put the selection after the paragraphs are drawn (`a,b,c,d,n`; `n` makes it new).
    pub restore: Signal<String>,
    pub status: Signal<String>,
    /// The language of the screen (a locale code); the screen is drawn again when it changes.
    pub language: Signal<String>,
    /// Whether the find bar is open, and what it holds.
    pub find_open: Signal<bool>,
    pub find_query: Signal<String>,
    pub replace_with: Signal<String>,
    pub match_case: Signal<bool>,
    /// Whether the link bar is open, and the address in it.
    pub link_open: Signal<bool>,
    pub link_url: Signal<String>,
    /// The zoom of the page, in percent.
    pub zoom: Signal<u32>,
    /// Whether the header and footer bar is open.
    pub hf_open: Signal<bool>,
    /// Whether the pages (the document as it prints) are shown instead of the editable text.
    pub pages_view: Signal<bool>,
    /// The file name shown on the ribbon.
    pub name: Signal<String>,
    package: Rc<RefCell<Package>>,
    changed: Rc<RefCell<Option<Rc<dyn Fn()>>>>,
}

impl std::fmt::Debug for DocState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DocState").finish_non_exhaustive()
    }
}

impl DocState {
    #[must_use]
    pub fn new(doc: Doc) -> Self {
        crate::i18n::reset();
        Self {
            doc: Rc::new(RefCell::new(doc)),
            history: Rc::default(),
            rev: create_signal(0),
            selection: create_signal(Range::default()),
            restore: create_signal(String::new()),
            status: create_signal(String::new()),
            language: create_signal(crate::i18n::language().to_string()),
            find_open: create_signal(false),
            find_query: create_signal(String::new()),
            replace_with: create_signal(String::new()),
            match_case: create_signal(false),
            link_open: create_signal(false),
            link_url: create_signal(String::new()),
            zoom: create_signal(100),
            hf_open: create_signal(false),
            pages_view: create_signal(false),
            name: create_signal("Untitled.docx".to_string()),
            package: Rc::default(),
            changed: Rc::default(),
        }
    }

    #[must_use]
    pub fn doc(&self) -> Doc {
        self.doc.borrow().clone()
    }

    /// Run `f` on the document without copying it.
    pub fn with_doc<R>(&self, f: impl FnOnce(&Doc) -> R) -> R {
        f(&self.doc.borrow())
    }

    fn put_caret(&self, range: Range) {
        let n = self.rev.get_untracked();
        batch(|| {
            self.selection.set(range);
            self.restore.set(format!(
                "{},{},{},{},{n}",
                range.anchor.para, range.anchor.offset, range.focus.para, range.focus.offset
            ));
        });
    }

    fn bump(&self) {
        self.rev.set(self.rev.get_untracked().wrapping_add(1));
        if let Some(f) = self.changed.borrow().clone() {
            f();
        }
    }

    /// Call `f` after every change to the document, so the host can keep it.
    pub fn on_change(&self, f: impl Fn() + 'static) {
        *self.changed.borrow_mut() = Some(Rc::new(f));
    }

    /// Switch the language of the screen and tell the app, which keeps it.
    pub fn set_language(&self, code: &str) {
        let now = crate::i18n::use_language(code);
        self.language.set_if_changed(now.to_string());
        crate::i18n::announce(now);
    }

    // ---- files ----

    fn replace_document(&self, doc: Doc, package: Package, name: &str) {
        *self.doc.borrow_mut() = doc;
        *self.package.borrow_mut() = package;
        *self.history.borrow_mut() = History::default();
        batch(|| {
            self.name.set(name.to_string());
            self.selection.set(Range::default());
            self.bump();
        });
    }

    /// A new, empty document.
    pub fn new_document(&self) {
        self.replace_document(Doc::new(), Package::new(), "Untitled.docx");
        self.put_caret(Range::default());
        self.status.set(String::new());
    }

    /// Read a file's bytes as a document: `.docx`, or plain text (`.txt`, `.md`). Returns what went wrong, if anything.
    ///
    /// # Errors
    ///
    /// A message for the person.
    pub fn load_bytes(&self, name: &str, bytes: &[u8]) -> Result<(), String> {
        let extension = name
            .rsplit_once('.')
            .map(|(_, e)| e.to_lowercase())
            .unwrap_or_default();
        if extension == "docx" {
            let loaded = docs_docx::read(bytes).map_err(|e| e.to_string())?;
            let note = if loaded.report.is_lossy() {
                format!(
                    "{} {name}; {}: {} {}, {} {}",
                    tr("Opened"),
                    tr("simplified"),
                    loaded.report.tables,
                    tr("table(s)"),
                    loaded.report.pictures,
                    tr("picture(s)")
                )
            } else {
                // The name is in the top bar: there is nothing to add.
                String::new()
            };
            self.replace_document(loaded.doc, loaded.package, name);
            self.status.set(note);
            Ok(())
        } else if extension == "md" {
            let stem = name.rsplit_once('.').map_or(name, |(s, _)| s);
            self.replace_document(
                docs_md::from_markdown(&String::from_utf8_lossy(bytes)),
                Package::new(),
                &format!("{stem}.docx"),
            );
            self.status.set(String::new());
            Ok(())
        } else if extension == "txt" {
            let stem = name.rsplit_once('.').map_or(name, |(s, _)| s);
            self.replace_document(
                Doc::from_text(&String::from_utf8_lossy(bytes)),
                Package::new(),
                &format!("{stem}.docx"),
            );
            self.status.set(String::new());
            Ok(())
        } else {
            Err(tr("Only .docx, .txt and .md files can be opened"))
        }
    }

    /// Open the first file picked or dropped.
    pub fn load_files(&self, files: &[PickedFile]) {
        // Pictures dropped on the page go into the document; any other file is opened.
        let lower = files
            .first()
            .map(|f| f.name.to_lowercase())
            .unwrap_or_default();
        if [".png", ".jpg", ".jpeg", ".gif", ".webp"]
            .iter()
            .any(|e| lower.ends_with(e))
        {
            for f in files {
                if let Err(e) = self.insert_picture(&f.bytes) {
                    self.status
                        .set(format!("{} {}: {e}", tr("Could not open"), f.name));
                }
            }
            return;
        }
        if let Some(f) = files.first()
            && let Err(e) = self.load_bytes(&f.name, &f.bytes)
        {
            self.status
                .set(format!("{} {}: {e}", tr("Could not open"), f.name));
        }
    }

    /// Ask for a file to open. Call it from a click.
    pub fn open_file(&self) {
        let state = self.clone();
        platform::pick_files(".docx,.txt,.md", false, move |files| {
            state.load_files(&files)
        });
    }

    /// Open files dropped on the page.
    pub fn accept_drops(&self) {
        let state = self.clone();
        platform::on_drop(move |files| state.load_files(&files));
    }

    /// The document as `.docx` bytes (in the package it came in, when it came from a file).
    ///
    /// # Errors
    ///
    /// A message for the person.
    pub fn docx_bytes(&self) -> Result<Vec<u8>, String> {
        docs_docx::write(&self.doc.borrow(), &self.package.borrow()).map_err(|e| e.to_string())
    }

    fn stem(&self) -> String {
        let name = self.name.get_untracked();
        name.rsplit_once('.')
            .map_or_else(|| name.clone(), |(s, _)| s.to_string())
    }

    /// Offer the document as a `.docx` to save.
    pub fn save_docx(&self) {
        match self.docx_bytes() {
            Ok(bytes) => {
                let name = format!("{}.docx", self.stem());
                platform::download(
                    &name,
                    "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
                    &bytes,
                );
                self.name.set(name.clone());
                self.status.set(format!("{} {name}", tr("Saved")));
            }
            Err(e) => self.status.set(format!("{}: {e}", tr("Could not save"))),
        }
    }

    /// Offer the document as a PDF to save.
    pub fn export_pdf(&self) {
        let bytes = docs_print::to_pdf(
            &self.doc.borrow(),
            &docs_print::Setup::for_doc(&self.doc.borrow()),
            &self.stem(),
        );
        let name = format!("{}.pdf", self.stem());
        platform::download(&name, "application/pdf", &bytes);
        self.status.set(format!("{} {name}", tr("Saved")));
    }

    /// Show the document as pages in a new tab, where the viewer's print button prints it. Call it from a click.
    pub fn print(&self) {
        let bytes = docs_print::to_pdf(
            &self.doc.borrow(),
            &docs_print::Setup::for_doc(&self.doc.borrow()),
            &self.stem(),
        );
        platform::open_in_tab("application/pdf", &bytes);
        self.status
            .set("Opened the pages in a new tab: print them from there".to_string());
    }

    /// Offer the text as a `.txt` to save.
    pub fn save_txt(&self) {
        let name = format!("{}.txt", self.stem());
        platform::download(
            &name,
            "text/plain;charset=utf-8",
            self.doc.borrow().plain().as_bytes(),
        );
        self.status.set(format!("{} {name}", tr("Saved")));
    }

    /// Offer the document as Markdown to save.
    pub fn save_md(&self) {
        let name = format!("{}.md", self.stem());
        platform::download(
            &name,
            "text/markdown;charset=utf-8",
            docs_md::to_markdown(&self.doc.borrow()).as_bytes(),
        );
        self.status.set(format!("{} {name}", tr("Saved")));
    }

    /// Offer the document as a web page to save.
    pub fn save_html(&self) {
        let name = format!("{}.html", self.stem());
        platform::download(
            &name,
            "text/html;charset=utf-8",
            docs_md::to_html(&self.doc.borrow(), &self.stem()).as_bytes(),
        );
        self.status.set(format!("{} {name}", tr("Saved")));
    }

    /// Say how many words and characters the document has (the selection, when there is one).
    pub fn count_words(&self) {
        let (a, b) = self.selection.get_untracked().ordered();
        let doc = self.doc.borrow();
        let (text, of) = if a == b {
            (doc.plain(), "")
        } else {
            (doc.slice(a, b), "selection")
        };
        let words = text.split_whitespace().count();
        let chars = text.chars().filter(|c| !c.is_whitespace()).count();
        let note = format!("{words} {}, {chars} {}", tr("words"), tr("characters"));
        self.status.set(if of.is_empty() {
            note
        } else {
            format!("{} {note}", tr("Selection:"))
        });
    }

    fn snapshot(&self, kind: &'static str) {
        let mut h = self.history.borrow_mut();
        // Typing one thing after another is one undo step.
        if !(kind == "type" && h.last == "type") {
            let (doc, sel) = (self.doc.borrow().clone(), self.selection.get_untracked());
            h.undo.push((doc, sel));
            if h.undo.len() > MAX_UNDO {
                h.undo.remove(0);
            }
        }
        h.redo.clear();
        h.last = kind;
    }

    /// The browser reported its selection.
    pub fn select(&self, range: Range) {
        self.selection.set_if_changed(range);
    }

    /// Apply one change the browser was about to make.
    pub fn before_input(&self, e: &BeforeInputEvent) {
        let (start, end) = (pos_of(e.start), pos_of(e.end));
        match e.input_type.as_str() {
            "insertText"
            | "insertReplacementText"
            | "insertFromPaste"
            | "insertFromPasteAsQuotation"
            | "insertFromDrop"
            | "insertFromYank"
            | "insertTranspose" => {
                self.snapshot(if e.input_type == "insertText" && !e.data.contains('\n') {
                    "type"
                } else {
                    "paste"
                });
                let caret = self.doc.borrow_mut().replace(start, end, &e.data);
                self.finish(caret);
            }
            // Shift+Enter breaks the line inside the paragraph.
            "insertLineBreak" => {
                self.snapshot("break");
                let caret =
                    self.doc
                        .borrow_mut()
                        .replace(start, end, &docs_text::LINE_BREAK.to_string());
                self.finish(caret);
            }
            // In a table Enter goes on to the next cell.
            "insertParagraph" if self.doc.borrow().cell_at(start.para).is_some() => {
                self.cell_move(false);
            }
            "insertParagraph" => {
                self.snapshot("break");
                let caret = self.doc.borrow_mut().replace(start, end, "\n");
                self.finish(caret);
            }
            t if t.starts_with("delete") => {
                self.snapshot("delete");
                let caret = self.doc.borrow_mut().delete(start, end);
                self.finish(caret);
            }
            "formatBold" => self.toggle(
                Mark::Bold,
                Range {
                    anchor: start,
                    focus: end,
                },
            ),
            "formatItalic" => self.toggle(
                Mark::Italic,
                Range {
                    anchor: start,
                    focus: end,
                },
            ),
            "formatUnderline" => self.toggle(
                Mark::Underline,
                Range {
                    anchor: start,
                    focus: end,
                },
            ),
            "historyUndo" => self.undo(),
            "historyRedo" => self.redo(),
            // An input method finished: the browser has changed the paragraph already, so the model takes its text.
            "compositionEnd" => {
                self.snapshot("compose");
                self.doc.borrow_mut().set_para_text(e.start.block, &e.data);
                let caret = Pos {
                    para: e.start.block,
                    offset: self.doc.borrow().len_of(e.start.block),
                };
                self.finish(caret);
            }
            other => self.status.set(format!("{}: {other}", tr("Not handled"))),
        }
    }

    fn finish(&self, caret: Pos) {
        batch(|| {
            self.bump();
            self.put_caret(Range {
                anchor: caret,
                focus: caret,
            });
        });
    }

    /// Turn a mark on or off over the selection (or the range given).
    pub fn toggle(&self, mark: Mark, range: Range) {
        let (a, b) = range.ordered();
        if a == b {
            return;
        }
        self.snapshot("format");
        self.doc.borrow_mut().toggle(a, b, mark);
        batch(|| {
            self.bump();
            self.put_caret(range);
        });
    }

    /// Toggle a mark over what is selected now.
    pub fn toggle_selected(&self, mark: Mark) {
        self.toggle(mark, self.selection.get_untracked());
    }

    /// Whether everything selected has the mark (for the buttons).
    #[must_use]
    pub fn marked(&self, mark: Mark) -> bool {
        let (a, b) = self.selection.get().ordered();
        let doc = self.doc.borrow();
        if a == b {
            doc.style_at(a).has(mark)
        } else {
            doc.all_marked(a, b, mark)
        }
    }

    // ---- zoom ----

    /// Zoom in (`1`) or out (`-1`) one step; `0` goes back to 100 percent.
    pub fn zoom_by(&self, step: i32) {
        const STEPS: [u32; 9] = [50, 67, 80, 90, 100, 110, 125, 150, 200];
        let now = self.zoom.get_untracked();
        let at = STEPS.iter().position(|z| *z >= now).unwrap_or(4);
        let to = if step == 0 {
            4
        } else {
            usize::try_from((i32::try_from(at).unwrap_or(4) + step).clamp(0, 8)).unwrap_or(4)
        };
        self.zoom.set(STEPS[to]);
    }

    // ---- page setup ----

    /// Set the text at the top of every page.
    pub fn set_header(&self, text: &str) {
        if self.doc.borrow().header() != text {
            self.snapshot("header");
            self.doc.borrow_mut().set_header(text);
            self.bump();
        }
    }

    /// Set the text at the foot of every page.
    pub fn set_footer(&self, text: &str) {
        if self.doc.borrow().footer() != text {
            self.snapshot("footer");
            self.doc.borrow_mut().set_footer(text);
            self.bump();
        }
    }

    /// Set the document for a paper size (the pages are laid out again on it).
    pub fn set_paper(&self, size: docs_text::PaperSize) {
        if self.doc.borrow().paper() == size {
            return;
        }
        self.snapshot("paper");
        self.doc.borrow_mut().set_paper(size);
        self.bump();
        self.status.set(format!("{} {}", tr("Paper:"), size.name()));
    }

    // ---- find and replace ----

    /// Every place the text being looked for is, in order, as (start, end) positions.
    #[must_use]
    pub fn matches(&self) -> Vec<(Pos, Pos)> {
        let query: Vec<char> = self.find_query.get_untracked().chars().collect();
        if query.is_empty() {
            return Vec::new();
        }
        let case = self.match_case.get_untracked();
        let same = |a: char, b: char| a == b || (!case && a.to_lowercase().eq(b.to_lowercase()));
        let doc = self.doc.borrow();
        let mut out = Vec::new();
        for para in 0..doc.paras().len() {
            let chars: Vec<char> = doc.text(para).chars().collect();
            // The UTF-16 offset of each character.
            let mut at = Vec::with_capacity(chars.len() + 1);
            let mut units = 0;
            for c in &chars {
                at.push(units);
                units += c.len_utf16();
            }
            at.push(units);
            let mut i = 0;
            while i + query.len() <= chars.len() {
                if query.iter().zip(&chars[i..]).all(|(a, b)| same(*a, *b)) {
                    out.push((Pos::new(para, at[i]), Pos::new(para, at[i + query.len()])));
                    i += query.len();
                } else {
                    i += 1;
                }
            }
        }
        out
    }

    /// Select the next (or with `back`, previous) match after the selection, wrapping round.
    pub fn find_next(&self, back: bool) {
        let all = self.matches();
        if all.is_empty() {
            self.status.set(tr("Not found"));
            return;
        }
        let (a, b) = self.selection.get_untracked().ordered();
        let pick = if back {
            all.iter().rev().find(|m| m.0 < a).or_else(|| all.last())
        } else {
            all.iter().find(|m| m.0 >= b).or_else(|| all.first())
        };
        if let Some(&(from, to)) = pick {
            self.put_caret(Range {
                anchor: from,
                focus: to,
            });
            self.status.set(format!("{} {}", all.len(), tr("matches")));
        }
    }

    /// Replace the selected match (if the selection is one) and move on to the next.
    pub fn replace_current(&self) {
        let (a, b) = self.selection.get_untracked().ordered();
        if !self.matches().iter().any(|m| m.0 == a && m.1 == b) {
            self.find_next(false);
            return;
        }
        self.snapshot("replace");
        let with = self.replace_with.get_untracked();
        let caret = self.doc.borrow_mut().replace(a, b, &with);
        batch(|| {
            self.bump();
            self.put_caret(Range {
                anchor: caret,
                focus: caret,
            });
        });
        self.find_next(false);
    }

    /// Replace every match; one undo step.
    pub fn replace_all(&self) {
        let all = self.matches();
        if all.is_empty() {
            self.status.set(tr("Not found"));
            return;
        }
        self.snapshot("replace");
        let with = self.replace_with.get_untracked();
        for (from, to) in all.iter().rev() {
            self.doc.borrow_mut().replace(*from, *to, &with);
        }
        self.bump();
        self.status.set(format!("{} {}", all.len(), tr("replaced")));
    }

    // ---- pictures ----

    /// Put the caret at the start of the paragraph with this `id` (a click on a picture, which has no text to click in).
    pub fn focus_paragraph(&self, id: u32) {
        let at = self.doc.borrow().paras().iter().position(|p| p.id == id);
        if let Some(para) = at {
            self.put_caret(Range {
                anchor: Pos::new(para, 0),
                focus: Pos::new(para, 0),
            });
        }
    }

    /// Ask for a picture to put after the paragraph the caret is in. Call it from a click.
    pub fn open_picture(&self) {
        let state = self.clone();
        platform::pick_files(".png,.jpg,.jpeg,.gif,.webp", false, move |files| {
            if let Some(f) = files.first()
                && let Err(e) = state.insert_picture(&f.bytes)
            {
                state
                    .status
                    .set(format!("{} {}: {e}", tr("Could not open"), f.name));
            }
        });
    }

    /// Put a picture (a PNG, JPEG, GIF or WebP file) in a paragraph of its own after the one the caret is in, as wide as its
    /// pixels at 96 to the inch, or the width of the text if that is less.
    ///
    /// # Errors
    ///
    /// What is wrong with the file, for the person.
    pub fn insert_picture(&self, bytes: &[u8]) -> Result<(), String> {
        let px = quench_image::decode(bytes).map_err(|e| e.to_string())?;
        let (data, mime) = if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
            (bytes.to_vec(), "image/png")
        } else if bytes.starts_with(&[0xff, 0xd8]) {
            (bytes.to_vec(), "image/jpeg")
        } else if bytes.starts_with(b"GIF8") {
            (bytes.to_vec(), "image/gif")
        } else {
            // What Word does not read (WebP) is kept as PNG.
            (
                quench_image::encode_png(px.width, px.height, &px.rgba),
                "image/png",
            )
        };
        let (w, h) = (f64::from(px.width) * 0.75, f64::from(px.height) * 0.75);
        let scale = (MAX_PICTURE_WIDTH / w).min(1.0);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let hundredths = |v: f64| (v * scale * 100.0).round().max(100.0) as u32;
        self.snapshot("picture");
        let at = self.doc.borrow_mut().insert_picture(
            self.caret_para(),
            docs_text::Picture {
                mime,
                data: Rc::new(data),
                width: hundredths(w),
                height: hundredths(h),
            },
        );
        self.bump();
        self.put_caret(Range {
            anchor: at,
            focus: at,
        });
        Ok(())
    }

    /// Show the picture the caret is in at `factor` times its size (never wider than the text).
    pub fn scale_picture(&self, factor: f64) {
        let para = self.caret_para();
        let Some(number) = self.doc.borrow().paras().get(para).and_then(|p| p.picture) else {
            self.status.set(tr("Put the caret on a picture first"));
            return;
        };
        self.snapshot("picture");
        self.doc
            .borrow_mut()
            .scale_picture(number, factor, MAX_PICTURE_WIDTH);
        self.bump();
    }

    // ---- tables ----

    fn caret_para(&self) -> usize {
        self.selection.get_untracked().focus.para
    }

    /// Put a table of `rows` by `cols` empty cells after the paragraph the caret is in; the caret goes in its first cell.
    pub fn insert_table(&self, rows: usize, cols: usize) {
        self.snapshot("table");
        let at = self
            .doc
            .borrow_mut()
            .insert_table(self.caret_para(), rows, cols);
        self.bump();
        self.put_caret(Range {
            anchor: at,
            focus: at,
        });
    }

    fn in_table(&self) -> bool {
        let ok = self.doc.borrow().cell_at(self.caret_para()).is_some();
        if !ok {
            self.status.set(tr("Put the caret in a table first"));
        }
        ok
    }

    /// Run a table edit on the paragraph of the caret; it says where the caret goes.
    fn edit_table(&self, f: impl FnOnce(&mut Doc, usize) -> Option<Pos>) {
        if !self.in_table() {
            return;
        }
        self.snapshot("table");
        let at = f(&mut self.doc.borrow_mut(), self.caret_para());
        self.bump();
        if let Some(at) = at {
            self.put_caret(Range {
                anchor: at,
                focus: at,
            });
        }
    }

    pub fn add_row(&self) {
        self.edit_table(Doc::add_row);
    }

    pub fn add_col(&self) {
        self.edit_table(Doc::add_col);
    }

    pub fn delete_row(&self) {
        self.edit_table(Doc::delete_row);
    }

    pub fn delete_col(&self) {
        self.edit_table(Doc::delete_col);
    }

    /// Make the column the caret is in wider or narrower by `by` thousandths of the table.
    pub fn resize_column(&self, by: i32) {
        let para = self.caret_para();
        let Some(c) = self.doc.borrow().cell_at(para) else {
            self.status.set(tr("Put the caret in a table first"));
            return;
        };
        self.snapshot("table");
        self.doc
            .borrow_mut()
            .resize_column(c.table, usize::from(c.col), by);
        self.bump();
    }

    /// Make the first row of the table the caret is in a header row, or an ordinary row again.
    pub fn toggle_header_row(&self) {
        let para = self.caret_para();
        let Some(c) = self.doc.borrow().cell_at(para) else {
            self.status.set(tr("Put the caret in a table first"));
            return;
        };
        self.snapshot("table");
        let on = !self.doc.borrow().table_header(c.table);
        self.doc.borrow_mut().set_table_header(c.table, on);
        self.bump();
    }

    pub fn delete_table(&self) {
        self.edit_table(Doc::delete_table);
    }

    /// Go to the next (or previous) cell; after the last cell a row is added. Returns whether the caret was in a table.
    pub fn cell_move(&self, back: bool) -> bool {
        let para = self.caret_para();
        let Some((first, last, ..)) = self.doc.borrow().table_of(para) else {
            return false;
        };
        let to = if back {
            para.saturating_sub(1).max(first)
        } else if para < last {
            para + 1
        } else {
            self.edit_table(Doc::add_row);
            return true;
        };
        let end = self.doc.borrow().len_of(to);
        self.put_caret(Range {
            anchor: Pos::new(to, 0),
            focus: Pos::new(to, end),
        });
        true
    }

    /// Open the link bar, with the address of the link at the caret.
    pub fn open_link(&self) {
        let (_, end) = self.selection.get_untracked().ordered();
        let url = self
            .doc
            .borrow()
            .link_at(end)
            .unwrap_or_default()
            .to_string();
        batch(|| {
            self.link_url.set(url);
            self.link_open.set(true);
        });
    }

    /// Make the selected text a link to the address in the bar.
    pub fn apply_link(&self) {
        let (a, b) = self.selection.get_untracked().ordered();
        let url = self.link_url.get_untracked();
        let url = url.trim();
        if a == b {
            self.status.set(tr("Select the text to link first"));
            return;
        }
        if url.is_empty() {
            self.remove_link();
            return;
        }
        // A bare address gets the scheme people mean.
        let url = if url.contains(':') || url.starts_with('#') {
            url.to_string()
        } else if url.contains('@') {
            format!("mailto:{url}")
        } else {
            format!("https://{url}")
        };
        self.snapshot("link");
        self.doc.borrow_mut().set_link(a, b, Some(&url));
        self.bump();
        self.put_caret(Range {
            anchor: a,
            focus: b,
        });
        self.link_open.set(false);
    }

    /// Take the link off the selected text.
    pub fn remove_link(&self) {
        let (a, b) = self.selection.get_untracked().ordered();
        if a == b {
            self.status.set(tr("Select the text to link first"));
            return;
        }
        self.snapshot("link");
        self.doc.borrow_mut().set_link(a, b, None);
        self.bump();
        self.put_caret(Range {
            anchor: a,
            focus: b,
        });
        self.link_open.set(false);
    }

    pub fn open_find(&self) {
        self.find_open.set(true);
    }

    /// Change the style of the selected text with `f` (nothing happens for a caret).
    pub fn restyle_selected(&self, f: impl Fn(Style) -> Style) {
        let range = self.selection.get_untracked();
        let (a, b) = range.ordered();
        if a == b {
            return;
        }
        self.snapshot("format");
        self.doc.borrow_mut().restyle(a, b, f);
        batch(|| {
            self.bump();
            self.put_caret(range);
        });
    }

    pub fn set_size(&self, half_points: Option<u16>) {
        self.restyle_selected(|s| Style {
            size: half_points,
            ..s
        });
    }

    pub fn set_color(&self, color: Option<u32>) {
        self.restyle_selected(|s| Style { color, ..s });
    }

    pub fn set_family(&self, family: Option<Family>) {
        self.restyle_selected(|s| Style {
            family,
            font: None,
            ..s
        });
    }

    /// Set the typeface by name: one of [`docs_text::TYPEFACES`] (which also says the kind it stands in for), or any name a
    /// document brought. An empty name goes back to the default.
    pub fn set_typeface(&self, name: &str) {
        if name.is_empty() {
            self.set_family(None);
            return;
        }
        let family = docs_text::typeface(name).map(|t| t.family);
        let id = self.doc.borrow_mut().font_id(name);
        self.restyle_selected(|s| Style {
            family: family.or(s.family).or(Some(Family::Sans)),
            font: Some(id),
            ..s
        });
    }

    /// The name of the typeface of the selection, when it is one by name.
    #[must_use]
    pub fn typeface_name(&self) -> Option<String> {
        let id = self.common_style()?.font?;
        self.doc.borrow().font_name(id).map(str::to_string)
    }

    /// Align the paragraphs the selection touches.
    pub fn set_align(&self, align: Align) {
        self.snapshot("align");
        let range = self.selection.get_untracked();
        let (a, b) = range.ordered();
        for i in a.para..=b.para {
            self.doc.borrow_mut().set_align(i, align);
        }
        batch(|| {
            self.bump();
            self.put_caret(range);
        });
    }

    /// Change the spacing and indents of the paragraphs the selection touches.
    pub fn format_paragraphs(&self, f: impl Fn(docs_text::ParaFmt) -> docs_text::ParaFmt) {
        self.snapshot("paragraph");
        let range = self.selection.get_untracked();
        let (a, b) = range.ordered();
        for i in a.para..=b.para {
            self.doc.borrow_mut().set_fmt(i, &f);
        }
        batch(|| {
            self.bump();
            self.put_caret(range);
        });
    }

    /// Move the left indent by `by` twentieths of a point (not below nothing).
    pub fn indent_by(&self, by: i32) {
        self.format_paragraphs(|mut f| {
            f.indent = u16::try_from((i32::from(f.indent) + by).clamp(0, 12_000)).unwrap_or(0);
            f
        });
    }

    /// Set the line spacing of the paragraphs (in lines; 1 is single).
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub fn set_line_spacing(&self, lines: f64) {
        let v = if (lines - 1.0).abs() < 0.01 {
            0
        } else {
            (lines * 240.0).round().clamp(0.0, 2000.0) as u16
        };
        self.format_paragraphs(move |mut f| {
            f.line = v;
            f
        });
    }

    /// Move a list item one level in (`1`) or out (`-1`); false when the caret is not in a list item.
    #[allow(clippy::must_use_candidate)]
    pub fn nest(&self, by: i32) -> bool {
        let (a, b) = self.selection.get_untracked().ordered();
        let listed = (a.para..=b.para).any(|i| {
            self.doc
                .borrow()
                .paras()
                .get(i)
                .is_some_and(|p| matches!(p.kind, Kind::Bullet | Kind::Numbered))
        });
        if !listed {
            return false;
        }
        self.format_paragraphs(move |mut f| {
            f.level = u8::try_from((i32::from(f.level) + by).clamp(0, 8)).unwrap_or(0);
            f
        });
        true
    }

    /// Start the paragraph the caret is in on a new page (or take that off again).
    pub fn toggle_page_break(&self) {
        let on = self
            .doc
            .borrow()
            .paras()
            .get(self.caret_para())
            .is_some_and(|p| p.fmt.page_break);
        let para = self.caret_para();
        self.snapshot("paragraph");
        self.doc.borrow_mut().set_fmt(para, move |mut f| {
            f.page_break = !on;
            f
        });
        self.bump();
    }

    /// The style the selection has in common (`None` where it differs).
    #[must_use]
    pub fn common_style(&self) -> Option<Style> {
        let (a, b) = self.selection.get().ordered();
        self.doc.borrow().common_style(a, b)
    }

    /// The kind of the paragraph the caret is in (a heading, a list item, plain text).
    #[must_use]
    pub fn kind_here(&self) -> Kind {
        let (a, _) = self.selection.get().ordered();
        self.doc
            .borrow()
            .paras()
            .get(a.para)
            .map_or(Kind::Paragraph, |p| p.kind)
    }

    /// The alignment of the paragraph the caret is in.
    #[must_use]
    pub fn align_here(&self) -> Align {
        let (a, _) = self.selection.get().ordered();
        self.doc
            .borrow()
            .paras()
            .get(a.para)
            .map_or(Align::Left, |p| p.align)
    }

    /// Make the paragraphs the selection touches this kind (a heading again is a plain paragraph).
    pub fn set_kind(&self, kind: Kind) {
        self.snapshot("kind");
        let (a, b) = self.selection.get_untracked().ordered();
        {
            let mut doc = self.doc.borrow_mut();
            let all = (a.para..=b.para).all(|i| doc.paras().get(i).is_some_and(|p| p.kind == kind));
            for i in a.para..=b.para {
                doc.set_kind(i, if all { Kind::Paragraph } else { kind });
            }
        }
        let range = self.selection.get_untracked();
        batch(|| {
            self.bump();
            self.put_caret(range);
        });
    }

    pub fn undo(&self) {
        let step = self.history.borrow_mut().undo.pop();
        if let Some((doc, sel)) = step {
            let now = (self.doc.borrow().clone(), self.selection.get_untracked());
            self.history.borrow_mut().redo.push(now);
            self.history.borrow_mut().last = "undo";
            *self.doc.borrow_mut() = doc;
            batch(|| {
                self.bump();
                self.put_caret(sel);
            });
        }
    }

    pub fn redo(&self) {
        let step = self.history.borrow_mut().redo.pop();
        if let Some((doc, sel)) = step {
            let now = (self.doc.borrow().clone(), self.selection.get_untracked());
            self.history.borrow_mut().undo.push(now);
            self.history.borrow_mut().last = "redo";
            *self.doc.borrow_mut() = doc;
            batch(|| {
                self.bump();
                self.put_caret(sel);
            });
        }
    }

    #[must_use]
    pub fn can_undo(&self) -> bool {
        !self.history.borrow().undo.is_empty()
    }

    #[must_use]
    pub fn can_redo(&self) -> bool {
        !self.history.borrow().redo.is_empty()
    }
}
