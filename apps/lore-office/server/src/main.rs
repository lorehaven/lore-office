//! lore-office: the launcher, the four apps and the cloud, as one service.
//!
//! Settings come from the environment:
//!
//! | | |
//! |---|---|
//! | `PORT`, `HOST` | where to listen (3000, `0.0.0.0`) |
//! | `BASE_PATH` | the path the service is served under (`/lore-office`; none by default) |
//! | `WASM_DIR` | where `docs.wasm`, `sheets.wasm`, `notes.wasm`, `pdf.wasm` and `launcher.wasm` are, unless built in |
//! | `CLOUD_STORAGE` | `local` (a directory per person under `DATA_DIR`) or `warehouse` (a storage per person) |
//! | `USER_QUOTA_BYTES` | what each person may keep (10 GiB) |
//! | `SERVICE_AUTH_ENABLED`, `GATEHOUSE_URL`, `GATEHOUSE_CLIENT_ID`, `GATEHOUSE_CLIENT_SECRET` | sign-in through gatehouse |

use lore_office_server::{Modules, Site, SiteConfig};
use office_cloud::{LocalStore, Storage, WarehouseStore};
use quench_auth::domain::jwt::JwtConfig;
use quench_auth::domain::sso_client::SsoConfig;
use quench_http::prelude::*;
use std::sync::Arc;

#[cfg(feature = "embed")]
fn modules() -> Modules {
    Modules {
        launcher: include_bytes!("../../../lore-launcher/dist/app.wasm").to_vec(),
        docs: include_bytes!("../../../lore-docs/dist/app.wasm").to_vec(),
        sheets: include_bytes!("../../../lore-sheets/dist/app.wasm").to_vec(),
        notes: include_bytes!("../../../lore-notes/dist/app.wasm").to_vec(),
        pdf: include_bytes!("../../../lore-pdf/dist/app.wasm").to_vec(),
    }
}

#[cfg(not(feature = "embed"))]
fn modules() -> Modules {
    let dir = std::env::var("WASM_DIR").unwrap_or_else(|_| "web".to_string());
    let read = |name: &str| {
        let path = format!("{dir}/{name}");
        std::fs::read(&path)
            .unwrap_or_else(|e| panic!("cannot read {path}: {e} - run scripts/web.sh"))
    };
    Modules {
        launcher: read("launcher.wasm"),
        docs: read("docs.wasm"),
        sheets: read("sheets.wasm"),
        notes: read("notes.wasm"),
        pdf: read("pdf.wasm"),
    }
}

fn storage() -> Arc<dyn Storage> {
    match std::env::var("CLOUD_STORAGE").as_deref() {
        Ok("warehouse") => Arc::new(WarehouseStore::from_env().expect(
            "CLOUD_STORAGE=warehouse needs WAREHOUSE_URL and the machine client's credentials",
        )),
        _ => Arc::new(LocalStore::from_env(
            std::env::var("DATA_DIR").unwrap_or_else(|_| "data".to_string()),
        )),
    }
}

#[tokio::main]
async fn main() {
    let auth = std::env::var("SERVICE_AUTH_ENABLED").is_ok_and(|v| v == "true");
    let jwt = if auth {
        JwtConfig::init().await
    } else {
        JwtConfig::for_tests()
    };
    let sso = auth.then(SsoConfig::init);
    let base = std::env::var("BASE_PATH").unwrap_or_default();
    let site = Site::new(SiteConfig {
        modules: modules(),
        store: storage(),
        jwt,
        sso,
        base: if base == "/" { String::new() } else { base },
        max_upload: std::env::var("MAX_UPLOAD_BYTES")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(256 << 20),
        dev: std::env::var_os("QUENCH_DEV").is_some(),
    });
    let container = Arc::new(ContainerBuilder::new().build().await.expect("container"));
    let host = std::env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let port = std::env::var("PORT").unwrap_or_else(|_| "3000".to_string());
    let addr = format!("{host}:{port}").parse().expect("address");
    println!("http://{addr}");
    serve_http(Arc::new(site), container, addr)
        .await
        .expect("server");
}
