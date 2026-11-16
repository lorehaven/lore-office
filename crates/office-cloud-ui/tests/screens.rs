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
        });
        c.install();
        cloud = Some(c);
        mount(&r, &root, overlay());
    });
    (r, cloud.unwrap(), seen, opened, scope)
}

#[quench_test::test]
fn without_a_server_the_menu_only_offers_the_panel_to_pick_one() {
    let cloud = Cloud::new(
        Context::default(),
        Rc::new(Fake {
            seen: Rc::default(),
            answer: Rc::new(|_, _| reply(404, &[], "")),
        }),
    );
    cloud.install();
    // A separator and "Cloud…": no saving, since there is nowhere to save to.
    assert_eq!(menu_items().len(), 2);
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
    });
    down.save();
    assert!(down.status.get_untracked().contains("Someone else changed"));
    let context = Context::from_json(
        &Context {
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
    s.clock.advance(1000.0);
    s.revision.set(2);
    s.clock.advance(1500.0);
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
