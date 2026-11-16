//! Charts drawn from a range of cells.

use crate::refs::{CellRef, Range};

/// The shape of a chart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChartKind {
    /// Vertical bars.
    Column,
    /// Horizontal bars.
    Bar,
    Line,
    Area,
    Pie,
    /// Points placed by two numbers: the first column is across, the others up.
    Scatter,
}

impl ChartKind {
    pub const ALL: [Self; 6] = [
        Self::Column,
        Self::Bar,
        Self::Line,
        Self::Area,
        Self::Pie,
        Self::Scatter,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Column => "column",
            Self::Bar => "bar",
            Self::Line => "line",
            Self::Area => "area",
            Self::Pie => "pie",
            Self::Scatter => "scatter",
        }
    }

    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.name() == name)
    }
}

/// How a sparkline (a tiny chart in one cell) is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SparkKind {
    Line,
    Column,
    /// Up for gains, down for losses, all the same size.
    WinLoss,
}

/// A tiny chart in one cell, drawn from a row (or column) of cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sparkline {
    pub cell: CellRef,
    pub data: Range,
    pub kind: SparkKind,
    pub color: u32,
}

/// A chart placed on a sheet: its kind, the cells it shows, and where it sits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chart {
    pub kind: ChartKind,
    pub title: String,
    /// The cells it shows: names in the first row and categories in the first column when they are text.
    pub data: Range,
    /// Each row is a series (the default is each column).
    pub rows_as_series: bool,
    pub legend: bool,
    /// The cell its top left corner is at, and its size in pixels.
    pub at: CellRef,
    pub width: u32,
    pub height: u32,
}

impl Chart {
    /// A chart of `data` with the usual choices, put to the right of it.
    #[must_use]
    pub const fn new(kind: ChartKind, data: Range) -> Self {
        Self {
            kind,
            title: String::new(),
            data,
            rows_as_series: false,
            legend: true,
            at: CellRef::new(data.start.row, data.end.col + 1),
            width: 480,
            height: 300,
        }
    }
}
