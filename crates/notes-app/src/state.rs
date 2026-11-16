//! The state of the app: the vault, which note is open, how it is shown, and what is being presented.

use crate::export;
use crate::i18n::tr;
use crate::vault::{Hit, Vault, clean_name};
use notes_md::{Slide, parse_front, set_front_matter_key, slides};
use quench_ui::platform::{self, PickedFile};
use quench_ui::prelude::*;
use quench_ui::{byte_to_utf16, utf16_to_byte};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// How the open note is shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Edit,
    Split,
    Preview,
}

/// What a small question asks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PromptKind {
    New,
    Rename,
    Delete,
    Switch,
}

/// A question with an answer to type (or none, for a confirmation).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Prompt {
    pub kind: PromptKind,
    pub value: String,
    pub error: String,
}

/// A deck: the slides of a note and how it asks to be shown.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Deck {
    pub title: String,
    pub theme: String,
    /// Width and height of the stage, in pixels.
    pub stage: (u32, u32),
    pub slides: Vec<Slide>,
}

/// The stage size of an aspect (`16:9`, `4:3`, `16:10`); 16:9 for anything else.
#[must_use]
pub fn stage_of(aspect: &str) -> (u32, u32) {
    match aspect.trim() {
        "4:3" => (1024, 768),
        "16:10" => (1280, 800),
        "1:1" => (900, 900),
        _ => (1280, 720),
    }
}

/// The decks' themes, as the menu lists them: the name in front matter and what it is called.
pub const THEMES: [(&str, &str); 4] = [
    ("default", "Default"),
    ("night", "Night"),
    ("paper", "Paper"),
    ("contrast", "High contrast"),
];

/// One second of the presentation clock, and the next one asked for while the same presentation goes on.
fn tick(state: NotesState, current: Rc<Cell<u32>>, generation: u32) {
    let _ = platform::after(1000, move || {
        if current.get() == generation && state.presenting.get_untracked() {
            state.elapsed.set(state.elapsed.get_untracked() + 1);
            tick(state, current, generation);
        }
    });
}

/// Everything the screen shows and the actions on it.
#[derive(Clone)]
pub struct NotesState {
    vault: Rc<RefCell<Vault>>,
    changed: Rc<RefCell<Option<Rc<dyn Fn()>>>>,
    timer: Rc<Cell<u32>>,
    /// Bumped when the vault changes.
    pub rev: Signal<u32>,
    /// The name of the open note.
    pub active: Signal<String>,
    /// The text in the editor (the open note's).
    pub buffer: Signal<String>,
    pub mode: Signal<Mode>,
    pub status: Signal<String>,
    /// What the search box holds.
    pub query: Signal<String>,
    pub prompt: Signal<Option<Prompt>>,
    pub presenting: Signal<bool>,
    pub slide: Signal<usize>,
    /// How many list items of the slide shown have appeared so far, when the slide shows them one at a time (`<!-- fragments -->`).
    pub revealed: Signal<usize>,
    pub show_notes: Signal<bool>,
    /// Seconds since the presentation began.
    pub elapsed: Signal<u32>,
    /// The size of the room the stage is shown in, for scaling it.
    pub room: Signal<(f64, f64)>,
    /// The caret or selection of the editor, as UTF-16 offsets into its text (the browser's units).
    pub caret: Signal<(usize, usize)>,
    /// Bumped to build the editor again and give it the focus (after a button of the ribbon changed its text).
    pub editor_epoch: Signal<u32>,
    /// The language of the screen (a locale code); the screen is drawn again when it changes.
    pub language: Signal<String>,
}

impl std::fmt::Debug for NotesState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NotesState").finish_non_exhaustive()
    }
}

impl NotesState {
    /// A state over a vault (a first note is made when it is empty).
    #[must_use]
    pub fn new(mut vault: Vault) -> Self {
        crate::i18n::reset();
        if vault.is_empty() {
            vault.add("Welcome", &crate::demo::welcome());
        }
        let first = vault
            .canonical("Welcome")
            .or_else(|| vault.notes().first().map(|n| n.name.as_str()))
            .unwrap_or("Untitled")
            .to_string();
        let text = vault.text(&first).unwrap_or_default().to_string();
        Self {
            vault: Rc::new(RefCell::new(vault)),
            changed: Rc::default(),
            timer: Rc::default(),
            rev: create_signal(0),
            active: create_signal(first),
            buffer: create_signal(text),
            mode: create_signal(Mode::Split),
            status: create_signal(String::new()),
            query: create_signal(String::new()),
            prompt: create_signal(None),
            presenting: create_signal(false),
            slide: create_signal(0),
            revealed: create_signal(0),
            show_notes: create_signal(false),
            elapsed: create_signal(0),
            room: create_signal((1280.0, 720.0)),
            caret: create_signal((0, 0)),
            editor_epoch: create_signal(0),
            language: create_signal(crate::i18n::language().to_string()),
        }
    }

    /// Call `f` after every change to the vault, so the host can keep it.
    pub fn on_change(&self, f: impl Fn() + 'static) {
        *self.changed.borrow_mut() = Some(Rc::new(f));
    }

    fn touched(&self) {
        self.rev.set(self.rev.get_untracked().wrapping_add(1));
        if let Some(f) = self.changed.borrow().clone() {
            f();
        }
    }

    /// Switch the language of the screen and tell the app, which keeps it.
    pub fn set_language(&self, code: &str) {
        crate::i18n::use_language(code);
        let now = crate::i18n::language();
        self.language.set_if_changed(now.to_string());
        crate::i18n::announce(now);
    }

    // ---- the vault ----

    /// The vault as it is now.
    #[must_use]
    pub fn vault(&self) -> Vault {
        self.vault.borrow().clone()
    }

    /// The vault in its stored form.
    #[must_use]
    pub fn encoded(&self) -> String {
        self.vault.borrow().encode()
    }

    /// Names and titles of the notes, in order.
    #[must_use]
    pub fn titles(&self) -> Vec<(String, String)> {
        self.vault
            .borrow()
            .notes()
            .iter()
            .map(|n| (n.name.clone(), notes_md::title_of(&n.name, &n.text)))
            .collect()
    }

    #[must_use]
    pub fn backlinks(&self) -> Vec<String> {
        self.vault.borrow().backlinks(&self.active.get_untracked())
    }

    #[must_use]
    pub fn outgoing(&self) -> (Vec<String>, Vec<String>) {
        self.vault.borrow().outgoing(&self.active.get_untracked())
    }

    #[must_use]
    pub fn tags(&self) -> Vec<(String, usize)> {
        self.vault
            .borrow()
            .tags()
            .into_iter()
            .map(|(t, n)| (t, n.len()))
            .collect()
    }

    #[must_use]
    pub fn search(&self, query: &str) -> Vec<Hit> {
        self.vault.borrow().search(query, 50)
    }

    #[must_use]
    pub fn exists(&self, name: &str) -> bool {
        self.vault.borrow().contains(name)
    }

    // ---- the open note ----

    /// Open a note by name.
    pub fn select(&self, name: &str) {
        let (name, text) = {
            let v = self.vault.borrow();
            let Some(n) = v.canonical(name) else { return };
            (n.to_string(), v.text(n).unwrap_or_default().to_string())
        };
        batch(|| {
            self.active.set(name);
            self.buffer.set(text);
            self.slide.set(0);
            self.presenting.set(false);
        });
    }

    /// The text of the editor changed.
    pub fn edit(&self, text: &str) {
        let name = self.active.get_untracked();
        self.vault.borrow_mut().set_text(&name, text);
        self.buffer.set(text.replace('\r', ""));
        self.touched();
    }

    // ---- editing at the caret ----

    /// The selection as byte offsets into the text.
    fn selection_bytes(&self, text: &str) -> (usize, usize) {
        let (a, b) = self.caret.get_untracked();
        let (a, b) = (utf16_to_byte(text, a.min(b)), utf16_to_byte(text, a.max(b)));
        (a, b)
    }

    /// Replace the text, and put the selection at `start..end` (byte offsets into the new text).
    fn replace_text(&self, text: &str, start: usize, end: usize) {
        batch(|| {
            self.caret
                .set((byte_to_utf16(text, start), byte_to_utf16(text, end)));
            self.edit(text);
        });
    }

    /// Give the focus back to the editor, with the caret where it was.
    pub fn refocus_editor(&self) {
        self.editor_epoch.set(self.editor_epoch.get_untracked() + 1);
    }

    /// The editor reported where its caret is.
    pub fn set_caret(&self, start: usize, end: usize) {
        self.caret.set_if_changed((start, end));
    }

    /// Put `snippet` where the caret is (replacing what is selected) and the caret after it.
    pub fn insert_text(&self, snippet: &str) {
        let text = self.buffer.get_untracked();
        let (a, b) = self.selection_bytes(&text);
        let new = format!("{}{snippet}{}", &text[..a], &text[b..]);
        self.replace_text(&new, a + snippet.len(), a + snippet.len());
    }

    /// Put a block of markdown at the caret, on lines of its own: a line break before it when the caret is not at the start
    /// of a line, and a blank line after the block.
    pub fn insert_block(&self, snippet: &str) {
        let text = self.buffer.get_untracked();
        let (a, _) = self.selection_bytes(&text);
        let before = &text[..a];
        let lead = if before.is_empty() || before.ends_with("\n\n") {
            ""
        } else if before.ends_with('\n') {
            "\n"
        } else {
            "\n\n"
        };
        self.insert_text(&format!("{lead}{snippet}\n"));
    }

    /// Wrap the selection in `before` and `after` (`**` and `**` for bold); wrapped already, it is unwrapped. With nothing
    /// selected the markers go in with `placeholder` between them, selected.
    pub fn wrap_selection(&self, before: &str, after: &str, placeholder: &str) {
        let text = self.buffer.get_untracked();
        let (a, b) = self.selection_bytes(&text);
        let inner = &text[a..b];
        if a >= before.len() && text[..a].ends_with(before) && text[b..].starts_with(after) {
            let new = format!(
                "{}{inner}{}",
                &text[..a - before.len()],
                &text[b + after.len()..]
            );
            self.replace_text(&new, a - before.len(), b - before.len());
            return;
        }
        let middle = if inner.is_empty() { placeholder } else { inner };
        let new = format!("{}{before}{middle}{after}{}", &text[..a], &text[b..]);
        self.replace_text(&new, a + before.len(), a + before.len() + middle.len());
    }

    /// Start every line the selection touches with `prefix` (a heading, a quote, a list marker), or take it off when they all
    /// have it. A heading prefix replaces another heading's.
    pub fn toggle_line_prefix(&self, prefix: &str) {
        let text = self.buffer.get_untracked();
        let (a, b) = self.selection_bytes(&text);
        let start = text[..a].rfind('\n').map_or(0, |i| i + 1);
        let end = text[b..].find('\n').map_or(text.len(), |i| b + i);
        let heading = prefix.starts_with('#');
        let strip = |line: &str| -> String {
            if heading {
                line.trim_start_matches('#').trim_start().to_string()
            } else {
                line.strip_prefix(prefix).unwrap_or(line).to_string()
            }
        };
        let lines: Vec<&str> = text[start..end].split('\n').collect();
        let all_have = lines.iter().all(|l| {
            l.starts_with(prefix) && (!heading || l[prefix.len()..].starts_with(|c: char| c != '#'))
        });
        let changed: Vec<String> = lines
            .iter()
            .map(|l| {
                if all_have {
                    strip(l)
                } else {
                    format!("{prefix}{}", strip(l))
                }
            })
            .collect();
        let joined = changed.join("\n");
        let new = format!("{}{joined}{}", &text[..start], &text[end..]);
        self.replace_text(&new, start, start + joined.len());
    }

    /// Indent (or with `back`, outdent) the lines the selection touches by two spaces; with nothing selected, a Tab types
    /// two spaces.
    pub fn indent(&self, back: bool) {
        let text = self.buffer.get_untracked();
        let (a, b) = self.selection_bytes(&text);
        if a == b && !back {
            self.insert_text("  ");
            return;
        }
        let start = text[..a].rfind('\n').map_or(0, |i| i + 1);
        let end = text[b..].find('\n').map_or(text.len(), |i| b + i);
        let changed: Vec<String> = text[start..end]
            .split('\n')
            .map(|l| {
                if back {
                    l.strip_prefix("  ")
                        .or_else(|| l.strip_prefix(' '))
                        .unwrap_or(l)
                        .to_string()
                } else {
                    format!("  {l}")
                }
            })
            .collect();
        let joined = changed.join("\n");
        let new = format!("{}{joined}{}", &text[..start], &text[end..]);
        self.replace_text(&new, start, start + joined.len());
    }

    /// Follow a link: open the note, or make it (empty) when there is none yet. `heading` is noted in the status line.
    pub fn open_link(&self, target: &str, heading: Option<&str>) {
        if !self.exists(target) {
            let name = self.vault.borrow_mut().add(target, "");
            self.touched();
            self.select(&name);
            self.status.set(format!("{} {name}", tr("Made the note")));
            return;
        }
        self.select(target);
        if let Some(h) = heading {
            self.status.set(h.to_string());
        }
    }

    /// A new note, named from `name` (made unique), opened.
    #[allow(clippy::must_use_candidate)]
    pub fn new_note(&self, name: &str) -> String {
        let used = self.vault.borrow_mut().add(name, "");
        self.touched();
        self.select(&used);
        self.mode.set(Mode::Split);
        used
    }

    /// Rename the open note; links in the other notes follow.
    ///
    /// # Errors
    /// What is wrong with the name, for the person.
    pub fn rename_active(&self, new: &str) -> Result<(), String> {
        let old = self.active.get_untracked();
        let used = self
            .vault
            .borrow_mut()
            .rename(&old, new)
            .map_err(|e| tr(&e))?;
        let text = self
            .vault
            .borrow()
            .text(&used)
            .unwrap_or_default()
            .to_string();
        batch(|| {
            self.active.set(used);
            self.buffer.set(text);
        });
        self.touched();
        Ok(())
    }

    /// Delete the open note (a vault is never left without one) and open another.
    pub fn delete_active(&self) {
        let old = self.active.get_untracked();
        self.vault.borrow_mut().remove(&old);
        if self.vault.borrow().is_empty() {
            self.vault.borrow_mut().add("Untitled", "");
        }
        let next = self
            .vault
            .borrow()
            .notes()
            .first()
            .map(|n| n.name.clone())
            .unwrap_or_default();
        self.touched();
        self.select(&next);
        self.status.set(tr("Deleted the note"));
    }

    // ---- prompts ----

    /// Ask something: a name for a new note, a new name for this one, whether to delete it, or which note to go to.
    pub fn ask(&self, kind: PromptKind) {
        let value = match kind {
            PromptKind::Rename => self.active.get_untracked(),
            _ => String::new(),
        };
        self.prompt.set(Some(Prompt {
            kind,
            value,
            error: String::new(),
        }));
    }

    pub fn set_prompt_value(&self, value: &str) {
        self.prompt.update(|p| {
            if let Some(p) = p {
                p.value = value.to_string();
                p.error.clear();
            }
        });
    }

    pub fn cancel_prompt(&self) {
        self.prompt.set(None);
    }

    /// Do what the open question asked with what was typed.
    pub fn answer(&self) {
        let Some(p) = self.prompt.get_untracked() else {
            return;
        };
        let result: Result<(), String> = match p.kind {
            PromptKind::New => {
                let _ = self.new_note(&p.value);
                Ok(())
            }
            PromptKind::Rename => self.rename_active(&p.value),
            PromptKind::Delete => {
                self.delete_active();
                Ok(())
            }
            PromptKind::Switch => {
                let wanted = p.value.trim();
                let pick = self
                    .search(wanted)
                    .into_iter()
                    .next()
                    .map(|h| h.name)
                    .or_else(|| self.exists(wanted).then(|| wanted.to_string()));
                pick.map_or_else(
                    || Err(tr("No note matches")),
                    |n| {
                        self.select(&n);
                        Ok(())
                    },
                )
            }
        };
        match result {
            Ok(()) => self.prompt.set(None),
            Err(e) => self.prompt.update(|p| {
                if let Some(p) = p {
                    p.error = e;
                }
            }),
        }
    }

    // ---- the deck ----

    /// The open note as a deck.
    #[must_use]
    pub fn deck(&self) -> Deck {
        let text = self.buffer.get_untracked();
        let (front, body) = parse_front(&text);
        let name = self.active.get_untracked();
        Deck {
            title: notes_md::title_of(&name, &text),
            theme: front
                .theme
                .clone()
                .filter(|t| THEMES.iter().any(|(k, _)| k == t))
                .unwrap_or_else(|| "default".into()),
            stage: stage_of(front.aspect.as_deref().unwrap_or("16:9")),
            slides: slides(body),
        }
    }

    /// Set the deck's theme in the note's front matter.
    pub fn set_theme(&self, theme: &str) {
        let text = set_front_matter_key(&self.buffer.get_untracked(), "theme", theme);
        self.edit(&text);
    }

    /// Set the deck's aspect in the note's front matter.
    pub fn set_aspect(&self, aspect: &str) {
        let text = set_front_matter_key(&self.buffer.get_untracked(), "aspect", aspect);
        self.edit(&text);
    }

    // ---- presenting ----

    /// Start presenting at slide `from` (counting from 0).
    pub fn present(&self, from: usize) {
        let last = self.deck().slides.len().saturating_sub(1);
        batch(|| {
            self.slide.set(from.min(last));
            self.revealed.set(0);
            self.elapsed.set(0);
            self.presenting.set(true);
        });
        self.run_timer();
    }

    pub fn stop_presenting(&self) {
        self.presenting.set(false);
        self.timer.set(self.timer.get().wrapping_add(1));
    }

    fn run_timer(&self) {
        let generation = self.timer.get().wrapping_add(1);
        self.timer.set(generation);
        tick(self.clone(), Rc::clone(&self.timer), generation);
    }

    /// Whether the slide shows its list items one at a time.
    fn incremental(slide: &notes_md::Slide) -> bool {
        slide
            .directives
            .iter()
            .any(|(k, v)| k == "fragments" && v != "false")
            || slide.layout.as_deref() == Some("fragments")
    }

    /// Move by `by` steps (negative is back): to the next list item of a slide that shows them one at a time, else to the
    /// next slide, staying inside the deck.
    pub fn step(&self, by: i64) {
        let deck = self.deck();
        let now_slide = self
            .slide
            .get_untracked()
            .min(deck.slides.len().saturating_sub(1));
        if let Some(slide) = deck.slides.get(now_slide)
            && Self::incremental(slide)
        {
            let total = fragment_total(&slide.markdown);
            let shown = self.revealed.get_untracked();
            if by > 0 && shown < total {
                self.revealed.set(shown + 1);
                return;
            }
            if by < 0 && shown > 0 {
                self.revealed.set(shown - 1);
                return;
            }
        }
        self.move_slides(by);
    }

    fn move_slides(&self, by: i64) {
        let last = i64::try_from(self.deck().slides.len().saturating_sub(1)).unwrap_or(0);
        let now = i64::try_from(self.slide.get_untracked()).unwrap_or(0);
        let to = usize::try_from((now + by).clamp(0, last)).unwrap_or(0);
        batch(|| {
            self.slide.set(to);
            // Coming back to a slide shows all of it; coming to it, none of its items yet.
            let total = self
                .deck()
                .slides
                .get(to)
                .map_or(0, |sl| fragment_total(&sl.markdown));
            self.revealed.set(if by < 0 { total } else { 0 });
        });
    }

    pub fn go_to(&self, slide: usize) {
        let last = self.deck().slides.len().saturating_sub(1);
        batch(|| {
            self.slide.set(slide.min(last));
            self.revealed.set(0);
        });
    }

    // ---- files ----

    /// Open what was picked or dropped: markdown files become notes (the first one is opened).
    pub fn load_files(&self, files: &[PickedFile]) {
        let mut first = None;
        let mut count = 0;
        for f in files {
            let extension = f
                .name
                .rsplit_once('.')
                .map(|(_, e)| e.to_lowercase())
                .unwrap_or_default();
            if !matches!(extension.as_str(), "md" | "markdown" | "txt") {
                continue;
            }
            let stem = f.name.rsplit_once('.').map_or(f.name.as_str(), |(s, _)| s);
            let name = self
                .vault
                .borrow_mut()
                .add(stem, &String::from_utf8_lossy(&f.bytes));
            first.get_or_insert(name);
            count += 1;
        }
        let Some(first) = first else {
            self.status.set(tr("Only .md and .txt files can be opened"));
            return;
        };
        self.touched();
        self.select(&first);
        self.status.set(format!("{} {count}", tr("Opened notes:")));
    }

    /// Ask for files to open. Call it from a click.
    pub fn open_files(&self) {
        let state = self.clone();
        platform::pick_files(".md,.markdown,.txt", true, move |files| {
            state.load_files(&files)
        });
    }

    /// Open files dropped on the page.
    pub fn accept_drops(&self) {
        let state = self.clone();
        platform::on_drop(move |files| state.load_files(&files));
    }

    /// Offer the open note as a `.md` to save.
    pub fn save_md(&self) {
        let name = format!("{}.md", self.active.get_untracked());
        platform::download(
            &name,
            "text/markdown;charset=utf-8",
            self.buffer.get_untracked().as_bytes(),
        );
        self.status.set(format!("{} {name}", tr("Saved")));
    }

    /// Offer every note as a `.zip` of `.md` files.
    pub fn save_zip(&self) {
        let mut zip = quench_archive::zip::Writer::new();
        for n in self.vault.borrow().notes() {
            if zip
                .add(&format!("{}.md", n.name), n.text.as_bytes())
                .is_err()
            {
                self.status.set(tr("Could not make the zip"));
                return;
            }
        }
        match zip.finish() {
            Ok(bytes) => {
                platform::download("notes.zip", "application/zip", &bytes);
                self.status.set(format!("{} notes.zip", tr("Saved")));
            }
            Err(_) => self.status.set(tr("Could not make the zip")),
        }
    }

    /// Offer the open note as a standalone web page.
    pub fn export_html(&self) {
        let name = format!("{}.html", self.active.get_untracked());
        let html = export::note_html(&self.deck().title, &self.buffer.get_untracked());
        platform::download(&name, "text/html;charset=utf-8", html.as_bytes());
        self.status.set(format!("{} {name}", tr("Saved")));
    }

    /// Offer the open note as a standalone slide show.
    pub fn export_deck(&self) {
        let name = format!("{} (deck).html", self.active.get_untracked());
        let html = export::deck_html(&self.deck());
        platform::download(&name, "text/html;charset=utf-8", html.as_bytes());
        self.status.set(format!("{} {name}", tr("Saved")));
    }

    /// Offer the open note as a deck on paper: a PDF with a page for each slide.
    pub fn export_pdf(&self) {
        let deck = self.deck();
        let bytes = notes_pdf::deck_pdf(&deck.title, &deck.theme, deck.stage, &deck.slides);
        let name = format!("{}.pdf", self.active.get_untracked());
        platform::download(&name, "application/pdf", &bytes);
        self.status.set(format!("{} {name}", tr("Saved")));
    }

    /// Words in the open note's text.
    #[must_use]
    pub fn word_count(&self) -> usize {
        self.buffer.get_untracked().split_whitespace().count()
    }

    /// A name made from text, for a note or a file.
    #[must_use]
    pub fn clean(name: &str) -> String {
        clean_name(name)
    }
}

/// Whether a line starts a top-level list item.
fn is_item(line: &str) -> bool {
    let t = line.trim_end();
    t.starts_with("- ")
        || t.starts_with("* ")
        || t.starts_with("+ ")
        || (t.chars().take_while(char::is_ascii_digit).count() > 0 && {
            let digits = t.chars().take_while(char::is_ascii_digit).count();
            t[digits..].starts_with(". ")
        })
}

/// How many list items (at the left edge) a slide has.
#[must_use]
pub fn fragment_total(markdown: &str) -> usize {
    markdown.lines().filter(|l| is_item(l)).count()
}

/// The slide's markdown with only the first `shown` list items (and what is indented under them) in it.
#[must_use]
pub fn fragments_shown(markdown: &str, shown: usize) -> String {
    let mut out = Vec::new();
    let mut seen = 0;
    let mut hiding = false;
    for line in markdown.lines() {
        if is_item(line) {
            seen += 1;
            hiding = seen > shown;
        } else if !line.starts_with(' ') && !line.starts_with('\t') && !line.trim().is_empty() {
            hiding = false;
        }
        if !hiding {
            out.push(line);
        }
    }
    out.join("\n")
}
