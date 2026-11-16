//! Front matter: a block of `key: value` lines between two `---` lines at the very top.

/// What a note says about itself at its top.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FrontMatter {
    pub title: Option<String>,
    /// The name of the theme a deck is shown in.
    pub theme: Option<String>,
    /// `16:9`, `4:3`...
    pub aspect: Option<String>,
    pub transition: Option<String>,
    /// Every other key, in the order written.
    pub extra: Vec<(String, String)>,
}

fn unquote(v: &str) -> String {
    let v = v.trim();
    for q in ['"', '\''] {
        if v.len() >= 2 && v.starts_with(q) && v.ends_with(q) {
            return v[1..v.len() - 1].to_string();
        }
    }
    v.to_string()
}

/// The front matter of a note and the text after it. A note without one gives the defaults and all of the text.
#[must_use]
pub fn parse_front(text: &str) -> (FrontMatter, &str) {
    let mut front = FrontMatter::default();
    let Some(rest) = text
        .strip_prefix("---\n")
        .or_else(|| text.strip_prefix("---\r\n"))
    else {
        return (front, text);
    };
    let mut offset = 0;
    let mut closed = None;
    for line in rest.split_inclusive('\n') {
        if line.trim_end() == "---" || line.trim_end() == "..." {
            closed = Some(offset + line.len());
            break;
        }
        offset += line.len();
    }
    let Some(end) = closed else {
        return (FrontMatter::default(), text);
    };
    for line in rest[..offset].lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let (key, value) = (key.trim(), unquote(value));
        if key.is_empty() || key.contains(' ') && !key.starts_with('"') {
            continue;
        }
        match key {
            "title" => front.title = Some(value),
            "theme" => front.theme = Some(value),
            "aspect" | "ratio" => front.aspect = Some(value),
            "transition" => front.transition = Some(value),
            _ => front.extra.push((key.to_string(), value)),
        }
    }
    (front, &rest[end..])
}

/// The note with one front matter key set (added, or replaced), keeping the rest as it was.
#[must_use]
pub fn set_front_matter_key(text: &str, key: &str, value: &str) -> String {
    let line = format!("{key}: {value}");
    let Some(rest) = text.strip_prefix("---\n") else {
        return format!("---\n{line}\n---\n{text}");
    };
    let mut out = String::from("---\n");
    let mut done = false;
    let mut closed = false;
    for l in rest.split_inclusive('\n') {
        if !closed && l.trim_end() == "---" {
            if !done {
                out.push_str(&line);
                out.push('\n');
                done = true;
            }
            closed = true;
        } else if !closed && l.split_once(':').is_some_and(|(k, _)| k.trim() == key) {
            out.push_str(&line);
            out.push('\n');
            done = true;
            continue;
        }
        out.push_str(l);
    }
    if closed {
        out
    } else {
        format!("---\n{line}\n---\n{text}")
    }
}
