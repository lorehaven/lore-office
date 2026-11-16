//! The OLE2 compound file `.xls` lives in: sectors chained through a table, with small streams in a mini stream.

use crate::Error;

const SIGNATURE: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
const END: u32 = 0xFFFF_FFFE;
const FREE: u32 = 0xFFFF_FFFF;

fn u16_at(b: &[u8], i: usize) -> usize {
    b.get(i..i + 2)
        .map_or(0, |s| usize::from(u16::from_le_bytes([s[0], s[1]])))
}

fn u32_at(b: &[u8], i: usize) -> u32 {
    b.get(i..i + 4)
        .map_or(FREE, |s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

fn bad(m: &str) -> Error {
    Error::Invalid(m.to_string())
}

/// The bytes of the stream called `name` (compared without regard to case).
pub fn stream(file: &[u8], names: &[&str]) -> Result<Vec<u8>, Error> {
    if file.len() < 512 || file[..8] != SIGNATURE {
        return Err(bad("not an OLE2 file"));
    }
    let sector = 1usize << u16_at(file, 0x1E).clamp(7, 16);
    let mini = 1usize << u16_at(file, 0x20).clamp(4, 12);
    let sector_at = |n: u32| -> Option<&[u8]> {
        let start = (n as usize).checked_add(1)?.checked_mul(sector)?;
        file.get(start..start + sector)
    };
    // The table of sectors: which comes after which.
    let mut fat_sectors: Vec<u32> = (0..109)
        .map(|i| u32_at(file, 0x4C + i * 4))
        .filter(|s| *s < END)
        .collect();
    let mut difat = u32_at(file, 0x44);
    let mut guard = 0;
    while difat < END && guard < 10_000 {
        let Some(s) = sector_at(difat) else { break };
        for i in 0..sector / 4 - 1 {
            let v = u32_at(s, i * 4);
            if v < END {
                fat_sectors.push(v);
            }
        }
        difat = u32_at(s, sector - 4);
        guard += 1;
    }
    let mut fat: Vec<u32> = Vec::new();
    for s in fat_sectors {
        let data = sector_at(s).ok_or_else(|| bad("a table sector is missing"))?;
        fat.extend((0..sector / 4).map(|i| u32_at(data, i * 4)));
    }
    let chain = |start: u32, table: &[u32]| -> Result<Vec<u32>, Error> {
        let mut out = Vec::new();
        let mut at = start;
        while at < END {
            if out.len() > table.len() {
                return Err(bad("a chain of sectors loops"));
            }
            out.push(at);
            at = *table
                .get(at as usize)
                .ok_or_else(|| bad("a chain leaves the table"))?;
        }
        Ok(out)
    };
    let read_chain = |start: u32| -> Result<Vec<u8>, Error> {
        let mut data = Vec::new();
        for s in chain(start, &fat)? {
            data.extend_from_slice(sector_at(s).ok_or_else(|| bad("a sector is missing"))?);
        }
        Ok(data)
    };
    // The directory: entries of 128 bytes.
    let dir = read_chain(u32_at(file, 0x30))?;
    let mut root = (FREE, 0usize);
    let mut found: Option<(u32, usize)> = None;
    for e in dir.chunks_exact(128) {
        let len = u16_at(e, 0x40).saturating_sub(2).min(64);
        let name: String = char::decode_utf16(
            e[..len]
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]])),
        )
        .map(|c| c.unwrap_or('?'))
        .collect();
        let kind = e[0x42];
        let (start, size) = (u32_at(e, 0x74), u32_at(e, 0x78) as usize);
        if kind == 5 {
            root = (start, size);
        } else if kind == 2
            && found.is_none()
            && names.iter().any(|n| n.eq_ignore_ascii_case(&name))
        {
            found = Some((start, size));
        }
    }
    let (start, size) = found.ok_or_else(|| bad("there is no workbook in the file"))?;
    let cutoff = u32_at(file, 0x38) as usize;
    if size >= cutoff.max(1) {
        let mut data = read_chain(start)?;
        data.truncate(size);
        return Ok(data);
    }
    // A small stream lives in the mini stream: pieces of 64 bytes, chained through the mini table.
    let container = {
        let mut d = read_chain(root.0)?;
        d.truncate(root.1);
        d
    };
    let mut mini_fat: Vec<u32> = Vec::new();
    for s in chain(u32_at(file, 0x3C), &fat)? {
        let data = sector_at(s).ok_or_else(|| bad("a mini table sector is missing"))?;
        mini_fat.extend((0..sector / 4).map(|i| u32_at(data, i * 4)));
    }
    let mut data = Vec::new();
    for s in chain(start, &mini_fat)? {
        let from = s as usize * mini;
        data.extend_from_slice(
            container
                .get(from..from + mini)
                .ok_or_else(|| bad("a small sector is missing"))?,
        );
    }
    data.truncate(size);
    Ok(data)
}
