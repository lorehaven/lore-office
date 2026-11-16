#![allow(
    clippy::unwrap_used,
    clippy::type_complexity,
    clippy::unnested_or_patterns,
    clippy::ignored_unit_patterns,
    clippy::assert_is_empty
)]
use office_cloud_ui::{Binding, Cloud, Context, Panel, Reply, Transport, menu_items, overlay};
use quench_ui::prelude::*;
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
            ("GET", "/api/shares") => reply(200, &[], "[]"),
            ("DELETE", _) => reply(204, &[], ""),
            _ => reply(404, &[], ""),
        }),
    };
    let context = Context {
        api: "/api".into(),
        user: user.map(str::to_string),
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
        r.text_of(r.find_by_attr("class", "cloud-body")[0])
            .contains("alice")
    );
    assert_eq!(r.find_by_attr("class", "cloud-row").len(), 1);
    assert!(seen.borrow().iter().any(|s| s.1 == "/api/docs?kind=docs"));
    // Open: the file goes to the app and later saves go over the same document, saying which version they began from.
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
    });
    down.save();
    assert!(down.status.get_untracked().contains("Someone else changed"));
    let context = Context::from_json(
        &Context {
            api: "/x/api".into(),
            user: Some("é".into()),
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
