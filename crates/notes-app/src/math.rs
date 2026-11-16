//! TeX math typeset as pictures: an SVG (paths, no fonts needed) as the address of an image.

use std::cell::RefCell;
use std::collections::HashMap;

/// A formula as it is shown: the address of its SVG picture and its size in ems.
#[derive(Clone, Debug, PartialEq)]
pub struct Formula {
    pub address: String,
    pub width: f64,
    pub height: f64,
    /// How far below the line of text the bottom of the picture goes (negative: above), in ems.
    pub align: f64,
}

thread_local! {
    static CACHE: RefCell<HashMap<(String, bool), Option<Formula>>> = RefCell::default();
}

fn attribute(svg: &str, name: &str) -> Option<f64> {
    let at = svg.find(&format!(" {name}=\""))? + name.len() + 3;
    let rest = &svg[at..];
    rest[..rest.find('"')?].trim_end_matches("em").parse().ok()
}

/// Typeset `tex` (`display` for a formula on a line of its own), or `None` when it cannot be read or set.
#[must_use]
pub fn typeset(tex: &str, display: bool) -> Option<Formula> {
    let key = (tex.to_string(), display);
    if let Some(hit) = CACHE.with(|c| c.borrow().get(&key).cloned()) {
        return hit;
    }
    let made =
        quench_tex::typeset::tex_to_svg(tex, display, &quench_tex::typeset::SvgOptions::default())
            .ok()
            .map(|svg| {
                let (width, height) = (
                    attribute(&svg, "width").unwrap_or(1.0),
                    attribute(&svg, "height").unwrap_or(1.0),
                );
                let mut address = String::from("data:image/svg+xml;utf8,");
                for c in svg.chars() {
                    match c {
                        '#' => address.push_str("%23"),
                        '<' => address.push_str("%3C"),
                        '>' => address.push_str("%3E"),
                        '"' => address.push_str("%22"),
                        '%' => address.push_str("%25"),
                        '\n' | '\r' => address.push(' '),
                        c => address.push(c),
                    }
                }
                let align = svg
                    .split_once("vertical-align: ")
                    .and_then(|(_, r)| r.split_once("em"))
                    .and_then(|(v, _)| v.trim().parse().ok())
                    .unwrap_or(0.0);
                Formula {
                    address,
                    width,
                    height,
                    align,
                }
            });
    CACHE.with(|c| c.borrow_mut().insert(key, made.clone()));
    made
}

/// Typeset `tex` as an `<svg>` element (in ems, filled with the text colour), to put in a web page.
#[must_use]
pub fn svg(tex: &str, display: bool) -> Option<String> {
    quench_tex::typeset::tex_to_svg(tex, display, &quench_tex::typeset::SvgOptions::default()).ok()
}
