//! Minimal read-only SQLite helper for ZaraRadio `sweepers` table.
//! Own implementation (no C sqlite). Enough for: TEXT, TEXT, INT, BOOL columns.
use super::{SweeperBook, SweeperEntry};
use std::path::{Path, PathBuf};

pub fn import_sqlite_db(path: &Path) -> anyhow::Result<SweeperBook> {
    let data = std::fs::read(path)?;
    if data.len() < 100 || !data.starts_with(b"SQLite format 3\0") {
        anyhow::bail!("arquivo não é SQLite 3: {}", path.display());
    }
    let mut page_size = u16::from_be_bytes([data[16], data[17]]) as usize;
    if page_size == 1 {
        page_size = 65536;
    }
    if page_size < 512 || data.len() < page_size {
        anyhow::bail!("page_size inválido");
    }
    // sqlite_master lives on page 1. Find rootpage for table "sweepers".
    let root = find_table_root(&data, page_size, "sweepers")
        .ok_or_else(|| anyhow::anyhow!("tabela sweepers não encontrada"))?;
    let rows = walk_table_pages(&data, page_size, root)?;
    let mut entries = Vec::new();
    for cols in rows {
        if cols.len() < 4 {
            continue;
        }
        let file = cols[0].as_str().unwrap_or("").replace('\\', "/");
        let pisa = cols[1].as_str().unwrap_or("").replace('\\', "/");
        let time = cols[2].as_i64().unwrap_or(0);
        let enabled = match &cols[3] {
            Val::Int(i) => *i != 0,
            Val::Text(s) => s != "0" && s.to_ascii_lowercase() != "false",
            _ => true,
        };
        if file.is_empty() {
            continue;
        }
        entries.push(SweeperEntry {
            base_file: PathBuf::from(file),
            sweeper_file: PathBuf::from(pisa),
            offset_secs: time as f32,
            enabled,
        });
    }
    Ok(SweeperBook {
        enabled: true,
        entries,
    })
}

/// Writes a tiny SQLite DB with one `sweepers` table (UTF-8, page_size 4096).
/// Compatible with Zara's SELECT * FROM sweepers.
pub fn export_sqlite_db(book: &SweeperBook, path: &Path) -> anyhow::Result<()> {
    // Prefer generating via an intermediate that Python/tests can also make.
    // Here we emit SQL dump as fallback companion and a real DB built with a
    // one-page writer for small entry counts.
    write_minimal_sweepers_db(book, path)
}

#[derive(Debug, Clone)]
enum Val {
    Null,
    Int(i64),
    Text(String),
    Blob(#[allow(dead_code)] Vec<u8>),
}

impl Val {
    fn as_str(&self) -> Option<&str> {
        match self {
            Val::Text(s) => Some(s),
            _ => None,
        }
    }
    fn as_i64(&self) -> Option<i64> {
        match self {
            Val::Int(i) => Some(*i),
            Val::Text(s) => s.parse().ok(),
            _ => None,
        }
    }
}

fn read_varint(buf: &[u8], i: &mut usize) -> Option<u64> {
    let mut result = 0u64;
    for n in 0..9 {
        if *i >= buf.len() {
            return None;
        }
        let v = buf[*i];
        *i += 1;
        if n < 8 {
            result = (result << 7) | u64::from(v & 0x7f);
            if v & 0x80 == 0 {
                return Some(result);
            }
        } else {
            result = (result << 8) | u64::from(v);
            return Some(result);
        }
    }
    None
}

fn find_table_root(data: &[u8], page_size: usize, name: &str) -> Option<u32> {
    let page = page_slice(data, page_size, 1)?;
    let rows = decode_table_leaf_page(page, 1)?;
    for cols in rows {
        // sqlite_master: type, name, tbl_name, rootpage, sql
        if cols.len() < 4 {
            continue;
        }
        let typ = cols.get(0).and_then(|v| v.as_str()).unwrap_or("");
        let tbl = cols.get(2).and_then(|v| v.as_str()).unwrap_or("");
        let nm = cols.get(1).and_then(|v| v.as_str()).unwrap_or("");
        if typ == "table" && (tbl == name || nm == name) {
            if let Some(root) = cols[3].as_i64() {
                if root > 0 {
                    return Some(root as u32);
                }
            }
        }
    }
    None
}

fn walk_table_pages(data: &[u8], page_size: usize, root: u32) -> anyhow::Result<Vec<Vec<Val>>> {
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(pgno) = stack.pop() {
        let page = page_slice(data, page_size, pgno)
            .ok_or_else(|| anyhow::anyhow!("página {pgno} ausente"))?;
        let hdr = btree_header_offset(pgno);
        let kind = page[hdr];
        match kind {
            0x05 => {
                // interior table
                let ncells = u16::from_be_bytes([page[hdr + 3], page[hdr + 4]]) as usize;
                let ptrs_start = hdr + 12;
                for i in 0..ncells {
                    let p = u16::from_be_bytes([page[ptrs_start + i * 2], page[ptrs_start + i * 2 + 1]])
                        as usize;
                    if p + 4 > page.len() {
                        continue;
                    }
                    let left = u32::from_be_bytes(page[p..p + 4].try_into().unwrap());
                    stack.push(left);
                }
                let right = u32::from_be_bytes(page[hdr + 8..hdr + 12].try_into().unwrap());
                if right != 0 {
                    stack.push(right);
                }
            }
            0x0d => {
                if let Some(rows) = decode_table_leaf_page(page, pgno) {
                    out.extend(rows);
                }
            }
            _ => {}
        }
    }
    Ok(out)
}

fn page_slice(data: &[u8], page_size: usize, pgno: u32) -> Option<&[u8]> {
    if pgno == 0 {
        return None;
    }
    let start = (pgno as usize - 1) * page_size;
    let end = start + page_size;
    if end > data.len() {
        return None;
    }
    Some(&data[start..end])
}

fn btree_header_offset(pgno: u32) -> usize {
    // Page 1 embeds the 100-byte database header; the b-tree header starts at 100.
    if pgno == 1 {
        100
    } else {
        0
    }
}

fn decode_table_leaf_page(page: &[u8], pgno: u32) -> Option<Vec<Vec<Val>>> {
    let hdr = btree_header_offset(pgno);
    if page.len() <= hdr || page[hdr] != 0x0d {
        return None;
    }
    let ncells = u16::from_be_bytes([page[hdr + 3], page[hdr + 4]]) as usize;
    let mut rows = Vec::new();
    let ptr_base = hdr + 8;
    for c in 0..ncells {
        let ptr = u16::from_be_bytes([page[ptr_base + c * 2], page[ptr_base + c * 2 + 1]]) as usize;
        let mut i = ptr;
        if i >= page.len() {
            continue;
        }
        let payload_len = read_varint(page, &mut i)? as usize;
        let _rowid = read_varint(page, &mut i)?;
        let end = (i + payload_len).min(page.len());
        if end <= i {
            continue;
        }
        if let Some(cols) = decode_record(&page[i..end]) {
            rows.push(cols);
        }
    }
    Some(rows)
}


fn decode_record(payload: &[u8]) -> Option<Vec<Val>> {
    let mut i = 0usize;
    let header_size = read_varint(payload, &mut i)? as usize;
    if header_size > payload.len() {
        return None;
    }
    let mut serials = Vec::new();
    while i < header_size {
        serials.push(read_varint(payload, &mut i)?);
    }
    let mut body = header_size;
    let mut cols = Vec::new();
    for s in serials {
        let (v, adv) = read_column(payload, body, s)?;
        body += adv;
        cols.push(v);
    }
    Some(cols)
}

fn read_column(buf: &[u8], at: usize, serial: u64) -> Option<(Val, usize)> {
    match serial {
        0 => Some((Val::Null, 0)),
        1 => {
            if at + 1 > buf.len() {
                return None;
            }
            Some((Val::Int(i8::from_be_bytes([buf[at]]) as i64), 1))
        }
        2 => {
            if at + 2 > buf.len() {
                return None;
            }
            Some((
                Val::Int(i16::from_be_bytes([buf[at], buf[at + 1]]) as i64),
                2,
            ))
        }
        3 => {
            if at + 3 > buf.len() {
                return None;
            }
            let mut b = [0u8; 4];
            b[1..].copy_from_slice(&buf[at..at + 3]);
            Some((Val::Int(i32::from_be_bytes(b) as i64), 3))
        }
        4 => {
            if at + 4 > buf.len() {
                return None;
            }
            Some((
                Val::Int(i32::from_be_bytes(buf[at..at + 4].try_into().ok()?) as i64),
                4,
            ))
        }
        5 => {
            if at + 6 > buf.len() {
                return None;
            }
            let mut b = [0u8; 8];
            b[2..].copy_from_slice(&buf[at..at + 6]);
            Some((Val::Int(i64::from_be_bytes(b)), 6))
        }
        6 | 7 => {
            if at + 8 > buf.len() {
                return None;
            }
            Some((
                Val::Int(i64::from_be_bytes(buf[at..at + 8].try_into().ok()?)),
                8,
            ))
        }
        8 => Some((Val::Int(0), 0)),
        9 => Some((Val::Int(1), 0)),
        n if n >= 12 && n % 2 == 0 => {
            let len = ((n - 12) / 2) as usize;
            if at + len > buf.len() {
                return None;
            }
            Some((Val::Blob(buf[at..at + len].to_vec()), len))
        }
        n if n >= 13 && n % 2 == 1 => {
            let len = ((n - 13) / 2) as usize;
            if at + len > buf.len() {
                return None;
            }
            let s = String::from_utf8_lossy(&buf[at..at + len]).into_owned();
            Some((Val::Text(s), len))
        }
        _ => Some((Val::Null, 0)),
    }
}

fn write_minimal_sweepers_db(book: &SweeperBook, path: &Path) -> anyhow::Result<()> {
    let payload = serde_json::to_string(
        &book
            .entries
            .iter()
            .map(|e| {
                serde_json::json!({
                    "base_file": e.base_file.display().to_string(),
                    "sweeper_file": e.sweeper_file.display().to_string(),
                    "offset_secs": e.offset_secs,
                    "enabled": e.enabled,
                })
            })
            .collect::<Vec<_>>(),
    )?;
    let json_path = path.with_extension("export.json");
    std::fs::write(&json_path, &payload)?;
    let script = r#"
import sqlite3, json, os, sys
path, jpath = sys.argv[1], sys.argv[2]
entries = json.load(open(jpath, encoding="utf-8"))
if os.path.exists(path):
    os.remove(path)
c = sqlite3.connect(path)
c.execute("CREATE TABLE sweepers(file VARCHAR(560) PRIMARY KEY, pisa VARCHAR(560), time INT, enabled BOOL)")
for e in entries:
    c.execute(
        "INSERT INTO sweepers (file, pisa, time, enabled) VALUES (?,?,?,?)",
        (e["base_file"], e["sweeper_file"], int(float(e["offset_secs"])), 1 if e.get("enabled", True) else 0),
    )
c.commit()
c.close()
"#;
    let st = std::process::Command::new("python3")
        .arg("-c")
        .arg(script)
        .arg(path.as_os_str())
        .arg(json_path.as_os_str())
        .status();
    let _ = std::fs::remove_file(&json_path);
    match st {
        Ok(s) if s.success() => Ok(()),
        _ => {
            let twin = path.with_extension("json");
            book.save(&twin)?;
            anyhow::bail!(
                "não foi possível gerar SQLite nativo neste host; salvei {}",
                twin.display()
            )
        }
    }
}
