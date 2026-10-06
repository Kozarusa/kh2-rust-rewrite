//! Read-only checks behind `docs/summons.md`: the `sumn` rows, their charm items, commands, summon
//! objects and summon limits, read from the user's own disc ($KH2_ISO or local/iso.txt). Each test
//! skips, with a note, when the disc is not available. Run with `--nocapture` to print the rows.

use std::path::{Path, PathBuf};

use kh2_archive::Archive;
use kh2_tables::player::{self, execute, item_type};
use kh2_tables::tables;

fn iso() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("KH2_ISO") {
        return Some(p.into());
    }
    let f = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../local/iso.txt");
    std::fs::read_to_string(f).ok().map(|s| PathBuf::from(s.trim()))
}

fn open() -> Option<Archive> {
    let p = iso().filter(|p| p.exists());
    if p.is_none() {
        eprintln!("skipped: no disc image (set KH2_ISO)");
    }
    Archive::open(p?).ok()
}

macro_rules! rows {
    ($a:expr, $ty:ty, $def:expr) => {
        kh2_tables::load::<$ty>($a, &$def).unwrap().rows
    };
}

/// objentry `ObjectType` of every summon object (Entity1/Entity2 and the summon's spawn object).
const OBJECT_TYPE_SUMMON: u8 = 19;

#[test]
fn summon_rows_join_items_commands_objects_and_limits() {
    let Some(mut a) = open() else { return };
    let sumn = rows!(&mut a, tables::Sumn, tables::SUMN);
    let cmds = rows!(&mut a, tables::Cmd, tables::CMD);
    let items = rows!(&mut a, tables::Item, tables::ITEM);
    let objs = rows!(&mut a, tables::Objentry, tables::OBJENTRY);
    let limt = rows!(&mut a, tables::Limt, tables::LIMT);
    let msg = player::load_messages(&mut a);
    let name = |id: u16| {
        cmds.iter()
            .find(|c| c.id == id)
            .and_then(|c| msg.as_ref().and_then(|m| m.get(c.message_id as u32)))
            .unwrap_or_default()
    };
    let cmd = |id: u16| cmds.iter().find(|c| c.id == id).unwrap_or_else(|| panic!("command {id}"));

    assert_eq!(sumn.len(), 4);
    for (i, s) in sumn.iter().enumerate() {
        let c = cmd(s.command);
        let charm = items.iter().find(|x| x.id == s.item).unwrap();
        println!(
            "sumn[{}] cmd {} {:?} exe {} cost {} flags {:#x} | charm {} | limit cmd {} {:?}",
            i + 1,
            s.command,
            name(s.command),
            c.execute,
            c.cost,
            c.flags,
            s.item,
            s.limit_command,
            name(s.limit_command)
        );
        assert_eq!(c.execute, execute::SUMMON);
        assert_eq!(charm.r#type, item_type::SUMMON_CHARM);
        assert_eq!(charm.command, s.command);
        // Every summon command costs drive points from the same field the drive commands use.
        assert!(c.cost > 0);
        // The summon objects are objentry type 19 with a NeoStatus in 200..=204, the same numbers
        // the limt `Summon` byte uses for a summon limit.
        for e in [s.entity1, s.entity2].into_iter().filter(|&e| e != 0) {
            let o = objs.iter().find(|o| o.object_id == e).unwrap_or_else(|| panic!("object {e}"));
            println!(
                "    object {e} {} {} type {} neo {} spawn {:?}",
                o.model_name,
                o.animation_name,
                o.object_type,
                o.neo_status,
                [o.spawn_object1, o.spawn_object2, o.spawn_object3, o.spawn_object4]
            );
            assert_eq!(o.object_type, OBJECT_TYPE_SUMMON);
            assert!(player::is_summon_limit(o.neo_status as u8) && o.neo_status < 256, "neo {}", o.neo_status);
        }
        // The summon limit rows for this summon: the limt `Limit` item is the charm.
        for l in limt.iter().filter(|l| l.limit == s.item) {
            let lc = cmd(l.command);
            println!(
                "    limit {} {:?} file {} summon byte {} cost {} time {} require {}",
                l.command,
                name(l.command),
                l.file_name,
                l.summon,
                lc.cost,
                lc.time,
                lc.require
            );
            assert!(player::is_summon_limit(l.summon));
            assert_eq!(lc.execute, execute::LIMIT);
        }
    }
    // The summon objects of every summon limit (Summon byte) are summon objects of some sumn row.
    let summon_neo: Vec<u16> = sumn
        .iter()
        .flat_map(|s| [s.entity1, s.entity2])
        .filter(|&e| e != 0)
        .filter_map(|e| objs.iter().find(|o| o.object_id == e))
        .map(|o| o.neo_status as u16)
        .collect();
    for l in limt.iter().filter(|l| player::is_summon_limit(l.summon)) {
        assert!(summon_neo.contains(&(l.summon as u16)), "limit {} summon byte {}", l.command, l.summon);
    }
}

/// The commands near the summon limits that the summon code and the summon limits name: Genie's
/// per-form commands (ELF table 0x37c036 lists 535..=538 for forms 1..=4), their finishers (`Require`
/// = the per-form command) and the summon-only rows whose `Require` is 600.
#[test]
fn summon_command_family() {
    let Some(mut a) = open() else { return };
    let cmds = rows!(&mut a, tables::Cmd, tables::CMD);
    let msg = player::load_messages(&mut a);
    let name = |id: u32| msg.as_ref().and_then(|m| m.get(id)).unwrap_or_default();
    for c in cmds.iter().filter(|c| (515..=549).contains(&c.id) || [468, 469, 600, 609].contains(&c.id)) {
        println!(
            "cmd {} {:?} exe {} arg {} cost {} time {} require {} mark {} flags {:#x} receiver {}",
            c.id,
            name(c.message_id as u32),
            c.execute,
            c.argument,
            c.cost,
            c.time,
            c.require,
            c.mark,
            c.flags,
            c.cmd_receiver
        );
    }
    // Genie's per-form commands exist and are their own executors.
    for id in 535..=538u16 {
        let c = cmds.iter().find(|c| c.id == id).unwrap();
        assert_eq!(c.execute, id);
    }
}
