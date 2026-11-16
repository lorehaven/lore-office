//! Reads an .xlsx, recalculates, writes it again, and says how long each took: `cargo run --release -p sheets-xlsx --example bench file.xlsx`.
use std::time::Instant;

fn main() {
    let path = std::env::args().nth(1).expect("a path to an .xlsx");
    let bytes = std::fs::read(&path).expect("readable file");
    let t = Instant::now();
    let x = sheets_xlsx::read(&bytes).expect("a workbook");
    let cells: usize = x
        .engine
        .workbook()
        .sheets
        .iter()
        .map(sheets_model::Sheet::len)
        .sum();
    println!(
        "read    {:>8.0} ms  ({} cells, {} KiB file)",
        t.elapsed().as_secs_f64() * 1000.0,
        cells,
        bytes.len() / 1024
    );
    let t = Instant::now();
    let out = x.write().expect("written");
    println!(
        "write   {:>8.0} ms  ({} KiB)",
        t.elapsed().as_secs_f64() * 1000.0,
        out.len() / 1024
    );
    let t = Instant::now();
    let mut x = sheets_xlsx::read(&out).expect("read back");
    println!("reread  {:>8.0} ms", t.elapsed().as_secs_f64() * 1000.0);
    let t = Instant::now();
    x.engine.set(
        0,
        sheets_model::CellRef::new(1, 0),
        sheets_model::Content::Value(sheets_model::Value::Number(123.0)),
    );
    println!("one edit{:>8.0} ms", t.elapsed().as_secs_f64() * 1000.0);
}
