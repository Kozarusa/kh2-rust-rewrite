//! The field command menu as the original builds and drives it, and the item command.
//!
//! The player's command manager (an object whose methods are the vtable at 0x35f260) keeps 18
//! command lists of 0x74 bytes (list `i` at manager `+0x24 + 0x74 i`; `+8` the count, `+0xc` up to
//! 8 entries of 12 bytes, `+0x6c` the cursor, `+0x70` the cursor's command) and a stack of the open
//! ones (`+0x974`, top pointer `+0x84c`; the HUD at 0x13ca30 draws one page per stacked list).
//! Method `+0xc` (FUN_001bc2b8) rebuilds the lists, method `+0x10` (FUN_001bbfd8) reads the pad and
//! method `+0x14` (FUN_001bc0c8) issues the selected command. Everything below cites the routine it
//! ports; docs/systems/ui/items.md has the whole reading.
//!
//! The item command: Potion and the other consumables sit in Sora's battle item slots (party record
//! `+0x34`, `progress::Progress::battle_items`); the Items list shows them merged by command with a
//! count. Using one asks for the target (list 4), then the player plays motion request 9
//! (FUN_00175e20); frame trigger 6 of that motion (FUN_00188498 case 6 -> FUN_001b8710) takes the
//! item out of the user's slot (FUN_001a1ba8) and queues the use, which starts system effect 0 at
//! the user, waits 30 frames and then starts the item's effect (`item.StatEntry`) on the target and
//! applies FUN_001a12a0 (HP, MP, drive gauge).
//!
//! Keys (the app's pad): T cursor up, G cursor down, J confirm (the attack button), B cancel. The
//! original's Left/Right on the list (go back / open the sub-menu) and the L1 shortcut list have no
//! key in the app (the arrows turn the camera). `KH2_KEYS` drives T, G and B (magic.rs maps them).
//! `KH2_ITEMLOG=1` logs every list change.

use bevy::prelude::*;
use kh2_anim::triggers::range;
use kh2_anim::PlayerState;
use kh2_tables::tables::{self as t, Cmd};

use crate::assets::Disc;
use crate::fx::{self, Bind, Placement};
use crate::hud::hud_menu::{CommandMenu, MenuEntry, MenuPage};
use crate::magic::{MagicData, MagicState};
use crate::magic_rules as mrules;
use crate::progress::{Progress, SORA};
use crate::schedule::Sim;
use crate::skel::Animator;
use crate::sora::{Battle, Pad, Sora, State};

// ------------------------------------------------------------------------------------------------
// The command lists (pure)
// ------------------------------------------------------------------------------------------------

/// Lists the manager holds (FUN_001baf48 builds 18).
pub const LISTS: usize = 18;
/// The list indices the builders fill (the list's own `+0` is its index: FUN_001baf48).
pub mod list {
    /// The commands (FUN_001bc578).
    pub const ROOT: usize = 0;
    /// Magic (FUN_001bc870 from the player's spell list +0x9d4).
    pub const MAGIC: usize = 1;
    /// Items (FUN_001bc910).
    pub const ITEMS: usize = 2;
    /// Drive forms (FUN_001bca70).
    pub const FORMS: usize = 3;
    /// The targets of an item (FUN_001bcb98: the party members).
    pub const TARGETS: usize = 4;
    /// Shortcuts (FUN_001bcc78), the root while L1 is held (FUN_001bc370).
    pub const SHORTCUTS: usize = 5;
    /// The second command deck (FUN_001bc6e0).
    pub const ROOT2: usize = 6;
    /// Summons (FUN_001bc8b0).
    pub const SUMMONS: usize = 7;
}

/// Command ids the builders put in (immediates of the code, not data).
pub mod cmd_id {
    pub const ATTACK: u16 = 1;
    pub const MAGIC: u16 = crate::magic_rules::EXECUTE_MAGIC;
    pub const ITEMS: u16 = 3;
    pub const DRIVE: u16 = 4;
    pub const REVERT: u16 = 5;
    pub const SUMMON: u16 = 0x48;
    pub const PARTY: u16 = 0x49;
    pub const LIMIT: u16 = 0x51;
    /// The empty bar (its `Execute` is 0, status 5).
    pub const EMPTY: u16 = 0x68;
    /// Attack in world 9 (FUN_001bc4c8).
    pub const ATTACK_WORLD9: u16 = 0x88;
    /// Wisdom form's attack (FUN_001bc4c8: player form 2).
    pub const ATTACK_WISDOM: u16 = 0x16d;
    /// Attack while MP charges with ability bit 107 (FUN_001bc4c8 -> FUN_001768b8).
    pub const ATTACK_CHARGING: u16 = 0x21d;
    /// Dismiss (FUN_001bc6e0 while the summon state `DAT_01d5e368` is set; docs/systems/drive-limits-summons/contract.md).
    pub const DISMISS: u16 = 0x13c;
    /// "Drive!" (command 605 = 0x25d, `Execute` 605, `SubMenu` 3): the second deck's third entry while Genie is out
    /// (FUN_001bc6e0 after FUN_001bcf10).
    pub const GENIE_DRIVE: u16 = 0x25d;
    /// The commands of Genie's form list (`FUN_001c6b10`: the table at 0x37c036, indices 1..=4): `Execute` equals the id, no
    /// sub-menu; FUN_00177c20 -> FUN_0016b420 raises event 11 on Genie's script (summon.rs).
    pub const GENIE_FORMS: std::ops::RangeInclusive<u16> = 535..=538;
}

/// Statuses (FUN_001ba2c0 and the overrides of FUN_001ba748): below 4 the command can be chosen
/// (FUN_00178a78). The HUD draws the name with the colour of the same number (0x355600).
pub mod status {
    pub const READY: u8 = 0;
    /// A sub-menu whose list has no usable entry (FUN_001ba748 with FUN_001ba910).
    pub const NONE_USABLE: u8 = 1;
    // 2: a spell with too little MP (it still casts and starts the charge), from magic_rules.
    pub const UNAVAILABLE: u8 = 5;
    /// An item entry whose slots only reserve the item (count 0, FUN_001ba748 / FUN_001bc910).
    pub const RESERVED: u8 = 6;
}

/// One list entry (12 bytes at list `+0xc + 12 i`, FUN_001ba670).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Entry {
    /// `+0`: bit 0 the entry came from a command slot (FUN_001ba748).
    pub flags: u16,
    /// `+2`: the command id.
    pub cmd: u16,
    /// `+4`: the count (items: how many battle slots carry it; 1 elsewhere).
    pub count: u8,
    /// `+5`: the status.
    pub status: u8,
}

/// One list: entries, cursor (`+0x6c`) and the cursor's command (`+0x70`, -1 = none).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct List {
    pub entries: Vec<Entry>,
    pub cursor: usize,
    pub remembered: Option<u16>,
}

impl List {
    /// FUN_001ba6e8: cursor 0, no remembered command.
    pub fn reset_cursor(&mut self) {
        self.cursor = 0;
        self.remembered = None;
    }

    /// FUN_001ba6f8: put the cursor on entry `i` (remembering its command); past the end, reset.
    pub fn set_cursor(&mut self, i: usize) {
        match self.entries.get(i) {
            Some(e) => {
                self.cursor = i;
                self.remembered = Some(e.cmd);
            }
            None => self.reset_cursor(),
        }
    }

    /// FUN_001badc8: after a rebuild the cursor follows its command: kept when the entry under it
    /// still has it, else moved to the entry that has it, else left where it is (and that entry's
    /// command remembered); out of range resets.
    pub fn restore_cursor(&mut self) {
        if self.cursor >= self.entries.len() {
            self.reset_cursor();
            return;
        }
        if Some(self.entries[self.cursor].cmd) == self.remembered {
            return;
        }
        let i = self.entries.iter().position(|e| Some(e.cmd) == self.remembered).unwrap_or(self.cursor);
        self.set_cursor(i);
    }

    /// FUN_001ba910: some entry can be chosen (status 3 or below).
    pub fn any_usable(&self) -> bool {
        self.entries.iter().any(|e| e.status <= 3)
    }

    pub fn selected(&self) -> Option<&Entry> {
        self.entries.get(self.cursor)
    }
}

/// FUN_001baee0: the two command decks.
pub fn is_deck(l: usize) -> bool {
    l == list::ROOT || l == list::ROOT2
}

/// FUN_001baf00: the lists that replace the deck as a whole (5 shortcuts, 10, 12..17).
pub fn is_special_root(l: usize) -> bool {
    l == list::SHORTCUTS || l == 10 || (12..=17).contains(&l)
}

/// What FUN_001ba2c0 needs besides the command row.
pub struct StatusCtx<'a> {
    /// The player's form (`+0xb50`).
    pub form: u8,
    /// FUN_0019aa28 for the player (the lock of `Flags` bit 1).
    pub airborne: bool,
    /// The battle flag (FUN_001903d8; the lock of `Flags` bit 16).
    pub battle: bool,
    /// The player's `+0xaf0` is set (Attack is usable). It comes from the player object's entry
    /// (FUN_001746e0 -> FUN_001d5e48); [inferred] set for every player object the app has.
    pub weapon: bool,
    /// The command lock bit 2 of `DAT_0035d268` (FUN_001b70b8). The app has none.
    pub locked: bool,
    /// FUN_001c7a38 (Revert's party test, not decoded) [inferred true].
    pub revert_ok: bool,
    /// A spell (FUN_001ba2c0 case 2: MP, MP charge, spell switch): magic.rs.
    pub magic: &'a dyn Fn(&Cmd) -> u8,
    /// FUN_001ba568 for a drive or summon command; Dismiss's test (FUN_001ba2c0 case 0x13c).
    pub drive: &'a dyn Fn(&Cmd) -> bool,
}

/// The command table by id (FUN_001b6f48).
pub fn find(cmds: &[Cmd], id: u16) -> Option<&Cmd> {
    cmds.iter().find(|c| c.id == id)
}

/// FUN_001b7290: bit `form` of the `DisableForm` mask of the command whose id is the command's
/// `Execute` (the command itself when they are equal).
pub fn form_disabled(cmds: &[Cmd], c: &Cmd, form: u8) -> bool {
    let mask = if c.id == c.execute { c.disable_form } else { find(cmds, c.execute).map_or(c.disable_form, |e| e.disable_form) };
    mrules::form_disables(mask, form)
}

/// FUN_001b70b8: `Flags` bit 1 locks the command while airborne, bit 16 outside battle, and without
/// bit 5 the global command lock applies.
pub fn command_locked(c: &Cmd, ctx: &StatusCtx) -> bool {
    (c.flags >> 1 & 1 != 0 && ctx.airborne) || (c.flags >> 16 & 1 != 0 && !ctx.battle) || (c.flags >> 5 & 1 == 0 && ctx.locked)
}

/// FUN_001ba2c0: a command's status by its `Execute`, then the locks and the form mask.
pub fn availability(cmds: &[Cmd], c: &Cmd, ctx: &StatusCtx) -> u8 {
    let s = match c.execute {
        0 => status::UNAVAILABLE,
        1 => {
            if ctx.weapon {
                status::READY
            } else {
                status::UNAVAILABLE
            }
        }
        2 => (ctx.magic)(c),
        // Items: nothing of their own (case 3 falls to the common tests).
        3 => status::READY,
        // Drive: FUN_001ba568; Summon: also the special-battle bit 14 (no special battle in the app).
        cmd_id::DRIVE | cmd_id::SUMMON => {
            if (ctx.drive)(c) {
                status::READY
            } else {
                status::UNAVAILABLE
            }
        }
        5 => {
            if ctx.revert_ok {
                status::READY
            } else {
                status::UNAVAILABLE
            }
        }
        // FUN_001b3e38 (the limit test) is not ported: conservative.
        cmd_id::LIMIT => status::UNAVAILABLE,
        0x67 | cmd_id::EMPTY => status::UNAVAILABLE,
        // Dismiss: summon state 2 and no limit task (summon.rs answers through `drive`).
        cmd_id::DISMISS => {
            if (ctx.drive)(c) {
                status::READY
            } else {
                status::UNAVAILABLE
            }
        }
        _ => status::READY,
    };
    if command_locked(c, ctx) || form_disabled(cmds, c, ctx.form) { status::UNAVAILABLE } else { s }
}

/// The manager: the lists, the stack of open lists, the deck in use (`+0xa34`) and the flag word
/// `+0x20` (2 cancel at the deck, 8 Square pressed, 0x10 no item received yet, 0x20 no form owned).
#[derive(Resource, Clone, Debug)]
pub struct CommandLists {
    pub lists: Vec<List>,
    pub stack: Vec<usize>,
    pub deck: usize,
    pub flags: u32,
    /// The member each entry of the target list names (`+0x9ec`), as party member ids.
    pub targets: Vec<u16>,
}

impl Default for CommandLists {
    fn default() -> Self {
        CommandLists { lists: vec![List::default(); LISTS], stack: Vec::new(), deck: list::ROOT, flags: 0, targets: Vec::new() }
    }
}

/// What the builders read (FUN_001bc2b8's callees).
pub struct BuildCtx<'a> {
    pub cmds: &'a [Cmd],
    pub status: StatusCtx<'a>,
    /// The spell commands (+0x9d4, FUN_001bbee8: magic_rules::magic_list).
    pub spells: Vec<u16>,
    /// The user's battle item slots (item, reserved id) and each item's command (`item` +0x10).
    pub battle_items: Vec<(u16, u16)>,
    pub item_command: &'a dyn Fn(u16) -> u16,
    /// The first received consumable (`0x340e50`, FUN_001bbca0).
    pub received: bool,
    /// The form items in the order of the table at 0x37b420 (index 1..=5), with their counts.
    pub form_items: Vec<(u16, u8)>,
    /// FUN_001c7970 for a form's command: 0 shown, 1 shown unavailable, 2 left out.
    pub form_party: &'a dyn Fn(&Cmd) -> u8,
    /// The summon commands (+0x9e2, `summon::SummonState::commands`).
    pub summons: Vec<u16>,
    /// The summon state `DAT_01d5e368` is set (the second deck shows Dismiss).
    pub summon_active: bool,
    /// Genie is out (state 2, NeoStatus 0xc9): the commands FUN_001bcf10 puts in the forms list, one per form 1..=4 whose item is
    /// owned (`FUN_001c6a20`), and the second deck shows "Drive!" in place of Party. `None` when he is not.
    pub genie_drive: Option<Vec<u16>>,
    /// The party members of list 4 (FUN_00175b68 0..2 that are valid, not summons and have a
    /// status) and the target command of each (FUN_001b58a8: the table at 0x37bab0).
    pub party: Vec<(u16, u16)>,
    /// The world (`DAT_0033e830`).
    pub world: u8,
    /// FUN_001768b8: ability bit 107 (block +0xc & 0x800) and an MP charge running.
    pub charging_attack: bool,
    /// The player is Sora (FUN_001699a8 == 1), for FUN_001793a8.
    pub is_sora: bool,
}

impl CommandLists {
    /// FUN_001ba748: add a command to list `l`. `count` is the entry's `+4`. The command slots that
    /// stand in for some commands (`CmdIcon` 1 -> slot 0, command 0x51 -> slot 2) are empty in the
    /// app. Returns the new entry's index.
    pub fn add(&mut self, l: usize, cmd: u16, count: u8, ctx: &BuildCtx) -> Option<usize> {
        let c = find(ctx.cmds, cmd)?;
        let mut e = Entry { flags: 0, cmd: c.id, count, status: availability(ctx.cmds, c, &ctx.status) };
        if c.execute == 3 && count == 0 {
            e.status = status::RESERVED;
        }
        let sub = c.sub_menu;
        if sub >= 0 && sub != 4 {
            let s = &self.lists[sub as usize];
            if s.entries.is_empty() {
                e.status = status::UNAVAILABLE;
            } else if !s.any_usable() {
                e.status = status::NONE_USABLE;
            }
        }
        let list = &mut self.lists[l];
        if list.entries.len() >= 8 {
            return None;
        }
        list.entries.push(e);
        Some(list.entries.len() - 1)
    }

    fn clear(&mut self, l: usize) {
        self.lists[l].entries.clear();
    }

    /// FUN_001bc4c8: the first command of a deck.
    fn add_attack(&mut self, l: usize, ctx: &BuildCtx) {
        let id = if ctx.world == 9 {
            cmd_id::ATTACK_WORLD9
        } else if ctx.status.form == 2 {
            cmd_id::ATTACK_WISDOM
        } else if ctx.charging_attack {
            cmd_id::ATTACK_CHARGING
        } else {
            cmd_id::ATTACK
        };
        // FUN_00180680 (a story flag test) holds in the app: no story flags.
        self.add(l, id, 1, ctx);
    }

    /// FUN_001bc2b8: rebuild every list in the game's order (sub-lists first, so a deck entry's
    /// status sees them).
    pub fn rebuild(&mut self, ctx: &BuildCtx) {
        // FUN_001bcb98: the targets.
        self.clear(list::TARGETS);
        self.targets.clear();
        for &(member, cmd) in &ctx.party {
            self.targets.push(member);
            self.add(list::TARGETS, cmd, 1, ctx);
        }
        // FUN_001bc870: the spells.
        self.clear(list::MAGIC);
        for &c in &ctx.spells {
            self.add(list::MAGIC, c, 1, ctx);
        }
        // FUN_001bc910: the items, one entry per command; a slot that carries the item adds to the
        // count, a slot that only reserves it adds an entry of count 0 (status 6).
        self.clear(list::ITEMS);
        for &(item, reserved) in &ctx.battle_items {
            if item == 0 && reserved == 0 {
                continue;
            }
            let cmd = (ctx.item_command)(if reserved != 0 { reserved } else { item });
            match self.lists[list::ITEMS].entries.iter().position(|e| e.cmd == cmd) {
                Some(i) => {
                    if item != 0 {
                        let e = &mut self.lists[list::ITEMS].entries[i];
                        e.count = e.count.wrapping_add(1);
                    }
                }
                None => {
                    if let Some(i) = self.add(list::ITEMS, cmd, 1, ctx)
                        && item == 0
                    {
                        let e = &mut self.lists[list::ITEMS].entries[i];
                        e.count = 0;
                        e.status = status::RESERVED;
                    }
                }
            }
        }
        self.lists[list::ITEMS].restore_cursor();
        // FUN_001bca70: the forms whose item is owned; flag 0x20 when there is none.
        self.clear(list::FORMS);
        self.flags |= 0x20;
        for &(item, n) in &ctx.form_items {
            if n == 0 {
                continue;
            }
            self.flags &= !0x20;
            let cmd = (ctx.item_command)(item);
            let Some(c) = find(ctx.cmds, cmd) else { continue };
            let party = (ctx.form_party)(c);
            if party != 2
                && let Some(i) = self.add(list::FORMS, cmd, 1, ctx)
                && party == 1
            {
                self.lists[list::FORMS].entries[i].status = status::UNAVAILABLE;
            }
        }
        // FUN_001bc8b0: the summons.
        self.clear(list::SUMMONS);
        for &c in &ctx.summons {
            self.add(list::SUMMONS, c, 1, ctx);
        }
        // FUN_001bc578: the commands deck (command slot 3, the shortcut override, is empty).
        self.clear(list::ROOT);
        self.add_attack(list::ROOT, ctx);
        self.add(list::ROOT, if ctx.spells.is_empty() { cmd_id::EMPTY } else { cmd_id::MAGIC }, 1, ctx);
        // FUN_001bbca0: flag 0x10 (no consumable at the last reset) or nothing received yet.
        let no_items = self.flags & 0x10 != 0 || !ctx.received;
        self.add(list::ROOT, if no_items { cmd_id::EMPTY } else { cmd_id::ITEMS }, 1, ctx);
        if self.flags & 0x20 == 0 {
            let form = ctx.status.form;
            // FUN_001793a8: Sora in a drive form (1..5).
            if ctx.is_sora && (1..6).contains(&form) {
                if let Some(i) = self.add(list::ROOT, cmd_id::REVERT, 1, ctx)
                    && form == 5
                    && ctx.status.battle
                {
                    self.lists[list::ROOT].entries[i].status = status::UNAVAILABLE;
                }
            } else if let Some(i) = self.add(list::ROOT, cmd_id::DRIVE, 1, ctx)
                && form != 0
            {
                self.lists[list::ROOT].entries[i].status = status::UNAVAILABLE;
            }
        } else {
            self.add(list::ROOT, cmd_id::EMPTY, 1, ctx);
        }
        // FUN_001bc6e0: the second deck (Dismiss while a summon is active; the story flags of
        // FUN_001807a0 / FUN_001807f8 read clear, so Party and Limit are listed).
        self.clear(list::ROOT2);
        self.add_attack(list::ROOT2, ctx);
        let summon = if ctx.summon_active {
            cmd_id::DISMISS
        } else if self.lists[list::SUMMONS].entries.is_empty() {
            cmd_id::EMPTY
        } else {
            cmd_id::SUMMON
        };
        self.add(list::ROOT2, summon, 1, ctx);
        // FUN_001bc6e0: state 2 with Genie out (FUN_001c6918 == 2, FUN_001c6940 == 0xc9): FUN_001bcf10 refills the FORMS list
        // (the manager's +0x180 = list 3) with his form commands, after FUN_001bca70 and the first deck were built, and
        // command 0x25d takes Party's place whatever the story flags say.
        if let Some(forms) = &ctx.genie_drive {
            self.clear(list::FORMS);
            for &c in forms {
                self.add(list::FORMS, c, 1, ctx);
            }
            self.add(list::ROOT2, cmd_id::GENIE_DRIVE, 1, ctx);
        } else {
            self.add(list::ROOT2, cmd_id::PARTY, 1, ctx);
        }
        self.add(list::ROOT2, cmd_id::LIMIT, 1, ctx);
    }

    /// FUN_001bb558: open list `l` on top.
    fn push(&mut self, l: usize) {
        self.stack.push(l);
    }

    fn top(&self) -> Option<usize> {
        self.stack.last().copied()
    }

    fn top_list(&mut self) -> Option<&mut List> {
        let l = self.top()?;
        Some(&mut self.lists[l])
    }
}

/// The pad as the menu reads it (newly pressed bits of the field button set 0x379768).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MenuInput {
    /// Logical 0x4 (and 0x200000000): confirm.
    pub confirm: bool,
    /// Logical 0x8: cancel.
    pub cancel: bool,
    /// Logical 0x200 (Square).
    pub square: bool,
    /// Logical 0x4000 / 0x8000: d-pad up and down.
    pub up: bool,
    pub down: bool,
    /// Logical 0x10000 / 0x10000000: d-pad left and right.
    pub left: bool,
    pub right: bool,
}

/// The menu sounds FUN_002c14a8 is asked for (ids of the system sound list; not played by the app).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sound {
    /// 1: the cursor moved.
    Cursor,
    /// 3: a list opened.
    Open,
    /// 4: a list closed.
    Close,
    /// 5: a refused choice.
    Refused,
    /// 0x14: the deck changed.
    Deck,
}

/// What one frame of menu input did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Outcome {
    /// A command to issue (FUN_001bc0c8 -> FUN_001bb8c0): the command and, for a command chosen
    /// through the target list, the target member.
    pub issue: Option<(u16, Option<u16>)>,
    /// A list opened (its index).
    pub opened: Option<usize>,
    pub sounds: Vec<Sound>,
}

impl CommandLists {
    /// FUN_001bc370 without L1 (the app has no shortcut button): the deck is the root; a stack with
    /// another bottom is dropped and the deck's cursor reset.
    pub fn select_root(&mut self) {
        if self.stack.first() != Some(&self.deck) {
            self.stack.clear();
            self.push(self.deck);
            self.lists[self.deck].reset_cursor();
            self.flags &= !2;
        }
    }

    /// FUN_001bacd0: below the top, the first list that is empty closes with everything above it
    /// (sound 4); a deck stops the walk. True when it closed something.
    fn close_empty(&mut self, out: &mut Outcome) -> bool {
        for depth in (0..self.stack.len()).rev() {
            let l = self.stack[depth];
            if is_deck(l) || is_special_root(l) {
                return false;
            }
            if self.lists[l].entries.is_empty() {
                while self.stack.len() > depth {
                    let top = self.stack.pop().unwrap();
                    self.lists[top].reset_cursor();
                }
                out.sounds.push(Sound::Close);
                return true;
            }
        }
        false
    }

    /// One frame of FUN_001bbfd8 (after the root selection): the top list's input (FUN_001ba9b0),
    /// then Left: back one list, or at the deck the deck swap (FUN_001bc410). `issue` answers
    /// whether a chosen command was accepted (FUN_001bb8c0 -> FUN_001b6fd0).
    pub fn input(&mut self, cmds: &[Cmd], inp: MenuInput, issue: &mut dyn FnMut(u16, Option<u16>) -> bool) -> Outcome {
        let mut out = Outcome::default();
        if self.stack.is_empty() {
            return out;
        }
        let handled = self.list_input(cmds, inp, issue, &mut out);
        if handled && inp.left {
            if self.stack.len() > 1 {
                self.stack.pop();
                out.sounds.push(Sound::Close);
            } else {
                // FUN_001bc410 (FUN_001807a0 holds: no story flags): the other deck, cursor kept
                // from the old one (FUN_001bc468).
                let other = if self.deck != list::ROOT { list::ROOT } else { list::ROOT2 };
                let cur = self.lists[self.deck].cursor;
                self.deck = other;
                self.lists[other].set_cursor(cur);
                self.stack.clear();
                self.push(other);
                out.sounds.push(Sound::Deck);
            }
        }
        out
    }

    /// FUN_001ba9b0 on the top list. Returns false when nothing was read (an empty list closed).
    fn list_input(&mut self, cmds: &[Cmd], inp: MenuInput, issue: &mut dyn FnMut(u16, Option<u16>) -> bool, out: &mut Outcome) -> bool {
        if self.close_empty(out) {
            return false;
        }
        let Some(top) = self.top() else { return false };
        if !is_deck(top) {
            self.lists[top].restore_cursor();
        }
        if inp.confirm {
            self.decide(cmds, issue, out);
            return true;
        }
        if inp.cancel {
            if !is_deck(top) {
                self.stack.pop();
                out.sounds.push(Sound::Close);
                if let Some(t) = self.top()
                    && (is_deck(t) || is_special_root(t))
                {
                    self.lists[t].reset_cursor();
                }
            } else {
                self.flags |= 2;
            }
            return true;
        }
        // FUN_001806c8 (a story flag test) holds in the app.
        if inp.square {
            self.flags |= 8;
            return true;
        }
        let l = &mut self.lists[top];
        let n = l.entries.len();
        if inp.up || inp.down {
            // Both directions wrap.
            let i = if inp.up {
                if l.cursor < 1 { n.saturating_sub(1) } else { l.cursor - 1 }
            } else if l.cursor + 1 < n {
                l.cursor + 1
            } else {
                0
            };
            l.set_cursor(i);
            out.sounds.push(Sound::Cursor);
            return true;
        }
        if inp.right
            && let Some(e) = l.selected().copied()
            && e.status <= 3
            && let Some(c) = find(cmds, e.cmd)
            && c.sub_menu >= 0
        {
            self.push(c.sub_menu as usize);
            out.opened = Some(c.sub_menu as usize);
            out.sounds.push(Sound::Open);
        }
        true
    }

    /// FUN_001bb930 with FUN_001bc0c8: a usable entry opens its sub-menu or is issued; an issued
    /// command closes every list down to the deck with every cursor back on the first entry. An
    /// unusable entry (5, 6) buzzes unless Square was pressed (flag 8).
    fn decide(&mut self, cmds: &[Cmd], issue: &mut dyn FnMut(u16, Option<u16>) -> bool, out: &mut Outcome) {
        let Some(top) = self.top() else { return };
        let Some(e) = self.lists[top].selected().copied() else { return };
        if e.status >= 4 {
            if matches!(e.status, 5 | 6) && self.flags & 8 == 0 {
                out.sounds.push(Sound::Refused);
            }
            return;
        }
        // FUN_001bc0c8.
        let (cmd, target) = if top == list::TARGETS {
            // The command is the one chosen in the list below; the target is this list's member.
            let below = self.stack.len().checked_sub(2).map(|d| self.stack[d]);
            let Some(cmd) = below.and_then(|b| self.lists[b].selected()).map(|e| e.cmd) else { return };
            (cmd, self.targets.get(self.lists[top].cursor).copied())
        } else {
            let Some(c) = find(cmds, e.cmd) else { return };
            if c.sub_menu >= 0 {
                self.push(c.sub_menu as usize);
                out.opened = Some(c.sub_menu as usize);
                out.sounds.push(Sound::Open);
                return;
            }
            (e.cmd, None)
        };
        if issue(cmd, target) {
            out.issue = Some((cmd, target));
            while let Some(l) = self.top_list() {
                l.set_cursor(0);
                let t = self.top().unwrap();
                if is_deck(t) || is_special_root(t) {
                    break;
                }
                self.stack.pop();
            }
        }
    }
}

// ------------------------------------------------------------------------------------------------
// The item effect (pure)
// ------------------------------------------------------------------------------------------------

/// The parts of a status record (`FUN_0019fe48(member)`) FUN_001a12a0 changes: HP `+0` (maximum `+4`,
/// minimum `+8`), MP `+0x180` (maximum `+0x184`), the MP charge `+0x1bc` (left) and `+0x1c0` (total),
/// the drive gauge `+0x1b0` (fraction), `+0x1b1` (points), `+0x1b2` (maximum points).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vitals {
    pub hp: i32,
    pub hp_max: i32,
    pub hp_min: i32,
    pub mp: i32,
    pub mp_max: i32,
    pub charge_left: f32,
    pub charge_total: f32,
    pub drive_fraction: u8,
    pub drive_points: u8,
    pub drive_max: u8,
}

impl Vitals {
    /// FUN_001a0100: HP plus `delta`, held between the minimum and the maximum.
    fn add_hp(&mut self, delta: i32) {
        let sum = self.hp + delta;
        self.hp = if self.hp_min <= sum { if self.hp_max < sum { self.hp_max } else { sum } } else { self.hp_min };
    }

    /// Whether the record belongs to a KO'd object: `FUN_00186368` tests the object's +0x7f8 bit 4, which the HP
    /// method sets exactly when the HP reaches 0 (`FUN_00186530`, `party_battle::hp_change`), so a maximum above 0
    /// with HP at its minimum 0 stands for it [inferred: the record holds no KO bit of its own].
    fn is_ko(&self) -> bool {
        self.hp_max > 0 && self.hp_min == 0 && self.hp == 0
    }

    /// FUN_001a0210 -> FUN_00186368 for an object that has one (every party member and Sora): while the object is KO'd
    /// the MP is not changed (the method answers 0 before `FUN_001a0180`). The Elixir-like items change the HP first
    /// (FUN_001a12a0 order), and a heal revives the object (`FUN_0017cee8`), so their MP lands in the same use.
    fn add_mp_unless_ko(&mut self, delta: i32, feed: f32) {
        if !self.is_ko() {
            self.add_mp(delta, feed);
        }
    }

    /// FUN_001a0180: MP plus `delta` (0..max); while the MP charge runs (FUN_001a05d0: total above 0)
    /// a gain shortens the charge by `delta * feed` instead (FUN_001a0540, `sstm` +0x7c), and a charge
    /// brought to 0 ends and refills MP (FUN_001a05a8).
    fn add_mp(&mut self, delta: i32, feed: f32) {
        if self.charge_total > 0.0 {
            if delta > 0 {
                let v = self.charge_left + -((delta as f32) * feed);
                let mut left = 0.0;
                if v >= 0.0 {
                    left = if self.charge_total < v { self.charge_total } else { v };
                }
                self.charge_left = left;
                if left == 0.0 {
                    self.charge_left = 0.0;
                    self.charge_total = 0.0;
                    let max = self.mp_max;
                    self.add_mp(max, feed);
                }
            }
            return;
        }
        let sum = self.mp + delta;
        self.mp = if sum < 0 { 0 } else if self.mp_max < sum { self.mp_max } else { sum };
    }

    /// FUN_001a0290: the drive gauge plus `delta` hundredths (carry into points up to the maximum).
    fn add_drive(&mut self, delta: i32) {
        let mut v = self.drive_fraction as i32 + delta;
        if delta < 0 {
            if v < 0 {
                if self.drive_points == 0 {
                    v = 0;
                } else {
                    v += 100;
                    self.drive_points -= 1;
                }
            }
        } else if v > 99 {
            let mut p = self.drive_points;
            loop {
                if p < self.drive_max {
                    p += 1;
                    self.drive_points = p;
                }
                if self.drive_max <= p {
                    v = 100;
                    break;
                }
                v -= 100;
                if v <= 99 {
                    break;
                }
            }
        }
        self.drive_fraction = v as u8;
    }
}

/// Item ids FUN_001a12a0 switches on (the item's `Id`).
pub mod item_id {
    pub const DRIVE_RECOVERY: u16 = 0x112;
    pub const HIGH_DRIVE_RECOVERY: u16 = 0x113;
    // 0x114..=0x117: the boosts (FUN_001a0ce0, 0d30, 0d80, 0dd0: stats of the party record; menu only).
    /// The "everyone" items (FUN_001a1fe0: 5..=7 and 0x83).
    pub fn group(id: u16) -> bool {
        (5..8).contains(&id) || id == 0x83
    }
}

/// FUN_001a12a0(item, target, user, mult): `amount` is the item's word at +4 (`Flag1 | Rank << 8`,
/// a percent for HP and MP), `mult` 100 plus the user's Item Boost (ability block +0x4c). HP items
/// add `(max HP * amount * mult + 9999) / 10000` (1, 2, 5), MP items the same of max MP (3, 6),
/// 4 and 7 both, 0x83 the whole maximum HP and the MP share, 0x112 / 0x113 the drive gauge
/// `amount * 100`. The boosts (0x114..0x117) are not ported (they write the party record; the
/// original allows them only from the pause menu). Returns false for an id that does nothing here.
pub fn item_effect(id: u16, amount: u16, mult: i32, v: &mut Vitals, feed: f32) -> bool {
    let hp = (v.hp_max * amount as i32 * mult + 9999) / 10000;
    let mp = (v.mp_max * amount as i32 * mult + 9999) / 10000;
    match id {
        1 | 2 | 5 => v.add_hp(hp),
        3 | 6 => v.add_mp_unless_ko(mp, feed),
        4 | 7 => {
            v.add_hp(hp);
            v.add_mp_unless_ko(mp, feed);
        }
        0x83 => {
            let full = v.hp_max;
            v.add_hp(full);
            v.add_mp_unless_ko(mp, feed);
        }
        item_id::DRIVE_RECOVERY | item_id::HIGH_DRIVE_RECOVERY => v.add_drive(amount as i32 * 100),
        _ => return false,
    }
    true
}

// ------------------------------------------------------------------------------------------------
// The app
// ------------------------------------------------------------------------------------------------

/// The motion request of the item use (FUN_00175e20: FUN_0017e220 with 9 and blend 8.0 at 0x37a4b8).
pub const USE_MOTION: usize = 9;
pub const USE_BLEND: f32 = 8.0;
/// Frame trigger 6 of the motion (FUN_00188498 case 6 -> FUN_001b8710): the item is used.
pub const TRIGGER_USE: u8 = 6;
/// The queued use's wait (FUN_001b82b8 with `DAT_0037bba4` = 30.0; FUN_001b84c8 starts effect 0 while
/// it is at least `DAT_0037bba8` = 30.0 and counts down by `DAT_00356f40` per frame). Read from the
/// executable at run time, these are the fallbacks.
const USE_WAIT: f32 = 30.0;
/// Ticks a chosen item waits for Sora to take it (app rule, as magic.rs's cast request).
const REQUEST_TICKS: u32 = 20;
/// The 00effect.bar element started at the user when the use starts (FUN_001b8400).
const USE_EFFECT: u16 = 0;

fn log_on() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("KH2_ITEMLOG").is_ok_and(|v| v != "0"))
}

/// Tables the menu and the item use read once.
#[derive(Resource)]
pub struct ItemsData {
    pub cmds: Vec<Cmd>,
    pub items: Vec<t::Item>,
    /// The form items by form index 1..=5 (the u16 table at 0x37b420, FUN_001a1e38).
    pub form_items: [u16; 6],
    /// Genie's form commands by form index 1..=4 (the u16 table at 0x37c036, FUN_001c6b10).
    pub genie_forms: [u16; 5],
    /// The target command of each party member id (the u16 table at 0x37bab0, FUN_001b58a8).
    pub member_cmds: Vec<u16>,
    /// `sstm` +0x7c: MP gained during the MP charge shortens it by this much per point.
    pub charge_feed: f32,
    /// `sstm` +0x54 and the word at 0x37a560: the scales `FUN_0017cee8` gives a heal on a KO'd member (the second for
    /// a member with Hyper Healing equipped, `party_rules::revive_scale`).
    pub revive_scale: (f32, f32),
    /// The queued use's wait (`DAT_0037bba4`) and the frame step (`DAT_00356f40`).
    pub use_wait: f32,
    pub messages: Option<kh2_image::text::Messages>,
}

impl ItemsData {
    fn item(&self, id: u16) -> Option<&t::Item> {
        self.items.iter().find(|i| i.id == id)
    }

    /// The item's name (its `Name` message), for the log.
    pub fn name(&self, id: u16) -> String {
        self.item(id).and_then(|i| self.messages.as_ref()?.text(i.name as u32)).unwrap_or_else(|| format!("item {id}"))
    }

    fn cmd_name(&self, cmd: u16) -> String {
        find(&self.cmds, cmd).and_then(|c| self.messages.as_ref()?.text(c.message_id as u32)).unwrap_or_else(|| format!("command {cmd}"))
    }
}

fn exe_u16(exe: &[u8], va: usize) -> Option<u16> {
    let o = va.checked_sub(0xff000)?;
    Some(u16::from_le_bytes(exe.get(o..o + 2)?.try_into().ok()?))
}

fn exe_f32(exe: &[u8], va: usize) -> Option<f32> {
    let o = va.checked_sub(0xff000)?;
    Some(f32::from_le_bytes(exe.get(o..o + 4)?.try_into().ok()?))
}

/// The item use in progress.
#[derive(Clone, Copy, Debug)]
struct Using {
    item: u16,
    slot: Option<usize>,
    target: u16,
}

/// A queued use (the 0x18-byte object of FUN_001b82b8): time left, item, user, target (0 = all).
#[derive(Clone, Copy, Debug)]
struct Queued {
    left: f32,
    item: u16,
    target: u16,
    count: (usize, usize),
}

#[derive(Resource, Default)]
pub struct ItemUse {
    /// The chosen item command and its target, waiting for Sora (command, target, ticks left).
    request: Option<(u16, u16, u32)>,
    using: Option<Using>,
    queue: Vec<Queued>,
    /// Ticks run (for the log).
    tick: u64,
}

pub struct ItemsPlugin;

impl Plugin for ItemsPlugin {
    fn build(&self, app: &mut App) {
        crate::progress::ensure(app);
        app.init_resource::<CommandLists>()
            .init_resource::<ItemUse>()
            .add_systems(Startup, setup)
            .add_systems(Update, command_menu.after(crate::magic::harness_keys).run_if(crate::menu::is_closed))
            .add_systems(FixedUpdate, run.in_set(Sim::Sora).after(crate::sora::tick));
    }
}

fn setup(mut commands: Commands, disc: Res<Disc>) {
    let (cmds, items, sstm) = {
        let Ok(mut a) = disc.0.lock() else { return };
        let cmds = kh2_tables::load::<Cmd>(&mut a, &t::CMD).map(|d| d.rows).unwrap_or_default();
        let items = kh2_tables::load::<t::Item>(&mut a, &t::ITEM).map(|d| d.rows).unwrap_or_default();
        let sstm = kh2_tables::player::load_sstm(&mut a).ok();
        (cmds, items, sstm)
    };
    let exe = disc.read_root("SLUS_210.05").unwrap_or_default();
    let mut form_items = [0u16; 6];
    for (i, f) in form_items.iter_mut().enumerate().skip(1) {
        *f = exe_u16(&exe, 0x37b420 + 2 * i).unwrap_or(0);
    }
    let mut genie_forms = [0u16; 5];
    for (i, f) in genie_forms.iter_mut().enumerate().skip(1) {
        *f = exe_u16(&exe, 0x37c036 + 2 * i).unwrap_or(0);
    }
    let member_cmds: Vec<u16> = (0..16).map(|i| exe_u16(&exe, 0x37bab0 + 2 * i).unwrap_or(0)).collect();
    let charge_feed = sstm.as_ref().filter(|s| s.words.len() > 0x7c / 4).map_or(0.0, |s| s.f32(0x7c));
    let revive_scale = (
        sstm.as_ref().filter(|s| s.words.len() > 0x54 / 4).map_or(1.0, |s| s.f32(0x54)),
        crate::party_rules::revive_full_scale(&exe).unwrap_or(1.0),
    );
    let use_wait = exe_f32(&exe, 0x37bba4).filter(|v| v.is_finite() && *v > 0.0).unwrap_or(USE_WAIT);
    let messages = disc.read("msg/us/sys.bar").and_then(|b| kh2_image::text::Messages::from_bar(&b));
    info!("items: form items {form_items:?}, Sora's target command {:?}, MP charge feed {charge_feed}, use wait {use_wait}", member_cmds.get(1));
    commands.insert_resource(ItemsData { cmds, items, form_items, genie_forms, member_cmds, charge_feed, revive_scale, use_wait, messages });
}

/// The game state the menu reads besides Sora: the drive state, `Progress`, the battle flag, the
/// ability block.
type MenuState<'w> = (Option<Res<'w, crate::drive::DriveState>>, Option<Res<'w, Progress>>, Option<Res<'w, Battle>>, Option<Res<'w, crate::abilities::Abilities>>);

/// What the item use changes or reads besides Sora: the MP charge, the maxima, the battle flag, the
/// ability block (Item Boost), the drive gauge.
type UseState<'w> = (ResMut<'w, MagicState>, Res<'w, crate::data::GameData>, Option<Res<'w, Battle>>, Option<Res<'w, crate::abilities::Abilities>>, Option<ResMut<'w, crate::drive::DriveState>>);

/// The command menu each frame: rebuild (FUN_001bc2b8), the root (FUN_001bc370), the pad
/// (FUN_001bbfd8), the issued command, and the pages the HUD draws.
#[allow(clippy::too_many_arguments)]
fn command_menu(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    data: Option<Res<ItemsData>>,
    magic_data: Option<Res<MagicData>>,
    mut magic: ResMut<MagicState>,
    mut pad: ResMut<Pad>,
    mut cm: ResMut<CommandMenu>,
    mut lists: ResMut<CommandLists>,
    mut uses: ResMut<ItemUse>,
    sora: Query<&Sora>,
    (drive, progress, battle, abilities): MenuState,
    (summon, mut summon_queue): (Option<Res<crate::shared::summon::SummonState>>, Option<ResMut<crate::shared::summon::SummonQueue>>),
    members: Query<&crate::party::PartyMember>,
    mut drive_queue: Option<ResMut<crate::drive::DriveQueue>>,
) {
    let (Some(data), Some(md), Some(progress)) = (data, magic_data, progress) else { return };
    let Ok(s) = sora.single() else { return };
    let form = drive.as_ref().map_or(0, |d| d.object_form());
    let battle_on = battle.as_ref().is_some_and(|b| b.active());
    let charging = magic.charge.active();
    let mp = s.mp;
    let magic_status = |c: &Cmd| md.status(c.id, mp, charging, form, Some(&*progress)) as u8;
    let points = drive.as_ref().map_or(0, |d| d.points.whole);
    let (donald, goofy) = drive.as_ref().map_or((true, true), |d| (d.donald, d.goofy));
    // FUN_001ba568: a drive command that opens a list (the generic Drive) passes; a form's command
    // needs no form change running and no form active (6 and 11 refuse), the form's companions,
    // and its cost in whole points. The player object's +0x588 bit 0x1000000 is taken as set.
    let drive_ok = |c: &Cmd| {
        if c.sub_menu != -1 && c.sub_menu != 4 {
            return true;
        }
        // A summon command and Dismiss: summon.rs (FUN_001ba568 + FUN_001c7970, FUN_001b3c00).
        if c.execute == cmd_id::SUMMON || c.execute == cmd_id::DISMISS {
            let (Some(sm), Some(d)) = (summon.as_deref(), drive.as_deref()) else { return false };
            return if c.execute == cmd_id::DISMISS { sm.dismiss_ok() } else { sm.command_ok(d, c) };
        }
        let party_ok = c.execute != 4 || crate::drive::rules::companions_ok(c.argument as u8, donald, goofy);
        party_ok && form != 6 && form != 11 && c.cost <= points
    };
    // FUN_001c7970 -> FUN_001c77f0: a missing companion leaves the form out (2).
    let form_party = |c: &Cmd| -> u8 {
        if c.execute == 4 && !crate::drive::rules::companions_ok(c.argument as u8, donald, goofy) { 2 } else { 0 }
    };
    let items = progress.items();
    let item_command = |id: u16| items.get(id).map_or(0, |i| i.command);
    let block_bits = abilities.as_ref().map(|a| a.block().bits[3]).unwrap_or(0);
    let ctx = BuildCtx {
        cmds: &data.cmds,
        status: StatusCtx { form, airborne: !s.motor.grounded, battle: battle_on, weapon: true, locked: false, revert_ok: true, magic: &magic_status, drive: &drive_ok },
        spells: mrules::magic_list(&md.magc, &md.levels, md.world),
        battle_items: progress.battle_items(SORA),
        item_command: &item_command,
        received: progress.recent_items()[0] != 0,
        form_items: (1..=5).map(|i| (data.form_items[i], progress.item_count(data.form_items[i]))).collect(),
        form_party: &form_party,
        summons: summon.as_ref().map_or_else(Vec::new, |s| s.commands.clone()),
        summon_active: summon.as_ref().is_some_and(|s| s.state != 0),
        genie_drive: summon.as_ref().filter(|s| s.genie_out()).map(|_| {
            // FUN_001c6a20: forms 1..=4 (the item at 0x37b420 + 2 i with a count above 0), each as the command at 0x37c036 + 2 i.
            (1..5).filter(|&i| progress.item_count(data.form_items[i]) > 0).map(|i| data.genie_forms[i]).collect()
        }),
        party: target_party(&data.member_cmds, members.iter().map(|m| (m.slot, m.neo))),
        world: md.world,
        charging_attack: block_bits & 0x800 != 0 && charging,
        is_sora: true,
    };
    let before: Vec<(usize, usize)> = lists.stack.iter().map(|&l| (l, lists.lists[l].cursor)).collect();
    lists.rebuild(&ctx);
    lists.select_root();
    let inp = MenuInput {
        confirm: keys.just_pressed(KeyCode::KeyJ) || mouse.just_pressed(MouseButton::Left),
        cancel: keys.just_pressed(KeyCode::KeyB),
        up: keys.just_pressed(KeyCode::KeyT),
        down: keys.just_pressed(KeyCode::KeyG),
        // H stands in for d-pad Left (the deck swap at the deck, FUN_001bc410).
        left: keys.just_pressed(KeyCode::KeyH),
        ..default()
    };
    // The confirm button is the attack button: on the deck's attack command the press stays an attack
    // (Sora's own controller issues it); on anything else the menu takes it.
    let on_attack = lists.top() == Some(lists.deck)
        && lists.lists[lists.deck].selected().and_then(|e| find(&data.cmds, e.cmd)).is_some_and(|c| c.execute == 1);
    if inp.confirm && !on_attack {
        pad.steal_attack();
    }
    let mut issue = |cmd: u16, target: Option<u16>| -> bool {
        let Some(c) = find(&data.cmds, cmd) else { return false };
        match c.execute {
            // Attack: Sora's controller takes the same press.
            1 => true,
            2 => {
                magic.request_cast(cmd);
                true
            }
            3 => {
                uses.request = Some((cmd, target.unwrap_or(SORA), REQUEST_TICKS));
                info!("items: {} chosen for member {}", data.cmd_name(cmd), target.unwrap_or(SORA));
                true
            }
            // A summon command, Dismiss or one of Genie's "Drive!" forms (535..538): summon.rs runs it (event 11 on his script
            // for a form, FUN_00177c20 -> FUN_0016b420).
            _ if summon_request(c, cmd).is_some() && summon_queue.is_some() => {
                let req = summon_request(c, cmd).unwrap_or(crate::shared::summon::SummonRequest::Dismiss);
                info!("items: {} chosen ({req:?})", data.cmd_name(cmd));
                summon_queue.as_deref_mut().into_iter().for_each(|q| q.0.push(req));
                true
            }
            // A form command (Valor 26 ...) or Revert: Sora's command handler takes Execute 4 and 5 (FUN_001cf030), drive.rs runs them.
            _ if drive_request(c).is_some() && drive_queue.is_some() => {
                let req = drive_request(c).unwrap_or(crate::drive::DriveRequest::Revert);
                info!("items: {} chosen ({req:?})", data.cmd_name(cmd));
                drive_queue.as_deref_mut().into_iter().for_each(|q| q.0.push(req));
                true
            }
            _ => {
                info!("items: command {cmd} ({}) is not run from the menu in the app", data.cmd_name(cmd));
                false
            }
        }
    };
    let out = lists.input(&data.cmds, inp, &mut issue);
    if let Some(l) = out.opened {
        let n = lists.lists[l].entries.len();
        if l == list::MAGIC {
            crate::magic::log_menu(&format!("magic: menu: Magic opened: {n} spells {:?} (MP {mp}, charging {charging})", ctx.spells));
        } else {
            info!("items: list {l} opened ({n} entries: {})", lists.lists[l].entries.iter().map(|e| format!("{} x{} status {}", data.cmd_name(e.cmd), e.count, e.status)).collect::<Vec<_>>().join(", "));
        }
    }
    if out.sounds.contains(&Sound::Refused) {
        let e = lists.top().and_then(|t| lists.lists[t].selected().copied());
        info!("items: refused {:?}", e.map(|e| (data.cmd_name(e.cmd), e.status)));
    }
    let after: Vec<(usize, usize)> = lists.stack.iter().map(|&l| (l, lists.lists[l].cursor)).collect();
    if log_on() && after != before {
        info!("items: menu stack {after:?}");
    }
    // The pages the HUD draws (0x13ca30: one per stacked list, the list's index is its kind).
    let entry = |e: &Entry| {
        let c = find(&data.cmds, e.cmd);
        MenuEntry { message: c.map_or(0, |c| c.message_id as u32), icon: c.map_or(0, |c| c.cmd_icon), style: e.status }
    };
    cm.visible = true;
    cm.pages = lists.stack.iter().map(|&l| MenuPage { kind: l as u8, entries: lists.lists[l].entries.iter().map(entry).collect(), cursor: lists.lists[l].cursor }).collect();
}

/// FUN_001bcb98: the target list's party, Sora first, then the field members by party slot (FUN_00175b68), each with
/// its target command from the u16 table at 0x37bab0 (`cmds`, by member id); a member with no command there is left out.
pub fn target_party(cmds: &[u16], members: impl Iterator<Item = (u8, u16)>) -> Vec<(u16, u16)> {
    let cmd = |m: u16| cmds.get(m as usize).copied().unwrap_or(0);
    let mut others: Vec<(u8, u16)> = members.filter(|&(_, m)| m != SORA && cmd(m) != 0).collect();
    others.sort();
    std::iter::once(SORA).chain(others.into_iter().map(|(_, m)| m)).map(|m| (m, cmd(m))).collect()
}

/// Whether a queued use finishing on `target` (0: the group items) counts for the "Healed Party Members" row:
/// FUN_001a11a0 answers `target != user` for one target, and for a group item true when a party slot other than the
/// user's holds a member; FUN_001b8658 then calls FUN_001d76a0 when the user is member 1 (Sora, who is always the user
/// here). `others` are the member ids of the field party besides Sora.
pub fn counts_as_healed(target: u16, others: &[u16]) -> bool {
    if target == 0 { !others.is_empty() } else { target != SORA && others.contains(&target) }
}

/// A field member as the item use sees it: the member id, whether it is down, and the values `item_effect` works on.
#[derive(Clone, Copy, Debug)]
struct FieldMember {
    neo: u16,
    ko: bool,
    v: Vitals,
}

/// A queued use landing on the field members (FUN_001a11a0 -> FUN_001a12a0 on their status blocks, then FUN_001b8658's
/// "Healed Party Members" count). Target 0 reaches every member, else the member with that id; Sora is handled by the
/// caller. A KO'd member is not skipped (`FUN_001a15a0` has no KO test): his HP gain goes through the HP method
/// (`FUN_00186238`, `party_battle::hp_change`), which gets him up with the gain scaled by `revive` (sstm +0x54, or the
/// word at 0x37a560 with Hyper Healing equipped, `FUN_0017cee8`); his MP changes only when that revive happened
/// (`FUN_00186368`). Counts once per use when `counts_as_healed` holds. Returns the indices it acted on.
#[allow(clippy::too_many_arguments)]
fn use_on_members(item: u16, target: u16, amount: u16, mult: i32, feed: f32, revive: (f32, f32), field: &mut [FieldMember], progress: &mut Progress) -> Vec<usize> {
    let mut hit = Vec::new();
    let others: Vec<u16> = field.iter().map(|m| m.neo).filter(|&n| n != SORA).collect();
    for (i, m) in field.iter_mut().enumerate().filter(|(_, m)| (target == 0 || target == m.neo) && m.neo != SORA) {
        let (hp0, max) = (m.v.hp, m.v.hp_max);
        // A down member's record holds HP 0 (`Vitals::is_ko`); the effect sums on it, the HP method applies the gain.
        item_effect(item, amount, mult, &mut m.v, feed);
        if m.ko {
            let abilities = progress.member(m.neo).map(|r| r.abilities).unwrap_or_default();
            let scale = crate::party_rules::revive_scale(&abilities, revive.1, revive.0);
            let gain = m.v.hp - hp0;
            m.v.hp = hp0;
            crate::shared::party::hp_change(&mut m.v.hp, max, 0, &mut m.ko, gain, scale);
        }
        hit.push(i);
    }
    // FUN_001b8658: the user is Sora (member 1) and FUN_001a11a0 answered true.
    if counts_as_healed(target, &others) {
        progress.count_healed();
        info!("items: Healed Party Members counts {}", progress.healed_count());
    }
    hit
}

/// The request the drive system gets for a chosen command: a form's command (`Execute` 4 with no sub-menu, its `Argument`
/// the form id; the generic Drive entry opens the forms list instead) as `Enter`, Revert (`Execute` 5) as `Revert`.
pub fn drive_request(c: &Cmd) -> Option<crate::drive::DriveRequest> {
    use crate::drive::DriveRequest;
    match c.execute {
        cmd_id::DRIVE if c.sub_menu == -1 => Some(DriveRequest::Enter(c.argument as u8)),
        cmd_id::REVERT => Some(DriveRequest::Revert),
        _ => None,
    }
}

/// The request the summon system gets for a chosen command (summon.rs): Summon and Dismiss by their `Execute`, one of Genie's
/// "Drive!" forms (`Execute` 535..538) as `Form`.
pub fn summon_request(c: &Cmd, cmd: u16) -> Option<crate::shared::summon::SummonRequest> {
    use crate::shared::summon::SummonRequest;
    match c.execute {
        cmd_id::DISMISS => Some(SummonRequest::Dismiss),
        cmd_id::SUMMON => Some(SummonRequest::Summon(cmd)),
        e if cmd_id::GENIE_FORMS.contains(&e) => Some(SummonRequest::Form(cmd)),
        _ => None,
    }
}

/// The item command on Sora: the request, the motion, frame trigger 6, the queued use.
#[allow(clippy::too_many_arguments)]
fn run(
    mut commands: Commands,
    data: Option<Res<ItemsData>>,
    mut uses: ResMut<ItemUse>,
    progress: Option<ResMut<Progress>>,
    mut sora_q: Query<&mut Sora>,
    mut anims: Query<&mut Animator>,
    gts: Query<&GlobalTransform>,
    lib: Option<Res<crate::fx::EffectLib>>,
    (mut magic, _, battle, abilities, mut drive): UseState,
    mut party: Query<&mut crate::party::PartyMember>,
) {
    let (Some(data), Some(mut progress)) = (data, progress) else { return };
    let Ok(mut s) = sora_q.single_mut() else { return };
    let body_e = s.body;
    let Ok(mut body) = anims.get_mut(body_e) else { return };
    uses.tick += 1;

    // Take the request when Sora is free (FUN_00174cf8 case 3 -> FUN_00175e20).
    if let Some((cmd, target, left)) = uses.request.take() {
        let free = match s.state {
            State::Ground | State::Air => true,
            State::Attack { .. } | State::Magic => body.trigger(range::GROUNDED) || body.trigger(range::AIR) || body.trigger(range::BLEND_TO_IDLE),
            _ => false,
        };
        let item = find(&data.cmds, cmd).map_or(0, |c| c.argument as u16);
        if free && uses.using.is_none() {
            let pstate = if battle.as_ref().is_some_and(|b| b.active()) { PlayerState::BattleWeapon } else { PlayerState::FieldNoWeapon };
            let slot = body.rig.mset.slot(USE_MOTION, pstate);
            body.restart(slot, false, USE_BLEND);
            s.enter(State::Magic, None);
            uses.using = Some(Using { item, slot, target });
            info!("items: use {} (command {cmd}, target {target}): motion request {USE_MOTION} slot {slot:?} ({})", data.name(item), body.tag());
        } else if left > 1 {
            uses.request = Some((cmd, target, left - 1));
        } else {
            info!("items: use of command {cmd} dropped (Sora {:?})", s.state);
        }
    }

    // Frame trigger 6 of the use motion: take the item out of the user's slot (FUN_00175ec0 ->
    // FUN_001a1ba8 with the user's member id) and queue the use (FUN_001b82b8).
    if let Some(u) = uses.using {
        let in_motion = s.state == State::Magic && body.slot == u.slot;
        let fired = body
            .motion()
            .zip(body.triggers())
            .map(|(m, tr)| {
                let g = |f: f32| f * crate::data::LOGIC_HZ / m.fps.max(1.0);
                let (from, to) = (g(body.prev_frame), g(body.frame));
                from != to && tr.crossed(from, to).any(|f| f.id == TRIGGER_USE)
            })
            .unwrap_or(false);
        if !in_motion {
            info!("items: the use motion ended before its trigger: nothing used");
            uses.using = None;
        } else if fired {
            uses.using = None;
            let count = |p: &Progress| p.battle_items(SORA).iter().filter(|&&(i, _)| i == u.item).count();
            let n0 = count(&progress);
            if progress.take_item(u.item, SORA) {
                let target = if item_id::group(u.item) { 0 } else { u.target };
                let n1 = count(&progress);
                uses.queue.push(Queued { left: data.use_wait, item: u.item, target, count: (n0, n1) });
                info!("items: trigger {TRIGGER_USE} at frame {:.1}: {} taken from Sora's slots ({n0} -> {n1}), stock {}", body.gframe(), data.name(u.item), progress.item_count(u.item));
            } else {
                info!("items: trigger {TRIGGER_USE}: Sora's slots hold no {}", data.name(u.item));
            }
        }
    }

    // The queued uses (FUN_001b84c8): effect 0 at the user on the first frame, then after the wait the
    // item's effect on the target and FUN_001b8658 -> FUN_001a11a0.
    let pos = gts.get(body_e).map_or(Vec3::ZERO, |g| g.translation());
    let mut done = Vec::new();
    let tick = uses.tick;
    for (i, q) in uses.queue.iter_mut().enumerate() {
        if q.left >= data.use_wait
            && let Some(lib) = lib.as_ref()
        {
            fx::start_placed(&mut commands, &lib.system, USE_EFFECT, Bind::Fixed(Mat4::from_translation(pos)), Placement::default());
        }
        q.left -= 1.0;
        if q.left > 0.0 {
            continue;
        }
        done.push(i);
        let Some(row) = data.item(q.item) else { continue };
        // The target (0: every member): the item's effect on it. Sora is hit by target 0 and by his own id.
        let on_sora = q.target == 0 || q.target == SORA;
        if on_sora && let Some(lib) = lib.as_ref() {
            fx::start(&mut commands, &lib.system, row.stat_entry, Bind::Owner(body_e));
        }
        let mult = 100 + abilities.as_ref().map_or(0, |a| a.block().item_boost as i32);
        let amount = row.flag1 as u16 | (row.rank as u16) << 8;
        let mut v = Vitals {
            hp: s.hp,
            hp_max: s.stats.max_hp,
            hp_min: 0,
            mp: s.mp,
            mp_max: s.stats.max_mp,
            charge_left: magic.charge.left,
            charge_total: magic.charge.total,
            drive_fraction: drive.as_ref().map_or(0, |d| d.points.fraction),
            drive_points: drive.as_ref().map_or(0, |d| d.points.whole),
            drive_max: drive.as_ref().map_or(0, |d| d.points.max),
        };
        let (hp0, mp0) = (v.hp, v.mp);
        if on_sora {
            item_effect(q.item, amount, mult, &mut v, data.charge_feed);
            s.hp = v.hp;
            s.mp = v.mp;
            magic.charge.left = v.charge_left;
            magic.charge.total = v.charge_total;
            if let Some(d) = drive.as_mut() {
                d.points.fraction = v.drive_fraction;
                d.points.whole = v.drive_points;
            }
        }
        // The other members (FUN_001a11a0 -> FUN_001a12a0 on their status blocks); a KO'd member gets up through the HP
        // method (`use_on_members`).
        let mut field: Vec<FieldMember> = party
            .iter()
            .map(|m| FieldMember {
                neo: m.neo,
                ko: m.ko,
                v: Vitals { hp: m.hp, hp_max: m.hp_max, hp_min: 0, mp: m.mp, mp_max: m.mp_max, charge_left: m.charge.left, charge_total: m.charge.total, ..Default::default() },
            })
            .collect();
        let hit = use_on_members(q.item, q.target, amount, mult, data.charge_feed, data.revive_scale, &mut field, &mut progress);
        for (i, (mut m, f)) in party.iter_mut().zip(&field).enumerate() {
            if hit.contains(&i) {
                info!("items: used {} on {}: HP {} -> {}, MP {} -> {}", data.name(q.item), m.model, m.hp, f.v.hp, m.mp, f.v.mp);
            }
            m.hp = f.v.hp;
            m.ko = f.ko;
            m.mp = f.v.mp;
            m.charge.left = f.v.charge_left;
            m.charge.total = f.v.charge_total;
        }
        info!(
            "items: used {}: HP {hp0} -> {}, MP {mp0} -> {}, count {} -> {} (amount {amount}, mult {mult}, effect {} on member {} (0 = all: Sora), tick {})",
            data.name(q.item),
            s.hp,
            s.mp,
            q.count.0,
            q.count.1,
            row.stat_entry,
            q.target,
            tick
        );
    }
    for i in done.into_iter().rev() {
        uses.queue.remove(i);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A command row with the fields the lists read; `Flags` bit 5 (no global lock) like the disc's.
    fn cmd(id: u16, execute: u16, sub_menu: i8) -> Cmd {
        Cmd {
            id,
            execute,
            argument: 0,
            sub_menu,
            cmd_icon: 0,
            message_id: 0,
            flags: 0x31,
            range: -1.0,
            dir: 0.0,
            dir_range: -1.0,
            cost: 0,
            cmd_camera: 0,
            priority: 100,
            cmd_receiver: 0,
            time: 0,
            require: 0,
            mark: 1,
            cmd_action: 0,
            reaction_count: 0,
            dist_range: 0,
            score: 0,
            disable_form: 0,
            group: 0,
            reserve: 0,
        }
    }

    /// Potion (item 1, command 23), Ether (item 3, command 21), Valor's item 26 (command 26), Fire 100.
    fn table() -> Vec<Cmd> {
        let mut v = vec![
            cmd(1, 1, -1),
            cmd(2, 2, 1),
            cmd(3, 3, 2),
            cmd(4, 4, 3),
            cmd(5, 5, -1),
            cmd(cmd_id::SUMMON, cmd_id::SUMMON, 7),
            cmd(cmd_id::PARTY, cmd_id::PARTY, 8),
            cmd(cmd_id::LIMIT, cmd_id::LIMIT, 9),
            cmd(cmd_id::EMPTY, 0, -1),
            cmd(23, 3, 4),
            cmd(21, 3, 4),
            cmd(42, 42, -1),
            cmd(100, 2, -1),
            cmd(26, 4, -1),
            cmd(cmd_id::GENIE_DRIVE, cmd_id::GENIE_DRIVE, 3),
            cmd(535, 535, -1),
            cmd(536, 536, -1),
            cmd(537, 537, -1),
            cmd(538, 538, -1),
        ];
        v.iter_mut().find(|c| c.id == 26).unwrap().argument = 1;
        v
    }

    fn item_command(id: u16) -> u16 {
        match id {
            1 => 23,
            3 => 21,
            26 => 26,
            _ => 0,
        }
    }

    struct Setup {
        form: u8,
        battle: bool,
        spells: Vec<u16>,
        spell_status: u8,
        slots: Vec<(u16, u16)>,
        received: bool,
        forms: Vec<(u16, u8)>,
        genie: Option<Vec<u16>>,
    }

    impl Default for Setup {
        fn default() -> Self {
            Setup { form: 0, battle: false, spells: vec![100], spell_status: 0, slots: vec![(1, 0), (3, 0), (1, 0)], received: true, forms: vec![(26, 1)], genie: None }
        }
    }

    fn build(cmds: &[Cmd], s: &Setup, lists: &mut CommandLists) {
        let st = s.spell_status;
        let magic = move |_: &Cmd| st;
        let drive = |_: &Cmd| true;
        let party = |_: &Cmd| 0u8;
        let ctx = BuildCtx {
            cmds,
            status: StatusCtx { form: s.form, airborne: false, battle: s.battle, weapon: true, locked: false, revert_ok: true, magic: &magic, drive: &drive },
            spells: s.spells.clone(),
            battle_items: s.slots.clone(),
            item_command: &item_command,
            received: s.received,
            form_items: s.forms.clone(),
            form_party: &party,
            summons: Vec::new(),
            summon_active: false,
            genie_drive: s.genie.clone(),
            party: vec![(SORA, 42)],
            world: 2,
            charging_attack: false,
            is_sora: true,
        };
        lists.rebuild(&ctx);
        lists.select_root();
    }

    fn ids(l: &List) -> Vec<(u16, u8, u8)> {
        l.entries.iter().map(|e| (e.cmd, e.count, e.status)).collect()
    }

    /// FUN_001bc578 (the deck), FUN_001bc910 (items merged by command), FUN_001ba748 (the sub-menu
    /// statuses 5 and 1, the reserved-only status 6), FUN_001bca70 (flag 0x20), FUN_001793a8 (Revert).
    #[test]
    fn deck_and_items_follow_fun_001bc578_and_fun_001bc910() {
        let cmds = table();
        let mut l = CommandLists::default();
        build(&cmds, &Setup::default(), &mut l);
        assert_eq!(ids(&l.lists[list::ROOT]), vec![(1, 1, 0), (2, 1, 0), (3, 1, 0), (4, 1, 0)]);
        assert_eq!(ids(&l.lists[list::ITEMS]), vec![(23, 2, 0), (21, 1, 0)], "two Potion slots make one entry of count 2");
        assert_eq!(ids(&l.lists[list::TARGETS]), vec![(42, 1, 0)]);
        assert_eq!(l.stack, vec![list::ROOT]);
        // Nothing received yet: the Items bar is the empty command; no spell: Magic too; no form item: Drive.
        let mut l = CommandLists::default();
        build(&cmds, &Setup { received: false, spells: vec![], forms: vec![(26, 0)], ..Setup::default() }, &mut l);
        assert_eq!(ids(&l.lists[list::ROOT]), vec![(1, 1, 0), (cmd_id::EMPTY, 1, 5), (cmd_id::EMPTY, 1, 5), (cmd_id::EMPTY, 1, 5)]);
        assert_ne!(l.flags & 0x20, 0);
        // Every spell unusable: Magic keeps a choosable status 1; no item in the slots: Items 5.
        let mut l = CommandLists::default();
        build(&cmds, &Setup { spell_status: 5, slots: vec![], ..Setup::default() }, &mut l);
        assert_eq!(l.lists[list::ROOT].entries[1].status, status::NONE_USABLE);
        assert_eq!(l.lists[list::ROOT].entries[2].status, status::UNAVAILABLE);
        // A slot that only reserves Ether: count 0, status 6; a later carried Ether adds to it.
        let mut l = CommandLists::default();
        build(&cmds, &Setup { slots: vec![(0, 3), (3, 0)], ..Setup::default() }, &mut l);
        assert_eq!(ids(&l.lists[list::ITEMS]), vec![(21, 1, status::RESERVED)]);
        // In a form Sora's deck shows Revert, unavailable in Anti during a battle.
        let mut l = CommandLists::default();
        build(&cmds, &Setup { form: 3, ..Setup::default() }, &mut l);
        assert_eq!(l.lists[list::ROOT].entries[3].cmd, cmd_id::REVERT);
        build(&cmds, &Setup { form: 5, battle: true, ..Setup::default() }, &mut l);
        assert_eq!((l.lists[list::ROOT].entries[3].cmd, l.lists[list::ROOT].entries[3].status), (cmd_id::REVERT, 5));
        // The second deck (FUN_001bc6e0): Attack, the empty bar for Summon (no summon), Party, Limit.
        assert_eq!(l.lists[list::ROOT2].entries.iter().map(|e| e.cmd).collect::<Vec<_>>(), vec![1, cmd_id::EMPTY, cmd_id::PARTY, cmd_id::LIMIT]);
    }

    /// FUN_001bc6e0 with Genie out: the third entry of the second deck is "Drive!" (0x25d) in place of Party, its list is the
    /// forms list refilled with his commands (FUN_001bcf10), choosing one issues it, and `summon_request` turns it into
    /// `SummonRequest::Form`. Without Genie the deck and the forms list are as before.
    #[test]
    fn second_deck_shows_drive_with_genie_and_issues_the_form() {
        use crate::shared::summon::SummonRequest;
        let cmds = table();
        let genie = Setup { genie: Some(vec![535, 537]), ..Setup::default() };
        let mut l = CommandLists::default();
        build(&cmds, &genie, &mut l);
        assert_eq!(l.lists[list::ROOT2].entries.iter().map(|e| e.cmd).collect::<Vec<_>>(), vec![1, cmd_id::EMPTY, cmd_id::GENIE_DRIVE, cmd_id::LIMIT]);
        assert_eq!(ids(&l.lists[list::FORMS]), vec![(535, 1, 0), (537, 1, 0)], "the forms list holds his commands, not Valor's");
        assert_eq!(l.lists[list::ROOT2].entries[2].status, status::READY, "a non-empty sub-list: choosable");
        // Open the second deck, the Drive! entry and its list, then choose the second form.
        let inp = |f: fn(&mut MenuInput)| {
            let mut m = MenuInput::default();
            f(&mut m);
            m
        };
        let mut issued = Vec::new();
        let mut step = |l: &mut CommandLists, m: MenuInput| {
            build(&cmds, &genie, l);
            l.select_root();
            l.input(&cmds, m, &mut |c, t| {
                issued.push((c, t));
                true
            })
        };
        step(&mut l, inp(|m| m.left = true));
        assert_eq!(l.deck, list::ROOT2);
        step(&mut l, inp(|m| m.down = true));
        step(&mut l, inp(|m| m.down = true));
        let out = step(&mut l, inp(|m| m.confirm = true));
        assert_eq!((out.opened, l.stack.clone()), (Some(list::FORMS), vec![list::ROOT2, list::FORMS]));
        step(&mut l, inp(|m| m.down = true));
        let out = step(&mut l, inp(|m| m.confirm = true));
        assert_eq!(out.issue, Some((537, None)));
        assert_eq!(issued, vec![(537, None)]);
        assert_eq!(summon_request(find(&cmds, 537).unwrap(), 537), Some(SummonRequest::Form(537)));
        assert_eq!(summon_request(find(&cmds, cmd_id::SUMMON).unwrap(), cmd_id::SUMMON), Some(SummonRequest::Summon(cmd_id::SUMMON)));
        assert_eq!(summon_request(find(&cmds, 23).unwrap(), 23), None, "an item is not a summon request");
        // No form item owned: the entry is there but unavailable (empty list, FUN_001ba748).
        let none = Setup { genie: Some(vec![]), ..Setup::default() };
        let mut l = CommandLists::default();
        build(&cmds, &none, &mut l);
        assert_eq!((l.lists[list::ROOT2].entries[2].cmd, l.lists[list::ROOT2].entries[2].status), (cmd_id::GENIE_DRIVE, status::UNAVAILABLE));
        // Genie gone: Party again and the forms list is Valor's.
        let mut l = CommandLists::default();
        build(&cmds, &Setup::default(), &mut l);
        assert_eq!(l.lists[list::ROOT2].entries[2].cmd, cmd_id::PARTY);
        assert_eq!(ids(&l.lists[list::FORMS]), vec![(26, 1, 0)]);
    }

    /// Choosing a form in the Drive list issues its command, which becomes `DriveRequest::Enter(form)`; Revert becomes `Revert`;
    /// the Drive entry itself opens the list and issues nothing.
    #[test]
    fn drive_menu_selection_becomes_a_drive_request() {
        use crate::drive::DriveRequest;
        let cmds = table();
        let mut l = CommandLists::default();
        build(&cmds, &Setup::default(), &mut l);
        l.select_root();
        let mut issued = Vec::new();
        let mut step = |l: &mut CommandLists, m: MenuInput| {
            build(&cmds, &Setup::default(), l);
            l.select_root();
            l.input(&cmds, m, &mut |c, t| {
                issued.push((c, t));
                true
            })
        };
        let inp = |f: &dyn Fn(&mut MenuInput)| {
            let mut m = MenuInput::default();
            f(&mut m);
            m
        };
        step(&mut l, inp(&|m| m.down = true));
        step(&mut l, inp(&|m| m.down = true));
        step(&mut l, inp(&|m| m.down = true));
        let out = step(&mut l, inp(&|m| m.confirm = true));
        assert_eq!((out.opened, out.issue), (Some(list::FORMS), None), "Drive opens the forms list");
        let out = step(&mut l, inp(&|m| m.confirm = true));
        assert_eq!(out.issue, Some((26, None)));
        assert_eq!(drive_request(find(&cmds, 26).unwrap()), Some(DriveRequest::Enter(1)));
        assert_eq!(drive_request(find(&cmds, cmd_id::DRIVE).unwrap()), None);
        assert_eq!(drive_request(find(&cmds, cmd_id::REVERT).unwrap()), Some(DriveRequest::Revert));
        assert_eq!(drive_request(find(&cmds, 23).unwrap()), None);
    }

    /// The disc rows behind the "Drive!" list: the table at 0x37c036 names commands 535..538 for forms 1..=4, each an
    /// `Execute` of its own id with no sub-menu, and command 605's `Execute` is 605 opening list 3 (the forms list). Skipped without a disc.
    #[test]
    fn genie_drive_rows_and_table_come_from_the_disc() {
        let iso = std::env::var("KH2_ISO").ok();
        let Some(Ok(mut a)) = iso.map(|p| kh2_archive::Archive::open(&p)) else {
            eprintln!("skipped: no disc (set KH2_ISO)");
            return;
        };
        let cmds = kh2_tables::load::<Cmd>(&mut a, &t::CMD).expect("cmd").rows;
        let exe = ["SLUS_210.05", "SLUS_210.05;1"].iter().find_map(|p| a.iso().read(p).ok()).expect("executable");
        let table: Vec<u16> = (1..5).map(|i| exe_u16(&exe, 0x37c036 + 2 * i).unwrap()).collect();
        assert_eq!(table, vec![535, 536, 537, 538]);
        for c in &table {
            let row = find(&cmds, *c).expect("row");
            assert_eq!((row.execute, row.sub_menu), (*c, -1));
            assert!(cmd_id::GENIE_FORMS.contains(c));
        }
        let drive = find(&cmds, cmd_id::GENIE_DRIVE).expect("605");
        assert_eq!((drive.execute, drive.sub_menu), (605, list::FORMS as i16 as _));
    }

    /// FUN_001ba9b0 / FUN_001bb930 / FUN_001bc0c8: wrap both ways, a sub-menu opens on confirm, cancel
    /// goes back and resets the deck's cursor, an item goes through the target list, an issued command
    /// closes everything down to the deck with every cursor on the first entry.
    #[test]
    fn menu_input_follows_fun_001ba9b0() {
        let cmds = table();
        let setup = Setup::default();
        let mut l = CommandLists::default();
        build(&cmds, &setup, &mut l);
        let mut issued = Vec::new();
        let step = |l: &mut CommandLists, inp: MenuInput, issued: &mut Vec<(u16, Option<u16>)>| {
            build(&cmds, &setup, l);
            l.input(&cmds, inp, &mut |c, t| {
                issued.push((c, t));
                true
            })
        };
        let up = MenuInput { up: true, ..default() };
        let down = MenuInput { down: true, ..default() };
        let ok = MenuInput { confirm: true, ..default() };
        let back = MenuInput { cancel: true, ..default() };
        step(&mut l, up, &mut issued);
        assert_eq!(l.lists[list::ROOT].cursor, 3, "up from the first entry wraps to the last");
        step(&mut l, down, &mut issued);
        assert_eq!(l.lists[list::ROOT].cursor, 0, "down from the last wraps to the first");
        step(&mut l, down, &mut issued);
        let out = step(&mut l, ok, &mut issued);
        assert_eq!((out.opened, l.stack.clone()), (Some(list::MAGIC), vec![list::ROOT, list::MAGIC]));
        step(&mut l, back, &mut issued);
        assert_eq!(l.stack, vec![list::ROOT]);
        assert_eq!(l.lists[list::ROOT].cursor, 0, "going back to the deck resets its cursor (FUN_001ba6e8)");
        // Items -> Potion -> Sora.
        step(&mut l, down, &mut issued);
        step(&mut l, down, &mut issued);
        step(&mut l, ok, &mut issued);
        assert_eq!(l.stack, vec![list::ROOT, list::ITEMS]);
        let out = step(&mut l, ok, &mut issued);
        assert_eq!((out.opened, l.stack.len()), (Some(list::TARGETS), 3), "an item asks for its target (SubMenu 4)");
        let out = step(&mut l, ok, &mut issued);
        assert_eq!(out.issue, Some((23, Some(SORA))));
        assert_eq!(l.stack, vec![list::ROOT]);
        assert!(l.lists.iter().all(|x| x.cursor == 0));
        // A spell is issued from its list; the menu closes back to the deck.
        step(&mut l, down, &mut issued);
        step(&mut l, ok, &mut issued);
        let out = step(&mut l, ok, &mut issued);
        assert_eq!(out.issue, Some((100, None)));
        assert_eq!((l.stack.clone(), l.lists[list::ROOT].cursor), (vec![list::ROOT], 0));
        assert_eq!(issued, vec![(23, Some(SORA)), (100, None)]);
        // The empty bar buzzes and issues nothing.
        let mut l = CommandLists::default();
        let none = Setup { received: false, ..Setup::default() };
        build(&cmds, &none, &mut l);
        l.lists[list::ROOT].set_cursor(2);
        let out = l.input(&cmds, ok, &mut |_, _| panic!("nothing is issued"));
        assert_eq!(out.sounds, vec![Sound::Refused]);
        // An open list that became empty closes (FUN_001bacd0).
        let mut l = CommandLists::default();
        build(&cmds, &setup, &mut l);
        l.lists[list::ROOT].set_cursor(2);
        l.input(&cmds, ok, &mut |_, _| true);
        assert_eq!(l.stack, vec![list::ROOT, list::ITEMS]);
        build(&cmds, &Setup { slots: vec![], ..Setup::default() }, &mut l);
        let out = l.input(&cmds, MenuInput::default(), &mut |_, _| true);
        assert_eq!((l.stack.clone(), out.sounds), (vec![list::ROOT], vec![Sound::Close]));
        // Left: back one list, or at the deck the other deck.
        let out = l.input(&cmds, MenuInput { left: true, ..default() }, &mut |_, _| true);
        assert_eq!((l.deck, out.sounds), (list::ROOT2, vec![Sound::Deck]));
    }

    /// FUN_001badc8: the cursor follows its command when the list is rebuilt.
    #[test]
    fn cursor_follows_its_command() {
        let e = |cmd: u16| Entry { cmd, ..default() };
        let mut l = List { entries: vec![e(23), e(21), e(5)], ..default() };
        l.set_cursor(1);
        l.entries.remove(0);
        l.restore_cursor();
        assert_eq!((l.cursor, l.remembered), (0, Some(21)), "moved to the entry that has its command");
        l.entries = vec![e(7), e(8)];
        l.restore_cursor();
        assert_eq!((l.cursor, l.remembered), (0, Some(7)), "command gone: stays, remembers the new one");
        l.set_cursor(1);
        l.entries.pop();
        l.restore_cursor();
        assert_eq!((l.cursor, l.remembered), (0, None), "past the end: reset");
    }

    #[test]
    fn the_target_list_holds_sora_then_the_members_by_slot() {
        // The table at 0x37bab0 by member id: Sora 42; Donald and Goofy have their own target commands.
        let mut cmds = vec![0u16; 16];
        cmds[1] = 42;
        cmds[2] = 43;
        cmds[3] = 44;
        assert_eq!(target_party(&cmds, [(1u8, 3u16), (0, 2)].into_iter()), vec![(1, 42), (2, 43), (3, 44)]);
        assert_eq!(target_party(&cmds, std::iter::empty()), vec![(1, 42)]);
        // A member with no command in the table is not offered.
        assert_eq!(target_party(&cmds, [(0u8, 9u16)].into_iter()), vec![(1, 42)]);
    }

    #[test]
    fn healing_another_member_counts_once_for_the_report() {
        // FUN_001a11a0 answers target != user; a group item counts when any other slot holds a member.
        assert!(counts_as_healed(2, &[2, 3]));
        assert!(counts_as_healed(3, &[2, 3]));
        assert!(!counts_as_healed(1, &[2, 3]), "Sora on himself counts nothing");
        assert!(!counts_as_healed(2, &[3]), "a member who left is not healed");
        assert!(counts_as_healed(0, &[2]));
        assert!(!counts_as_healed(0, &[]), "a group item with Sora alone counts nothing");
        // The counter itself: Progress::count_healed adds one per call.
        let mut p = Progress::from_block(&[], std::sync::Arc::new(crate::progress::ItemIndex::default()));
        let n = p.healed_count();
        p.count_healed();
        assert_eq!(p.healed_count(), n + 1);
    }

    #[test]
    fn a_potion_heals_donald_from_his_own_maximum() {
        // Donald at 10 of 30 HP: Potion (item 1, amount 40) adds (30 * 40 * 100 + 9999) / 10000 = 12.
        let mut v = Vitals { hp: 10, hp_max: 30, ..Default::default() };
        assert!(item_effect(1, 40, 100, &mut v, 12.0));
        assert_eq!(v.hp, 22);
        assert!(item_effect(1, 40, 100, &mut v, 12.0));
        assert_eq!(v.hp, 30, "held at the maximum");
    }

    /// The member-heal path of `run`: a Potion thrown at Donald heals him from his own maximum, leaves Goofy and a
    /// KO'd member alone, and counts "Healed Party Members" once.
    #[test]
    fn a_potion_on_donald_heals_him_and_counts_once() {
        let mut p = Progress::from_block(&[], std::sync::Arc::new(crate::progress::ItemIndex::default()));
        let m = |neo, hp, ko| FieldMember { neo, ko, v: Vitals { hp, hp_max: 30, ..Default::default() } };
        let mut field = [m(2, 10, false), m(3, 10, false)];
        let n = p.healed_count();
        assert_eq!(use_on_members(1, 2, 40, 100, 12.0, (0.5, 1.0), &mut field, &mut p), vec![0]);
        assert_eq!((field[0].v.hp, field[1].v.hp), (22, 10), "Donald healed by 12, Goofy untouched");
        assert_eq!(p.healed_count(), n + 1, "counted once");
        // A group item reaches both; one count for the use.
        use_on_members(1, 0, 40, 100, 12.0, (0.5, 1.0), &mut field, &mut p);
        assert_eq!((field[0].v.hp, field[1].v.hp), (30, 22));
        assert_eq!(p.healed_count(), n + 2);
        // A KO'd member gets up through the HP method: the gain 12 is scaled by sstm +0x54 (0.5 here) and the
        // use counts (FUN_001a11a0 answered true).
        let mut field = [m(2, 0, true)];
        use_on_members(1, 2, 40, 100, 12.0, (0.5, 1.0), &mut field, &mut p);
        assert_eq!((field[0].v.hp, field[0].ko), (6, false));
        assert_eq!(p.healed_count(), n + 3);
        // Sora on himself changes no member and counts nothing.
        let mut field = [m(2, 10, false)];
        use_on_members(1, SORA, 40, 100, 12.0, (0.5, 1.0), &mut field, &mut p);
        assert_eq!(field[0].v.hp, 10);
        assert_eq!(p.healed_count(), n + 3);
    }

    /// FUN_001a12a0's sums: a percent of the maximum rounded up, HP held at the maximum, MP fed into a
    /// running charge, the drive gauge in hundredths.
    #[test]
    fn item_effect_follows_fun_001a12a0() {
        let base = Vitals { hp: 50, hp_max: 120, hp_min: 0, mp: 10, mp_max: 100, drive_max: 3, ..default() };
        let mut v = base;
        assert!(item_effect(1, 40, 100, &mut v, 1.0));
        assert_eq!(v.hp, 98, "Potion: 40% of 120 rounded up is 48");
        let mut v = Vitals { hp: 100, ..base };
        item_effect(1, 40, 150, &mut v, 1.0);
        assert_eq!(v.hp, 120, "held at the maximum");
        let mut v = base;
        item_effect(3, 100, 100, &mut v, 1.0);
        assert_eq!((v.hp, v.mp), (50, 100), "Ether: MP only");
        let mut v = base;
        item_effect(4, 255, 100, &mut v, 1.0);
        assert_eq!((v.hp, v.mp), (120, 100), "Elixir: both");
        let mut v = Vitals { charge_left: 300.0, charge_total: 400.0, mp: 0, ..base };
        item_effect(3, 30, 100, &mut v, 2.0);
        assert_eq!((v.mp, v.charge_left), (0, 240.0), "during the charge an MP gain shortens it");
        let mut v = Vitals { charge_left: 20.0, charge_total: 400.0, mp: 0, ..base };
        item_effect(3, 30, 100, &mut v, 2.0);
        assert_eq!((v.mp, v.charge_total), (100, 0.0), "a charge brought to 0 ends and refills MP");
        let mut v = Vitals { drive_fraction: 50, drive_points: 1, ..base };
        item_effect(item_id::DRIVE_RECOVERY, 1, 100, &mut v, 1.0);
        assert_eq!((v.drive_points, v.drive_fraction), (2, 50));
        assert!(!item_effect(0x114, 1, 100, &mut base.clone(), 1.0), "the boosts are not run here");
    }

    /// The oracle: the game's FUN_001a12a0 in kh2-ee on the user's executable, for every item row
    /// below the boosts, over HP, MP, charge and drive states and Item Boost multipliers, against
    /// `item_effect`. `FUN_0019fe48` (the member's status record) is replaced by a stub that returns
    /// a status record laid out in free memory; the record has no object (+0x24c = 0), so the HP and
    /// MP adds take FUN_001a0100 / FUN_001a0180, the same sums the player object's methods end in
    /// (FUN_00176c98 -> FUN_00186238 -> FUN_001a0100; FUN_00186368 -> FUN_001a0180).
    #[test]
    fn item_effect_matches_the_game() {
        use kh2_ee::Ee;
        const STACK: u32 = 0x01ff_0000;
        const ITEM_PTR: u32 = 0x01c9_5588;
        const ITEM_AT: u32 = 0x0180_0000;
        const STATUS: u32 = 0x0190_0000;
        const SSTM: u32 = 0x0191_0000;
        let iso = std::env::var("KH2_ISO").ok().or_else(|| {
            let f = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../local/iso.txt");
            std::fs::read_to_string(f).ok().map(|s| s.trim().to_string())
        });
        let Some(iso) = iso else {
            eprintln!("skipped: no disc (set KH2_ISO)");
            return;
        };
        let Ok(mut a) = kh2_archive::Archive::open(&iso) else {
            eprintln!("skipped: {iso}");
            return;
        };
        let items = kh2_tables::load::<t::Item>(&mut a, &t::ITEM).expect("item").rows;
        let blob = kh2_tables::subfile(&mut a, &t::ITEM).expect("item table");
        let feed = kh2_tables::player::load_sstm(&mut a).expect("sstm").f32(0x7c);
        let elf = ["SLUS_210.05", "SLUS_210.05;1"].iter().find_map(|p| a.iso().read(p).ok()).expect("executable");
        let mut ee = Ee::new();
        ee.load_elf(&elf).expect("elf");
        ee.write(ITEM_AT, &blob);
        ee.w32(ITEM_PTR, ITEM_AT);
        // DAT_00360130: the sstm record; only +0x7c is read here.
        ee.write(SSTM, &[0u8; 0x200]);
        ee.write(SSTM + 0x7c, &feed.to_le_bytes());
        ee.w32(0x0036_0130, SSTM);
        // FUN_0019fe48 -> `lui v0, hi; jr ra; ori v0, v0, lo`.
        ee.w32(0x0019_fe48, 0x3c02_0000 | (STATUS >> 16));
        ee.w32(0x0019_fe4c, 0x03e0_0008);
        ee.w32(0x0019_fe50, 0x3442_0000 | (STATUS & 0xffff));
        let states = [
            Vitals { hp: 1, hp_max: 120, hp_min: 0, mp: 0, mp_max: 100, charge_left: 0.0, charge_total: 0.0, drive_fraction: 0, drive_points: 0, drive_max: 3 },
            Vitals { hp: 90, hp_max: 120, hp_min: 0, mp: 95, mp_max: 100, charge_left: 0.0, charge_total: 0.0, drive_fraction: 70, drive_points: 2, drive_max: 3 },
            Vitals { hp: 33, hp_max: 37, hp_min: 1, mp: 0, mp_max: 120, charge_left: 500.0, charge_total: 600.0, drive_fraction: 99, drive_points: 3, drive_max: 3 },
            Vitals { hp: 200, hp_max: 300, hp_min: 0, mp: 0, mp_max: 200, charge_left: 3.0, charge_total: 600.0, drive_fraction: 10, drive_points: 0, drive_max: 9 },
        ];
        let mut checked = 0;
        for row in items.iter().filter(|r| r.id < 0x114) {
            let amount = row.flag1 as u16 | (row.rank as u16) << 8;
            let ptr = ee.call(0x001a_66e8, &[row.id as u64], &[], STACK).expect("FUN_001a66e8") as u32;
            if ptr == 0 {
                continue;
            }
            for st in &states {
                for mult in [100, 125, 150] {
                    ee.write(STATUS, &[0u8; 0x260]);
                    ee.w32(STATUS, st.hp as u32);
                    ee.w32(STATUS + 4, st.hp_max as u32);
                    ee.w32(STATUS + 8, st.hp_min as u32);
                    ee.w32(STATUS + 0x180, st.mp as u32);
                    ee.w32(STATUS + 0x184, st.mp_max as u32);
                    ee.write(STATUS + 0x1b0, &[st.drive_fraction, st.drive_points, st.drive_max]);
                    ee.write(STATUS + 0x1bc, &st.charge_left.to_le_bytes());
                    ee.write(STATUS + 0x1c0, &st.charge_total.to_le_bytes());
                    ee.w32(STATUS + 0x248, 100);
                    ee.call(0x001a_12a0, &[ptr as u64, 1, 1, mult as u64], &[], STACK).expect("FUN_001a12a0");
                    let mut port = *st;
                    item_effect(row.id, amount, mult, &mut port, feed);
                    let game = Vitals {
                        hp: ee.r32(STATUS) as i32,
                        hp_max: st.hp_max,
                        hp_min: st.hp_min,
                        mp: ee.r32(STATUS + 0x180) as i32,
                        mp_max: st.mp_max,
                        charge_left: ee.f32_at(STATUS + 0x1bc),
                        charge_total: ee.f32_at(STATUS + 0x1c0),
                        drive_fraction: ee.r8(STATUS + 0x1b0),
                        drive_points: ee.r8(STATUS + 0x1b1),
                        drive_max: st.drive_max,
                    };
                    assert_eq!(port, game, "item {} amount {amount} mult {mult} from {st:?}", row.id);
                    checked += 1;
                }
            }
        }
        assert!(checked > 100);
        eprintln!("oracle: FUN_001a12a0 equal for {checked} cases (items below 0x114, 4 states, 3 multipliers; sstm +0x7c = {feed})");
    }

    /// `FUN_001a0210` -> `FUN_00186368`: a KO'd object's MP does not change (an Ether does nothing), while the
    /// Elixir-like items change the HP first, which revives, so their MP lands too.
    #[test]
    fn mp_items_leave_a_ko_member_alone_unless_the_hp_revives_him() {
        let down = Vitals { hp: 0, hp_max: 30, mp: 5, mp_max: 40, ..Default::default() };
        let mut v = down;
        assert!(item_effect(3, 100, 100, &mut v, 12.0));
        assert_eq!((v.hp, v.mp), (0, 5), "Ether on a KO'd member");
        let mut v = down;
        item_effect(6, 100, 100, &mut v, 12.0);
        assert_eq!((v.hp, v.mp), (0, 5), "Mega-Ether on a KO'd member");
        let mut v = down;
        item_effect(4, 100, 100, &mut v, 12.0);
        assert_eq!((v.hp, v.mp), (30, 40), "Elixir: HP first, then the MP of the same use, held at the maximum");
        let mut v = down;
        item_effect(0x83, 100, 100, &mut v, 12.0);
        assert_eq!((v.hp, v.mp), (30, 40), "Hi-Elixir-like full restore");
        let mut v = down;
        item_effect(1, 40, 100, &mut v, 12.0);
        assert_eq!((v.hp, v.mp), (12, 5), "a Potion revives and leaves the MP");
        // A living member takes the Ether as before, and a charge running on a KO'd one is not fed.
        let mut v = Vitals { hp: 7, ..down };
        item_effect(3, 100, 100, &mut v, 12.0);
        assert_eq!(v.mp, 40);
        let mut v = Vitals { charge_left: 50.0, charge_total: 100.0, ..down };
        item_effect(3, 100, 100, &mut v, 12.0);
        assert_eq!(v.charge_left, 50.0);
    }

    /// `FUN_001a12a0` on a KO'd member in the field: an Ether does not change his MP, an Elixir revives him with the
    /// HP gain scaled and then lands its MP (`FUN_00186368` tests the KO bit after the HP method cleared it).
    #[test]
    fn field_items_on_a_ko_member_follow_the_hp_method() {
        let mut p = Progress::from_block(&[], std::sync::Arc::new(crate::progress::ItemIndex::default()));
        let down = FieldMember { neo: 2, ko: true, v: Vitals { hp: 0, hp_max: 30, mp: 5, mp_max: 40, ..Default::default() } };
        let mut field = [down];
        use_on_members(3, 2, 100, 100, 12.0, (0.5, 1.0), &mut field, &mut p);
        assert_eq!((field[0].v.hp, field[0].v.mp, field[0].ko), (0, 5, true), "Ether: still down, MP unchanged");
        let mut field = [down];
        use_on_members(4, 2, 100, 100, 12.0, (0.5, 1.0), &mut field, &mut p);
        assert_eq!((field[0].v.hp, field[0].v.mp, field[0].ko), (15, 40, false), "Elixir: 30 * 0.5, then the MP");
    }
}
