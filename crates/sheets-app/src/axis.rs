//! Sizes along one direction of the grid: every column (or row) has the default size except a few that were resized.
//! Positions are sums of sizes, so they are found with a prefix sum over the exceptions instead of a table of every column.

/// A default size and the items that differ from it.
#[derive(Clone, Debug, PartialEq)]
pub struct Axis {
    default: f64,
    count: u32,
    /// `(index, size)` of the items that are not the default, by index.
    exceptions: Vec<(u32, f64)>,
    /// `extra[k]` is how much longer than default sizes the first `k` exceptions make everything before them.
    extra: Vec<f64>,
}

impl Axis {
    /// `count` items of `default` pixels, except those in `exceptions` (index and size; later entries win; indexes past
    /// `count` are ignored).
    #[must_use]
    pub fn new(default: f64, count: u32, exceptions: impl IntoIterator<Item = (u32, f64)>) -> Self {
        let mut list: Vec<(u32, f64)> = exceptions
            .into_iter()
            .filter(|(i, s)| *i < count && s.is_finite() && *s >= 0.0)
            .collect();
        list.sort_by_key(|(i, _)| *i);
        list.dedup_by(|later, earlier| {
            let same = later.0 == earlier.0;
            if same {
                *earlier = *later;
            }
            same
        });
        let mut extra = Vec::with_capacity(list.len() + 1);
        let mut sum = 0.0;
        extra.push(sum);
        for (_, size) in &list {
            sum += size - default;
            extra.push(sum);
        }
        Self {
            default,
            count,
            exceptions: list,
            extra,
        }
    }

    #[must_use]
    pub const fn count(&self) -> u32 {
        self.count
    }

    /// The size of item `i`.
    #[must_use]
    pub fn size(&self, i: u32) -> f64 {
        self.exceptions
            .binary_search_by_key(&i, |(k, _)| *k)
            .map_or(self.default, |at| self.exceptions[at].1)
    }

    /// Where item `i` starts (item `count` is where the last one ends).
    #[must_use]
    pub fn offset(&self, i: u32) -> f64 {
        let before = self.exceptions.partition_point(|(k, _)| *k < i);
        f64::from(i).mul_add(self.default, self.extra[before])
    }

    /// The length of the whole axis.
    #[must_use]
    pub fn total(&self) -> f64 {
        self.offset(self.count)
    }

    /// The item that covers position `pos`; the first for a negative position, the last past the end.
    #[must_use]
    pub fn index_at(&self, pos: f64) -> u32 {
        if pos <= 0.0 || self.count == 0 {
            return 0;
        }
        // The last item that starts at or before `pos`.
        let (mut lo, mut hi) = (0, self.count - 1);
        while lo < hi {
            let mid = lo + (hi - lo).div_ceil(2);
            if self.offset(mid) <= pos {
                lo = mid;
            } else {
                hi = mid - 1;
            }
        }
        lo
    }
}

/// Both axes of the sheet on screen.
#[derive(Clone, Debug, PartialEq)]
pub struct Axes {
    pub cols: Axis,
    pub rows: Axis,
    /// How many rows and columns are frozen (kept in view when scrolling), as (rows, columns).
    pub frozen: (u32, u32),
}
