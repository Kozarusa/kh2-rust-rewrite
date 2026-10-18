//! The player's game state: what the original keeps in its save block for the party. One resource,
//! `Progress`, holds the inventory (spells, form items, summon charms and keyblades are items), the
//! party records (equipment, abilities, stats), the drive form records (level, experience, second
//! keyblade), the summon level and experience, munny, the per-world outfit bytes and the per-spell
//! switch bits. `docs/progress.md` has the layout with the addresses.
//!
//! `Progress` is the original's party block itself (`0x33fec0`, 0x1998 bytes; the save block is
//! `0x33e860`), with typed accessors. Keeping the bytes means every rule here is a port of the
//! routine that reads or writes them, and the kh2-ee oracle test compares the whole block with
//! the game's own code.
//!
//! Start state, in this order:
//! 1. The new game: the save block is cleared at boot (`FUN_00105638(0x33e860)`), the party block
//!    is set up (`FUN_00187b38`) and every `plrp` row with `Id` 0 is applied (`FUN_00187d20` ->
//!    `FUN_001c0cb0(0)`). Ported here and read from the user's disc (`plrp`, `item`, `cmd`).
//! 2. `KH2_STATE=<path>`: a JSON file applied on top of the new game (schema in docs/progress.md).
//!    Nothing else is added then.
//! 3. Without `KH2_STATE`: the app baseline, which is NOT the original's new game: the spells
//!    Fire, Blizzard, Thunder and Cure at level 1, the four form items (with the spare Kingdom Keys
//!    the forms take), the Kingdom Key equipped and Guard (`KH2_ABILITY_START=data` drops Guard).
//!    The app had these defaults before `Progress`; they stay until the story gives them.
//!
//! The systems read `Progress` and still apply their own environment variables on top
//! (`KH2_MAGIC`, `KH2_DRIVE_*`, `KH2_ABILITIES`, ...). `KH2_PROGRESSLOG=1` logs the state at start.

// The API serves later work packages (items, prizes, magic, summons); not all of it has a reader yet.
#![allow(dead_code)]

use std::collections::BTreeMap;
use std::sync::Arc;

use bevy::prelude::*;
use kh2_tables::tables::{self as t, Cmd, Item, Magc, Plrp};

use crate::assets::Disc;
use crate::data::KINGDOM_KEY;

/// EE addresses of the blocks (for the docs and the oracle test).
pub mod addr {
    /// The save block: `FUN_00105638(0x33e860)` clears 0xb4e0 bytes from here at boot.
    pub const SAVE: u32 = 0x0033_e860;
    pub const SAVE_LEN: u32 = 0xb4e0;
    /// The party block (save + 0x1660), set up by `FUN_00187b38`.
    pub const PARTY: u32 = 0x0033_fec0;
}

/// Byte offsets in the party block.
pub mod off {
    /// Party member records: `FUN_00187d40(0x33fec0, member)` = `0x33fec0 + (index - 1) * 0xf4`.
    pub const MEMBER_SIZE: usize = 0xf4;
    pub const MEMBERS: usize = 13;
    /// Form records: `FUN_00187f60(0x33fec0, f)` = `0x33fec0 + 0xc3c + f * 0x28`, `f` 1..=9.
    pub const FORM_BASE: usize = 0xc3c;
    pub const FORM_SIZE: usize = 0x28;
    pub const FORMS: usize = 9;
    /// `0x340c88` + 4: the current form (`FUN_001b4c28`).
    pub const CURRENT_FORM: usize = 0xdcc;
    /// `0x340c88` + 5: the active summon (1-based `sumn` index), + 6 the summon level, + 7 the
    /// Genie form / Tinker Bell byte.
    pub const SUMMON: usize = 0xdcd;
    pub const SUMMON_LEVEL: usize = 0xdce;
    pub const SUMMON_EXTRA: usize = 0xdcf;
    /// Saved drive gauge bytes `+0xdd0..+0xdd3` and floats `+0xdd4`, `+0xdd8` (`FUN_0019f920`
    /// copies them to status `+0x1b0..+0x1b8`).
    pub const DRIVE: usize = 0xdd0;
    /// World records `0x340c9c + 4 * world` (`FUN_001a71c0`), 19 of them.
    pub const WORLDS: usize = 0xddc;
    pub const WORLD_COUNT: usize = 19;
    /// Item counts `0x340ce8`, one byte per item `Slot` (`FUN_001a1cc8`).
    pub const COUNTS: usize = 0xe28;
    pub const COUNT_SLOTS: usize = 0x140;
    /// Key-item bits `0x340e28` (items with `Flag0` bit 0), by `Slot`.
    pub const KEYS: usize = 0xf68;
    pub const KEY_WORDS: usize = 8;
    /// Experience total `0x340e48` (`FUN_001b51e0`, followed by the level-up check `FUN_001b52f8`).
    pub const EXPERIENCE: usize = 0xf88;
    /// Summon experience `0x340e4c` (`FUN_001d4ec8`).
    pub const SUMMON_EXP: usize = 0xf8c;
    /// Consumables Sora received, 8 ids (`FUN_001a2120`).
    pub const RECENT: usize = 0xf90;
    /// Four command shortcuts (`0x340e60`; `FUN_001a2120` and `FUN_001a1a18` fill empty ones).
    pub const SHORTCUTS: usize = 0xfa0;
    /// Per-spell switch bits `0x340eac` (`FUN_001a4b30`, `FUN_001a4b68`, `FUN_001a4bb8`).
    pub const SPELL_SWITCH: usize = 0xfec;
    /// "Obtained once" bits `0x3414f0`, by item `Slot` (`FUN_001a18d8`).
    pub const OBTAINED: usize = 0x1630;
    pub const OBTAINED_WORDS: usize = 10;
    /// Length of the party block as `FUN_00187b38` touches it.
    pub const LEN: usize = 0x1998;
}

/// Offsets in a party member record.
mod rec {
    pub const WEAPONS: usize = 0x0;
    pub const HP: usize = 0x4;
    pub const HP_MAX: usize = 0x5;
    pub const MP: usize = 0x6;
    pub const MP_MAX: usize = 0x7;
    pub const AP: usize = 0x8;
    pub const STRENGTH: usize = 0x9;
    pub const MAGIC: usize = 0xa;
    pub const DEFENSE: usize = 0xb;
    pub const WEAPON_SET: usize = 0xe;
    pub const LEVEL: usize = 0xf;
    pub const ARMOR_SLOTS: usize = 0x10;
    pub const ACCESSORY_SLOTS: usize = 0x11;
    pub const ITEM_SLOTS: usize = 0x12;
    pub const ARMOR: usize = 0x14;
    pub const ACCESSORIES: usize = 0x24;
    pub const ITEMS: usize = 0x34;
    pub const RESERVED: usize = 0x44;
    pub const ABILITIES: usize = 0x54;
    pub const ABILITY_SLOTS: usize = 64;
    /// The AI settings block (FUN_00112308): byte 0 the battle style, bytes 1.. the item / ability use settings.
    pub const SETTINGS: usize = 0xd4;
    pub const SETTINGS_LEN: usize = 32;
}

/// Item types the rules switch on (`item` +2).
pub mod item_type {
    pub const CONSUMABLE: u8 = 0;
    pub const KEYBLADE: u8 = 2;
    /// Armor and accessory rows (the filter words of `FUN_00283810`: bits 0x4000 and 0x8000).
    pub const ARMOR: u8 = 14;
    pub const ACCESSORY: u8 = 15;
    pub const MAGIC: u8 = 0x12;
    pub const ABILITY: u8 = 0x13;
    pub const DRIVE_FORM: u8 = 0x15;
}

/// The "inventory" target id (`FUN_001a1768` / `FUN_001a18d8` with 100: no party member).
pub const INVENTORY: u16 = 100;
/// Count limit of an item (`FUN_001a1768`: a count above 99 refuses the item).
pub const MAX_COUNT: u8 = 99;
/// The command shortcuts of the Customize page (save `+0x2600`, four u16).
pub const SHORTCUTS: usize = 4;
/// Experience limit (`FUN_001b51e0`).
pub const MAX_EXPERIENCE: i32 = 9_999_999;

/// One level a member reached in `Progress::raise_level` (`FUN_001b53d0`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LevelReached {
    pub member: u16,
    /// The new level.
    pub level: u8,
    /// The rises of the new `lvup` row over the previous one (a message is queued for each above 0).
    pub strength: i16,
    pub magic: i16,
    pub defense: i16,
    pub ap: i16,
    /// The row's ability for the member's weapon set (0 for none) and whether `give` took it.
    pub ability: u16,
    pub given: bool,
}

/// What `Progress::pay_experience` leaves: the levels reached in slot order and Sora's remaining experience.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ExperiencePaid {
    pub to_next: i32,
    pub levels: Vec<LevelReached>,
}

/// `FUN_001b50c0(member, level)`: the `lvup` row of a member's level. The table is the member's own (ids 14 and
/// 15 use 1 and 6), the first one when his is missing or empty, and the level is held at the table's length
/// (row `level - 1`); level 0 counts as 1 [inferred, as `menu_equipment::member_ap_max`].
pub fn lvup_row(tables: &[Vec<t::Lvup>], member: u16, level: u8) -> Option<&t::Lvup> {
    let k = match member {
        14 => 1,
        15 => 6,
        n @ 1..=13 => n as usize,
        _ => 1,
    };
    let table = tables.get(k - 1).filter(|t| !t.is_empty()).or_else(|| tables.first())?;
    table.get((level.max(1) as usize).min(table.len()).checked_sub(1)?)
}

/// The experience a defeated enemy pays (`FUN_001b25d0` step 2): `ceil(table * row / 100)` where `table` is the
/// `lvpm` Experience of the world level (`FUN_001beaf0`, word `+10` of its 12-byte row) and `row` the enemy's
/// `enmp` Experience (record `+0x56`). Nothing when that is 0 or less. When the killer is the player, has bit
/// 0x80 of his word `+0x588` and his HP fraction is at or below the float at `sstm +0x144`, the amount is
/// multiplied by the ability block's byte `+0x2b` (`exp_multiplier`: 1, plus 1 per Experience Boost).
pub fn defeat_payout(table: u16, row: u16, low_hp_multiplier: Option<i32>) -> i32 {
    let amount = (i32::from(table) * i32::from(row) + 99) / 100;
    if amount <= 0 {
        return 0;
    }
    amount * low_hp_multiplier.unwrap_or(1)
}

/// The experience popup request (`FUN_0015b198(position, amount, to_next)`): `hud_banner` projects `at` (the
/// defeated enemy's point, `FUN_0016bbe8`: shape kind 0x11 of the model or else the object's position, in
/// world units) to the screen, shows the amount (0xae0-byte digit popup) and the "NEXT LEVEL" strip with
/// `to_next` (`FUN_0014f2b8`).
#[derive(Message, Clone, Copy, Debug, PartialEq)]
pub struct ExperienceGained {
    pub amount: u32,
    pub to_next: u32,
    pub at: Vec3,
}

/// Shape kind of the popup's point (`FUN_0016bbe8` asks `FUN_0016b558(object, 0x11, -1)`).
const POPUP_SHAPE: u8 = 0x11;

/// `FUN_001b25d0` steps 2 for every `EnemyDefeated`: the payout of `defeat_payout` (the lvpm row of the world
/// level 1 the app builds all enemies with, the enemy's `enmp` Experience), `Progress::pay_experience`, the
/// obtained-log record of each ability a level gave (`FUN_001d0828(ability, member, 3)`), the level-up system
/// effect 0xf on Sora (`FUN_001b54e8`, once, when he reached a level), and, when Sora still needs experience
/// (`FUN_001b51e0` answered above 0), the popup request. Not ported: `FUN_001b5148`'s extra tests (a minigame
/// manager, status flag 0x100000), the level-up window (`FUN_001b56a8`), the status rebuild of the actor
/// (`FUN_0019fb98`), and the effect for members other than Sora.
#[allow(clippy::too_many_arguments)]
pub fn pay_defeats(
    mut defeats: MessageReader<EnemyDefeated>,
    data: Option<Res<crate::data::GameData>>,
    tables: Option<Res<ExperienceTables>>,
    mut progress: Option<ResMut<Progress>>,
    abilities: Option<Res<crate::abilities::Abilities>>,
    sora: Query<&crate::sora::Sora>,
    enemies: Query<(&crate::enemy::Enemy, &GlobalTransform)>,
    bodies: Query<(&crate::skel::Animator, &GlobalTransform)>,
    (mut log, area, mut shown, mut fx): (
        ResMut<crate::prize::ObtainedLog>,
        Option<Res<crate::assets::CurrentArea>>,
        MessageWriter<ExperienceGained>,
        MessageWriter<crate::fx::StartBoundSystemEffect>,
    ),
) {
    let (Some(data), Some(tables), Some(progress)) = (data, tables, progress.as_deref_mut()) else {
        defeats.clear();
        return;
    };
    let place = area.as_ref().map_or((0, 0), |a| crate::prize::place_bytes(&a.name));
    for d in defeats.read() {
        let Some(row) = data.enmp.iter().find(|e| e.id == d.enmp) else { continue };
        let table = data.lvpm.first().map_or(0, |l| l.experience);
        // The bonus: the killer is the player and his HP fraction is at or below the sstm float.
        let low_hp = sora.single().ok().filter(|s| d.by_player && s.stats.max_hp > 0 && s.hp as f32 / s.stats.max_hp as f32 <= tables.low_hp_fraction);
        let multiplier = low_hp.map(|_| abilities.as_ref().map_or(1, |a| i32::from(a.block().exp_multiplier)));
        let amount = defeat_payout(table, row.experience, multiplier);
        if amount <= 0 {
            info!("experience: enmp {} pays nothing (table {table}, row {})", d.enmp, row.experience);
            continue;
        }
        let before = progress.experience();
        let paid = progress.pay_experience(amount, &tables.lvup, SORA);
        info!(
            "experience: enmp {} defeated: +{amount} (lvpm {table} x enmp {}{}); total {before} -> {}; Sora needs {} more",
            d.enmp,
            row.experience,
            multiplier.map_or(String::new(), |m| format!(", low-HP x{m}")),
            progress.experience(),
            paid.to_next
        );
        for l in &paid.levels {
            info!(
                "experience: member {} reaches level {} (Strength +{}, Magic +{}, Defense +{}, AP +{}); ability {:#x}{}",
                l.member,
                l.level,
                l.strength.max(0),
                l.magic.max(0),
                l.defense.max(0),
                l.ap.max(0),
                l.ability,
                if l.ability == 0 { " (none)" } else if l.given { " given" } else { " refused" }
            );
            if l.ability != 0 {
                let key = progress.items().get(l.ability).is_some_and(|i| i.is_key());
                log.push(crate::prize::obtained_record(place, l.ability, key, l.given, l.member as u8, 3));
            }
        }
        if let (Some(s), true) = (sora.single().ok(), paid.levels.iter().any(|l| l.member == SORA)) {
            fx.write(crate::fx::StartBoundSystemEffect { id: LEVEL_UP_SYSTEM_EFFECT, owner: s.body, handle: crate::fx::FxHandle::new() });
        }
        if paid.to_next > 0 {
            let at = enemies.get(d.victim).ok().map(|(e, gt)| {
                bodies
                    .get(e.body)
                    .ok()
                    .and_then(|(a, g)| crate::prize::shape_point(a, g, POPUP_SHAPE))
                    .map_or(gt.translation(), |p| p * crate::SCALE)
            });
            if let Some(at) = at {
                shown.write(ExperienceGained { amount: amount as u32, to_next: paid.to_next as u32, at });
            }
        }
    }
}

/// The system effect `FUN_001b54e8` starts on the member's object when a level was reached
/// (`FUN_001db868(.., 0xf, 1, 1, 0x2715, object)`); the form level-up uses 0x10 (`prize::LEVEL_UP_EFFECT`).
pub const LEVEL_UP_SYSTEM_EFFECT: u16 = 0xf;

/// The state-free tables the payout reads, from the disc: every character's `lvup` rows and the float at
/// `sstm +0x144` (the HP fraction of the bonus). Read by `ProgressPlugin`.
#[derive(Resource, Clone, Debug)]
pub struct ExperienceTables {
    pub lvup: Arc<Vec<Vec<t::Lvup>>>,
    pub low_hp_fraction: f32,
}

/// An enemy defeat (`FUN_001b25d0`'s entry): sent by `enemy::receive_hits` the moment a hit takes the last HP.
/// `enmp` is the row of the enemy's kind (a script's `set_obj_param` row is not followed, docs/progress.md);
/// `by_player` is whether the hit's attacker was Sora.
#[derive(Message, Clone, Copy, Debug)]
pub struct EnemyDefeated {
    pub victim: Entity,
    pub enmp: u16,
    pub by_player: bool,
}
/// Sora's party member id.
pub const SORA: u16 = 1;
/// Donald's and Goofy's party member ids (`FUN_001b58d8`: ids 1..=13 are their own record index; the
/// spawn rows' NeoStatus 2 and 3).
pub const DONALD: u16 = 2;
pub const GOOFY: u16 = 3;

/// The weapon words the game object's constructor leaves in the party records and form records at every boot:
/// `00common.bdx` entry event 7 (routine R00c1) calls host 0.75 (`set_member_weapons`, `FUN_0016d9d8` -> `FUN_00187d88`)
/// once per member and host 0.76 (`set_form_weapon`, `FUN_0016da08`) for the forms, and `FUN_00194d78` runs it from
/// `FUN_00105638`. Read from the user's disc by running that event on a recording host; nothing is copied into the app.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BootWeapons {
    /// `(member, word at record +0, word at +2)` in call order.
    pub members: Vec<(u16, u16, u16)>,
    /// `(form, word at the form record's +0)` in call order.
    pub forms: Vec<(u8, u16)>,
}

/// A host for `00common.bdx` event 7 that records hosts 0.75 and 0.76 and answers 0 to everything else.
struct BootHost(BootWeapons);

impl kh2_bdx::Host for BootHost {
    fn call(&mut self, c: &kh2_bdx::Call) -> kh2_bdx::Value {
        let (id, args): ((u16, u16), Vec<i32>) = match c {
            kh2_bdx::Call::Func { group, index, args } => ((*group, *index), args.iter().map(|a| a.int()).collect()),
            kh2_bdx::Call::Unknown { group, index, args } => ((*group, *index), args.iter().map(|&a| a as i32).collect()),
            _ => return kh2_bdx::Value::Int(0),
        };
        match (id, args.as_slice()) {
            ((0, 75), &[member, w0, w1]) => self.0.members.push((member as u16, w0 as u16, w1 as u16)),
            ((0, 76), &[form, w0]) => self.0.forms.push((form as u8, w0 as u16)),
            _ => {}
        }
        kh2_bdx::Value::Int(0)
    }
}

impl BootWeapons {
    /// Runs event 7 (as a thread, to its end) of the disc's `00common.bdx` and keeps the weapon words it sets. `None` when the script is unreadable
    /// or the event sets nothing.
    pub fn read(disc: &Disc) -> Option<BootWeapons> {
        let prog = kh2_bdx::Program::parse(&disc.read("00common.bdx")?).ok()?;
        let mut rt = kh2_bdx::Runtime::new(prog);
        if let Some(table) = disc.read_root("SLUS_210.05").and_then(|exe| kh2_bdx::HostTable::from_exe(&exe)) {
            rt.set_host_table(table);
        }
        let mut host = BootHost(BootWeapons::default());
        // Event 7 waits once (host 0.14) before its calls, so it runs as a thread until it ends (`FUN_00194d78`).
        let ran = rt.start_thread(0, 7, &[]);
        for _ in 0..64 {
            if rt.thread_done() {
                break;
            }
            rt.update(&mut host);
        }
        if !ran || host.0.members.is_empty() {
            return None;
        }
        Some(host.0)
    }
}

/// What the rules read of one item row (`item`, 0x18 bytes, found by `FUN_001a66e8`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ItemInfo {
    pub id: u16,
    pub kind: u8,
    pub flag0: u8,
    /// `Flag1 | Rank << 8` (item +4).
    pub word4: u16,
    pub command: u16,
    /// Index into the count array and the bit sets (item +0x12).
    pub slot: u16,
    /// `StatEntry` (item +6); an ability's low byte is the kind the Customize page of a companion tests (2).
    pub stat: u16,
}

impl ItemInfo {
    /// `Flag0` bit 0: a key item, owned or not (one bit, no count).
    pub fn is_key(&self) -> bool {
        self.flag0 & 1 != 0
    }
}

/// The tables the rules need, from the user's disc.
#[derive(Debug, Default)]
pub struct ItemIndex {
    /// In table order (`FUN_001a6730` walks them in this order).
    pub rows: Vec<ItemInfo>,
    by_id: BTreeMap<u16, usize>,
    /// `cmd.Argument` by command id (`FUN_001b6f48` + 4).
    cmd_arg: BTreeMap<u16, i16>,
    /// The item of each magic slot: the first `magc` row of the slot (`FUN_001a4c78` reads its
    /// `Item` at +0x24); 0 when the slot needs none.
    pub magic_items: [u16; 6],
    /// The "X Form" item (type 21) of each form id 1..=5 (its command's `Execute` is 4 and its
    /// `Argument` the form).
    pub form_items: [u16; 6],
}

impl ItemIndex {
    pub fn new(items: &[Item], cmds: &[Cmd], magc: &[Magc]) -> ItemIndex {
        let rows: Vec<ItemInfo> = items
            .iter()
            .map(|i| ItemInfo {
                id: i.id,
                kind: i.r#type,
                flag0: i.flag0,
                word4: i.flag1 as u16 | (i.rank as u16) << 8,
                command: i.command,
                slot: i.slot,
                stat: i.stat_entry,
            })
            .collect();
        let mut by_id = BTreeMap::new();
        for (n, r) in rows.iter().enumerate() {
            by_id.entry(r.id).or_insert(n);
        }
        let cmd_arg: BTreeMap<u16, i16> = cmds.iter().map(|c| (c.id, c.argument)).collect();
        let mut magic_items = [0u16; 6];
        for (slot, it) in magic_items.iter_mut().enumerate() {
            *it = magc.iter().find(|r| r.id as usize == slot).map_or(0, |r| r.item);
        }
        let mut form_items = [0u16; 6];
        for r in rows.iter().filter(|r| r.kind == item_type::DRIVE_FORM) {
            if let Some(c) = cmds.iter().find(|c| c.id == r.command && c.execute == 4)
                && (1..=5).contains(&c.argument)
                && form_items[c.argument as usize] == 0
            {
                form_items[c.argument as usize] = r.id;
            }
        }
        ItemIndex { rows, by_id, cmd_arg, magic_items, form_items }
    }

    /// `FUN_001a66e8`: the row of an item id.
    pub fn get(&self, id: u16) -> Option<&ItemInfo> {
        self.by_id.get(&id).map(|&n| &self.rows[n])
    }
}

/// A party member record, decoded.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Member {
    /// Hand 0 and hand 1 (`FUN_0016ee98`: record `+2 * hand`); hand 0 is the keyblade.
    pub weapons: [u16; 2],
    pub hp: u8,
    pub hp_max: u8,
    pub mp: u8,
    pub mp_max: u8,
    pub ap: u8,
    pub strength: u8,
    pub magic: u8,
    pub defense: u8,
    /// Sword, Shield or Staff column of `lvup` (`FUN_001b52f8`).
    pub weapon_set: u8,
    pub level: u8,
    /// Armor slots (the count is `+0x10`); 0 is an empty slot.
    pub armor: Vec<u16>,
    /// Accessory slots (count `+0x11`).
    pub accessories: Vec<u16>,
    /// Battle item slots (count `+0x12`).
    pub items: Vec<u16>,
    /// The ability list (64 slots, bit 15 = equipped), non-empty slots only.
    pub abilities: Vec<(u16, bool)>,
    /// The AI settings block (record `+0xd4`, 32 bytes; FUN_00112308 reads it, `Progress::init_member` zeroes it).
    /// Byte 0 is the battle style 0..5, byte `1 + k` the use setting of item or ability `k` (docs/party.md).
    pub settings: [u8; rec::SETTINGS_LEN],
}

/// A drive form record (`FUN_00187f60`), decoded.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct FormRecord {
    /// The form's second keyblade (`FUN_001a2010` writes it, `FUN_0016ee98` hand 1 reads it).
    pub weapon: u16,
    /// Internal level (`+2`, `FUN_001d3da8`).
    pub level: u8,
    /// `+3`: set to 1 with the level by `FUN_00187ed0`; no reader was traced.
    pub byte3: u8,
    /// Experience (`+4`, `FUN_001d3f88`); for form 5 the Anti counter (`FUN_001d4278`).
    pub exp: u32,
    /// The form's own ability list (16 slots, `FUN_00187f18`), non-empty slots only.
    pub abilities: Vec<(u16, bool)>,
}

/// Where the state came from (the log's "state from ...").
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    NewGame,
    File(String),
}

/// The game state. See the module doc.
#[derive(Resource, Clone)]
pub struct Progress {
    block: Vec<u8>,
    items: Arc<ItemIndex>,
    pub source: Source,
    /// What the app baseline added on top of the new game (empty with `KH2_STATE`).
    pub baseline: Vec<String>,
    /// The boot weapon words read from `00common.bdx` (empty without a disc); `apply_party_weapons` writes them.
    pub boot_weapons: Arc<BootWeapons>,
    /// Sora's Dodge Roll growth level (0 = not learned). The USA disc has no Dodge Roll ability item
    /// (docs/final-mix.md: the Final Mix game has it as growth slot 4), so the level is an app field
    /// beside the save block. 0 in the new game; the app baseline sets 1 (docs/progress.md).
    pub dodge_roll: u8,
    /// The munny the prizes pay: save block `+0x1600` (`0x33fe60`, before the party block, so it is
    /// kept apart from `block`). `FUN_0023e620` adds to it and `FUN_0023e5e0` lets a sum through only
    /// below 1,000,000 (else 999,999). The word at `0x340e48` that `experience()` reads is raised by
    /// `FUN_001b51e0`, which then runs the level-up check of every party member (`FUN_001b52f8`
    /// compares it with the level table), so that word is the experience total (docs/prizes.md).
    wallet: u32,
    /// The option word (save `+0x31b0`, `0x33fa10`: the ten Config rows of docs/menus.md 2.8) and the game
    /// mode (save `+0x1658`, read-only on that page). Outside the party block, so kept apart like `wallet`.
    config: [u32; 2],
    /// The story flags, the area program table and the exit-lock halfwords (`Story`, save block
    /// `0x33f6b0`, `0x33e86f`, `0x33f918`). Shared with the script hosts (`shared_story`): a clone of
    /// `Progress` shares it too.
    story: StoryHandle,
    /// The "seen" words of the NEW tags (save `+0x31b4`, docs/menus.md 16.1) as a save slot carries them: empty until a slot
    /// that holds them is loaded or the menu stores them (`set_new_seen`).
    new_seen: Vec<u32>,
    /// The record bits (the word array at `0x33fdb0`, save `+0x1550`, outside the party block): `FUN_001a5328` tests
    /// bit n, `FUN_001a52b0` sets it. The shops read and set them (`menu_shop::stock`); nothing else in the app does yet.
    record_bits: Vec<u32>,
    /// The moogle's synthesis record (save `+0x3728`, `FUN_002906c0`; docs/menus.md 23), read and written by the shop's
    /// third row (`menu_shop::synth`). Outside the party block, so kept apart like `wallet`. Saved in the slot files (key `moogle`).
    pub moogle: Moogle,
    /// The shops' "seen" bits (save `+0x3714`, 0x80 bits over the shop file's item list, `FUN_00265ad0`): an item whose
    /// bit is clear is NEW in a shop's Buy list until the cursor reaches it (`FUN_00265348`). Saved in the slot files (key `shop_seen`).
    shop_seen: [u32; 4],
    /// The shops' first-visit byte (save `+0x3724`): bit 0 / bit 1 once the opening text of a shop with the state's
    /// `+0xd` clear / set was dismissed (`FUN_002621a0`, `FUN_00265498`). Saved in the slot files (key `shop_visits`).
    shop_visits: u8,
    /// The journal's seen set (save `+0x38f0`, `FUN_002b70e0`, docs/menus.md 20.9): one (category, entry) pair per entry the
    /// journal has marked seen (`menu_journal::marks::Cat` as a number). The original keeps bits at per-category offsets; the
    /// app keeps the pairs [inferred storage, same meaning]. Saved in the slot files (key `journal_seen`).
    journal_seen: std::collections::BTreeSet<(u8, u32)>,
    /// The minigame records (`0x341620 + 8 * n`, `FUN_001d36e8`): byte 0 the state (0 never seen, 1 seen, 2 played, 4 a time
    /// score) and the word at +4 the score. Only the records the app has set are kept; an absent one is state 0. Saved in the
    /// slot files (key `minigames`). Nothing in the app plays a minigame yet, so only a state file fills it (gap).
    minigames: std::collections::BTreeMap<u16, (u8, u32)>,
    /// The defeated-enemy counters (save `+0x1660`, words from `+0xff0` for the NeoStatus ids 1000 .. 1046 and from `+0x1170`
    /// for 3000 .. 3011; `FUN_00187dc8` finds the slot, `FUN_00187e08` adds 1 up to 9999 once story flag 0x8d0 is set, `FUN_00187e68`
    /// reads it for the journal's counter row). Only the counters the app has raised are kept. Saved in the slot files (key `enemy_kills`).
    enemy_kills: std::collections::BTreeMap<u16, u32>,
    /// The reaction commands' use counts (the u16 array at `0x3410b0`, indexed by the command row's `ReactionCount` slot, `FUN_001b7168`; the
    /// journal's enemy page shows them, capped at 999). Only the slots the app has raised are kept. Saved in the slot files (key
    /// `reaction_uses`). `reaction_cmd::bump_use` writes it on every accepted reaction; a state file also fills it.
    reaction_uses: std::collections::BTreeMap<u16, u16>,
    /// The limits' best counts (the u16 array at `0x3414b0`, indexed by the `limt` row's `Id`, `FUN_001d2590`; `FUN_001d25b0` raises a slot to
    /// the value it is given, never lowers it). A limit is "used" (listed on the journal's limit page) once its slot is above 0. Only the
    /// slots the app has raised are kept. Saved in the slot files (key `limit_best`). Nothing in the app calls the writer yet: a state file
    /// fills it.
    limit_best: std::collections::BTreeMap<u8, u16>,
    /// The BATTLE REPORT counters (WP-CT1): `[healed, saved by Mickey, then the ten u16 words from 0x341840]`. Saved in the slot files
    /// (key `report_counters`). See [`Progress::count_healed`], [`Progress::count_mickey_save`], [`Progress::count_form_use`] and
    /// [`Progress::count_summon_use`].
    report: [u16; REPORT_WORDS],
}

/// The number of report counters: the healed count, the Mickey count and the ten words from 0x341840 (index 0 to 9: the form uses
/// at 1 to 5, then the summon uses behind them).
const REPORT_WORDS: usize = 12;

/// The synthesis record at save `+0x3728` (`FUN_002906c0` answers its address). Bit sets are kept as u64 (the
/// original's two words); a bit past 63 is dropped.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Moogle {
    /// +0, +4: one bit per material (its place in the material list) set by its first hand-over (`FUN_002887c8`).
    pub known: u64,
    /// +0x10: two bits per recipe, bit 2r = the recipe is learned (`FUN_002889e0` for recipe items, `FUN_00290cc0`
    /// for the Free Development sets).
    pub learned: u64,
    /// +0x18: one bit per list entry (2r the item, 2r+1 the higher rank) once made (`FUN_00288228`).
    pub made: u64,
    /// +0x20: the same entries once looked at (`FUN_00288fc0` sets it on the list cursor; `FUN_00288228` on a make).
    pub seen: u64,
    /// +0x28: the unlock bits `FUN_002903b8(n)` tests: 0..2 the Free Development sets, 3 the first two bonus rows, 4 the
    /// higher rank, 5 two bonuses at once, 6..9 the halved costs of a rank on the Creations list, 10 the Recipes row
    /// (`FUN_00290c28` sets the level rows' bits; the writer of bit 10 was not found).
    pub unlocks: u64,
    /// +8: one bit per collection reward (`cond` row k) once paid (`FUN_00291140`).
    pub rewards: u64,
    /// +0x30: the moogle's points (`FUN_002909d0`, at most 99,999).
    pub points: u32,
    /// +0x34: the materials in stock, by place in the material list (50 words; at most 999).
    pub stock: Vec<u32>,
    /// +0xfc: every material ever handed over, by place (50 words; at most 999).
    pub totals: Vec<u32>,
    /// +0x1c4: the moogle's level (`FUN_00290bb8`).
    pub level: u8,
}

/// The material slots of the record (`FUN_002887c8` and `FUN_00290400` walk 0x32).
/// The first chest bit in the record array: `0x33fdcc - 0x33fdb0` bytes is 224 bits (`FUN_001a6a90` vs `FUN_001a5328`).
pub const CHEST_BITS: u16 = 224;

pub const MOOGLE_MATERIALS: usize = 0x32;

impl Moogle {
    pub fn bit(set: u64, n: u32) -> bool {
        n < 64 && set >> n & 1 != 0
    }

    pub fn with(set: &mut u64, n: u32) {
        if n < 64 {
            *set |= 1 << n;
        }
    }

    /// `FUN_002903b8(n)`.
    pub fn unlocked(&self, n: u32) -> bool {
        Self::bit(self.unlocks, n)
    }

    /// The stock word of material slot `i` (0 past the end).
    pub fn stock_at(&self, i: usize) -> u32 {
        self.stock.get(i).copied().unwrap_or(0)
    }

    pub fn total_at(&self, i: usize) -> u32 {
        self.totals.get(i).copied().unwrap_or(0)
    }

    pub fn set_stock(&mut self, i: usize, v: u32) {
        if self.stock.len() < MOOGLE_MATERIALS {
            self.stock.resize(MOOGLE_MATERIALS, 0);
        }
        if let Some(s) = self.stock.get_mut(i) {
            *s = v;
        }
    }

    pub fn set_total(&mut self, i: usize, v: u32) {
        if self.totals.len() < MOOGLE_MATERIALS {
            self.totals.resize(MOOGLE_MATERIALS, 0);
        }
        if let Some(s) = self.totals.get_mut(i) {
            *s = v;
        }
    }
}

/// The option word `FUN_0023e9a0` writes on a new game: it keeps bits 0, 3 and 11 up and sets 0x40b, so
/// vibration on, Stereo, Navigational Map on, Summon Effects Auto, and every other row at its first value.
pub const OPTION_DEFAULT: u32 = 0x40b;
/// The game mode the app starts in: Standard (1; Beginner 0, Proud 2). The title screen sets the original's;
/// the app has none, so this is [inferred].
pub const GAME_MODE_DEFAULT: u32 = 1;

/// The prize munny limit (`FUN_0023e5e0`: a sum below 1,000,000 is kept, else 999,999).
pub const MAX_WALLET: u32 = 999_999;

impl Progress {
    // --- raw block access -------------------------------------------------------------------

    fn u8_at(&self, o: usize) -> u8 {
        self.block[o]
    }
    fn set_u8(&mut self, o: usize, v: u8) {
        self.block[o] = v;
    }
    fn u16_at(&self, o: usize) -> u16 {
        u16::from_le_bytes([self.block[o], self.block[o + 1]])
    }
    fn set_u16(&mut self, o: usize, v: u16) {
        self.block[o..o + 2].copy_from_slice(&v.to_le_bytes());
    }
    fn u32_at(&self, o: usize) -> u32 {
        u32::from_le_bytes(self.block[o..o + 4].try_into().unwrap())
    }
    fn set_u32(&mut self, o: usize, v: u32) {
        self.block[o..o + 4].copy_from_slice(&v.to_le_bytes());
    }
    fn zero(&mut self, o: usize, len: usize) {
        self.block[o..o + len].fill(0);
    }
    fn bit(&self, base: usize, words: usize, n: u16) -> bool {
        let w = n as usize >> 5;
        w < words && self.u32_at(base + 4 * w) >> (n & 31) & 1 != 0
    }
    fn set_bit(&mut self, base: usize, words: usize, n: u16, on: bool) {
        let w = n as usize >> 5;
        if w < words {
            let v = self.u32_at(base + 4 * w);
            let m = 1 << (n & 31);
            self.set_u32(base + 4 * w, if on { v | m } else { v & !m });
        }
    }

    /// The party block bytes (`0x33fec0`, `off::LEN` bytes).
    pub fn block(&self) -> &[u8] {
        &self.block
    }

    pub fn items(&self) -> &ItemIndex {
        &self.items
    }

    /// A state from party block bytes (for the oracle test: the block the game's code built).
    pub fn from_block(bytes: &[u8], items: Arc<ItemIndex>) -> Progress {
        let mut block = vec![0u8; off::LEN];
        let n = bytes.len().min(off::LEN);
        block[..n].copy_from_slice(&bytes[..n]);
        Progress { block, items, source: Source::NewGame, baseline: Vec::new(), boot_weapons: Arc::default(), dodge_roll: 0, wallet: 0, config: [OPTION_DEFAULT, GAME_MODE_DEFAULT], story: StoryHandle::default(), new_seen: Vec::new(), record_bits: Vec::new(), moogle: Moogle::default(), shop_seen: [0; 4], shop_visits: 0, journal_seen: Default::default(), minigames: Default::default(), enemy_kills: Default::default(), reaction_uses: Default::default(), limit_best: Default::default(), report: [0; REPORT_WORDS] }
    }

    // --- story flags ------------------------------------------------------------------------

    /// The story state (flags, program table, exit locks). Hold the guard briefly.
    pub fn story(&self) -> std::sync::MutexGuard<'_, Story> {
        self.story.lock()
    }

    /// The handle the script hosts share (`install_shared_story`).
    pub fn story_handle(&self) -> StoryHandle {
        self.story.clone()
    }

    /// The `KH2_STATE` keys that rebuild the story flags: `{"story_flags": [set ids]}` (applying it
    /// sets each flag through `FUN_0017ff60`, so its side effects run again in id order).
    pub fn story_json(&self) -> String {
        let ids: Vec<String> = self.story().set_flags().iter().map(|i| i.to_string()).collect();
        let points: Vec<String> = self.story().world_points().iter().map(|i| i.to_string()).collect();
        let points = if points.is_empty() { String::new() } else { format!(", \"world_points\": [{}]", points.join(", ")) };
        format!("{{\"story_flags\": [{}]{points}}}", ids.join(", "))
    }

    // --- records ----------------------------------------------------------------------------

    /// `FUN_001b58d8`: member id -> 1-based record index (14 is Sora's record, 15 the sixth).
    fn member_index(member: u16) -> usize {
        match member {
            14 => 1,
            15 => 6,
            m if (1..=15).contains(&m) => m as usize,
            _ => 0,
        }
    }

    /// `FUN_00187d40`: offset of a member's record.
    fn member_off(member: u16) -> Option<usize> {
        let i = Self::member_index(member);
        (i != 0 && i <= off::MEMBERS).then(|| (i - 1) * off::MEMBER_SIZE)
    }

    /// `FUN_00187f60`: offset of a form record, `form` 1..=9.
    fn form_off(form: u8) -> Option<usize> {
        (1..=off::FORMS as u8).contains(&form).then(|| off::FORM_BASE + form as usize * off::FORM_SIZE)
    }

    /// A party member's record (`member` is the party id: 1 Sora, 2 Donald, 3 Goofy, ...).
    pub fn member(&self, member: u16) -> Option<Member> {
        let r = Self::member_off(member)?;
        let list = |o: usize, n: usize| (0..n.min(8)).map(|i| self.u16_at(r + o + 2 * i)).collect::<Vec<_>>();
        Some(Member {
            weapons: [self.u16_at(r + rec::WEAPONS), self.u16_at(r + rec::WEAPONS + 2)],
            hp: self.u8_at(r + rec::HP),
            hp_max: self.u8_at(r + rec::HP_MAX),
            mp: self.u8_at(r + rec::MP),
            mp_max: self.u8_at(r + rec::MP_MAX),
            ap: self.u8_at(r + rec::AP),
            strength: self.u8_at(r + rec::STRENGTH),
            magic: self.u8_at(r + rec::MAGIC),
            defense: self.u8_at(r + rec::DEFENSE),
            weapon_set: self.u8_at(r + rec::WEAPON_SET),
            level: self.u8_at(r + rec::LEVEL),
            armor: list(rec::ARMOR, self.u8_at(r + rec::ARMOR_SLOTS) as usize),
            accessories: list(rec::ACCESSORIES, self.u8_at(r + rec::ACCESSORY_SLOTS) as usize),
            items: list(rec::ITEMS, self.u8_at(r + rec::ITEM_SLOTS) as usize),
            abilities: (0..rec::ABILITY_SLOTS)
                .map(|i| self.u16_at(r + rec::ABILITIES + 2 * i))
                .filter(|v| v & 0x7fff != 0)
                .map(|v| (v & 0x7fff, v & 0x8000 != 0))
                .collect(),
            settings: self.settings_at(r),
        })
    }

    fn settings_at(&self, r: usize) -> [u8; rec::SETTINGS_LEN] {
        self.block[r + rec::SETTINGS..r + rec::SETTINGS + rec::SETTINGS_LEN].try_into().unwrap()
    }

    /// A member's setting byte `index` (0 = the battle style, `1 + k` = the use setting of item or ability `k`).
    /// `None` for an unknown member or an index past the block.
    pub fn member_setting(&self, member: u16, index: usize) -> Option<u8> {
        let r = Self::member_off(member)?;
        (index < rec::SETTINGS_LEN).then(|| self.u8_at(r + rec::SETTINGS + index))
    }

    /// `FUN_001a0030`: what a status block writes back to its party record when it is released (the field actor's
    /// destructor `FUN_00174998`, the last user of a block `FUN_0019ffb8`): the current HP (record `+4`) and MP (`+6`),
    /// each held to the record's maximum (`+5`, `+7`; the block's current value is built the same way by
    /// `FUN_0019f920`, `min(maximum, current)`). The maxima are left as the record has them. False when the member has no record.
    pub fn set_member_vitals(&mut self, member: u16, hp: i32, mp: i32) -> bool {
        let Some(r) = Self::member_off(member) else { return false };
        let (hp_max, mp_max) = (self.u8_at(r + rec::HP_MAX), self.u8_at(r + rec::MP_MAX));
        self.set_u8(r + rec::HP, hp.clamp(0, i32::from(hp_max)) as u8);
        self.set_u8(r + rec::MP, mp.clamp(0, i32::from(mp_max)) as u8);
        true
    }

    /// `FUN_0019f920`'s current HP and MP of a record: `min(maximum, current)` for each (`None`: no such record).
    pub fn member_vitals(&self, member: u16) -> Option<(u8, u8)> {
        let r = Self::member_off(member)?;
        Some((self.u8_at(r + rec::HP).min(self.u8_at(r + rec::HP_MAX)), self.u8_at(r + rec::MP).min(self.u8_at(r + rec::MP_MAX))))
    }

    /// Auto Healing's heal of a member outside the field (`FUN_0017d4b8`): the block of the record is made
    /// (`FUN_0019fe48`), `FUN_001a0140(block, 1, 0, 0)` adds one HP (a block without an owning actor takes
    /// `FUN_001a0100`: the sum held between the minimum 0 and the maximum), then the block is released
    /// (`FUN_0019ffb8`, `FUN_001a0030`) and the new HP is in the record. Returns (before, after).
    pub fn heal_reserve_member(&mut self, member: u16, delta: i32) -> Option<(u8, u8)> {
        let (hp, mp) = self.member_vitals(member)?;
        let r = Self::member_off(member)?;
        let after = crate::party_rules::clamped_add(i32::from(hp), delta, 0, i32::from(self.u8_at(r + rec::HP_MAX))) as u8;
        self.set_member_vitals(member, i32::from(after), i32::from(mp));
        Some((hp, after))
    }

    /// Writes a member's setting byte (what the party menu's settings page stores). False when nothing was written.
    pub fn set_member_setting(&mut self, member: u16, index: usize, value: u8) -> bool {
        let Some(r) = Self::member_off(member) else { return false };
        if index >= rec::SETTINGS_LEN {
            return false;
        }
        self.set_u8(r + rec::SETTINGS + index, value);
        true
    }

    pub fn sora(&self) -> Member {
        self.member(SORA).unwrap_or_default()
    }

    /// A drive form record (`form` 1..=9; 1..=5 are Valor, Wisdom, Master, Final, Anti).
    pub fn form(&self, form: u8) -> FormRecord {
        let Some(r) = Self::form_off(form) else { return FormRecord::default() };
        FormRecord {
            weapon: self.u16_at(r),
            level: self.u8_at(r + 2),
            byte3: self.u8_at(r + 3),
            exp: self.u32_at(r + 4),
            abilities: (0..16).map(|i| self.u16_at(r + 8 + 2 * i)).filter(|v| v & 0x7fff != 0).map(|v| (v & 0x7fff, v & 0x8000 != 0)).collect(),
        }
    }

    pub fn form_level(&self, form: u8) -> u8 {
        self.form(form).level
    }

    pub fn form_exp(&self, form: u8) -> u32 {
        self.form(form).exp
    }

    pub fn set_form_level(&mut self, form: u8, level: u8) {
        if let Some(r) = Self::form_off(form) {
            self.set_u8(r + 2, level);
        }
    }

    pub fn set_form_exp(&mut self, form: u8, exp: u32) {
        if let Some(r) = Self::form_off(form) {
            self.set_u32(r + 4, exp);
        }
    }

    pub fn set_form_weapon(&mut self, form: u8, item: u16) {
        if let Some(r) = Self::form_off(form) {
            self.set_u16(r, item);
        }
    }

    /// The form is unlocked: its "X Form" item is owned.
    pub fn owns_form(&self, form: u8) -> bool {
        self.items.form_items.get(form as usize).is_some_and(|&i| i != 0 && self.item_count(i) > 0)
    }

    pub fn current_form(&self) -> u8 {
        self.u8_at(off::CURRENT_FORM)
    }

    /// The active summon (1-based `sumn` index, 0 none).
    pub fn summon(&self) -> u8 {
        self.u8_at(off::SUMMON)
    }

    pub fn summon_level(&self) -> u8 {
        self.u8_at(off::SUMMON_LEVEL)
    }

    pub fn summon_exp(&self) -> u32 {
        self.u32_at(off::SUMMON_EXP)
    }

    /// Byte 6 of `0x340c88` (the summon level-up, `FUN_001d4ec8`).
    pub fn set_summon_level(&mut self, level: u8) {
        self.set_u8(off::SUMMON_LEVEL, level);
    }

    /// `0x340e4c`, the summon experience (`FUN_001d4ec8`).
    pub fn set_summon_exp(&mut self, exp: u32) {
        self.set_u32(off::SUMMON_EXP, exp);
    }

    /// The Genie form / Tinker Bell byte (`0x340c8f`).
    pub fn summon_extra(&self) -> u8 {
        self.u8_at(off::SUMMON_EXTRA)
    }

    /// The saved drive gauge bytes (`+0xdd0..+0xdd3`, status `+0x1b0..+0x1b3`). Not wired: the
    /// drive system keeps its own start values (docs/progress.md, Gaps).
    pub fn drive_bytes(&self) -> [u8; 4] {
        [self.u8_at(off::DRIVE), self.u8_at(off::DRIVE + 1), self.u8_at(off::DRIVE + 2), self.u8_at(off::DRIVE + 3)]
    }

    /// The four bytes of a world's record (`0x340c9c + 4 * world`).
    pub fn world_record(&self, world: usize) -> [u8; 4] {
        if world >= off::WORLD_COUNT {
            return [0; 4];
        }
        let o = off::WORLDS + 4 * world;
        [self.u8_at(o), self.u8_at(o + 1), self.u8_at(o + 2), self.u8_at(o + 3)]
    }

    /// Sora's outfit in a world: the low 5 bits of byte 0 of the world's record (`FUN_001a71e8`).
    pub fn outfit(&self, world: usize) -> u8 {
        self.world_record(world)[0] & 0x1f
    }

    pub fn set_outfit(&mut self, world: usize, outfit: u8) {
        if world < off::WORLD_COUNT {
            let o = off::WORLDS + 4 * world;
            let b = self.u8_at(o);
            self.set_u8(o, (b & !0x1f) | (outfit & 0x1f));
        }
    }

    /// The per-spell switch bit of a magic slot (`FUN_001a4bb8`; the pause menu toggles it).
    pub fn spell_switch(&self, slot: u8) -> bool {
        self.bit(off::SPELL_SWITCH, 1, slot as u16)
    }

    pub fn spell_switch_word(&self) -> u32 {
        self.u32_at(off::SPELL_SWITCH)
    }

    /// `FUN_001a4b68` (set the bit) and `FUN_001a4b30` (clear it): the per-spell switch bit `bit` of the
    /// word at save `+0x264c` (party block `+0xfec`). A SET bit means "Not Used": `FUN_001a4bb8` answers
    /// 1 (usable) when the bit is clear. A bit past the word (`bit >= 32`) is ignored here; the original
    /// indexes a second word it never has.
    pub fn set_spell_switch(&mut self, bit: u8, set: bool) {
        self.set_bit(off::SPELL_SWITCH, 1, bit as u16, set);
    }

    /// `FUN_00274e50`, the Customize page's switch: flips the bit and answers whether the spell is now
    /// usable ("Use", the bit clear). The original reads the state with `FUN_001a4bb8` and writes the
    /// opposite (clear when it read 0, set otherwise), which is a plain flip for every spell whose command
    /// `Cost` is not 255 (`docs/magic.md`).
    pub fn toggle_spell_switch(&mut self, bit: u8) -> bool {
        let now = self.spell_switch(bit);
        self.set_spell_switch(bit, !now);
        now
    }

    /// One of the four command shortcuts (`FUN_001a1f78` gives an item's command, `FUN_00274dd0` writes
    /// it): the u16 at save `+0x2600 + 2 * slot` (party block `+0xfa0`).
    pub fn shortcut(&self, slot: usize) -> u16 {
        if slot < SHORTCUTS { self.u16_at(off::SHORTCUTS + 2 * slot) } else { 0 }
    }

    pub fn shortcuts(&self) -> [u16; SHORTCUTS] {
        std::array::from_fn(|i| self.shortcut(i))
    }

    /// `FUN_00274dd0`: store a command (0 empties the slot). False for a slot past the four.
    pub fn set_shortcut(&mut self, slot: usize, command: u16) -> bool {
        if slot >= SHORTCUTS {
            return false;
        }
        self.set_u16(off::SHORTCUTS + 2 * slot, command);
        true
    }

    /// `FUN_001a0ec0`, `0ee0`, `0f00`, `0f20`: whether the stat boost item `id` (0x114 strength, 0x115 magic,
    /// 0x116 defense, 0x117 AP) has reached its limit on the member's record (the byte is above 99).
    pub fn boost_maxed(&self, member: u16, id: u16) -> bool {
        let (Some(r), Some(b)) = (Self::member_off(member), Self::boost_byte(id)) else { return false };
        self.u8_at(r + b) > 99
    }

    fn boost_byte(id: u16) -> Option<usize> {
        match id {
            0x114 => Some(rec::STRENGTH),
            0x115 => Some(rec::MAGIC),
            0x116 => Some(rec::DEFENSE),
            0x117 => Some(rec::AP),
            _ => None,
        }
    }

    /// `FUN_001a0ce0`, `0d30`, `0d80`, `0dd0`: add `n` to the record byte the boost item raises, held at 100
    /// (when the sum would pass 100 only the room is added). Answers the amount added; the original adds
    /// the same amount to the live status block, which the app recomputes from the record.
    pub fn apply_boost(&mut self, member: u16, id: u16, n: u8) -> u8 {
        let (Some(r), Some(b)) = (Self::member_off(member), Self::boost_byte(id)) else { return 0 };
        let old = self.u8_at(r + b);
        let add = if (old as u32 + n as u32) < 0x65 { n } else { 100u8.saturating_sub(old) };
        self.set_u8(r + b, old.wrapping_add(add));
        add
    }

    /// The experience total (`0x340e48`, see the field `wallet` for the munny).
    pub fn experience(&self) -> i32 {
        self.u32_at(off::EXPERIENCE) as i32
    }

    /// `FUN_001b51e0`: add (or, negative, remove) experience; a sum of 10,000,000 or more becomes
    /// 9,999,999. The routine does not floor at 0 (its callers check first).
    pub fn add_experience(&mut self, delta: i32) {
        let sum = self.experience().wrapping_add(delta);
        self.set_u32(off::EXPERIENCE, (if sum < 10_000_000 { sum } else { MAX_EXPERIENCE }) as u32);
    }

    /// `FUN_001b52f8` for one member: while his level is below 99 and the `lvup` threshold of his level (the
    /// row `FUN_001b50c0(member, level)`, its `Exp`) is at most the experience total, the record's level byte
    /// (`+0xf`) rises by one and `FUN_001b53d0` runs for the new row: the rises of Strength, Magic, Defense and
    /// AP over the previous row are queued as messages (0x8ade to 0x8ae1, not drawn here), and the new row's
    /// ability column for the member's weapon set (record `+0xe`: Sword, Shield, Staff) is given to him
    /// (`FUN_001a18d8(ability, member, 0)` = `give`). A record whose level is 0 is skipped [inferred: the
    /// original reads the bytes before the table then]. The stats themselves are not stored: the status
    /// rebuild (`FUN_0019f920`) adds the row to the record when the actor is made.
    pub fn raise_level(&mut self, member: u16, tables: &[Vec<t::Lvup>]) -> Vec<LevelReached> {
        let Some(r) = Self::member_off(member) else { return Vec::new() };
        let exp = self.experience();
        let set = self.u8_at(r + rec::WEAPON_SET);
        let mut reached = Vec::new();
        loop {
            let level = self.u8_at(r + rec::LEVEL);
            if level == 0 || level >= 99 {
                break;
            }
            let (Some(old), Some(new)) = (lvup_row(tables, member, level), lvup_row(tables, member, level + 1)) else { break };
            if old.exp > exp {
                break;
            }
            self.set_u8(r + rec::LEVEL, level + 1);
            let rise = |a: u8, b: u8| i16::from(a) - i16::from(b);
            let ability = match set {
                0 => new.sword_ability,
                1 => new.shield_ability,
                2 => new.staff_ability,
                _ => 0,
            };
            let given = ability != 0 && self.give(ability, member, false);
            reached.push(LevelReached {
                member,
                level: level + 1,
                strength: rise(new.strength, old.strength),
                magic: rise(new.magic, old.magic),
                defense: rise(new.defense, old.defense),
                ap: rise(new.ap, old.ap),
                ability,
                given,
            });
        }
        reached
    }

    /// `FUN_001878b0(record, 1)`: the experience a member still needs for his next level (the `lvup` threshold
    /// of his level minus the total); 0 at level 99 or without a record.
    pub fn experience_to_next(&self, member: u16, tables: &[Vec<t::Lvup>]) -> i32 {
        let Some(m) = self.member(member) else { return 0 };
        if m.level >= 99 {
            return 0;
        }
        lvup_row(tables, member, m.level).map_or(0, |row| row.exp.wrapping_sub(self.experience()))
    }

    /// `FUN_001b51e0(amount)` whole: `add_experience`, then the level check of the 13 party slots (slot 1 is the
    /// player's character `player`, the others their own ids; the original's slot 6 answers the id
    /// `FUN_001a7158(3)` instead while the world byte `0x33e830` is 8, which is not ported), and the answer
    /// `FUN_001878b0(Sora's record)`: what Sora still needs for his next level (the value the "NEXT LEVEL" strip shows).
    pub fn pay_experience(&mut self, amount: i32, tables: &[Vec<t::Lvup>], player: u16) -> ExperiencePaid {
        self.add_experience(amount);
        let mut levels = Vec::new();
        for slot in 1..=13u16 {
            let id = if slot == 1 { player } else { slot };
            levels.extend(self.raise_level(id, tables));
        }
        ExperiencePaid { to_next: self.experience_to_next(SORA, tables), levels }
    }

    fn set_experience(&mut self, v: i32) {
        self.set_u32(off::EXPERIENCE, v.min(MAX_EXPERIENCE) as u32);
    }

    /// `KH2_STATE` key `munny`: set the wallet, held at the prize limit.
    fn set_wallet(&mut self, v: u32) {
        self.wallet = v.min(MAX_WALLET);
    }

    /// The option word (save `+0x31b0`).
    pub fn option_word(&self) -> u32 {
        self.config[0]
    }

    pub fn set_option_word(&mut self, w: u32) {
        self.config[0] = w;
    }

    /// The game mode (save `+0x1658`).
    pub fn game_mode(&self) -> u32 {
        self.config[1]
    }

    /// `FUN_001a5328`: record bit `n` (the array at `0x33fdb0`) is set.
    pub fn record_bit(&self, n: u16) -> bool {
        self.record_bits.get(n as usize >> 5).is_some_and(|w| w >> (n & 31) & 1 != 0)
    }

    /// `FUN_001a52b0`: sets record bit `n` (its call of `FUN_001d39d0` after the write is not ported).
    pub fn set_record_bit(&mut self, n: u16) {
        let w = n as usize >> 5;
        if self.record_bits.len() <= w {
            self.record_bits.resize(w + 1, 0);
        }
        self.record_bits[w] |= 1 << (n & 31);
    }

    /// `FUN_00265348`'s test: the shop item at place `n` of the shop file's list was seen (save `+0x3714`).
    pub fn shop_seen(&self, n: usize) -> bool {
        n < 128 && self.shop_seen[n >> 5] >> (n & 31) & 1 != 0
    }

    /// `FUN_00265348`: marks the item at place `n` seen (`|=` of its bit in save `+0x3714`).
    pub fn set_shop_seen(&mut self, n: usize) {
        if n < 128 {
            self.shop_seen[n >> 5] |= 1 << (n & 31);
        }
    }

    /// `FUN_001a6a90`: the chest with `OverallChestIndex` `n` has been opened. The chest bits are the word array at
    /// `0x33fdcc`, which is the record array (`0x33fdb0`) from bit 224 on, so they share `record_bits`.
    pub fn chest_opened(&self, n: u16) -> bool {
        n != 0 && self.record_bit(CHEST_BITS + n)
    }

    /// Marks the chest `n` opened. The original sets the bit in `FUN_001a6910` (after the reward is given; `FUN_001a6a58`
    /// mode 1 from the chest-open sequence `FUN_001a6db8`, `FUN_001a6a78` mode 2 from the reward list of an area trigger,
    /// `FUN_001978b0`). The app has no chest object and does not run a trigger's reward list, so nothing calls this
    /// outside a state file yet (gap, docs/menus.md "WP-MN13 status").
    pub fn set_chest_opened(&mut self, n: u16) {
        if n != 0 {
            self.set_record_bit(CHEST_BITS + n);
        }
    }

    /// The journal's seen set (save `+0x38f0`): entry `key` of category `cat` was marked seen.
    pub fn journal_seen(&self, cat: u8, key: u32) -> bool {
        self.journal_seen.contains(&(cat, key))
    }

    /// Marks an entry of the journal seen.
    pub fn set_journal_seen(&mut self, cat: u8, key: u32) {
        self.journal_seen.insert((cat, key));
    }

    /// Every (category, entry) pair of the journal's seen set.
    pub fn journal_seen_all(&self) -> impl Iterator<Item = (u8, u32)> + '_ {
        self.journal_seen.iter().copied()
    }

    /// `FUN_001d36e8(n)`: the minigame record `n` as (state byte, score word); (0, 0) for one nothing has set.
    pub fn minigame(&self, n: u16) -> (u8, u32) {
        self.minigames.get(&n).copied().unwrap_or((0, 0))
    }

    /// `FUN_00187dc8`'s slot test: the counter ids are 1000 .. 1046 and 3000 .. 3011 (`NeoStatus` of the enemy objects).
    pub fn enemy_counter_id(id: u32) -> bool {
        id.wrapping_sub(1000) <= 0x2e || id.wrapping_sub(3000) < 0xc
    }

    /// `FUN_00187e68(save + 0x1660, id)`: how often the enemy with `NeoStatus` `id` has been defeated (0 for an id with no slot).
    pub fn enemy_defeats(&self, id: u32) -> u32 {
        self.enemy_kills.get(&(id as u16)).copied().filter(|_| Self::enemy_counter_id(id)).unwrap_or(0)
    }

    /// `FUN_00187e08(0x33fec0, id)`, called by the enemies' defeat method `FUN_00184a30`: adds 1 to the counter of a defeated
    /// enemy when story flag 0x8d0 (`FUN_00180850`) is set and the id has a slot; the count stops at 9999. Returns whether it counted.
    pub fn count_defeat(&mut self, id: u32) -> bool {
        if !Self::enemy_counter_id(id) || !self.story().test(0x8d0) {
            return false;
        }
        let n = self.enemy_kills.entry(id as u16).or_insert(0);
        if *n >= 9999 {
            return false;
        }
        *n += 1;
        true
    }

    fn bump_report(&mut self, i: usize) {
        if let Some(w) = self.report.get_mut(i)
            && *w < 999
        {
            *w += 1;
        }
    }

    /// `FUN_001d76a0` (save `+0x2fdc` = `0x34183c`, report row 3): adds 1, up to 999. The original calls it from the heal task's end
    /// `FUN_001b8658` when the heal reached member 1 (Sora) (`FUN_001a11a0` with the member id at task `+0xc`).
    pub fn count_healed(&mut self) {
        self.bump_report(0);
    }

    /// Report row 3.
    pub fn healed_count(&self) -> u16 {
        self.report[0]
    }

    /// The word at save `+0x2dbe` (`0x34161e`, report row 4): `FUN_001cda78` / `FUN_001cdb98` (the init of the rescuing object, made by
    /// `FUN_001cdf48`) add 1 to it, up to 999, together with the word before it (`0x34161c`, read by `FUN_001ad520`).
    pub fn count_mickey_save(&mut self) {
        self.bump_report(1);
    }

    /// Report row 4.
    pub fn mickey_save_count(&self) -> u16 {
        self.report[1]
    }

    /// `FUN_001d76c8(index)`: adds 1 to the u16 at `0x341840 + 2 * index`, up to 999 (`FUN_001cf030` message 4 calls it with the
    /// index `FUN_001d4180` answers). The report reads the indexes 1 to 5 (row 5). An index above 9 counts nothing.
    pub fn count_form_use(&mut self, index: usize) {
        if index < 10 {
            self.bump_report(2 + index);
        }
    }

    /// `FUN_001d76f8(index)`: the same on the u16 at `0x34184a + 2 * index` (`FUN_001c6168` calls it with `FUN_001d4ca0` of the
    /// command, 0 when the command is not in the object table). Index 0 is the word of form index 5 (and report row 6, the Antiform
    /// count); the summon counters of row 7 are the indexes 1 to 4.
    pub fn count_summon_use(&mut self, index: usize) {
        if index < 5 {
            self.bump_report(2 + 5 + index);
        }
    }

    /// Report row 5: the form uses at the indexes 1 to 5 (`0x341842`, five u16).
    pub fn form_uses(&self) -> [u16; 5] {
        std::array::from_fn(|i| self.report[2 + 1 + i])
    }

    /// Report row 6: the word at save `+0x2fea` (`0x34184a`), which is also form index 5 and summon index 0.
    pub fn antiform_uses(&self) -> u16 {
        self.report[2 + 5]
    }

    /// Report row 7: the summon uses at the indexes 1 to 4 (`0x34184c`).
    pub fn summon_uses(&self) -> [u16; 4] {
        std::array::from_fn(|i| self.report[2 + 6 + i])
    }

    /// `FUN_001d2590`: the best count of the limit with `limt` slot `slot` (0 for one never used).
    pub fn limit_best(&self, slot: u8) -> u16 {
        self.limit_best.get(&slot).copied().unwrap_or(0)
    }

    /// `FUN_001d25b0`: raises the limit's best count to `n` when it is below (the slot never goes down).
    pub fn raise_limit_best(&mut self, slot: u8, n: u16) {
        if n > self.limit_best(slot) {
            self.limit_best.insert(slot, n);
        }
    }

    /// Every raised limit slot as (slot, best count).
    pub fn limit_best_all(&self) -> impl Iterator<Item = (u8, u16)> + '_ {
        self.limit_best.iter().map(|(&i, &c)| (i, c))
    }

    /// The use count of the reaction-command slot `slot` (`0x3410b0 + 2 * slot`).
    pub fn reaction_use(&self, slot: u16) -> u16 {
        self.reaction_uses.get(&slot).copied().unwrap_or(0)
    }

    /// Sets a reaction-command use count (capped at 999 as the journal shows it; 0 removes it).
    pub fn set_reaction_use(&mut self, slot: u16, n: u16) {
        if n == 0 {
            self.reaction_uses.remove(&slot);
        } else {
            self.reaction_uses.insert(slot, n.min(999));
        }
    }

    /// Every raised reaction-command use count as (slot, count).
    pub fn reaction_uses_all(&self) -> impl Iterator<Item = (u16, u16)> + '_ {
        self.reaction_uses.iter().map(|(&i, &c)| (i, c))
    }

    /// Every raised defeated-enemy counter as (NeoStatus id, count).
    pub fn enemy_defeats_all(&self) -> impl Iterator<Item = (u32, u32)> + '_ {
        self.enemy_kills.iter().map(|(&i, &c)| (u32::from(i), c))
    }

    /// Sets a defeated-enemy counter (the state file and the slot loader); ids without a slot are ignored.
    pub fn set_enemy_defeats(&mut self, id: u32, n: u32) {
        if Self::enemy_counter_id(id) {
            if n == 0 {
                self.enemy_kills.remove(&(id as u16));
            } else {
                self.enemy_kills.insert(id as u16, n.min(9999));
            }
        }
    }

    /// Sets the minigame record `n` (state byte at +0, score word at +4). Only a state file calls this yet.
    pub fn set_minigame(&mut self, n: u16, state: u8, score: u32) {
        if state == 0 && score == 0 {
            self.minigames.remove(&n);
        } else {
            self.minigames.insert(n, (state, score));
        }
    }

    /// `FUN_00265498`: bit `n` (0 or 1) of the first-visit byte (save `+0x3724`).
    pub fn shop_visit(&self, n: u8) -> bool {
        self.shop_visits >> (n & 7) & 1 != 0
    }

    /// `FUN_002621a0`: sets bit `n` of the first-visit byte.
    pub fn set_shop_visit(&mut self, n: u8) {
        self.shop_visits |= 1 << (n & 7);
    }

    /// `FUN_0023e6b0`: takes `n` from the munny (`FUN_0023e680` runs first [GAP: not read]); the shop's buy only
    /// asks for what the munny covers (`menu_shop::step_quantity`), so the port stops at 0.
    pub fn spend(&mut self, n: u32) -> u32 {
        self.wallet = self.wallet.saturating_sub(n);
        self.wallet
    }

    /// The prize munny (save `+0x1600`, see the field).
    pub fn wallet(&self) -> u32 {
        self.wallet
    }

    /// `FUN_0023e620`: add `n`; when the unsigned sum is not below 1,000,000 (`FUN_0023e5e0`) the
    /// counter becomes 999,999. Returns the new value.
    pub fn add_to_wallet(&mut self, n: u32) -> u32 {
        let sum = self.wallet.wrapping_add(n);
        self.wallet = if sum < MAX_WALLET + 1 { sum } else { MAX_WALLET };
        self.wallet
    }

    // --- inventory ----------------------------------------------------------------------------

    /// `FUN_001a1cc8`: how many of an item the player owns. A key item answers 0 or 1 (its bit);
    /// an ability (type 0x13) is not counted here (0); everything else is the count byte of the
    /// item's slot. A spell's count is its level.
    pub fn item_count(&self, id: u16) -> u8 {
        let Some(it) = self.items.get(id) else { return 0 };
        if it.is_key() {
            self.bit(off::KEYS, off::KEY_WORDS, it.slot) as u8
        } else if it.kind != item_type::ABILITY {
            self.count_at(it.slot)
        } else {
            0
        }
    }

    fn count_at(&self, slot: u16) -> u8 {
        if (slot as usize) < off::COUNT_SLOTS { self.u8_at(off::COUNTS + slot as usize) } else { 0 }
    }

    fn set_count_at(&mut self, slot: u16, v: u8) {
        if (slot as usize) < off::COUNT_SLOTS {
            self.set_u8(off::COUNTS + slot as usize, v);
        }
    }

    /// The level of a magic slot (0 Fire, 1 Blizzard, 2 Thunder, 3 Cure, 4 Magnet, 5 Reflect): the
    /// count of the slot's `magc` item (`FUN_001a4c78`; 0 = not owned). A slot without an item
    /// answers 0 (its base row is then used regardless, `magic_rules::pick_row`).
    pub fn spell_level(&self, slot: u8) -> u8 {
        match self.items.magic_items.get(slot as usize) {
            Some(&i) if i != 0 => self.item_count(i),
            _ => 0,
        }
    }

    pub fn spell_levels(&self) -> [u8; 6] {
        std::array::from_fn(|s| self.spell_level(s as u8))
    }

    /// `FUN_001a1768(id, to, n)`: where `n` copies of an item would go. `to` is a party member id
    /// or `INVENTORY`. Answers `to` when the member's record takes it (a consumable into a free
    /// battle item slot, an ability into the ability list), `INVENTORY` when the count can grow
    /// (to 99 at most; a key item only once), 0 when the item is refused.
    pub fn room(&self, id: u16, to: u16, n: u8) -> u16 {
        let Some(it) = self.items.get(id).copied() else { return 0 };
        if it.is_key() {
            return if self.bit(off::KEYS, off::KEY_WORDS, it.slot) || n != 1 { 0 } else { INVENTORY };
        }
        if to != INVENTORY
            && n == 1
            && let Some(r) = Self::member_off(to)
        {
            if it.kind == item_type::CONSUMABLE {
                let slots = self.u8_at(r + rec::ITEM_SLOTS) as usize;
                for i in 0..slots {
                    if self.u16_at(r + rec::ITEMS + 2 * i) == 0 {
                        let reserved = self.u16_at(r + rec::RESERVED + 2 * i);
                        if reserved == id || reserved == 0 {
                            return to;
                        }
                    }
                }
            } else if it.kind == item_type::ABILITY {
                return to;
            }
        }
        if self.count_at(it.slot) as u32 + n as u32 > MAX_COUNT as u32 { 0 } else { INVENTORY }
    }

    /// `FUN_001a18d8(id, to, equipped)`: give one item to a party member or the inventory.
    /// Consumables fill the member's battle item slots first, abilities go into the member's
    /// list (`equipped` sets bit 15), everything else is counted in the inventory. Sets the
    /// "obtained" bit of a non-key item. False when refused.
    pub fn give(&mut self, id: u16, to: u16, equipped: bool) -> bool {
        let Some(it) = self.items.get(id).copied() else { return false };
        let target = self.room(id, to, 1);
        if target == 0 {
            return false;
        }
        if !it.is_key() && target != INVENTORY {
            if let Some(r) = Self::member_off(target) {
                if it.kind == item_type::CONSUMABLE {
                    self.put_battle_item(r, id);
                    if target == 1 || target == 0xe {
                        self.note_recent(id);
                    }
                } else if it.kind == item_type::ABILITY {
                    self.put_ability(r, id, equipped);
                }
            }
        } else {
            self.add_count(id, 1);
        }
        if !it.is_key() {
            self.set_bit(off::OBTAINED, off::OBTAINED_WORDS, it.slot, true);
        }
        true
    }

    /// Add `n` copies to the inventory, one `give` each (the original's own entry point); returns
    /// how many were taken (a full count or an owned key item stops it).
    pub fn add_item(&mut self, id: u16, n: u8) -> u8 {
        (0..n).take_while(|_| self.give(id, INVENTORY, false)).count() as u8
    }

    /// `FUN_001a1c30(id, n)`: remove `n` copies (to 0 at least); a key item loses its bit.
    pub fn use_item(&mut self, id: u16, n: u8) {
        let Some(it) = self.items.get(id).copied() else { return };
        if it.is_key() {
            self.set_bit(off::KEYS, off::KEY_WORDS, it.slot, false);
        } else {
            let c = self.count_at(it.slot);
            self.set_count_at(it.slot, c.saturating_sub(n));
        }
    }

    /// Set a count directly (the state file; no side effects). Key items: set or clear the bit.
    pub fn set_count(&mut self, id: u16, n: u8) {
        let Some(it) = self.items.get(id).copied() else { return };
        if it.is_key() {
            self.set_bit(off::KEYS, off::KEY_WORDS, it.slot, n > 0);
        } else if it.kind != item_type::ABILITY {
            self.set_count_at(it.slot, n.min(MAX_COUNT));
        }
    }

    /// `FUN_001a1a18(id, n)`: count the item (or set its key bit), then the type's side effect:
    /// a consumable is noted (`FUN_001a2120`); a form item makes its form take a keyblade
    /// (`FUN_001a2010`); a spell obtained for the first time puts its command into the first
    /// empty shortcut 1..3. Not ported: item 0x20's `FUN_001803b0` and the spell's tutorial
    /// prompts `FUN_00180450` (both act outside this block).
    fn add_count(&mut self, id: u16, n: u8) {
        let Some(it) = self.items.get(id).copied() else { return };
        let mut before = 0;
        if it.is_key() {
            self.set_bit(off::KEYS, off::KEY_WORDS, it.slot, true);
        } else {
            before = self.count_at(it.slot);
            self.set_count_at(it.slot, before.wrapping_add(n));
        }
        match it.kind {
            item_type::CONSUMABLE => self.note_recent(id),
            item_type::DRIVE_FORM => self.form_takes_keyblade(&it),
            item_type::MAGIC if before == 0 => {
                for k in 1..4 {
                    if self.u16_at(off::SHORTCUTS + 2 * k) == 0 {
                        self.set_u16(off::SHORTCUTS + 2 * k, it.command);
                        break;
                    }
                }
            }
            _ => {}
        }
    }

    /// `FUN_001a2120`: the first consumable also becomes shortcut 0 (its command,
    /// `FUN_001a1f78`); the id joins the 8-entry list if not there.
    fn note_recent(&mut self, id: u16) {
        if self.u16_at(off::RECENT) == 0 && self.u16_at(off::SHORTCUTS) == 0 {
            let cmd = self.shortcut_command(id);
            self.set_u16(off::SHORTCUTS, cmd);
        }
        for i in 0..8 {
            let v = self.u16_at(off::RECENT + 2 * i);
            if v == 0 {
                self.set_u16(off::RECENT + 2 * i, id);
                return;
            }
            if v == id {
                return;
            }
        }
    }

    /// `FUN_001a1f78`: a consumable's command; a spell's command comes from the loaded magic
    /// resources (`FUN_001a4dd0`, empty until an area loads them), answered 0 here.
    fn shortcut_command(&self, id: u16) -> u16 {
        match self.items.get(id) {
            Some(it) if it.kind == item_type::CONSUMABLE => it.command,
            _ => 0,
        }
    }

    /// `FUN_001a2010`: obtaining a form item resets the Anti counter (form record 5 `+4`) and,
    /// for Valor, Master and Final with no second keyblade yet, takes the first keyblade the
    /// inventory holds (`FUN_001a20b0(2)`, table order) for the form's left hand.
    fn form_takes_keyblade(&mut self, it: &ItemInfo) {
        self.set_form_exp(5, 0);
        let form = self.items.cmd_arg.get(&it.command).copied().unwrap_or(0);
        if matches!(form, 1 | 3 | 4) && self.form(form as u8).weapon == 0 {
            let taken = self.take_first_of_type(item_type::KEYBLADE);
            self.set_form_weapon(form as u8, taken);
        }
    }

    /// `FUN_001a20b0(type)`: the first item of the type (table order) with a count, minus one.
    fn take_first_of_type(&mut self, kind: u8) -> u16 {
        let found = self.items.rows.iter().find(|r| r.kind == kind && self.item_count(r.id) >= 1).map(|r| r.id);
        match found {
            Some(id) => {
                self.use_item(id, 1);
                id
            }
            None => 0,
        }
    }

    /// `FUN_00187728`: a consumable into the member's first free battle item slot whose reserved
    /// id is this item or empty (an empty reserved slot takes the id when another slot reserves it).
    fn put_battle_item(&mut self, r: usize, id: u16) {
        let n = self.u8_at(r + rec::ITEM_SLOTS) as usize;
        for i in 0..n {
            if self.u16_at(r + rec::ITEMS + 2 * i) != 0 {
                continue;
            }
            let reserved = self.u16_at(r + rec::RESERVED + 2 * i);
            if reserved == id {
                self.set_u16(r + rec::ITEMS + 2 * i, id);
                return;
            }
            if reserved == 0 {
                if (0..n).any(|j| self.u16_at(r + rec::RESERVED + 2 * j) == id) {
                    self.set_u16(r + rec::RESERVED + 2 * i, id);
                }
                self.set_u16(r + rec::ITEMS + 2 * i, id);
                return;
            }
        }
    }

    /// A member's battle item slots as (item, reserved id) pairs, `+0x34` and `+0x44`, for the first
    /// `+0x12` slots (the field Items list `FUN_001bc910` reads both).
    pub fn battle_items(&self, member: u16) -> Vec<(u16, u16)> {
        let Some(r) = Self::member_off(member) else { return Vec::new() };
        let n = (self.u8_at(r + rec::ITEM_SLOTS) as usize).min(8);
        (0..n).map(|i| (self.u16_at(r + rec::ITEMS + 2 * i), self.u16_at(r + rec::RESERVED + 2 * i))).collect()
    }

    /// `FUN_001a1ba8(id, to)`: take one item away. `to == INVENTORY`: one off the stock count
    /// (`FUN_001a1c30(id, 1)`), always true. A member: the LAST of its battle item slots that holds
    /// the item is emptied (the reserved id stays); false when none holds it. Using an item from the
    /// field menu takes it this way from the user's slots (`FUN_00175ec0` -> here with the user).
    pub fn take_item(&mut self, id: u16, to: u16) -> bool {
        if to == INVENTORY {
            self.use_item(id, 1);
            return true;
        }
        let Some(r) = Self::member_off(to) else { return false };
        let n = (self.u8_at(r + rec::ITEM_SLOTS) as usize).min(8);
        for i in (0..n).rev() {
            if self.u16_at(r + rec::ITEMS + 2 * i) == id {
                self.set_u16(r + rec::ITEMS + 2 * i, 0);
                return true;
            }
        }
        false
    }

    /// `FUN_001877c8`: every empty battle item slot that reserves an item takes one from the stock
    /// when the stock has it (`FUN_001a1cc8` >= 1, then `FUN_001a1c30(id, 1)`).
    pub fn refill_battle_items(&mut self, member: u16) {
        let Some(r) = Self::member_off(member) else { return };
        let n = (self.u8_at(r + rec::ITEM_SLOTS) as usize).min(8);
        for i in 0..n {
            let reserved = self.u16_at(r + rec::RESERVED + 2 * i);
            if reserved != 0 && self.u16_at(r + rec::ITEMS + 2 * i) == 0 && self.item_count(reserved) >= 1 {
                self.use_item(reserved, 1);
                self.set_u16(r + rec::ITEMS + 2 * i, reserved);
            }
        }
    }

    /// `FUN_0026c698(slot, item)` (pause menu, Items category): the slot's item goes back to the stock
    /// (`FUN_001a18d8(old, 100, 0)`); then, when `item` is 0 or `carried`, the reserved id is cleared, the
    /// slot takes `item` and one comes off the stock (`FUN_001a1ba8(item, 100)`); otherwise the item
    /// only becomes the slot's reserved id and the slot stays empty. `carried` is the menu row's flag
    /// (the byte `(row - 1) * 6 + 2` of the page's list table): the list entry's count byte
    /// (`FUN_00283a70` stores the stock count there; the recent items `FUN_00283e50` adds with 0 are not carried).
    /// False when the member or the slot does not exist.
    pub fn equip_battle_item(&mut self, member: u16, slot: usize, item: u16, carried: bool) -> bool {
        let Some(r) = Self::member_off(member) else { return false };
        if slot >= 8 {
            return false;
        }
        let o = r + rec::ITEMS + 2 * slot;
        let old = self.u16_at(o);
        if old != 0 {
            self.give(old, INVENTORY, false);
        }
        if item == 0 || carried {
            self.set_u16(r + rec::RESERVED + 2 * slot, 0);
            self.set_u16(o, item);
            if item != 0 {
                self.take_item(item, INVENTORY);
            }
        } else {
            self.set_u16(r + rec::RESERVED + 2 * slot, item);
            self.set_u16(o, 0);
        }
        true
    }

    /// `FUN_0026c420(item)` (pause menu, Items, Weapons): the selected member's main weapon (record `+0`). When it
    /// differs, the old one goes back to the stock (`FUN_001a18d8(old, 100, 0)`, called whether the old id is 0 or
    /// not), the record takes the new one and one comes off the stock (`FUN_001a1ba8(new, 100)`). The original then
    /// queues a live-weapon change (`FUN_002a1060(member, 0, item, 0)`); the app's caller refreshes `Abilities`.
    /// Answers whether anything changed (the original's return value).
    pub fn equip_weapon(&mut self, member: u16, item: u16) -> bool {
        let Some(r) = Self::member_off(member) else { return false };
        let old = self.u16_at(r + rec::WEAPONS);
        if old == item {
            return false;
        }
        self.give(old, INVENTORY, false);
        self.set_u16(r + rec::WEAPONS, item);
        self.take_item(item, INVENTORY);
        true
    }

    /// `FUN_0026c4d0(form, item)`: a drive form's second keyblade (form record `+0`), the same swap on the form
    /// record. The live weapon is refreshed by the caller when `form` is the current form (`FUN_00282df8`, the
    /// byte at save `+0x242c`: `current_form()`).
    pub fn equip_form_weapon(&mut self, form: u8, item: u16) -> bool {
        let Some(r) = Self::form_off(form) else { return false };
        let old = self.u16_at(r);
        if old == item {
            return false;
        }
        self.give(old, INVENTORY, false);
        self.set_u16(r, item);
        self.take_item(item, INVENTORY);
        true
    }

    /// `FUN_0026c588(slot, item)`: an armor slot (record `+0x14 + 2 * slot`). The old item goes back to the stock
    /// when there was one, the slot takes the new one and one comes off the stock when it is not 0. False when the
    /// slot already holds the item (the original's 0) or does not exist.
    pub fn equip_armor(&mut self, member: u16, slot: usize, item: u16) -> bool {
        self.equip_slot(member, rec::ARMOR_SLOTS, rec::ARMOR, slot, item)
    }

    /// `FUN_0026c610(slot, item)`: an accessory slot (record `+0x24 + 2 * slot`), as `equip_armor`.
    pub fn equip_accessory(&mut self, member: u16, slot: usize, item: u16) -> bool {
        self.equip_slot(member, rec::ACCESSORY_SLOTS, rec::ACCESSORIES, slot, item)
    }

    fn equip_slot(&mut self, member: u16, count_at: usize, list_at: usize, slot: usize, item: u16) -> bool {
        let Some(r) = Self::member_off(member) else { return false };
        // The original does not look at the slot count (the page only offers existing slots).
        if slot >= self.u8_at(r + count_at) as usize {
            return false;
        }
        let o = r + list_at + 2 * slot;
        let old = self.u16_at(o);
        if old == item {
            return false;
        }
        if old != 0 {
            self.give(old, INVENTORY, false);
        }
        self.set_u16(o, item);
        if item != 0 {
            self.take_item(item, INVENTORY);
        }
        true
    }

    /// What the pause menu runs when it leaves an equipment list page (`FUN_0026a528`, after the leave animation
    /// and on a close): `FUN_0026c770` then `FUN_001877c8`. The first re-reserves: the reserved ids of the
    /// member's slots are collected in slot order, and every slot that carries one of those items takes it as its
    /// reserved id (so an item moved between slots keeps its Auto-Reload); the second refills every empty slot
    /// that reserves an item from the stock (`refill_battle_items`).
    pub fn settle_battle_items(&mut self, member: u16) {
        let Some(r) = Self::member_off(member) else { return };
        let n = (self.u8_at(r + rec::ITEM_SLOTS) as usize).min(8);
        let reserved: Vec<u16> = (0..n).map(|i| self.u16_at(r + rec::RESERVED + 2 * i)).filter(|&v| v != 0).collect();
        for i in 0..n {
            let carried = self.u16_at(r + rec::ITEMS + 2 * i);
            if let Some(&v) = reserved.iter().find(|&&v| v == carried) {
                self.set_u16(r + rec::RESERVED + 2 * i, v);
            }
        }
        self.refill_battle_items(member);
    }

    /// `FUN_00274f70`, the slot loop (the Customize page's Auto-Reload switch): every battle item slot of the
    /// member that holds the item or reserves it gets its reserved id set to the item (on) or cleared (off).
    /// The items stay where they are.
    pub fn set_auto_reload(&mut self, member: u16, id: u16, on: bool) {
        let Some(r) = Self::member_off(member) else { return };
        let n = (self.u8_at(r + rec::ITEM_SLOTS) as usize).min(8);
        for i in 0..n {
            if self.u16_at(r + rec::ITEMS + 2 * i) == id || self.u16_at(r + rec::RESERVED + 2 * i) == id {
                self.set_u16(r + rec::RESERVED + 2 * i, if on { id } else { 0 });
            }
        }
    }

    /// `FUN_00275288` for the leader: whether turning Auto-Reload off for an item that is on needs the question
    /// first, which is when a slot reserves the item without carrying it (an empty slot waiting for a refill).
    pub fn reload_off_needs_question(&self, member: u16, id: u16) -> bool {
        self.battle_items(member).iter().any(|&(item, reserved)| item != id && reserved == id)
    }

    /// The 8 consumable ids Sora received (`0x340e50`, `FUN_001a2120` fills it); the field menu shows
    /// its Items entry only once the first is set (`FUN_001bbca0`).
    pub fn recent_items(&self) -> [u16; 8] {
        std::array::from_fn(|i| self.u16_at(off::RECENT + 2 * i))
    }

    /// `FUN_00187868`: an ability into the first empty slot of the member's list.
    fn put_ability(&mut self, r: usize, id: u16, equipped: bool) {
        for i in 0..rec::ABILITY_SLOTS {
            let o = r + rec::ABILITIES + 2 * i;
            if self.u16_at(o) & 0x7fff == 0 {
                self.set_u16(o, id & 0x7fff | (equipped as u16) << 15);
                return;
            }
        }
    }

    /// Test aid: a record's maximum HP and MP.
    #[cfg(test)]
    pub(crate) fn set_member_maxima(&mut self, member: u16, hp_max: u8, mp_max: u8) {
        if let Some(r) = Self::member_off(member) {
            self.set_u8(r + rec::HP_MAX, hp_max);
            self.set_u8(r + rec::MP_MAX, mp_max);
        }
    }

    /// Test aid: an ability into the first empty slot of a member's list (`put_ability` by member id).
    #[cfg(test)]
    pub(crate) fn give_ability(&mut self, member: u16, id: u16, equipped: bool) {
        if let Some(r) = Self::member_off(member) {
            self.put_ability(r, id, equipped);
        }
    }

    /// The member's ability list with the raw slot index (0..64) of each non-empty slot: the menu's
    /// rows keep the slot (`FUN_0027a7e8` stores the loop index in the row) because the toggle
    /// writes that slot (`FUN_0027d8b8`).
    pub fn ability_slots(&self, member: u16) -> Vec<(usize, u16, bool)> {
        let Some(r) = Self::member_off(member) else { return Vec::new() };
        (0..rec::ABILITY_SLOTS)
            .map(|i| (i, self.u16_at(r + rec::ABILITIES + 2 * i)))
            .filter(|&(_, v)| v & 0x7fff != 0)
            .map(|(i, v)| (i, v & 0x7fff, v & 0x8000 != 0))
            .collect()
    }

    /// `FUN_0027d8b8`: the equipped flag (bit 15) of one slot of a member's ability list is written
    /// as `(old & 0x7fff) | on << 15`. False when the member or the slot does not exist.
    pub fn set_ability_equipped(&mut self, member: u16, slot: usize, on: bool) -> bool {
        let Some(r) = Self::member_off(member) else { return false };
        if slot >= rec::ABILITY_SLOTS {
            return false;
        }
        let o = r + rec::ABILITIES + 2 * slot;
        let old = self.u16_at(o);
        self.set_u16(o, old & 0x7fff | (on as u16) << 15);
        true
    }

    /// `FUN_001d3f18(record + 0x54, 0x40, form, level)` on a member's 64-slot list: the first slot whose
    /// id (`& 0x7fff`) is `old` takes `new`, keeping its bit 15; at most one slot changes. `old` 0 matches
    /// the first empty slot, which has bit 15 clear, so a new growth ability arrives **unequipped**
    /// (`FUN_001d3da8` calls it for Sora at the form row's display level minus one; `old` / `new` are
    /// the growth chain's ids at 0x37c556, `prize::GrowthChains`). Returns the slot written, `None` when
    /// no slot matches (nothing changes) or the member does not exist.
    pub fn upgrade_ability(&mut self, member: u16, old: u16, new: u16) -> Option<usize> {
        let r = Self::member_off(member)?;
        let slot = (0..rec::ABILITY_SLOTS).find(|&i| self.u16_at(r + rec::ABILITIES + 2 * i) & 0x7fff == old)?;
        let o = r + rec::ABILITIES + 2 * slot;
        let v = self.u16_at(o);
        self.set_u16(o, v & 0x8000 | new & 0x7fff);
        Some(slot)
    }

    /// `FUN_00187f18`: an ability into the first empty slot of a form's list, always equipped.
    fn put_form_ability(&mut self, r: usize, v: u16) {
        for i in 0..16 {
            let o = r + 8 + 2 * i;
            let old = self.u16_at(o);
            if old & 0x7fff == 0 {
                self.set_u16(o, old & 0x8000 | v & 0x7fff | 0x8000);
                return;
            }
        }
    }

    /// `FUN_00187658`: change a member's number of battle item slots; slots that disappear give
    /// their item back to the inventory.
    fn set_item_slots(&mut self, r: usize, n: u8) {
        let cur = self.u8_at(r + rec::ITEM_SLOTS);
        if cur < n {
            for i in cur as usize..n as usize {
                self.set_u16(r + rec::ITEMS + 2 * i, 0);
                self.set_u16(r + rec::RESERVED + 2 * i, 0);
            }
        } else if n < cur {
            for i in n as usize..cur as usize {
                let v = self.u16_at(r + rec::ITEMS + 2 * i);
                if v != 0 {
                    self.give(v, INVENTORY, false);
                    self.set_u16(r + rec::ITEMS + 2 * i, 0);
                }
            }
        }
        self.set_u8(r + rec::ITEM_SLOTS, n);
    }

    /// Keyblades in the inventory, one id per copy, in table order.
    pub fn keyblades(&self) -> Vec<u16> {
        self.items
            .rows
            .iter()
            .filter(|r| r.kind == item_type::KEYBLADE)
            .flat_map(|r| std::iter::repeat_n(r.id, self.item_count(r.id) as usize))
            .collect()
    }

    /// Sora's equipped items: the weapons, then the armor and accessory slots (non-empty).
    pub fn equipment(&self) -> Vec<u16> {
        let s = self.sora();
        s.weapons.iter().chain(&s.armor).chain(&s.accessories).copied().filter(|&i| i != 0).collect()
    }

    /// Everything owned in the inventory as (item id, count), in table order.
    pub fn inventory(&self) -> Vec<(u16, u8)> {
        self.items.rows.iter().map(|r| (r.id, self.item_count(r.id))).filter(|&(_, c)| c > 0).collect()
    }

    // --- the new game -------------------------------------------------------------------------

    /// The new-game state: the party block after `FUN_00187b38` on the cleared save block and the
    /// `plrp` rows with `Id` 0 (`FUN_001c0cb0(0)`).
    pub fn new_game(items: Arc<ItemIndex>, plrp: &[Plrp]) -> Progress {
        let mut p = Progress {
            block: vec![0; off::LEN],
            items,
            source: Source::NewGame,
            baseline: Vec::new(), boot_weapons: Arc::default(),
            dodge_roll: 0,
            wallet: 0,
            config: [OPTION_DEFAULT, GAME_MODE_DEFAULT],
            story: StoryHandle::default(),
            new_seen: Vec::new(),
            record_bits: Vec::new(),
            moogle: Moogle::default(),
            shop_seen: [0; 4],
            shop_visits: 0,
            journal_seen: Default::default(),
            minigames: Default::default(),
            enemy_kills: Default::default(),
            reaction_uses: Default::default(),
            limit_best: Default::default(),
            report: [0; REPORT_WORDS],
        };
        p.init_party();
        p.apply_plrp(plrp, 0);
        p
    }

    /// `FUN_00187b38(0x33fec0)`. The stores of zero are kept so the port reads like the routine.
    fn init_party(&mut self) {
        for m in 0..off::MEMBERS {
            self.init_member(m * off::MEMBER_SIZE);
        }
        for f in 1..=off::FORMS {
            // FUN_00187ed0: weapon 0, level 1, +3 = 1, experience 0, 16 empty ability slots.
            let r = off::FORM_BASE + f * off::FORM_SIZE;
            self.set_u16(r, 0);
            self.set_u8(r + 3, 1);
            self.set_u8(r + 2, 1);
            self.set_u32(r + 4, 0);
            self.zero(r + 8, 32);
        }
        self.set_u8(off::CURRENT_FORM, 0);
        self.set_u8(off::DRIVE + 2, 3);
        self.set_u8(off::SUMMON_LEVEL, 1);
        self.set_u8(off::DRIVE, 100);
        self.set_u8(off::SUMMON, 0);
        self.set_u8(off::DRIVE + 1, 3);
        for w in 0..off::WORLD_COUNT {
            // FUN_001a71a8: {0, 0x10, 0x10, 0x10}.
            let o = off::WORLDS + 4 * w;
            self.block[o..o + 4].copy_from_slice(&[0, 0x10, 0x10, 0x10]);
        }
        self.zero(off::KEYS, 4 * off::KEY_WORDS);
        self.set_u32(off::EXPERIENCE, 0);
        self.zero(0xfac, 32);
        self.zero(0xfcc, 32);
        self.set_u32(off::SPELL_SWITCH, 0);
        self.zero(off::OBTAINED, 4 * off::OBTAINED_WORDS);
        self.set_u32(0x1658, 0);
        for k in 0..32 {
            let o = 0x165c + 8 * k + 2;
            let v = self.u16_at(o) & 0x8000;
            self.set_u16(o, v);
        }
        for k in 0..64 {
            let o = 0x1760 + 8 * k;
            self.set_u8(o, 0);
            self.set_u32(o + 4, 0);
        }
        self.zero(0x1960, 16);
        self.zero(0x197a, 26);
        self.set_u16(0x1994, 0);
        self.zero(off::SHORTCUTS, 8);
    }

    /// `FUN_001875c8`: a member record's defaults.
    fn init_member(&mut self, r: usize) {
        self.set_u8(r + rec::STRENGTH, 0);
        self.set_u8(r + rec::MP_MAX, 100);
        self.set_u8(r + rec::ACCESSORY_SLOTS, 1);
        self.set_u8(r + rec::HP, 100);
        self.set_u8(r + rec::HP_MAX, 100);
        self.set_u8(r + rec::MP, 100);
        for o in [rec::MAGIC, rec::DEFENSE, 0xc, 0xd, rec::WEAPON_SET] {
            self.set_u8(r + o, 0);
        }
        self.set_u8(r + rec::LEVEL, 1);
        self.set_u8(r + rec::ARMOR_SLOTS, 1);
        self.set_u8(r + rec::ITEM_SLOTS, 0);
        self.zero(r + rec::ABILITIES, 2 * rec::ABILITY_SLOTS);
        self.zero(r + rec::SETTINGS, rec::SETTINGS_LEN);
    }

    /// `FUN_001c0cb0(id)`: the first `plrp` row with this `Id` and the rows after it with the same
    /// `Id`, each through `FUN_001c0b80`.
    fn apply_plrp(&mut self, plrp: &[Plrp], id: u16) {
        let Some(first) = plrp.iter().position(|r| r.id == id) else { return };
        for row in plrp[first..].iter().take_while(|r| r.id == id) {
            self.apply_plrp_row(row);
        }
    }

    /// `FUN_001c0b80`: a row for a party member (`Character` < 0x80) sets the record's HP, MP, AP,
    /// strength, magic, defense and slot counts, then gives each item (bit 15 = equipped); a row
    /// for a form (`Character` 0x80 | form) adds each item to the form's ability list.
    fn apply_plrp_row(&mut self, row: &Plrp) {
        let c = row.character;
        let (member, form) = if c & 0x80 == 0 { (Self::member_off(c as u16), None) } else { (None, Self::form_off(c & 0x7f)) };
        if let Some(r) = member {
            self.set_u8(r + rec::HP_MAX, row.hp);
            self.set_u8(r + rec::HP, row.hp);
            self.set_u8(r + rec::MP_MAX, row.mp);
            self.set_u8(r + rec::MP, row.mp);
            self.set_u8(r + rec::AP, row.ap);
            self.set_u8(r + rec::STRENGTH, row.strength);
            self.set_u8(r + rec::MAGIC, row.magic);
            self.set_u8(r + rec::DEFENSE, row.defense);
            self.set_u8(r + rec::ARMOR_SLOTS, row.armor_slot_max);
            self.set_u8(r + rec::ACCESSORY_SLOTS, row.accessory_slot_max);
            self.set_item_slots(r, row.item_slot_max);
        }
        for &v in row.items.iter().filter(|&&v| v != 0) {
            match form {
                None => {
                    self.give(v & 0x7fff, c as u16, v >> 15 != 0);
                }
                Some(f) => self.put_form_ability(f, v),
            }
        }
    }

    // --- start state --------------------------------------------------------------------------

    /// The app baseline on top of the new game (see the module doc). Records what it adds.
    pub fn apply_baseline(&mut self, guard: bool) {
        let mut added = Vec::new();
        let missing: Vec<u8> = (1..=4u8).filter(|&f| !self.owns_form(f) && self.items.form_items[f as usize] != 0).collect();
        // The forms that take a keyblade when their item is obtained (FUN_001a2010) find one
        // spare Kingdom Key each, as the app always had.
        let takers = missing.iter().filter(|&&f| matches!(f, 1 | 3 | 4) && self.form(f).weapon == 0).count() as u8;
        if takers > 0 {
            let n = self.add_item(KINGDOM_KEY, takers);
            added.push(format!("{n} spare Kingdom Key (item {KINGDOM_KEY})"));
        }
        for f in missing {
            let item = self.items.form_items[f as usize];
            if self.add_item(item, 1) == 1 {
                added.push(format!("form {f} item {item}"));
            }
        }
        for slot in 0..4u8 {
            let item = self.items.magic_items[slot as usize];
            if item != 0 && self.spell_level(slot) == 0 && self.add_item(item, 1) == 1 {
                added.push(format!("spell slot {slot} item {item} level 1"));
            }
        }
        if let Some(r) = Self::member_off(SORA) {
            if self.u16_at(r + rec::WEAPONS) == 0 {
                self.set_u16(r + rec::WEAPONS, KINGDOM_KEY);
                added.push(format!("Kingdom Key (item {KINGDOM_KEY}) equipped"));
            }
            let guard_id = crate::abilities::id::GUARD;
            if guard && !self.sora().abilities.iter().any(|&(id, _)| id == guard_id) {
                self.put_ability(r, guard_id, true);
                added.push(format!("Guard (item {guard_id}) equipped"));
            }
        }
        added.extend(self.apply_party_weapons());
        // The user's wish (2026-10-06): Sora knows the Dodge Roll. It goes with the Guard switch:
        // `KH2_ABILITY_START=data` (no Guard) starts without it too, so the original's rules are reachable.
        if guard && self.dodge_roll == 0 {
            self.dodge_roll = 1;
            added.push("Dodge Roll growth level 1".to_string());
        }
        self.baseline = added;
    }

    /// WP-RV3: the boot weapon words of the party records and form records (`BootWeapons`: `00common.bdx` event 7, host 0.75 /
    /// 0.76) where a record's first word is 0, so every member fights with the weapon the game object's constructor gives him.
    /// A state with a `block` keeps its words. The new game itself leaves them 0 (no `plrp` row carries a weapon item, docs/party.md).
    /// Returns what it set (also run for `KH2_STATE` files that carry no `block`).
    pub fn apply_party_weapons(&mut self) -> Vec<String> {
        let table = self.boot_weapons.clone();
        let mut added = Vec::new();
        for &(member, w0, w1) in &table.members {
            if let Some(r) = Self::member_off(member)
                && self.u16_at(r + rec::WEAPONS) == 0
                && w0 != 0
            {
                self.set_u16(r + rec::WEAPONS, w0);
                self.set_u16(r + rec::WEAPONS + 2, w1);
                added.push(format!("member {member} weapon item {w0} equipped"));
            }
        }
        for &(form, w) in &table.forms {
            if w != 0 && self.form(form).weapon == 0 {
                self.set_form_weapon(form, w);
                added.push(format!("form {form} weapon item {w}"));
            }
        }
        added
    }

    /// Applies a state file (schema in docs/progress.md) on top of this state.
    pub fn apply_json(&mut self, text: &str) -> Result<(), String> {
        // The story is a shared handle: an old-format slot resets it before its other keys are parsed, so a failure restores it.
        let story = self.story().clone();
        let r = self.apply_json_keys(text);
        if r.is_err() {
            *self.story() = story;
        }
        r
    }

    fn apply_json_keys(&mut self, text: &str) -> Result<(), String> {
        let j = Json::parse(text)?;
        let Json::Obj(fields) = &j else { return Err("the state must be a JSON object".into()) };
        let int = |v: &Json, what: &str| v.int().ok_or_else(|| format!("{what}: expected an integer"));
        let in_range = |n: i64, max: i64, what: &str| if (0..=max).contains(&n) { Ok(n) } else { Err(format!("{what}: {n} is out of range (0..={max})")) };
        let id_of = |k: &str, what: &str| k.trim().parse::<u16>().map_err(|_| format!("{what}: '{k}' is not an id"));
        let list = |v: &Json, what: &str| -> Result<Vec<u16>, String> {
            match v {
                Json::Arr(a) => a.iter().map(|x| int(x, what).map(|n| n as u16)).collect(),
                _ => Err(format!("{what}: expected an array")),
            }
        };
        // A slot file is a whole state: one without the story arrays (an older file) loads the new-game story, not the
        // running session's flags and program table.
        if j.get("block").is_some() && j.get("story_raw").is_none() {
            self.story().reset_state();
        }
        for (key, v) in fields {
            match key.as_str() {
                "experience" => self.set_experience(int(v, "experience")? as i32),
                "munny" => self.set_wallet(int(v, "munny")?.max(0) as u32),
                "items" => {
                    let Json::Obj(m) = v else { return Err("items: expected an object of id: count".into()) };
                    for (k, n) in m {
                        self.set_count(id_of(k, "items")?, int(n, "items")?.clamp(0, 255) as u8);
                    }
                }
                "magic" => {
                    for (slot, n) in list(v, "magic")?.into_iter().enumerate().take(6) {
                        let item = self.items.magic_items[slot];
                        self.set_count(item, n.min(255) as u8);
                    }
                }
                "weapon" => {
                    let r = Self::member_off(SORA).unwrap();
                    self.set_u16(r + rec::WEAPONS, int(v, "weapon")? as u16);
                }
                "armor" | "accessories" => {
                    let (count, base) = if key == "armor" { (rec::ARMOR_SLOTS, rec::ARMOR) } else { (rec::ACCESSORY_SLOTS, rec::ACCESSORIES) };
                    let ids = list(v, key)?;
                    let r = Self::member_off(SORA).unwrap();
                    let n = (ids.len().min(8) as u8).max(self.u8_at(r + count));
                    self.set_u8(r + count, n);
                    self.zero(r + base, 16);
                    for (i, id) in ids.iter().take(8).enumerate() {
                        self.set_u16(r + base + 2 * i, *id);
                    }
                }
                "abilities" | "abilities_off" => {}
                // Each id is given to Sora through the original's give (`FUN_001a18d8(id, 1, 0)`, what a
                // pickup does): a consumable fills his first free battle item slot (else the stock) and
                // joins the received list.
                "sora_items" => {
                    for id in list(v, "sora_items")? {
                        self.give(id, SORA, false);
                    }
                }
                "dodge_roll" => self.dodge_roll = int(v, "dodge_roll")?.clamp(0, 5) as u8,
                "forms" => {
                    let Json::Obj(m) = v else { return Err("forms: expected an object of form id: {...}".into()) };
                    for (k, f) in m {
                        let form = id_of(k, "forms")? as u8;
                        if Self::form_off(form).is_none() {
                            return Err(format!("forms: no form record {form}"));
                        }
                        if f.get("owned").is_some_and(|o| *o == Json::Bool(true)) && !self.owns_form(form) {
                            let item = self.items.form_items.get(form as usize).copied().unwrap_or(0);
                            self.add_item(item, 1);
                        }
                        if let Some(l) = f.get("level") {
                            self.set_form_level(form, int(l, "forms.level")? as u8);
                        }
                        if let Some(e) = f.get("exp") {
                            self.set_form_exp(form, int(e, "forms.exp")? as u32);
                        }
                        if let Some(w) = f.get("weapon") {
                            self.set_form_weapon(form, int(w, "forms.weapon")? as u16);
                        }
                    }
                }
                "summon" => {
                    if let Some(l) = v.get("level") {
                        self.set_u8(off::SUMMON_LEVEL, int(l, "summon.level")? as u8);
                    }
                    if let Some(e) = v.get("exp") {
                        self.set_u32(off::SUMMON_EXP, int(e, "summon.exp")? as u32);
                    }
                }
                "outfits" => {
                    let Json::Obj(m) = v else { return Err("outfits: expected an object of world: outfit".into()) };
                    for (k, o) in m {
                        self.set_outfit(id_of(k, "outfits")? as usize, int(o, "outfits")? as u8);
                    }
                }
                "spell_switch" => self.set_u32(off::SPELL_SWITCH, int(v, "spell_switch")? as u32),
                "option_word" => self.config[0] = int(v, "option_word")? as u32,
                "game_mode" => self.config[1] = int(v, "game_mode")?.clamp(0, 2) as u32,
                // Story flags: each id set through the game's setter `FUN_0017ff60` (its side effects
                // from `00progress.bin` run: area programs, exit locks, other flags), in file order.
                "story_flags" => {
                    let mut s = self.story();
                    for id in list(v, "story_flags")? {
                        s.set(id);
                    }
                }
                // Each id cleared through `FUN_00180298` (only the bit; no side effects).
                "story_clear" => {
                    let mut s = self.story();
                    for id in list(v, "story_clear")? {
                        s.clear(id);
                    }
                }
                // World-point bits (0x33f910), set through `FUN_00194fa8`.
                "world_points" => {
                    let mut s = self.story();
                    for id in list(v, "world_points")? {
                        s.set_world_point(id as usize);
                    }
                }
                // The moogle's synthesis record (save +0x3728): `{"unlocks": [bits], "points": n, "level": n, "rewards": [bits]}`.
                "moogle" => {
                    if let Some(u) = v.get("unlocks") {
                        for b in list(u, "moogle.unlocks")? {
                            Moogle::with(&mut self.moogle.unlocks, u32::from(b));
                        }
                    }
                    if let Some(n) = v.get("points") {
                        self.moogle.points = int(n, "moogle.points")?.clamp(0, 99_999) as u32;
                    }
                    if let Some(n) = v.get("level") {
                        self.moogle.level = int(n, "moogle.level")?.clamp(0, 8) as u8;
                    }
                    if let Some(u) = v.get("rewards") {
                        for b in list(u, "moogle.rewards")? {
                            Moogle::with(&mut self.moogle.rewards, u32::from(b));
                        }
                    }
                    for (name, field) in [("known", 0), ("learned", 1), ("made", 2), ("seen", 3)] {
                        if let Some(u) = v.get(name) {
                            let set = match field {
                                0 => &mut self.moogle.known,
                                1 => &mut self.moogle.learned,
                                2 => &mut self.moogle.made,
                                _ => &mut self.moogle.seen,
                            };
                            for b in list(u, "moogle bit set")? {
                                Moogle::with(set, u32::from(b));
                            }
                        }
                    }
                    for (name, totals) in [("stock", false), ("totals", true)] {
                        if let Some(Json::Arr(a)) = v.get(name) {
                            for (i, x) in a.iter().enumerate().take(MOOGLE_MATERIALS) {
                                let n = int(x, "moogle.stock")?.clamp(0, 999) as u32;
                                if totals {
                                    self.moogle.set_total(i, n);
                                } else {
                                    self.moogle.set_stock(i, n);
                                }
                            }
                        }
                    }
                }
                // The shops' seen bits (save +0x3714, places in the shop file's item list) and first-visit bits
                // (save +0x3724): `"shop_seen": [places]`, `"shop_visits": [bits]`.
                "shop_seen" => {
                    for n in list(v, "shop_seen")? {
                        self.set_shop_seen(usize::from(n));
                    }
                }
                // The opened chests (`OverallChestIndex` numbers, bits of the record array from 224 on) and the journal's seen
                // set (save +0x38f0): `"chests": [indexes]`, `"journal_seen": [[category, entry], ..]`.
                "chests" => {
                    for n in list(v, "chests")? {
                        self.set_chest_opened(n);
                    }
                }
                "journal_seen" => {
                    let Json::Arr(a) = v else { return Err("journal_seen: expected an array of [category, entry]".into()) };
                    for p in a {
                        let Json::Arr(p) = p else { return Err("journal_seen: expected [category, entry] pairs".into()) };
                        let (Some(c), Some(e)) = (p.first(), p.get(1)) else { return Err("journal_seen: expected [category, entry] pairs".into()) };
                        self.set_journal_seen(int(c, "journal_seen")?.clamp(0, 255) as u8, int(e, "journal_seen")?.max(0) as u32);
                    }
                }
                // The minigame records: `"minigames": [[index, state, score], ..]` (`0x341620 + 8 * index`).
                "minigames" => {
                    let Json::Arr(a) = v else { return Err("minigames: expected an array of [index, state, score]".into()) };
                    for p in a {
                        let Json::Arr(p) = p else { return Err("minigames: expected [index, state, score] triples".into()) };
                        let (Some(i), Some(st)) = (p.first(), p.get(1)) else { return Err("minigames: expected [index, state, score] triples".into()) };
                        let score = p.get(2).map_or(Ok(0), |x| int(x, "minigames"))?;
                        self.set_minigame(int(i, "minigames")?.clamp(0, 0xffff) as u16, int(st, "minigames")?.clamp(0, 255) as u8, score.clamp(0, i64::from(u32::MAX)) as u32);
                    }
                }
                // The defeated-enemy counters: `"enemy_kills": [[NeoStatus, count], ..]` (save `+0x1660`, `FUN_00187dc8`).
                "enemy_kills" => {
                    let Json::Arr(a) = v else { return Err("enemy_kills: expected an array of [id, count] pairs".into()) };
                    for p in a {
                        let Json::Arr(p) = p else { return Err("enemy_kills: expected [id, count] pairs".into()) };
                        let (Some(i), Some(c)) = (p.first(), p.get(1)) else { return Err("enemy_kills: expected [id, count] pairs".into()) };
                        self.set_enemy_defeats(int(i, "enemy_kills")?.clamp(0, 0xffff) as u32, int(c, "enemy_kills")?.clamp(0, 9999) as u32);
                    }
                }
                // The reaction commands' use counts: `"reaction_uses": [[slot, count], ..]` (`0x3410b0`, `FUN_001b7168`).
                "reaction_uses" => {
                    let Json::Arr(a) = v else { return Err("reaction_uses: expected an array of [slot, count] pairs".into()) };
                    for p in a {
                        let Json::Arr(p) = p else { return Err("reaction_uses: expected [slot, count] pairs".into()) };
                        let (Some(i), Some(c)) = (p.first(), p.get(1)) else { return Err("reaction_uses: expected [slot, count] pairs".into()) };
                        self.set_reaction_use(int(i, "reaction_uses")?.clamp(0, 0xffff) as u16, int(c, "reaction_uses")?.clamp(0, 999) as u16);
                    }
                }
                // The limits' best counts: `"limit_best": [[slot, count], ..]` (`0x3414b0`, `FUN_001d25b0`).
                "limit_best" => {
                    let Json::Arr(a) = v else { return Err("limit_best: expected an array of [slot, count] pairs".into()) };
                    for p in a {
                        let Json::Arr(p) = p else { return Err("limit_best: expected [slot, count] pairs".into()) };
                        let (Some(i), Some(c)) = (p.first(), p.get(1)) else { return Err("limit_best: expected [slot, count] pairs".into()) };
                        self.raise_limit_best(int(i, "limit_best")?.clamp(0, 255) as u8, int(c, "limit_best")?.clamp(0, 0xffff) as u16);
                    }
                }
                // The BATTLE REPORT counters: `"report_counters": [healed, saved by Mickey, ten words from 0x341840]`.
                "report_counters" => {
                    let Json::Arr(a) = v else { return Err("report_counters: expected an array of numbers".into()) };
                    if a.len() != REPORT_WORDS {
                        return Err(format!("report_counters: expected {REPORT_WORDS} numbers"));
                    }
                    for (i, x) in a.iter().enumerate() {
                        self.report[i] = int(x, "report_counters")?.clamp(0, 999) as u16;
                    }
                }
                "shop_visits" => {
                    for n in list(v, "shop_visits")? {
                        self.set_shop_visit(n as u8);
                    }
                }
                // The members' AI settings (record +0xd4): `{"2": 2, "3": [3, 1]}` = member id -> the style byte, or the
                // bytes from byte 0 on (docs/progress.md).
                "party_settings" => {
                    let Json::Obj(m) = v else { return Err("party_settings: expected an object of member id: byte or bytes".into()) };
                    for (k, b) in m {
                        let member = id_of(k, "party_settings")?;
                        if Self::member_off(member).is_none() {
                            return Err(format!("party_settings: no member record {member}"));
                        }
                        // Rejected, not truncated: a value past 255 is no byte and byte 0 (the battle style) is 0..5
                        // (the six rows of the style table, docs/party.md).
                        let items: Vec<&Json> = match b {
                            Json::Arr(a) => a.iter().collect(),
                            one => vec![one],
                        };
                        let mut bytes = Vec::new();
                        for (i, x) in items.into_iter().enumerate().take(rec::SETTINGS_LEN) {
                            let n = int(x, "party_settings")?;
                            if !(0..=255).contains(&n) || (i == 0 && n > 5) {
                                return Err(format!("party_settings: member {member} byte {i} = {n} is out of range (0..=255, the style 0..=5)"));
                            }
                            bytes.push(n as u8);
                        }
                        for (i, x) in bytes.into_iter().enumerate() {
                            self.set_member_setting(member, i, x);
                        }
                    }
                }
                // WP-AH1b harness keys: a member's current HP and MP (record `+4`, `+6`, held to the maxima) and equipped
                // abilities (`FUN_00187868`, first empty slots).
                "party_vitals" => {
                    let Json::Obj(m) = v else { return Err("party_vitals: expected an object of member id: [hp, mp]".into()) };
                    for (k, b) in m {
                        let member = id_of(k, "party_vitals")?;
                        let Json::Arr(a) = b else { return Err("party_vitals: expected [hp, mp]".into()) };
                        let (Some(hp), Some(mp)) = (a.first(), a.get(1)) else { return Err("party_vitals: expected [hp, mp]".into()) };
                        let (hp, mp) = (int(hp, "party_vitals")?, int(mp, "party_vitals")?);
                        if !self.set_member_vitals(member, hp.clamp(0, 255) as i32, mp.clamp(0, 255) as i32) {
                            return Err(format!("party_vitals: no member record {member}"));
                        }
                    }
                }
                "party_abilities" => {
                    let Json::Obj(m) = v else { return Err("party_abilities: expected an object of member id: [ability ids]".into()) };
                    for (k, b) in m {
                        let member = id_of(k, "party_abilities")?;
                        let Some(r) = Self::member_off(member) else { return Err(format!("party_abilities: no member record {member}")) };
                        let Json::Arr(a) = b else { return Err("party_abilities: expected an array of ids".into()) };
                        for x in a {
                            let id = in_range(int(x, "party_abilities")?, 0x7fff, "party_abilities")?;
                            self.put_ability(r, id as u16, true);
                        }
                    }
                }
                // The record bits (save `+0x1550`): the words as integers.
                "record_bits" => {
                    let Json::Arr(a) = v else { return Err("record_bits: expected an array".into()) };
                    self.record_bits = a.iter().map(|x| in_range(int(x, "record_bits")?, u32::MAX.into(), "record_bits")).collect::<Result<Vec<_>, _>>()?.into_iter().map(|n| n as u32).collect();
                }
                // The story arrays of a slot file (`slot_json`): the flag words and the area program table as hex
                // and the exit locks as a flat list of (state, message) pairs. Replaces them; the effect lists stay.
                "story_raw" => {
                    let hex_of = |k: &str| match v.get(k) {
                        Some(Json::Str(h)) if h.len() % 2 == 0 && h.is_ascii() => {
                            (0..h.len() / 2).map(|i| u8::from_str_radix(&h[2 * i..2 * i + 2], 16).ok()).collect::<Option<Vec<u8>>>()
                        }
                        _ => None,
                    };
                    let (Some(words), Some(programs)) = (hex_of("words"), hex_of("programs")) else {
                        return Err("story_raw: expected hex strings words and programs".into());
                    };
                    // Rejected, not truncated: the words are whole u32s, the locks whole (state, message) pairs.
                    if words.len() % 4 != 0 {
                        return Err(format!("story_raw: words is {} bytes, not a multiple of 4", words.len()));
                    }
                    let locks = match v.get("locks") {
                        Some(Json::Arr(a)) => a.iter().map(|x| in_range(int(x, "story_raw.locks")?, u16::MAX.into(), "story_raw.locks").map(|n| n as u16)).collect::<Result<Vec<_>, _>>()?,
                        _ => Vec::new(),
                    };
                    if locks.len() % 2 != 0 {
                        return Err(format!("story_raw: locks has {} numbers, not pairs", locks.len()));
                    }
                    let mut st = self.story();
                    st.words = words.as_chunks::<4>().0.iter().map(|c| u32::from_le_bytes(*c)).collect();
                    st.programs = programs;
                    st.locks = locks.as_chunks::<2>().0.to_vec();
                }
                "comment" | "_comment" => {}
                // The save slot keys (WP-M8, `slot_json`): the whole party block, the app fields beside it and the
                // row data the Save menu shows (ignored here).
                "format" | "slot" => {}
                "block" => {
                    let Json::Str(h) = v else { return Err("block: expected a hex string".into()) };
                    self.block = unhex(h, off::LEN).ok_or_else(|| format!("block: expected {} hex bytes", off::LEN))?;
                    // A slot file is a whole state: what it does not carry (an older file) loads as the default.
                    self.moogle = Moogle::default();
                    self.shop_seen = [0; 4];
                    self.shop_visits = 0;
                    self.journal_seen.clear();
                    self.minigames.clear();
                    self.enemy_kills.clear();
                    self.reaction_uses.clear();
                    self.limit_best.clear();
                    self.report = [0; REPORT_WORDS];
                    self.record_bits.clear();
                }
                "new_seen" => {
                    let Json::Arr(a) = v else { return Err("new_seen: expected an array".into()) };
                    self.new_seen = a.iter().map(|x| in_range(int(x, "new_seen")?, u32::MAX.into(), "new_seen").map(|n| n as u32)).collect::<Result<_, _>>()?;
                }
                other => return Err(format!("unknown field '{other}'")),
            }
        }
        // Sora's ability list: replaced as a whole when either list is given.
        if j.get("abilities").is_some() || j.get("abilities_off").is_some() {
            let on = j.get("abilities").map(|v| list(v, "abilities")).transpose()?.unwrap_or_default();
            let off_list = j.get("abilities_off").map(|v| list(v, "abilities_off")).transpose()?.unwrap_or_default();
            let r = Self::member_off(SORA).unwrap();
            self.zero(r + rec::ABILITIES, 2 * rec::ABILITY_SLOTS);
            for id in on {
                self.put_ability(r, id, true);
            }
            for id in off_list {
                self.put_ability(r, id, false);
            }
        }
        Ok(())
    }

    /// Reads the tables from the disc and builds the start state (new game, then `KH2_STATE` or
    /// the app baseline).
    pub fn start(disc: &Disc) -> Option<Progress> {
        let (items, cmds, magc, plrp) = {
            let mut a = disc.0.lock().ok()?;
            (
                kh2_tables::load::<Item>(&mut a, &t::ITEM).ok()?.rows,
                kh2_tables::load::<Cmd>(&mut a, &t::CMD).ok()?.rows,
                kh2_tables::load::<Magc>(&mut a, &t::MAGC).ok()?.rows,
                kh2_tables::load::<Plrp>(&mut a, &t::PLRP).ok()?.rows,
            )
        };
        let index = Arc::new(ItemIndex::new(&items, &cmds, &magc));
        // `FUN_0017fd10`: the 19 type-2 world entries of `00progress.bin` (the flag side effects).
        let effects: Arc<Vec<Vec<u8>>> = Arc::new(
            disc.read("00progress.bin")
                .and_then(|f| Some(kh2_bar::parse(&f).ok()?.into_iter().filter(|e| e.kind == 2).take(story::WORLDS).map(|e| e.data.to_vec()).collect()))
                .unwrap_or_default(),
        );
        if effects.is_empty() {
            warn!("progress: 00progress.bin not readable; story flags set without their side effects");
        }
        let mut p = Progress::new_game(index.clone(), &plrp);
        let boot = Arc::new(BootWeapons::read(disc).unwrap_or_else(|| {
            warn!("progress: 00common.bdx event 7 not readable; the party starts without weapons");
            BootWeapons::default()
        }));
        p.boot_weapons = boot.clone();
        p.story().effects = effects.clone();
        match std::env::var("KH2_STATE") {
            Ok(path) if !path.is_empty() => match std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|s| p.apply_json(&s)) {
                Ok(()) => {
                    // A file without a `block` cannot set the members' weapon words (WP-PG1): the same app entry as the baseline.
                    if !std::fs::read_to_string(&path).is_ok_and(|s| s.contains("\"block\"")) {
                        p.baseline = p.apply_party_weapons();
                    }
                    p.source = Source::File(path);
                }
                Err(e) => {
                    // A file that failed half way is dropped as a whole.
                    warn!("progress: KH2_STATE={path}: {e}; using the new game and the app baseline");
                    p = Progress::new_game(index, &plrp);
                    p.boot_weapons = boot;
                    p.story().effects = effects;
                    p.apply_baseline(std::env::var("KH2_ABILITY_START").as_deref() != Ok("data"));
                }
            },
            _ => p.apply_baseline(std::env::var("KH2_ABILITY_START").as_deref() != Ok("data")),
        }
        Some(p)
    }

    /// The `KH2_PROGRESSLOG` line.
    pub fn report(&self) -> String {
        let from = match &self.source {
            Source::NewGame if self.baseline.is_empty() => "new game".to_string(),
            Source::NewGame => "new game + app baseline".to_string(),
            Source::File(p) => p.clone(),
        };
        let s = self.sora();
        let owned: Vec<u8> = (1..=5).filter(|&f| self.owns_form(f)).collect();
        let levels: Vec<u8> = (1..=5).map(|f| self.form_level(f)).collect();
        let fw: Vec<u16> = (1..=4).map(|f| self.form(f).weapon).collect();
        let abil: Vec<String> = s.abilities.iter().map(|&(id, on)| if on { id.to_string() } else { format!("{id} off") }).collect();
        let inv: Vec<String> = self.inventory().iter().map(|&(id, n)| format!("{id}x{n}")).collect();
        format!(
            "progress: state from {from}: experience {}; munny {}; spells {:?}; forms owned {owned:?} levels {levels:?} keyblades {fw:?}; \
             weapon {:?} armor {:?} accessories {:?}; abilities [{}]; HP {}/{} MP {}/{}; summon level {} exp {}; \
             outfit (world 2) {}; spell switch {:#x}; option word {:#x}; inventory [{}]",
            self.experience(),
            self.wallet(),
            self.spell_levels(),
            s.weapons,
            s.armor,
            s.accessories,
            abil.join(", "),
            s.hp,
            s.hp_max,
            s.mp,
            s.mp_max,
            self.summon_level(),
            self.summon_exp(),
            self.outfit(2),
            self.spell_switch_word(),
            self.option_word(),
            inv.join(" "),
        ) + &format!("; story flags {:x?}", self.story().set_flags())
    }
}

// --- story flags, area programs, exit locks (save block) ------------------------------------------

/// EE addresses of the story state (docs/events.md sections 0, 2 and 5).
pub mod story {
    /// The flag array: 19 pages of 0x20 bytes, one per world (`FUN_001802e0`, `FUN_00180298`,
    /// `FUN_0017ff60`). It ends at 0x33f910, where the world-point bits of `FUN_00194fd8` start.
    pub const FLAGS: u32 = 0x0033_f6b0;
    pub const PAGE: usize = 0x20;
    pub const WORLDS: usize = 19;
    /// The area program table: `(map, btl, evt)` at `0x33e86f + world * 0xc0 + area * 3`
    /// (`FUN_00104bd0` copies it into the area record, flag op 1 writes it through `FUN_00104b90`).
    pub const PROGRAMS: u32 = 0x0033_e86f;
    pub const PROGRAMS_PER_WORLD: usize = 0xc0;
    /// The exit-lock halfwords: entry `i` of the `dsa` table owns `0x33f918 + 4i` (state) and
    /// `+2` (the locked prompt's message); `FUN_00195260` .. `FUN_00195300`, read by `FUN_00195358`.
    pub const LOCKS: u32 = 0x0033_f918;
}

/// What the save block keeps of the story: the flag bits, the area program table and the exit-lock
/// halfwords, plus the side-effect lists of `00progress.bin` that setting a flag runs. The new game
/// has every byte 0 (`FUN_00105638` clears the save block at boot) [inferred: no later new-game writer
/// of these three regions was found; docs/events.md section 10].
#[derive(Clone, Debug, Default)]
pub struct Story {
    /// The flag words (`story::FLAGS`), `WORLDS * PAGE / 4` of them.
    words: Vec<u32>,
    /// `story::PROGRAMS`, `WORLDS * PROGRAMS_PER_WORLD` bytes.
    programs: Vec<u8>,
    /// `story::LOCKS`: (state, message) per `dsa` entry; entries past the end read 0.
    locks: Vec<[u16; 2]>,
    /// The 19 type-2 world entries of `00progress.bin` (`FUN_0017fd10` keeps them at `0x35ccc0`).
    /// Empty: flags are set without side effects.
    pub effects: Arc<Vec<Vec<u8>>>,
    /// Side-effect ops met that the app does not act on (6, 8, 9, 0xd and the world-point bits of
    /// ops 4 and 5), with the flag that ran them; for the logs.
    pub unported: Vec<(u16, u8)>,
    /// The world-point bits at `0x33f910` (`FUN_00194fa8` sets bit `i` for record `i` of
    /// `00worldpoint.bin`, at most 0x36 of them; `FUN_00194fd8` clears, not ported). Two words.
    world_bits: [u32; 2],
}

/// Records of `00worldpoint.bin` `FUN_00194f68` searches (it returns an index 0..=0x35, else -1).
const WORLD_POINT_RECORDS: usize = 0x36;

/// `Story` behind a lock, shared by `Progress` and the script hosts.
#[derive(Clone, Debug, Default)]
pub struct StoryHandle(Arc<std::sync::Mutex<Story>>);

impl StoryHandle {
    pub fn new(s: Story) -> StoryHandle {
        StoryHandle(Arc::new(std::sync::Mutex::new(s)))
    }

    pub fn lock(&self) -> std::sync::MutexGuard<'_, Story> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }
}

static SHARED_STORY: std::sync::Mutex<Option<StoryHandle>> = std::sync::Mutex::new(None);

/// Makes `h` the story the script hosts read and write (hosts 0.14, 0.15, 0.70). The game's flags
/// are one fixed array in the save block; the hosts' `Env` is built in files outside the story code,
/// so they reach it through this one process-wide handle. `ensure` installs the app's.
pub fn install_shared_story(h: StoryHandle) {
    *SHARED_STORY.lock().unwrap_or_else(|e| e.into_inner()) = Some(h);
}

/// The installed story (`None` in unit tests and tools: the hosts then answer as before, all clear).
pub fn shared_story() -> Option<StoryHandle> {
    SHARED_STORY.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

impl Story {
    /// A new-game story whose flags run the side effects of these world entries.
    pub fn with_effects(effects: Arc<Vec<Vec<u8>>>) -> Story {
        Story { effects, ..Story::default() }
    }

    /// The new-game story: every flag, program byte, exit lock and world point clear (`effects` stay: they are tables).
    fn reset_state(&mut self) {
        self.words.clear();
        self.programs.clear();
        self.locks.clear();
        self.world_bits = [0; 2];
    }

    fn ensure(&mut self) {
        if self.words.is_empty() {
            self.words = vec![0; story::WORLDS * story::PAGE / 4];
            self.programs = vec![0; story::WORLDS * story::PROGRAMS_PER_WORLD];
        }
    }

    /// The word index and bit of flag `id`: page `id >> 10` (0x20 bytes), word `(id & 0x3ff) >> 5`,
    /// bit `id & 0x1f` (`FUN_001802e0`). `None` past the array: a page beyond the 19 worlds, or a
    /// word past the end [inferred: the game would address memory after the array; no flag list
    /// names such an id, each world uses 256 flags].
    fn slot(id: u16) -> Option<(usize, u32)> {
        let w = (id as usize >> 10) * (story::PAGE / 4) + ((id as usize & 0x3ff) >> 5);
        (w < story::WORLDS * story::PAGE / 4).then_some((w, 1 << (id & 0x1f)))
    }

    /// `FUN_001802e0`: flag `id` is set (0xffff is always clear).
    pub fn test(&self, id: u16) -> bool {
        id != 0xffff && Self::slot(id).is_some_and(|(w, m)| self.words.get(w).is_some_and(|v| v & m != 0))
    }

    /// `FUN_00180298`: clears the bit (no side effects).
    pub fn clear(&mut self, id: u16) {
        self.ensure();
        if let Some((w, m)) = Self::slot(id) {
            self.words[w] &= !m;
        }
    }

    /// `FUN_0017ff60`: sets the bit, then runs the flag's side-effect list from the world entry
    /// `id >> 10` at the offset in halfword `id & 0x3ff` (`kh2_ard::progress::effects`):
    /// op 1 writes (map, btl, evt) of area `record[0]` of the flag's world (0xff keeps,
    /// `FUN_00104b90`); op 2 the lock entry's state and message (`FUN_00195260`); op 3 the state 0
    /// (`FUN_00195288`); op 4 the state 0xffff (`FUN_001952a0`); op 5 the state 0 (`FUN_00195300`);
    /// op 7 clears each flag (`FUN_00180298`); op 0xc sets each flag not yet set (recursive). The
    /// first halfword of ops 2..5 is the `dsa` index, each called once. Not acted on (recorded in
    /// `unported`): op 6 (`FUN_001a7730`, `0x33fe0c[world]`), 8 / 9 (the record bits 0x33fdb0,
    /// `FUN_001a52b0` / `FUN_001a52f0`), 0xd (`FUN_001b4e80`: `0x341858 + 4 * world` bytes 1, 2), the
    /// world-point bit ops 4 and 5 clear (`FUN_00194f68`, `FUN_00194fd8` on 0x33f910), and the calls
    /// after the list (`FUN_001803b0`, `FUN_00180450`, `FUN_001805d0`, `FUN_00180558`, `FUN_001d39d0`,
    /// `FUN_001d3238`, `FUN_001b6da0`, `FUN_001be9c8`, `FUN_001d5718`, `FUN_0019d890` for 0x8d0,
    /// `FUN_00195130`).
    pub fn set(&mut self, id: u16) {
        self.ensure();
        let Some((w, m)) = Self::slot(id) else { return };
        self.words[w] |= m;
        let world = id as usize >> 10;
        let effects = self.effects.clone();
        let Some(list) = effects.get(world).and_then(|e| kh2_ard::progress::effects(e, id & 0x3ff)) else { return };
        for e in list {
            let h = e.flags();
            let h0 = h.first().copied().unwrap_or(0) as usize;
            match e.op {
                1 => {
                    for [area, prog @ ..] in e.programs() {
                        let o = world * story::PROGRAMS_PER_WORLD + area as usize * 3;
                        for (k, b) in prog.iter().enumerate() {
                            if *b != 0xff
                                && let Some(slot) = self.programs.get_mut(o + k)
                            {
                                *slot = *b;
                            }
                        }
                    }
                }
                2 => {
                    let v = [h.get(1).copied().unwrap_or(0), h.get(2).copied().unwrap_or(0)];
                    self.lock_entry(h0)[..].copy_from_slice(&v);
                }
                3 => self.lock_entry(h0)[0] = 0,
                4 | 5 => {
                    self.lock_entry(h0)[0] = if e.op == 4 { 0xffff } else { 0 };
                    self.unported.push((id, e.op));
                }
                7 => h.iter().for_each(|&f| self.clear(f)),
                0xc => {
                    for f in h {
                        if !self.test(f) {
                            self.set(f);
                        }
                    }
                }
                op => self.unported.push((id, op)),
            }
        }
    }

    fn lock_entry(&mut self, i: usize) -> &mut [u16; 2] {
        if self.locks.len() <= i {
            self.locks.resize(i + 1, [0, 0]);
        }
        &mut self.locks[i]
    }

    /// The lock state halfword of `dsa` entry `i` (`0x33f918 + 4i`, read by `FUN_00195358`).
    pub fn lock_state(&self, i: usize) -> u16 {
        self.locks.get(i).map_or(0, |l| l[0])
    }

    /// The locked prompt's message halfword of entry `i` (`0x33f91a + 4i`, `FUN_001953e0`).
    pub fn lock_message(&self, i: usize) -> u16 {
        self.locks.get(i).map_or(0, |l| l[1])
    }

    /// The (map, btl, evt) bytes of the program table for (world, area) (`FUN_00104bd0` reads
    /// `0x33e86f`, `0x33e870`, `0x33e871` + `world * 0xc0 + area * 3`); 0 for each in a new game.
    pub fn programs(&self, world: u8, area: u8) -> [u8; 3] {
        let o = world as usize * story::PROGRAMS_PER_WORLD + area as usize * 3;
        let b = |k: usize| self.programs.get(o + k).copied().unwrap_or(0);
        [b(0), b(1), b(2)]
    }

    /// `FUN_00194fa8`: sets world-point bit `i`. The bit is the record index `FUN_00194f68` finds in
    /// `00worldpoint.bin`, which stops after 0x36 records (docs/events.md section 4), so an index from
    /// `WORLD_POINT_RECORDS` up is ignored (it cannot come from the lookup; the two words end at 64).
    pub fn set_world_point(&mut self, i: usize) {
        if i >= WORLD_POINT_RECORDS {
            return;
        }
        if let Some(w) = self.world_bits.get_mut(i >> 5) {
            *w |= 1 << (i & 31);
        }
    }

    /// Whether world-point bit `i` is set (the reader of the bits `FUN_00194fd8` tests).
    pub fn world_point(&self, i: usize) -> bool {
        self.world_bits.get(i >> 5).is_some_and(|w| w >> (i & 31) & 1 != 0)
    }

    /// Every set world-point bit, in order.
    pub fn world_points(&self) -> Vec<usize> {
        (0..self.world_bits.len() * 32).filter(|&i| self.world_point(i)).collect()
    }

    /// Every set flag id, in order.
    pub fn set_flags(&self) -> Vec<u16> {
        let mut out = Vec::new();
        for (w, &v) in self.words.iter().enumerate() {
            for b in 0..32 {
                if v >> b & 1 != 0 {
                    let page = w / (story::PAGE / 4);
                    out.push(((page << 10) | ((w % (story::PAGE / 4)) << 5) | b) as u16);
                }
            }
        }
        out
    }
}

// --- save slots (WP-M8, docs/menus.md 18) ------------------------------------------------------

/// What a slot's row on the Save menu shows besides the state itself: the leader's level, the munny, the play time and the
/// area the file was written in (the area's code, `tt03`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SlotInfo {
    pub level: u32,
    pub munny: u32,
    pub play_seconds: u64,
    pub area: String,
}

/// The save slots the Save menu lists (the footage shows four rows).
pub const SLOTS: usize = 4;

/// The folder the slots live in: `KH2_SAVE_DIR`, else `local/saves` below the working directory (git-ignored). The app never
/// reads or writes anywhere else (a PS2 memory card is not touched).
pub fn save_dir() -> std::path::PathBuf {
    std::env::var_os("KH2_SAVE_DIR").filter(|v| !v.is_empty()).map_or_else(|| std::path::PathBuf::from("local/saves"), std::path::PathBuf::from)
}

/// The file of slot `n` (0-based).
pub fn slot_path(dir: &std::path::Path, n: usize) -> std::path::PathBuf {
    dir.join(format!("slot{:02}.json", n + 1))
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn unhex(s: &str, len: usize) -> Option<Vec<u8>> {
    (s.len() == 2 * len && s.is_ascii()).then(|| (0..len).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).ok()).collect::<Option<Vec<u8>>>()).flatten()
}

impl Progress {
    /// The state as a slot file: a JSON object that `apply_json` (and so `KH2_STATE`) reads back. The party block goes as hex,
    /// the app fields kept beside it (munny, option word, game mode, Dodge Roll level, the NEW tags' seen words) as the keys
    /// of the state schema, and `slot` carries the row data. Rows are keys of the same schema, so a slot file is also a
    /// state file.
    pub fn slot_json(&self, info: &SlotInfo) -> String {
        self.slot_json_at(info, None)
    }

    /// `slot_json` with the entrance the area was entered by (read back by `slot_entrance`; a file without it keeps entrance 0).
    pub fn slot_json_at(&self, info: &SlotInfo, entrance: Option<u8>) -> String {
        Self::join_fields(&self.slot_fields(info, entrance))
    }

    /// The keys of a slot file as (name, JSON value text) in file order: `block` comes first among the state keys so that
    /// `apply_json` sees it before the keys it resets.
    fn slot_fields(&self, info: &SlotInfo, entrance: Option<u8>) -> Vec<(&'static str, String)> {
        let area: String = info.area.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
        let seen: Vec<String> = self.new_seen.iter().map(u32::to_string).collect();
        let mut f = vec![
            ("format", "1".to_string()),
            ("slot", format!("{{\"level\": {}, \"munny\": {}, \"play_seconds\": {}, \"area\": \"{}\"{}}}", info.level, info.munny, info.play_seconds, area, entrance.map_or(String::new(), |e| format!(", \"entrance\": {e}")))),
            ("block", format!("\"{}\"", hex(&self.block))),
            ("munny", self.wallet.to_string()),
            ("option_word", self.config[0].to_string()),
            ("game_mode", self.config[1].to_string()),
            ("dodge_roll", self.dodge_roll.to_string()),
            ("new_seen", format!("[{}]", seen.join(", "))),
        ];
        f.extend(self.persisted_fields());
        f
    }

    /// One key per line, in the order given.
    fn join_fields(fields: &[(&str, String)]) -> String {
        let lines: Vec<String> = fields.iter().map(|(k, v)| format!("  \"{k}\": {v}")).collect();
        format!("{{
{}
}}
", lines.join(",
"))
    }

    /// The slot keys for the state kept outside the party block (the moogle's record, the shops' bits, the record bits,
    /// the story arrays and the world points). Bit sets are lists of the set bits.
    fn persisted_fields(&self) -> Vec<(&'static str, String)> {
        fn bits(set: u64) -> String {
            let v: Vec<String> = (0..64).filter(|&n| Moogle::bit(set, n)).map(|n| n.to_string()).collect();
            format!("[{}]", v.join(", "))
        }
        fn nums<T: ToString>(v: impl IntoIterator<Item = T>) -> String {
            format!("[{}]", v.into_iter().map(|x| x.to_string()).collect::<Vec<_>>().join(", "))
        }
        let m = &self.moogle;
        let shop_seen: Vec<usize> = (0..128).filter(|&n| self.shop_seen(n)).collect();
        let shop_visits: Vec<u8> = (0..8).filter(|&n| self.shop_visit(n)).collect();
        let st = self.story();
        let words: Vec<u8> = st.words.iter().flat_map(|w| w.to_le_bytes()).collect();
        let locks: Vec<u16> = st.locks.iter().flat_map(|l| [l[0], l[1]]).collect();
        let games = format!("[{}]", self.minigames.iter().map(|(i, (s, v))| format!("[{i}, {s}, {v}]")).collect::<Vec<_>>().join(", "));
        let kills = format!("[{}]", self.enemy_kills.iter().map(|(i, c)| format!("[{i}, {c}]")).collect::<Vec<_>>().join(", "));
        let react = format!("[{}]", self.reaction_uses.iter().map(|(i, c)| format!("[{i}, {c}]")).collect::<Vec<_>>().join(", "));
        let limits = format!("[{}]", self.limit_best.iter().map(|(i, c)| format!("[{i}, {c}]")).collect::<Vec<_>>().join(", "));
        let seen_pairs = format!("[{}]", self.journal_seen.iter().map(|(c, e)| format!("[{c}, {e}]")).collect::<Vec<_>>().join(", "));
        vec![
            (
                "moogle",
                format!(
                    "{{\"known\": {}, \"learned\": {}, \"made\": {}, \"seen\": {}, \"unlocks\": {}, \"rewards\": {}, \"points\": {}, \"level\": {}, \"stock\": {}, \"totals\": {}}}",
                    bits(m.known),
                    bits(m.learned),
                    bits(m.made),
                    bits(m.seen),
                    bits(m.unlocks),
                    bits(m.rewards),
                    m.points,
                    m.level,
                    nums((0..MOOGLE_MATERIALS).map(|i| m.stock_at(i))),
                    nums((0..MOOGLE_MATERIALS).map(|i| m.total_at(i)))
                ),
            ),
            ("shop_seen", nums(shop_seen)),
            ("shop_visits", nums(shop_visits)),
            ("record_bits", nums(self.record_bits.iter().copied())),
            ("journal_seen", seen_pairs),
            ("minigames", games),
            ("enemy_kills", kills),
            ("reaction_uses", react),
            ("limit_best", limits),
            ("report_counters", nums(self.report)),
            ("story_raw", format!("{{\"words\": \"{}\", \"programs\": \"{}\", \"locks\": {}}}", hex(&words), hex(&st.programs), nums(locks))),
            ("world_points", nums(st.world_points())),
        ]
    }

    /// The entrance a slot file records (`None` for a file written without it).
    pub fn slot_entrance(text: &str) -> Option<u8> {
        let j = Json::parse(text).ok()?;
        j.get("slot")?.get("entrance")?.int().and_then(|v| u8::try_from(v).ok())
    }

    /// The row data of a slot file (`None` for text that is not a slot file).
    pub fn slot_info(text: &str) -> Option<SlotInfo> {
        let j = Json::parse(text).ok()?;
        let s = j.get("slot")?;
        let n = |k: &str| s.get(k).and_then(Json::int).map(|v| v.max(0));
        Some(SlotInfo {
            level: n("level")? as u32,
            munny: n("munny")? as u32,
            play_seconds: n("play_seconds")? as u64,
            area: match s.get("area") {
                Some(Json::Str(a)) => a.clone(),
                _ => return None,
            },
        })
    }

    /// The NEW tags' seen words (save `+0x31b4`) a loaded slot carried.
    pub fn new_seen(&self) -> &[u32] {
        &self.new_seen
    }

    pub fn set_new_seen(&mut self, words: &[u32]) {
        self.new_seen = words.to_vec();
    }
}

fn log_on() -> bool {
    std::env::var("KH2_PROGRESSLOG").is_ok_and(|v| v != "0")
}

/// Inserts `Progress` if it is not there yet (needs `Disc`). The plugins that read it while they
/// build (abilities, drive) call this, so the order of the plugin tuples does not matter.
pub fn ensure(app: &mut App) -> bool {
    if app.world().contains_resource::<Progress>() {
        return true;
    }
    let Some(p) = app.world().get_resource::<Disc>().and_then(Progress::start) else {
        warn!("progress: tables not readable; the systems use their own defaults");
        return false;
    };
    if log_on() {
        info!("{}", p.report());
        if !p.baseline.is_empty() {
            info!("progress: app baseline (not the original's new game, docs/progress.md): {}", p.baseline.join("; "));
        }
    }
    {
        let s = p.story();
        if !s.set_flags().is_empty() {
            let tt: Vec<String> = (0..64u8).filter(|&a| s.programs(2, a) != [0; 3]).map(|a| format!("tt{a:02} {:x?}", s.programs(2, a))).collect();
            let locks: Vec<String> = (0..64).filter(|&i| s.lock_state(i) != 0).map(|i| format!("{i}: {:#x}", s.lock_state(i))).collect();
            info!("progress: story flags {:x?}; Twilight Town program table (map, btl, evt) [{}]; exit locks [{}]; side effects not ported {:x?}", s.set_flags(), tt.join(", "), locks.join(", "), s.unported);
        }
    }
    install_shared_story(p.story_handle());
    app.insert_resource(p);
    true
}

pub struct ProgressPlugin;

impl Plugin for ProgressPlugin {
    fn build(&self, app: &mut App) {
        ensure(app);
        app.add_message::<EnemyDefeated>()
            .add_message::<ExperienceGained>()
            .add_systems(FixedUpdate, pay_defeats.in_set(crate::schedule::Sim::Effects));
        let tables = app.world().get_resource::<Disc>().and_then(|disc| {
            let mut a = disc.0.lock().ok()?;
            let lvup = kh2_tables::subfile(&mut a, &t::LVUP).ok().and_then(|b| kh2_tables::rt::decode_lvup::<t::Lvup>(&b))?;
            let sstm = kh2_tables::player::load_sstm(&mut a).ok()?;
            Some(ExperienceTables { lvup: Arc::new(lvup), low_hp_fraction: sstm.f32(0x144) })
        });
        match tables {
            Some(t) => {
                app.insert_resource(t);
            }
            None => warn!("progress: lvup or sstm not readable; enemy defeats pay no experience"),
        }
    }
}

// --- a small JSON reader (the state file) ----------------------------------------------------

/// A JSON value. Objects keep their order.
#[derive(Clone, Debug, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

impl Json {
    pub fn parse(s: &str) -> Result<Json, String> {
        let mut p = JsonParser { b: s.as_bytes(), i: 0 };
        let v = p.value()?;
        p.ws();
        if p.i != p.b.len() {
            return p.err("trailing characters");
        }
        Ok(v)
    }

    pub fn get(&self, k: &str) -> Option<&Json> {
        match self {
            Json::Obj(m) => m.iter().find(|(n, _)| n == k).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn int(&self) -> Option<i64> {
        match self {
            Json::Num(n) if n.fract() == 0.0 => Some(*n as i64),
            _ => None,
        }
    }
}

struct JsonParser<'a> {
    b: &'a [u8],
    i: usize,
}

impl JsonParser<'_> {
    fn err<T>(&self, what: &str) -> Result<T, String> {
        Err(format!("JSON: {what} at byte {}", self.i))
    }

    fn ws(&mut self) {
        while self.b.get(self.i).is_some_and(|c| c.is_ascii_whitespace()) {
            self.i += 1;
        }
    }

    fn eat(&mut self, c: u8) -> bool {
        self.ws();
        let hit = self.b.get(self.i) == Some(&c);
        if hit {
            self.i += 1;
        }
        hit
    }

    fn value(&mut self) -> Result<Json, String> {
        self.ws();
        match self.b.get(self.i) {
            None => self.err("unexpected end"),
            Some(b'{') => {
                self.i += 1;
                let mut m = Vec::new();
                if self.eat(b'}') {
                    return Ok(Json::Obj(m));
                }
                loop {
                    self.ws();
                    let k = self.string()?;
                    if !self.eat(b':') {
                        return self.err("expected ':'");
                    }
                    let v = self.value()?;
                    m.push((k, v));
                    if self.eat(b',') {
                        continue;
                    }
                    if self.eat(b'}') {
                        return Ok(Json::Obj(m));
                    }
                    return self.err("expected ',' or '}'");
                }
            }
            Some(b'[') => {
                self.i += 1;
                let mut a = Vec::new();
                if self.eat(b']') {
                    return Ok(Json::Arr(a));
                }
                loop {
                    a.push(self.value()?);
                    if self.eat(b',') {
                        continue;
                    }
                    if self.eat(b']') {
                        return Ok(Json::Arr(a));
                    }
                    return self.err("expected ',' or ']'");
                }
            }
            Some(b'"') => Ok(Json::Str(self.string()?)),
            Some(b't') => self.word("true", Json::Bool(true)),
            Some(b'f') => self.word("false", Json::Bool(false)),
            Some(b'n') => self.word("null", Json::Null),
            Some(_) => self.number(),
        }
    }

    fn word(&mut self, w: &str, v: Json) -> Result<Json, String> {
        if self.b[self.i..].starts_with(w.as_bytes()) {
            self.i += w.len();
            Ok(v)
        } else {
            self.err("unknown word")
        }
    }

    fn number(&mut self) -> Result<Json, String> {
        let start = self.i;
        while self.b.get(self.i).is_some_and(|c| c.is_ascii_digit() || matches!(c, b'-' | b'+' | b'.' | b'e' | b'E')) {
            self.i += 1;
        }
        std::str::from_utf8(&self.b[start..self.i]).ok().and_then(|s| s.parse::<f64>().ok()).map(Json::Num).map_or_else(|| self.err("bad number"), Ok)
    }

    fn string(&mut self) -> Result<String, String> {
        if self.b.get(self.i) != Some(&b'"') {
            return self.err("expected a string");
        }
        self.i += 1;
        let mut out: Vec<u8> = Vec::new();
        loop {
            let Some(&c) = self.b.get(self.i) else { return self.err("unterminated string") };
            self.i += 1;
            match c {
                b'"' => return Ok(String::from_utf8_lossy(&out).into_owned()),
                b'\\' => {
                    let Some(&e) = self.b.get(self.i) else { return self.err("unterminated escape") };
                    self.i += 1;
                    let ch = match e {
                        b'"' => '"',
                        b'\\' => '\\',
                        b'/' => '/',
                        b'n' => '\n',
                        b't' => '\t',
                        b'r' => '\r',
                        b'b' => '\u{8}',
                        b'f' => '\u{c}',
                        b'u' => {
                            let hex = self.b.get(self.i..self.i + 4).and_then(|h| std::str::from_utf8(h).ok()).and_then(|h| u32::from_str_radix(h, 16).ok());
                            let Some(h) = hex else { return self.err("bad \\u escape") };
                            self.i += 4;
                            char::from_u32(h).unwrap_or('\u{fffd}')
                        }
                        _ => return self.err("bad escape"),
                    };
                    let mut buf = [0u8; 4];
                    out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                }
                _ => out.push(c),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `FUN_0027d8b8`: the toggle writes bit 15 of exactly the named slot, keeps the id bits, and
    /// leaves the other slots (and the empty slots between them) alone.
    #[test]
    fn ability_toggle_sets_and_clears_bit_15_of_the_right_slot() {
        let mut p = Progress::from_block(&[], Arc::new(ItemIndex::default()));
        let r = Progress::member_off(SORA).unwrap();
        p.put_ability(r, 82, true);
        p.put_ability(r, 162, false);
        p.put_ability(r, 394, false);
        // A gap: slot 1 emptied, so the next row's slot index differs from its position in the list.
        p.set_u16(r + rec::ABILITIES + 2, 0);
        assert_eq!(p.ability_slots(SORA), vec![(0, 82, true), (2, 394, false)]);
        assert!(p.set_ability_equipped(SORA, 2, true));
        assert_eq!(p.ability_slots(SORA), vec![(0, 82, true), (2, 394, true)]);
        assert_eq!(p.u16_at(r + rec::ABILITIES + 4), 394 | 0x8000);
        assert!(p.set_ability_equipped(SORA, 0, false));
        assert_eq!(p.ability_slots(SORA), vec![(0, 82, false), (2, 394, true)]);
        assert_eq!(p.u16_at(r + rec::ABILITIES), 82, "the id bits are kept");
        assert_eq!(p.sora().abilities, vec![(82, false), (394, true)]);
        assert!(!p.set_ability_equipped(SORA, 64, true), "slot 64 does not exist");
        assert!(!p.set_ability_equipped(99, 0, true), "no such member");
    }

    /// `FUN_001d3f18` on Sora's list: the first slot holding the level below takes the new level and keeps
    /// its bit 15; the first growth level fills the first empty slot unequipped; no match changes nothing.
    /// The same as `prize::growth_upgrade` (the array form the form level-up log uses).
    #[test]
    fn growth_upgrade_keeps_the_equipped_bit_and_matches_the_array_form() {
        let mut p = Progress::from_block(&[], Arc::new(ItemIndex::default()));
        let r = Progress::member_off(SORA).unwrap();
        p.put_ability(r, 82, true);
        let chains = crate::prize::GrowthChains([[0; 5], [0, 94, 95, 96, 97], [0, 98, 99, 100, 101], [0; 5], [0; 5]]);
        let mut arr = [0u16; 64];
        arr[0] = 82 | 0x8000;
        let step = |p: &mut Progress, arr: &mut [u16; 64], form: u8, level: u8| {
            let old = if level == 0 { chains.at(form - 1, 4) } else { chains.at(form, level - 1) };
            let got = p.upgrade_ability(SORA, old, chains.at(form, level));
            crate::prize::growth_upgrade(arr, &chains, form, level);
            let words: Vec<u16> = (0..64).map(|i| p.u16_at(r + rec::ABILITIES + 2 * i)).collect();
            assert_eq!(words, arr.to_vec(), "form {form} level {level}");
            got
        };
        assert_eq!(step(&mut p, &mut arr, 1, 1), Some(1), "High Jump LV1 into the first empty slot");
        assert_eq!(p.ability_slots(SORA), vec![(0, 82, true), (1, 94, false)], "it arrives unequipped");
        p.set_ability_equipped(SORA, 1, true);
        arr[1] |= 0x8000;
        assert_eq!(step(&mut p, &mut arr, 1, 2), Some(1));
        assert_eq!(p.ability_slots(SORA), vec![(0, 82, true), (1, 95, true)], "LV2 keeps LV1's equipped bit");
        assert_eq!(step(&mut p, &mut arr, 2, 2), None, "no Quick Run LV1 to upgrade: nothing changes");
        assert_eq!(step(&mut p, &mut arr, 2, 1), Some(2));
        assert_eq!(step(&mut p, &mut arr, 2, 2), Some(2));
        assert_eq!(p.ability_slots(SORA), vec![(0, 82, true), (1, 95, true), (2, 99, false)]);
        assert_eq!(p.upgrade_ability(99, 0, 94), None, "no such member");
    }

    /// The oracle for `upgrade_ability`: the game's `FUN_001d3f18(list, 0x40, form, level)` (a leaf) runs in
    /// kh2-ee on the user's executable over a 64-slot list, with the chain table at 0x37c556 as the
    /// executable holds it; the port gets the chain's ids at `level - 1` (the routine reads the entry just before the
    /// chain's `level` entry, which for level 0 is the previous form's entry 4) and `level`. Every form 0..=4 and level 0..=4, twice
    /// round, with one slot taken off between the rounds; the 64 words must stay equal.
    #[test]
    fn growth_upgrade_matches_the_game() {
        use kh2_ee::Ee;
        const STACK: u32 = 0x01ff_0000;
        const LIST: u32 = 0x0180_0000;
        const CHAINS: u32 = crate::prize::GROWTH_CHAINS;
        let Some(mut tb) = tables() else { return };
        let elf = ["SLUS_210.05", "SLUS_210.05;1"].iter().find_map(|p| tb.archive.iso().read(p).ok()).expect("executable");
        let mut ee = Ee::new();
        ee.load_elf(&elf).expect("elf");
        let mut p = Progress::from_block(&[], Arc::new(ItemIndex::default()));
        let r = Progress::member_off(SORA).unwrap();
        p.put_ability(r, 82, true);
        p.put_ability(r, 162, false);
        let words = |p: &Progress| (0..64).map(|i| p.u16_at(r + rec::ABILITIES + 2 * i)).collect::<Vec<u16>>();
        let bytes: Vec<u8> = words(&p).iter().flat_map(|w| w.to_le_bytes()).collect();
        ee.write(LIST, &bytes);
        let (mut changed, mut cases) = (0, 0);
        for round in 0..2 {
            for form in 0..=4u32 {
                for level in 0..=4u32 {
                    let at = CHAINS + 10 * form + 2 * level;
                    let (old, new) = (ee.r16(at - 2), ee.r16(at));
                    ee.call(0x001d_3f18, &[LIST as u64, 0x40, form as u64, level as u64], &[], STACK).expect("FUN_001d3f18");
                    changed += p.upgrade_ability(SORA, old, new).is_some() as u32;
                    cases += 1;
                    let game: Vec<u16> = (0..64).map(|i| ee.r16(LIST + 2 * i)).collect();
                    assert_eq!(game, words(&p), "round {round} form {form} level {level} (old {old}, new {new})");
                }
            }
            // Take the first growth slot off (FUN_0027d8b8) in both, then go round again.
            if let Some(&(slot, _, _)) = p.ability_slots(SORA).iter().find(|s| s.1 != 82 && s.1 != 162) {
                p.set_ability_equipped(SORA, slot, false);
                let v = ee.r16(LIST + 2 * slot as u32);
                ee.write(LIST + 2 * slot as u32, &(v & 0x7fff).to_le_bytes());
            }
        }
        eprintln!("oracle: FUN_001d3f18 equal for {cases} calls ({changed} slots written); Sora's list {:?}", p.ability_slots(SORA));
        assert!(changed > 0);
    }

    /// The Customize writers: the shortcut words and the spell switch bits, and the stat boost bytes.
    #[test]
    fn shortcuts_spell_switches_and_boosts_write_their_words() {
        let mut p = Progress::from_block(&[], Arc::new(ItemIndex::default()));
        assert_eq!(p.shortcuts(), [0; 4]);
        assert!(p.set_shortcut(2, 0x1234));
        assert_eq!((p.shortcut(2), p.shortcut(0), p.shortcut(4)), (0x1234, 0, 0));
        assert_eq!(p.u16_at(off::SHORTCUTS + 4), 0x1234, "slot 2 is the u16 at party block +0xfa0 + 4 (save +0x2604)");
        assert!(!p.set_shortcut(4, 1), "only four slots");
        // The switch: a set bit is "Not Used"; the toggle answers the state it left (true = it was set).
        assert!(!p.toggle_spell_switch(3), "was clear");
        assert!(p.spell_switch(3) && p.spell_switch_word() == 8);
        assert!(p.toggle_spell_switch(3), "was set");
        assert_eq!(p.spell_switch_word(), 0);
        p.set_spell_switch(5, true);
        p.set_spell_switch(0, true);
        p.set_spell_switch(5, false);
        assert_eq!(p.spell_switch_word(), 1);
        // The boosts add to the record byte and stop at 100.
        let r = Progress::member_off(SORA).unwrap();
        p.set_u8(r + rec::STRENGTH, 98);
        assert!(!p.boost_maxed(SORA, 0x114));
        assert_eq!(p.apply_boost(SORA, 0x114, 1), 1);
        assert_eq!(p.apply_boost(SORA, 0x114, 5), 1, "only the room up to 100 is added");
        assert_eq!(p.u8_at(r + rec::STRENGTH), 100);
        assert!(p.boost_maxed(SORA, 0x114));
        assert_eq!(p.apply_boost(SORA, 0x114, 1), 0);
        assert_eq!(p.apply_boost(SORA, 0x117, 2), 2);
        assert_eq!(p.u8_at(r + rec::AP), 2);
        assert_eq!(p.apply_boost(SORA, 0x118, 1), 0, "not a boost");
    }

    /// `FUN_00274f70`'s slot loop and `FUN_00275288`'s question test.
    #[test]
    fn auto_reload_reserves_and_releases_every_slot_of_the_item() {
        let mut p = Progress::from_block(&[], Arc::new(ItemIndex::default()));
        let r = Progress::member_off(SORA).unwrap();
        p.set_item_slots(r, 4);
        // Slot 0 carries item 1, slot 1 reserves 1 and is empty, slot 2 carries item 3, slot 3 is free.
        p.set_u16(r + rec::ITEMS, 1);
        p.set_u16(r + rec::RESERVED + 2, 1);
        p.set_u16(r + rec::ITEMS + 4, 3);
        assert_eq!(p.battle_items(SORA), vec![(1, 0), (0, 1), (3, 0), (0, 0)]);
        // Item 1 is on in slot 1 and not carried there: turning it off asks first (the empty slot waits for a refill).
        assert!(p.reload_off_needs_question(SORA, 1));
        assert!(!p.reload_off_needs_question(SORA, 3));
        p.set_auto_reload(SORA, 1, false);
        assert_eq!(p.battle_items(SORA), vec![(1, 0), (0, 0), (3, 0), (0, 0)], "the carried item stays, the reservation goes");
        assert!(!p.reload_off_needs_question(SORA, 1));
        // On: every slot that carries or reserves it reserves it; other items are untouched.
        p.set_auto_reload(SORA, 3, true);
        p.set_auto_reload(SORA, 1, true);
        assert_eq!(p.battle_items(SORA), vec![(1, 1), (0, 0), (3, 3), (0, 0)]);
        assert!(!p.reload_off_needs_question(SORA, 1), "reserved and carried: no question");
    }

    /// The shops' seen bits (save `+0x3714`), first-visit bits (`+0x3724`) and the moogle's reward bits (record `+8`):
    /// set by the accessors and by the `KH2_STATE` keys `shop_seen`, `shop_visits` and `moogle.rewards`.
    #[test]
    fn the_shop_seen_bits_the_first_visit_bits_and_the_reward_bits_are_kept() {
        let mut p = Progress::from_block(&[], Arc::new(ItemIndex::default()));
        assert!(!p.shop_seen(0) && !p.shop_seen(127) && !p.shop_visit(0) && !p.shop_visit(1));
        p.set_shop_seen(5);
        p.set_shop_seen(127);
        p.set_shop_seen(128);
        assert!(p.shop_seen(5) && p.shop_seen(127) && !p.shop_seen(6) && !p.shop_seen(128), "128 places, the 129th is dropped");
        p.set_shop_visit(1);
        assert!(p.shop_visit(1) && !p.shop_visit(0));
        p.apply_json(r#"{"shop_seen": [0, 33], "shop_visits": [0], "moogle": {"rewards": [0, 49]}}"#).unwrap();
        assert!(p.shop_seen(0) && p.shop_seen(33) && p.shop_visit(0));
        assert_eq!(p.moogle.rewards, 1 | 1 << 49);
        assert!(p.apply_json(r#"{"shop_seen": 3}"#).is_err());
    }

    /// The Save menu's slot file (docs/menus.md 18): written from one state, read back through `apply_json` (the path
    /// `KH2_STATE` takes) into another, it reproduces the party block byte for byte and the app fields beside it.
    #[test]
    fn a_slot_file_restores_the_block_byte_for_byte() {
        let bytes: Vec<u8> = (0..off::LEN).map(|i| (i * 7 + 3) as u8).collect();
        let mut p = Progress::from_block(&bytes, Arc::new(ItemIndex::default()));
        p.wallet = 123_456;
        p.config = [0x41b, 2];
        p.dodge_roll = 3;
        p.set_new_seen(&[1, 0xffff_ffff, 7]);
        let info = SlotInfo { level: 7, munny: 123_456, play_seconds: 3725, area: "tt03".into() };
        let text = p.slot_json(&info);
        assert_eq!(Progress::slot_info(&text), Some(info));
        let mut q = Progress::from_block(&[], Arc::new(ItemIndex::default()));
        assert_ne!(q.block(), p.block());
        q.apply_json(&text).expect("a slot file is a state file");
        assert_eq!(q.block(), p.block(), "the party block, byte for byte");
        assert_eq!((q.wallet(), q.option_word(), q.game_mode(), q.dodge_roll), (123_456, 0x41b, 2, 3));
        assert_eq!(q.new_seen(), [1, 0xffff_ffff, 7]);
        // A block of the wrong length is refused, and the slot's row data is not a state key of its own.
        let short = text.replacen("\"block\": \"", "\"block\": \"00", 1);
        assert!(q.apply_json(&short).is_err());
        assert_eq!(Progress::slot_info("{\"munny\": 1}"), None);
    }

    /// WP-MN13: the opened chests and the journal's seen set go through a slot file and the `chests` / `journal_seen` keys;
    /// the chest bits are the record array from bit 224 on (`FUN_001a6a90` reads `0x33fdcc`, `FUN_001a5328` `0x33fdb0`).
    #[test]
    fn opened_chests_and_the_journal_seen_set_are_saved() {
        let mut p = Progress::from_block(&[], Arc::new(ItemIndex::default()));
        assert!(!p.chest_opened(7) && !p.chest_opened(0));
        p.set_chest_opened(7);
        p.set_chest_opened(0);
        p.set_journal_seen(3, 9);
        p.set_journal_seen(0, 1);
        assert!(p.chest_opened(7) && !p.chest_opened(8) && !p.chest_opened(0), "chest 0 is no chest");
        assert!(p.record_bit(224 + 7) && !p.record_bit(7), "the chest bits sit behind the 224 record bits");
        let text = p.slot_json(&SlotInfo { level: 1, munny: 0, play_seconds: 0, area: "tt04".into() });
        let mut q = Progress::from_block(&[], Arc::new(ItemIndex::default()));
        q.set_journal_seen(5, 5);
        q.apply_json(&text).expect("loads");
        assert!(q.chest_opened(7) && !q.chest_opened(6));
        assert!(q.journal_seen(3, 9) && q.journal_seen(0, 1) && !q.journal_seen(5, 5), "the block key reset the old pair");
        assert_eq!(q.journal_seen_all().collect::<Vec<_>>(), [(0, 1), (3, 9)]);
        let mut r = Progress::from_block(&[], Arc::new(ItemIndex::default()));
        r.apply_json("{\"chests\": [12, 300], \"journal_seen\": [[4, 2]]}").expect("the keys load");
        assert!(r.chest_opened(12) && r.chest_opened(300) && r.journal_seen(4, 2));
        assert!(r.apply_json("{\"journal_seen\": [3]}").is_err());
    }

    /// WP-MN14: the minigame records go through a slot file and the `minigames` key; a record nothing set reads as (0, 0).
    #[test]
    fn minigame_records_are_saved() {
        let mut p = Progress::from_block(&[], Arc::new(ItemIndex::default()));
        assert_eq!(p.minigame(3), (0, 0));
        p.set_minigame(3, 2, 1500);
        p.set_minigame(9, 4, 7200);
        p.set_minigame(9, 0, 0);
        assert_eq!((p.minigame(3), p.minigame(9)), ((2, 1500), (0, 0)));
        let text = p.slot_json(&SlotInfo { level: 1, munny: 0, play_seconds: 0, area: "tt04".into() });
        let mut q = Progress::from_block(&[], Arc::new(ItemIndex::default()));
        q.set_minigame(5, 1, 1);
        q.apply_json(&text).expect("loads");
        assert_eq!((q.minigame(3), q.minigame(5)), ((2, 1500), (0, 0)), "the block key reset the old record");
        let mut r = Progress::from_block(&[], Arc::new(ItemIndex::default()));
        r.apply_json("{\"minigames\": [[7, 4, 3661], [8, 2]]}").expect("the key loads");
        assert_eq!((r.minigame(7), r.minigame(8)), ((4, 3661), (2, 0)));
        assert!(r.apply_json("{\"minigames\": [3]}").is_err());
    }

    /// WP-MN15: the defeated-enemy counters count only with story flag 0x8d0 and only for ids with a slot, stop at 9999, and go
    /// through a slot file and the `enemy_kills` key; an older file loads them empty.
    #[test]
    fn enemy_defeat_counters_count_and_are_saved() {
        let mut p = Progress::from_block(&[], Arc::new(ItemIndex::default()));
        assert!(!p.count_defeat(1000), "story flag 0x8d0 is clear");
        p.story().set(0x8d0);
        assert!(p.count_defeat(1000) && p.count_defeat(1000) && p.count_defeat(3011));
        assert!(!p.count_defeat(1047) && !p.count_defeat(999) && !p.count_defeat(3012), "no slot");
        assert_eq!((p.enemy_defeats(1000), p.enemy_defeats(3011), p.enemy_defeats(1001)), (2, 1, 0));
        p.set_enemy_defeats(1046, 9998);
        assert!(p.count_defeat(1046) && !p.count_defeat(1046), "the count stops at 9999");
        assert_eq!(p.enemy_defeats(1046), 9999);
        let text = p.slot_json(&SlotInfo { level: 1, munny: 0, play_seconds: 0, area: "tt04".into() });
        let mut q = Progress::from_block(&[], Arc::new(ItemIndex::default()));
        q.set_enemy_defeats(1010, 5);
        q.apply_json(&text).expect("loads");
        assert_eq!((q.enemy_defeats(1000), q.enemy_defeats(1046), q.enemy_defeats(1010)), (2, 9999, 0), "the block key reset the old counter");
        let mut r = Progress::from_block(&[], Arc::new(ItemIndex::default()));
        r.apply_json("{\"enemy_kills\": [[1002, 7], [5, 3]]}").expect("the key loads");
        assert_eq!((r.enemy_defeats(1002), r.enemy_defeats(5)), (7, 0));
        assert!(r.apply_json("{\"enemy_kills\": [3]}").is_err());
    }

    /// WP-MN15: the reaction commands' use counts go through a slot file and the `reaction_uses` key, capped at 999.
    #[test]
    fn reaction_use_counts_are_saved() {
        let mut p = Progress::from_block(&[], Arc::new(ItemIndex::default()));
        assert_eq!(p.reaction_use(41), 0);
        p.set_reaction_use(41, 5000);
        p.set_reaction_use(2, 7);
        p.set_reaction_use(3, 9);
        p.set_reaction_use(3, 0);
        assert_eq!((p.reaction_use(41), p.reaction_use(2), p.reaction_use(3)), (999, 7, 0));
        let text = p.slot_json(&SlotInfo { level: 1, munny: 0, play_seconds: 0, area: "tt04".into() });
        let mut q = Progress::from_block(&[], Arc::new(ItemIndex::default()));
        q.set_reaction_use(9, 1);
        q.apply_json(&text).expect("loads");
        assert_eq!((q.reaction_use(41), q.reaction_use(2), q.reaction_use(9)), (999, 7, 0), "the block key reset the old count");
        assert!(q.apply_json("{\"reaction_uses\": [1]}").is_err());
    }

    /// WP-CT1: the BATTLE REPORT counters count like `FUN_001d76a0` / `FUN_001d76c8` / `FUN_001d76f8` (stop at 999, the form index 5 and the
    /// summon index 0 are one word) and go through a slot file.
    #[test]
    fn report_counters_count_stop_at_999_and_are_saved() {
        let mut p = Progress::from_block(&[], std::sync::Arc::new(ItemIndex::default()));
        assert_eq!((p.healed_count(), p.mickey_save_count(), p.form_uses(), p.antiform_uses(), p.summon_uses()), (0, 0, [0; 5], 0, [0; 4]));
        for _ in 0..1000 {
            p.count_healed();
        }
        p.count_mickey_save();
        p.count_form_use(1);
        p.count_form_use(1);
        p.count_form_use(4);
        p.count_form_use(5);
        p.count_summon_use(0);
        p.count_summon_use(1);
        p.count_summon_use(4);
        p.count_form_use(10);
        p.count_summon_use(5);
        assert_eq!((p.healed_count(), p.mickey_save_count()), (999, 1));
        assert_eq!(p.form_uses(), [2, 0, 0, 1, 2], "form index 5 is the word the Antiform row reads");
        assert_eq!(p.antiform_uses(), 2, "summon index 0 and form index 5 share 0x34184a");
        assert_eq!(p.summon_uses(), [1, 0, 0, 1]);
        let text = p.slot_json(&SlotInfo { level: 1, munny: 0, play_seconds: 0, area: "tt04".into() });
        let mut q = Progress::from_block(&[], std::sync::Arc::new(ItemIndex::default()));
        q.count_healed();
        q.apply_json(&text).expect("loads");
        assert_eq!((q.healed_count(), q.mickey_save_count(), q.form_uses(), q.summon_uses()), (999, 1, [2, 0, 0, 1, 2], [1, 0, 0, 1]), "the block key reset the old count");
        assert!(q.apply_json("{\"report_counters\": [1, 2]}").is_err());
    }

    /// WP-AH1: a member's HP and MP live in his party record (`+4`, `+6`, held to `+5`, `+7`; `FUN_001a0030` writes them
    /// back, `FUN_0019f920` reads them), Auto Healing's heal adds one HP up to the maximum, and the record goes through a
    /// slot file in the `block` key (an older file without the vitals changes nothing: they are bytes of the block).
    #[test]
    fn member_vitals_write_back_heal_and_round_trip() {
        let mut p = Progress::from_block(&[], Arc::new(ItemIndex::default()));
        let r = Progress::member_off(DONALD).unwrap();
        p.set_u8(r + rec::HP_MAX, 30);
        p.set_u8(r + rec::MP_MAX, 50);
        assert!(p.set_member_vitals(DONALD, 100, 7));
        assert_eq!(p.member_vitals(DONALD), Some((30, 7)), "HP is held to the maximum");
        assert!(p.set_member_vitals(DONALD, 28, 7));
        assert_eq!(p.heal_reserve_member(DONALD, 1), Some((28, 29)));
        assert_eq!(p.heal_reserve_member(DONALD, 1), Some((29, 30)));
        assert_eq!(p.heal_reserve_member(DONALD, 1), Some((30, 30)), "the maximum stops the heal");
        assert_eq!(p.member_vitals(GOOFY), Some((0, 0)), "Goofy's empty record");
        assert_eq!(p.member_vitals(99), None);
        assert!(!p.set_member_vitals(99, 1, 1));
        p.set_member_vitals(DONALD, 12, 34);
        let text = p.slot_json(&SlotInfo { level: 1, munny: 0, play_seconds: 0, area: "tt04".into() });
        let mut q = Progress::from_block(&[], Arc::new(ItemIndex::default()));
        q.apply_json(&text).expect("loads");
        assert_eq!(q.member_vitals(DONALD), Some((12, 34)), "the stored vitals come back");
        assert_eq!(q.member(DONALD).map(|m| (m.hp, m.hp_max, m.mp, m.mp_max)), Some((12, 30, 34, 50)));
    }

    /// WP-AH1b: the state file's `party_vitals` and `party_abilities` keys set a member's HP, MP and equipped abilities.
    #[test]
    fn state_file_sets_member_vitals_and_abilities() {
        let mut p = Progress::from_block(&[], Arc::new(ItemIndex::default()));
        p.set_member_maxima(DONALD, 30, 50);
        p.apply_json("{\"party_vitals\": {\"2\": [5, 9]}, \"party_abilities\": {\"2\": [420]}}").expect("loads");
        assert_eq!(p.member_vitals(DONALD), Some((5, 9)));
        assert_eq!(p.member(DONALD).map(|m| m.abilities), Some(vec![(420, true)]));
        assert!(p.apply_json("{\"party_vitals\": {\"99\": [1, 1]}}").is_err());
        assert!(p.apply_json("{\"party_abilities\": {\"2\": [40000]}}").is_err());
    }

    /// WP-MN15: the limits' best counts only go up, and go through a slot file and the `limit_best` key.
    #[test]
    fn limit_best_counts_only_go_up_and_are_saved() {
        let mut p = Progress::from_block(&[], Arc::new(ItemIndex::default()));
        assert_eq!(p.limit_best(4), 0);
        p.raise_limit_best(4, 9);
        p.raise_limit_best(4, 3);
        p.raise_limit_best(7, 0);
        assert_eq!((p.limit_best(4), p.limit_best(7)), (9, 0));
        let text = p.slot_json(&SlotInfo { level: 1, munny: 0, play_seconds: 0, area: "tt04".into() });
        let mut q = Progress::from_block(&[], Arc::new(ItemIndex::default()));
        q.raise_limit_best(1, 5);
        q.apply_json(&text).expect("loads");
        assert_eq!((q.limit_best(4), q.limit_best(1)), (9, 0), "the block key reset the old count");
        assert!(q.apply_json("{\"limit_best\": [2]}").is_err());
        assert_eq!(q.limit_best_all().collect::<Vec<_>>(), [(4, 9)]);
    }

    /// WP-PRG1: the moogle's record, the shops' bits, the record bits, the story arrays and the world points go through a
    /// slot file; a file without the keys (an older one) loads the defaults; the party settings are range-checked.
    #[test]
    fn a_slot_file_carries_the_moogle_the_shop_bits_and_the_story() {
        let mut p = Progress::from_block(&[], Arc::new(ItemIndex::default()));
        p.moogle = Moogle { known: 5, learned: 1 << 40, made: 6, seen: 7, unlocks: 1 << 10 | 3, rewards: 1 | 1 << 49, points: 1234, level: 3, stock: Vec::new(), totals: Vec::new() };
        p.moogle.set_stock(0, 7);
        p.moogle.set_stock(49, 999);
        p.moogle.set_total(3, 12);
        p.set_shop_seen(0);
        p.set_shop_seen(127);
        p.set_shop_visit(1);
        p.set_record_bit(0x45);
        {
            let mut st = p.story();
            st.set(0x801);
            st.set_world_point(5);
            st.lock_entry(2)[..].copy_from_slice(&[0xffff, 9]);
        }
        let text = p.slot_json(&SlotInfo { level: 1, munny: 0, play_seconds: 0, area: "tt04".into() });
        let mut q = Progress::from_block(&[], Arc::new(ItemIndex::default()));
        q.set_shop_seen(9);
        q.moogle.points = 99;
        q.apply_json(&text).expect("loads");
        assert_eq!(q.moogle.known, 5);
        assert_eq!((q.moogle.learned, q.moogle.made, q.moogle.seen, q.moogle.unlocks, q.moogle.rewards), (p.moogle.learned, 6, 7, p.moogle.unlocks, p.moogle.rewards));
        assert_eq!((q.moogle.points, q.moogle.level), (1234, 3));
        assert_eq!((q.moogle.stock_at(0), q.moogle.stock_at(49), q.moogle.total_at(3)), (7, 999, 12));
        assert!(q.shop_seen(0) && q.shop_seen(127) && !q.shop_seen(9), "the block key reset the old bit");
        assert!(q.shop_visit(1) && !q.shop_visit(0));
        assert!(q.record_bit(0x45) && !q.record_bit(0x44));
        {
            let st = q.story();
            assert!(st.test(0x801) && st.world_point(5) && !st.world_point(6));
            assert_eq!((st.lock_state(2), st.lock_message(2)), (0xffff, 9));
        }
        // An older slot file (no such keys) loads them as the defaults.
        let info = SlotInfo { level: 1, munny: 0, play_seconds: 0, area: "tt04".into() };
        let mut fields = p.slot_fields(&info, None);
        let before = fields.len();
        let persisted: Vec<&str> = p.persisted_fields().iter().map(|f| f.0).collect();
        fields.retain(|f| !persisted.contains(&f.0));
        assert_eq!((persisted.len(), before - fields.len()), (12, 12), "every persisted key is in the slot file (WP-MN15 added enemy_kills, reaction_uses, limit_best; WP-CT1 report_counters)");
        let old = Progress::join_fields(&fields);
        assert!(!old.contains("moogle") && !old.contains("story_raw") && old.contains("\"block\""));
        let mut r = Progress::from_block(&[], Arc::new(ItemIndex::default()));
        r.moogle.points = 5;
        r.set_shop_seen(3);
        {
            let mut st = r.story();
            st.set(0x801);
            st.set_world_point(9);
        }
        r.apply_json(&old).expect("an old file loads");
        {
            let st = r.story();
            assert!(!st.test(0x801) && !st.world_point(9), "an old slot loads the new-game story, not the running one");
        }
        // The story arrays are checked, not truncated or wrapped.
        assert!(r.apply_json("{\"story_raw\": {\"words\": \"0102030405\", \"programs\": \"\"}}").is_err(), "5 bytes are no u32 words");
        assert!(r.apply_json("{\"story_raw\": {\"words\": \"\", \"programs\": \"\", \"locks\": [1, 2, 3]}}").is_err(), "an odd lock list");
        assert!(r.apply_json("{\"story_raw\": {\"words\": \"\", \"programs\": \"\", \"locks\": [65536, 1]}}").is_err());
        assert!(r.apply_json("{\"record_bits\": [-1]}").is_err());
        assert!(r.apply_json("{\"record_bits\": [4294967296]}").is_err());
        assert!(r.apply_json("{\"new_seen\": [-5]}").is_err());
        r.apply_json("{\"record_bits\": [4294967295, 7]}").unwrap();
        assert_eq!(r.record_bits, vec![u32::MAX, 7]);
        assert_eq!((r.moogle.clone(), r.shop_seen(3), r.shop_visit(1)), (Moogle::default(), false, false));
        // The party settings: a byte past 255 and a style past 5 are refused, not truncated.
        assert!(r.apply_json("{\"party_settings\": {\"2\": 256}}").is_err());
        assert!(r.apply_json("{\"party_settings\": {\"2\": 6}}").is_err());
        assert!(r.apply_json("{\"party_settings\": {\"2\": [5, 300]}}").is_err());
        assert!(r.apply_json("{\"party_settings\": {\"2\": [5, 255]}}").is_ok());
        assert_eq!((r.member_setting(2, 0), r.member_setting(2, 1)), (Some(5), Some(255)));
    }

    /// WP-PA3: the members' setting block is zero at a new game (FUN_001875c8), the `party_settings` state key and the
    /// setter write it, and a slot file carries it with the block.
    #[test]
    fn member_settings_default_to_zero_and_round_trip() {
        let mut p = Progress::from_block(&[], Arc::new(ItemIndex::default()));
        p.init_party();
        assert_eq!(p.member(2).unwrap().settings, [0; 32]);
        assert_eq!(p.member_setting(2, 0), Some(0));
        p.apply_json("{\"party_settings\": {\"2\": 2, \"3\": [4, 1, 3]}}").unwrap();
        assert_eq!(p.member_setting(2, 0), Some(2));
        assert_eq!(p.member(3).unwrap().settings[..4], [4, 1, 3, 0]);
        assert_eq!(p.member_setting(1, 0), Some(0), "Sora's record is untouched");
        assert!(p.set_member_setting(2, 0, 5));
        assert!(!p.set_member_setting(2, 32, 1) && !p.set_member_setting(99, 0, 1));
        assert!(p.apply_json("{\"party_settings\": {\"99\": 1}}").is_err());
        let text = p.slot_json(&SlotInfo { level: 1, munny: 0, play_seconds: 0, area: "tt04".into() });
        let mut q = Progress::from_block(&[], Arc::new(ItemIndex::default()));
        q.apply_json(&text).unwrap();
        assert_eq!((q.member_setting(2, 0), q.member_setting(3, 2)), (Some(5), Some(3)));
    }

    struct Tables {
        items: Vec<Item>,
        cmds: Vec<Cmd>,
        magc: Vec<Magc>,
        plrp: Vec<Plrp>,
        archive: kh2_archive::Archive,
    }

    fn tables() -> Option<Tables> {
        let iso = std::env::var("KH2_ISO").ok().or_else(|| {
            let f = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../local/iso.txt");
            std::fs::read_to_string(f).ok().map(|s| s.trim().to_string())
        });
        let Some(iso) = iso else {
            eprintln!("skipped: no disc (set KH2_ISO)");
            return None;
        };
        let mut a = kh2_archive::Archive::open(&iso).map_err(|e| eprintln!("skipped: {iso}: {e}")).ok()?;
        Some(Tables {
            items: kh2_tables::load::<Item>(&mut a, &t::ITEM).ok()?.rows,
            cmds: kh2_tables::load::<Cmd>(&mut a, &t::CMD).ok()?.rows,
            magc: kh2_tables::load::<Magc>(&mut a, &t::MAGC).ok()?.rows,
            plrp: kh2_tables::load::<Plrp>(&mut a, &t::PLRP).ok()?.rows,
            archive: a,
        })
    }

    fn index(tb: &Tables) -> Arc<ItemIndex> {
        Arc::new(ItemIndex::new(&tb.items, &tb.cmds, &tb.magc))
    }

    /// The oracle for the pause menu's Items and Customize writers (WP-M3): the game's own `FUN_001a4b68` /
    /// `FUN_001a4b30` / `FUN_001a4bb8` (the spell switch bit and the flip `FUN_00274e50` makes of it) and
    /// `FUN_001a1ba8(id, 100)` (the Stock page's "Throw away" and the take after "Use") run in kh2-ee on the
    /// block the game built; the port must leave the same bytes. The shortcut write `FUN_00274dd0` is one u16
    /// store at `+0x2600 + 2 * slot` of the save block, checked at that address.
    #[test]
    fn menu_writers_match_the_game() {
        use kh2_ee::Ee;
        const STACK: u32 = 0x01ff_0000;
        const ITEM_PTR: u32 = 0x01c9_5588;
        const PLRP_PTR: u32 = 0x01d5_df5c;
        const CMD_PTR: u32 = 0x01d5_d798;
        const MAGC_PTR: u32 = 0x01c9_5520;
        let Some(mut tb) = tables() else { return };
        let ix = index(&tb);
        let elf = ["SLUS_210.05", "SLUS_210.05;1"].iter().find_map(|p| tb.archive.iso().read(p).ok()).expect("executable");
        let blob = |a: &mut kh2_archive::Archive, d: &kh2_tables::TableDef| kh2_tables::subfile(a, d).expect("table");
        let tables = [
            (ITEM_PTR, 0x0180_0000, blob(&mut tb.archive, &t::ITEM)),
            (PLRP_PTR, 0x0181_0000, blob(&mut tb.archive, &t::PLRP)),
            (CMD_PTR, 0x0182_0000, blob(&mut tb.archive, &t::CMD)),
            (MAGC_PTR, 0x0183_0000, blob(&mut tb.archive, &t::MAGC)),
        ];
        let mut ee = Ee::new();
        ee.load_elf(&elf).expect("elf");
        for (ptr, at, data) in &tables {
            ee.write(*at, data);
            ee.w32(*ptr, *at);
        }
        let party = addr::PARTY;
        ee.write(addr::SAVE, &vec![0u8; addr::SAVE_LEN as usize]);
        ee.call(0x0018_7b38, &[party as u64], &[], STACK).expect("FUN_00187b38");
        ee.call(0x001c_0cb0, &[0], &[], STACK).expect("FUN_001c0cb0");
        let read = |ee: &mut Ee| (0..off::LEN as u32).map(|o| ee.r8(party + o)).collect::<Vec<u8>>();
        let mut port = Progress::new_game(ix.clone(), &tb.plrp);
        let diff = |a: &[u8], b: &[u8]| a.iter().zip(b).enumerate().filter(|(_, (x, y))| x != y).map(|(o, (x, y))| format!("+{o:#x}: game {x:#x} port {y:#x}")).take(12).collect::<Vec<_>>();
        assert_eq!(diff(&read(&mut ee), port.block()), Vec::<String>::new(), "start");

        // The spell switch: set, clear, and the page's flip (read FUN_001a4bb8, write the opposite).
        for bit in [0u8, 3, 5, 31, 2, 3, 0, 17] {
            ee.call(0x001a_4b68, &[bit as u64], &[], STACK).expect("FUN_001a4b68");
            port.set_spell_switch(bit, true);
            assert_eq!(diff(&read(&mut ee), port.block()), Vec::<String>::new(), "set bit {bit}");
            ee.call(0x001a_4b30, &[bit as u64], &[], STACK).expect("FUN_001a4b30");
            port.set_spell_switch(bit, false);
            assert_eq!(diff(&read(&mut ee), port.block()), Vec::<String>::new(), "clear bit {bit}");
        }
        let mut flips = Vec::new();
        for bit in [1u8, 4, 1, 1, 9, 4] {
            let state = ee.call(0x001a_4bb8, &[bit as u64], &[], STACK).expect("FUN_001a4bb8") as u8;
            // FUN_00274e50: state 0 (the bit is set) clears it, anything else sets it.
            ee.call(if state == 0 { 0x001a_4b30 } else { 0x001a_4b68 }, &[bit as u64], &[], STACK).expect("set or clear");
            let was_set = port.toggle_spell_switch(bit);
            assert_eq!(state == 0, was_set, "bit {bit}: the game read {state}");
            flips.push(!was_set);
            assert_eq!(diff(&read(&mut ee), port.block()), Vec::<String>::new(), "flip bit {bit}");
        }

        // The shortcut word: save +0x2600 is party block +0xfa0 (0x340e60); one u16 per slot.
        for (slot, cmd) in [(0usize, 23u16), (3, 0x1234), (1, 0)] {
            ee.write(0x0034_0e60 + 2 * slot as u32, &cmd.to_le_bytes());
            port.set_shortcut(slot, cmd);
        }
        assert_eq!(diff(&read(&mut ee), port.block()), Vec::<String>::new(), "shortcuts");

        // Throw away / the take after Use: FUN_001a1ba8(id, 100) over the owned items and some not owned.
        let owned: Vec<u16> = ix.rows.iter().filter(|r| r.kind == item_type::CONSUMABLE && !r.is_key() && r.id != 0).map(|r| r.id).take(6).collect();
        for (k, &id) in owned.iter().enumerate() {
            for _ in 0..=(k % 3) {
                let ok = ee.call(0x001a_18d8, &[id as u64, INVENTORY as u64, 0], &[], STACK).expect("FUN_001a18d8") as u32 != 0;
                assert_eq!(port.give(id, INVENTORY, false), ok);
            }
        }
        let mut taken = 0;
        for &id in owned.iter().chain(&owned).chain(&owned) {
            ee.call(0x001a_1ba8, &[id as u64, INVENTORY as u64], &[], STACK).expect("FUN_001a1ba8");
            assert!(port.take_item(id, INVENTORY));
            taken += 1;
            assert_eq!(diff(&read(&mut ee), port.block()), Vec::<String>::new(), "after taking item {id} from the stock");
        }
        eprintln!("oracle: menu writers equal (spell switch set/clear/flip {flips:?}, 4 shortcut words, {taken} stock takes; counts floor at 0)");
    }

    /// The oracle for the pause menu's equipment writers (WP-M4): the game's own `FUN_0026c420` (main weapon),
    /// `FUN_0026c4d0` (a form's keyblade), `FUN_0026c588` (armor slot) and `FUN_0026c610` (accessory slot) run in
    /// kh2-ee on the block the game built, with the page's party object faked (`DAT_0036ce90` points at a table
    /// whose row 0 holds a status object whose `+0x238` is Sora's record; the page's remembered cursor
    /// `FUN_00252fb8(1)` reads 0) and the live-weapon queue `FUN_002a1060` stubbed (it only queues a request for
    /// the field). The port must leave the same bytes and answer the same value after every swap.
    #[test]
    fn equipment_writers_match_the_game() {
        use kh2_ee::Ee;
        const STACK: u32 = 0x01ff_0000;
        const TABLE: u32 = 0x0197_0000;
        const STATUS: u32 = 0x0198_0000;
        let Some(mut tb) = tables() else { return };
        let ix = index(&tb);
        let elf = ["SLUS_210.05", "SLUS_210.05;1"].iter().find_map(|p| tb.archive.iso().read(p).ok()).expect("executable");
        let blob = |a: &mut kh2_archive::Archive, d: &kh2_tables::TableDef| kh2_tables::subfile(a, d).expect("table");
        let tables = [
            (0x01c9_5588u32, 0x0180_0000u32, blob(&mut tb.archive, &t::ITEM)),
            (0x01d5_df5c, 0x0181_0000, blob(&mut tb.archive, &t::PLRP)),
            (0x01d5_d798, 0x0182_0000, blob(&mut tb.archive, &t::CMD)),
            (0x01c9_5520, 0x0183_0000, blob(&mut tb.archive, &t::MAGC)),
        ];
        let mut ee = Ee::new();
        ee.load_elf(&elf).expect("elf");
        for (ptr, at, data) in &tables {
            ee.write(*at, data);
            ee.w32(*ptr, *at);
        }
        let party = addr::PARTY;
        ee.write(addr::SAVE, &vec![0u8; addr::SAVE_LEN as usize]);
        ee.call(0x0018_7b38, &[party as u64], &[], STACK).expect("FUN_00187b38");
        ee.call(0x001c_0cb0, &[0], &[], STACK).expect("FUN_001c0cb0");
        let mut port = Progress::new_game(ix.clone(), &tb.plrp);
        let r = Progress::member_off(SORA).unwrap();
        // The fake party object: one row, id 1, the status object pointer at row +0x14; the record at status +0x238.
        ee.w32(0x0036_ce90, TABLE);
        ee.w32(TABLE, 1);
        ee.write(TABLE + 4, &1u16.to_le_bytes());
        ee.write(TABLE + 6, &1u16.to_le_bytes());
        ee.w32(TABLE + 0x14, STATUS);
        ee.w32(STATUS + 0x238, party + r as u32);
        // `FUN_002a1060` returns at once.
        ee.write(0x002a_1060, &[0x08, 0x00, 0xe0, 0x03, 0, 0, 0, 0]);
        // Slots: two armor and two accessory slots (the game does not look at the counts), set in both blocks.
        for (o, n) in [(rec::ARMOR_SLOTS, 2u8), (rec::ACCESSORY_SLOTS, 2)] {
            ee.write(party + (r + o) as u32, &[n]);
            port.set_u8(r + o, n);
        }
        let read = |ee: &mut Ee| (0..off::LEN as u32).map(|o| ee.r8(party + o)).collect::<Vec<u8>>();
        let diff = |a: &[u8], b: &[u8]| a.iter().zip(b).enumerate().filter(|(_, (x, y))| x != y).map(|(o, (x, y))| format!("+{o:#x}: game {x:#x} port {y:#x}")).take(12).collect::<Vec<_>>();
        assert_eq!(diff(&read(&mut ee), port.block()), Vec::<String>::new(), "start");
        let of_kind = |k: u8, n: usize| ix.rows.iter().filter(|x| x.kind == k && !x.is_key() && x.id != 0).map(|x| x.id).take(n).collect::<Vec<u16>>();
        let (keys, armor, acc) = (of_kind(item_type::KEYBLADE, 3), of_kind(item_type::ARMOR, 3), of_kind(item_type::ACCESSORY, 3));
        assert!(keys.len() == 3 && armor.len() == 3 && acc.len() == 3, "the item table has keyblades, armor and accessories");
        // Own two of each (a second copy of the first, so a swap keeps a count above 0), through the game's own give.
        for &id in keys.iter().chain(&armor).chain(&acc).chain(&keys[..1]).chain(&armor[..1]).chain(&acc[..1]) {
            let ok = ee.call(0x001a_18d8, &[id as u64, INVENTORY as u64, 0], &[], STACK).expect("FUN_001a18d8") as u32 != 0;
            assert_eq!(port.give(id, INVENTORY, false), ok);
        }
        // The new game holds no weapon (0) in the main hand and the forms. The game's writers give the old id back
        // unconditionally, and its item lookup answers a null row for id 0 whose zero bytes bump count slot 0 (save
        // `+0xe28`) -- a junk side effect the port does not model (`give` refuses an unknown id) and the menu never
        // reaches (a keyblade is always held). So start from held keyblades, set the same way in both blocks.
        for o in std::iter::once(r).chain((1..=4u8).map(|f| Progress::form_off(f).unwrap())) {
            ee.write(party + o as u32, &keys[0].to_le_bytes());
            port.set_u16(o, keys[0]);
        }
        assert_eq!(diff(&read(&mut ee), port.block()), Vec::<String>::new(), "after the purchases");
        // The game's four writers answer 0 or 1; the port the same. A swap, the same item again, a removal, a swap back.
        let mut swaps = 0;
        let mut run = |ee: &mut Ee, port: &mut Progress, what: &str, game: u32, args: &[u64], f: &dyn Fn(&mut Progress) -> bool| {
            let g = ee.call(game, args, &[], STACK).expect("the game's writer") as u32 & 0xff;
            let p = f(port);
            assert_eq!(g != 0, p, "{what}: the answer");
            assert_eq!(diff(&read(ee), port.block()), Vec::<String>::new(), "{what}");
            swaps += 1;
        };
        for &w in &[keys[1], keys[1], keys[2], keys[0], keys[0]] {
            run(&mut ee, &mut port, &format!("weapon {w}"), 0x0026_c420, &[w as u64], &|p| p.equip_weapon(SORA, w));
        }
        for (slot, id) in [(0usize, armor[0]), (0, armor[0]), (1, armor[1]), (0, armor[2]), (1, 0), (0, 0), (0, armor[1])] {
            run(&mut ee, &mut port, &format!("armor slot {slot} item {id}"), 0x0026_c588, &[slot as u64, id as u64], &|p| p.equip_armor(SORA, slot, id));
        }
        for (slot, id) in [(0usize, acc[0]), (1, acc[0]), (1, acc[1]), (0, 0), (0, acc[2]), (1, 0)] {
            run(&mut ee, &mut port, &format!("accessory slot {slot} item {id}"), 0x0026_c610, &[slot as u64, id as u64], &|p| p.equip_accessory(SORA, slot, id));
        }
        // A form's keyblade: forms 1, 3 and 4 (the Valor, Master and Final hands).
        for (form, id) in [(1u8, keys[1]), (3, keys[2]), (1, keys[1]), (1, keys[0]), (4, keys[1])] {
            run(&mut ee, &mut port, &format!("form {form} weapon {id}"), 0x0026_c4d0, &[form as u64, id as u64], &|p| p.equip_form_weapon(form, id));
        }
        eprintln!("oracle: equipment writers equal after {swaps} swaps (weapon, armor, accessory, form weapon; stock counts and slots)");
    }

    #[test]
    fn json_reader_handles_the_schema_shapes() {
        let j = Json::parse(r#" { "a": [1, 2.5, -3e2], "b": {"x": true, "y": null}, "s": "q\"A\n" } "#).unwrap();
        assert_eq!(j.get("a"), Some(&Json::Arr(vec![Json::Num(1.0), Json::Num(2.5), Json::Num(-300.0)])));
        assert_eq!(j.get("b").and_then(|b| b.get("x")), Some(&Json::Bool(true)));
        assert_eq!(j.get("s"), Some(&Json::Str("q\"A\n".into())));
        assert!(Json::parse("{\"a\": 1,}").is_err());
        assert!(Json::parse("[1 2]").is_err());
        assert!(Json::parse("{} x").is_err());
    }

    #[test]
    fn state_file_sets_the_fields() {
        let Some(tb) = tables() else { return };
        let ix = index(&tb);
        let mut p = Progress::new_game(ix.clone(), &tb.plrp);
        let fire = ix.magic_items[0];
        let valor = ix.form_items[1];
        let json = format!(
            r#"{{ "experience": 1234, "munny": 4321, "magic": [3, 0, 2], "items": {{ "{fire}": 2, "1": 7 }},
                 "weapon": {KINGDOM_KEY}, "abilities": [82, 162], "abilities_off": [394],
                 "forms": {{ "1": {{ "owned": true, "level": 3, "exp": 12 }}, "2": {{ "level": 2 }} }},
                 "summon": {{ "level": 4, "exp": 99 }}, "outfits": {{ "2": 1 }}, "spell_switch": 5 }}"#
        );
        p.apply_json(&json).unwrap();
        assert_eq!(p.experience(), 1234);
        assert_eq!(p.wallet(), 4321, "the munny key sets the wallet, not the experience");
        // "items" comes after "magic" in the file, so Fire ends at 2.
        assert_eq!(p.spell_levels()[..3], [2, 0, 2]);
        assert_eq!(p.item_count(1), 7);
        assert_eq!(p.sora().weapons[0], KINGDOM_KEY);
        assert_eq!(p.sora().abilities, vec![(82, true), (162, true), (394, false)]);
        assert!(p.owns_form(1) && !p.owns_form(2));
        assert_eq!(p.item_count(valor), 1);
        assert_eq!((p.form_level(1), p.form_exp(1), p.form_level(2)), (3, 12, 2));
        assert_eq!((p.summon_level(), p.summon_exp()), (4, 99));
        assert_eq!(p.outfit(2), 1);
        assert!(p.spell_switch(0) && !p.spell_switch(1) && p.spell_switch(2));
        assert!(p.apply_json(r#"{"munnny": 1}"#).is_err(), "typos are reported");
    }

    /// A world entry of `00progress.bin` with `n` flags whose lists are given (the others end at once):
    /// the offset table, then a shared end byte, then each list.
    fn flag_entry(n: usize, lists: &[(u16, Vec<u8>)]) -> Vec<u8> {
        let end = 2 * n;
        let mut d = vec![0u8; end + 1];
        for k in 0..n {
            d[2 * k..2 * k + 2].copy_from_slice(&(end as u16).to_le_bytes());
        }
        for (index, list) in lists {
            let at = d.len() as u16;
            d[2 * *index as usize..2 * *index as usize + 2].copy_from_slice(&at.to_le_bytes());
            d.extend_from_slice(list);
            d.push(0);
        }
        d
    }

    /// WP-E5 item 1: the flag array at 0x33f6b0 addressed by id (`FUN_001802e0` test, `FUN_00180298`
    /// clear, `FUN_0017ff60` set), without side effects.
    #[test]
    fn story_flags_set_test_and_clear_by_id() {
        let mut s = Story::default();
        assert!(!s.test(0x883), "a new game has every flag clear");
        s.set(0x883);
        // Page 2 (0x883 >> 10), word 4 ((0x83) >> 5), bit 3.
        assert_eq!(Story::slot(0x883), Some((2 * 8 + 4, 1 << 3)));
        assert!(s.test(0x883) && !s.test(0x882) && !s.test(0x483));
        s.set(0x8db);
        s.set(0x0);
        assert_eq!(s.set_flags(), vec![0x0, 0x883, 0x8db]);
        s.clear(0x883);
        assert!(!s.test(0x883) && s.test(0x8db));
        // 0xffff always tests clear; a page past the 19 worlds is not in the array.
        assert!(!s.test(0xffff));
        s.set(19 << 10);
        assert!(!s.test(19 << 10));
        assert_eq!(s.set_flags(), vec![0x0, 0x8db]);
        assert_eq!(s.programs(2, 4), [0, 0, 0]);
    }

    /// `FUN_0017ff60`'s side effects on a synthetic world-2 entry: op 1 writes the program bytes of
    /// the flag's world (0xff keeps), op 2 a lock entry, op 0xc sets flags not yet set (and their
    /// lists run), op 7 clears, op 4 / 3 write the lock state; op 6 is recorded as not ported.
    #[test]
    fn story_flag_side_effects_write_programs_locks_and_flags() {
        let lists = vec![
            // 0x883: area 4 btl 1; lock entry 3 = (5, 0x1234); set 0x805.
            (0x83, vec![1, 2, 4, 0xff, 1, 0xff, 2, 3, 3, 0, 5, 0, 0x34, 0x12, 0xc, 1, 0x05, 0x08]),
            // 0x805: clear 0x806; lock entry 3 state 0xffff; op 6.
            (0x05, vec![7, 1, 0x06, 0x08, 4, 1, 3, 0, 6, 1, 2, 0]),
            // 0x807: lock entry 3 state 0.
            (0x07, vec![3, 1, 3, 0]),
        ];
        let mut s = Story { effects: Arc::new(vec![Vec::new(), Vec::new(), flag_entry(0x100, &lists)]), ..Story::default() };
        s.set(0x806);
        s.set(0x883);
        assert_eq!(s.programs(2, 4), [0, 1, 0]);
        assert_eq!(s.programs(2, 3), [0, 0, 0]);
        assert!(s.test(0x805) && !s.test(0x806), "0xc set 0x805, whose op 7 cleared 0x806");
        assert_eq!((s.lock_state(3), s.lock_message(3)), (0xffff, 0x1234));
        assert_eq!(s.unported, vec![(0x805, 4), (0x805, 6)]);
        s.set(0x807);
        assert_eq!(s.lock_state(3), 0);
        assert_eq!(s.lock_state(40), 0, "entries no flag wrote read 0");
        // A flag of another world (no entry) only sets its bit.
        s.set(0x1883);
        assert!(s.test(0x1883) && s.programs(6, 4) == [0; 3]);
    }

    /// The world-point bits (0x33f910, `FUN_00194fa8`) are kept in the save block's story and ride the
    /// state file as `world_points`.
    #[test]
    fn world_point_bits_are_kept_and_round_trip() {
        let ix = Arc::new(ItemIndex::default());
        let p = Progress::from_block(&[], ix.clone());
        assert!(p.story().world_points().is_empty());
        assert_eq!(p.story_json(), r#"{"story_flags": []}"#, "a new game has none");
        p.story().set_world_point(1);
        p.story().set_world_point(33);
        p.story().set_world_point(0x36 + 100); // past the two words: ignored
        p.story().set_world_point(63); // inside the second word, but not a record (0x36 of them): ignored
        p.story().set_world_point(64); // the first bit past the two words: ignored
        assert!(p.story().world_point(1) && p.story().world_point(33) && !p.story().world_point(2));
        assert!(!p.story().world_point(63) && !p.story().world_point(64));
        p.story().set_world_point(0x35); // the last record
        assert!(p.story().world_point(0x35));
        p.story().set_world_point(0x36); // one past the last record
        assert!(!p.story().world_point(0x36));
        let p = Progress::from_block(&[], ix.clone());
        p.story().set_world_point(1);
        p.story().set_world_point(33);
        let text = p.story_json();
        assert_eq!(text, r#"{"story_flags": [], "world_points": [1, 33]}"#);
        let mut q = Progress::from_block(&[], ix);
        q.apply_json(&text).unwrap();
        assert_eq!(q.story().world_points(), vec![1, 33]);
    }

    /// The `story_flags` / `story_clear` keys of the state file, and `story_json` read back.
    #[test]
    fn story_flags_round_trip_through_the_state_file() {
        let ix = Arc::new(ItemIndex::default());
        let mut p = Progress::from_block(&[], ix.clone());
        p.apply_json(r#"{"story_flags": [2179, 2267, 4096, 2050], "story_clear": [2050]}"#).unwrap();
        assert_eq!(p.story().set_flags(), vec![0x883, 0x8db, 0x1000]);
        let text = p.story_json();
        assert_eq!(text, r#"{"story_flags": [2179, 2267, 4096]}"#);
        let mut q = Progress::from_block(&[], ix);
        q.apply_json(&text).unwrap();
        assert_eq!(q.story().set_flags(), p.story().set_flags());
        assert!(q.apply_json(r#"{"story_flags": 3}"#).is_err());
        // A clone shares the story (the hosts' handle is the same store).
        let r = q.clone();
        q.story().clear(0x883);
        assert!(!r.story().test(0x883));
    }

    /// The oracle for `FUN_0017ff60` (WP-E5): for every flag of every world entry of `00progress.bin`,
    /// the game's setter runs in kh2-ee on a cleared save block and the port must leave the same flag
    /// words (0x33f6b0, 0x260 bytes), program table (0x33e86f, 19 * 0xc0 bytes) and lock halfwords
    /// (0x33f918, 4 per `dsa` entry). The calls after the list (`FUN_001803b0` .. `FUN_00195130`, which
    /// reach menus, tutorials and the world map) are `jr ra` stubs: they do not write these regions
    /// [inferred from their names in docs/events.md section 5; not read]. The world-point table
    /// `DAT_01c8a984` points at zeros, so ops 4 and 5 find no point (as the port, which leaves the bits).
    #[test]
    fn story_flag_setter_matches_the_game() {
        use kh2_ee::Ee;
        const STACK: u32 = 0x01ff_0000;
        let Some(mut tb) = tables() else { return };
        let file = tb.archive.read("00progress.bin").expect("00progress.bin");
        let bar = kh2_bar::parse(&file).expect("BAR");
        let entries: Vec<Vec<u8>> = bar.iter().filter(|e| e.kind == 2).take(story::WORLDS).map(|e| e.data.to_vec()).collect();
        let dsa = bar.iter().find(|e| e.tag == kh2_ard::progress::LOCK_TABLE).expect("dsa").data.to_vec();
        let locks = u16::from_le_bytes([dsa[0], dsa[1]]) as usize;
        let elf = ["SLUS_210.05", "SLUS_210.05;1"].iter().find_map(|p| tb.archive.iso().read(p).ok()).expect("executable");
        let mut ee = Ee::new();
        ee.load_elf(&elf).expect("elf");
        let mut at = 0x0180_0000u32;
        for (w, e) in entries.iter().enumerate() {
            ee.write(at, e);
            ee.w32(0x0035_ccc0 + 4 * w as u32, at);
            at = (at + e.len() as u32 + 0xf) & !0xf;
        }
        ee.write(at, &dsa);
        ee.w32(0x01c8_a988, at);
        at = (at + dsa.len() as u32 + 0xf) & !0xf;
        ee.write(at, &[0u8; 0x100]);
        ee.w32(0x01c8_a984, at);
        for stub in [0x001803b0u32, 0x00180450, 0x001805d0, 0x00180558, 0x001d39d0, 0x001d3238, 0x001b6da0, 0x001be9c8, 0x001d5718, 0x0019d890, 0x00195130] {
            ee.w32(stub, 0x03e0_0008);
            ee.w32(stub + 4, 0);
        }
        let effects = Arc::new(entries.clone());
        let flags_len = story::WORLDS * story::PAGE;
        let prog_len = story::WORLDS * story::PROGRAMS_PER_WORLD;
        let mut checked = 0;
        let mut with_effects = 0;
        for (w, e) in entries.iter().enumerate() {
            for f in 0..kh2_ard::progress::flag_count(e) as u16 {
                let id = (w as u16) << 10 | f;
                ee.write(story::FLAGS, &vec![0u8; flags_len]);
                ee.write(story::PROGRAMS, &vec![0u8; prog_len]);
                ee.write(story::LOCKS, &vec![0u8; 4 * locks]);
                ee.call(0x0017_ff60, &[id as u64], &[], STACK).unwrap_or_else(|e| panic!("FUN_0017ff60({id:#x}): {e:?}"));
                let mut s = Story { effects: effects.clone(), ..Story::default() };
                s.set(id);
                let words: Vec<u32> = (0..flags_len / 4).map(|k| ee.r32(story::FLAGS + 4 * k as u32)).collect();
                assert_eq!(s.words, words, "flag {id:#x}: flag words");
                let prog: Vec<u8> = (0..prog_len).map(|k| ee.r8(story::PROGRAMS + k as u32)).collect();
                assert_eq!(s.programs, prog, "flag {id:#x}: program table");
                for i in 0..locks {
                    let game = (ee.r16(story::LOCKS + 4 * i as u32), ee.r16(story::LOCKS + 4 * i as u32 + 2));
                    assert_eq!((s.lock_state(i), s.lock_message(i)), game, "flag {id:#x}: lock entry {i}");
                }
                checked += 1;
                with_effects += (s.set_flags().len() > 1 || s.programs != vec![0; prog_len] || s.locks.iter().any(|l| *l != [0, 0])) as usize;
            }
        }
        eprintln!("oracle: FUN_0017ff60 equal for {checked} flags of {} worlds ({with_effects} change more than their own bit)", entries.len());
        assert!(checked > 1000 && with_effects > 100, "{checked} {with_effects}");
    }

    /// WP-E5 item 3 data: on the disc, setting flag 0x883 gives tt04 btl program 1 (docs/events.md
    /// section 5), and 0x814 sets tt04 back to (0, 0, 0).
    #[test]
    fn flag_0x883_selects_tt04_btl_program_one() {
        let Some(mut tb) = tables() else { return };
        let file = tb.archive.read("00progress.bin").expect("00progress.bin");
        let entries: Vec<Vec<u8>> = kh2_bar::parse(&file).expect("BAR").iter().filter(|e| e.kind == 2).map(|e| e.data.to_vec()).collect();
        let mut s = Story { effects: Arc::new(entries), ..Story::default() };
        s.set(0x883);
        eprintln!("after 0x883: flags {:x?} tt04 {:?} unported {:x?}", s.set_flags(), s.programs(2, 4), s.unported);
        assert_eq!(s.programs(2, 4)[1], 1);
        s.set(0x814);
        assert_eq!(s.programs(2, 4), [0, 0, 0]);
    }

    /// `FUN_0026c698` (equip a battle item, the pause menu writer) and the `sora_items` state key
    /// (`FUN_001a18d8` to Sora, what a pickup does).
    #[test]
    fn battle_item_equip_swaps_with_the_stock() {
        let Some(tb) = tables() else { return };
        let ix = index(&tb);
        let mut p = Progress::new_game(ix.clone(), &tb.plrp);
        let r = Progress::member_off(SORA).unwrap();
        p.set_item_slots(r, 3);
        let (potion, ether) = (1u16, 3u16);
        p.add_item(potion, 2);
        assert!(p.equip_battle_item(SORA, 0, potion, p.item_count(potion) > 0));
        assert_eq!((p.battle_items(SORA)[0], p.item_count(potion)), ((potion, 0), 1), "one off the stock into the slot");
        // Not carried: the item is only reserved and the old one goes back.
        assert!(p.equip_battle_item(SORA, 0, ether, p.item_count(ether) > 0));
        assert_eq!((p.battle_items(SORA)[0], p.item_count(potion)), ((0, ether), 2));
        assert!(p.equip_battle_item(SORA, 0, 0, false));
        assert_eq!(p.battle_items(SORA)[0], (0, 0));
        assert!(!p.equip_battle_item(SORA, 8, potion, true));
        // The state file: two Potions to Sora fill two slots and the received list.
        let mut q = Progress::new_game(ix, &tb.plrp);
        q.set_item_slots(Progress::member_off(SORA).unwrap(), 3);
        q.apply_json(r#"{"sora_items": [1, 1]}"#).unwrap();
        assert_eq!(q.battle_items(SORA).iter().filter(|s| s.0 == potion).count(), 2);
        assert_eq!(q.recent_items()[0], potion);
        assert!(q.take_item(potion, SORA) && q.take_item(potion, SORA) && !q.take_item(potion, SORA));
    }

    /// An item index without the disc: keyblades 10 and 11, armor 20, accessories 30 and 31, consumables 1 and 2
    /// (the count slot of each is its id).
    fn small_index() -> Arc<ItemIndex> {
        let row = |id: u16, kind: u8| ItemInfo { id, kind, flag0: 0, word4: 0, command: 0, slot: id, stat: 0 };
        let rows = vec![row(1, 0), row(2, 0), row(10, 2), row(11, 2), row(20, item_type::ARMOR), row(30, item_type::ACCESSORY), row(31, item_type::ACCESSORY)];
        let by_id = rows.iter().enumerate().map(|(n, r)| (r.id, n)).collect();
        Arc::new(ItemIndex { rows, by_id, ..ItemIndex::default() })
    }

    /// The equipment writers keep the stock: the old item goes back, the new one comes out; the same item again
    /// and a slot that does not exist change nothing.
    #[test]
    fn equipment_writers_swap_with_the_stock_and_keep_the_counts() {
        let mut p = Progress::from_block(&[], small_index());
        let r = Progress::member_off(SORA).unwrap();
        p.set_u8(r + rec::ARMOR_SLOTS, 1);
        p.set_u8(r + rec::ACCESSORY_SLOTS, 2);
        p.add_item(10, 2);
        p.add_item(11, 1);
        p.add_item(20, 1);
        p.add_item(30, 1);
        // The weapon: 10 in, 10 out of two; then 11, which sends 10 back.
        assert!(p.equip_weapon(SORA, 10));
        assert_eq!((p.sora().weapons[0], p.item_count(10)), (10, 1));
        assert!(p.equip_weapon(SORA, 11));
        assert_eq!((p.sora().weapons[0], p.item_count(10), p.item_count(11)), (11, 2, 0));
        assert!(!p.equip_weapon(SORA, 11), "the same weapon changes nothing");
        assert_eq!((p.item_count(10), p.item_count(11)), (2, 0));
        // Armor: in, out (the slot empties, the item returns), no second slot.
        assert!(p.equip_armor(SORA, 0, 20));
        assert_eq!((p.sora().armor, p.item_count(20)), (vec![20], 0));
        assert!(!p.equip_armor(SORA, 0, 20));
        assert!(p.equip_armor(SORA, 0, 0));
        assert_eq!((p.sora().armor, p.item_count(20)), (vec![0], 1));
        assert!(!p.equip_armor(SORA, 0, 0), "an empty slot stays empty");
        assert!(!p.equip_armor(SORA, 1, 20), "the record has one armor slot");
        // Accessories: the two slots are separate words.
        assert!(p.equip_accessory(SORA, 1, 30));
        assert_eq!((p.sora().accessories, p.item_count(30)), (vec![0, 30], 0));
        assert!(p.equip_accessory(SORA, 0, 31) && p.item_count(31) == 0, "taking an item the stock does not have floors at 0");
        assert!(p.equip_accessory(SORA, 1, 31));
        assert_eq!((p.sora().accessories, p.item_count(30), p.item_count(31)), (vec![31, 31], 1, 0));
        // A form's keyblade: the form record's own word.
        assert!(p.equip_form_weapon(1, 10));
        assert_eq!((p.form(1).weapon, p.item_count(10)), (10, 1));
        assert!(p.equip_form_weapon(1, 11) && p.form(1).weapon == 11 && p.item_count(10) == 2);
        assert!(!p.equip_form_weapon(0, 10) && !p.equip_form_weapon(10, 10), "forms 1 to 9 only");
        // A full stock loses the old item (the menu refuses that swap before it gets here).
        p.set_count(10, 99);
        assert!(p.equip_weapon(SORA, 10));
        assert_eq!((p.sora().weapons[0], p.item_count(10), p.item_count(11)), (10, 98, 1));
        p.set_count(10, 99);
        assert!(p.equip_weapon(SORA, 11));
        assert_eq!((p.sora().weapons[0], p.item_count(10), p.item_count(11)), (11, 99, 0), "the old 10 had no room and is gone");
        assert_eq!(p.equipment(), vec![11, 31, 31]);
    }

    /// `FUN_0026c770` then `FUN_001877c8`: an item moved between slots keeps its reservation, and a slot that
    /// reserves an item the stock has is refilled.
    #[test]
    fn leaving_the_equipment_lists_settles_the_reservations_and_refills() {
        let mut p = Progress::from_block(&[], small_index());
        let r = Progress::member_off(SORA).unwrap();
        p.set_u8(r + rec::ITEM_SLOTS, 4);
        // Slot 0 reserves item 1 but is empty, slot 1 carries item 1 without a reservation, slot 2 carries 2 and
        // reserves it, slot 3 reserves 2 but is empty.
        for (i, (item, reserved)) in [(0u16, 1u16), (1, 0), (2, 2), (0, 2)].into_iter().enumerate() {
            p.set_u16(r + rec::ITEMS + 2 * i, item);
            p.set_u16(r + rec::RESERVED + 2 * i, reserved);
        }
        p.set_count(1, 1);
        p.settle_battle_items(SORA);
        // Slot 1 carries item 1, which another slot reserves: it takes the reservation; then the empty slots that
        // reserve something the stock has take it (item 1: one in stock, slot 0 gets it; item 2: none).
        assert_eq!(p.battle_items(SORA), vec![(1, 1), (1, 1), (2, 2), (0, 2)]);
        assert_eq!(p.item_count(1), 0);
    }

    #[test]
    fn baseline_gives_the_old_app_defaults() {
        let Some(tb) = tables() else { return };
        let mut p = Progress::new_game(index(&tb), &tb.plrp);
        p.apply_baseline(true);
        assert_eq!(p.spell_levels(), [1, 1, 1, 1, 0, 0]);
        assert_eq!((1..=5).map(|f| p.owns_form(f)).collect::<Vec<_>>(), [true, true, true, true, false]);
        assert_eq!(p.sora().weapons[0], KINGDOM_KEY);
        assert!(p.sora().abilities.contains(&(crate::abilities::id::GUARD, true)));
        assert_eq!(p.dodge_roll, 1, "the baseline gives Sora the Dodge Roll");
        // Valor, Master and Final took the three spare Kingdom Keys (FUN_001a2010).
        assert_eq!((1..=4).map(|f| p.form(f).weapon).collect::<Vec<_>>(), [KINGDOM_KEY, 0, KINGDOM_KEY, KINGDOM_KEY]);
        assert!(!p.keyblades().contains(&KINGDOM_KEY));
        let mut q = Progress::new_game(p.items.clone(), &tb.plrp);
        q.apply_baseline(false);
        assert!(q.sora().abilities.is_empty(), "KH2_ABILITY_START=data: no Guard");
        assert_eq!(q.dodge_roll, 0, "KH2_ABILITY_START=data: no Dodge Roll either");
        q.apply_json(r#"{"dodge_roll": 3}"#).unwrap();
        assert_eq!(q.dodge_roll, 3);
    }

    /// WP-PG1: the new game fills Donald's and Goofy's records from the disc's `plrp` rows with `Id` 0
    /// (`FUN_001c0cb0(0)`): vitals, slot counts, battle items and equipped ability items. No row of `Id` 0 or 1
    /// carries a weapon item, so the weapon words stay 0 until the app baseline (or a state file) gives them.
    #[test]
    fn donald_and_goofy_start_from_their_plrp_rows() {
        let Some(tb) = tables() else { return };
        let mut p = Progress::new_game(index(&tb), &tb.plrp);
        for (member, hp, abilities) in [(DONALD, 18u8, [167u16, 168]), (GOOFY, 25, [429, 411])] {
            let row = tb.plrp.iter().find(|r| r.id == 0 && r.character as u16 == member).expect("an Id 0 row");
            assert_eq!(row.hp, hp);
            let m = p.member(member).unwrap();
            assert_eq!((m.hp, m.hp_max, m.mp_max, m.level), (hp, hp, 100, 1));
            assert_eq!(m.armor.len(), row.armor_slot_max as usize);
            assert_eq!(m.accessories.len(), row.accessory_slot_max as usize);
            assert_eq!(m.items.len(), row.item_slot_max as usize);
            for a in abilities {
                assert!(m.abilities.contains(&(a, true)), "member {member} ability {a} equipped by its plrp row");
            }
            assert_eq!(m.weapons, [0, 0], "no plrp row gives member {member} a weapon");
        }
        // No row of either Id carries a weapon or shield item for the members (the searched negative result).
        for r in tb.plrp.iter().filter(|r| r.character == 2 || r.character == 3) {
            for &v in r.items.iter().filter(|&&v| v != 0) {
                let kind = tb.items.iter().find(|i| i.id == v & 0x7fff).map(|i| i.r#type);
                assert!(!matches!(kind, Some(3 | 4)), "plrp Id {} character {}: item {} is a weapon", r.id, r.character, v & 0x7fff);
            }
        }
        // The staff and the shield are a staff and a shield item of the table (types 3 and 4).
        let kind = |id: u16| tb.items.iter().find(|i| i.id == id).map(|i| i.r#type);
        let boot = disc_boot_weapons().expect("00common.bdx event 7");
        p.boot_weapons = Arc::new(boot);
        let (staff, shield) = (p.boot_weapons.members[1].1, p.boot_weapons.members[2].1);
        assert_eq!((kind(staff), kind(shield)), (Some(3), Some(4)));
        let added = p.apply_party_weapons();
        assert!(added.len() >= 2);
        assert_eq!(p.member(DONALD).unwrap().weapons[0], staff);
        assert_eq!(p.member(GOOFY).unwrap().weapons[0], shield);
        assert!(p.apply_party_weapons().is_empty(), "a nonzero word is kept");
        // Sora's record is untouched by the party entry.
        let mut q = Progress::new_game(p.items.clone(), &tb.plrp);
        q.boot_weapons = p.boot_weapons.clone();
        let mut sora = q.sora();
        q.apply_party_weapons();
        sora.weapons = [41, 0];
        assert_eq!(q.sora(), sora, "only Sora's weapon word (41 from the boot table) differs");
    }

    fn disc_boot_weapons() -> Option<BootWeapons> {
        let iso = crate::assets::iso_path().ok().filter(|p| p.exists())?;
        BootWeapons::read(&Disc::open(&iso))
    }

    /// WP-RV3: running `00common.bdx` event 7 on the disc gives the 13 member words (Sora 41, Donald 75, Goofy 49 ...) and the
    /// form words (forms 1 to 4 = 0, 5 = 71, 9 = 42) of docs/bdx.md host 0.75 / 0.76.
    #[test]
    fn the_boot_script_sets_the_member_and_form_weapon_words() {
        let Some(b) = disc_boot_weapons() else { return };
        let ids: Vec<u16> = b.members.iter().map(|m| m.0).collect();
        assert_eq!(ids, [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13]);
        let w0: Vec<u16> = b.members.iter().map(|m| m.1).collect();
        assert_eq!(w0, [41, 75, 49, 81, 54, 55, 72, 62, 59, 60, 61, 74, 73]);
        assert_eq!(b.members[5].2, 80, "member 6 has a second word");
        for f in [1, 2, 3, 4] {
            assert!(b.forms.contains(&(f, 0)), "form {f}");
        }
        assert!(b.forms.contains(&(5, 71)) && b.forms.contains(&(9, 42)), "{:?}", b.forms);
    }

    /// WP-RV3: `apply_party_weapons` writes both words of a member whose first word is 0, the form words where 0, and keeps a
    /// word that is already set (a saved state).
    #[test]
    fn boot_weapons_fill_empty_words_only() {
        let items = Arc::new(ItemIndex::new(&[], &[], &[]));
        let mut p = Progress::from_block(&vec![0; off::LEN], items);
        p.boot_weapons = Arc::new(BootWeapons { members: vec![(2, 75, 0), (3, 49, 0), (6, 55, 80)], forms: vec![(5, 71), (9, 42), (1, 0)] });
        p.set_u16(Progress::member_off(3).unwrap() + rec::WEAPONS, 7);
        let added = p.apply_party_weapons();
        assert_eq!(added.len(), 4);
        assert_eq!(p.member(DONALD).unwrap().weapons, [75, 0]);
        assert_eq!(p.member(GOOFY).unwrap().weapons[0], 7, "a nonzero word is kept");
        assert_eq!(p.member(6).unwrap().weapons, [55, 80]);
        assert_eq!((p.form(5).weapon, p.form(9).weapon, p.form(1).weapon), (71, 42, 0));
        assert!(p.apply_party_weapons().is_empty());
    }

    /// `FUN_001b25d0` step 2: `ceil(lvpm * enmp / 100)`, nothing for 0, times the byte `+0x2b` with the low-HP bonus.
    #[test]
    fn a_defeat_pays_the_rounded_up_percentage_and_the_bonus_multiplies_it() {
        assert_eq!(defeat_payout(100, 7, None), 7);
        assert_eq!(defeat_payout(50, 3, None), 2, "1.5 rounds up");
        assert_eq!(defeat_payout(100, 0, None), 0);
        assert_eq!(defeat_payout(0, 9, Some(3)), 0, "nothing to multiply");
        assert_eq!(defeat_payout(100, 7, Some(1)), 7, "no Experience Boost: the byte is 1");
        assert_eq!(defeat_payout(100, 7, Some(3)), 21);
    }

    /// `FUN_001b50c0`: the member's own table, the first one for a missing or empty table, the level held at the
    /// table's length (row `level - 1`), ids 14 and 15 mapped to 1 and 6.
    #[test]
    fn lvup_rows_follow_the_member_table_and_hold_at_its_end() {
        let row = |exp: i32| t::Lvup { exp, strength: 0, magic: 0, defense: 0, ap: 0, sword_ability: 0, shield_ability: 0, staff_ability: 0, padding: 0 };
        let tables = vec![vec![row(10), row(20)], vec![row(5)], Vec::new()];
        assert_eq!(lvup_row(&tables, 1, 1).map(|r| r.exp), Some(10));
        assert_eq!(lvup_row(&tables, 1, 2).map(|r| r.exp), Some(20));
        assert_eq!(lvup_row(&tables, 1, 9).map(|r| r.exp), Some(20), "held at the table's length");
        assert_eq!(lvup_row(&tables, 1, 0).map(|r| r.exp), Some(10), "level 0 counts as 1");
        assert_eq!(lvup_row(&tables, 2, 1).map(|r| r.exp), Some(5));
        assert_eq!(lvup_row(&tables, 3, 2).map(|r| r.exp), Some(20), "an empty table falls back to the first");
        assert_eq!(lvup_row(&tables, 14, 1).map(|r| r.exp), Some(10), "14 is Sora's table");
        assert_eq!(lvup_row(&[], 1, 1), None);
    }

    /// `FUN_001b51e0` + `FUN_001b52f8` on the disc's `lvup` rows: a sum below Sora's threshold keeps his level and
    /// leaves the difference; reaching it raises one level; a large sum raises him through every threshold it
    /// covers, one `LevelReached` per level, each ability of the weapon-set column in his list, the answer
    /// is what the next row still asks. Skips without a disc.
    #[test]
    fn experience_raises_sora_through_the_lvup_thresholds() {
        let Some(mut tb) = tables() else { return };
        let ix = index(&tb);
        let lvup = kh2_tables::subfile(&mut tb.archive, &t::LVUP).ok().and_then(|b| kh2_tables::rt::decode_lvup::<t::Lvup>(&b)).expect("lvup");
        let mut p = Progress::new_game(ix, &tb.plrp);
        let start = p.member(SORA).expect("Sora").level;
        let rows = &lvup[0];
        assert!(start >= 1 && usize::from(start) < rows.len());
        let need = p.experience_to_next(SORA, &lvup);
        assert_eq!(need, rows[usize::from(start) - 1].exp, "nothing earned yet: the whole threshold");
        let paid = p.pay_experience(need - 1, &lvup, SORA);
        assert_eq!(p.member(SORA).unwrap().level, start, "one short of the threshold");
        assert_eq!(paid.to_next, 1);
        let paid = p.pay_experience(1, &lvup, SORA);
        assert_eq!(p.member(SORA).unwrap().level, start + 1);
        let sora: Vec<u8> = paid.levels.iter().filter(|l| l.member == SORA).map(|l| l.level).collect();
        assert_eq!(sora, vec![start + 1]);
        assert_eq!(paid.to_next, rows[usize::from(start)].exp - need);
        // A large sum: the level is the first one whose threshold is above the total.
        let total = rows[40].exp;
        let paid = p.pay_experience(total - p.experience(), &lvup, SORA);
        let mut expect = 1usize;
        while expect < 99 && rows[expect - 1].exp <= total {
            expect += 1;
        }
        assert_eq!(usize::from(p.member(SORA).unwrap().level), expect);
        let levels: Vec<u8> = paid.levels.iter().filter(|l| l.member == SORA).map(|l| l.level).collect();
        assert_eq!(levels, (start + 2..=expect as u8).collect::<Vec<_>>(), "one entry per level, in order");
        let set = p.member(SORA).unwrap().weapon_set;
        let column = |r: &t::Lvup| [r.sword_ability, r.shield_ability, r.staff_ability].get(usize::from(set)).copied().unwrap_or(0);
        let abilities = p.member(SORA).unwrap().abilities;
        for l in paid.levels.iter().filter(|l| l.member == SORA) {
            assert_eq!(l.ability, column(&rows[usize::from(l.level) - 1]), "level {}", l.level);
            if l.given {
                assert!(abilities.iter().any(|(id, _)| *id == l.ability), "ability {:#x} of level {} is in the list", l.ability, l.level);
            }
        }
        assert_eq!(paid.to_next, rows[expect - 1].exp - p.experience());
    }

    #[test]
    fn counts_experience_and_spell_rows_follow_the_rules() {
        let Some(tb) = tables() else { return };
        let ix = index(&tb);
        let mut p = Progress::new_game(ix.clone(), &tb.plrp);
        // Experience: capped at 9,999,999 (FUN_001b51e0).
        p.add_experience(9_999_000);
        p.add_experience(5_000);
        assert_eq!(p.experience(), MAX_EXPERIENCE);
        p.add_experience(-999);
        assert_eq!(p.experience(), MAX_EXPERIENCE - 999);
        // Counts stop at 99 (FUN_001a1768), use floors at 0 (FUN_001a1c30).
        let potion = ix.rows.iter().find(|r| r.kind == item_type::CONSUMABLE && !r.is_key()).unwrap().id;
        let before = p.item_count(potion);
        assert_eq!(p.add_item(potion, 120), 99 - before);
        assert_eq!(p.item_count(potion), 99);
        p.use_item(potion, 200);
        assert_eq!(p.item_count(potion), 0);
        // A spell's level is its item's count; the magc row follows it (FUN_001a4c78).
        for slot in 0..6u8 {
            let item = ix.magic_items[slot as usize];
            assert_ne!(item, 0, "every magic slot has an item");
            for level in 0..=3u8 {
                p.set_count(item, level);
                assert_eq!(p.spell_level(slot), level);
                let row = crate::magic_rules::pick_row(&tb.magc, slot, p.spell_level(slot), 0);
                match level {
                    0 => assert!(row.is_none(), "slot {slot}: not owned, no row"),
                    l => {
                        let r = row.expect("a row");
                        assert!(r.level <= l && r.id == slot, "slot {slot} level {l}: row {} level {}", r.file_name, r.level);
                    }
                }
            }
        }
    }

    /// The oracle: the game's own `FUN_00187b38` and `FUN_001c0cb0(0)` run in kh2-ee on the user's
    /// executable build the party block; it must equal the port byte for byte. Then the game's
    /// `FUN_001a1cc8` (item count) and `FUN_001a4c78` (spell row) answer as `Progress` does, and
    /// `FUN_001a18d8` / `FUN_001a1c30` (give / take) change the block as the port does over a
    /// sweep of items. Item 0x20 and spell items are left out of the give sweep: their side
    /// calls (`FUN_001803b0`, `FUN_00180450`) act on state outside this block.
    #[test]
    fn new_game_block_matches_the_game() {
        use kh2_ee::Ee;
        const STACK: u32 = 0x01ff_0000;
        const ITEM_PTR: u32 = 0x01c9_5588;
        const PLRP_PTR: u32 = 0x01d5_df5c;
        const CMD_PTR: u32 = 0x01d5_d798;
        const MAGC_PTR: u32 = 0x01c9_5520;
        const WORLD: u32 = 0x0033_e830;
        let Some(mut tb) = tables() else { return };
        let ix = index(&tb);
        let elf = ["SLUS_210.05", "SLUS_210.05;1"].iter().find_map(|p| tb.archive.iso().read(p).ok()).expect("executable");
        let blob = |a: &mut kh2_archive::Archive, d: &kh2_tables::TableDef| kh2_tables::subfile(a, d).expect("table");
        let tables = [
            (ITEM_PTR, 0x0180_0000, blob(&mut tb.archive, &t::ITEM)),
            (PLRP_PTR, 0x0181_0000, blob(&mut tb.archive, &t::PLRP)),
            (CMD_PTR, 0x0182_0000, blob(&mut tb.archive, &t::CMD)),
            (MAGC_PTR, 0x0183_0000, blob(&mut tb.archive, &t::MAGC)),
        ];
        let mut ee = Ee::new();
        ee.load_elf(&elf).expect("elf");
        for (ptr, at, data) in &tables {
            ee.write(*at, data);
            ee.w32(*ptr, *at);
        }
        let party = addr::PARTY;
        ee.write(addr::SAVE, &vec![0u8; addr::SAVE_LEN as usize]);
        ee.call(0x0018_7b38, &[party as u64], &[], STACK).expect("FUN_00187b38");
        ee.call(0x001c_0cb0, &[0], &[], STACK).expect("FUN_001c0cb0");
        let read = |ee: &mut Ee| (0..off::LEN as u32).map(|o| ee.r8(party + o)).collect::<Vec<u8>>();
        let game = read(&mut ee);
        let mut port = Progress::new_game(ix.clone(), &tb.plrp);
        let diff = |a: &[u8], b: &[u8]| a.iter().zip(b).enumerate().filter(|(_, (x, y))| x != y).map(|(o, (x, y))| format!("+{o:#x}: game {x:#x} port {y:#x}")).take(12).collect::<Vec<_>>();
        assert_eq!(diff(&game, port.block()), Vec::<String>::new(), "new-game party block");

        // The decoded fields of the game's block.
        let g = Progress::from_block(&game, ix.clone());
        let sora_row = tb.plrp.iter().find(|r| r.id == 0 && r.character == 1).expect("Sora's new-game row");
        let s = g.sora();
        assert_eq!((s.hp, s.hp_max, s.mp, s.mp_max), (sora_row.hp, sora_row.hp, sora_row.mp, sora_row.mp));
        assert_eq!(s.items.len(), sora_row.item_slot_max as usize);
        assert_eq!((g.experience(), g.summon(), g.summon_level(), g.summon_exp(), g.spell_switch_word()), (0, 0, 1, 0, 0));
        assert!((1..=5).all(|f| g.form_level(f) == 1 && g.form(f).weapon == 0));
        assert!((0..off::WORLD_COUNT).all(|w| g.outfit(w) == 0));
        for r in &ix.rows {
            let n = ee.call(0x001a_1cc8, &[r.id as u64], &[], STACK).expect("FUN_001a1cc8") as u8;
            assert_eq!(g.item_count(r.id), n, "item {} count", r.id);
        }

        // Give and take a sweep of items in both and compare the blocks again.
        let world = 2u8;
        ee.write(WORLD, &[world]);
        let mut gives = 0;
        for (n, r) in ix.rows.iter().enumerate().filter(|(_, r)| r.id != 0x20 && r.kind != item_type::MAGIC && r.id != 0) {
            let (to, eq) = match n % 3 {
                0 => (INVENTORY, false),
                1 => (SORA, true),
                _ => (2, false),
            };
            let reps = 1 + n % 2;
            for _ in 0..reps {
                let ok = ee.call(0x001a_18d8, &[r.id as u64, to as u64, eq as u64], &[], STACK).expect("FUN_001a18d8") as u32 != 0;
                assert_eq!(port.give(r.id, to, eq), ok, "give item {} to {to}", r.id);
                gives += ok as u32;
            }
            if n % 5 == 0 {
                ee.call(0x001a_1c30, &[r.id as u64, 1], &[], STACK).expect("FUN_001a1c30");
                port.use_item(r.id, 1);
            }
        }
        // Spells through the counts: FUN_001a1a18 (no FUN_00180450 when the count was not 0) is
        // avoided by writing the counts the same way into both.
        let magc_base = 0x0183_0008u32;
        for slot in 0..6u8 {
            let item = ix.magic_items[slot as usize];
            let it = *ix.get(item).unwrap();
            for level in 0..=3u8 {
                ee.write(party + (off::COUNTS + it.slot as usize) as u32, &[level]);
                port.set_count(item, level);
                let ptr = ee.call(0x001a_4c78, &[slot as u64], &[], STACK).expect("FUN_001a4c78") as u32;
                let game_row = (ptr != 0).then(|| ((ptr - magc_base) / 0x38) as usize);
                let port_row = crate::magic_rules::pick_row(&tb.magc, slot, port.spell_level(slot), world).map(|r| tb.magc.iter().position(|x| std::ptr::eq(x, r)).unwrap());
                assert_eq!(game_row, port_row, "spell slot {slot} level {level}");
            }
        }
        let game = read(&mut ee);
        assert_eq!(diff(&game, port.block()), Vec::<String>::new(), "party block after the give/take sweep");

        // The field item use (WP-7C): FUN_001a1ba8 (take from a member's slots or the stock) and
        // FUN_001877c8 (refill reserved slots from the stock), on Sora with three battle item slots.
        let sora = Progress::member_off(SORA).unwrap();
        ee.write(party + (sora + rec::ITEM_SLOTS) as u32, &[3]);
        port.set_u8(sora + rec::ITEM_SLOTS, 3);
        for i in 0..3 {
            ee.write(party + (sora + rec::ITEMS + 2 * i) as u32, &[0, 0]);
            port.set_u16(sora + rec::ITEMS + 2 * i, 0);
        }
        let consumables: Vec<u16> = ix.rows.iter().filter(|r| r.kind == item_type::CONSUMABLE && !r.is_key() && r.id != 0).map(|r| r.id).take(2).collect();
        let (potion, other) = (consumables[0], consumables[1]);
        for (id, to) in [(potion, SORA), (other, SORA), (potion, SORA), (potion, INVENTORY), (potion, INVENTORY)] {
            let ok = ee.call(0x001a_18d8, &[id as u64, to as u64, 0], &[], STACK).expect("FUN_001a18d8") as u32 != 0;
            assert_eq!(port.give(id, to, false), ok, "give {id} to {to}");
        }
        assert_eq!(diff(&read(&mut ee), port.block()), Vec::<String>::new(), "after the gives to Sora's slots");
        let mut takes = Vec::new();
        for (id, to) in [(potion, SORA), (potion, SORA), (potion, SORA), (other, SORA), (potion, INVENTORY)] {
            let ok = ee.call(0x001a_1ba8, &[id as u64, to as u64], &[], STACK).expect("FUN_001a1ba8") as u32 != 0;
            assert_eq!(port.take_item(id, to), ok, "take {id} from {to}");
            takes.push(ok);
            assert_eq!(diff(&read(&mut ee), port.block()), Vec::<String>::new(), "after taking {id} from {to}");
        }
        // Reserve the potion in slots 0 and 2 and refill from the stock (one potion is left there).
        for i in [0usize, 2] {
            ee.write(party + (sora + rec::RESERVED + 2 * i) as u32, &potion.to_le_bytes());
            port.set_u16(sora + rec::RESERVED + 2 * i, potion);
        }
        ee.call(0x0018_77c8, &[(party + sora as u32) as u64], &[], STACK).expect("FUN_001877c8");
        port.refill_battle_items(SORA);
        assert_eq!(diff(&read(&mut ee), port.block()), Vec::<String>::new(), "after the refill");
        eprintln!("oracle: battle item take/refill equal (takes {takes:?}; Sora's slots {:?}; stock {})", port.battle_items(SORA), port.item_count(potion));
        let inv = port.inventory().len();
        eprintln!(
            "oracle: new-game party block equal ({} bytes, {} plrp rows with Id 0); {} item counts equal; \
             give/take sweep equal ({gives} gives accepted, {inv} items owned after); 24 spell rows equal",
            off::LEN,
            tb.plrp.iter().filter(|r| r.id == 0).count(),
            ix.rows.len(),
        );
    }
}
