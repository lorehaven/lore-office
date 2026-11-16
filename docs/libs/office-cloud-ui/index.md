---
title: office-cloud-ui
parent: Libraries
nav_order: 2
---

The Cloud menu, the panels and the banner of the apps, over a `Transport` that can be replaced (the browser's `fetch` in the page, a fake in tests). Version 0.1.0.

## Use

An app calls `setup()` before building its view, gives the returned `Cloud` a `Binding`, and puts `with_menu(items)` (File menu items with the cloud's after them), `cloud_button()` and `overlay()` in its screen:

```rust
let cloud = office_cloud_ui::setup();
cloud.bind(office_cloud_ui::Binding {
    kind: "docs",
    export: std::rc::Rc::new(move || /* Some((name, bytes)) */ None),
    import: std::rc::Rc::new(move |name, bytes| /* open the file */ Ok(())),
});
```

`setup()` reads the page state in the browser (`quench_ui_web::initial_state()`) and, on the server, the context given by `set_server_context`. It also tells `office-ui` where the suite's home is, so every ribbon shows the home link.

## Public items

| Item | Meaning |
|---|---|
| `Context` | what the server told the page: `api` (empty: no cloud), `user`, `denied` (signed in but without access), `login`, `share` (`SharedWith { token, edit }`); `Context::from_json`, `to_json` |
| `Transport` | how a request reaches the server: `send(method, url, headers, body, done)` |
| `Reply` | `status`, `headers`, `body`; `Reply::header(name)` |
| `Binding` | what an app gives the cloud: its `kind`, `export`, `import`, a `revision` (a number that changes whenever the document does) and whether `autosave` is on |
| `Cloud` | the state and actions: `enabled`, `open_panel`, `save`, `save_copy`, `open_doc`, `open_share`, `delete_doc`, `share`, `list_shares`, `withdraw`, `refresh`, `close`, and the signals the panels read |
| `Panel` | `Closed`, `Documents`, `Sharing(id)` |
| `Section` | `Mine` (the person's folders) or `Shared` (what others shared with them) |
| `Opened` | the document open now and where it is in the cloud: its `id`, `etag`, the link `token` or the `owner` it came through, and whether this page may save it (`edit`) |
| `current()`, `setup()`, `set_server_context(context)` | the page's cloud |
| `with_menu(items)`, `menu_items()` | the File menu's cloud items |
| `saves_to_cloud()`, `save_shortcut()` | whether Ctrl+S saves to the cloud here (signed in, or an edit link); the app's Ctrl+S calls `save_shortcut()` and saves a file only when it answers `false` |
| `cloud_button()`, `overlay()` | the top-bar button; the panel and the banner |
| `server_page(address, kind)` | the page of an app on a server the person typed: adds `https://` and `/lore-office` where missing |
| `web` (wasm32 only) | the browser's `Transport` |

The panel's account line shows who is signed in (Sign out), or Sign in, or that the account has no access; the document being in the cloud is looked after on a timer (changes noticed every 400 ms, sent after 2 s of quiet when `autosave` is on, the cloud asked every 5 s whether someone else saved; `check_remote`, `reload`, `keep_mine`); a Server field with *Connect* opens the same app on another lore-office server (the page changes; the API is not called across origins).
