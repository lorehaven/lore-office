//! The container under office files: an Open Packaging Conventions package (a zip of parts), the relationships that tie
//! parts together, content types, and XML parts as plain trees. `docx`, `xlsx` and `pptx` are all this.
//!
//! The package keeps every part it was given, in order, as bytes. A format reader looks at the parts it understands and
//! leaves the rest alone, so a file written back still has the drawings, themes and properties this suite does not know.
//!
//! ```
//! use office_fmt::Package;
//! let mut p = Package::new();
//! p.set("a/b.txt", b"hello".to_vec());
//! let again = Package::read(&p.write().unwrap()).unwrap();
//! assert_eq!(again.get("a/b.txt"), Some(&b"hello"[..]));
//! ```

mod content_types;
mod error;
mod package;
pub mod rels;
pub mod xml;

pub use content_types::ContentTypes;
pub use error::Error;
pub use package::Package;
