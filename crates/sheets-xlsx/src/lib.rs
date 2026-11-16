//! `.xlsx` workbooks in and out of the sheet model.
//!
//! Reading keeps the whole package and the original XML of every sheet. Writing replaces only what this suite owns: the
//! cells of each sheet, the shared strings, the styles it added, and the list of sheets. Column widths, merged cells,
//! drawings, themes, defined names, document properties and everything else this crate does not understand are written
//! back as they were read.
//!
//! What is lost on a round trip, by design for now: rich-text formatting of a string whose text was edited, cached
//! results (they are recalculated and written fresh, and Excel is asked to recalculate on open), shared and array formula
//! markers (formulas are written one by one), `calcChain`, and cell metadata (`cm`, `vm`).

mod charts;
mod comments;
mod extras;
mod formula;
mod read;
mod scan;
mod strings;
mod styles;
mod tables;
mod write;

use office_fmt::Package;
use sheets_calc::Engine;
use sheets_model::Workbook;
use std::fmt;

pub use read::read;

/// Why a file could not be read or written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Package(office_fmt::Error),
    /// The file is a package but not a workbook this crate can read.
    Invalid(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Package(e) => e.fmt(f),
            Self::Invalid(m) => write!(f, "not a workbook: {m}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<office_fmt::Error> for Error {
    fn from(e: office_fmt::Error) -> Self {
        Self::Package(e)
    }
}

/// An open workbook: the calculated cells, and what is needed to write the file back without losing the rest of it.
#[derive(Debug)]
pub struct Xlsx {
    /// The live workbook. Edit it through the engine and it stays calculated.
    pub engine: Engine,
    pub(crate) package: Package,
    pub(crate) workbook_part: String,
    /// The part of each sheet of the workbook (same order), `None` for a sheet added since reading.
    pub(crate) sheet_parts: Vec<Option<String>>,
    pub(crate) styles_part: Option<String>,
    /// How many styles the file had; later ones were added here.
    pub(crate) original_styles: usize,
    /// The `<si>` elements of the shared strings the file had, to put back strings whose text did not change.
    pub(crate) original_strings: Vec<office_fmt::xml::Value>,
}

impl Xlsx {
    /// A new file for a workbook.
    #[must_use]
    pub fn new(workbook: Workbook) -> Self {
        Self {
            engine: Engine::new(workbook),
            package: Package::new(),
            workbook_part: "xl/workbook.xml".into(),
            sheet_parts: Vec::new(),
            styles_part: None,
            original_styles: 0,
            original_strings: Vec::new(),
        }
    }

    /// Add an empty sheet at the end.
    pub fn add_sheet(&mut self, name: &str) -> Option<usize> {
        let i = self.engine.add_sheet(name)?;
        self.sheet_parts.resize(i, None);
        self.sheet_parts.push(None);
        Some(i)
    }

    /// Remove a sheet and forget its part.
    pub fn remove_sheet(&mut self, sheet: usize) -> bool {
        if !self.engine.remove_sheet(sheet) {
            return false;
        }
        if sheet < self.sheet_parts.len() {
            self.sheet_parts.remove(sheet);
        }
        true
    }

    /// The `.xlsx` file.
    pub fn write(&self) -> Result<Vec<u8>, Error> {
        write::write(self)
    }
}
