//! Languages for the lore apps.
//!
//! An app lists its locales and their Fluent files once, keeps the locale in use itself, and asks [`Catalogs::message`] for a
//! text. Labels in the code are English; [`id_for`] turns one into the id its entry has in every catalogue, so adding a
//! label means adding one line to each file. A text a language lacks comes back in the first language listed.

use quench_ui::i18n::Catalog;

/// The parsed catalogues of an app.
pub struct Catalogs {
    list: Vec<(&'static str, Catalog)>,
}

impl std::fmt::Debug for Catalogs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Catalogs")
            .field("locales", &self.codes())
            .finish()
    }
}

impl Catalogs {
    /// The catalogues from `(locale, Fluent text)` pairs; the first locale is the fallback.
    ///
    /// # Panics
    /// When a file does not parse: they are part of the program, so that is a bug to find at once.
    #[must_use]
    pub fn new(files: &[(&'static str, &str)]) -> Self {
        let list = files
            .iter()
            .map(|(code, text)| {
                (
                    *code,
                    Catalog::parse(text).unwrap_or_else(|e| panic!("{code}: {e:?}")),
                )
            })
            .collect();
        Self { list }
    }

    /// The locales, in the order given.
    #[must_use]
    pub fn codes(&self) -> Vec<&'static str> {
        self.list.iter().map(|(c, _)| *c).collect()
    }

    /// The locale a code names (`de-DE`, or the base language alone, `de`); the first locale for one that is unknown.
    #[must_use]
    pub fn locale_of(&self, code: &str) -> &'static str {
        let base = code.split(['-', '_']).next();
        self.list
            .iter()
            .map(|(c, _)| *c)
            .find(|c| *c == code)
            .or_else(|| {
                self.list
                    .iter()
                    .map(|(c, _)| *c)
                    .find(|c| c.split('-').next() == base)
            })
            .or_else(|| self.list.first().map(|(c, _)| *c))
            .unwrap_or_default()
    }

    /// Entry `id` in `language`, or in the first locale when that has none.
    #[must_use]
    pub fn message(&self, language: &str, id: &str) -> Option<String> {
        let first = self.list.first().map(|(c, _)| *c)?;
        [language, first].into_iter().find_map(|code| {
            self.list
                .iter()
                .find(|(c, _)| *c == code)
                .and_then(|(c, catalog)| catalog.format(c, id, &[]))
        })
    }
}

/// The id of an English text: its first words, then a hash so that texts alike in their words stay apart.
#[must_use]
pub fn id_for(english: &str) -> String {
    let mut slug = String::new();
    for c in english.chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
        } else if !slug.ends_with('_') && !slug.is_empty() {
            slug.push('_');
        }
        if slug.len() >= 32 {
            break;
        }
    }
    let hash = english.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    });
    format!("{}_{:06x}", slug.trim_end_matches('_'), hash & 0x00FF_FFFF)
}

/// The ids of a Fluent file, in order (lines of the form `id = text`).
#[must_use]
pub fn ids(file: &str) -> Vec<String> {
    file.lines()
        .filter(|l| !l.starts_with('#') && l.contains(" = "))
        .map(|l| l.split(" = ").next().unwrap_or_default().to_string())
        .collect()
}

/// The language an app is in, with its catalogues: what an app keeps in a `thread_local!` so that every label can ask for
/// its text without being handed anything.
pub struct Lang {
    catalogs: Catalogs,
    code: std::cell::RefCell<&'static str>,
}

impl std::fmt::Debug for Lang {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Lang")
            .field("code", &self.language())
            .finish()
    }
}

impl Lang {
    /// The languages of `files`, in the first of them.
    ///
    /// # Panics
    /// When a file does not parse (see [`Catalogs::new`]).
    #[must_use]
    pub fn new(files: &[(&'static str, &str)]) -> Self {
        let catalogs = Catalogs::new(files);
        let first = catalogs.codes().first().copied().unwrap_or_default();
        Self {
            catalogs,
            code: std::cell::RefCell::new(first),
        }
    }

    /// The locale in use.
    #[must_use]
    pub fn language(&self) -> &'static str {
        *self.code.borrow()
    }

    /// Back to the first language.
    pub fn reset(&self) {
        let first = self.catalogs.codes().first().copied().unwrap_or_default();
        *self.code.borrow_mut() = first;
    }

    /// Use the locale a code names (see [`Catalogs::locale_of`]); returns it.
    pub fn set(&self, code: &str) -> &'static str {
        let found = self.catalogs.locale_of(code);
        *self.code.borrow_mut() = found;
        found
    }

    /// The locales, in the order given.
    #[must_use]
    pub fn codes(&self) -> Vec<&'static str> {
        self.catalogs.codes()
    }

    /// A catalogue entry by its id, or the id itself when there is none.
    #[must_use]
    pub fn text(&self, id: &str) -> String {
        self.catalogs
            .message(self.language(), id)
            .unwrap_or_else(|| id.to_string())
    }

    /// An English label in the language in use (itself when there is no entry for it).
    #[must_use]
    pub fn tr(&self, english: &str) -> String {
        if english.is_empty() {
            return String::new();
        }
        self.catalogs
            .message(self.language(), &id_for(english))
            .unwrap_or_else(|| english.to_string())
    }
}
