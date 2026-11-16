//! Markdown on the screen: the syntax tree of `quench-html` drawn as quench-ui elements. Nothing here writes HTML or
//! passes raw HTML on, so a note cannot put markup of its own on the page.

use notes_md::{Link, Piece, callout, label, pieces, slug};
use quench_html::markdown::{Alignment, Block, Document, Inline, List, Options, Table, parse};
use quench_ui::prelude::*;
use std::rc::Rc;

/// What a click on a wikilink does: the note's name and the heading asked for.
pub type OnLink = Rc<dyn Fn(&str, Option<&str>)>;

/// The reading options: everything on.
#[must_use]
pub fn document(markdown: &str) -> Document {
    parse(markdown, Options::ALL)
}

/// The markdown as one element holding its blocks.
#[must_use]
pub fn render(markdown: &str, on_link: &OnLink) -> Element {
    let doc = document(markdown);
    let mut root = div().class("md");
    for b in &doc.blocks {
        if let Some(v) = block(b, on_link, false) {
            root = root.child(v);
        }
    }
    if !doc.footnotes.is_empty() {
        let mut list = element("ol");
        for f in &doc.footnotes {
            let mut item = li().attr("id", format!("fn-{}", f.label));
            for b in &f.blocks {
                if let Some(v) = block(b, on_link, true) {
                    item = item.child(v);
                }
            }
            list = list.child(item);
        }
        root = root.child(element("section").class("footnotes").child(list));
    }
    root
}

/// The plain text of some inlines (for a heading's address or an alt text).
#[must_use]
pub fn plain(inlines: &[Inline]) -> String {
    let mut out = String::new();
    for i in inlines {
        match i {
            Inline::Text(t) | Inline::Code(t) | Inline::Math(t) => out.push_str(t),
            Inline::Emph(c) | Inline::Strong(c) | Inline::Strikethrough(c) => {
                out.push_str(&plain(c))
            }
            Inline::Link { children, .. } | Inline::Image { children, .. } => {
                out.push_str(&plain(children))
            }
            Inline::SoftBreak | Inline::HardBreak => out.push(' '),
            _ => {}
        }
    }
    out
}

fn safe_link(destination: &str) -> bool {
    let d = destination.trim().to_lowercase();
    d.starts_with("http://")
        || d.starts_with("https://")
        || d.starts_with("mailto:")
        || d.starts_with('#')
}

fn safe_image(destination: &str) -> bool {
    let d = destination.trim().to_lowercase();
    d.starts_with("data:image/") || d.starts_with("http://") || d.starts_with("https://")
}

fn wikilink(l: &Link, on_link: &OnLink) -> View {
    let text = label(l);
    if l.embed {
        return span()
            .class("embed")
            .text(format!("\u{1F5BC} {text}"))
            .into_view();
    }
    let (target, heading, go) = (l.target.clone(), l.heading.clone(), Rc::clone(on_link));
    span()
        .class("wikilink")
        .attr("role", "link")
        .attr("tabindex", "0")
        .text(text)
        .on_click(move || go(&target, heading.as_deref()))
        .into_view()
}

/// The parser cuts text at every bracket; a wikilink needs it whole.
fn joined(list: &[Inline]) -> Vec<Inline> {
    let mut out: Vec<Inline> = Vec::new();
    for i in list {
        match (out.last_mut(), i) {
            (Some(Inline::Text(last)), Inline::Text(next)) => last.push_str(next),
            _ => out.push(i.clone()),
        }
    }
    out
}

fn inlines(list: &[Inline], on_link: &OnLink) -> Vec<View> {
    joined(list).iter().map(|i| inline(i, on_link)).collect()
}

fn inline(i: &Inline, on_link: &OnLink) -> View {
    match i {
        Inline::Text(t) => {
            let parts = pieces(t);
            if parts.len() == 1 && matches!(parts[0], Piece::Text(_)) {
                return t.clone().into_view();
            }
            let mut s = span();
            for p in parts {
                s = s.child(match p {
                    Piece::Text(t) => t.into_view(),
                    Piece::Link(l) => wikilink(&l, on_link),
                });
            }
            s.into_view()
        }
        Inline::Code(c) => element("code").text(c.clone()).into_view(),
        Inline::Emph(c) => element("em").children(inlines(c, on_link)).into_view(),
        Inline::Strong(c) => element("strong").children(inlines(c, on_link)).into_view(),
        Inline::Strikethrough(c) => element("del").children(inlines(c, on_link)).into_view(),
        Inline::Link {
            destination,
            title,
            children,
        } => {
            if safe_link(destination) {
                a().attr("href", destination.clone())
                    .attr("title", title.clone())
                    .attr("target", "_blank")
                    .attr("rel", "noopener noreferrer")
                    .children(inlines(children, on_link))
                    .into_view()
            } else {
                span().children(inlines(children, on_link)).into_view()
            }
        }
        Inline::Image {
            destination,
            title,
            children,
        } => {
            if safe_image(destination) {
                img()
                    .attr("src", destination.clone())
                    .attr("alt", plain(children))
                    .attr("title", title.clone())
                    .into_view()
            } else {
                span()
                    .class("embed")
                    .text(format!("\u{1F5BC} {}", plain(children)))
                    .into_view()
            }
        }
        Inline::Html(h) => span().class("raw").text(h.clone()).into_view(),
        Inline::SoftBreak => " ".to_string().into_view(),
        Inline::HardBreak => element("br").into_view(),
        Inline::FootnoteReference { label, number, .. } => element("sup")
            .class("footnote-ref")
            .child(
                a().attr("href", format!("#fn-{label}"))
                    .text(number.to_string()),
            )
            .into_view(),
        Inline::Math(tex) => match crate::math::typeset(tex, false) {
            Some(f) => formula_view(&f, tex, "math math-image"),
            None => span().class("math").text(tex.clone()).into_view(),
        },
    }
}

fn align_class(a: Alignment) -> &'static str {
    match a {
        Alignment::Left => "al",
        Alignment::Center => "ac",
        Alignment::Right => "ar",
        Alignment::None => "",
    }
}

fn list_view(l: &List, on_link: &OnLink) -> View {
    let mut el = element(if l.ordered { "ol" } else { "ul" });
    if l.ordered && l.start != 1 {
        el = el.attr("start", l.start.to_string());
    }
    for item in &l.items {
        let mut li_el = li();
        if let Some(done) = item.task {
            li_el = li_el.class("task").child(
                input()
                    .attr("type", "checkbox")
                    .attr("disabled", "")
                    .attr_bool("checked", move || done),
            );
        }
        for b in &item.blocks {
            // A tight list shows its paragraphs without the space around them.
            match (b, l.tight) {
                (Block::Paragraph(content), true) => {
                    li_el = li_el.children(inlines(content, on_link))
                }
                _ => {
                    if let Some(v) = block(b, on_link, false) {
                        li_el = li_el.child(v);
                    }
                }
            }
        }
        el = el.child(li_el);
    }
    el.into_view()
}

fn table_view(t: &Table, on_link: &OnLink) -> View {
    let cell = |tag: &'static str, col: usize, content: &[Inline]| {
        let class = t.alignments.get(col).copied().map_or("", align_class);
        element(tag)
            .class(class)
            .children(inlines(content, on_link))
    };
    let mut head = element("tr");
    for (i, h) in t.header.iter().enumerate() {
        head = head.child(cell("th", i, h));
    }
    let mut body = element("tbody");
    for row in &t.rows {
        let mut tr = element("tr");
        for (i, c) in row.iter().enumerate() {
            tr = tr.child(cell("td", i, c));
        }
        body = body.child(tr);
    }
    element("table")
        .child(element("thead").child(head))
        .child(body)
        .into_view()
}

/// A block quote that starts with `[!kind] Title` is a callout.
fn callout_view(blocks: &[Block], on_link: &OnLink) -> Option<View> {
    let Some(Block::Paragraph(first)) = blocks.first() else {
        return None;
    };
    let first = joined(first);
    let Some(Inline::Text(head)) = first.first() else {
        return None;
    };
    let marker = head.lines().next().unwrap_or_default();
    let c = callout(marker)?;
    let title = if c.title.is_empty() {
        c.kind.chars().next().map_or_else(String::new, |f| {
            f.to_uppercase().chain(c.kind.chars().skip(1)).collect()
        })
    } else {
        c.title
    };
    // What follows the marker line in the first paragraph stays as a paragraph of its own.
    let mut rest: Vec<Inline> = Vec::new();
    let after = head
        .split_once('\n')
        .map(|(_, r)| r.to_string())
        .filter(|r| !r.is_empty());
    if let Some(r) = after {
        rest.push(Inline::Text(r));
    }
    let mut skipped_break = false;
    for i in first.iter().skip(1) {
        if !skipped_break && matches!(i, Inline::SoftBreak) && rest.is_empty() {
            skipped_break = true;
            continue;
        }
        rest.push(i.clone());
    }
    let mut el = div()
        .class(format!("callout callout-{}", c.kind))
        .child(div().class("callout-title").text(title));
    if !rest.is_empty() {
        el = el.child(p().children(inlines(&rest, on_link)));
    }
    for b in &blocks[1..] {
        if let Some(v) = block(b, on_link, false) {
            el = el.child(v);
        }
    }
    Some(el.into_view())
}

fn block(b: &Block, on_link: &OnLink, in_note: bool) -> Option<View> {
    Some(match b {
        Block::Paragraph(content) => p().children(inlines(content, on_link)).into_view(),
        Block::Heading { level, content } => {
            let tag = match level {
                1 => "h1",
                2 => "h2",
                3 => "h3",
                4 => "h4",
                5 => "h5",
                _ => "h6",
            };
            let id = if in_note {
                String::new()
            } else {
                slug(&plain(content))
            };
            element(tag)
                .attr("id", id)
                .children(inlines(content, on_link))
                .into_view()
        }
        Block::ThematicBreak => element("hr").into_view(),
        Block::CodeBlock { info, literal, .. } => {
            let language = info.split_whitespace().next().unwrap_or_default();
            pre()
                .child(
                    element("code")
                        .class(format!("language-{language}"))
                        .text(literal.clone()),
                )
                .into_view()
        }
        // Raw HTML is not put on the page.
        Block::Html(_) => return None,
        Block::BlockQuote(blocks) => callout_view(blocks, on_link).unwrap_or_else(|| {
            let mut q = element("blockquote");
            for b in blocks {
                if let Some(v) = block(b, on_link, in_note) {
                    q = q.child(v);
                }
            }
            q.into_view()
        }),
        Block::List(l) => list_view(l, on_link),
        Block::Table(t) => table_view(t, on_link),
        Block::Math(tex) => match crate::math::typeset(tex, true) {
            Some(f) => div()
                .class("math display")
                .child(formula_view(&f, tex, "math-image"))
                .into_view(),
            None => div()
                .class("math display")
                .child(pre().text(tex.clone()))
                .into_view(),
        },
    })
}

/// A typeset formula as a picture sized in ems, standing on the text's baseline; its TeX is the alternative text.
fn formula_view(f: &crate::math::Formula, tex: &str, class: &str) -> View {
    let (w, h, a) = (f.width, f.height, f.align);
    img()
        .class(class.to_string())
        .attr("src", f.address.clone())
        .attr("alt", tex.to_string())
        .attr("title", tex.to_string())
        .css("width", move || format!("{w}em"))
        .css("height", move || format!("{h}em"))
        .css("vertical-align", move || format!("{a}em"))
        .into_view()
}
