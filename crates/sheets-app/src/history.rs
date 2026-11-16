//! Undo and redo: every edit is a step, a list of cells with what they were and what they became.

use sheets_model::{CellRef, Content, Workbook};

/// The most steps kept.
pub const MAX_STEPS: usize = 500;

/// All there is to a cell that an edit can change: what is in it and how it looks.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CellState {
    pub content: Option<Content>,
    /// An index into the workbook's styles.
    pub style: u32,
}

/// One cell of a step.
#[derive(Clone, Debug, PartialEq)]
pub struct Delta {
    pub sheet: usize,
    pub at: CellRef,
    pub before: CellState,
    pub after: CellState,
}

/// A whole workbook as it was, for changes too big to list cell by cell (rows inserted, a sort): undoing swaps it with the
/// workbook now, which makes it the step to redo.
#[derive(Clone, Debug, PartialEq)]
pub struct Snapshot {
    pub workbook: Workbook,
    /// The sheet that was on screen.
    pub sheet: usize,
}

/// One undoable step.
#[derive(Clone, Debug, PartialEq)]
pub enum Step {
    Cells(Vec<Delta>),
    Book(Box<Snapshot>),
}

/// The most whole-workbook steps kept (each holds a copy of the workbook).
pub const MAX_BOOK_STEPS: usize = 20;

/// Steps that can be undone, and steps that were undone and can be done again.
#[derive(Debug, Default)]
pub struct History {
    undo: Vec<Step>,
    redo: Vec<Step>,
}

impl History {
    /// A new step; whatever could have been redone no longer can.
    pub fn push(&mut self, step: Vec<Delta>) {
        self.push_step(Step::Cells(step));
    }

    /// A new step that holds the workbook as it was before the change.
    pub fn push_book(&mut self, snapshot: Snapshot) {
        self.push_step(Step::Book(Box::new(snapshot)));
        // Keep only the newest few of these: they are the big ones.
        let mut seen = 0;
        for step in self.undo.iter_mut().rev() {
            if matches!(step, Step::Book(_)) {
                seen += 1;
                if seen > MAX_BOOK_STEPS {
                    *step = Step::Cells(Vec::new());
                }
            }
        }
        self.undo
            .retain(|s| !matches!(s, Step::Cells(c) if c.is_empty()));
    }

    fn push_step(&mut self, step: Step) {
        self.redo.clear();
        self.undo.push(step);
        if self.undo.len() > MAX_STEPS {
            self.undo.remove(0);
        }
    }

    pub fn pop_undo(&mut self) -> Option<Step> {
        self.undo.pop()
    }

    pub fn pop_redo(&mut self) -> Option<Step> {
        self.redo.pop()
    }

    pub fn push_redo(&mut self, step: Step) {
        self.redo.push(step);
    }

    pub fn push_undo(&mut self, step: Step) {
        self.undo.push(step);
    }

    #[must_use]
    pub const fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    #[must_use]
    pub const fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}
