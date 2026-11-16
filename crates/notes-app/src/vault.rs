//! The notes of a vault, in memory: their names and texts, and what can be asked of them together.

use notes_md::{tags, title_of, wikilinks};
use std::collections::BTreeMap;

/// Separators of the stored form; they cannot be typed into a note, so they are taken out of what is stored.
const RECORD: char = '\u{1E}';
const FIELD: char = '\u{1F}';

/// A note: its name (the file name without `.md`) and its text.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Note {
    pub name: String,
    pub text: String,
}

/// A match of a search.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hit {
    pub name: String,
    /// The line it was found on (the name when only the name matched).
    pub line: String,
}

/// A name made safe for a file: no path separators or characters a link would trip on; never empty.
#[must_use]
pub fn clean_name(raw: &str) -> String {
    let cleaned: String = raw
        .trim()
        .trim_end_matches(".md")
        .chars()
        .map(|c| {
            if matches!(
                c,
                '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '#' | '^' | '[' | ']'
            ) || c.is_control()
            {
                '-'
            } else {
                c
            }
        })
        .collect();
    let cleaned = cleaned.trim().trim_matches('.').to_string();
    if cleaned.is_empty() {
        "Untitled".to_string()
    } else {
        cleaned
    }
}

/// All the notes, sorted by name without regard to case.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Vault {
    notes: Vec<Note>,
}

impl Vault {
    #[must_use]
    pub const fn new() -> Self {
        Self { notes: Vec::new() }
    }

    fn position(&self, name: &str) -> Option<usize> {
        self.notes
            .iter()
            .position(|n| n.name.eq_ignore_ascii_case(name.trim()))
    }

    fn sort(&mut self) {
        self.notes.sort_by_key(|n| n.name.to_lowercase());
    }

    #[must_use]
    pub fn notes(&self) -> &[Note] {
        &self.notes
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.notes.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.notes.is_empty()
    }

    #[must_use]
    pub fn contains(&self, name: &str) -> bool {
        self.position(name).is_some()
    }

    /// The name a note is stored under when asked for by `name` in any case (`None` if there is none).
    #[must_use]
    pub fn canonical(&self, name: &str) -> Option<&str> {
        self.position(name).map(|i| self.notes[i].name.as_str())
    }

    #[must_use]
    pub fn text(&self, name: &str) -> Option<&str> {
        self.position(name).map(|i| self.notes[i].text.as_str())
    }

    /// A name no note has: `base`, else `base 2`, `base 3`...
    #[must_use]
    pub fn unique_name(&self, base: &str) -> String {
        let base = clean_name(base);
        if !self.contains(&base) {
            return base;
        }
        (2..=u32::MAX)
            .map(|n| format!("{base} {n}"))
            .find(|n| !self.contains(n))
            .unwrap_or(base)
    }

    /// Add a note under a name made unique; returns the name used.
    pub fn add(&mut self, name: &str, text: &str) -> String {
        let name = self.unique_name(name);
        self.notes.push(Note {
            name: name.clone(),
            text: sanitize(text),
        });
        self.sort();
        name
    }

    /// Set the text of a note that exists.
    pub fn set_text(&mut self, name: &str, text: &str) {
        if let Some(i) = self.position(name) {
            self.notes[i].text = sanitize(text);
        }
    }

    pub fn remove(&mut self, name: &str) -> bool {
        self.position(name).map(|i| self.notes.remove(i)).is_some()
    }

    /// Give a note another name, and point the links of the others at it. Fails when the new name is taken by another note.
    ///
    /// # Errors
    /// A message for the person: the name is taken, or there is no such note.
    pub fn rename(&mut self, old: &str, new: &str) -> Result<String, String> {
        let new = clean_name(new);
        let Some(i) = self.position(old) else {
            return Err("There is no such note".into());
        };
        if self.position(&new).is_some_and(|j| j != i) {
            return Err("Another note has that name".into());
        }
        let old_name = self.notes[i].name.clone();
        self.notes[i].name.clone_from(&new);
        for n in &mut self.notes {
            n.text = notes_md::rename_links(&n.text, &old_name, &new);
        }
        self.sort();
        Ok(new)
    }

    /// The notes that link to `name`, by name.
    #[must_use]
    pub fn backlinks(&self, name: &str) -> Vec<String> {
        self.notes
            .iter()
            .filter(|n| !n.name.eq_ignore_ascii_case(name))
            .filter(|n| {
                wikilinks(&n.text)
                    .iter()
                    .any(|l| !l.embed && l.target.eq_ignore_ascii_case(name))
            })
            .map(|n| n.name.clone())
            .collect()
    }

    /// The notes a note links to that exist, then those that do not, each once, in order of first mention.
    #[must_use]
    pub fn outgoing(&self, name: &str) -> (Vec<String>, Vec<String>) {
        let (mut found, mut missing): (Vec<String>, Vec<String>) = (Vec::new(), Vec::new());
        for l in wikilinks(self.text(name).unwrap_or_default()) {
            if l.embed || l.target.eq_ignore_ascii_case(name) {
                continue;
            }
            match self.canonical(&l.target) {
                Some(c) if !found.iter().any(|f| f == c) => found.push(c.to_string()),
                None if !missing.iter().any(|m| m.eq_ignore_ascii_case(&l.target)) => {
                    missing.push(l.target)
                }
                _ => {}
            }
        }
        (found, missing)
    }

    /// Every tag with the notes that have it, tags in order.
    #[must_use]
    pub fn tags(&self) -> BTreeMap<String, Vec<String>> {
        let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for n in &self.notes {
            for t in tags(&n.text) {
                out.entry(t).or_default().push(n.name.clone());
            }
        }
        out
    }

    /// Notes whose name, title or text hold `query` (without regard to case), a line of each; names first. At most `limit`.
    #[must_use]
    pub fn search(&self, query: &str, limit: usize) -> Vec<Hit> {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return Vec::new();
        }
        let mut by_name = Vec::new();
        let mut by_text = Vec::new();
        for n in &self.notes {
            let title = title_of(&n.name, &n.text);
            if n.name.to_lowercase().contains(&q) || title.to_lowercase().contains(&q) {
                by_name.push(Hit {
                    name: n.name.clone(),
                    line: title,
                });
            } else if let Some(line) = n.text.lines().find(|l| l.to_lowercase().contains(&q)) {
                by_text.push(Hit {
                    name: n.name.clone(),
                    line: line.trim().chars().take(120).collect(),
                });
            }
        }
        by_name.extend(by_text);
        by_name.truncate(limit);
        by_name
    }

    /// The vault as one text for the browser's storage.
    #[must_use]
    pub fn encode(&self) -> String {
        self.notes
            .iter()
            .map(|n| format!("{}{FIELD}{}", n.name, n.text))
            .collect::<Vec<_>>()
            .join(&RECORD.to_string())
    }

    /// A vault from [`Vault::encode`]'s text (a damaged record is skipped).
    #[must_use]
    pub fn decode(text: &str) -> Self {
        let mut vault = Self::new();
        for record in text.split(RECORD).filter(|r| !r.is_empty()) {
            if let Some((name, body)) = record.split_once(FIELD) {
                vault.add(name, body);
            }
        }
        vault
    }
}

fn sanitize(text: &str) -> String {
    text.replace(['\r', RECORD, FIELD], "")
}
