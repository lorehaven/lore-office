//! Languages.
//!
//! The catalogues are Fluent files, one per locale, read through `office-i18n`; the picker is `office-theme`'s and
//! the choice is kept in the browser's storage (`q:language`).
//!
//! Labels are written in English in the code and looked up by an id made from the English text ([`id_for`]); a text a
//! catalogue does not have is shown in English. [`reset`] starts over in English and points `office-ui`'s labels here.

use office_i18n::Catalogs;
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

thread_local! {
    static CATALOGS: Catalogs = Catalogs::new(&LOCALES);
    static CODE: RefCell<&'static str> = const { RefCell::new(CODES[0]) };
    static ON_CHANGE: RefCell<Option<Rc<dyn Fn(&str)>>> = const { RefCell::new(None) };
}

/// Back to English, and `office-ui`'s labels pointed at this module. A new document starts here.
pub fn reset() {
    CODE.with(|c| *c.borrow_mut() = CODES[0]);
    office_ui::set_translator(tr);
}

/// The locale in use.
#[must_use]
pub fn language() -> &'static str {
    CODE.with(|c| *c.borrow())
}

/// The locale a code names (`de-DE`, or the base language alone, `de`); English for one that is not known.
#[must_use]
pub fn locale_of(code: &str) -> &'static str {
    CATALOGS.with(|c| c.locale_of(code))
}

/// Use a locale from now on. Screens follow through [`crate::SheetState::set_language`], which calls this.
pub fn use_language(code: &str) {
    CODE.with(|c| *c.borrow_mut() = locale_of(code));
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

/// The text of a catalogue entry in the language in use, or in English when that language lacks it.
#[must_use]
pub fn message(id: &str) -> Option<String> {
    let now = language();
    CATALOGS.with(|c| c.message(now, id))
}

/// A catalogue entry by its id, or the id itself when there is none.
#[must_use]
pub fn text(id: &str) -> String {
    message(id).unwrap_or_else(|| id.to_string())
}

/// The catalogue id of an English text.
#[must_use]
pub fn id_for(english: &str) -> String {
    office_i18n::id_for(english)
}

/// An English text in the language in use (itself when the catalogue has no entry for it).
#[must_use]
pub fn tr(english: &str) -> String {
    if english.is_empty() {
        return String::new();
    }
    message(&id_for(english)).unwrap_or_else(|| english.to_string())
}
