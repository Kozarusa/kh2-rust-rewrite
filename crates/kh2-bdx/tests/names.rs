//! The table of named host functions in `names.rs` and the one in docs/bdx.md must agree: every named
//! function has a row with its signature and native address, and the kinds describe the arguments.

use kh2_bdx::names::{HOST_FUNCS, lookup};

#[test]
fn every_named_function_is_documented_with_signature_and_address() {
    let doc = std::fs::read_to_string("../../docs/bdx.md").expect("docs/bdx.md");
    for f in HOST_FUNCS {
        let ret = if f.ret_kind == '-' { String::new() } else { format!(" -> {}", f.ret_kind) };
        let row_start = format!("| {}.{} | `{}({}){}` | {:#x} |", f.group, f.index, f.name, f.kinds, ret, f.native);
        assert!(doc.contains(&row_start), "docs/bdx.md has no row starting `{row_start}`");
        assert!(doc.contains(f.about), "docs/bdx.md does not carry the note of {}.{} {}", f.group, f.index, f.name);
    }
}

#[test]
fn the_table_is_sorted_and_consistent() {
    for w in HOST_FUNCS.windows(2) {
        assert!((w[0].group, w[0].index) < (w[1].group, w[1].index), "{} before {}", w[0].name, w[1].name);
    }
    let mut names: Vec<_> = HOST_FUNCS.iter().map(|f| f.name).collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), HOST_FUNCS.len(), "names are unique");
    for f in HOST_FUNCS {
        assert_eq!(f.args as usize, f.kinds.len(), "{}", f.name);
        assert_eq!(f.ret, f.ret_kind != '-', "{}", f.name);
        assert!(f.kinds.chars().all(|c| "Ooifvsp".contains(c)), "{}: {}", f.name, f.kinds);
        assert!(lookup(f.group, f.index).is_some_and(|g| g.name == f.name));
    }
}
