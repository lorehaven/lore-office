//! Searching cell text: whether a text matches what was looked for, and the text with matches replaced.

/// A character folded for comparison: lower case unless the case matters.
fn fold(c: char, match_case: bool) -> char {
    if match_case {
        c
    } else {
        c.to_lowercase().next().unwrap_or(c)
    }
}

fn folded(text: &str, match_case: bool) -> Vec<char> {
    text.chars().map(|c| fold(c, match_case)).collect()
}

/// Where `query` occurs in `text` (character positions of the start of each non-overlapping match).
fn occurrences(text: &[char], query: &[char]) -> Vec<usize> {
    let mut found = Vec::new();
    let mut i = 0;
    while !query.is_empty() && i + query.len() <= text.len() {
        if text[i..i + query.len()] == *query {
            found.push(i);
            i += query.len();
        } else {
            i += 1;
        }
    }
    found
}

/// Whether `text` holds `query` (or, with `whole`, is exactly it). An empty query matches nothing.
#[must_use]
pub fn matches(text: &str, query: &str, match_case: bool, whole: bool) -> bool {
    if query.is_empty() {
        return false;
    }
    let (t, q) = (folded(text, match_case), folded(query, match_case));
    if whole {
        t == q
    } else {
        !occurrences(&t, &q).is_empty()
    }
}

/// `text` with every match of `query` replaced by `with` (the whole text with `whole`); `None` when nothing matched.
#[must_use]
pub fn replace(
    text: &str,
    query: &str,
    with: &str,
    match_case: bool,
    whole: bool,
) -> Option<String> {
    if query.is_empty() {
        return None;
    }
    let (t, q) = (folded(text, match_case), folded(query, match_case));
    if whole {
        return (t == q).then(|| with.to_string());
    }
    let at = occurrences(&t, &q);
    if at.is_empty() {
        return None;
    }
    let original: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    for start in at {
        out.extend(&original[i..start]);
        out.push_str(with);
        i = start + q.len();
    }
    out.extend(&original[i..]);
    Some(out)
}

/// What the find bar looks for, ready to test cells against.
pub enum Matcher {
    Plain {
        query: String,
        match_case: bool,
        whole: bool,
    },
    Pattern {
        regex: quench_regex::Regex,
    },
}

impl Matcher {
    /// A matcher for the query; `Err` carries what is wrong with a pattern.
    ///
    /// # Errors
    ///
    /// The regular expression's own explanation, when `regex` is on and the query is not a valid pattern.
    pub fn new(query: &str, match_case: bool, whole: bool, regex: bool) -> Result<Self, String> {
        if !regex {
            return Ok(Self::Plain {
                query: query.to_string(),
                match_case,
                whole,
            });
        }
        let pattern = if whole {
            format!("^(?:{query})$")
        } else {
            query.to_string()
        };
        quench_regex::RegexBuilder::new(&pattern)
            .case_insensitive(!match_case)
            .build()
            .map(|regex| Self::Pattern { regex })
            .map_err(|e| e.to_string())
    }

    /// Whether the text holds a match.
    #[must_use]
    pub fn matches(&self, text: &str) -> bool {
        match self {
            Self::Plain {
                query,
                match_case,
                whole,
            } => matches(text, query, *match_case, *whole),
            Self::Pattern { regex } => regex.is_match(text),
        }
    }

    /// The text with the matches replaced (`$1` in a pattern's replacement names a group); `None` when nothing matched.
    #[must_use]
    pub fn replace(&self, text: &str, with: &str) -> Option<String> {
        match self {
            Self::Plain {
                query,
                match_case,
                whole,
            } => replace(text, query, with, *match_case, *whole),
            Self::Pattern { regex } => regex
                .is_match(text)
                .then(|| regex.replace_all(text, with).into_owned()),
        }
    }
}
