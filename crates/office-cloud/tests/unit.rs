#![allow(
    clippy::unwrap_used,
    clippy::assert_is_empty,
    clippy::cloned_ref_to_slice_refs
)]
use office_cloud::{
    Error, LocalStore, Rights, Share, Storage, WarehouseStore, new_id, now, open_share, safe_name,
    save_share,
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
