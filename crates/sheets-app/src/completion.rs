//! Helping while a formula is typed: the function names that fit what was typed, and the arguments of the function being
//! written.

use sheets_formula::KNOWN;

/// Name, arguments, and what it does, for the functions people reach for most.
const SIGNATURES: &[(&str, &str)] = &[
    (
        "SUM",
        "SUM(number1, [number2], ...): adds numbers and ranges",
    ),
    ("AVERAGE", "AVERAGE(number1, [number2], ...): the mean"),
    ("COUNT", "COUNT(value1, [value2], ...): how many numbers"),
    (
        "COUNTA",
        "COUNTA(value1, [value2], ...): how many cells are not empty",
    ),
    ("MAX", "MAX(number1, [number2], ...): the largest"),
    ("MIN", "MIN(number1, [number2], ...): the smallest"),
    (
        "IF",
        "IF(test, then, [else]): one value when the test is true, another when not",
    ),
    (
        "IFS",
        "IFS(test1, value1, [test2, value2], ...): the value of the first true test",
    ),
    (
        "IFERROR",
        "IFERROR(value, if_error): a replacement for an error",
    ),
    (
        "AND",
        "AND(logical1, [logical2], ...): true when all are true",
    ),
    ("OR", "OR(logical1, [logical2], ...): true when any is true"),
    ("NOT", "NOT(logical): the opposite"),
    (
        "SUMIF",
        "SUMIF(range, criteria, [sum_range]): adds the cells that fit",
    ),
    (
        "SUMIFS",
        "SUMIFS(sum_range, criteria_range1, criteria1, ...): adds the cells that fit all",
    ),
    (
        "COUNTIF",
        "COUNTIF(range, criteria): counts the cells that fit",
    ),
    (
        "COUNTIFS",
        "COUNTIFS(range1, criteria1, ...): counts the cells that fit all",
    ),
    ("AVERAGEIF", "AVERAGEIF(range, criteria, [average_range])"),
    (
        "VLOOKUP",
        "VLOOKUP(lookup, table, column, [exact_match_is_FALSE]): finds a row by its first column",
    ),
    (
        "HLOOKUP",
        "HLOOKUP(lookup, table, row, [range_lookup]): finds a column by its first row",
    ),
    (
        "XLOOKUP",
        "XLOOKUP(lookup, lookup_range, return_range, [if_not_found], [match_mode], [search_mode])",
    ),
    (
        "INDEX",
        "INDEX(array, row, [column]): the value at a place in a range",
    ),
    (
        "MATCH",
        "MATCH(lookup, range, [match_type]): where a value is in a range",
    ),
    ("ROUND", "ROUND(number, digits): rounds to digits places"),
    ("ROUNDUP", "ROUNDUP(number, digits)"),
    ("ROUNDDOWN", "ROUNDDOWN(number, digits)"),
    ("ABS", "ABS(number): without the sign"),
    ("MOD", "MOD(number, divisor): the remainder"),
    ("POWER", "POWER(number, power)"),
    ("SQRT", "SQRT(number)"),
    ("CONCAT", "CONCAT(text1, [text2], ...): joins texts"),
    (
        "TEXTJOIN",
        "TEXTJOIN(delimiter, ignore_empty, text1, ...): joins texts with a separator",
    ),
    ("LEFT", "LEFT(text, [count]): the first characters"),
    ("RIGHT", "RIGHT(text, [count]): the last characters"),
    ("MID", "MID(text, start, count): characters from the middle"),
    ("LEN", "LEN(text): how many characters"),
    ("TRIM", "TRIM(text): without extra spaces"),
    ("UPPER", "UPPER(text)"),
    ("LOWER", "LOWER(text)"),
    ("PROPER", "PROPER(text): Each Word Capitalised"),
    (
        "SUBSTITUTE",
        "SUBSTITUTE(text, old, new, [instance]): replaces text",
    ),
    (
        "FIND",
        "FIND(find, within, [start]): where text is, exactly as written",
    ),
    (
        "SEARCH",
        "SEARCH(find, within, [start]): where text is, any case",
    ),
    ("TEXT", "TEXT(value, format): a number as text in a format"),
    ("VALUE", "VALUE(text): text as a number"),
    ("DATE", "DATE(year, month, day)"),
    ("TODAY", "TODAY(): today's date"),
    ("NOW", "NOW(): the date and time now"),
    ("YEAR", "YEAR(date)"),
    ("MONTH", "MONTH(date)"),
    ("DAY", "DAY(date)"),
    (
        "EDATE",
        "EDATE(date, months): the same day some months later",
    ),
    ("EOMONTH", "EOMONTH(date, months): the end of a month"),
    (
        "DATEDIF",
        "DATEDIF(start, end, unit): the difference in Y, M or D",
    ),
    (
        "NETWORKDAYS",
        "NETWORKDAYS(start, end, [holidays]): working days between",
    ),
    (
        "PMT",
        "PMT(rate, periods, present_value, [future_value], [type]): a loan payment",
    ),
    (
        "FV",
        "FV(rate, periods, payment, [present_value], [type]): a future value",
    ),
    (
        "PV",
        "PV(rate, periods, payment, [future_value], [type]): a present value",
    ),
    ("NPV", "NPV(rate, value1, [value2], ...): net present value"),
    ("IRR", "IRR(values, [guess]): the internal rate of return"),
    (
        "FILTER",
        "FILTER(array, include, [if_empty]): the rows that fit",
    ),
    (
        "SORT",
        "SORT(array, [sort_index], [order], [by_col]): sorted rows",
    ),
    (
        "SORTBY",
        "SORTBY(array, by_array1, [order1], ...): rows sorted by other values",
    ),
    (
        "UNIQUE",
        "UNIQUE(array, [by_col], [exactly_once]): without repeats",
    ),
    (
        "SEQUENCE",
        "SEQUENCE(rows, [columns], [start], [step]): a series of numbers",
    ),
    ("TRANSPOSE", "TRANSPOSE(array): rows and columns swapped"),
    (
        "LET",
        "LET(name1, value1, [name2, value2], ..., calculation): names for values inside a formula",
    ),
    (
        "INDIRECT",
        "INDIRECT(reference_text): the cell a text names",
    ),
    (
        "OFFSET",
        "OFFSET(reference, rows, columns, [height], [width]): a range moved from another",
    ),
    (
        "SUMPRODUCT",
        "SUMPRODUCT(array1, [array2], ...): products added up",
    ),
    (
        "RANK",
        "RANK(number, ref, [order]): where a number stands in a list",
    ),
    ("LARGE", "LARGE(array, k): the k-th largest"),
    ("SMALL", "SMALL(array, k): the k-th smallest"),
    ("MEDIAN", "MEDIAN(number1, [number2], ...)"),
    (
        "STDEV",
        "STDEV(number1, [number2], ...): the standard deviation",
    ),
    (
        "CHOOSE",
        "CHOOSE(index, value1, [value2], ...): the value at a position",
    ),
    ("SWITCH", "SWITCH(value, match1, result1, ..., [default])"),
    ("ISBLANK", "ISBLANK(value): true for an empty cell"),
    ("ISNUMBER", "ISNUMBER(value)"),
    ("ISTEXT", "ISTEXT(value)"),
    ("HYPERLINK", "HYPERLINK(link, [label])"),
];

/// Where the word being typed at the end of a formula starts, and the word, when it could be a function or a name.
fn word_at_end(draft: &str) -> Option<(usize, &str)> {
    if !draft.starts_with('=') {
        return None;
    }
    // Inside a quoted text nothing is suggested.
    if draft.chars().filter(|c| *c == '"').count() % 2 == 1 {
        return None;
    }
    let start = draft
        .char_indices()
        .rev()
        .take_while(|(_, c)| c.is_alphanumeric() || *c == '_' || *c == '.')
        .last()
        .map(|(i, _)| i)?;
    let word = &draft[start..];
    word.chars()
        .next()
        .is_some_and(char::is_alphabetic)
        .then_some((start, word))
}

/// Function names and defined names that begin with what was just typed (at most `limit`).
#[must_use]
pub fn suggestions(draft: &str, names: &[String], limit: usize) -> Vec<String> {
    let Some((_, word)) = word_at_end(draft) else {
        return Vec::new();
    };
    let upper = word.to_uppercase();
    let mut out: Vec<String> = KNOWN
        .iter()
        .filter(|f| f.starts_with(&upper) && **f != upper)
        .map(|f| (*f).to_string())
        .collect();
    out.extend(
        names
            .iter()
            .filter(|n| n.to_uppercase().starts_with(&upper) && n.to_uppercase() != upper)
            .cloned(),
    );
    out.truncate(limit);
    out
}

/// The draft with the word at its end replaced by a suggestion (a function gets its opening bracket).
#[must_use]
pub fn accept(draft: &str, suggestion: &str) -> String {
    let Some((start, _)) = word_at_end(draft) else {
        return draft.to_string();
    };
    let function = KNOWN.contains(&suggestion);
    format!(
        "{}{suggestion}{}",
        &draft[..start],
        if function { "(" } else { "" }
    )
}

/// The signature of the function whose arguments are being typed, and which argument (from 0) the end of the draft is in.
#[must_use]
pub fn hint(draft: &str) -> Option<(String, usize)> {
    if !draft.starts_with('=') {
        return None;
    }
    let mut stack: Vec<(usize, usize)> = Vec::new(); // (index of the bracket, commas so far)
    let mut quoted = false;
    for (i, c) in draft.char_indices() {
        match c {
            '"' => quoted = !quoted,
            '(' if !quoted => stack.push((i, 0)),
            ')' if !quoted => {
                stack.pop();
            }
            ',' if !quoted => {
                if let Some(top) = stack.last_mut() {
                    top.1 += 1;
                }
            }
            _ => {}
        }
    }
    let (open, commas) = *stack.last()?;
    let before = &draft[..open];
    let start = before
        .char_indices()
        .rev()
        .take_while(|(_, c)| c.is_alphanumeric() || *c == '_' || *c == '.')
        .last()
        .map(|(i, _)| i)?;
    let name = before[start..].to_uppercase();
    SIGNATURES
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, s)| ((*s).to_string(), commas))
}
