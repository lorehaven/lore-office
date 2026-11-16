use quench_html::markdown::{Options, parse};
fn main() {
    let d = parse(
        "Back to [[Welcome]] and [[Plans#H|alias]]. #brainstorm\n\n![[pic.png]]",
        Options::ALL,
    );
    println!("{:#?}", d.blocks);
}
