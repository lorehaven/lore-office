//! Serves the start page: the page rendered on the server, the module, and nothing else.
//!
//! `PORT` sets the port (3000); `LORE_DOCS_URL`, `LORE_SHEETS_URL`, `LORE_NOTES_URL` and `LORE_PDF_URL` say where the apps
//! are (`/lore-docs/` and so on by default, as when they are served under one address). `WASM` is the module to serve unless
//! it is built in.

use std::sync::Arc;

use lore_launcher_app::Links;
use quench_fullstack::{Page, Ui, UiApp};
use quench_http::prelude::*;

#[cfg(feature = "embed")]
fn module() -> Vec<u8> {
    include_bytes!("../../dist/app.wasm").to_vec()
}

#[cfg(not(feature = "embed"))]
fn module() -> Vec<u8> {
    let path = std::env::var("WASM").unwrap_or_else(|_| {
        "target/wasm32-unknown-unknown/wasm-dev/lore_launcher_app.wasm".to_string()
    });
    std::fs::read(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e} - run `quench build`"))
}

fn links() -> Links {
    let mut links = Links::default();
    for (var, field) in [
        ("LORE_DOCS_URL", &mut links.docs),
        ("LORE_SHEETS_URL", &mut links.sheets),
        ("LORE_NOTES_URL", &mut links.notes),
        ("LORE_PDF_URL", &mut links.pdf),
    ] {
        if let Ok(v) = std::env::var(var) {
            *field = v;
        }
    }
    links
}

#[tokio::main]
async fn main() {
    let port = std::env::var("PORT").unwrap_or_else(|_| "3000".to_string());
    let loader = format!(
        "{}\n{}\n{}",
        office_theme::icon_script(include_str!("../../icon.svg")),
        office_theme::SCRIPT,
        quench_ui_web::QUENCH_JS
    );
    let mut ui = Ui::with_script(loader, module());
    if std::env::var_os("QUENCH_DEV").is_some() {
        ui = ui.dev_reload();
    }
    let style = ui.add_asset(
        "site.css",
        format!(
            "{}\n{}\n{}",
            office_theme::STYLE,
            office_ui::STYLE,
            lore_launcher_app::STYLE
        ),
        "text/css",
    );
    let home = move || {
        let links = links();
        Page::new("lore-office", lore_launcher_app::launcher(&links))
            .stylesheet(&style)
            .state(links.to_json())
    };
    let export = home.clone();
    let app = UiApp::new(ui).page("/", move |_req| home());
    if let Some(dir) = std::env::var_os("QUENCH_EXPORT") {
        app.ui()
            .export(std::path::Path::new(&dir), export())
            .expect("cannot write the bundle");
        println!("bundle written to {}", std::path::Path::new(&dir).display());
        return;
    }
    let container = Arc::new(ContainerBuilder::new().build().await.expect("container"));
    let addr = format!("127.0.0.1:{port}").parse().expect("address");
    println!("http://{addr}");
    serve_http(Arc::new(app), container, addr)
        .await
        .expect("server");
}
