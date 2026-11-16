use sheets_formula::{Context, Expr, Operand, ParseError, eval, eval_operand, parse};
use sheets_model::{Axis, CellError, CellRef, Content, Range, Sheet, Style, Value, Workbook};
use std::cell::Cell as StateCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt;
use std::rc::Rc;

/// A range with no more cells than this is registered cell by cell (so finding who reads a cell is a lookup); bigger ones are
/// kept as ranges, shared between formulas that read the same one.
const SMALL_RANGE: u64 = 128;

/// A cell of the workbook: the index of its sheet and its address.
pub type Key = (usize, CellRef);

/// What a formula that gives several values fills in: `values` row by row, `cols` wide.
#[derive(Debug, Clone)]
struct Spill {
    cols: u32,
    values: Vec<Value>,
}

impl Spill {
    fn rows(&self) -> u32 {
        u32::try_from(self.values.len() / self.cols.max(1) as usize).unwrap_or(0)
    }
}

/// A formula cell: the parsed formula, its last value and what it reads.
#[derive(Debug)]
struct Calc {
    expr: Result<Expr, ParseError>,
    value: Value,
    /// The rectangle this formula gives when it gives more than one value: the other cells show these.
    spill: Option<Spill>,
    cells: Vec<Key>,
    ranges: Vec<(usize, Range)>,
}

/// A workbook that keeps its formulas calculated.
///
/// Two limits, both on purpose for now: a cell that is part of a cycle, or reads one, gets `#CIRC!`; and a formula
/// that does not parse gets `#NAME?` (see [`Engine::parse_error`]).
#[derive(Debug)]
pub struct Engine {
    workbook: Workbook,
    calc: HashMap<Key, Calc>,
    /// Formula cells that read a given cell directly.
    by_cell: HashMap<Key, HashSet<Key>>,
    /// Formula cells that read a range, per sheet.
    by_range: HashMap<usize, HashMap<Range, HashSet<Key>>>,
    /// Formula cells whose value can change with no cell they read changing: they are calculated again after every edit.
    volatile: HashSet<Key>,
    /// Cells that show a value some other formula gave them, and which formula.
    spilled: HashMap<Key, Key>,
    /// Formulas that gave several values but found something in the way.
    blocked: HashSet<Key>,
    /// Cells a spill started or stopped filling since the last report.
    touched: Vec<Key>,
    clock: Option<Clock>,
    seed: StateCell<u64>,
}

/// Where the engine gets the date and time: the local time as a serial number.
#[derive(Clone)]
struct Clock(Rc<dyn Fn() -> f64>);

impl fmt::Debug for Clock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Clock")
    }
}

struct View<'a> {
    engine: &'a Engine,
    sheet: usize,
    at: CellRef,
}

impl Context for View<'_> {
    fn now(&self) -> f64 {
        self.engine.clock.as_ref().map_or(0.0, |c| (c.0)())
    }

    fn random(&self) -> f64 {
        self.engine.next_random()
    }

    fn origin(&self) -> Option<CellRef> {
        Some(self.at)
    }

    fn name(&self, name: &str) -> Option<Expr> {
        self.engine.name_expr(name)
    }

    fn cell(&self, sheet: Option<&str>, at: CellRef) -> Value {
        let idx = match sheet {
            None => self.sheet,
            Some(name) => match self.engine.sheet_index(name) {
                Some(i) => i,
                None => return Value::Error(CellError::Ref),
            },
        };
        self.engine.value(idx, at)
    }
}

impl Engine {
    /// Take a workbook and calculate every formula in it.
    #[must_use]
    pub fn new(workbook: Workbook) -> Self {
        let mut e = Self {
            workbook,
            calc: HashMap::new(),
            by_cell: HashMap::new(),
            by_range: HashMap::new(),
            volatile: HashSet::new(),
            spilled: HashMap::new(),
            blocked: HashSet::new(),
            touched: Vec::new(),
            clock: None,
            seed: StateCell::new(0x9E37_79B9_7F4A_7C15),
        };
        e.rebuild();
        e
    }

    /// The cells as typed, without calculated values.
    #[must_use]
    pub const fn workbook(&self) -> &Workbook {
        &self.workbook
    }

    #[must_use]
    pub fn sheet_index(&self, name: &str) -> Option<usize> {
        self.workbook
            .sheets
            .iter()
            .position(|s| s.name.eq_ignore_ascii_case(name))
    }

    /// What a cell shows: a constant as it is, a formula as calculated, `Empty` for nothing (or a sheet that is not there).
    #[must_use]
    pub fn value(&self, sheet: usize, at: CellRef) -> Value {
        if let Some(c) = self.calc.get(&(sheet, at)) {
            return c.value.clone();
        }
        if let Some(anchor) = self.spilled.get(&(sheet, at))
            && let Some(sp) = self.calc.get(anchor).and_then(|c| c.spill.as_ref())
        {
            let (r, c) = (at.row - anchor.1.row, at.col - anchor.1.col);
            if let Some(v) = sp.values.get((r * sp.cols + c) as usize) {
                return v.clone();
            }
        }
        match self.workbook.sheets.get(sheet).and_then(|s| s.get(at)) {
            Some(Content::Value(v)) => v.clone(),
            _ => Value::Empty,
        }
    }

    /// Why a formula cell did not parse.
    #[must_use]
    pub fn parse_error(&self, sheet: usize, at: CellRef) -> Option<&ParseError> {
        self.calc
            .get(&(sheet, at))
            .and_then(|c| c.expr.as_ref().err())
    }

    /// Whether the cell is in, or reads, a cycle.
    #[must_use]
    pub fn is_circular(&self, sheet: usize, at: CellRef) -> bool {
        self.value(sheet, at) == Value::Error(CellError::Circular)
    }

    /// Put something in a cell and recalculate what depends on it. Returns every cell whose value is now different,
    /// the cell itself included when it changed. A sheet index past the end changes nothing.
    pub fn set(&mut self, sheet: usize, at: CellRef, content: Content) -> Vec<Key> {
        if sheet >= self.workbook.sheets.len() {
            return Vec::new();
        }
        let key = (sheet, at);
        let before = self.value(sheet, at);
        self.unregister(key);
        self.clear_spill(key);
        self.calc.remove(&key);
        match &content {
            Content::Formula(text) => {
                self.workbook.sheets[sheet].set(at, content.clone());
                self.install(key, text);
            }
            Content::Value(_) => self.workbook.sheets[sheet].set(at, content),
        }
        // Cells that depend on the clock or on chance are calculated again with every edit, as in other spreadsheets.
        let mut seeds = vec![key];
        seeds.extend(self.volatile.iter().copied().filter(|k| *k != key));
        // A formula whose result was spread over this cell, or could not be for want of room, looks again.
        seeds.extend(self.spilled.get(&key).copied());
        seeds.extend(self.blocked.iter().copied().filter(|k| *k != key));
        let mut changed = self.propagate(&seeds, &HashMap::from([(key, before)]));
        changed.sort_unstable();
        changed.dedup();
        changed
    }

    /// The content of a cell as typed, `None` when it has none.
    #[must_use]
    pub fn content(&self, sheet: usize, at: CellRef) -> Option<&Content> {
        self.workbook.sheets.get(sheet).and_then(|s| s.get(at))
    }

    /// The style index of a cell (0 for the default).
    #[must_use]
    pub fn style(&self, sheet: usize, at: CellRef) -> u32 {
        self.workbook
            .sheets
            .get(sheet)
            .map_or(0, |s| s.effective_style(at))
    }

    /// The index of a style, added to the workbook when it is new.
    pub fn style_id(&mut self, style: &Style) -> u32 {
        self.workbook.style_id(style)
    }

    /// Give a cell a style (an index from [`Engine::style_id`]). No value changes, so nothing is recalculated. Returns
    /// whether the style was different.
    pub fn set_style(&mut self, sheet: usize, at: CellRef, style: u32) -> bool {
        let Some(s) = self.workbook.sheets.get_mut(sheet) else {
            return false;
        };
        let changed = s.style(at) != style;
        s.set_style(at, style);
        changed
    }

    /// Set a column's width in pixels (`None`: the default). Nothing is recalculated.
    pub fn set_col_width(&mut self, sheet: usize, col: u32, width: Option<f64>) {
        if let Some(s) = self.workbook.sheets.get_mut(sheet) {
            s.set_col_width(col, width);
        }
    }

    /// Set a row's height in pixels (`None`: the default). Nothing is recalculated.
    pub fn set_row_height(&mut self, sheet: usize, row: u32, height: Option<f64>) {
        if let Some(s) = self.workbook.sheets.get_mut(sheet) {
            s.set_row_height(row, height);
        }
    }

    /// Empty a cell.
    pub fn clear(&mut self, sheet: usize, at: CellRef) -> Vec<Key> {
        self.set(sheet, at, Content::Value(Value::Empty))
    }

    /// Add a sheet at the end and recalculate: formulas that named it stop being `#REF!`. `None` when the name is
    /// taken (names compare without case).
    pub fn add_sheet(&mut self, name: &str) -> Option<usize> {
        if self.sheet_index(name).is_some() || name.is_empty() {
            return None;
        }
        self.workbook.sheets.push(Sheet::new(name));
        self.rebuild();
        Some(self.workbook.sheets.len() - 1)
    }

    /// Rename a sheet; formulas that named it are rewritten to the new name. `false` when the name is taken or the sheet is not there.
    pub fn rename_sheet(&mut self, sheet: usize, name: &str) -> bool {
        let taken = self.sheet_index(name).is_some_and(|i| i != sheet);
        if taken || name.is_empty() || sheet >= self.workbook.sheets.len() {
            return false;
        }
        let old = std::mem::replace(&mut self.workbook.sheets[sheet].name, name.to_string());
        // Formulas that named the sheet follow it to its new name.
        for s in &mut self.workbook.sheets {
            let updates: Vec<(CellRef, String)> = s
                .iter()
                .filter_map(|(at, c)| match c {
                    Content::Formula(f) => {
                        let mut e = parse(f).ok()?;
                        e.rename_sheet(&old, name)
                            .then(|| (at, sheets_formula::print(&e)))
                    }
                    Content::Value(_) => None,
                })
                .collect();
            for (at, text) in updates {
                s.set(at, Content::Formula(text));
            }
        }
        self.rebuild();
        true
    }

    /// Remove a sheet; formulas that read it become `#REF!`. Later sheets move down one index.
    pub fn remove_sheet(&mut self, sheet: usize) -> bool {
        if sheet >= self.workbook.sheets.len() {
            return false;
        }
        self.workbook.sheets.remove(sheet);
        self.rebuild();
        true
    }

    /// The rectangle a formula at `at` fills when it gives more than one value (its own cell included).
    #[must_use]
    pub fn spill_range(&self, sheet: usize, at: CellRef) -> Option<Range> {
        let sp = self.calc.get(&(sheet, at))?.spill.as_ref()?;
        Some(Range::new(
            at,
            CellRef::new(at.row + sp.rows() - 1, at.col + sp.cols - 1),
        ))
    }

    /// Whether the cell shows a value some other formula gave it.
    #[must_use]
    pub fn is_spilled(&self, sheet: usize, at: CellRef) -> bool {
        self.spilled.contains_key(&(sheet, at))
    }

    /// The cells of a sheet that show a value some other formula gave them, by row then column.
    #[must_use]
    pub fn spilled_cells(&self, sheet: usize) -> Vec<CellRef> {
        let mut cells: Vec<CellRef> = self
            .spilled
            .keys()
            .filter(|k| k.0 == sheet)
            .map(|k| k.1)
            .collect();
        cells.sort_unstable();
        cells
    }

    /// What a defined name stands for as a formula, with names inside it expanded (a few levels deep).
    #[must_use]
    pub fn name_expr(&self, name: &str) -> Option<Expr> {
        fn expand(e: &mut Expr, wb: &Workbook, depth: u32) {
            match e {
                Expr::Name(n) => {
                    *e = wb
                        .name(n)
                        .filter(|_| depth < 8)
                        .and_then(|t| parse(t).ok())
                        .map_or(Expr::Error(CellError::Name), |mut inner| {
                            expand(&mut inner, wb, depth + 1);
                            inner
                        });
                }
                Expr::Unary(_, a) | Expr::Percent(a) => expand(a, wb, depth),
                Expr::Binary(_, a, b) => {
                    expand(a, wb, depth);
                    expand(b, wb, depth);
                }
                Expr::Call(_, args) => args.iter_mut().for_each(|a| expand(a, wb, depth)),
                _ => {}
            }
        }
        let mut e = parse(self.workbook.name(name)?).ok()?;
        expand(&mut e, &self.workbook, 1);
        Some(e)
    }

    /// The value of a formula (written without the `=`) as if it were in cell `at` of a sheet; `Value::Error` when it does not
    /// parse. Nothing is stored.
    #[must_use]
    pub fn evaluate_text(&self, sheet: usize, at: CellRef, formula: &str) -> Value {
        parse(formula.trim_start_matches('=')).map_or(Value::Error(CellError::Name), |e| {
            self.evaluate_expr(sheet, at, &e)
        })
    }

    /// The value of a parsed formula as if it were in cell `at` of a sheet.
    #[must_use]
    pub fn evaluate_expr(&self, sheet: usize, at: CellRef, expr: &Expr) -> Value {
        eval(
            expr,
            &View {
                engine: self,
                sheet,
                at,
            },
        )
    }

    /// The values a list refers to (`Sheet1!$A$1:$A$9`, a defined name, or a formula giving one value), as validation lists
    /// use them.
    #[must_use]
    pub fn list_values(&self, sheet: usize, formula: &str) -> Vec<Value> {
        let Ok(mut expr) = parse(formula.trim_start_matches('=')) else {
            return Vec::new();
        };
        if let Expr::Name(n) = &expr {
            match self.name_expr(n) {
                Some(e) => expr = e,
                None => return Vec::new(),
            }
        }
        match &expr {
            Expr::Range(r) => {
                let target = r
                    .sheet
                    .as_deref()
                    .or(r.start.sheet.as_deref())
                    .map_or(Some(sheet), |n| self.sheet_index(n));
                let Some(target) = target else {
                    return Vec::new();
                };
                let range = Range::new(r.start.cell, r.end.cell);
                if u64::from(range.rows()) * u64::from(range.cols()) > 100_000 {
                    return Vec::new();
                }
                range.cells().map(|c| self.value(target, c)).collect()
            }
            other => vec![self.evaluate_expr(sheet, CellRef::new(0, 0), other)],
        }
    }

    /// Define, change or (with empty text) remove a defined name. Formulas that use it are calculated again. Returns
    /// whether the name is acceptable (letters, digits, `_` and `.`; not starting with a digit; not like a cell address).
    pub fn define_name(&mut self, name: &str, refers_to: &str) -> bool {
        let ok = name
            .chars()
            .next()
            .is_some_and(|c| c.is_alphabetic() || c == '_')
            && name
                .chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == '.')
            && CellRef::parse(name).is_none()
            && !name.eq_ignore_ascii_case("R")
            && !name.eq_ignore_ascii_case("C");
        if !ok && !refers_to.is_empty() {
            return false;
        }
        if !refers_to.is_empty() && parse(refers_to).is_err() {
            return false;
        }
        self.workbook.set_name(name, refers_to);
        self.rebuild();
        true
    }

    /// Make room for `count` rows or columns at `at` on a sheet; formulas anywhere that read what moved follow it.
    /// Returns whether the sheet exists.
    pub fn insert(&mut self, sheet: usize, axis: Axis, at: u32, count: u32) -> bool {
        self.structure(sheet, axis, at, i64::from(count))
    }

    /// Remove `count` rows or columns from `at`; formulas that read removed cells get `#REF!`, ranges shrink.
    pub fn delete(&mut self, sheet: usize, axis: Axis, at: u32, count: u32) -> bool {
        self.structure(sheet, axis, at, -i64::from(count))
    }

    fn structure(&mut self, sheet: usize, axis: Axis, at: u32, delta: i64) -> bool {
        let Some(target) = self.workbook.sheets.get(sheet).map(|s| s.name.clone()) else {
            return false;
        };
        if delta == 0 {
            return true;
        }
        self.workbook.sheets[sheet].shift(axis, at, delta);
        for s in &mut self.workbook.sheets {
            let own = s.name.clone();
            let updates: Vec<(CellRef, String)> = s
                .iter()
                .filter_map(|(cell, c)| match c {
                    Content::Formula(f) => {
                        let mut e = parse(f).ok()?;
                        e.shift_axis(&own, &target, axis, at, delta)
                            .then(|| (cell, sheets_formula::print(&e)))
                    }
                    Content::Value(_) => None,
                })
                .collect();
            for (cell, text) in updates {
                let cached = s.cached(cell).cloned();
                s.set(cell, Content::Formula(text));
                if let Some(v) = cached {
                    s.set_cached(cell, v);
                }
            }
        }
        self.rebuild();
        true
    }

    /// Change a sheet's settings that no formula reads (merged ranges, hidden rows, frozen panes, ...). `f` must not change
    /// cells: formulas are not calculated again afterwards.
    pub fn update_sheet<R>(&mut self, sheet: usize, f: impl FnOnce(&mut Sheet) -> R) -> Option<R> {
        self.workbook.sheets.get_mut(sheet).map(f)
    }

    /// Put a whole workbook in place of the current one (an undo of a big change), keeping the clock.
    pub fn restore(&mut self, workbook: Workbook) {
        self.workbook = workbook;
        self.rebuild();
    }

    /// Where the time of day comes from: a function giving the local date and time as a serial number. Without one `NOW()` is 0.
    pub fn set_clock(&mut self, now: impl Fn() -> f64 + 'static) {
        self.clock = Some(Clock(Rc::new(now)));
    }

    /// Calculate the cells that depend on the clock or on chance again. Returns those whose value changed.
    pub fn recalc_volatile(&mut self) -> Vec<Key> {
        let seeds: Vec<Key> = self.volatile.iter().copied().collect();
        let mut changed = self.propagate(&seeds, &HashMap::new());
        changed.sort_unstable();
        changed
    }

    /// A number from 0 up to 1: xorshift, seeded afresh from the clock when there is one.
    fn next_random(&self) -> f64 {
        let mut x = self.seed.get();
        if let Some(c) = &self.clock {
            x ^= (c.0)().to_bits();
        }
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.seed.set(x);
        #[allow(clippy::cast_precision_loss)]
        {
            (x >> 11) as f64 / (1u64 << 53) as f64
        }
    }

    /// Calculate everything again.
    pub fn recalc_all(&mut self) {
        self.rebuild();
    }

    fn rebuild(&mut self) {
        self.calc.clear();
        self.by_cell.clear();
        self.by_range.clear();
        self.volatile.clear();
        self.spilled.clear();
        self.blocked.clear();
        self.touched.clear();
        let mut formulas = Vec::new();
        for (i, s) in self.workbook.sheets.iter().enumerate() {
            for (at, c) in s.iter() {
                if let Content::Formula(t) = c {
                    formulas.push(((i, at), t.clone()));
                }
            }
        }
        let keys: Vec<Key> = formulas.iter().map(|(k, _)| *k).collect();
        for (k, text) in formulas {
            self.install(k, &text);
        }
        self.propagate(&keys, &HashMap::new());
    }

    /// Parse a formula and record what it reads; its value is filled in by `propagate`.
    fn install(&mut self, key: Key, text: &str) {
        let expr = parse(text);
        let (mut cells, mut ranges) = (Vec::new(), Vec::new());
        if let Ok(e) = &expr {
            self.collect(key.0, e, &mut cells, &mut ranges);
            if e.is_volatile() {
                self.volatile.insert(key);
            }
        }
        for c in &cells {
            self.by_cell.entry(*c).or_default().insert(key);
        }
        for (s, r) in &ranges {
            self.by_range
                .entry(*s)
                .or_default()
                .entry(*r)
                .or_default()
                .insert(key);
        }
        self.calc.insert(
            key,
            Calc {
                spill: None,
                expr,
                value: Value::Empty,
                cells,
                ranges,
            },
        );
    }

    fn unregister(&mut self, key: Key) {
        self.volatile.remove(&key);
        let Some(c) = self.calc.get(&key) else { return };
        for cell in &c.cells {
            if let Some(set) = self.by_cell.get_mut(cell) {
                set.remove(&key);
                if set.is_empty() {
                    self.by_cell.remove(cell);
                }
            }
        }
        for (s, r) in &c.ranges {
            if let Some(sheet) = self.by_range.get_mut(s)
                && let Some(readers) = sheet.get_mut(r)
            {
                readers.remove(&key);
                if readers.is_empty() {
                    sheet.remove(r);
                }
            }
        }
    }

    fn collect(
        &self,
        own: usize,
        e: &Expr,
        cells: &mut Vec<Key>,
        ranges: &mut Vec<(usize, Range)>,
    ) {
        let sheet_of = |name: Option<&String>| name.map_or(Some(own), |n| self.sheet_index(n));
        match e {
            Expr::Ref(r) => {
                if let Some(s) = sheet_of(r.sheet.as_ref()) {
                    cells.push((s, r.cell));
                }
            }
            Expr::Range(r) => {
                if let Some(s) = sheet_of(r.sheet.as_ref()) {
                    let range = Range::new(r.start.cell, r.end.cell);
                    if u64::from(range.rows()) * u64::from(range.cols()) <= SMALL_RANGE {
                        cells.extend(range.cells().map(|c| (s, c)));
                    } else {
                        ranges.push((s, range));
                    }
                }
            }
            Expr::Unary(_, a) | Expr::Percent(a) => self.collect(own, a, cells, ranges),
            Expr::Binary(_, a, b) => {
                self.collect(own, a, cells, ranges);
                self.collect(own, b, cells, ranges);
            }
            Expr::Call(_, args) => {
                for a in args {
                    self.collect(own, a, cells, ranges);
                }
            }
            Expr::Name(n) => {
                if let Some(inner) = self.name_expr(n) {
                    self.collect(own, &inner, cells, ranges);
                }
            }
            Expr::Number(_) | Expr::Text(_) | Expr::Bool(_) | Expr::Error(_) => {}
        }
    }

    /// The formula cells that read this cell, directly or through a range.
    fn dependents(&self, key: Key) -> HashSet<Key> {
        let mut out = self.cell_readers(key);
        // What reads a cell a formula spread its result over reads that formula.
        if let Some(sp) = self.calc.get(&key).and_then(|c| c.spill.as_ref()) {
            for r in 0..sp.rows() {
                for c in 0..sp.cols {
                    if (r, c) != (0, 0) {
                        out.extend(
                            self.cell_readers((key.0, CellRef::new(key.1.row + r, key.1.col + c))),
                        );
                    }
                }
            }
        }
        out
    }

    fn cell_readers(&self, key: Key) -> HashSet<Key> {
        let mut out = self.by_cell.get(&key).cloned().unwrap_or_default();
        if let Some(sheet) = self.by_range.get(&key.0) {
            for (range, readers) in sheet {
                if range.contains(key.1) {
                    out.extend(readers.iter().copied());
                }
            }
        }
        out
    }

    /// Recalculate the formulas among `seeds` and everything that reads them, in dependency order. `before` holds the
    /// old values of cells whose change is reported even if they are not formulas.
    fn propagate(&mut self, seeds: &[Key], before: &HashMap<Key, Value>) -> Vec<Key> {
        let mut changed = self.propagate_once(seeds, before);
        // Cells a spill began or stopped filling show something new: what reads them is calculated again (a few rounds at most:
        // a formula that reads its own spill cannot settle).
        for _ in 0..4 {
            let mut cells: Vec<Key> = std::mem::take(&mut self.touched)
                .into_iter()
                .filter(|k| !self.calc.contains_key(k))
                .collect();
            cells.sort_unstable();
            cells.dedup();
            if cells.is_empty() {
                break;
            }
            changed.extend(cells.iter().copied());
            changed.extend(self.propagate_once(&cells, &HashMap::new()));
        }
        self.touched.clear();
        changed
    }

    fn propagate_once(&mut self, seeds: &[Key], before: &HashMap<Key, Value>) -> Vec<Key> {
        // 1. Everything that may change: the seeds' dependents, transitively, plus the seeds that are formulas.
        let mut dirty: HashSet<Key> = seeds
            .iter()
            .copied()
            .filter(|k| self.calc.contains_key(k))
            .collect();
        let mut queue: VecDeque<Key> = seeds.iter().copied().collect();
        while let Some(k) = queue.pop_front() {
            for d in self.dependents(k) {
                if dirty.insert(d) {
                    queue.push_back(d);
                }
            }
        }
        let old: HashMap<Key, Value> = dirty
            .iter()
            .map(|k| (*k, self.calc[k].value.clone()))
            .collect();
        // 2. Edges between dirty cells, then Kahn's algorithm.
        let mut edges: HashMap<Key, Vec<Key>> = HashMap::new();
        let mut indegree: HashMap<Key, usize> = dirty.iter().map(|k| (*k, 0)).collect();
        for p in &dirty {
            for d in self.dependents(*p) {
                if let Some(n) = indegree.get_mut(&d) {
                    *n += 1;
                    edges.entry(*p).or_default().push(d);
                }
            }
        }
        let mut ready: Vec<Key> = indegree
            .iter()
            .filter(|(_, n)| **n == 0)
            .map(|(k, _)| *k)
            .collect();
        let mut done = 0;
        while let Some(k) = ready.pop() {
            done += 1;
            self.evaluate(k);
            for d in edges.get(&k).into_iter().flatten() {
                let n = indegree.get_mut(d).expect("edges only lead to dirty cells");
                *n -= 1;
                if *n == 0 {
                    ready.push(*d);
                }
            }
        }
        if done < dirty.len() {
            for (k, n) in &indegree {
                if *n > 0
                    && let Some(c) = self.calc.get_mut(k)
                {
                    c.value = Value::Error(CellError::Circular);
                }
            }
        }
        // 3. What is different now.
        let mut changed: Vec<Key> = old
            .iter()
            .filter(|(k, v)| self.calc[k].value != **v)
            .map(|(k, _)| *k)
            .collect();
        for (k, v) in before {
            if !old.contains_key(k) && self.value(k.0, k.1) != *v {
                changed.push(*k);
            }
        }
        changed
    }

    /// Whether something stands in the way of a result `rows` by `cols` starting at `key`.
    fn spill_blocked(&self, key: Key, rows: u32, cols: u32) -> bool {
        let Some(sheet) = self.workbook.sheets.get(key.0) else {
            return true;
        };
        if u64::from(key.1.row) + u64::from(rows) > u64::from(sheets_model::MAX_ROWS)
            || u64::from(key.1.col) + u64::from(cols) > u64::from(sheets_model::MAX_COLS)
        {
            return true;
        }
        for r in 0..rows {
            for c in 0..cols {
                if (r, c) == (0, 0) {
                    continue;
                }
                let at = CellRef::new(key.1.row + r, key.1.col + c);
                if sheet.get(at).is_some()
                    || self
                        .spilled
                        .get(&(key.0, at))
                        .is_some_and(|owner| *owner != key)
                {
                    return true;
                }
            }
        }
        false
    }

    /// Forget the cells a formula spread its result over.
    fn clear_spill(&mut self, key: Key) {
        self.blocked.remove(&key);
        let Some(sp) = self.calc.get_mut(&key).and_then(|c| c.spill.take()) else {
            return;
        };
        for r in 0..sp.rows() {
            for c in 0..sp.cols {
                if (r, c) != (0, 0) {
                    let cell = (key.0, CellRef::new(key.1.row + r, key.1.col + c));
                    if self.spilled.get(&cell) == Some(&key) {
                        self.spilled.remove(&cell);
                    }
                    self.touched.push(cell);
                }
            }
        }
    }

    fn evaluate(&mut self, key: Key) {
        self.clear_spill(key);
        let Some(mut c) = self.calc.remove(&key) else {
            return;
        };
        let operand = c.expr.as_ref().ok().map(|e| {
            eval_operand(
                e,
                &View {
                    engine: self,
                    sheet: key.0,
                    at: key.1,
                },
            )
        });
        c.value = match operand {
            None => Value::Error(CellError::Name),
            Some(Operand::Scalar(v)) => v,
            Some(Operand::Array(a)) if a.data.len() <= 1 => {
                a.data.into_iter().next().unwrap_or_default()
            }
            Some(Operand::Array(a)) => {
                let cols = u32::try_from(a.cols).unwrap_or(1).max(1);
                let rows = u32::try_from(a.data.len() / a.cols.max(1)).unwrap_or(0);
                let first = a.data[0].clone();
                if self.spill_blocked(key, rows, cols) {
                    self.blocked.insert(key);
                    Value::Error(CellError::Spill)
                } else {
                    for r in 0..rows {
                        for cc in 0..cols {
                            if (r, cc) != (0, 0) {
                                let cell = (key.0, CellRef::new(key.1.row + r, key.1.col + cc));
                                self.spilled.insert(cell, key);
                                self.touched.push(cell);
                            }
                        }
                    }
                    c.spill = Some(Spill {
                        cols,
                        values: a.data,
                    });
                    first
                }
            }
        };
        // A formula this suite cannot calculate (an unknown function) shows what the file said it was.
        if c.value == Value::Error(CellError::Name)
            && let Some(cached) = self
                .workbook
                .sheets
                .get(key.0)
                .and_then(|s| s.cached(key.1))
        {
            c.value = cached.clone();
        }

        self.calc.insert(key, c);
    }
}
