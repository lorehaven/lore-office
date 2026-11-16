#![allow(
    clippy::unwrap_used,
    clippy::assert_is_empty,
    clippy::cloned_ref_to_slice_refs
)]
use office_cloud::{
    Error, LocalStore, Rights, Share, Storage, WarehouseStore, clean_folder, grant_doc,
    granted_meta, new_id, now, open_granted, open_share, safe_name, save_granted, save_share,
};

fn run<F: std::future::Future>(f: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap()
        .block_on(f)
}

fn store(quota: u64) -> (LocalStore, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("lore-cloud-test-{}", new_id(6)));
    (LocalStore::new(&dir, quota), dir)
}

#[quench_test::test]
fn a_user_keeps_documents_with_etags_and_only_their_own() {
    run(async {
        let (s, dir) = store(1 << 20);
        let a = s
            .put("alice", None, "Report.docx", "docs", b"one", None)
            .await
            .unwrap();
        s.put("bob", None, "Other.docx", "docs", b"xx", None)
            .await
            .unwrap();
        assert_eq!((a.size, a.kind.as_str()), (3, "docs"));
        assert_eq!(s.list("alice").await.unwrap(), [a.clone()]);
        assert_eq!(s.list("bob").await.unwrap().len(), 1);
        assert_eq!(s.get("alice", &a.id).await.unwrap().1, b"one");
        assert_eq!(
            s.get("bob", &a.id).await,
            Err(Error::NotFound),
            "not another person's document"
        );
        // A save says where it started: the right etag goes through, a stale one is a conflict that names the current one.
        let b = s
            .put(
                "alice",
                Some(&a.id),
                "Report.docx",
                "docs",
                b"two!",
                Some(&a.etag),
            )
            .await
            .unwrap();
        assert_ne!(a.etag, b.etag);
        assert_eq!(
            s.put(
                "alice",
                Some(&a.id),
                "Report.docx",
                "docs",
                b"three",
                Some(&a.etag)
            )
            .await,
            Err(Error::Conflict {
                etag: b.etag.clone()
            })
        );
        assert_eq!(s.get("alice", &a.id).await.unwrap().1, b"two!");
        s.delete("alice", &a.id).await.unwrap();
        assert!(s.list("alice").await.unwrap().is_empty());
        assert_eq!(s.delete("alice", &a.id).await, Err(Error::NotFound));
        for bad in ["../x", "", "zz", "a/b"] {
            assert!(
                matches!(s.get("alice", bad).await, Err(Error::Invalid(_))),
                "{bad}"
            );
        }
        assert!(matches!(
            s.put("alice", None, "x", "exe", b"", None).await,
            Err(Error::Invalid(_))
        ));
        assert!(matches!(
            s.put("alice", None, "  ", "docs", b"", None).await,
            Err(Error::Invalid(_))
        ));
        let _ = std::fs::remove_dir_all(dir);
    });
}

#[quench_test::test]
fn the_quota_counts_what_a_user_keeps_and_a_save_over_a_document_replaces_its_size() {
    run(async {
        let (s, dir) = store(10);
        let a = s
            .put("u", None, "a", "docs", b"123456", None)
            .await
            .unwrap();
        assert_eq!(s.usage("u").await.unwrap().used, 6);
        assert_eq!(
            s.put("u", None, "b", "docs", b"12345", None).await,
            Err(Error::Quota { used: 6, quota: 10 })
        );
        // Replacing a six-byte document with a nine-byte one fits: the old six are not counted twice.
        s.put("u", Some(&a.id), "a", "docs", b"123456789", Some(&a.etag))
            .await
            .unwrap();
        assert_eq!(
            s.usage("u").await.unwrap(),
            office_cloud::Usage { used: 9, quota: 10 }
        );
        // Another person has a quota of their own.
        s.put("v", None, "a", "docs", b"1234567890", None)
            .await
            .unwrap();
        let _ = std::fs::remove_dir_all(dir);
    });
}

#[quench_test::test]
fn names_that_look_alike_get_directories_of_their_own() {
    run(async {
        let (s, dir) = store(1 << 20);
        s.put("a/b", None, "x", "docs", b"1", None).await.unwrap();
        s.put("a_b", None, "y", "docs", b"2", None).await.unwrap();
        assert_eq!(s.list("a/b").await.unwrap()[0].name, "x");
        assert_eq!(s.list("a_b").await.unwrap()[0].name, "y");
        assert_eq!(safe_name("../../etc"), ".._.._etc");
        assert_eq!(safe_name(""), "_");
        assert_eq!(safe_name(".."), "_");
        let _ = std::fs::remove_dir_all(dir);
    });
}

fn share_of(owner: &str, doc: &str, rights: Rights, expires: Option<u64>) -> Share {
    Share {
        token: new_id(32),
        owner: owner.into(),
        doc: doc.into(),
        name: "n".into(),
        kind: "docs".into(),
        rights,
        created: now(),
        expires,
    }
}

#[quench_test::test]
fn a_share_link_opens_one_document_and_only_an_edit_link_saves_it() {
    run(async {
        let (s, dir) = store(1 << 20);
        let d = s
            .put("alice", None, "Plan.docx", "docs", b"v1", None)
            .await
            .unwrap();
        let view = share_of("alice", &d.id, Rights::View, None);
        let edit = share_of("alice", &d.id, Rights::Edit, None);
        s.put_share(&view).await.unwrap();
        s.put_share(&edit).await.unwrap();
        let (got, meta, bytes) = open_share(&s, &view.token).await.unwrap();
        assert_eq!(
            (got.rights, meta.name.as_str(), bytes.as_slice()),
            (Rights::View, "Plan.docx", &b"v1"[..])
        );
        assert_eq!(
            save_share(&s, &view.token, b"hack", None).await,
            Err(Error::Denied)
        );
        let saved = save_share(&s, &edit.token, b"v2 by a guest", Some(&d.etag))
            .await
            .unwrap();
        assert_eq!(s.get("alice", &d.id).await.unwrap().1, b"v2 by a guest");
        // The guest saving from an old copy is a conflict, like anyone.
        assert_eq!(
            save_share(&s, &edit.token, b"late", Some(&d.etag)).await,
            Err(Error::Conflict { etag: saved.etag })
        );
        // Links nobody made, malformed ones and expired ones open nothing.
        for token in ["", "abc", &"0".repeat(64), "../../etc/passwd"] {
            assert!(open_share(&s, token).await.is_err(), "{token}");
        }
        let old = share_of("alice", &d.id, Rights::View, Some(now() - 1));
        s.put_share(&old).await.unwrap();
        assert_eq!(
            open_share(&s, &old.token).await.unwrap_err(),
            Error::NotFound
        );
        // The owner lists and withdraws links; nobody else can.
        assert_eq!(s.shares_of("alice").await.unwrap().len(), 3);
        assert!(s.shares_of("bob").await.unwrap().is_empty());
        assert_eq!(
            s.delete_share("bob", &view.token).await,
            Err(Error::NotFound)
        );
        s.delete_share("alice", &view.token).await.unwrap();
        assert!(open_share(&s, &view.token).await.is_err());
        // Deleting the document takes its links with it; a link to a document that is not there is refused.
        s.delete("alice", &d.id).await.unwrap();
        assert!(s.shares_of("alice").await.unwrap().is_empty());
        assert_eq!(
            s.put_share(&share_of("alice", &d.id, Rights::View, None))
                .await,
            Err(Error::NotFound)
        );
        let _ = std::fs::remove_dir_all(dir);
    });
}

/// What both stores must do with folders and with documents shared with a named person.
async fn folders_and_grants(store: &dyn Storage) {
    let doc = store
        .put("ada", None, "Plan.docx", "docs", b"one", None)
        .await
        .unwrap();
    assert_eq!(doc.folder, "");
    // Folders: made, found with the ones above them, filled by moving, kept when a document is saved over.
    store.make_folder("ada", "Work/2026").await.unwrap();
    assert_eq!(store.folders("ada").await.unwrap(), ["Work", "Work/2026"]);
    assert!(store.make_folder("ada", "../x").await.is_err());
    let moved = store.move_doc("ada", &doc.id, "Work/2026").await.unwrap();
    assert_eq!(
        (moved.folder.as_str(), moved.etag.as_str()),
        ("Work/2026", doc.etag.as_str())
    );
    let saved = store
        .put(
            "ada",
            Some(&doc.id),
            "Plan.docx",
            "docs",
            b"two",
            Some(&doc.etag),
        )
        .await
        .unwrap();
    assert_eq!(saved.folder, "Work/2026", "a save keeps the folder");
    assert!(
        matches!(
            store.delete_folder("ada", "Work").await,
            Err(Error::Invalid(_))
        ),
        "not empty"
    );
    store.move_doc("ada", &doc.id, "").await.unwrap();
    store.delete_folder("ada", "Work").await.unwrap();
    assert!(store.folders("ada").await.unwrap().is_empty());
    // Shared with a person: seen by them, opened and (with edit rights) saved in the owner's cloud.
    assert!(matches!(
        grant_doc(store, "ada", &doc.id, "ada", Rights::Edit).await,
        Err(Error::Invalid(_))
    ));
    grant_doc(store, "ada", &doc.id, "bob", Rights::View)
        .await
        .unwrap();
    let to_bob = store.grants_to("bob").await.unwrap();
    assert_eq!(
        (to_bob.len(), to_bob[0].owner.as_str(), to_bob[0].rights),
        (1, "ada", Rights::View)
    );
    assert!(store.grants_to("carol").await.unwrap().is_empty());
    let (_, meta, bytes) = open_granted(store, "bob", "ada", &doc.id).await.unwrap();
    assert_eq!(
        (meta.name.as_str(), bytes.as_slice()),
        ("Plan.docx", &b"two"[..])
    );
    assert!(matches!(
        save_granted(store, "bob", "ada", &doc.id, b"x", None).await,
        Err(Error::Denied)
    ));
    assert!(matches!(
        open_granted(store, "carol", "ada", &doc.id).await,
        Err(Error::NotFound)
    ));
    grant_doc(store, "ada", &doc.id, "bob", Rights::Edit)
        .await
        .unwrap();
    assert_eq!(
        store.grants_to("bob").await.unwrap().len(),
        1,
        "one grant for a person and a document"
    );
    let (_, current) = granted_meta(store, "bob", "ada", &doc.id).await.unwrap();
    let by_bob = save_granted(store, "bob", "ada", &doc.id, b"three", Some(&current.etag))
        .await
        .unwrap();
    assert_eq!(store.get("ada", &doc.id).await.unwrap().1, b"three");
    assert_ne!(by_bob.etag, current.etag);
    assert!(matches!(
        save_granted(store, "bob", "ada", &doc.id, b"late", Some(&current.etag)).await,
        Err(Error::Conflict { .. })
    ));
    assert_eq!(store.grants_of("ada", &doc.id).await.unwrap().len(), 1);
    // Taken back, and gone with the document.
    store.delete_grant("ada", &doc.id, "bob").await.unwrap();
    assert!(store.grants_to("bob").await.unwrap().is_empty());
    grant_doc(store, "ada", &doc.id, "bob", Rights::View)
        .await
        .unwrap();
    store.delete("ada", &doc.id).await.unwrap();
    assert!(store.grants_to("bob").await.unwrap().is_empty());
    assert!(store.get("ada", &doc.id).await.is_err());
}

#[quench_test::test]
fn the_local_store_has_folders_and_documents_shared_with_a_named_person() {
    run(async {
        let (s, dir) = store(1 << 20);
        folders_and_grants(&s).await;
        let _ = std::fs::remove_dir_all(dir);
    });
}

#[quench_test::test]
fn folder_paths_are_cleaned_or_refused() {
    assert_eq!(clean_folder(" /Work//2026/ ").unwrap(), "Work/2026");
    assert_eq!(clean_folder("").unwrap(), "");
    for bad in ["a/../b", "a/./b", "a\\b", "x:y", "a?b"] {
        assert!(clean_folder(bad).is_err(), "{bad}");
    }
    assert!(clean_folder(&"a/".repeat(9)).is_err());
    assert!(clean_folder(&"x".repeat(65)).is_err());
}

/// A small warehouse: the token route of gatehouse, and the files API with dynamic storages (`POST /api/v1/files` makes one and
/// answers a second request for the same name with a 500, as the live one may; names are checked as warehouse checks them).
mod fake {
    use std::collections::HashMap;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    type Files = HashMap<String, HashMap<String, Vec<u8>>>;

    pub struct Warehouse {
        pub port: u16,
    }

    fn decode(text: &str) -> String {
        let bytes = text.as_bytes();
        let mut out = Vec::new();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'%' && i + 2 < bytes.len() {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("00");
                out.push(u8::from_str_radix(hex, 16).unwrap_or(b'?'));
                i += 3;
            } else {
                out.push(bytes[i]);
                i += 1;
            }
        }
        String::from_utf8_lossy(&out).into_owned()
    }

    fn reply(status: u16, body: &[u8]) -> Vec<u8> {
        let mut out = format!(
            "HTTP/1.1 {status} X\r\ncontent-length: {}\r\nconnection: close\r\ncontent-type: application/octet-stream\r\n\r\n",
            body.len()
        )
        .into_bytes();
        out.extend_from_slice(body);
        out
    }

    #[allow(
        clippy::significant_drop_tightening,
        clippy::too_many_lines,
        clippy::option_if_let_else
    )]
    fn handle(files: &Mutex<Files>, method: &str, target: &str, body: &[u8]) -> Vec<u8> {
        let (path, query) = target.split_once('?').unwrap_or((target, ""));
        let param = |key: &str| {
            query
                .split('&')
                .find_map(|p| p.strip_prefix(&format!("{key}=")))
                .map(decode)
                .unwrap_or_default()
        };
        let mut files = files.lock().unwrap();
        if method == "POST" && path == "/gate/api/v1/token" {
            return reply(200, br#"{"access_token":"t","refresh_token":"","token_type":"Bearer","expires_in":900}"#);
        }
        let Some(rest) = path.strip_prefix("/wh/api/v1/files") else {
            return reply(404, b"no route");
        };
        if rest.is_empty() && method == "POST" {
            let text = String::from_utf8_lossy(body).into_owned();
            let name = text
                .split("\"name\":\"")
                .nth(1)
                .and_then(|r| r.split('"').next())
                .unwrap_or("")
                .to_string();
            if name.is_empty()
                || !name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
            {
                return reply(
                    400,
                    b"storage names may use letters, digits, `-` and `_` only",
                );
            }
            if files.contains_key(&name) {
                return reply(500, b"could not create the storage");
            }
            files.insert(name, HashMap::new());
            return reply(201, b"{}");
        }
        let parts: Vec<&str> = rest.split('/').filter(|p| !p.is_empty()).collect();
        match (method, parts.as_slice()) {
            ("GET", [storage]) => match files.get(*storage) {
                None => reply(404, b"no such storage"),
                Some(map) => {
                    let prefix = param("prefix");
                    let last = param("last");
                    let mut names: Vec<&String> = map
                        .keys()
                        .filter(|k| k.starts_with(&prefix) && (last.is_empty() || **k > last))
                        .collect();
                    names.sort();
                    names.truncate(param("n").parse().unwrap_or(1000));
                    let entries: Vec<String> = names
                        .iter()
                        .map(|k| {
                            format!(
                                "{{\"name\":\"{k}\",\"path\":\"{k}\",\"kind\":\"file\",\"size\":{}}}",
                                map[*k].len()
                            )
                        })
                        .collect();
                    reply(
                        200,
                        format!(
                            "{{\"storage\":\"{storage}\",\"entries\":[{}]}}",
                            entries.join(",")
                        )
                        .as_bytes(),
                    )
                }
            },
            (m, [storage, "file"]) => {
                let Some(map) = files.get_mut(*storage) else {
                    return reply(404, b"no such storage");
                };
                let file = param("path");
                match m {
                    "PUT" => {
                        let new = !map.contains_key(&file);
                        map.insert(file, body.to_vec());
                        reply(if new { 201 } else { 200 }, b"{}")
                    }
                    "GET" => map
                        .get(&file)
                        .map_or_else(|| reply(404, b"no such file"), |b| reply(200, b)),
                    "DELETE" => {
                        map.remove(&file);
                        reply(204, b"")
                    }
                    _ => reply(405, b""),
                }
            }
            _ => reply(404, b"no route"),
        }
    }

    pub fn start() -> Warehouse {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let files: Arc<Mutex<Files>> = Arc::default();
        let shared = files;
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let files = shared.clone();
                std::thread::spawn(move || {
                    let mut data = Vec::new();
                    let mut chunk = [0_u8; 8192];
                    let (head_end, length) = loop {
                        let n = stream.read(&mut chunk).unwrap_or(0);
                        if n == 0 {
                            return;
                        }
                        data.extend_from_slice(&chunk[..n]);
                        if let Some(at) = data.windows(4).position(|w| w == b"\r\n\r\n") {
                            let head = String::from_utf8_lossy(&data[..at]).to_lowercase();
                            let length = head
                                .lines()
                                .find_map(|l| l.strip_prefix("content-length:"))
                                .and_then(|v| v.trim().parse::<usize>().ok())
                                .unwrap_or(0);
                            break (at + 4, length);
                        }
                    };
                    while data.len() < head_end + length {
                        let n = stream.read(&mut chunk).unwrap_or(0);
                        if n == 0 {
                            break;
                        }
                        data.extend_from_slice(&chunk[..n]);
                    }
                    let head = String::from_utf8_lossy(&data[..head_end]).into_owned();
                    let mut first = head.lines().next().unwrap_or("").split(' ');
                    let (method, target) = (first.next().unwrap_or(""), first.next().unwrap_or(""));
                    let response =
                        handle(&files, method, target, &data[head_end..head_end + length]);
                    let _ = stream.write_all(&response);
                });
            }
        });
        Warehouse { port }
    }
}

#[quench_test::test]
fn a_warehouse_store_keeps_documents_and_links_for_a_user_whose_name_has_a_dot_and_saves_again() {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let w = fake::start();
            quench_core::env::set("WAREHOUSE_URL", format!("http://127.0.0.1:{}/wh", w.port));
            quench_core::env::set("GATEHOUSE_URL", format!("http://127.0.0.1:{}/gate", w.port));
            quench_core::env::set("WAREHOUSE_CLIENT_ID", "lore-office-warehouse");
            quench_core::env::set("WAREHOUSE_CLIENT_SECRET", "x");
            let store = WarehouseStore::from_env().unwrap();
            let user = "ada.lovelace";
            // The first save makes the storage; later saves, also through a link, go into it.
            let first = store
                .put(user, None, "Plan.docx", "docs", b"one", None)
                .await
                .unwrap();
            let second = store
                .put(
                    user,
                    Some(&first.id),
                    "Plan.docx",
                    "docs",
                    b"two!",
                    Some(&first.etag),
                )
                .await
                .unwrap();
            assert_ne!(first.etag, second.etag);
            assert_eq!(store.get(user, &first.id).await.unwrap().1, b"two!");
            assert_eq!(store.list(user).await.unwrap().len(), 1);
            assert!(store.usage(user).await.unwrap().used >= 4);
            // A link: made by the owner, opened and saved through by someone who is nobody in particular.
            let share = Share {
                token: new_id(32),
                owner: user.to_string(),
                doc: first.id.clone(),
                name: "Plan.docx".into(),
                kind: "docs".into(),
                rights: Rights::Edit,
                created: now(),
                expires: None,
            };
            store.put_share(&share).await.unwrap();
            let (_, meta, bytes) = open_share(&store, &share.token).await.unwrap();
            assert_eq!(
                (meta.id.as_str(), bytes.as_slice()),
                (first.id.as_str(), &b"two!"[..])
            );
            let saved = save_share(&store, &share.token, b"three", Some(&meta.etag))
                .await
                .unwrap();
            assert_eq!(saved.size, 5);
            assert_eq!(store.get(user, &first.id).await.unwrap().1, b"three");
            assert!(matches!(
                save_share(&store, &share.token, b"four", Some(&meta.etag)).await,
                Err(Error::Conflict { .. })
            ));
            store.delete(user, &first.id).await.unwrap();
            assert!(matches!(
                open_share(&store, &share.token).await,
                Err(Error::NotFound)
            ));
            for key in [
                "WAREHOUSE_URL",
                "GATEHOUSE_URL",
                "WAREHOUSE_CLIENT_ID",
                "WAREHOUSE_CLIENT_SECRET",
            ] {
                quench_core::env::set(key, "");
            }
        });
}

#[quench_test::test]
fn a_warehouse_storage_has_a_name_of_its_own_for_each_user_and_needs_its_addresses() {
    let a = WarehouseStore::storage_of("alice");
    assert!(a.starts_with("lore-alice-") && a.len() < 40);
    assert_ne!(
        WarehouseStore::storage_of("a/b"),
        WarehouseStore::storage_of("a_b")
    );
    assert_eq!(
        WarehouseStore::storage_of("Alice"),
        WarehouseStore::storage_of("Alice")
    );
    // Warehouse takes letters, digits, `-` and `_` only: dots, @ and the like are turned into `_`.
    for name in ["ada.lovelace", "a@b.c", "\u{c5}sa", "x y/z", "..."] {
        let storage = WarehouseStore::storage_of(name);
        assert!(
            storage
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'),
            "{storage}"
        );
    }
    assert!(WarehouseStore::storage_of("ada.lovelace").starts_with("lore-ada_lovelace-"));
    assert_ne!(
        WarehouseStore::storage_of("ada.lovelace"),
        WarehouseStore::storage_of("ada_lovelace")
    );
    // Without WAREHOUSE_URL and GATEHOUSE_URL there is nothing to talk to.
    if std::env::var("WAREHOUSE_URL").is_err() {
        assert!(matches!(WarehouseStore::from_env(), Err(Error::Invalid(_))));
    }
}

quench_test::main!();
