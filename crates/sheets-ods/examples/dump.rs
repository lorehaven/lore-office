#![allow(clippy::unwrap_used, clippy::print_stdout)]
fn main() {
    let bytes = std::fs::read(std::env::args().nth(1).unwrap()).unwrap();
    let wb = sheets_ods::read(&bytes).unwrap();
    for s in &wb.sheets {
        println!(
            "== {} merges={:?} widths={:?}",
            s.name,
            s.merges(),
            s.col_widths().collect::<Vec<_>>()
        );
        for (at, c) in s.iter() {
            let st = &wb.styles[s.style(at) as usize];
            println!(
                "{at}: {c:?} fmt={} b={} fill={:?}",
                st.number_format, st.bold, st.fill
            );
        }
    }
}
