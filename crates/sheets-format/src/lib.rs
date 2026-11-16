//! Number formats as spreadsheets write them: `#,##0.00`, `0%`, `yyyy-mm-dd`, `[h]:mm`, `0;[Red]-0`, `General`. One
//! engine for cell display and for `TEXT()`. English conventions only (`.` decimal point, `,` thousands) for now.
//!
//! ```
//! use sheets_format::format_value;
//! use sheets_model::Value;
//! assert_eq!(format_value("#,##0.00", &Value::Number(1234.5)), "1,234.50");
//! assert_eq!(format_value("yyyy-mm-dd", &Value::Number(45351.0)), "2024-02-29");
//! ```

mod builtin;
mod datetime;
mod general;
mod number;
mod parse;

pub use builtin::builtin;
pub use general::general;
pub use parse::{Color, Format, FormatError, Rendered};

use sheets_model::Value;

/// The text a cell shows for `value` under `format` (the colour of the format is dropped; see [`Format::render`]).
/// A format that does not parse shows the value as `General`.
#[must_use]
pub fn format_value(format: &str, value: &Value) -> String {
    Format::parse(format).map_or_else(
        |_| Format::general().render(value).text,
        |f| f.render(value).text,
    )
}
