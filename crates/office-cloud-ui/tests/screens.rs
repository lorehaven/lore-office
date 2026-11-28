#![allow(
    clippy::unwrap_used,
    clippy::type_complexity,
    clippy::unnested_or_patterns,
    clippy::ignored_unit_patterns,
    clippy::assert_is_empty
)]
use office_cloud_ui::{
    Binding, Cloud, Context, Panel, Reply, Transport, menu_items, overlay, save_shortcut,
    saves_to_cloud, server_page,
};
use quench_ui::prelude::*;
use quench_ui::test_renderer::NodeId;
use std::cell::RefCell;
use std::rc::Rc;

type Seen = Rc<RefCell<Vec<(String, String, Vec<(String, String)>, Vec<u8>)>>>;

struct Fake {
    seen: Seen,
    answer: Rc<dyn Fn(&str, &str) -> Reply>,
}

impl Transport for Fake {
    fn send(
        &self,
        method: &str,
        url: &str,
        headers: Vec<(String, String)>,
        body: Vec<u8>,
        done: Box<dyn FnOnce(Reply)>,
    ) {
        self.seen
            .borrow_mut()
            .push((method.into(), url.into(), headers, body));
        done((self.answer)(method, url));
    }
}

fn reply(status: u16, headers: &[(&str, &str)], body: &str) -> Reply {
    Reply {
        status,
        headers: headers
            .iter()
            .map(|(k, v)| ((*k).into(), (*v).into()))
            .collect(),
        body: body.as_bytes().to_vec(),
    }
}

const META: &str =
    r#"{"id":"ab12","name":"Plan.docx","kind":"docs","size":2048,"modified":5,"etag":"e1"}"#;

fn setup(
    user: Option<&str>,
    share: bool,
) -> (
    Rc<TestRenderer>,
    Cloud,
    Seen,
    Rc<RefCell<Vec<(String, Vec<u8>)>>>,
    Scope,
) {
    let seen: Seen = Rc::default();
    let opened: Rc<RefCell<Vec<(String, Vec<u8>)>>> = Rc::default();
    let transport = Fake {
        seen: seen.clone(),
        answer: Rc::new(|m, u| match (m, u) {
            ("GET", u) if u.starts_with("/api/docs?") => reply(200, &[], &format!("[{META}]")),
            ("GET", "/api/usage") => reply(200, &[], r#"{"used":2048,"quota":10737418240}"#),
            ("GET", "/api/docs/ab12") | ("GET", "/api/s/tok") => {
                reply(200, &[("x-name", "Plan.docx"), ("x-etag", "e1")], "FILE")
            }
            ("PUT", _) => reply(200, &[], META),
            ("POST", u) if u.contains("/share") => reply(
                200,
                &[("x-link", "https://h/lore-office/docs?share=T")],
                &format!(
                    r#"{{"token":"{}","owner":"u","doc":"ab12","name":"Plan.docx","kind":"docs","rights":"View","created":1,"expires":null}}"#,
                    "T"
                ),
            ),
            ("GET", "/api/shares" | "/api/docs/ab12/grants") => reply(200, &[], "[]"),
            ("GET", "/api/folders") => reply(200, &[], r#"["Work"]"#),
            ("GET", "/api/grants") => reply(
                200,
                &[],
                r#"[{"owner":"carol","doc":"cd34","with":"bob","name":"Budget.docx","kind":"docs","rights":"Edit","created":1}]"#,
            ),
            ("GET", "/api/shared/carol/cd34") => reply(
                200,
                &[
                    ("x-name", "Budget.docx"),
                    ("x-etag", "g1"),
                    ("x-rights", "edit"),
                ],
                "SHARED",
            ),
            ("POST", u) if u.contains("/grant?") => reply(
                200,
                &[],
                r#"{"owner":"u","doc":"ab12","with":"dave","name":"Plan.docx","kind":"docs","rights":"Edit","created":2}"#,
            ),
            ("DELETE", _) => reply(204, &[], ""),
            _ => reply(404, &[], ""),
        }),
    };
    let context = Context {
        open: None,
        gatehouse: "https://h/gatehouse".into(),
        api: "/api".into(),
        user: user.map(str::to_string),
        denied: None,
        login: "/ui/login".into(),
        share: share.then(|| office_cloud_ui::SharedWith {
            token: "tok".into(),
            edit: true,
        }),
    };
    let r = Rc::new(TestRenderer::new());
    let root = r.root();
    let (mut cloud, got) = (None, opened.clone());
    let (_, scope) = create_root(|| {
        let c = Cloud::new(context, Rc::new(transport));
        c.bind(Binding {
            kind: "docs",
            export: Rc::new(|| Some(("Plan.docx".to_string(), b"BYTES".to_vec()))),
            import: Rc::new(move |n, b| {
                got.borrow_mut().push((n.to_string(), b.to_vec()));
                Ok(())
            }),
            revision: Rc::new(|| 0),
            autosave: false,
            position: None,
            delta: None,
        });
        c.install();
        cloud = Some(c);
        mount(&r, &root, overlay());
    });
    (r, cloud.unwrap(), seen, opened, scope)
}

#[quench_test::test]
fn without_forge_the_menu_offers_nothing_about_the_cloud() {
    let cloud = Cloud::new(
        Context::default(),
        Rc::new(Fake {
            seen: Rc::default(),
            answer: Rc::new(|_, _| reply(404, &[], "")),
        }),
    );
    cloud.install();
    assert!(menu_items().is_empty());
    assert!(!cloud.enabled());
}

#[quench_test::test]
fn signed_out_the_panel_offers_signing_in_and_signed_in_it_lists_opens_saves_and_deletes() {
    let (r, cloud, seen, opened, _scope) = setup(None, false);
    cloud.open_panel();
    let links = r.find_by_tag("a");
    assert_eq!(r.attr(links[0], "href").as_deref(), Some("/ui/login"));
    assert!(
        seen.borrow().is_empty(),
        "nothing is asked of the server before signing in"
    );
    let (r, cloud, seen, opened2, _scope) = setup(Some("alice"), false);
    assert_eq!(
        menu_items().len(),
        4,
        "a separator, Cloud, Save to the cloud and Save a copy"
    );
    cloud.open_panel();
    assert!(
        r.text_of(r.find_by_attr("class", "cloud-who")[0])
            .contains("alice")
    );
    assert_eq!(
        r.find_by_attr("class", "cloud-item").len(),
        1,
        "one document, at the top"
    );
    assert!(seen.borrow().iter().any(|s| s.1 == "/api/docs?kind=docs"));
    assert!(seen.borrow().iter().any(|s| s.1 == "/api/folders"));
    // Open: pick it, then Open. The file goes to the app and later saves go over the same document, saying which version
    // they began from.
    // (The folder is the first card, the document the second.)
    r.fire(r.find_by_attr("class", "cloud-item-main")[1], &Event::Click);
    assert_eq!(cloud.selected.get_untracked(), ["ab12"]);
    let open = r
        .find_by_tag("button")
        .into_iter()
        .find(|b| r.text_of(*b) == "Open")
        .unwrap();
    r.fire(open, &Event::Click);
    assert_eq!(
        opened2.borrow()[0],
        ("Plan.docx".to_string(), b"FILE".to_vec())
    );
    assert_eq!(cloud.panel.get_untracked(), Panel::Closed);
    cloud.save();
    let put = seen
        .borrow()
        .iter()
        .find(|s| s.0 == "PUT")
        .cloned()
        .unwrap();
    assert_eq!(put.1, "/api/docs/ab12?name=Plan.docx&kind=docs");
    assert_eq!(put.2, [("if-match".to_string(), "e1".to_string())]);
    assert_eq!(put.3, b"BYTES");
    let _ = opened;
}

#[quench_test::test]
fn a_new_document_is_saved_as_new_and_links_are_made_and_withdrawn() {
    let (r, cloud, seen, _o, _scope) = setup(Some("bob"), false);
    cloud.save();
    assert_eq!(
        seen.borrow().last().unwrap().1,
        "/api/docs?name=Plan.docx&kind=docs"
    );
    cloud.open_panel();
    r.fire(r.find_by_attr("class", "cloud-item-main")[1], &Event::Click);
    let share = r
        .find_by_tag("button")
        .into_iter()
        .find(|b| r.text_of(*b).starts_with("Share"))
        .unwrap();
    r.fire(share, &Event::Click);
    assert!(matches!(cloud.panel.get_untracked(), Panel::Sharing(ref id) if id == "ab12"));
    let edit_box = r.find_by_attr("type", "checkbox")[0];
    r.fire(edit_box, &Event::Click);
    assert!(cloud.rights_edit.get_untracked());
    cloud.days.set("7".to_string());
    let make = r
        .find_by_tag("button")
        .into_iter()
        .find(|b| r.text_of(*b) == "Make a link")
        .unwrap();
    r.fire(make, &Event::Click);
    assert!(
        seen.borrow()
            .iter()
            .any(|s| s.0 == "POST" && s.1 == "/api/docs/ab12/share?rights=edit&days=7")
    );
    assert_eq!(
        cloud.link.get_untracked(),
        "https://h/lore-office/docs?share=T"
    );
    assert_eq!(
        r.attr(r.find_by_attr("class", "cloud-link")[0], "value")
            .as_deref(),
        Some("https://h/lore-office/docs?share=T")
    );
    // A name outside ASCII is sent as UTF-8 bytes.
    cloud.bind(Binding {
        kind: "docs",
        export: Rc::new(|| Some(("Zażółć.docx".into(), vec![1]))),
        import: Rc::new(|_, _| Ok(())),
        revision: Rc::new(|| 0),
        autosave: false,
        position: None,
        delta: None,
    });
    cloud.opened.set(None);
    cloud.save();
    assert_eq!(
        seen.borrow().last().unwrap().1,
        "/api/docs?name=Za%C5%BC%C3%B3%C5%82%C4%87.docx&kind=docs"
    );
}

#[quench_test::test]
fn a_page_opened_from_a_link_loads_it_and_saves_through_it_and_failures_are_said_in_words() {
    let (r, cloud, seen, opened, _scope) = setup(None, true);
    assert_eq!(
        menu_items().len(),
        3,
        "a link with edit rights can save without signing in"
    );
    cloud.open_share();
    assert_eq!(opened.borrow()[0].1, b"FILE");
    assert!(cloud.status.get_untracked().contains("you can edit"));
    cloud.save();
    let put = seen
        .borrow()
        .iter()
        .find(|s| s.0 == "PUT")
        .cloned()
        .unwrap();
    assert_eq!(put.1, "/api/s/tok");
    assert_eq!(put.2, [("if-match".to_string(), "e1".to_string())]);
    assert!(!r.find_by_attr("class", "cloud-banner").is_empty());
    // Taken away: the answer says so.
    let down = Cloud::new(
        Context {
            open: None,
            gatehouse: "https://h/gatehouse".into(),
            api: "/api".into(),
            ..Context::default()
        },
        Rc::new(Fake {
            seen: Rc::default(),
            answer: Rc::new(|_, _| reply(409, &[], "")),
        }),
    );
    down.bind(Binding {
        kind: "docs",
        export: Rc::new(|| Some(("a".into(), vec![]))),
        import: Rc::new(|_, _| Ok(())),
        revision: Rc::new(|| 0),
        autosave: false,
        position: None,
        delta: None,
    });
    down.save();
    assert!(down.status.get_untracked().contains("Someone else changed"));
    let context = Context::from_json(
        &Context {
            open: None,
            gatehouse: "https://h/gatehouse".into(),
            api: "/x/api".into(),
            user: Some("é".into()),
            denied: Some("d".into()),
            login: "/l".into(),
            share: None,
        }
        .to_json(),
    );
    assert_eq!(
        (context.api.as_str(), context.user.as_deref()),
        ("/x/api", Some("é"))
    );
    assert_eq!(Context::from_json("not json"), Context::default());
}

quench_test::main!();

#[quench_test::test]
fn a_server_address_becomes_the_page_of_the_app_on_it() {
    assert_eq!(server_page("", "docs"), "");
    assert_eq!(
        server_page("https://ennor.ddns.net", "docs"),
        "https://ennor.ddns.net/lore-office/docs"
    );
    assert_eq!(
        server_page("ennor.ddns.net/", "sheets"),
        "https://ennor.ddns.net/lore-office/sheets"
    );
    assert_eq!(
        server_page(" https://example.org/office/ ", "pdf"),
        "https://example.org/office/pdf"
    );
}

/// A cloud whose document is `ab12`, with a clock that moves when told to, a revision the test changes, and a server whose
/// version of the document (its etag) the test changes.
struct Sync {
    cloud: Cloud,
    seen: Seen,
    imported: Rc<RefCell<Vec<(String, Vec<u8>)>>>,
    remote: Rc<RefCell<String>>,
    conflict: Rc<std::cell::Cell<bool>>,
    revision: Signal<u64>,
    clock: Rc<quench_ui::platform::ManualPlatform>,
    _scope: Scope,
}

fn sync(autosave: bool) -> Sync {
    let clock = Rc::new(quench_ui::platform::ManualPlatform::new());
    quench_ui::platform::install(clock.clone());
    let seen: Seen = Rc::default();
    let imported: Rc<RefCell<Vec<(String, Vec<u8>)>>> = Rc::default();
    let remote = Rc::new(RefCell::new("e1".to_string()));
    let said = remote.clone();
    let conflict: Rc<std::cell::Cell<bool>> = Rc::default();
    let refuse = conflict.clone();
    let transport = Fake {
        seen: seen.clone(),
        answer: Rc::new(move |m, u| {
            let etag = said.borrow().clone();
            match (m, u) {
                ("GET", "/api/docs/ab12/meta") => reply(
                    200,
                    &[],
                    &format!(
                        r#"{{"id":"ab12","name":"Plan.docx","kind":"docs","size":4,"modified":5,"etag":"{etag}"}}"#
                    ),
                ),
                ("GET", "/api/docs/ab12") => {
                    reply(200, &[("x-name", "Plan.docx"), ("x-etag", &etag)], "THEIRS")
                }
                ("PUT", _) if refuse.get() => reply(409, &[("x-etag", &etag)], "changed"),
                ("PUT", _) => {
                    *said.borrow_mut() = "mine".to_string();
                    reply(
                        200,
                        &[],
                        r#"{"id":"ab12","name":"Plan.docx","kind":"docs","size":5,"modified":6,"etag":"mine"}"#,
                    )
                }
                _ => reply(404, &[], ""),
            }
        }),
    };
    let context = Context {
        open: None,
        gatehouse: "https://h/gatehouse".into(),
        api: "/api".into(),
        user: Some("alice".into()),
        denied: None,
        login: "/ui/login".into(),
        share: None,
    };
    let (mut made, mut revision) = (None, None);
    let got = imported.clone();
    let (_, scope) = create_root(|| {
        let rev = create_signal(0_u64);
        let c = Cloud::new(context, Rc::new(transport));
        let read = rev.clone();
        c.bind(Binding {
            kind: "docs",
            export: Rc::new(|| Some(("Plan.docx".to_string(), b"MINE!".to_vec()))),
            import: Rc::new(move |n, b| {
                got.borrow_mut().push((n.to_string(), b.to_vec()));
                Ok(())
            }),
            revision: Rc::new(move || read.get()),
            autosave,
            position: None,
            delta: None,
        });
        c.install();
        made = Some(c);
        revision = Some(rev);
    });
    Sync {
        cloud: made.unwrap(),
        seen,
        imported,
        remote,
        conflict,
        revision: revision.unwrap(),
        clock,
        _scope: scope,
    }
}

fn puts(s: &Sync) -> Vec<(String, Vec<(String, String)>)> {
    s.seen
        .borrow()
        .iter()
        .filter(|(m, ..)| m == "PUT")
        .map(|(_, u, h, _)| (u.clone(), h.clone()))
        .collect()
}

#[quench_test::test]
fn ctrl_s_saves_to_the_cloud_when_signed_in_and_to_a_file_otherwise() {
    let s = sync(false);
    assert!(saves_to_cloud());
    assert!(save_shortcut());
    let sent = puts(&s);
    assert_eq!(sent.len(), 1);
    assert!(
        sent[0].0.starts_with("/api/docs?name=Plan.docx&kind=docs"),
        "{}",
        sent[0].0
    );
    // Nobody signed in, no link: the app saves a file itself.
    let (_r, _cloud, _seen, _opened, _scope) = setup(None, false);
    assert!(!saves_to_cloud());
    assert!(!save_shortcut());
    // A link that lets them edit counts; one that only views does not.
    let (_r, _cloud, _seen, _opened, _scope2) = setup(None, true);
    assert!(saves_to_cloud());
}

#[quench_test::test]
fn a_document_that_someone_else_saved_is_loaded_by_itself_when_the_person_has_changed_nothing() {
    let s = sync(false);
    s.cloud.open_doc("ab12");
    assert_eq!(s.imported.borrow().len(), 1);
    // Nothing new: nothing is fetched but the meta.
    s.clock.advance(5000.0);
    assert_eq!(s.imported.borrow().len(), 1);
    // Someone saved.
    *s.remote.borrow_mut() = "e2".to_string();
    s.clock.advance(5000.0);
    assert_eq!(s.imported.borrow().len(), 2);
    assert_eq!(s.imported.borrow()[1].1, b"THEIRS");
    assert_eq!(s.cloud.opened.get_untracked().unwrap().etag, "e2");
    assert!(s.cloud.status.get_untracked().contains("Updated"));
    assert!(!s.cloud.dirty.get_untracked());
}

#[quench_test::test]
fn with_changes_of_their_own_the_person_chooses_between_the_two_versions() {
    let s = sync(false);
    s.cloud.open_doc("ab12");
    s.revision.set(1);
    s.clock.advance(400.0);
    assert!(s.cloud.dirty.get_untracked(), "a change is noticed");
    *s.remote.borrow_mut() = "e2".to_string();
    s.clock.advance(5000.0);
    assert_eq!(
        s.imported.borrow().len(),
        1,
        "their version is not forced on the screen"
    );
    assert_eq!(s.cloud.newer.get_untracked().unwrap().etag, "e2");
    // Keep mine: the save goes over theirs.
    s.cloud.keep_mine();
    let sent = puts(&s);
    assert_eq!(sent.len(), 1);
    assert!(
        sent[0].1.iter().any(|(k, v)| k == "if-match" && v == "e2"),
        "{:?}",
        sent[0].1
    );
    assert!(s.cloud.newer.get_untracked().is_none());
    assert!(!s.cloud.dirty.get_untracked());
}

#[quench_test::test]
fn their_version_replaces_the_screen_when_chosen() {
    let s = sync(false);
    s.cloud.open_doc("ab12");
    s.revision.set(1);
    s.clock.advance(400.0);
    *s.remote.borrow_mut() = "e2".to_string();
    s.clock.advance(5000.0);
    s.cloud.reload();
    assert_eq!(s.imported.borrow().len(), 2);
    assert!(s.cloud.newer.get_untracked().is_none());
    assert!(!s.cloud.dirty.get_untracked());
}

#[quench_test::test]
fn a_document_in_the_cloud_is_saved_by_itself_a_moment_after_the_last_change() {
    let s = sync(true);
    s.cloud.open_doc("ab12");
    s.revision.set(1);
    s.clock.advance(600.0);
    s.revision.set(2);
    s.clock.advance(900.0);
    assert!(
        puts(&s).is_empty(),
        "the quiet moment starts again with each change"
    );
    s.clock.advance(1000.0);
    let sent = puts(&s);
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert!(sent[0].1.iter().any(|(k, v)| k == "if-match" && v == "e1"));
    assert!(!s.cloud.dirty.get_untracked());
    assert_eq!(s.cloud.opened.get_untracked().unwrap().etag, "mine");
    // A save of its own is no change of someone else's: the next look finds nothing new.
    s.clock.advance(5000.0);
    assert_eq!(s.imported.borrow().len(), 1);
    assert!(s.cloud.newer.get_untracked().is_none());
}

#[quench_test::test]
fn a_save_over_someone_elses_is_a_conflict_the_person_settles_not_a_silent_overwrite() {
    let s = sync(true);
    s.cloud.open_doc("ab12");
    // They saved first, and the server refuses our save (the version we began from is not current).
    *s.remote.borrow_mut() = "e2".to_string();
    s.conflict.set(true);
    s.revision.set(1);
    s.clock.advance(3000.0);
    assert_eq!(puts(&s).len(), 1, "the save was tried");
    assert_eq!(s.cloud.newer.get_untracked().unwrap().etag, "e2");
    assert!(s.cloud.dirty.get_untracked());
    assert_eq!(
        s.imported.borrow().len(),
        1,
        "nothing was replaced on the screen"
    );
    // No more tries by themselves while the person has not chosen.
    s.revision.set(2);
    s.clock.advance(3000.0);
    assert_eq!(puts(&s).len(), 1);
}

fn button_named(r: &TestRenderer, text: &str) -> NodeId {
    r.find_by_tag("button")
        .into_iter()
        .find(|b| r.text_of(*b).starts_with(text))
        .unwrap_or_else(|| panic!("no button {text}"))
}

#[quench_test::test]
fn the_panel_shows_folders_as_cards_or_a_list_and_picked_documents_move_between_them() {
    let (r, cloud, seen, _o, _scope) = setup(Some("bob"), false);
    cloud.open_panel();
    // The top: the folder `Work` and the document.
    assert_eq!(r.find_by_attr("class", "cloud-item folder").len(), 1);
    assert_eq!(r.find_by_attr("class", "cloud-item").len(), 1);
    // Cards or a list, as the person likes.
    assert!(!r.find_by_attr("class", "cloud-items list").is_empty());
    r.fire(r.find_by_attr("title", "Cards")[0], &Event::Click);
    assert!(cloud.grid.get_untracked());
    assert!(!r.find_by_attr("class", "cloud-items grid").is_empty());
    // Nothing is picked: nothing can be moved, shared or deleted.
    assert!(r.attr(button_named(&r, "Delete"), "disabled").is_some());
    r.fire(r.find_by_attr("class", "cloud-item-main")[1], &Event::Click);
    // Move to: the menu lists the top and the folders.
    r.fire(button_named(&r, "Move to"), &Event::Click);
    let items: Vec<String> = r
        .find_by_attr("class", "menu-item")
        .into_iter()
        .map(|n| r.text_of(n))
        .collect();
    assert_eq!(items, ["Top (no folder)", "Work"]);
    r.fire(r.find_by_attr("class", "menu-item")[1], &Event::Click);
    assert!(
        seen.borrow()
            .iter()
            .any(|s| s.0 == "PUT" && s.1 == "/api/docs/ab12/move?folder=Work"),
        "{:?}",
        seen.borrow()
            .iter()
            .map(|s| (&s.0, &s.1))
            .collect::<Vec<_>>()
    );
    // Into a folder: the list shows it, and a new folder is made inside it.
    cloud.go("Work");
    assert_eq!(cloud.folder.get_untracked(), "Work");
    cloud.folder_name.set("2026".to_string());
    cloud.make_folder();
    assert!(
        seen.borrow()
            .iter()
            .any(|s| s.0 == "POST" && s.1 == "/api/folders?path=Work%2F2026")
    );
    // A document saved while a folder is shown goes into it.
    cloud.opened.set(None);
    cloud.save();
    let saved = seen
        .borrow()
        .iter()
        .rfind(|s| s.0 == "PUT")
        .cloned()
        .unwrap();
    assert_eq!(saved.1, "/api/docs?name=Plan.docx&kind=docs&folder=Work");
}

#[quench_test::test]
fn what_others_shared_is_opened_from_their_cloud_and_saved_back_there() {
    let (r, cloud, seen, opened, _scope) = setup(Some("bob"), false);
    cloud.open_panel();
    cloud.go_shared();
    assert_eq!(r.find_by_attr("class", "cloud-item").len(), 1);
    assert!(
        r.text_of(r.find_by_attr("class", "cloud-item")[0])
            .contains("carol")
    );
    r.fire(r.find_by_attr("class", "cloud-item-main")[0], &Event::Click);
    assert_eq!(cloud.selected.get_untracked(), ["carol/cd34"]);
    r.fire(button_named(&r, "Open"), &Event::Click);
    assert_eq!(opened.borrow().last().unwrap().1, b"SHARED");
    let now = cloud.opened.get_untracked().unwrap();
    assert_eq!(
        (now.owner.as_deref(), now.edit, now.etag.as_str()),
        (Some("carol"), true, "g1")
    );
    // Saved back into the owner's cloud, from the version it began from.
    cloud.save();
    let put = seen
        .borrow()
        .iter()
        .rfind(|s| s.0 == "PUT")
        .cloned()
        .unwrap();
    assert_eq!(put.1, "/api/shared/carol/cd34");
    assert_eq!(put.2, [("if-match".to_string(), "g1".to_string())]);
    // And a document of one's own is shared with a person by name.
    cloud.opened.set(None);
    cloud.go("");
    cloud.panel.set(Panel::Sharing("ab12".into()));
    cloud.person.set("dave".to_string());
    cloud.person_edit.set(true);
    cloud.share_with("ab12");
    assert!(
        seen.borrow()
            .iter()
            .any(|s| s.0 == "POST" && s.1 == "/api/docs/ab12/grant?user=dave&rights=edit")
    );
}

#[quench_test::test]
fn someone_who_types_without_a_pause_is_still_saved_after_the_longest_wait() {
    let s = sync(true);
    s.cloud.open_doc("ab12");
    // A change every 300 ms: never quiet for the second the save waits for.
    for n in 1..=23_u64 {
        s.revision.set(n);
        s.clock.advance(300.0);
    }
    assert!(puts(&s).is_empty(), "not yet: about 7 s of typing");
    for n in 24..=31_u64 {
        s.revision.set(n);
        s.clock.advance(300.0);
    }
    assert!(
        !puts(&s).is_empty(),
        "after 8 s it was sent although the typing went on"
    );
}

/// A document's log as the service keeps it, and the pages' side of it: what the app has to send, and what it was told to apply.
#[derive(Default)]
struct LogState {
    head: u64,
    version: u64,
    ops: Vec<(u64, String)>,
    /// Whether the service holds the question about who is here (the tests that do not look at it get a 404).
    presence: bool,
}

struct LogServer {
    state: Rc<RefCell<LogState>>,
    seen: Seen,
}

impl Transport for LogServer {
    fn send(
        &self,
        method: &str,
        url: &str,
        headers: Vec<(String, String)>,
        body: Vec<u8>,
        done: Box<dyn FnOnce(Reply)>,
    ) {
        self.seen
            .borrow_mut()
            .push((method.into(), url.into(), headers, body.clone()));
        let path = url.strip_prefix("/api").unwrap_or(url);
        let (route, query) = path.split_once('?').unwrap_or((path, ""));
        let param = |k: &str| {
            query
                .split('&')
                .find_map(|p| p.strip_prefix(&format!("{k}=")))
                .and_then(|v| v.parse::<u64>().ok())
        };
        let mut st = self.state.borrow_mut();
        let numbers = (st.version.to_string(), st.head.to_string());
        let answer = match (method, route) {
            ("GET", "/docs/ab12") => reply(
                200,
                &[
                    ("x-name", "Book.xlsx"),
                    ("x-etag", "e1"),
                    ("x-version", &numbers.0),
                    ("x-head", &numbers.1),
                ],
                "FILE",
            ),
            ("GET", "/docs/ab12/ops") => {
                let after = param("after").unwrap_or(0);
                if after < st.version {
                    reply(
                        410,
                        &[],
                        &format!(r#"{{"head":{},"version":{}}}"#, st.head, st.version),
                    )
                } else {
                    let list: Vec<String> = st
                        .ops
                        .iter()
                        .filter(|(n, _)| *n > after)
                        .map(|(n, op)| format!(r#"{{"n":{n},"op":{op}}}"#))
                        .collect();
                    reply(
                        200,
                        &[],
                        &format!(
                            r#"{{"head":{},"version":{},"ops":[{}]}}"#,
                            st.head,
                            st.version,
                            list.join(",")
                        ),
                    )
                }
            }
            ("GET", "/docs/ab12/presence") if st.presence => reply(
                200,
                &[],
                r##"{"rev":2,"doc":1,"peers":[{"client":"bb","name":"Fox","color":"#3b82d6","pos":"1,2"},{"client":"aa","name":"Fox","color":"#d9534f","pos":""}]}"##,
            ),
            ("POST", "/docs/ab12/presence") => reply(200, &[], r#"{"rev":2}"#),
            ("POST", "/docs/ab12/ops") => {
                if param("base") == Some(st.head) {
                    st.head += 1;
                    let n = st.head;
                    st.ops.push((n, String::from_utf8_lossy(&body).to_string()));
                    reply(200, &[], &format!(r#"{{"n":{n}}}"#))
                } else {
                    reply(409, &[], &format!(r#"{{"head":{}}}"#, st.head))
                }
            }
            ("PUT", "/docs/ab12") => match param("at") {
                Some(at) if at == st.head => {
                    st.version = at;
                    st.ops.clear();
                    reply(200, &[], META)
                }
                Some(_) => reply(409, &[], &format!(r#"{{"head":{}}}"#, st.head)),
                None => reply(200, &[], META),
            },
            _ => reply(404, &[], ""),
        };
        drop(st);
        done(answer);
    }
}

struct Following {
    cloud: Cloud,
    seen: Seen,
    server: Rc<RefCell<LogState>>,
    /// What the app has to send, oldest first.
    to_send: Rc<RefCell<std::collections::VecDeque<office_cloud_ui::Change>>>,
    /// What the app was told to apply.
    applied: Rc<RefCell<Vec<String>>>,
    imported: Rc<RefCell<usize>>,
    clock: Rc<quench_ui::platform::ManualPlatform>,
    _scope: Scope,
}

fn following() -> Following {
    let clock = Rc::new(quench_ui::platform::ManualPlatform::new());
    quench_ui::platform::install(clock.clone());
    let seen: Seen = Rc::default();
    let server: Rc<RefCell<LogState>> = Rc::default();
    let to_send: Rc<RefCell<std::collections::VecDeque<office_cloud_ui::Change>>> = Rc::default();
    let applied: Rc<RefCell<Vec<String>>> = Rc::default();
    let imported: Rc<RefCell<usize>> = Rc::default();
    let context = Context {
        open: None,
        gatehouse: "https://h/gatehouse".into(),
        api: "/api".into(),
        user: Some("alice".into()),
        denied: None,
        login: "/ui/login".into(),
        share: None,
    };
    let (mut made, (take, apply, got)) =
        (None, (to_send.clone(), applied.clone(), imported.clone()));
    let transport = LogServer {
        state: server.clone(),
        seen: seen.clone(),
    };
    let (_, scope) = create_root(|| {
        let c = Cloud::new(context, Rc::new(transport));
        let (queue, log) = (take, apply);
        c.bind(Binding {
            kind: "sheets",
            export: Rc::new(|| Some(("Book.xlsx".to_string(), b"WHOLE".to_vec()))),
            import: Rc::new(move |_, _| {
                *got.borrow_mut() += 1;
                Ok(())
            }),
            revision: Rc::new(|| 0),
            autosave: true,
            position: None,
            delta: Some(office_cloud_ui::DeltaHooks {
                take: Rc::new(move || queue.borrow_mut().pop_front()),
                apply: Rc::new(move |op| {
                    log.borrow_mut()
                        .push(String::from_utf8_lossy(op).to_string());
                    Ok(())
                }),
                recompute: false,
                accepted: Rc::new(|| {}),
            }),
        });
        c.install();
        made = Some(c);
    });
    Following {
        cloud: made.unwrap(),
        seen,
        server,
        to_send,
        applied,
        imported,
        clock,
        _scope: scope,
    }
}

fn requests(f: &Following, method: &str) -> Vec<(String, String)> {
    f.seen
        .borrow()
        .iter()
        .filter(|s| s.0 == method && !s.1.contains("/presence"))
        .map(|s| (s.1.clone(), String::from_utf8_lossy(&s.3).to_string()))
        .collect()
}

fn op(text: &str) -> office_cloud_ui::Change {
    office_cloud_ui::Change::Ops(text.as_bytes().to_vec())
}

#[quench_test::test]
fn a_change_is_sent_as_one_numbered_change_after_a_short_quiet_and_the_whole_file_is_not() {
    let f = following();
    f.cloud.open_doc("ab12");
    f.to_send.borrow_mut().push_back(op(r#"{"cells":[1]}"#));
    f.clock.advance(400.0);
    assert!(f.cloud.dirty.get_untracked(), "taken, not yet sent");
    f.clock.advance(800.0);
    assert_eq!(
        requests(&f, "POST"),
        [(
            "/api/docs/ab12/ops?base=0".to_string(),
            r#"{"cells":[1]}"#.to_string()
        )]
    );
    assert!(requests(&f, "PUT").is_empty(), "no whole file");
    assert!(!f.cloud.dirty.get_untracked());
    // The next change begins from the number the service gave.
    f.to_send.borrow_mut().push_back(op(r#"{"cells":[2]}"#));
    f.clock.advance(1200.0);
    assert_eq!(requests(&f, "POST")[1].0, "/api/docs/ab12/ops?base=1");
}

#[quench_test::test]
fn a_change_that_began_behind_catches_up_applies_their_changes_first_then_its_own_and_is_sent_again()
 {
    let f = following();
    f.cloud.open_doc("ab12");
    // Two changes made by others that this page has not seen.
    {
        let mut st = f.server.borrow_mut();
        st.head = 2;
        st.ops = vec![(1, r#"{"theirs":1}"#.into()), (2, r#"{"theirs":2}"#.into())];
    }
    f.to_send.borrow_mut().push_back(op(r#"{"mine":1}"#));
    f.clock.advance(1200.0);
    // Refused (behind), then caught up, then sent from the new number.
    assert_eq!(
        f.applied.borrow().as_slice(),
        [r#"{"theirs":1}"#, r#"{"theirs":2}"#, r#"{"mine":1}"#],
        "theirs, then this page's own again"
    );
    f.clock.advance(1200.0);
    let posts = requests(&f, "POST");
    assert_eq!(posts.last().unwrap().0, "/api/docs/ab12/ops?base=2");
    assert_eq!(f.server.borrow().head, 3);
}

#[quench_test::test]
fn other_pages_changes_arrive_by_looking_every_couple_of_seconds_without_a_reload() {
    let f = following();
    f.cloud.open_doc("ab12");
    f.clock.advance(2000.0);
    assert!(f.applied.borrow().is_empty());
    {
        let mut st = f.server.borrow_mut();
        st.head = 1;
        st.ops = vec![(1, r#"{"theirs":1}"#.into())];
    }
    f.clock.advance(2000.0);
    assert_eq!(f.applied.borrow().as_slice(), [r#"{"theirs":1}"#]);
    assert_eq!(
        *f.imported.borrow(),
        1,
        "the file was fetched once, at the start"
    );
}

#[quench_test::test]
fn changes_folded_into_a_new_snapshot_send_the_page_back_to_the_stored_file() {
    let f = following();
    f.cloud.open_doc("ab12");
    {
        let mut st = f.server.borrow_mut();
        st.head = 5;
        st.version = 5;
    }
    f.clock.advance(2000.0);
    assert_eq!(*f.imported.borrow(), 2, "the stored file was fetched again");
    assert!(f.cloud.status.get_untracked().contains("Updated"));
}

#[quench_test::test]
fn a_change_nothing_describes_sends_the_whole_file_at_the_latest_number() {
    let f = following();
    f.cloud.open_doc("ab12");
    f.server.borrow_mut().head = 0;
    f.to_send
        .borrow_mut()
        .push_back(office_cloud_ui::Change::Whole);
    f.clock.advance(1200.0);
    let puts = requests(&f, "PUT");
    assert_eq!(puts.len(), 1, "{puts:?}");
    assert_eq!(puts[0].0, "/api/docs/ab12?name=Book.xlsx&kind=sheets&at=0");
    assert_eq!(puts[0].1, "WHOLE");
    assert!(!f.cloud.dirty.get_untracked());
}

#[quench_test::test]
fn saving_by_hand_sends_the_whole_file_and_says_so() {
    let f = following();
    f.cloud.open_doc("ab12");
    f.cloud.save();
    f.clock.advance(800.0);
    assert_eq!(requests(&f, "PUT").len(), 1);
    assert!(
        f.cloud
            .status
            .get_untracked()
            .contains("Saved to the cloud")
    );
}

#[quench_test::test]
fn after_fifty_changes_an_idle_page_that_is_up_to_date_folds_them_into_the_stored_file() {
    let f = following();
    {
        let mut st = f.server.borrow_mut();
        st.head = 60;
        st.ops = (1..=60).map(|n| (n, format!(r#"{{"n":{n}}}"#))).collect();
    }
    f.cloud.open_doc("ab12");
    assert_eq!(
        f.applied.borrow().len(),
        60,
        "the file is at version 0 and the changes after it were applied"
    );
    f.clock.advance(1200.0);
    assert!(requests(&f, "PUT").is_empty(), "not while it was busy");
    f.clock.advance(5000.0);
    let puts = requests(&f, "PUT");
    assert_eq!(puts.len(), 1, "{puts:?}");
    assert!(puts[0].0.ends_with("&at=60"), "{}", puts[0].0);
    assert_eq!(f.server.borrow().version, 60);
}

#[quench_test::test]
fn making_the_document_in_the_cloud_sends_the_file_whole_and_what_was_waiting_is_not_sent_again() {
    let f = following();
    f.to_send
        .borrow_mut()
        .push_back(op(r#"{"cells":["typed before the first save"]}"#));
    f.cloud.save();
    assert_eq!(requests(&f, "PUT").len(), 1, "the file");
    f.clock.advance(1600.0);
    assert!(
        requests(&f, "POST").is_empty(),
        "no change that is already in the file"
    );
}

#[quench_test::test]
fn others_in_the_document_are_shown_with_told_apart_names_and_a_change_to_the_document_is_looked_at_at_once()
 {
    let f = following();
    f.server.borrow_mut().presence = true;
    f.cloud.open_doc("ab12");
    let asked = |f: &Following| {
        f.seen
            .borrow()
            .iter()
            .filter(|s| s.1.contains("/ops?after="))
            .count()
    };
    let before = asked(&f);
    f.clock.advance(400.0);
    let names: Vec<String> = f.cloud.others().into_iter().map(|p| p.name).collect();
    assert_eq!(names, ["Fox", "Fox 2"], "the same name twice gets a number");
    assert!(
        asked(&f) > before,
        "the document changed: asked for its changes now"
    );
    // It says it is here.
    let says: Vec<String> = f
        .seen
        .borrow()
        .iter()
        .filter(|s| s.0 == "POST" && s.1.ends_with("/presence"))
        .map(|s| String::from_utf8_lossy(&s.3).to_string())
        .collect();
    assert!(says[0].contains(r#""name":"alice""#), "{says:?}");
    // Leaving the document says so.
    f.cloud.opened.set(None);
    f.clock.advance(400.0);
    assert!(f.seen.borrow().iter().any(|s| s.0 == "POST"
        && s.1.ends_with("/presence")
        && String::from_utf8_lossy(&s.3).contains(r#""gone":true"#)));
    assert!(f.cloud.others().is_empty());
}

#[quench_test::test]
fn the_top_bar_has_the_dashboard_account_and_cloud_only_in_forge() {
    let bar_of = |user: Option<&str>, gatehouse: &str| {
        let r = Rc::new(TestRenderer::new());
        let root = r.root();
        let context = Context {
            api: "/api".into(),
            user: user.map(str::to_string),
            login: "/ui/login".into(),
            gatehouse: gatehouse.into(),
            ..Context::default()
        };
        let transport = Fake {
            seen: Rc::default(),
            answer: Rc::new(|_, _| reply(404, &[], "")),
        };
        let (_, scope) = create_root(|| {
            let cloud = Cloud::new(context, Rc::new(transport));
            cloud.install();
            office_cloud_ui::frame(&cloud);
            let actions = div().child(office_cloud_ui::cloud_button());
            mount(
                &r,
                &root,
                office_ui::topbar("lore-docs", actions).into_view(),
            );
        });
        let found = (
            r.find_by_attr("id", "dashboard").len(),
            r.find_by_attr("aria-label", "Account").len(),
            r.find_by_tag("button").len(),
            office_cloud_ui::menu_items().len(),
            office_cloud_ui::saves_to_cloud(),
        );
        drop(scope);
        found
    };
    let in_forge = bar_of(Some("ada"), "https://h/gatehouse");
    assert_eq!((in_forge.0, in_forge.1, in_forge.2), (1, 1, 1));
    assert!(in_forge.3 > 0 && in_forge.4);
    // Signed out in forge: no account menu; the cloud is offered (it is where signing in is).
    let signed_out = bar_of(None, "https://h/gatehouse");
    assert_eq!((signed_out.0, signed_out.1, signed_out.2), (1, 0, 1));
    // Without forge: nothing about accounts or the cloud, and Ctrl+S saves a file.
    assert_eq!(bar_of(Some("local"), ""), (0, 0, 0, 0, false));
}

#[quench_test::test]
fn edit_profile_goes_to_the_account_page_by_way_of_the_sign_in_page() {
    assert_eq!(
        office_cloud_ui::profile_link("https://h/gatehouse"),
        "https://h/gatehouse/ui/login?redirect=%2Fgatehouse%2Fui%2Faccount"
    );
    // At the root of its own host, and given as a path.
    assert_eq!(
        office_cloud_ui::profile_link("https://auth.example"),
        "https://auth.example/ui/login?redirect=%2Fui%2Faccount"
    );
    assert_eq!(
        office_cloud_ui::profile_link("/gatehouse"),
        "/gatehouse/ui/login?redirect=%2Fgatehouse%2Fui%2Faccount"
    );
}
