//! A note as a page of its own: standalone HTML that needs nothing from the app.

use crate::state::Deck;
use notes_md::{flatten_wikilinks, parse_front, slides};
use quench_html::markdown::{Block, Document, Inline, List, Options, parse, render_html};

const PAGE_CSS: &str = "body{margin:0;font:17px/1.6 system-ui,sans-serif;color:#1f2328;background:#fff}@media(prefers-color-scheme:dark){body{color:#e6e8ea;background:#15181c}a{color:#8ab4ff}}main{max-width:46rem;margin:0 auto;padding:2rem 1.2rem}h1,h2,h3{line-height:1.25}pre{padding:.8em 1em;overflow:auto;background:rgba(127,127,127,.15);border-radius:8px}code{font-family:ui-monospace,Menlo,Consolas,monospace;font-size:.92em}table{border-collapse:collapse}th,td{padding:.3em .8em;border:1px solid rgba(127,127,127,.5)}blockquote{margin-left:0;padding-left:1em;border-left:4px solid rgba(127,127,127,.5)}img{max-width:100%}";

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// The document with raw HTML turned into plain text, so what is written is what is shown.
fn defang_blocks(blocks: &mut Vec<Block>) {
    blocks.retain(|b| !matches!(b, Block::Html(_)));
    for b in blocks.iter_mut() {
        match b {
            Block::Paragraph(i) | Block::Heading { content: i, .. } => defang_inlines(i),
            Block::BlockQuote(inner) => defang_blocks(inner),
            Block::List(List { items, .. }) => {
                for item in items {
                    defang_blocks(&mut item.blocks);
                }
            }
            Block::Table(t) => {
                for cell in t.header.iter_mut().chain(t.rows.iter_mut().flatten()) {
                    defang_inlines(cell);
                }
            }
            _ => {}
        }
    }
}

fn defang_inlines(list: &mut [Inline]) {
    for i in list.iter_mut() {
        match i {
            Inline::Html(h) => *i = Inline::Text(h.clone()),
            Inline::Emph(c) | Inline::Strong(c) | Inline::Strikethrough(c) => defang_inlines(c),
            Inline::Link { children, .. } | Inline::Image { children, .. } => {
                defang_inlines(children)
            }
            _ => {}
        }
    }
}

fn html_of(markdown: &str) -> String {
    let mut doc: Document = parse(&flatten_wikilinks(markdown), Options::ALL);
    defang_blocks(&mut doc.blocks);
    for f in &mut doc.footnotes {
        defang_blocks(&mut f.blocks);
    }
    embed_math(&render_html(&doc))
}

fn unescape(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
}

/// Put typeset formulas (inline SVG) where the page has math written as TeX; what cannot be set stays as TeX.
fn embed_math(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    loop {
        let inline = rest.find("<span class=\"math inline\">\\(");
        let display = rest.find("<div class=\"math display\">\\[");
        let (at, open, close, shown) = match (inline, display) {
            (Some(i), Some(d)) if i < d => {
                (i, "<span class=\"math inline\">\\(", "\\)</span>", false)
            }
            (Some(i), None) => (i, "<span class=\"math inline\">\\(", "\\)</span>", false),
            (_, Some(d)) => (d, "<div class=\"math display\">\\[", "\\]</div>", true),
            (None, None) => break,
        };
        out.push_str(&rest[..at]);
        let body = &rest[at + open.len()..];
        let Some(end) = body.find(close) else {
            out.push_str(&rest[at..]);
            return out;
        };
        let whole = &rest[at..at + open.len() + end + close.len()];
        match crate::math::svg(&unescape(&body[..end]), shown) {
            Some(svg) if shown => {
                out.push_str(
                    "<div class=\"math display\" style=\"text-align:center;margin:1em 0\">",
                );
                out.push_str(&svg);
                out.push_str("</div>");
            }
            Some(svg) => out.push_str(&svg),
            None => out.push_str(whole),
        }
        rest = &rest[at + whole.len()..];
    }
    out.push_str(rest);
    out
}

/// A note as a web page.
#[must_use]
pub fn note_html(title: &str, text: &str) -> String {
    let body = html_of(parse_front(text).1);
    format!(
        "<!doctype html>\n<html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>{}</title><style>{PAGE_CSS}</style></head><body><main>\n{body}</main></body></html>\n",
        escape(title)
    )
}

/// A deck as a web page that shows one slide at a time (arrow keys, space, Home and End; a click advances).
#[must_use]
pub fn deck_html(deck: &Deck) -> String {
    let (w, h) = deck.stage;
    let mut sections = String::new();
    for (n, s) in deck.slides.iter().enumerate() {
        let layout = s
            .layout
            .as_deref()
            .map_or_else(String::new, |l| format!(" layout-{}", escape(l)));
        let notes = if s.notes.is_empty() {
            String::new()
        } else {
            format!("<aside hidden>{}</aside>", escape(&s.notes))
        };
        sections.push_str(&format!(
            "<section class=\"slide theme-{}{layout}\" data-n=\"{n}\"{}>{}{notes}</section>\n",
            escape(&deck.theme),
            if n == 0 { "" } else { " hidden" },
            html_of(&s.markdown)
        ));
    }
    let css = format!(
        "{}\nhtml,body{{margin:0;height:100%;background:#000;overflow:hidden}}#stage{{position:absolute;left:50%;top:50%;width:{w}px;height:{h}px;transform-origin:center}}#stage .slide{{position:absolute;inset:0}}#stage .slide[hidden]{{display:none}}#count{{position:fixed;right:12px;bottom:8px;font:14px system-ui;color:#888}}",
        include_str!("../assets/deck.css")
    );
    let script = format!(
        "var n=0,all=[].slice.call(document.querySelectorAll('.slide')),st=document.getElementById('stage'),ct=document.getElementById('count');function fit(){{var s=Math.min(innerWidth/{w},innerHeight/{h});st.style.transform='translate(-50%,-50%) scale('+s+')'}}function show(i){{n=Math.max(0,Math.min(all.length-1,i));all.forEach(function(s,k){{s.hidden=k!==n}});ct.textContent=(n+1)+' / '+all.length;location.hash=n+1}}addEventListener('resize',fit);addEventListener('keydown',function(e){{if(['ArrowRight','ArrowDown','PageDown',' ','Enter'].indexOf(e.key)>=0)show(n+1);else if(['ArrowLeft','ArrowUp','PageUp','Backspace'].indexOf(e.key)>=0)show(n-1);else if(e.key==='Home')show(0);else if(e.key==='End')show(all.length-1)}});addEventListener('click',function(){{show(n+1)}});fit();show(parseInt(location.hash.slice(1)||'1',10)-1)"
    );
    format!(
        "<!doctype html>\n<html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>{}</title><style>{css}</style></head><body><div id=\"stage\">\n{sections}</div><div id=\"count\"></div><script>{script}</script></body></html>\n",
        escape(&deck.title)
    )
}

/// The slides of a note's text, for a caller that has no state (a test, an export of another note).
#[must_use]
pub fn slides_of(text: &str) -> Vec<notes_md::Slide> {
    slides(parse_front(text).1)
}
