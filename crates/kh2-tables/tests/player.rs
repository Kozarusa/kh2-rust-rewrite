//! Structure checks for the player-system decoders. They read the user's own disc ($KH2_ISO or
//! local/iso.txt) and skip, with a note, when it is not available.


use kh2_archive::Archive;
use kh2_tables::player::{self, execute, form, item_type};
use kh2_tables::tables;

fn open() -> Option<Archive> {
    kh2_factory::disc("player")
}

macro_rules! rows {
    ($a:expr, $ty:ty, $def:expr) => {
        kh2_tables::load::<$ty>($a, &$def).unwrap().rows
    };
}

#[test]
fn form_levels_are_a_complete_curve() {
    let Some(mut a) = open() else { return };
    let fl = player::FormLevels::from_rows(&rows!(&mut a, tables::Fmlv, tables::FMLV));
    // Forms 1..=4 have levels 1..=7; the last level has no exp requirement.
    for f in form::VALOR..=form::FINAL {
        assert_eq!(fl.top_level(f), 7, "form {f}");
        let mut prev = 0;
        for l in 1..=6 {
            let r = fl.get(f, l).unwrap();
            assert!(r.exp > prev, "form {f} level {l} exp not increasing");
            prev = r.exp;
        }
        assert_eq!(fl.get(f, 7).unwrap().exp, 0);
        // The displayed level never goes down and ends at 4.
        let shown: Vec<u8> = fl.levels(f).map(|r| r.display_level).collect();
        assert!(shown.windows(2).all(|w| w[0] <= w[1]), "form {f} display level {shown:?}");
        assert_eq!(*shown.last().unwrap(), 4);
    }
    // Summon curve (form 0) and the Anti roll table (form 5).
    assert_eq!(fl.top_level(form::SUMMON_CURVE), 7);
    assert!(fl.levels(form::ANTI).count() >= 2);
    // The Anti chance only grows with the counter and is zero at a fresh counter.
    let chances: Vec<u8> = (0..40).map(|c| player::anti_form_chance(&fl, c)).collect();
    assert_eq!(chances[0], 0);
    assert!(chances.windows(2).all(|w| w[0] <= w[1]), "{chances:?}");
    assert!(*chances.last().unwrap() > 0);
    assert_eq!(player::form_level_cap(0), 2);
    assert_eq!(player::form_level_cap(3), 5);
    assert_eq!(player::form_level_cap(4), 7);
}

#[test]
fn form_abilities_exist_in_the_ability_list() {
    let Some(mut a) = open() else { return };
    let fl = player::FormLevels::from_rows(&rows!(&mut a, tables::Fmlv, tables::FMLV));
    let items = rows!(&mut a, tables::Item, tables::ITEM);
    let list = player::abilities(&items);
    assert!(list.len() > 100);
    for r in fl.0.iter().filter(|r| r.ability != 0 && r.form != form::ANTI) {
        assert!(list.iter().any(|x| x.id == r.ability), "form {} level {} grants unknown ability {}", r.form, r.level, r.ability);
    }
    // Auto-form abilities point at drive commands.
    let cmds = rows!(&mut a, tables::Cmd, tables::CMD);
    let autos: Vec<_> = list.iter().filter(|x| x.command != 0).collect();
    assert!(autos.len() >= 4);
    for x in autos {
        let c = cmds.iter().find(|c| c.id == x.command).expect("ability command exists");
        assert_eq!(c.execute, execute::DRIVE);
    }
    // AP cost and kind are small numbers.
    assert!(list.iter().all(|x| x.ap_cost <= 9 && x.kind <= 3));
}

#[test]
fn drive_forms_join_item_command_and_objects() {
    let Some(mut a) = open() else { return };
    let items = rows!(&mut a, tables::Item, tables::ITEM);
    let cmds = rows!(&mut a, tables::Cmd, tables::CMD);
    let objs = rows!(&mut a, tables::Objentry, tables::OBJENTRY);
    let memt = player::load_memt(&mut a).unwrap();
    let forms = player::drive_forms(&items, &cmds, &memt);
    assert_eq!(forms.iter().map(|f| f.form).collect::<Vec<_>>(), vec![1, 2, 3, 4, 5]);
    assert_eq!(items.iter().filter(|i| i.r#type == item_type::DRIVE_FORM).count(), 5);
    for f in &forms {
        assert!(f.cost >= 3, "{f:?}");
        let o = objs.iter().find(|o| Some(o.object_id as u16) == f.object).expect("form object exists");
        assert_eq!(o.object_form, f.form, "objentry ObjectForm of {}", o.model_name);
        assert!(o.model_name.starts_with("P_EX100"), "{}", o.model_name);
        assert!(objs.iter().any(|o| Some(o.object_id as u16) == f.weapon), "weapon object of form {}", f.form);
    }
    // Costs rise with the form's power: Anti needs the most.
    assert!(forms[4].cost > forms[3].cost && forms[3].cost >= forms[0].cost);
    // Normal-state weapon (form 0) is also a real object.
    let w0 = memt.form_weapon(0).unwrap();
    assert!(objs.iter().any(|o| o.object_id == w0 as u32));
}

#[test]
fn memt_shape() {
    let Some(mut a) = open() else { return };
    let m = player::load_memt(&mut a).unwrap();
    assert_eq!(m.rows.len(), 34);
    assert_eq!(m.trailing.len(), 7);
    assert!(m.sora().is_some());
    assert!(m.form_object(0).is_none() && m.form_object(6).is_none());
}

#[test]
fn limits_commands_and_abilities_agree() {
    let Some(mut a) = open() else { return };
    let limt = rows!(&mut a, tables::Limt, tables::LIMT);
    let cmds = rows!(&mut a, tables::Cmd, tables::CMD);
    let items = rows!(&mut a, tables::Item, tables::ITEM);
    let sumn = rows!(&mut a, tables::Sumn, tables::SUMN);
    assert_eq!(limt.len(), 27);
    for l in &limt {
        let c = cmds.iter().find(|c| c.id == l.command).expect("limit command exists");
        assert_eq!(c.execute, execute::LIMIT, "limit {} command {}", l.id, l.command);
        assert_eq!(c.argument as u16, l.command);
        let ab = items.iter().find(|i| i.id == l.limit).expect("limit ability exists");
        // A normal limit is gated by an ability; a summon limit by the summon's charm item
        // (FUN_001d1ce0 only checks the ability bit when the item is type 19).
        let want = if player::is_summon_limit(l.summon) { item_type::SUMMON_CHARM } else { item_type::ABILITY };
        assert_eq!(ab.r#type, want, "limit {}", l.id);
        assert!(l.file_name.ends_with(".bar"));
        assert!(l.character >= 1 && l.character <= 13);
    }
    // World variants resolve to the first row whose World is 0 or the current world.
    let wi = limt.iter().find(|l| l.world != 0).unwrap();
    assert_eq!(player::limit_for_command(&limt, wi.command, wi.world).unwrap().world, wi.world);
    assert_eq!(player::limit_for_command(&limt, wi.command, 0).unwrap().world, 0);
    // Summons: charm item -> command, limit command exists; three of four share the limit's command.
    assert_eq!(sumn.len(), 4);
    let mut shared = 0;
    for s in &sumn {
        let charm = items.iter().find(|i| i.id == s.item).unwrap();
        assert_eq!(charm.r#type, item_type::SUMMON_CHARM);
        assert_eq!(charm.command, s.command);
        let c = cmds.iter().find(|c| c.id == s.command).unwrap();
        assert_eq!(c.execute, execute::SUMMON);
        assert!(cmds.iter().any(|c| c.id == s.limit_command));
        if let Some(l) = limt.iter().find(|l| l.command == s.limit_command) {
            assert!(player::is_summon_limit(l.summon), "summon limit {}", l.id);
            shared += 1;
        }
        assert_eq!(player::summon_for_command(&sumn, s.command).unwrap().item, s.item);
    }
    assert!(shared >= 3);
}

#[test]
fn rcct_is_a_sorted_key_table_of_reaction_commands() {
    let Some(mut a) = open() else { return };
    let cmds = rows!(&mut a, tables::Cmd, tables::CMD);
    let (version, rows) = player::load_rcct(&mut a).unwrap();
    assert_eq!(version, 1);
    assert_eq!(rows.len(), 32);
    // The game binary-searches by key, so keys must ascend.
    assert!(rows.windows(2).all(|w| w[0].key < w[1].key));
    for r in &rows {
        assert!(r.commands[0] != 0, "key {} lists nothing", r.key);
        for &c in r.commands.iter().filter(|&&c| c != 0) {
            assert!(cmds.iter().any(|x| x.id == c), "key {} command {c}", r.key);
        }
    }
}

#[test]
fn sstm_record_matches_the_engine_reads() {
    let Some(mut a) = open() else { return };
    let s = player::load_sstm(&mut a).unwrap();
    assert_eq!(s.words.len(), 92);
    assert!(s.drive_point_units() > 0.0);
    // The sheet decodes the first 21 fields of the same record.
    let pref = rows!(&mut a, tables::PrefSstm, tables::PREF_SSTM);
    assert_eq!(pref.len(), 1);
    assert_eq!(pref[0].ceiling_stop.to_bits(), s.words[0]);
    assert_eq!(pref[0].lock_on_release_range.to_bits(), s.words[20]);
    // Every labelled offset is inside the record and 4-aligned.
    for off in (0..s.words.len() * 4).filter_map(|o| player::sstm_label(o).map(|_| o)) {
        assert_eq!(off % 4, 0);
    }
}
