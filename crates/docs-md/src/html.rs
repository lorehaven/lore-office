//! The document as a standalone web page.

use docs_text::{Align, Doc, Family, Kind};
use std::fmt::Write as _;

fn escape(text: &str, out: &mut String) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            c => out.push(c),
        }
    }
}

fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let n = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        out.push(char::from(T[(n >> 18) as usize & 63]));
        out.push(char::from(T[(n >> 12) as usize & 63]));
        out.push(if chunk.len() > 1 {
            char::from(T[(n >> 6) as usize & 63])
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            char::from(T[n as usize & 63])
        } else {
            '='
        });
    }
    out
}

fn inline(doc: &Doc, para: usize, out: &mut String) {
    for r in &doc.paras()[para].runs {
        let s = r.style;
        let mut style = String::new();
        if let Some(sz) = s.size {
            let _ = write!(style, "font-size:{}pt;", f64::from(sz) / 2.0);
        }
        if let Some(c) = s.color {
            let _ = write!(style, "color:#{c:06x};");
        }
        if let Some(h) = s.highlight {
            let _ = write!(style, "background:#{h:06x};");
        }
        let named = s
            .font
            .and_then(|n| doc.font_name(n))
            .map(|n| n.replace(['\'', '"', '\\', ';'], ""));
        let lead = named.map_or_else(String::new, |n| format!("'{n}',"));
        let stack = match s.family {
            Some(Family::Sans) => Some("Arial,Helvetica,sans-serif"),
            Some(Family::Serif) => Some("'Times New Roman',serif"),
            Some(Family::Mono) => Some("'Courier New',monospace"),
            None => None,
        };
        if let Some(stack) = stack {
            let _ = write!(style, "font-family:{lead}{stack};");
        }
        let mut tags: Vec<&str> = Vec::new();
        for (on, tag) in [
            (s.bold, "b"),
            (s.italic, "i"),
            (s.underline, "u"),
            (s.strike, "s"),
            (s.sup, "sup"),
            (s.sub, "sub"),
        ] {
            if on {
                tags.push(tag);
            }
        }
        let link = s.link.and_then(|l| doc.url(l));
        if let Some(u) = link {
            out.push_str("<a href=\"");
            escape(u, out);
            out.push_str("\">");
        }
        if !style.is_empty() {
            let _ = write!(out, "<span style=\"{style}\">");
        }
        for t in &tags {
            let _ = write!(out, "<{t}>");
        }
        for (n, piece) in r.text.split(docs_text::LINE_BREAK).enumerate() {
            if n > 0 {
                out.push_str("<br>");
            }
            escape(piece, out);
        }
        for t in tags.iter().rev() {
            let _ = write!(out, "</{t}>");
        }
        if !style.is_empty() {
            out.push_str("</span>");
        }
        if link.is_some() {
            out.push_str("</a>");
        }
    }
}

/// The document as a complete web page, with its pictures inside it.
#[must_use]
pub fn to_html(doc: &Doc, title: &str) -> String {
    let mut out = String::from("<!doctype html>\n<html><head><meta charset=\"utf-8\"><title>");
    escape(title, &mut out);
    out.push_str("</title><style>body{font-family:'Times New Roman',serif;max-width:46em;margin:2em auto;padding:0 1em;line-height:1.5}table{border-collapse:collapse;margin:1em 0}td{border:1px solid #888;padding:.3em .6em;vertical-align:top}img{max-width:100%}.pb{break-before:page}</style></head><body>\n");
    let paras = doc.paras();
    let mut i = 0;
    let mut list: Option<Kind> = None;
    while i < paras.len() {
        let p = &paras[i];
        let in_list = p.cell.is_none() && matches!(p.kind, Kind::Bullet | Kind::Numbered);
        if !in_list && let Some(k) = list.take() {
            out.push_str(if k == Kind::Bullet {
                "</ul>\n"
            } else {
                "</ol>\n"
            });
        }
        if let Some(cell) = p.cell {
            let mut j = i;
            while j < paras.len() && paras[j].cell.is_some_and(|c| c.table == cell.table) {
                j += 1;
            }
            let cols = doc.table_cols(cell.table).max(1);
            let widths = doc.table_widths(cell.table);
            let header = doc.table_header(cell.table);
            out.push_str("<table>\n");
            for (n, row) in paras[i..j].chunks(cols).enumerate() {
                out.push_str("<tr>");
                for (c, _) in row.iter().enumerate() {
                    let tag = if header && n == 0 { "th" } else { "td" };
                    let _ = write!(
                        out,
                        "<{tag} style=\"width:{}%\">",
                        f64::from(widths.get(c).copied().unwrap_or(0)) / 10.0
                    );
                    inline(doc, i + n * cols + c, &mut out);
                    let _ = write!(out, "</{tag}>");
                }
                out.push_str("</tr>\n");
            }
            out.push_str("</table>\n");
            i = j;
            continue;
        }
        if let Some(pic) = p.picture.and_then(|n| doc.picture(n)) {
            let (w, _) = pic.size();
            let _ = writeln!(
                out,
                "<p style=\"text-align:{}\"><img alt=\"\" width=\"{}\" src=\"data:{};base64,{}\"></p>",
                match p.align {
                    Align::Center => "center",
                    Align::Right => "right",
                    _ => "left",
                },
                (w * 4.0 / 3.0).round(),
                pic.mime,
                base64(&pic.data)
            );
            i += 1;
            continue;
        }
        if matches!(p.kind, Kind::Bullet | Kind::Numbered) {
            if list != Some(p.kind) {
                if let Some(k) = list.take() {
                    out.push_str(if k == Kind::Bullet {
                        "</ul>\n"
                    } else {
                        "</ol>\n"
                    });
                }
                out.push_str(if p.kind == Kind::Bullet {
                    "<ul>\n"
                } else {
                    "<ol>\n"
                });
                list = Some(p.kind);
            }
            out.push_str("<li>");
            inline(doc, i, &mut out);
            out.push_str("</li>\n");
            i += 1;
            continue;
        }
        let tag = match p.kind {
            Kind::Heading(1) => "h1",
            Kind::Heading(2) => "h2",
            Kind::Heading(3) => "h3",
            Kind::Heading(4) => "h4",
            Kind::Heading(5) => "h5",
            Kind::Heading(_) => "h6",
            _ => "p",
        };
        let mut style = String::new();
        match p.align {
            Align::Center => style.push_str("text-align:center;"),
            Align::Right => style.push_str("text-align:right;"),
            Align::Justify => style.push_str("text-align:justify;"),
            Align::Left => {}
        }
        if p.fmt.indent != 0 {
            let _ = write!(style, "margin-left:{}pt;", f64::from(p.fmt.indent) / 20.0);
        }
        if p.fmt.line != 0 {
            let _ = write!(
                style,
                "line-height:{};",
                f64::from(p.fmt.line) / 240.0 * 1.5
            );
        }
        let class = if p.fmt.page_break {
            " class=\"pb\""
        } else {
            ""
        };
        let _ = write!(
            out,
            "<{tag}{class}{}>",
            if style.is_empty() {
                String::new()
            } else {
                format!(" style=\"{style}\"")
            }
        );
        if p.runs.is_empty() {
            out.push_str("<br>");
        }
        inline(doc, i, &mut out);
        let _ = writeln!(out, "</{tag}>");
        i += 1;
    }
    if let Some(k) = list.take() {
        out.push_str(if k == Kind::Bullet {
            "</ul>\n"
        } else {
            "</ol>\n"
        });
    }
    out.push_str("</body></html>\n");
    out
}
