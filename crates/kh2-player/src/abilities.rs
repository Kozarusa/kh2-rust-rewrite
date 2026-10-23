//! Sora's abilities: the set he has equipped and the numbers the game derives from it.
//!
//! The game keeps an "ability block" in the character's battle status (`status + 0x1cc`): a bit set
//! of the applied abilities plus the stat fields that some abilities change (combo length, boosts,
//! drain multipliers, ...). `FUN_001c0d88` resets it, `FUN_001c0e50` applies the equipped abilities
//! and `FUN_001c0f40` applies one; `FUN_001c1340` derives the growth levels. `Block` is that block
//! and `Block::apply` is `FUN_001c0f40`. Everything is read from the disc: the ability rows (`item`
//! type 19), the equipment rows (`item_stat`), the step sizes (`sstm`) and the AP curve (`lvup`).
//! `docs/systems/combat-effects/abilities.md` has the verified facts, the API for `sora.rs` / `drive.rs` and the gaps.
//!
//! Environment: `KH2_ABILITIES=bit,bit,...` (ability bits, as the attack table gates them: each is
//! equipped through the ability row that owns the bit, so `21` really adds a combo hit),
//! `KH2_ABILITY_IDS=id,...` (item ids), `KH2_ABILITY_START=data` (no Guard baseline, see the doc),
//! `KH2_ABILITYLOG=1` (log the active abilities and the stat block deltas).
//!
//! Readers: the controller's view (`sora::Abilities`, `sora::Growth`) is written by
//! `sync_controller` whenever the block changes; the drive code sets and clears the form list
//! and reads `form_drain`; `attacker_bonuses` is the damage routine's use of the boosts.

// The menu queries (`ap_used`, `ap_max`, `equipment_stats`) wait for a menu (docs/systems/combat-effects/abilities.md).
#![allow(dead_code)]

use std::sync::Arc;

use bevy::prelude::*;

use crate::assets::Disc;
use crate::schedule::Sim;
use crate::sora::{self, Growth, GrowthOverride};

pub use crate::shared::abilities::{attacker_bonuses, berserk_attack, berserk_for_row, damage_drive_units, defender_bonus, hp_is_low, id, leaf_bracer_frames, lethal_hit_damage, mp_rage_gain, scan_binds_gauge, survives_lethal_hit, Abilities, AbilityRules, BIT_AIR_COMBO_BOOST, BIT_AUTO_CHANGE, BIT_AUTO_HEALING, BIT_AUTO_LIMIT, BIT_COMBO_BOOST, BIT_LEAF_BRACER, BIT_ONCE_MORE, BIT_REACTION_BOOST, BIT_SCAN, BIT_SECOND_CHANCE, Block, EquipRow, Info, LEAF_BRACER_FRAMES, Slot, TARGET_NOT_SCANNABLE, TARGET_NO_GAUGE, Tuning};

/// Sora lost `amount` HP to a hit (`FUN_00185a68` calls the HP method with the negative amount, after
/// Second Chance changed it). `sora::receive_hits` writes it; `apply_damage_rewards` runs the abilities.
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct SoraDamaged {
    pub amount: i32,
}

/// The HP method's side effects for the player: Damage Drive (`FUN_001792f0`, gauge mode only) fills the
/// drive gauge through `FUN_001788d8`, then MP Rage (`FUN_00176c98`) gives MP unless the MP charge runs.
#[allow(clippy::too_many_arguments)]
fn apply_damage_rewards(
    mut hits: MessageReader<SoraDamaged>,
    abilities: Res<Abilities>,
    mut sora: Query<&mut sora::Sora>,
    drive: Option<ResMut<crate::drive::DriveState>>,
    magic: Option<Res<crate::magic::MagicState>>,
    lib: Option<Res<crate::prize::PrizeLib>>,
) {
    let Ok(mut s) = sora.single_mut() else {
        hits.clear();
        return;
    };
    let mut drive = drive;
    let charging = magic.as_ref().is_some_and(|m| m.charge.active());
    let block = abilities.block();
    for h in hits.read() {
        if let (Some(d), Some(lib)) = (drive.as_deref_mut(), lib.as_ref())
            && !d.in_form
        {
            let units = damage_drive_units(block, h.amount, s.stats.max_hp);
            let gain = if units > 0.0 { lib.gauge_units(units, charging, block.drive_gain_charging) } else { 0 };
            if gain > 0 {
                d.points.gain(gain);
                if std::env::var_os("KH2_ABILITYLOG").is_some() {
                    info!("abilities: Damage Drive: {} damage gives {gain} drive units (gauge {}.{:02})", h.amount, d.points.whole, d.points.fraction);
                }
            }
        }
        let mp = mp_rage_gain(block, charging, h.amount, s.stats.max_hp);
        if mp > 0 {
            let before = s.mp;
            s.mp = (s.mp + mp).min(s.stats.max_mp);
            if std::env::var_os("KH2_ABILITYLOG").is_some() {
                info!("abilities: MP Rage: {} damage gives {mp} MP ({before} -> {})", h.amount, s.mp);
            }
        }
    }
}

fn log_on() -> bool {
    std::env::var_os("KH2_ABILITYLOG").is_some()
}

fn log_changes(a: Res<Abilities>) {
    if log_on() {
        info!("{}", a.report());
    }
}

/// The growth levels of the block as the controller's resource.
pub fn growth_of(a: &Abilities) -> Growth {
    Growth { high_jump: a.growth(0), quick_run: a.growth(1), aerial_dodge: a.growth(2), glide: a.growth(3), dodge_roll: a.growth(4) }
}

/// Writes the block into the controller's view (`sora::Abilities`: the ability bits and the combo
/// lengths) and the growth levels (`sora::Growth`) whenever the abilities change. In a form the
/// block's growth is the form's own ability; outside one `KH2_GROWTH` overrides it. Runs between
/// the drive code's swap (`Sim::Weapon`) and the controller (`Sim::Sora`), so a form's list applies
/// on the tick the body is swapped.
fn sync_controller(a: Res<Abilities>, over: Option<Res<GrowthOverride>>, mut view: ResMut<sora::Abilities>, growth: Option<ResMut<Growth>>, mut logged: Local<bool>) {
    *view = sora::Abilities::from_block(&a);
    let from_block = growth_of(&a);
    let (next, source) = match over.and_then(|o| o.0) {
        Some(o) if !a.in_form() => (o, "the KH2_GROWTH override"),
        _ if a.in_form() => (from_block, "the drive form's own list"),
        _ => (from_block, "Sora's equipped abilities"),
    };
    if let Some(mut g) = growth
        && (*g != next || !*logged)
    {
        *logged = true;
        *g = next;
        info!(
            "abilities: growth level High Jump {}, Quick Run {}, Aerial Dodge {}, Glide {}, Dodge Roll {} (from {source})",
            next.high_jump, next.quick_run, next.aerial_dodge, next.glide, next.dodge_roll
        );
    }
    if log_on() {
        info!("abilities: controller view: combo {}+finisher x{}, air {}, growth {:?}, bits {:#x}", view.max_ground, view.max_finishers, view.max_air, next, view.bits);
    }
}

/// `Abilities::follow_progress` whenever `Progress` changed; `Abilities` is only marked changed (and
/// the controller view rebuilt) when the record really moved.
fn follow_progress(p: Option<Res<crate::progress::Progress>>, mut a: ResMut<Abilities>) {
    let Some(p) = p else { return };
    if a.differs_from(&p) && a.follow_progress(&p) && log_on() {
        info!("abilities: Sora's record changed; the block is rebuilt from it");
    }
}

/// Registers `Abilities`. Needs `Disc`, so add it after `AssetsPlugin`.
pub struct AbilitiesPlugin;

impl Plugin for AbilitiesPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<SoraDamaged>();
        let rules = {
            let disc = app.world().resource::<Disc>();
            let mut a = disc.0.lock().unwrap();
            AbilityRules::load(&mut a)
        };
        let Some(rules) = rules else {
            warn!("abilities: tables not readable; the ability system is off");
            return;
        };
        crate::progress::ensure(app);
        let abilities = Abilities::with_defaults(Arc::new(rules), app.world().get_resource::<crate::progress::Progress>());
        app.insert_resource(sora::Abilities::from_block(&abilities))
            .insert_resource(abilities)
            .add_systems(Update, log_changes.run_if(resource_changed::<Abilities>))
            .add_systems(
                FixedUpdate,
                (
                    follow_progress.run_if(resource_changed::<crate::progress::Progress>),
                    sync_controller.run_if(resource_changed::<Abilities>),
                )
                    .chain()
                    .after(Sim::Weapon)
                    .before(Sim::Sora),
            )
            .add_systems(FixedUpdate, apply_damage_rewards.in_set(Sim::SoraHurt).after(sora::receive_hits));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::KINGDOM_KEY;
    use kh2_tables::tables as t;

    fn rules() -> Option<Arc<AbilityRules>> {
        let iso = std::env::var("KH2_ISO").ok().or_else(|| {
            let f = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../local/iso.txt");
            std::fs::read_to_string(f).ok().map(|s| s.trim().to_string())
        });
        let Some(iso) = iso else {
            eprintln!("skipped: no disc (set KH2_ISO)");
            return None;
        };
        let mut a = kh2_archive::Archive::open(&iso).map_err(|e| eprintln!("skipped: {iso}: {e}")).ok()?;
        AbilityRules::load(&mut a).map(Arc::new)
    }

    fn with(r: &Arc<AbilityRules>, ids: &[u16]) -> Abilities {
        let mut a = Abilities::new(r.clone());
        a.equipment.clear();
        a.recompute();
        for id in ids {
            a.equip_ability(*id);
        }
        a
    }

    #[test]
    fn defaults_match_the_block_initialiser() {
        let Some(r) = rules() else { return };
        let a = with(&r, &[]);
        assert_eq!(a.block(), &Block::default());
        assert_eq!((a.ground_combo(), a.air_combo(), a.finishers()), (3, 3, 1));
        assert_eq!(a.bits(), 0);
    }

    #[test]
    fn combo_abilities_change_the_lengths() {
        let Some(r) = rules() else { return };
        let a = with(&r, &[id::COMBO_PLUS, id::AIR_COMBO_PLUS, id::FINISHING_PLUS]);
        assert_eq!((a.ground_combo(), a.air_combo(), a.finishers()), (4, 4, 2));
        // The bits are the attack table's gates: Combo Plus 21, Air Combo Plus 22, Finishing Plus 105.
        assert!(a.has_bit(21) && a.has_bit(22) && a.has_bit(105));
        let two = with(&r, &[id::COMBO_PLUS, id::COMBO_PLUS, id::NEGATIVE_COMBO]);
        assert_eq!((two.ground_combo(), two.air_combo()), (4, 2));
        assert!(two.has(id::NEGATIVE_COMBO));
    }

    #[test]
    fn every_stat_ability_changes_the_block() {
        let Some(r) = rules() else { return };
        let all = [
            id::COMBO_PLUS, id::AIR_COMBO_PLUS, id::FINISHING_PLUS, id::NEGATIVE_COMBO, id::COMBO_BOOST,
            id::AIR_COMBO_BOOST, id::REACTION_BOOST, id::BERSERK_CHARGE, id::DAMAGE_DRIVE, id::DRIVE_BOOST,
            id::FORM_BOOST, id::SUMMON_BOOST, id::COMBINATION_BOOST, id::EXPERIENCE_BOOST, id::DRAW, id::JACKPOT,
            id::LUCKY_LUCKY, id::FIRE_BOOST, id::BLIZZARD_BOOST, id::THUNDER_BOOST, id::ITEM_BOOST, id::MP_RAGE,
            id::MP_HASTE, id::MP_HASTERA, id::MP_HASTEGA, id::DEFENDER, id::HYPER_HEALING,
        ];
        let base = Block::default();
        for id in all {
            let a = with(&r, &[id]);
            let mut want_bit = a.block().clone();
            want_bit.bits = [0; 8];
            assert_ne!(want_bit, base, "ability {id} must change a stat field");
            assert!(a.has(id), "ability {id} must set its bit");
            eprintln!("ability {id}: {}", a.block().deltas().join("; "));
        }
        // Steps come from sstm: Form Boost multiplies the drain by less than 1.
        let a = with(&r, &[id::FORM_BOOST, id::FORM_BOOST]);
        assert!(a.block().form_drain < 1.0 && a.block().form_drain > 0.0);
        let hy = with(&r, &[id::MP_HASTE]);
        assert!(hy.block().mp_regen > 1.0);
    }

    #[test]
    fn growth_levels_follow_the_highest_ability() {
        let Some(r) = rules() else { return };
        // Growth ids 94..=109 are four groups of four levels (items verified against the disc).
        let a = with(&r, &[94, 95, 98, 106, 107, 108, 109]);
        assert_eq!((a.growth(0), a.growth(1), a.growth(2), a.growth(3)), (2, 1, 0, 4));
        assert_eq!(with(&r, &[]).growth(0), 0);
        assert_eq!(r.growth_bits[0], [2, 3, 4, 5]);
        assert_eq!(r.growth_bits[3], [14, 15, 16, 17]);
    }

    /// The growth rule with the disc's rows: a group's level is the highest *equipped* level
    /// (`FUN_001c0e50` applies a slot only with bit 15, `FUN_001c1340` takes the highest set bit of the
    /// group). Quick Run is 98..=101, Glide 106..=109.
    #[test]
    fn growth_level_is_the_highest_equipped_level() {
        let Some(r) = rules() else { return };
        // Every growth row is a kind 3 (Growth group) ability with a name; print the costs.
        for g in 0..4 {
            for l in 1..=4u8 {
                let id = r.growth_ability(g, l).expect("growth row");
                let i = r.abilities[&id];
                assert_eq!(i.kind, 3, "id {id} is a growth ability");
                eprintln!("growth group {g} LV{l}: id {id}, bit {}, AP {}", i.bit, i.ap);
            }
        }
        let mut a = with(&r, &[98, 99, 100]);
        assert_eq!(a.growth(1), 3);
        a.unequip_ability(100);
        assert_eq!(a.growth(1), 2, "LV3 taken off: LV2 is the highest equipped");
        a.unequip_ability(98);
        assert_eq!(a.growth(1), 2, "LV1 off changes nothing while LV2 is on");
        a.unequip_ability(99);
        assert_eq!(a.growth(1), 0, "nothing equipped: not learned");
        // Learned but not equipped counts for nothing; equipping it again restores the level.
        a.learned.push(Slot { id: 109, equipped: false });
        a.recompute();
        assert_eq!(a.growth(3), 0);
        a.learned.last_mut().unwrap().equipped = true;
        a.recompute();
        assert_eq!(a.growth(3), 4);
        assert_eq!(growth_of(&a), Growth { high_jump: 0, quick_run: 0, aerial_dodge: 0, glide: 4, dodge_roll: 0 });
        // A lower level equipped beside a higher one: the higher wins (the loop runs from the top).
        let b = with(&r, &[106, 108]);
        assert_eq!(b.growth(3), 3);
    }

    /// The block follows Sora's record: a menu toggle, a growth upgrade or a gift written to `Progress`
    /// reaches the growth levels; the harness's own additions stay.
    #[test]
    fn the_block_follows_the_record() {
        let Some(r) = rules() else { return };
        let mut p = crate::progress::Progress::from_block(&[], Arc::new(crate::progress::ItemIndex::default()));
        p.apply_json(r#"{"abilities": [82, 94], "abilities_off": [98], "dodge_roll": 1}"#).unwrap();
        let mut a = Abilities::with_defaults(r.clone(), Some(&p));
        a.equip_ability(id::COMBO_PLUS);
        assert_eq!((a.growth(0), a.growth(1), a.growth(4)), (1, 0, 1));
        assert!(!a.differs_from(&p));
        assert!(!a.follow_progress(&p), "nothing moved: no rebuild");
        // The Abilities menu's toggle (FUN_0027d8b8) on the Quick Run slot.
        let slot = p.ability_slots(crate::progress::SORA).iter().find(|s| s.1 == 98).unwrap().0;
        p.set_ability_equipped(crate::progress::SORA, slot, true);
        assert!(a.differs_from(&p));
        assert!(a.follow_progress(&p));
        assert_eq!(a.growth(1), 1);
        // A form level-up's growth upgrade (FUN_001d3f18): LV1 -> LV2 keeps the equipped bit.
        p.upgrade_ability(crate::progress::SORA, 98, 99);
        a.follow_progress(&p);
        assert_eq!(a.growth(1), 2);
        // Taking High Jump off.
        let hj = p.ability_slots(crate::progress::SORA).iter().find(|s| s.1 == 94).unwrap().0;
        p.set_ability_equipped(crate::progress::SORA, hj, false);
        a.follow_progress(&p);
        assert_eq!(a.growth(0), 0);
        assert!(a.has(id::COMBO_PLUS) && a.ground_combo() == 4, "the harness's Combo Plus stays");
        assert_eq!(a.learned.len(), 4);
    }

    #[test]
    fn form_hides_action_and_growth_abilities_of_the_player() {
        let Some(r) = rules() else { return };
        let mut a = with(&r, &[id::GUARD, 94, id::COMBO_PLUS]);
        assert!(a.has(id::GUARD) && a.growth(0) == 1 && a.ground_combo() == 4);
        // In a form only support (kind 0) abilities of the player stay; the form's own list applies.
        a.set_form(Some(vec![id::AIR_COMBO_PLUS, 95]));
        assert!(!a.has(id::GUARD));
        assert_eq!(a.growth(0), 2);
        assert_eq!((a.ground_combo(), a.air_combo()), (4, 4));
        a.set_form(None);
        assert!(a.has(id::GUARD) && a.growth(0) == 1);
    }

    #[test]
    fn dodge_roll_is_the_fifth_growth_kind_and_hidden_in_a_form() {
        let Some(r) = rules() else { return };
        let mut a = with(&r, &[id::GUARD]);
        assert_eq!(a.growth(4), 0, "not learned by default");
        a.dodge_roll = 1;
        assert_eq!(a.growth(4), 1);
        assert_eq!(growth_of(&a).dodge_roll, 1);
        a.set_form(Some(vec![95]));
        assert_eq!(a.growth(4), 0, "a form's growth is its own ability");
        a.set_form(None);
        assert_eq!(a.growth(4), 1);
    }

    #[test]
    fn kingdom_key_grants_its_own_ability_and_ap_is_counted() {
        let Some(r) = rules() else { return };
        let a = Abilities::new(r.clone());
        eprintln!("{}", a.report());
        eprintln!("{}", with(&r, &[id::COMBO_PLUS, id::FORM_BOOST, 94, 95]).report());
        let row =r.equipment_row(KINGDOM_KEY).expect("Kingdom Key row");
        assert_ne!(row.ability, 0);
        assert!(a.has(row.ability));
        let (s, m, _) = a.equipment_stats();
        assert_eq!((s, m), (row.strength as i32, row.magic as i32));
        let mut b = with(&r, &[id::COMBO_PLUS, id::GUARD]);
        assert_eq!(b.ap_used(), r.abilities[&id::COMBO_PLUS].ap as u16 + r.abilities[&id::GUARD].ap as u16);
        b.unequip_ability(id::GUARD);
        assert!(!b.has(id::GUARD));
        assert_eq!(b.ap_max(), r.ap_by_level[0]);
    }

    #[test]
    fn damage_boosts_follow_the_routine() {
        let Some(r) = rules() else { return };
        // The bits the damage routine tests are the rows' bits.
        assert_eq!(r.abilities[&id::COMBO_BOOST].bit, BIT_COMBO_BOOST);
        assert_eq!(r.abilities[&id::AIR_COMBO_BOOST].bit, BIT_AIR_COMBO_BOOST);
        assert_eq!(r.abilities[&id::REACTION_BOOST].bit, BIT_REACTION_BOOST);
        let atk = |element: u8, kind: u8| { let mut a = <t::Atkp as kh2_tables::Record>::parse(&mut kh2_tables::rt::Reader::new(&[0u8; 48])); a.element = element; a.kind = kind; a };
        let none = with(&r, &[]);
        assert_eq!(attacker_bonuses(none.block(), 3, &atk(1, 7), 100), 100);
        // Fire Boost scales a Fire attack only.
        let fire = with(&r, &[id::FIRE_BOOST]);
        let f = fire.block().fire as i32;
        assert!(f > 100);
        assert_eq!(attacker_bonuses(fire.block(), 1, &atk(1, 0), 100), (100 * f + 99) / 100);
        assert_eq!(attacker_bonuses(fire.block(), 1, &atk(2, 0), 100), 100);
        // Combo Boost: per combo hit, capped at the ground length, only for rows with Kind bit 0.
        let cb = with(&r, &[id::COMBO_BOOST]);
        let step = cb.block().combo_boost as i32;
        assert!(step > 0);
        assert_eq!(attacker_bonuses(cb.block(), 2, &atk(0, 1), 100), 100 + (100 * step * 2 + 99) / 100);
        assert_eq!(attacker_bonuses(cb.block(), 9, &atk(0, 1), 100), 100 + (100 * step * 3 + 99) / 100);
        assert_eq!(attacker_bonuses(cb.block(), 2, &atk(0, 2), 100), 100);
        // Reaction Boost: a flat percent for Kind bit 2; Combo Boost takes precedence when both match.
        let rb = with(&r, &[id::REACTION_BOOST, id::COMBO_BOOST]);
        let rstep = rb.block().reaction_boost as i32;
        assert_eq!(attacker_bonuses(rb.block(), 1, &atk(0, 4), 100), 100 + (100 * rstep + 99) / 100);
        assert_eq!(attacker_bonuses(rb.block(), 1, &atk(0, 5), 100), 100 + (100 * step + 99) / 100);
        // The form chain's growth abilities by group and level.
        assert_eq!(r.growth_ability(0, 1), Some(94));
        assert_eq!(r.growth_ability(3, 4), Some(109));
        assert_eq!(r.growth_ability(0, 0), None);
    }

    #[test]
    fn defender_applies_only_at_low_hp() {
        // FUN_001a0be0: HP at or below a quarter of the maximum (integer shift).
        assert!(hp_is_low(25, 100) && !hp_is_low(26, 100));
        assert!(hp_is_low(0, 3) && !hp_is_low(1, 3), "3 >> 2 is 0");
        assert!(hp_is_low(-1, -3), "a negative maximum adds 3 before the shift");
        let mut b = Block::default();
        assert_eq!(defender_bonus(&b, 10, 100), 0, "no Defender: the byte is 0");
        b.defense_bonus = 4;
        assert_eq!(defender_bonus(&b, 25, 100), 4);
        assert_eq!(defender_bonus(&b, 80, 100), 0, "healthy: nothing");
        b.defense_bonus = -1;
        assert_eq!(defender_bonus(&b, 1, 100), 255, "the game reads the byte unsigned (lbu)");
        let Some(r) = rules() else { return };
        let d = with(&r, &[id::DEFENDER]);
        assert!(d.block().defense_bonus > 0);
        assert_eq!(defender_bonus(d.block(), 10, 100), i32::from(d.block().defense_bonus));
    }

    #[test]
    fn berserk_charge_adds_strength_only_while_charging() {
        let mut b = Block::default();
        assert_eq!(berserk_attack(&b, true), 0);
        b.berserk_charge = 7;
        assert_eq!(berserk_attack(&b, true), 7);
        assert_eq!(berserk_attack(&b, false), 0);
        assert_eq!(berserk_for_row(&b, true, 0), 7);
        assert_eq!(berserk_for_row(&b, true, 1), 0, "the magic stat rows read no strength");
        let Some(r) = rules() else { return };
        let a = with(&r, &[id::BERSERK_CHARGE]);
        assert!(a.block().berserk_charge > 0);
        assert_eq!(berserk_attack(a.block(), true), i32::from(a.block().berserk_charge));
    }

    #[test]
    fn mp_rage_and_damage_drive_follow_the_hp_method() {
        let mut b = Block::default();
        assert_eq!(mp_rage_gain(&b, false, 50, 100), 0, "no MP Rage: nothing");
        assert_eq!(damage_drive_units(&b, 50, 100), 0.0);
        b.mp_rage = 40.0;
        b.damage_drive = 10.0;
        assert_eq!(mp_rage_gain(&b, false, 50, 100), 20);
        assert_eq!(mp_rage_gain(&b, false, 1, 100), 0, "0.4 truncates to 0");
        assert_eq!(mp_rage_gain(&b, true, 50, 100), 0, "not while the MP charge runs");
        assert_eq!(mp_rage_gain(&b, false, 0, 100), 0);
        assert_eq!(damage_drive_units(&b, 25, 100), 2.5);
        assert_eq!(damage_drive_units(&b, 0, 100), 0.0);
        assert_eq!(damage_drive_units(&b, 5, 0), 0.0);
        let Some(r) = rules() else { return };
        let a = with(&r, &[id::MP_RAGE, id::DAMAGE_DRIVE]);
        assert!(a.block().mp_rage > 0.0 && a.block().damage_drive > 0.0);
        assert!(mp_rage_gain(a.block(), false, 100, 100) > 0);
        assert!(damage_drive_units(a.block(), 100, 100) > 0.0);
    }

    #[test]
    fn combat_ability_bits_are_the_rows_bits() {
        let Some(r) = rules() else { return };
        for (id, bit) in [(402, BIT_LEAF_BRACER), (415, BIT_SECOND_CHANCE), (416, BIT_ONCE_MORE), (417, BIT_AUTO_LIMIT), (418, BIT_AUTO_CHANGE), (420, BIT_AUTO_HEALING), (138, BIT_SCAN)] {
            assert_eq!(r.abilities[&id].bit, bit, "ability {id}");
        }
    }

    #[test]
    fn scan_gate_follows_fun_0019b4a8() {
        assert!(scan_binds_gauge(true, true, true, 0, true));
        assert!(scan_binds_gauge(true, true, true, 0x3, true), "other +0x108 bits do not matter");
        assert!(!scan_binds_gauge(true, false, true, 0, true), "no Scan");
        assert!(!scan_binds_gauge(false, true, true, 0, true));
        assert!(!scan_binds_gauge(true, true, false, 0, true));
        assert!(!scan_binds_gauge(true, true, true, 0, false));
        for f in [0x4000, 0x8_0000, 0x1000_0000] {
            assert!(!scan_binds_gauge(true, true, true, f, true), "flag {f:#x}");
        }
    }

    #[test]
    fn lethal_hit_rules() {
        // Second Chance: HP 2 or more survives with 1; at HP 1 it does not.
        assert!(survives_lethal_hit(true, false, 2, None));
        assert!(!survives_lethal_hit(true, false, 1, None));
        assert_eq!(lethal_hit_damage(5, 0, 9, survives_lethal_hit(true, false, 5, None)), 4);
        // Once More: only in a damage motion 10..=39.
        assert!(survives_lethal_hit(false, true, 1, Some(10)) && survives_lethal_hit(false, true, 1, Some(39)));
        assert!(!survives_lethal_hit(false, true, 1, Some(40)) && !survives_lethal_hit(false, true, 1, None));
        assert_eq!(lethal_hit_damage(1, 0, 3, true), 0);
        // Not lethal: unchanged.
        assert_eq!(lethal_hit_damage(10, 0, 3, true), 3);
        assert_eq!(leaf_bracer_frames(true, true), Some(60.0));
        assert_eq!(leaf_bracer_frames(true, false), None);
    }

    #[test]
    fn env_bits_equip_the_ability_that_owns_the_bit() {
        let Some(r) = rules() else { return };
        assert_eq!(r.id_of_bit(21), Some(id::COMBO_PLUS));
        assert_eq!(r.id_of_bit(100), Some(272));
        assert_eq!(r.id_of_bit(1), Some(id::GUARD));
    }
}
