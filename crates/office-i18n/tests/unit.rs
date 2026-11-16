use office_i18n::{Catalogs, id_for, ids};

const EN: &str = "hello_x = Hello\nonly_english = Only\n";
const DE: &str = "hello_x = Hallo\n";

#[quench_test::test]
fn texts_come_in_the_language_asked_or_the_first() {
    let c = Catalogs::new(&[("en-US", EN), ("de-DE", DE)]);
    assert_eq!(c.codes(), ["en-US", "de-DE"]);
    assert_eq!(c.message("de-DE", "hello_x").as_deref(), Some("Hallo"));
    assert_eq!(
        c.message("de-DE", "only_english").as_deref(),
        Some("Only"),
        "the first language fills the gap"
    );
    assert_eq!(c.message("fr-FR", "hello_x").as_deref(), Some("Hello"));
    assert_eq!(c.message("en-US", "nothing"), None);
    assert_eq!(
        (c.locale_of("de"), c.locale_of("de-AT"), c.locale_of("xx")),
        ("de-DE", "de-DE", "en-US")
    );
}

#[quench_test::test]
fn ids_are_made_from_the_english_and_listed_from_a_file() {
    assert_eq!(id_for("Bold"), id_for("Bold"));
    assert_ne!(id_for("Bold"), id_for("bold!"));
    assert!(id_for("Paste special").starts_with("paste_special_"));
    assert_eq!(ids("# c\na_1 = A\nb_2 = B\n"), ["a_1", "b_2"]);
}

quench_test::main!();
