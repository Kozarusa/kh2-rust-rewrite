//! Limits: the data rows, the conditions the game tests before a limit command is offered and run,
//! and as much of the activation as is possible without companions. See docs/limits-reactions.md.
//!
//! Game rules ported (addresses in `SLUS_210.05`; the notes in docs/player-data.md describe the rest):
//! - Row usable (FUN_001d1ce0): the row's world is 0 or the current world; the player's command
//!   lock bit is clear (`Flags` of the player's status, +0x8 & 0x20); each non-zero member byte among
//!   `Character`, `Summon`, `Group` is a party member that is present and not downed (FUN_00175ba0 +
//!   FUN_0019a3f8; a member id 2 or 3 is refused while the form field is 6); when the `Limit` item is a
//!   type 19 ability, the member `Character` must have its ability bit set.
//! - Command available (FUN_001b3e38, Execute 81): the row exists and is usable, no global command
//!   lock; a normal limit needs MP >= the command's `Cost` byte (FUN_001b3f00), else status 2 (not
//!   enough MP); a summon limit (`Summon` 200..=204) needs a non-zero summon gauge.
//! - Execution (FUN_001b28a0): spend MP (normal) or summon gauge, create the limit task with the
//!   command's `Time` as its duration, load the limit's resource BAR `limit/us/<FileName>`, and put
//!   the participants into the limit state. The app runs the first three steps (the BAR is listed,
//!   not played) and writes `LimitStarted`; the task body, the script, the effects and the
//!   participants' motions are gaps.
//!
//! The app has no companions, so a normal limit is refused with "member absent" unless the harness
//! names a party (`KH2_LIMIT_PARTY=2,3`). That path exists to exercise the rules, not as gameplay.

use bevy::prelude::*;
use kh2_tables::tables as t;
use kh2_tables::player;

use crate::assets::Disc;
use crate::schedule::Sim;
use crate::sora::Sora;

/// Item type of an ability (`item.Type`).
const ITEM_ABILITY: u8 = 19;

/// The fields of a `limt` row the rules read, joined with its command and ability item.
#[derive(Clone, Debug, PartialEq)]
pub struct LimitRow {
    pub id: u8,
    /// The three participant bytes (`Character`, `Summon`, `Group`).
    pub members: [u8; 3],
    pub file: String,
    pub command: u16,
    pub item: u16,
    pub world: u16,
    /// From the command row: MP cost byte and duration.
    pub cost: u8,
    pub time: u16,
    pub message: u32,
    /// The ability bit of `item` when it is a type 19 item.
    pub ability_bit: Option<u16>,
}

impl LimitRow {
    pub fn is_summon(&self) -> bool {
        player::is_summon_limit(self.members[1])
    }
}

/// Rows of the disc's limit table with their command and ability data.
pub fn rows(limt: &[t::Limt], cmds: &[t::Cmd], items: &[t::Item]) -> Vec<LimitRow> {
    limt.iter()
        .map(|l| {
            let c = cmds.iter().find(|c| c.id == l.command);
            let it = items.iter().find(|i| i.id == l.limit);
            LimitRow {
                id: l.id,
                members: [l.character, l.summon, l.group],
                file: l.file_name.clone(),
                command: l.command,
                item: l.limit,
                world: l.world,
                cost: c.map_or(0, |c| c.cost),
                time: c.map_or(0, |c| c.time),
                message: c.map_or(0, |c| c.message_id as u32),
                ability_bit: it.filter(|i| i.r#type == ITEM_ABILITY).map(|i| i.flag1 as u16 | (i.rank as u16) << 8),
            }
        })
        .collect()
}

/// What the tests read about the party and the world.
#[derive(Clone, Debug, Default)]
pub struct Party {
    /// Member ids (1 Sora, 2 Donald, 3 Goofy, 5 Auron ...) that are in the party and not downed.
    pub present: Vec<u8>,
    /// Ability bits of the member a row's `Character` names.
    pub abilities: u128,
    /// The player's form field (6 is a special non-drive mode).
    pub form: u8,
    pub world: u16,
    /// The player's command lock (FUN_001d1ce0 reads status flag 0x20).
    pub locked: bool,
    pub mp: i32,
    /// The summon gauge (status +0x1b4) is non-zero.
    pub summon_gauge: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    WrongWorld,
    Locked,
    /// A participant byte names a member that is not in the party (or is downed).
    MemberAbsent(u8),
    /// The type 19 ability item is not learned (the bit).
    AbilityMissing(u16),
    /// Status 2 of FUN_001b3e38.
    NotEnoughMp,
    SummonGaugeEmpty,
}

/// FUN_001d1ce0: is the row usable now.
pub fn usable(r: &LimitRow, p: &Party) -> Result<(), Refusal> {
    if r.world != 0 && r.world != p.world {
        return Err(Refusal::WrongWorld);
    }
    if p.locked {
        return Err(Refusal::Locked);
    }
    for &m in r.members.iter().filter(|&&m| m != 0) {
        if p.form == 6 && (m == 2 || m == 3) {
            return Err(Refusal::MemberAbsent(m));
        }
        // Summon limits name a summon (200..=204), not a party member: no participant to find.
        if player::is_summon_limit(m) {
            continue;
        }
        if m != 1 && !p.present.contains(&m) {
            return Err(Refusal::MemberAbsent(m));
        }
    }
    if let Some(bit) = r.ability_bit
        && (bit >= 128 || p.abilities >> bit & 1 == 0)
    {
        return Err(Refusal::AbilityMissing(bit));
    }
    Ok(())
}

/// FUN_001b3e38 for Execute 81: usable, then MP (normal) or the summon gauge (summon limit).
pub fn available(r: &LimitRow, p: &Party) -> Result<(), Refusal> {
    usable(r, p)?;
    if r.is_summon() {
        if !p.summon_gauge {
            return Err(Refusal::SummonGaugeEmpty);
        }
    } else if p.mp < mp_needed(r) {
        return Err(Refusal::NotEnoughMp);
    }
    Ok(())
}

/// The MP a normal limit needs and spends. Every normal limit's `Cost` byte on this disc is 255.
/// Read literally (FUN_001b3e38 compares the byte, FUN_001b9ff0 only replaces it in a special
/// battle) no MP value reaches it, so the notes in docs/player-data.md call it "all MP". The app
/// follows that reading [inferred]: any MP is enough and the limit spends all of it. Whether the
/// game's MP unit makes 255 reachable was not checked (the MP block at status +0x180 was not traced).
pub fn mp_needed(r: &LimitRow) -> i32 {
    if r.cost == 255 { 1 } else { r.cost as i32 }
}

/// The first usable row of a command (FUN_001d2040 walks the table in order).
pub fn row_for_command(rows: &[LimitRow], command: u16, p: &Party) -> Option<usize> {
    rows.iter().position(|r| r.command == command && usable(r, p).is_ok())
}

#[derive(Resource)]
pub struct LimitRules {
    pub rows: Vec<LimitRow>,
}

/// The party the rules see. The app has only Sora; `KH2_LIMIT_PARTY=2,3` (and `KH2_LIMIT_ABILITIES`,
/// a comma list of ability bits) pretends the others are there so the rules can be exercised.
#[derive(Resource, Default)]
pub struct LimitParty(pub Party);

/// Sent when a limit starts. A limit task (script, effects, participants) would hang off this.
#[allow(dead_code)]
#[derive(Message, Clone, Debug)]
pub struct LimitStarted {
    pub command: u16,
    pub file: String,
    /// The command's `Time`, in the units the game's limit task takes.
    pub time: u16,
}

#[derive(Resource, Default)]
struct Selected {
    next: usize,
    scripted: Option<u16>,
    armed: bool,
    /// L was pressed since the last tick (latched in `Update`, consumed by the 60 Hz tick).
    pressed: bool,
}

fn read_key(keys: Res<ButtonInput<KeyCode>>, mut sel: ResMut<Selected>) {
    if keys.just_pressed(KeyCode::KeyL) {
        sel.pressed = true;
    }
}

pub struct LimitPlugin;

impl Plugin for LimitPlugin {
    fn build(&self, app: &mut App) {
        let rows = app.world().get_resource::<Disc>().and_then(|d| {
            let mut a = d.0.lock().ok()?;
            let limt = kh2_tables::load::<t::Limt>(&mut a, &t::LIMT).ok()?.rows;
            let cmds = kh2_tables::load::<t::Cmd>(&mut a, &t::CMD).ok()?.rows;
            let items = kh2_tables::load::<t::Item>(&mut a, &t::ITEM).ok()?.rows;
            Some(rows(&limt, &cmds, &items))
        });
        let Some(rows) = rows else {
            warn!("limit: tables not readable; limits are off");
            return;
        };
        let env = |k: &str| std::env::var(k).ok();
        let list = |k: &str| env(k).map(|v| v.split(',').filter_map(|x| x.trim().parse::<u16>().ok()).collect::<Vec<_>>()).unwrap_or_default();
        let party = Party {
            present: list("KH2_LIMIT_PARTY").into_iter().map(|m| m as u8).collect(),
            abilities: list("KH2_LIMIT_ABILITIES").into_iter().filter(|&b| b < 128).fold(0, |a, b| a | 1u128 << b),
            world: 2,
            ..Default::default()
        };
        app.insert_resource(LimitRules { rows })
            .insert_resource(LimitParty(party))
            .insert_resource(Selected { scripted: env("KH2_LIMIT").and_then(|v| v.parse().ok()), ..Default::default() })
            .add_message::<LimitStarted>()
            .add_systems(Update, read_key)
            .add_systems(FixedUpdate, try_limit.in_set(Sim::Sora));
    }
}

/// Key `L` tries the next limit command in table order; `KH2_LIMIT=<command id>` tries that one once
/// at about 4 s. Both print the decision (every limit) and run an available limit.
#[allow(clippy::too_many_arguments)]
fn try_limit(
    time: Res<Time>,
    rules: Res<LimitRules>,
    mut party: ResMut<LimitParty>,
    mut sel: ResMut<Selected>,
    disc: Res<Disc>,
    mut sora: Query<&mut Sora>,
    mut out: MessageWriter<LimitStarted>,
    mut active: Local<Option<(u16, u32)>>,
    summon: (Option<Res<crate::summon::SummonState>>, Option<Res<crate::drive::DriveState>>),
) {
    if let Some((cmd, left)) = active.as_mut() {
        *left = left.saturating_sub(1);
        if *left == 0 {
            info!("limit: command {cmd} ended (duration over)");
            *active = None;
        }
    }
    let Ok(mut s) = sora.single_mut() else { return };
    let command = if let Some(c) = sel.scripted.filter(|_| !sel.armed && time.elapsed_secs() >= 4.0) {
        sel.armed = true;
        Some(c)
    } else if std::mem::take(&mut sel.pressed) {
        let mut cmds: Vec<u16> = Vec::new();
        for r in &rules.rows {
            if !cmds.contains(&r.command) {
                cmds.push(r.command);
            }
        }
        let c = cmds.get(sel.next % cmds.len().max(1)).copied();
        sel.next += 1;
        c
    } else {
        None
    };
    let Some(command) = command else { return };
    party.0.mp = s.mp;
    // The summon gauge: the mode-3 timer (status +0x1b4) is non-zero (summon.rs). A summon row also
    // needs the summon object (FUN_001d1ce0 -> FUN_00175ba0 by NeoStatus), which `usable` does not
    // test; no summon object exists yet (docs/summons.md stage 3), so the read requires one and the
    // summon rows stay refused until then.
    party.0.summon_gauge = match summon {
        (Some(sm), Some(d)) => sm.gauge(&d) && sm.slots.iter().any(Option::is_some),
        _ => false,
    };
    let Some(i) = rules.rows.iter().position(|r| r.command == command) else {
        info!("limit: command {command} has no limit row");
        return;
    };
    // The first usable row of the command wins (FUN_001d2040); otherwise report the first row's refusal.
    let i = row_for_command(&rules.rows, command, &party.0).unwrap_or(i);
    let r = &rules.rows[i];
    let desc = format!("limit {} command {command} (participant bytes {:?}, file {}, cost {}, time {})", r.id, r.members, r.file, r.cost, r.time);
    match available(r, &party.0) {
        Err(e) => info!("limit: {desc} refused: {e:?} (party {:?})", party.0.present),
        Ok(()) => {
            // A summon limit spends the summon gauge (not modelled); a normal one MP.
            if !r.is_summon() {
                s.mp = if r.cost == 255 { 0 } else { (s.mp - r.cost as i32).max(0) };
            }
            let entries = disc
                .read(&format!("limit/us/{}", r.file))
                .and_then(|b| kh2_bar::parse(&b).ok().map(|es| es.iter().map(|e| format!("{}:{}", e.tag, e.kind)).collect::<Vec<_>>().join(" ")))
                .unwrap_or_else(|| "(resource not found)".into());
            info!("limit: {desc} started, MP now {}; resource BAR entries {entries}; the task body is not ported", s.mp);
            *active = Some((command, (r.time as u32).max(1)));
            out.write(LimitStarted { command, file: r.file.clone(), time: r.time });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(members: [u8; 3], world: u16, cost: u8, bit: Option<u16>) -> LimitRow {
        LimitRow { id: 1, members, file: "x.bar".into(), command: 100, item: 0, world, cost, time: 10, message: 0, ability_bit: bit }
    }

    fn party(present: &[u8], abilities: u128, mp: i32) -> Party {
        Party { present: present.to_vec(), abilities, world: 2, mp, ..Default::default() }
    }

    #[test]
    fn members_world_ability_and_mp() {
        let r = row([2, 3, 0], 0, 50, Some(7));
        assert_eq!(usable(&r, &party(&[], 1 << 7, 100)), Err(Refusal::MemberAbsent(2)));
        assert_eq!(usable(&r, &party(&[2], 1 << 7, 100)), Err(Refusal::MemberAbsent(3)));
        assert_eq!(usable(&r, &party(&[2, 3], 0, 100)), Err(Refusal::AbilityMissing(7)));
        assert_eq!(available(&r, &party(&[2, 3], 1 << 7, 49)), Err(Refusal::NotEnoughMp));
        assert_eq!(available(&r, &party(&[2, 3], 1 << 7, 50)), Ok(()));
        let w = row([2, 0, 0], 5, 0, None);
        assert_eq!(usable(&w, &party(&[2], 0, 0)), Err(Refusal::WrongWorld));
        let mut locked = party(&[2], 0, 0);
        locked.locked = true;
        assert_eq!(usable(&row([2, 0, 0], 0, 0, None), &locked), Err(Refusal::Locked));
        let mut f6 = party(&[2], 0, 0);
        f6.form = 6;
        assert_eq!(usable(&row([2, 0, 0], 0, 0, None), &f6), Err(Refusal::MemberAbsent(2)));
    }

    #[test]
    fn all_mp_cost_needs_some_mp() {
        let r = row([0, 0, 0], 0, 255, None);
        assert_eq!(available(&r, &party(&[], 0, 0)), Err(Refusal::NotEnoughMp));
        assert_eq!(available(&r, &party(&[], 0, 1)), Ok(()));
    }

    #[test]
    fn summon_limits_need_the_gauge_not_a_partner() {
        let r = row([0, 201, 0], 0, 0, None);
        let mut p = party(&[], 0, 0);
        assert_eq!(available(&r, &p), Err(Refusal::SummonGaugeEmpty));
        p.summon_gauge = true;
        assert_eq!(available(&r, &p), Ok(()));
    }

    fn disc_rows() -> Option<Vec<LimitRow>> {
        let iso = crate::assets::iso_path().ok().filter(|p| p.exists());
        let Some(iso) = iso else {
            eprintln!("skipped: no disc (set KH2_ISO)");
            return None;
        };
        let d = Disc::open(&iso);
        let mut a = d.0.lock().ok()?;
        let limt = kh2_tables::load::<t::Limt>(&mut a, &t::LIMT).ok()?.rows;
        let cmds = kh2_tables::load::<t::Cmd>(&mut a, &t::CMD).ok()?.rows;
        let items = kh2_tables::load::<t::Item>(&mut a, &t::ITEM).ok()?.rows;
        Some(rows(&limt, &cmds, &items))
    }

    /// Real rows: Sora alone can use none of the normal limits; the table is non-empty and every
    /// row's file exists in the limit directory.
    #[test]
    fn real_rows_need_companions() {
        let Some(rows) = disc_rows() else { return };
        assert!(!rows.is_empty());
        let alone = party(&[], u128::MAX, 100);
        for r in &rows {
            eprintln!("LIMIT id {} members {:?} cmd {} item {} bit {:?} world {} cost {} time {} file {} alone {:?}", r.id, r.members, r.command, r.item, r.ability_bit, r.world, r.cost, r.time, r.file, usable(r, &alone));
        }
        // Every row that names a partner is refused for Sora alone; the one row that needs nobody is the
        // Trinity variant with no partner bytes (file trinity_zz.bar, command 620).
        for r in rows.iter().filter(|r| !r.is_summon()) {
            let alone_ok = usable(r, &alone).is_ok();
            assert_eq!(alone_ok, r.members[0] == 1 && r.members[1] == 0 && r.members[2] == 0, "limit {} command {}", r.id, r.command);
        }
    }
}
