//! Serves the app: the page rendered on the server, the module, and nothing else.
//!
//! `PORT` sets the port (3000), `WASM` the module to serve unless it is built in. `quench dev` sets
//! `QUENCH_DEV`, which makes every page reload when the server restarts.

use std::sync::Arc;

use quench_fullstack::{Page, Ui, UiApp};
use quench_http::prelude::*;

#[cfg(feature = "embed")]
fn module() -> Vec<u8> {
    include_bytes!("../../dist/app.wasm").to_vec()
}

#[cfg(not(feature = "embed"))]
fn module() -> Vec<u8> {
    let path = std::env::var("WASM").unwrap_or_else(|_| {
        "target/wasm32-unknown-unknown/wasm-dev/lore_docs_app.wasm".to_string()
    });
    std::fs::read(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e} - run `quench build`"))
}

#[tokio::main]
async fn main() {
    let port = std::env::var("PORT").unwrap_or_else(|_| "3000".to_string());
    // The loader is quench's with the theme script before it, which applies the stored theme and palette.
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
            docs_app::STYLE
        ),
        "text/css",
    );
    let home = move || Page::new("lore-docs", lore_docs_app::app()).stylesheet(&style);
    let export = home.clone();
    let app = UiApp::new(ui).page("/", move |_req| home());
    // `quench build --bundle` runs the server with QUENCH_EXPORT=<dir>: the page and the assets are
    // written out as a static bundle (what the Android host serves) and the server does not start.
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
