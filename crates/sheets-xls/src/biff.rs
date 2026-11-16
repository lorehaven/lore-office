//! BIFF8 records: the workbook stream of an `.xls` file.

use crate::{Error, ole};
use sheets_model::{Align, CellRef, Content, Range, Sheet, Style, Value, Workbook};
use std::collections::HashMap;

fn bad(m: &str) -> Error {
    Error::Invalid(m.to_string())
}

fn u16_at(b: &[u8], i: usize) -> usize {
    b.get(i..i + 2)
        .map_or(0, |s| usize::from(u16::from_le_bytes([s[0], s[1]])))
}

fn f64_at(b: &[u8], i: usize) -> f64 {
    b.get(i..i + 8).map_or(0.0, |s| {
        f64::from_le_bytes([s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7]])
    })
}

/// A number stored as an RK value.
fn rk(raw: u32) -> f64 {
    let mut v = if raw & 2 != 0 {
        f64::from((raw as i32) >> 2)
    } else {
        f64::from_bits(u64::from(raw & 0xFFFF_FFFC) << 32)
    };
    if raw & 1 != 0 {
        v /= 100.0;
    }
    v
}

/// Strings are cut into pieces by `CONTINUE` records; a piece that starts inside a string begins with a new flag byte.
struct Chunks<'a> {
    parts: Vec<&'a [u8]>,
    part: usize,
    at: usize,
}

impl Chunks<'_> {
    fn byte(&mut self) -> Option<u8> {
        while self.at >= self.parts.get(self.part)?.len() {
            self.part += 1;
            self.at = 0;
        }
        let b = self.parts[self.part][self.at];
        self.at += 1;
        Some(b)
    }

    fn u16(&mut self) -> Option<usize> {
        Some(usize::from(self.byte()?) | usize::from(self.byte()?) << 8)
    }

    fn u32(&mut self) -> Option<usize> {
        Some(self.u16()? | self.u16()? << 16)
    }

    fn skip(&mut self, n: usize) {
        for _ in 0..n {
            if self.byte().is_none() {
                break;
            }
        }
    }

    fn string(&mut self) -> Option<String> {
        let chars = self.u16()?;
        let flags = self.byte()?;
        let (runs, ext) = (
            if flags & 8 != 0 { self.u16()? } else { 0 },
            if flags & 4 != 0 { self.u32()? } else { 0 },
        );
        let mut wide = flags & 1 != 0;
        let mut units: Vec<u16> = Vec::with_capacity(chars.min(65_536));
        while units.len() < chars {
            // At a piece boundary the flag byte is repeated.
            if self.at >= self.parts.get(self.part)?.len() {
                self.part += 1;
                self.at = 0;
                if self.part >= self.parts.len() {
                    return None;
                }
                wide = self.byte()? & 1 != 0;
            }
            units.push(if wide {
                self.u16()? as u16
            } else {
                u16::from(self.byte()?)
            });
        }
        self.skip(runs * 4 + ext);
        Some(
            char::decode_utf16(units)
                .map(|c| c.unwrap_or('\u{FFFD}'))
                .collect(),
        )
    }
}

fn builtin(id: usize) -> Option<&'static str> {
    Some(match id {
        0 => "General",
        1 => "0",
        2 => "0.00",
        3 => "#,##0",
        4 => "#,##0.00",
        9 => "0%",
        10 => "0.00%",
        11 => "0.00E+00",
        14 => "yyyy-mm-dd",
        15 => "d-mmm-yy",
        16 => "d-mmm",
        17 => "mmm-yy",
        18 => "h:mm AM/PM",
        19 => "h:mm:ss AM/PM",
        20 => "h:mm",
        21 => "h:mm:ss",
        22 => "yyyy-mm-dd h:mm",
        37 => "#,##0;(#,##0)",
        38 => "#,##0;(#,##0)",
        39 => "#,##0.00;(#,##0.00)",
        40 => "#,##0.00;(#,##0.00)",
        45 => "mm:ss",
        46 => "[h]:mm:ss",
        47 => "mm:ss.0",
        49 => "@",
        _ => return None,
    })
}

struct Record<'a> {
    id: usize,
    parts: Vec<&'a [u8]>,
}

impl<'a> Record<'a> {
    fn data(&self) -> &'a [u8] {
        self.parts[0]
    }
}

/// The records of a stream, with the `CONTINUE`s of each gathered in.
fn records(stream: &[u8]) -> Vec<Record<'_>> {
    let mut out: Vec<Record<'_>> = Vec::new();
    let mut i = 0;
    while i + 4 <= stream.len() {
        let (id, len) = (u16_at(stream, i), u16_at(stream, i + 2));
        let Some(data) = stream.get(i + 4..i + 4 + len) else {
            break;
        };
        if id == 0x003C {
            if let Some(last) = out.last_mut() {
                last.parts.push(data);
            }
        } else {
            out.push(Record {
                id,
                parts: vec![data],
            });
        }
        i += 4 + len;
    }
    out
}

/// Read an `.xls` file. Formulas are kept as the values they came to.
///
/// # Errors
///
/// [`Error::Invalid`] when the file is not a readable BIFF8 workbook (or is password protected).
pub fn read(bytes: &[u8]) -> Result<Workbook, Error> {
    let stream = ole::stream(bytes, &["Workbook", "Book"])?;
    let records = records(&stream);
    if records.iter().any(|r| r.id == 0x002F) {
        return Err(bad("the file is protected with a password"));
    }
    let mut strings: Vec<String> = Vec::new();
    let mut formats: HashMap<usize, String> = HashMap::new();
    let mut xfs: Vec<(usize, usize)> = Vec::new(); // (font, format)
    let mut bold_fonts: Vec<bool> = Vec::new();
    let mut names: Vec<String> = Vec::new();
    let mut date1904 = false;
    for r in &records {
        let d = r.data();
        match r.id {
            0x0022 => date1904 = u16_at(d, 0) == 1,
            0x0085 => {
                let (n, flags) = (
                    usize::from(*d.get(6).unwrap_or(&0)),
                    *d.get(7).unwrap_or(&0),
                );
                let text = d.get(8..).unwrap_or(&[]);
                names.push(if flags & 1 != 0 {
                    char::decode_utf16(
                        text.chunks_exact(2)
                            .take(n)
                            .map(|c| u16::from_le_bytes([c[0], c[1]])),
                    )
                    .map(|c| c.unwrap_or('?'))
                    .collect()
                } else {
                    text.iter().take(n).map(|b| char::from(*b)).collect()
                });
            }
            0x00FC => {
                let mut c = Chunks {
                    parts: r.parts.clone(),
                    part: 0,
                    at: 8,
                };
                let count = u16_at(d, 4) | u16_at(d, 6) << 16;
                for _ in 0..count {
                    match c.string() {
                        Some(s) => strings.push(s),
                        None => break,
                    }
                }
            }
            0x041E => {
                let mut c = Chunks {
                    parts: r.parts.clone(),
                    part: 0,
                    at: 2,
                };
                if let Some(s) = c.string() {
                    formats.insert(u16_at(d, 0), s);
                }
            }
            0x0031 => bold_fonts.push(u16_at(d, 6) >= 700),
            0x00E0 => xfs.push((u16_at(d, 0), u16_at(d, 2))),
            _ => {}
        }
    }
    let mut wb = Workbook {
        sheets: Vec::new(),
        styles: vec![Style::default()],
        names: Vec::new(),
    };
    // The style of an XF index, made once.
    let mut made: HashMap<usize, u32> = HashMap::new();
    let mut style_of = |wb: &mut Workbook, xf: usize| -> u32 {
        if let Some(id) = made.get(&xf) {
            return *id;
        }
        let (font, fmt) = xfs.get(xf).copied().unwrap_or((0, 0));
        // Font index 4 does not exist in the file: it is skipped.
        let font = if font > 4 { font - 1 } else { font };
        let format = formats
            .get(&fmt)
            .cloned()
            .or_else(|| builtin(fmt).map(str::to_string))
            .unwrap_or_else(|| "General".into());
        let style = Style {
            bold: bold_fonts.get(font).copied().unwrap_or(false) && xf > 15,
            number_format: format,
            align: Align::General,
            ..Style::default()
        };
        let id = wb.style_id(&style);
        made.insert(xf, id);
        id
    };
    let is_date = |wb: &Workbook, id: u32| {
        let f = wb.styles[id as usize].number_format.to_lowercase();
        f.contains('y') || f.contains('d') && !f.contains('0') || f.contains('h') && f.contains(':')
    };
    let mut starts: Vec<usize> = Vec::new();
    let mut depth = 0;

    let mut pending_formula: Option<(CellRef, u32)> = None;
    let _ = (&mut depth, &starts);
    let mut order: Vec<usize> = Vec::new();
    for (i, r) in records.iter().enumerate() {
        if r.id == 0x0809 && u16_at(r.data(), 2) == 0x0010 {
            order.push(i);
        }
    }
    starts.extend(order);
    if starts.is_empty() {
        return Err(bad("no worksheets"));
    }
    for (si, name) in names.iter().enumerate() {
        wb.sheets.push(Sheet::new(name));
        let _ = si;
    }
    let serial = |v: f64| if date1904 { v + 1462.0 } else { v };
    let mut sheet_no = 0usize;
    for (k, start) in starts.iter().enumerate() {
        let end = starts.get(k + 1).copied().unwrap_or(records.len());
        if wb.sheets.get(sheet_no).is_none() {
            break;
        }
        let current = Some(sheet_no);
        sheet_no += 1;
        let si = current.unwrap_or(0);
        let mut put =
            |wb: &mut Workbook, row: usize, col: usize, xf: usize, value: Option<Value>| {
                let at = CellRef::new(row as u32, col as u32);
                let id = style_of(wb, xf);
                let sheet = &mut wb.sheets[si];
                if let Some(v) = value {
                    sheet.set(at, Content::Value(v));
                }
                if id != 0 {
                    sheet.set_style(at, id);
                }
            };
        for r in &records[*start..end] {
            let d = r.data();
            match r.id {
                0x0203 => {
                    let v = serial(f64_at(d, 6));
                    put(
                        &mut wb,
                        u16_at(d, 0),
                        u16_at(d, 2),
                        u16_at(d, 4),
                        Some(Value::Number(v)),
                    );
                }
                0x027E => {
                    let raw = u16_at(d, 6) as u32 | (u16_at(d, 8) as u32) << 16;
                    put(
                        &mut wb,
                        u16_at(d, 0),
                        u16_at(d, 2),
                        u16_at(d, 4),
                        Some(Value::Number(serial(rk(raw)))),
                    );
                }
                0x00BD => {
                    let (row, first) = (u16_at(d, 0), u16_at(d, 2));
                    let last = u16_at(d, d.len().saturating_sub(2));
                    for (n, col) in (first..=last).enumerate() {
                        let at = 4 + n * 6;
                        let raw = u16_at(d, at + 2) as u32 | (u16_at(d, at + 4) as u32) << 16;
                        put(
                            &mut wb,
                            row,
                            col,
                            u16_at(d, at),
                            Some(Value::Number(serial(rk(raw)))),
                        );
                    }
                }
                0x00FD => {
                    let text = strings
                        .get(u16_at(d, 6) | u16_at(d, 8) << 16)
                        .cloned()
                        .unwrap_or_default();
                    put(
                        &mut wb,
                        u16_at(d, 0),
                        u16_at(d, 2),
                        u16_at(d, 4),
                        Some(Value::Text(text)),
                    );
                }
                0x0204 => {
                    let n = u16_at(d, 6);
                    let wide = d.get(8) == Some(&1);
                    let body = d.get(9..).unwrap_or(&[]);
                    let text: String = if wide {
                        char::decode_utf16(
                            body.chunks_exact(2)
                                .take(n)
                                .map(|c| u16::from_le_bytes([c[0], c[1]])),
                        )
                        .map(|c| c.unwrap_or('?'))
                        .collect()
                    } else {
                        body.iter().take(n).map(|b| char::from(*b)).collect()
                    };
                    put(
                        &mut wb,
                        u16_at(d, 0),
                        u16_at(d, 2),
                        u16_at(d, 4),
                        Some(Value::Text(text)),
                    );
                }
                0x0205 => {
                    let v = if d.get(7) == Some(&0) {
                        Value::Bool(d.get(6) == Some(&1))
                    } else {
                        Value::Text("#VALUE!".into())
                    };
                    put(&mut wb, u16_at(d, 0), u16_at(d, 2), u16_at(d, 4), Some(v));
                }
                0x0006 => {
                    let (row, col, xf) = (u16_at(d, 0), u16_at(d, 2), u16_at(d, 4));
                    let result = d.get(6..14).unwrap_or(&[0; 8]);
                    if result[6] == 0xFF && result[7] == 0xFF {
                        match result[0] {
                            0 => {
                                pending_formula =
                                    Some((CellRef::new(row as u32, col as u32), xf as u32))
                            }
                            1 => put(&mut wb, row, col, xf, Some(Value::Bool(result[2] == 1))),
                            _ => put(&mut wb, row, col, xf, None),
                        }
                    } else {
                        put(
                            &mut wb,
                            row,
                            col,
                            xf,
                            Some(Value::Number(serial(f64_at(d, 6)))),
                        );
                    }
                }
                0x0207 => {
                    if let Some((at, xf)) = pending_formula.take() {
                        let mut c = Chunks {
                            parts: r.parts.clone(),
                            part: 0,
                            at: 0,
                        };
                        let text = c.string().unwrap_or_default();
                        put(
                            &mut wb,
                            at.row as usize,
                            at.col as usize,
                            xf as usize,
                            Some(Value::Text(text)),
                        );
                    }
                }
                0x00E5 => {
                    let n = u16_at(d, 0);
                    for m in 0..n {
                        let o = 2 + m * 8;
                        let (r1, r2, c1, c2) = (
                            u16_at(d, o),
                            u16_at(d, o + 2),
                            u16_at(d, o + 4),
                            u16_at(d, o + 6),
                        );
                        wb.sheets[si].merge(Range::new(
                            CellRef::new(r1 as u32, c1 as u32),
                            CellRef::new(r2 as u32, c2 as u32),
                        ));
                    }
                }
                0x007D => {
                    let (first, last, width) = (u16_at(d, 0), u16_at(d, 2).min(255), u16_at(d, 4));
                    // Widths are in 1/256 of a character; a character is about 7 pixels.
                    let px = width as f64 / 256.0 * 7.0 + 5.0;
                    for col in first..=last {
                        if u16_at(d, 8) & 1 != 0 {
                            wb.sheets[si].set_col_hidden(col as u32, true);
                        } else {
                            wb.sheets[si].set_col_width(col as u32, Some(px));
                        }
                    }
                }
                _ => {}
            }
        }
    }
    // A number in a date format is a date already: nothing to convert, the format says how it shows.
    let _ = is_date;
    if wb.sheets.is_empty() {
        return Err(bad("no sheets"));
    }
    Ok(wb)
}
