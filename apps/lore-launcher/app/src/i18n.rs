//! Languages.
//!
//! The catalogues are Fluent files, one per locale; labels are written in English in the code and found by an id made from
//! the English text (`office-i18n`). The choice is kept by the host (the browser's storage, `q:language`).

use office_i18n::Lang;
use std::cell::RefCell;
use std::rc::Rc;

/// The locales to pick from, the first is the fallback.
pub const CODES: &[&str] = &["en-US", "de-DE", "es-ES", "fr-FR", "pl-PL"];

const LOCALES: [(&str, &str); 5] = [
    ("en-US", include_str!("../i18n/en-US.ftl")),
    ("de-DE", include_str!("../i18n/de-DE.ftl")),
    ("es-ES", include_str!("../i18n/es-ES.ftl")),
    ("fr-FR", include_str!("../i18n/fr-FR.ftl")),
    ("pl-PL", include_str!("../i18n/pl-PL.ftl")),
];

type Hook = Rc<dyn Fn(&str)>;

thread_local! {
    static LANG: Lang = Lang::new(&LOCALES);
    static ON_CHANGE: RefCell<Option<Hook>> = const { RefCell::new(None) };
}

/// Back to English, and `office-ui`'s labels pointed at this module. A new document starts here.
pub fn reset() {
    LANG.with(Lang::reset);
}

/// The locale in use.
#[must_use]
pub fn language() -> &'static str {
    LANG.with(Lang::language)
}

/// Use a locale; returns the one found for the code.
#[allow(clippy::must_use_candidate)]
pub fn use_language(code: &str) -> &'static str {
    LANG.with(|l| l.set(code))
}

/// Called with the locale the person picked, so the app can keep it.
pub fn on_language(f: impl Fn(&str) + 'static) {
    ON_CHANGE.with(|c| *c.borrow_mut() = Some(Rc::new(f)));
}

/// Tell whoever asked (see [`on_language`]) that the language was changed.
pub fn announce(code: &str) {
    if let Some(f) = ON_CHANGE.with(|c| c.borrow().clone()) {
        f(code);
    }
}

/// A catalogue entry by its id, or the id itself when there is none.
#[must_use]
pub fn text(id: &str) -> String {
    LANG.with(|l| l.text(id))
}

/// An English label in the language in use.
#[must_use]
pub fn tr(english: &str) -> String {
    LANG.with(|l| l.tr(english))
}
