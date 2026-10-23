//! Read-only checks behind `docs/systems/party/contract.md`: the party member table `memt` merged per world the way
//! FUN_001a6ff0 merges it, the party presets after its records, the objects `FRIEND_1`/`FRIEND_2`
//! stand for, the party members' own objects (objentry type 1) with the friend-AI class FUN_0010b780
//! picks for them, and their `plrp`, `prty` and `lvup` rows. Everything is read from the user's own
//! disc ($KH2_ISO or local/iso.txt); each test skips with a note when the disc is not available.
//! Run with `--nocapture` to print the rows.

use std::path::{Path, PathBuf};

use kh2_archive::Archive;
use kh2_tables::player::{self, Memt};
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

/// objentry ids the spawn code resolves at run time (FUN_0016e208): the player and the two friends.
const PLAYER: u32 = 0x236;
const FRIEND_1: u32 = 0x237;
const FRIEND_2: u32 = 0x238;
/// objentry `ObjectType` of a party member (FUN_0016e8c0 case 1 -> FUN_0017d1b0).
const TYPE_FRIEND: u8 = 1;
/// "No member" in a slot byte (FUN_001a71e8 masks the byte with 0x1f; 0x10 means empty).
const EMPTY: u8 = 0x10;

/// The doc's rule for FUN_001a6ff0 / FUN_001a6f70: the 16 member objects of a world, from every
/// memt record whose world word is 0 or the world and whose story-flag words pass (`flag(w1)` set when
/// w1 != 0, `flag(w2)` clear when w2 != 0); a later record's non-zero words replace earlier ones.
fn merged(m: &Memt, world: u16, flag: impl Fn(u16) -> bool) -> [u16; 16] {
    let mut out = [0u16; 16];
    for r in &m.rows {
        let ok = (r[0] == 0 || r[0] == world) && (r[1] == 0 || flag(r[1])) && (r[2] == 0 || !flag(r[2]));
        if ok {
            for k in 0..16 {
                if r[8 + k] != 0 {
                    out[k] = r[8 + k];
                }
            }
        }
    }
    out
}

/// The doc's rule for FUN_0010b780: the friend-AI class index (into the parameter table at 0x34ea00)
/// for an object's NeoStatus and NeoMoveset; `None` = no AI object (Sora 1, Mickey 4, Roxas 14), and
/// NeoStatus outside the listed values takes the default class with the table's first entry.
fn ai_class(neo: u16, moveset: u16) -> Option<u8> {
    Some(match neo {
        1 | 4 | 14 => return None,
        2 if moveset == 0x12 => 12,
        2 => 0,
        3 if moveset == 0x15 => 13,
        3 => 1,
        5 => 2,
        15 => 3,
        6 => 4,
        7 => 5,
        8 => 6,
        9 => 7,
        10 => 8,
        11 => 9,
        12 => 10,
        13 => 11,
        200 => 14,
        201 => 15,
        202 => 16,
        203 => 17,
        204 => 18,
        _ => 0,
    })
}

#[test]
fn party_members_per_world_from_memt() {
    let Some(mut a) = open() else { return };
    let m = player::load_memt(&mut a).unwrap();
    let objs = rows!(&mut a, tables::Objentry, tables::OBJENTRY);
    let name = |id: u16| objs.iter().find(|o| o.object_id == id as u32).map(|o| o.model_name.clone()).unwrap_or_else(|| "-".into());
    let neo = |id: u16| objs.iter().find(|o| o.object_id == id as u32).map(|o| o.neo_status).unwrap_or(0);

    println!("memt version {} records {} presets {}", m.version, m.rows.len(), m.trailing.len());
    for (i, r) in m.rows.iter().enumerate() {
        let members: Vec<String> = (0..16).filter(|&k| r[8 + k] != 0).map(|k| format!("{k}:{}({})", r[8 + k], name(r[8 + k]))).collect();
        println!("  record {i:2}: world {:2} flag {:5} not-flag {:5} | {}", r[0], r[1], r[2], members.join(" "));
    }
    // The presets FUN_001a7330 reads (DAT_01c95604 = after the records, 4 bytes each): a member index
    // in the low 5 bits (0x10 = none), bits 0x80/0x40 = "put first".
    for (i, w) in m.trailing.iter().enumerate() {
        let b = w.to_le_bytes();
        let s: Vec<String> = b
            .iter()
            .map(|&x| if x & 0x1f == EMPTY { "-".to_string() } else { format!("{}{}", x & 0x1f, if x & 0xc0 != 0 { "*" } else { "" }) })
            .collect();
        println!("  preset {i}: bytes {b:02x?} members {}", s.join(","));
    }

    // Per world with no story flag set: player (index 0), the two companions (1, 2), the world's
    // partner (3) and Mickey (9).
    for world in 0..=18u16 {
        let w = merged(&m, world, |_| false);
        println!(
            "  world {world:2}: 0 {} | 1 {} | 2 {} | 3 {} | 9 {}",
            name(w[0]),
            name(w[1]),
            name(w[2]),
            name(w[3]),
            name(w[9])
        );
    }
    // With no flag set, world 0's companions are Donald (NeoStatus 2) and Goofy (3), and Sora's normal
    // object is index 0: FRIEND_1/FRIEND_2 resolve to them through slot bytes 1 and 2.
    let w0 = merged(&m, 0, |_| false);
    assert_eq!(neo(w0[0]), 1);
    assert_eq!(neo(w0[1]), 2);
    assert_eq!(neo(w0[2]), 3);
    // memt holds the object ids FUN_001cf2c8 reads for the forms (indices 4..=8) as well.
    for f in 1..=5u8 {
        assert_eq!(m.form_object(f), Some(w0[3 + f as usize]));
    }
}

#[test]
fn party_member_objects_and_rows() {
    let Some(mut a) = open() else { return };
    let objs = rows!(&mut a, tables::Objentry, tables::OBJENTRY);
    let plrp = rows!(&mut a, tables::Plrp, tables::PLRP);
    let prty = rows!(&mut a, tables::PrefPrty, tables::PREF_PRTY);
    let items = rows!(&mut a, tables::Item, tables::ITEM);
    let msg = player::load_messages(&mut a);
    let item_name = |id: u16| {
        items
            .iter()
            .find(|i| i.id == id)
            .and_then(|i| msg.as_ref().and_then(|m| m.get(i.name as u32)))
            .unwrap_or_default()
    };

    // The three objects the spawn code resolves at run time.
    for id in [PLAYER, FRIEND_1, FRIEND_2] {
        let o = objs.iter().find(|o| o.object_id == id).unwrap();
        println!("object {id:#x} {} {} type {} neo {}", o.model_name, o.animation_name, o.object_type, o.neo_status);
        assert_eq!(o.neo_status, 0, "a stand-in object has no NeoStatus of its own");
    }
    assert_eq!(objs.iter().find(|o| o.object_id == FRIEND_1).unwrap().object_type, TYPE_FRIEND);
    assert_eq!(objs.iter().find(|o| o.object_id == FRIEND_2).unwrap().object_type, TYPE_FRIEND);

    // Every party-member object: files on the disc, the AI class, the BDX script if its model has one.
    for o in objs.iter().filter(|o| o.object_type == TYPE_FRIEND && o.neo_status != 0) {
        let mdlx = format!("obj/{}.mdlx", o.model_name);
        let kinds: Vec<u16> = a
            .read(&mdlx)
            .ok()
            .and_then(|d| kh2_bar::parse(&d).ok().map(|e| e.iter().map(|x| x.kind).collect()))
            .unwrap_or_default();
        let mset = a.read(&format!("obj/{}", o.animation_name)).map(|d| d.len()).unwrap_or(0);
        let apdx = a.read(&format!("obj/{}.apdx", o.model_name)).map(|d| d.len()).unwrap_or(0);
        println!(
            "member object {:4} {:18} neo {:2} moveset {:2} ai class {:?} | mdlx BAR kinds {:?} (BDX {}) mset {} bytes apdx {} bytes",
            o.object_id,
            o.model_name,
            o.neo_status,
            o.neo_moveset,
            ai_class(o.neo_status, o.neo_moveset),
            kinds,
            kinds.contains(&3),
            mset,
            apdx
        );
    }
    // Donald and Goofy's objects in the default world are on the disc with a model and a motion set.
    for model in ["P_EX020", "P_EX030"] {
        let o = objs.iter().find(|o| o.model_name == model).unwrap();
        assert_eq!(o.object_type, TYPE_FRIEND);
        assert!(a.read(&format!("obj/{model}.mdlx")).is_ok());
        assert!(a.read(&format!("obj/{}", o.animation_name)).is_ok());
    }

    // Base stats and starting items per party member id (plrp.Character).
    for p in &plrp {
        let it: Vec<String> = p
            .items
            .iter()
            .filter(|&&x| x != 0)
            .map(|&x| format!("{}{}{:?}", x & 0x7fff, if x & 0x8000 != 0 { "*" } else { "" }, item_name(x & 0x7fff)))
            .collect();
        println!(
            "plrp id {:3} char {:3} hp {:3} mp {:3} ap {:3} str {:3} mag {:3} def {:3} slots a{} c{} i{} | {}",
            p.id,
            p.character,
            p.hp,
            p.mp,
            p.ap,
            p.strength,
            p.magic,
            p.defense,
            p.armor_slot_max,
            p.accessory_slot_max,
            p.item_slot_max,
            it.join(" ")
        );
    }
    assert!(plrp.iter().any(|p| p.character == 2), "Donald has a plrp row");
    assert!(plrp.iter().any(|p| p.character == 3), "Goofy has a plrp row");

    // prty rows by character index (row 1 is Sora's, as kh2-app reads it): walk, run, StunTime.
    for (i, r) in prty.iter().enumerate().take(16) {
        println!("prty row {i:2}: walk {:6.2} run {:6.2} jump {:6.1} turn {:.4} stun {:6.1}", r.walk_speed, r.run_speed, r.jump_height, r.turn_speed, r.stun_time);
    }

    // lvup: per-character level tables, Sora first.
    let blob = kh2_tables::subfile(&mut a, &tables::LVUP).unwrap();
    let chars = kh2_tables::rt::decode_lvup::<tables::Lvup>(&blob).unwrap();
    for (i, c) in chars.iter().enumerate() {
        let first = c.first().map(|l| (l.exp, l.strength, l.magic, l.defense, l.ap));
        println!("lvup character table {i:2}: {} levels, level 1 (exp, str, mag, def, ap) {:?}", c.len(), first);
    }
    assert!(chars.len() > 3);
}
