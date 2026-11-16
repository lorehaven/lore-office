//! The open file, the page, the search.

use crate::i18n::tr;
use quench_ui::platform::{self, PickedFile};
use quench_ui::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

/// Where a search found something.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hit {
    pub page: usize,
    /// Byte offset in the page's text.
    pub at: usize,
}

/// A page drawn: its size in points and the address of the SVG picture of it.
#[derive(Debug)]
pub struct Picture {
    pub width: f64,
    pub height: f64,
    /// The top left of the page's box in the file's own coordinates.
    pub origin: (f64, f64),
    pub address: String,
    /// The text on the page and where it is, to lay over the picture.
    pub runs: Vec<quench_pdf::TextRun>,
    /// Links on the page.
    pub links: Vec<quench_pdf::PageLink>,
}

/// Zoom steps, as the percent of the normal text size.
pub const ZOOMS: [u32; 9] = [60, 75, 90, 100, 115, 130, 160, 200, 250];

#[derive(Clone)]
pub struct PdfState {
    bytes: Rc<RefCell<Vec<u8>>>,
    pages: Rc<RefCell<Vec<String>>>,
    /// The file as drawn: kept so pages are drawn when shown, and the pictures made (`width`, `height`, SVG address) by page.
    drawn: Rc<RefCell<Option<Rc<quench_pdf::Document>>>>,
    pictures: Rc<RefCell<std::collections::HashMap<usize, Rc<Picture>>>>,
    /// The file's outline (bookmarks), if it has one.
    outline: Rc<RefCell<Vec<quench_pdf::OutlineItem>>>,
    /// The file as it was before each change that can be taken back.
    before: Rc<RefCell<Vec<Vec<u8>>>>,
    hits: Rc<RefCell<Vec<Hit>>>,
    pub name: Signal<String>,
    pub status: Signal<String>,
    /// The language of the screen (a locale code); the screen is drawn again when it changes.
    pub language: Signal<String>,
    /// Bumped when another file is opened.
    pub rev: Signal<u32>,
    /// The page shown, from 0.
    pub page: Signal<usize>,
    pub zoom: Signal<usize>,
    /// Whether the page is shown as it is drawn (else as its text).
    pub as_picture: Signal<bool>,
    /// What a click on a line of text does: nothing (`""`), or puts this markup on it (`Highlight`, `Underline`, `StrikeOut`, `Text`).
    pub mark: Signal<String>,
    /// The line of text (a number on the page shown) after which words are being typed, if any.
    pub typing: Signal<Option<usize>>,
    /// The words being typed after a line.
    pub typed: Signal<String>,
    /// The words of the next note.
    pub note: Signal<String>,
    /// Whether the drawn pages are shown with their colours turned round (white paper becomes dark).
    pub dark_pages: Signal<bool>,
    /// Whether pages were turned, moved, taken out or added since the file was opened.
    pub edited: Signal<bool>,
    pub query: Signal<String>,
    /// Which hit is current (an index into the hits), if there are any.
    pub current: Signal<Option<usize>>,
    /// How many hits the search has (for the status line and buttons).
    pub hit_count: Signal<usize>,
}

impl std::fmt::Debug for PdfState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PdfState").finish_non_exhaustive()
    }
}

impl Default for PdfState {
    fn default() -> Self {
        Self::new()
    }
}

impl PdfState {
    #[must_use]
    pub fn new() -> Self {
        crate::i18n::reset();
        Self {
            bytes: Rc::default(),
            pages: Rc::default(),
            drawn: Rc::default(),
            pictures: Rc::default(),
            outline: Rc::default(),
            hits: Rc::default(),
            name: create_signal(String::new()),
            status: create_signal(String::new()),
            language: create_signal(crate::i18n::language().to_string()),
            rev: create_signal(0),
            page: create_signal(0),
            zoom: create_signal(3),
            as_picture: create_signal(true),
            mark: create_signal(String::new()),
            note: create_signal(String::new()),
            typing: create_signal(None),
            typed: create_signal(String::new()),
            dark_pages: create_signal(false),
            edited: create_signal(false),
            before: Rc::default(),
            query: create_signal(String::new()),
            current: create_signal(None),
            hit_count: create_signal(0),
        }
    }

    /// Switch the language of the screen and tell the app, which keeps it.
    pub fn set_language(&self, code: &str) {
        let now = crate::i18n::use_language(code);
        self.language.set_if_changed(now.to_string());
        crate::i18n::announce(now);
    }

    /// Read a PDF. On failure the file shown stays as it was.
    ///
    /// # Errors
    ///
    /// What is wrong with the file, for the person.
    pub fn load_bytes(&self, name: &str, bytes: &[u8]) -> Result<(), String> {
        let pages = quench_pdf::extract_pages(bytes).map_err(|e| match e {
            quench_pdf::Error::Unsupported(what) => format!("{}: {what}", tr("not supported")),
            other => other.to_string(),
        })?;
        let count = pages.len();
        *self.bytes.borrow_mut() = bytes.to_vec();
        *self.pages.borrow_mut() = pages;
        *self.drawn.borrow_mut() = quench_pdf::Document::load(bytes).ok().map(Rc::new);
        self.pictures.borrow_mut().clear();
        *self.outline.borrow_mut() = self
            .drawn
            .borrow()
            .as_ref()
            .map(|d| d.outline())
            .unwrap_or_default();
        batch(|| {
            self.name.set(name.to_string());
            self.edited.set(false);
            self.page.set(0);
            self.query.set(String::new());
            self.rev.set(self.rev.get_untracked().wrapping_add(1));
        });
        self.search("");
        self.status.set(format!(
            "{name}: {count} {}",
            tr(if count == 1 { "page" } else { "pages" })
        ));
        Ok(())
    }

    /// Open the first file picked or dropped.
    pub fn load_files(&self, files: &[PickedFile]) {
        self.before.borrow_mut().clear();
        if let Some(f) = files.first()
            && let Err(e) = self.load_bytes(&f.name, &f.bytes)
        {
            self.status
                .set(format!("{} {}: {e}", tr("Could not open"), f.name));
        }
    }

    /// Ask for a file. Call it from a click.
    pub fn open_file(&self) {
        let state = self.clone();
        platform::pick_files(".pdf", false, move |files| state.load_files(&files));
    }

    /// Open files dropped on the page.
    pub fn accept_drops(&self) {
        let state = self.clone();
        platform::on_drop(move |files| state.load_files(&files));
    }

    // ---- markup ----

    /// Put the markup chosen (see [`PdfState::mark`]) on the line of text number `run` of the page shown.
    pub fn mark_run(&self, run: usize) {
        let kind = self.mark.get_untracked();
        if kind.is_empty() {
            return;
        }
        if kind == "Type" {
            // Words are typed after the line: a form's blank follows its label.
            batch(|| {
                self.typed.set(String::new());
                self.typing.set(Some(run));
            });
            return;
        }
        let page = self.page.get_untracked();
        let Some(pic) = self.picture(page) else {
            return;
        };
        let Some(r) = pic.runs.get(run) else {
            return;
        };
        // The line's box, from the top of its letters to below the baseline, turned into the file's coordinates.
        let (x0, x1) = (r.x + pic.origin.0, r.x + r.width + pic.origin.0);
        let (top, bottom) = (
            pic.origin.1 - r.size.mul_add(-0.85, r.y),
            pic.origin.1 - r.size.mul_add(0.25, r.y),
        );
        let contents = if kind == "Text" {
            self.note.get_untracked()
        } else {
            String::new()
        };
        let boxes = if kind == "Text" {
            vec![(x0, bottom, x0 + 16.0, top.max(bottom + 16.0))]
        } else {
            vec![(x0, bottom, x1, top)]
        };
        let color = match kind.as_str() {
            "Highlight" => (255, 235, 59),
            "Underline" | "StrikeOut" => (220, 38, 38),
            _ => (255, 200, 0),
        };
        let mark = pdf_edit::NewMarkup {
            kind,
            boxes,
            color,
            contents,
        };
        self.apply_markup(page, mark);
    }

    /// Put the words typed after the line being typed on (see [`PdfState::typing`]) into the file, in the size of that line.
    pub fn commit_text(&self) {
        let Some(run) = self.typing.get_untracked() else {
            return;
        };
        let text = self.typed.get_untracked();
        self.typing.set(None);
        if text.trim().is_empty() {
            return;
        }
        let page = self.page.get_untracked();
        let Some(pic) = self.picture(page) else {
            return;
        };
        let Some(r) = pic.runs.get(run) else {
            return;
        };
        let new = pdf_edit::NewText {
            x: r.size.mul_add(0.4, r.x + r.width + pic.origin.0),
            y: pic.origin.1 - r.y,
            size: r.size.clamp(6.0, 36.0),
            color: (0, 0, 120),
            text,
        };
        let Ok(source) = quench_pdf::Document::load(&self.bytes.borrow()) else {
            return;
        };
        let parts = pdf_edit::all_parts(std::slice::from_ref(&source));
        match pdf_edit::assemble_full(std::slice::from_ref(&source), &parts, &[], &[(page, new)]) {
            Ok(bytes) => {
                let old = self.bytes.borrow().clone();
                self.before.borrow_mut().push(old);
                let (name, keep) = (self.name.get_untracked(), self.mark.get_untracked());
                if self.load_bytes(&name, &bytes).is_ok() {
                    self.go_to(page);
                    self.mark.set(keep);
                    self.edited.set(true);
                }
            }
            Err(e) => self.status.set(e.to_string()),
        }
    }

    fn apply_markup(&self, page: usize, mark: pdf_edit::NewMarkup) {
        let Ok(source) = quench_pdf::Document::load(&self.bytes.borrow()) else {
            return;
        };
        let parts = pdf_edit::all_parts(std::slice::from_ref(&source));
        match pdf_edit::assemble_marked(std::slice::from_ref(&source), &parts, &[(page, mark)]) {
            Ok(bytes) => {
                let old = self.bytes.borrow().clone();
                self.before.borrow_mut().push(old);
                let name = self.name.get_untracked();
                let keep = self.mark.get_untracked();
                if self.load_bytes(&name, &bytes).is_ok() {
                    self.go_to(page);
                    self.mark.set(keep);
                    self.edited.set(true);
                }
            }
            Err(e) => self.status.set(e.to_string()),
        }
    }

    /// Take back the last change (markup or page operation).
    pub fn undo_edit(&self) {
        let Some(old) = self.before.borrow_mut().pop() else {
            self.status.set(tr("Nothing to undo"));
            return;
        };
        let (name, page, keep) = (
            self.name.get_untracked(),
            self.page.get_untracked(),
            self.mark.get_untracked(),
        );
        if self.load_bytes(&name, &old).is_ok() {
            self.go_to(page);
            self.mark.set(keep);
            self.edited.set(!self.before.borrow().is_empty());
        }
    }

    // ---- page operations ----

    /// Put the file together again from `parts` of itself (and of `others`), and show the result at page `show` (from 0).
    fn rebuild(
        &self,
        others: &[Vec<u8>],
        parts: impl FnOnce(&[quench_pdf::Document]) -> Vec<pdf_edit::Part>,
        show: usize,
    ) {
        let mut sources = Vec::new();
        for bytes in std::iter::once(self.bytes.borrow().clone()).chain(others.iter().cloned()) {
            match quench_pdf::Document::load(&bytes) {
                Ok(d) => sources.push(d),
                Err(e) => {
                    self.status.set(format!("{}: {e}", tr("Could not open")));
                    return;
                }
            }
        }
        let wanted = parts(&sources);
        match pdf_edit::assemble(&sources, &wanted) {
            Ok(bytes) => {
                let old = self.bytes.borrow().clone();
                self.before.borrow_mut().push(old);
                let name = self.name.get_untracked();
                if let Err(e) = self.load_bytes(&name, &bytes) {
                    self.status.set(e);
                    return;
                }
                self.go_to(show);
                self.edited.set(true);
                self.status.set(format!(
                    "{} {}",
                    self.page_count(),
                    tr(if self.page_count() == 1 {
                        "page"
                    } else {
                        "pages"
                    })
                ));
            }
            Err(e) => self.status.set(e.to_string()),
        }
    }

    fn own_parts(sources: &[quench_pdf::Document]) -> Vec<pdf_edit::Part> {
        pdf_edit::all_parts(&sources[..1])
    }

    /// Turn the page shown a quarter turn (clockwise for 90, the other way for -90).
    pub fn rotate_page(&self, turn: i32) {
        if self.page_count() == 0 {
            self.status.set(tr("Open a PDF first"));
            return;
        }
        let at = self.page.get_untracked();
        self.rebuild(
            &[],
            |s| {
                let mut parts = Self::own_parts(s);
                parts[at].turn += turn;
                parts
            },
            at,
        );
    }

    /// Take the page shown out of the file (the last page cannot be).
    pub fn delete_page(&self) {
        let at = self.page.get_untracked();
        if self.page_count() <= 1 {
            self.status.set(tr("A file needs at least one page"));
            return;
        }
        self.rebuild(
            &[],
            |s| {
                let mut parts = Self::own_parts(s);
                parts.remove(at);
                parts
            },
            at.min(self.page_count() - 2),
        );
    }

    /// Move the page shown one place earlier (`-1`) or later (`1`).
    pub fn move_page(&self, by: i64) {
        let at = self.page.get_untracked();
        let to = i64::try_from(at).unwrap_or(0) + by;
        if self.page_count() == 0 || to < 0 || to >= i64::try_from(self.page_count()).unwrap_or(0) {
            return;
        }
        let to = usize::try_from(to).unwrap_or(0);
        self.rebuild(
            &[],
            |s| {
                let mut parts = Self::own_parts(s);
                parts.swap(at, to);
                parts
            },
            to,
        );
    }

    /// Put the pages of another file after this one. Call it from a click.
    pub fn append_file(&self) {
        let state = self.clone();
        platform::pick_files(".pdf", false, move |files| {
            let Some(f) = files.into_iter().next() else {
                return;
            };
            if state.page_count() == 0 {
                state.load_files(&[f]);
                return;
            }
            let at = state.page.get_untracked();
            state.rebuild(&[f.bytes], pdf_edit::all_parts, at);
        });
    }

    /// The file as it is now, with the changes made.
    #[must_use]
    pub fn save_bytes(&self) -> Vec<u8> {
        self.bytes.borrow().clone()
    }

    /// Offer the file as it is now (with the changes made) to save. Call it from a click.
    pub fn save_copy(&self) {
        let bytes = self.bytes.borrow().clone();
        if bytes.is_empty() {
            self.status.set(tr("Open a PDF first"));
            return;
        }
        let name = pdf_name(&self.name.get_untracked(), "");
        platform::download(&name, "application/pdf", &bytes);
        self.status.set(format!("{} {name}", tr("Saved")));
    }

    /// Offer only the page shown as a file of its own.
    pub fn save_page(&self) {
        if self.page_count() == 0 {
            self.status.set(tr("Open a PDF first"));
            return;
        }
        let at = self.page.get_untracked();
        let Ok(source) = quench_pdf::Document::load(&self.bytes.borrow()) else {
            return;
        };
        let part = pdf_edit::Part {
            source: 0,
            page: at,
            turn: 0,
        };
        if let Ok(bytes) = pdf_edit::assemble(&[source], &[part]) {
            let name = pdf_name(&self.name.get_untracked(), &format!("-page-{}", at + 1));
            platform::download(&name, "application/pdf", &bytes);
            self.status.set(format!("{} {name}", tr("Saved")));
        }
    }

    /// Hand the file to the browser's own viewer, in a new tab (it draws the pages). Call it from a click.
    pub fn open_in_viewer(&self) {
        let bytes = self.bytes.borrow();
        if bytes.is_empty() {
            self.status.set(tr("Open a PDF first"));
            return;
        }
        platform::open_in_tab("application/pdf", &bytes);
    }

    /// The page drawn (the first time it is asked for it is drawn; then it is kept).
    #[must_use]
    pub fn picture(&self, page: usize) -> Option<Rc<Picture>> {
        if let Some(p) = self.pictures.borrow().get(&page) {
            return Some(Rc::clone(p));
        }
        let doc = self.drawn.borrow().clone()?;
        let drawn = doc.render_page(page)?;
        let made = Rc::new(Picture {
            width: drawn.width,
            height: drawn.height,
            origin: drawn.origin,
            address: svg_address(&drawn.svg),
            runs: drawn.runs,
            links: drawn.links,
        });
        self.pictures.borrow_mut().insert(page, Rc::clone(&made));
        Some(made)
    }

    #[must_use]
    pub fn page_count(&self) -> usize {
        self.pages.borrow().len()
    }

    /// The text of a page.
    #[must_use]
    pub fn page_text(&self, page: usize) -> String {
        self.pages.borrow().get(page).cloned().unwrap_or_default()
    }

    /// The file's own outline, if it has one.
    #[must_use]
    pub fn bookmarks(&self) -> Vec<quench_pdf::OutlineItem> {
        self.outline.borrow().clone()
    }

    /// The first non-empty line of each page, for the list on the left.
    #[must_use]
    pub fn outline(&self) -> Vec<String> {
        self.pages
            .borrow()
            .iter()
            .map(|p| {
                p.lines()
                    .find(|l| !l.trim().is_empty())
                    .unwrap_or("")
                    .trim()
                    .chars()
                    .take(60)
                    .collect()
            })
            .collect()
    }

    pub fn go_to(&self, page: usize) {
        let last = self.page_count().saturating_sub(1);
        self.page.set(page.min(last));
    }

    pub fn step(&self, by: i64) {
        let now = i64::try_from(self.page.get_untracked()).unwrap_or(0);
        self.go_to(usize::try_from((now + by).max(0)).unwrap_or(0));
    }

    pub fn zoom_by(&self, by: i64) {
        let now = i64::try_from(self.zoom.get_untracked()).unwrap_or(3);
        let last = i64::try_from(ZOOMS.len() - 1).unwrap_or(0);
        self.zoom
            .set(usize::try_from((now + by).clamp(0, last)).unwrap_or(3));
    }

    /// Look for text in every page (without regard to case) and show the first hit.
    pub fn search(&self, query: &str) {
        let q = query.trim().to_lowercase();
        let mut hits = Vec::new();
        if !q.is_empty() {
            for (page, text) in self.pages.borrow().iter().enumerate() {
                let lower = text.to_lowercase();
                // Offsets are in the lower-cased text; they equal the text's for all scripts where case keeps lengths.
                if lower.len() != text.len() {
                    continue;
                }
                let mut from = 0;
                while let Some(i) = lower[from..].find(&q) {
                    hits.push(Hit { page, at: from + i });
                    from += i + q.len().max(1);
                }
            }
        }
        let count = hits.len();
        *self.hits.borrow_mut() = hits;
        batch(|| {
            self.query.set(query.to_string());
            self.hit_count.set(count);
            self.current.set((count > 0).then_some(0));
        });
        if count > 0 {
            self.show_hit(0);
        }
    }

    fn show_hit(&self, index: usize) {
        if let Some(h) = self.hits.borrow().get(index).copied() {
            self.page.set(h.page);
        }
        self.current.set(Some(index));
    }

    /// Go to the next (or with `back`, previous) hit, wrapping round.
    pub fn next_hit(&self, back: bool) {
        let n = self.hits.borrow().len();
        if n == 0 {
            return;
        }
        let now = self.current.get_untracked().unwrap_or(0);
        self.show_hit(if back {
            (now + n - 1) % n
        } else {
            (now + 1) % n
        });
    }

    /// The hits on a page, as byte ranges of its text, and which of them is the current one.
    #[must_use]
    pub fn hits_on(&self, page: usize) -> (Vec<(usize, usize)>, Option<usize>) {
        let q = self.query.get().trim().len();
        let current = self.current.get();
        let hits = self.hits.borrow();
        let mut ranges = Vec::new();
        let mut current_here = None;
        for (i, h) in hits.iter().enumerate().filter(|(_, h)| h.page == page) {
            if Some(i) == current {
                current_here = Some(ranges.len());
            }
            ranges.push((h.at, h.at + q));
        }
        (ranges, current_here)
    }
}

/// An SVG as the address of an image: `data:` addresses are allowed on the page, with the characters that would end one escaped.
fn svg_address(svg: &str) -> String {
    let mut out = String::with_capacity(svg.len() + svg.len() / 8 + 32);
    out.push_str("data:image/svg+xml;utf8,");
    for c in svg.chars() {
        match c {
            '#' => out.push_str("%23"),
            '<' => out.push_str("%3C"),
            '>' => out.push_str("%3E"),
            '"' => out.push_str("%22"),
            '%' => out.push_str("%25"),
            '\n' | '\r' => out.push(' '),
            c => out.push(c),
        }
    }
    out
}

/// The name of a PDF to save: the file's own with `suffix` before the extension.
fn pdf_name(name: &str, suffix: &str) -> String {
    let stem = name.rsplit_once('.').map_or(name, |(s, _)| s);
    let stem = if stem.is_empty() { "document" } else { stem };
    format!("{stem}{suffix}.pdf")
}
