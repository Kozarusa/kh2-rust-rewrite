//! Struts: each sheet row -> a piece of Rust. Unfinished rows still emit code, as an explicit
//! `UNFINISHED` item naming the unchecked columns, so gaps stay visible in the generated source.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::Sheet;

pub const KNOWN: &[&str] = &["rust_table", "rust_system", crate::ledger::STRUT];

#[derive(Debug, Clone, PartialEq)]
pub enum Ty {
    Prim(&'static str),
    Str(usize),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub name: String,
    pub ty: Ty,
    pub count: usize,
}

impl Field {
    pub fn size(&self) -> usize {
        match self.ty {
            Ty::Str(n) => n,
            Ty::Prim(p) => prim_size(p) * self.count,
        }
    }
}

fn prim_size(p: &str) -> usize {
    match p {
        "u8" | "i8" => 1,
        "u16" | "i16" => 2,
        "u32" | "i32" | "f32" => 4,
        _ => 8,
    }
}

/// `"Id:u16 Name:str32 Items:u16x32"` -> fields. Sheet types use `s8..s32` for signed.
pub fn parse_layout(s: &str) -> Result<Vec<Field>, String> {
    if s.trim().is_empty() {
        return Err("empty layout".into());
    }
    s.split_whitespace()
        .map(|f| {
            let (name, t) = f.split_once(':').ok_or_else(|| format!("field '{f}' lacks ':type'"))?;
            if let Some(n) = t.strip_prefix("str") {
                let n = n.parse().map_err(|_| format!("bad string size in '{f}'"))?;
                return Ok(Field { name: name.into(), ty: Ty::Str(n), count: 1 });
            }
            let (base, count) = match t.split_once('x') {
                Some((b, c)) => (b, c.parse().map_err(|_| format!("bad count in '{f}'"))?),
                None => (t, 1),
            };
            let prim = match base {
                "u8" => "u8",
                "s8" => "i8",
                "u16" => "u16",
                "s16" => "i16",
                "u32" => "u32",
                "s32" => "i32",
                "f32" => "f32",
                "u64" => "u64",
                "s64" => "i64",
                _ => return Err(format!("unknown type '{base}' in '{f}'")),
            };
            Ok(Field { name: name.into(), ty: Ty::Prim(prim), count })
        })
        .collect()
}

pub fn snake(s: &str) -> String {
    let mut out = String::new();
    let chars: Vec<char> = s.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        let prev_lower = i > 0 && (chars[i - 1].is_lowercase() || chars[i - 1].is_ascii_digit());
        let next_lower = chars.get(i + 1).is_some_and(|n| n.is_lowercase());
        if c.is_uppercase() && i > 0 && (prev_lower || next_lower && chars[i - 1].is_uppercase()) {
            out.push('_');
        }
        out.extend(c.to_lowercase());
    }
    match out.as_str() {
        "type" | "match" | "ref" | "mod" | "move" | "loop" | "use" | "fn" | "impl" | "self" | "static" | "const"
        | "where" | "enum" | "struct" | "trait" | "box" | "in" | "as" | "dyn" => format!("r#{out}"),
        _ => out,
    }
}

pub fn pascal(s: &str) -> String {
    s.split(['_', '.', '-'])
        .map(|w| {
            let mut c = w.chars();
            c.next().map(|f| f.to_uppercase().chain(c).collect::<String>()).unwrap_or_default()
        })
        .collect()
}

fn s<'a>(sheet: &Sheet, row: &'a [serde_json::Value], col: &str) -> Option<&'a str> {
    sheet.get(row, col).as_str()
}

fn rust_table(sh: &Sheet) -> Result<String, String> {
    let mut o = String::from(
        "// @generated from sheets/tables.json by kh2-sheets (strut rust_table). Do not edit; edit the sheet.\n\
         use crate::rt::{Header, Reader, Record, TableDef};\n\n",
    );
    let mut all = Vec::new();
    let mut decoders = Vec::new();
    // Record sizes of first tables, for `table2` rows that follow them in the same subfile.
    let mut first_size = BTreeMap::new();
    for row in &sh.rows {
        if let (Some("table"), Some(l)) = (s(sh, row, "header"), s(sh, row, "layout")) {
            let size: usize = parse_layout(l)?.iter().map(Field::size).sum();
            first_size.insert((s(sh, row, "archive").unwrap_or(""), s(sh, row, "tag").unwrap_or("")), size);
        }
    }
    for row in &sh.rows {
        let id = sh.key_of(row);
        let konst = id.to_uppercase();
        let (arc, tag) = (s(sh, row, "archive").unwrap_or(""), s(sh, row, "tag").unwrap_or(""));
        let miss = sh.unchecked(row);
        if !miss.is_empty() {
            let _ = writeln!(o, "// UNFINISHED {id}: unchecked {}", miss.join(", "));
        }
        let (header, size, skip) = match (s(sh, row, "header"), s(sh, row, "layout")) {
            (Some(h), Some(l)) => {
                let fields = parse_layout(l)?;
                let size: usize = fields.iter().map(Field::size).sum();
                let ty = pascal(id);
                let mut names = Vec::new();
                let _ = writeln!(o, "#[derive(Debug, Clone, PartialEq)]\npub struct {ty} {{");
                for f in &fields {
                    let n = snake(&f.name);
                    let t = match (&f.ty, f.count) {
                        (Ty::Str(_), _) => "String".to_string(),
                        (Ty::Prim(p), 1) => p.to_string(),
                        (Ty::Prim(p), c) => format!("[{p}; {c}]"),
                    };
                    let _ = writeln!(o, "    pub {n}: {t},");
                    names.push(n);
                }
                let _ = writeln!(o, "}}\n\nimpl Record for {ty} {{\n    const SIZE: usize = {size};");
                let quoted: Vec<String> = fields.iter().map(|f| format!("{:?}", f.name)).collect();
                let _ = writeln!(o, "    const FIELDS: &'static [&'static str] = &[{}];", quoted.join(", "));
                let _ = writeln!(o, "    fn parse(r: &mut Reader<'_>) -> Self {{\n        Self {{");
                for (f, n) in fields.iter().zip(&names) {
                    let e = match (&f.ty, f.count) {
                        (Ty::Str(k), _) => format!("r.string({k})"),
                        (Ty::Prim(p), 1) => format!("r.{p}()"),
                        (Ty::Prim(p), _) => format!("std::array::from_fn(|_| r.{p}())"),
                    };
                    let _ = writeln!(o, "            {n}: {e},");
                }
                let _ = writeln!(o, "        }}\n    }}\n    fn cells(&self) -> Vec<String> {{\n        vec![");
                for n in &names {
                    let _ = writeln!(o, "            format!(\"{{:?}}\", self.{n}),");
                }
                let _ = writeln!(o, "        ]\n    }}\n}}\n");
                decoders.push(format!(
                    "        \"{id}\" => Some(crate::rt::decode::<{ty}>(&{konst}, blob).map(crate::rt::DynTable::from)),"
                ));
                let header = match h {
                    "table" => "Header::Table".to_string(),
                    "short" => "Header::Short".to_string(),
                    "table2" => "Header::SecondTable".to_string(),
                    "list" => "Header::List".to_string(),
                    "lvup" => "Header::Lvup".to_string(),
                    p if p.starts_with("pref:") => format!("Header::Pref({:?})", &p[5..]),
                    other => return Err(format!("tables[{id}]: unknown header '{other}'")),
                };
                let skip = if h == "table2" {
                    *first_size.get(&(arc, tag)).ok_or(format!("tables[{id}]: table2 without a first table"))?
                } else {
                    0
                };
                (header, format!("Some({size})"), skip)
            }
            _ => ("Header::Unknown".to_string(), "None".to_string(), 0),
        };
        let _ = writeln!(
            o,
            "pub const {konst}: TableDef = TableDef {{ name: {id:?}, archive: {arc:?}, tag: {tag:?}, header: {header}, \
             record_size: {size}, first_record_size: {skip} }};\n"
        );
        all.push(konst);
    }
    let _ = writeln!(o, "pub const ALL: &[TableDef] = &[{}];\n", all.join(", "));
    let _ = writeln!(
        o,
        "/// Decodes any modelled table by name (None = no layout yet).\n\
         pub fn decode_dyn(name: &str, blob: &[u8]) -> Option<Result<crate::rt::DynTable, crate::rt::DecodeError>> {{\n    match name {{\n{}\n        _ => None,\n    }}\n}}",
        decoders.join("\n")
    );
    Ok(o)
}

fn rust_system(sh: &Sheet) -> Result<String, String> {
    let mut o = String::from(
        "// @generated from sheets/systems.json by kh2-sheets (strut rust_system). Do not edit; edit the sheet.\n\n\
         #[derive(Debug, Clone, Copy)]\npub struct SystemDef {\n    pub id: &'static str,\n    pub name: &'static str,\n    \
         pub krate: &'static str,\n    pub tables: &'static [&'static str],\n    pub finished: bool,\n}\n\npub const SYSTEMS: &[SystemDef] = &[\n",
    );
    for row in &sh.rows {
        let miss = sh.unchecked(row);
        if !miss.is_empty() {
            let _ = writeln!(o, "    // UNFINISHED {}: unchecked {}", sh.key_of(row), miss.join(", "));
        }
        let tables: Vec<String> = sh
            .get(row, "tables")
            .as_array()
            .map(|a| a.iter().filter_map(|v| v.as_str()).map(|t| format!("{t:?}")).collect())
            .unwrap_or_default();
        let _ = writeln!(
            o,
            "    SystemDef {{ id: {:?}, name: {:?}, krate: {:?}, tables: &[{}], finished: {} }},",
            sh.key_of(row),
            s(sh, row, "name").unwrap_or(""),
            s(sh, row, "crate").unwrap_or(""),
            tables.join(", "),
            miss.is_empty()
        );
    }
    o.push_str("];\n");
    Ok(o)
}

/// Renders every sheet's strut: file name -> Rust source.
pub fn render_all(sheets: &BTreeMap<String, Sheet>) -> Result<BTreeMap<String, String>, String> {
    let mut out = BTreeMap::new();
    for s in sheets.values() {
        let code = match s.strut.as_str() {
            "rust_table" => rust_table(s)?,
            "rust_system" => rust_system(s)?,
            // The function ledger renders no code.
            crate::ledger::STRUT => continue,
            other => return Err(format!("{}: unknown strut {other}", s.name)),
        };
        out.insert(format!("{}.rs", s.name), code);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_and_names() {
        let f = parse_layout("Id:u16 FileName:str32 Items:u16x32 Hp:s8").unwrap();
        assert_eq!(f.iter().map(Field::size).sum::<usize>(), 2 + 32 + 64 + 1);
        assert_eq!(snake("ReflectRC"), "reflect_rc");
        assert_eq!(snake("Item1Percentage"), "item1_percentage");
        assert_eq!(snake("Type"), "r#type");
        assert_eq!(pascal("item_stat"), "ItemStat");
        assert!(parse_layout("Id:q8").is_err());
    }
}
