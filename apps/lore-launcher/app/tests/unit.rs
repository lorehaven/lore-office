use lore_launcher_app::{Links, launcher};
use quench_ui::prelude::*;
use std::rc::Rc;

#[quench_test::test]
fn the_links_travel_as_json_and_the_page_has_a_tile_for_each_app() {
    let links = Links {
        docs: "http://localhost:1/".into(),
        sheets: "/s/".into(),
        notes: "https://n.example/?a=\"b\"".into(),
        pdf: "/p/".into(),
    };
    assert_eq!(Links::from_json(&links.to_json()), links);
    assert_eq!(Links::from_json("garbage"), Links::default());
    assert_eq!(Links::from_json("{\"docs\":\"/d/\"}").docs, "/d/");
    assert_eq!(Links::from_json("{\"docs\":\"/d/\"}").pdf, "/lore-pdf/");
    let r = Rc::new(TestRenderer::new());
    let root = r.root();
    let (_, _scope) = create_root(|| mount(&r, &root, launcher(&links)));
    let anchors = r.find_by_tag("a");
    let hrefs: Vec<String> = anchors.iter().filter_map(|a| r.attr(*a, "href")).collect();
    assert_eq!(
        hrefs,
        [
            "http://localhost:1/",
            "/s/",
            "https://n.example/?a=\"b\"",
            "/p/"
        ]
    );
    assert!(r.text_of(anchors[0]).contains("Documents"));
}

#[quench_test::test]
fn the_page_follows_the_language_and_the_catalogues_agree() {
    let r = Rc::new(TestRenderer::new());
    let root = r.root();
    let (_, _scope) = create_root(|| mount(&r, &root, launcher(&Links::default())));
    assert!(
        r.find_by_tag("h1")
            .iter()
            .any(|h| r.text_of(*h) == "What would you like to work on?")
    );
    let select = r.find_by_tag("select")[0];
    r.fire(select, &Event::Input("de-DE".into()));
    assert!(
        r.find_by_tag("h1")
            .iter()
            .any(|h| r.text_of(*h) == "Woran m\u{f6}chten Sie arbeiten?")
    );
    assert!(
        r.find_by_tag("span")
            .iter()
            .any(|s| r.text_of(*s) == "Dokumente")
    );
    let english = office_i18n::ids(include_str!("../i18n/en-US.ftl"));
    for file in [
        include_str!("../i18n/de-DE.ftl"),
        include_str!("../i18n/es-ES.ftl"),
        include_str!("../i18n/fr-FR.ftl"),
        include_str!("../i18n/pl-PL.ftl"),
    ] {
        assert_eq!(office_i18n::ids(file), english);
    }
}

quench_test::main!();
