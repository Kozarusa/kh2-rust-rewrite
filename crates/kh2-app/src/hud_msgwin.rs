//! Message windows: a framed text box (WP-6D). Rules in `kh2_image::window` and docs/hud.md
//! ("Message windows").
//!
//! What the game does (`SLUS_210.05`): a window object (base 0x1aaf20, update 0x1ab170, draw
//! 0x1ab370) holds a text object and a sequence player for its frame. It starts the frame's
//! opening group (state 0) and, when that has played, the idle group (state 1); from state 1 the
//! text is drawn with the typewriter (0x1aac80 calls 0x1a80d8 at the window's x, y). When the text
//! object reports its page complete (bit 0 of its flags) the button prompt starts (group 0x41,
//! 0x1aa3a0) and the window waits (state 2). Every draw first stretches the frame's nine parts
//! around the measured text (0x2387c0 with the text object's +0x50 width, or 432 for the 0x1ab8e0
//! class, and +0x54 height), then draws the frame at its centre point, then the text, then the
//! prompt at the text box's bottom-right corner (0x1aa4e8).
//!
//! The port draws one window at a time from the `MsgWindow` resource. Windows opened by game code
//! through `ScriptWindow` (the talk, WP-E3) also run the button, the pages and the closing: in
//! state 2 the confirm or cancel button (logical pad bits 0x40c, 0x1aa430; J or K here [inferred,
//! as the tutorial]) plays sound 0x18 and either shows the next page (0x1aacd8 -> 0x1a8c78 ->
//! 0x190fa8: a page ends at text command 0x10; the window goes back to state 1) or closes: the
//! window's method +0 (0x1ab328 for class 3: prompt group 0x42, state 3), the owner's +0x20 (0x1aacb0:
//! state 4 and event 0x22), then 0x1ab170 states 4 to 7 (the tail's closing group, the frame's
//! closing group, gone). The speech tail of 0x1ab9e0 (0x1aa628(1, 1, 0): group 2 * (4 * 1 + 1) +
//! 0x25 = 47) is drawn from state 1 on at (x - w / 2, y - h / 2) (0x1aa6b8 with 0x35ea68[0] = -0.5,
//! w and h the text box). Not ported: the selection-slot prompt (0x1aa128, used when the text has
//! 0x15 slots) and the child window of 0x1ab708/0x1abad0.

use bevy::prelude::*;
use kh2_image::text::{Style, Token};
use kh2_image::window::{self, Phase, CLASSES, PROMPT_CLOSE_GROUP, PROMPT_GROUP};

use super::hud_gfx::{Edge, HudScene, Node};
use super::hud_text::{Label, TextSys};
use crate::audio::PlaySe;

/// The tutorial presenter (docs/hud.md "Tutorial windows"): opens windows from the game's own messages.
#[path = "hud_tutorial.rs"]
pub(crate) mod hud_tutorial;

/// A window to show: the message and the window class (index into `kh2_image::window::CLASSES`).
#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    pub tokens: Vec<Token>,
    pub class: usize,
}

/// The game state the window draws: `Some` shows the window, `None` hides it.
#[derive(Resource, Default, Debug)]
pub struct MsgWindow(pub Option<Request>);

/// A window game code opens by message id (the talk's `FUN_001a5a60`): the window looks the id up
/// (0x190c40), shows its pages and closes on the button; `closed` is set when it reached state 3
/// (what the opener waits for).
#[derive(Resource, Default, Debug)]
pub struct ScriptWindow {
    /// Open this message in this class (index into `CLASSES`).
    pub open: Option<(u32, usize)>,
    /// The window opened through `open` was closed by the player.
    pub closed: bool,
}

/// The speech tail of class 0x1ab9e0 (0x1aa628(1, 1, 0)): its group, and its closing group (0x1aa680:
/// the group plus one).
pub const TAIL_GROUP: usize = 2 * (4 + 1) + 0x25;

/// The constructor of the class with the speech tail (0x1ab9e0; 0x1aba58 builds the same).
pub const SPEECH_TAIL_CLASS: u32 = 0x1ab9e0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum State {
    #[default]
    Off,
    Opening,
    Typing,
    Waiting,
    /// States 4 and 5: the tail's closing group plays.
    ClosingTail,
    /// State 6: the frame's closing group plays.
    ClosingFrame,
}

#[derive(Resource, Default)]
pub struct WindowNodes {
    frame: Option<usize>,
    prompt: Option<usize>,
    /// The pointer arrow a message's 0x0f command starts (`pointer_of`).
    pointer: Option<usize>,
    /// The speech tail of class 3 (0x1ab9e0).
    tail: Option<usize>,
    state: State,
    shown: Option<Request>,
    /// Text box size (even pixels) of the request shown.
    text_box: [i32; 2],
    /// The pages of a `ScriptWindow` message and the one shown.
    pages: Option<(Vec<Vec<Token>>, usize)>,
    /// The class of a window in its closing states.
    closing: Option<usize>,
}

/// The pages of message bytes: a page ends at text command 0x10 (0x190fa8 walks the commands by
/// their sizes, 0x190f68), the message at 0. A 0x10 byte is a page end only where the walk reaches
/// it: the tokens of the bytes up to it equal the page's own tokens.
pub fn pages(b: &[u8]) -> Vec<Vec<Token>> {
    let mut out = Vec::new();
    let mut rest = b;
    while !rest.is_empty() {
        let page = kh2_image::text::tokens(rest);
        let end = (0..rest.len()).find(|&k| matches!(rest[k], 0 | 0x10) && kh2_image::text::tokens(&rest[..=k]) == page);
        out.push(page);
        match end {
            Some(k) if rest[k] == 0x10 => rest = &rest[k + 1..],
            _ => break,
        }
    }
    out
}

/// The pages of message `id` as 0x190c40 finds it (the system table, then the world's; a missing id
/// falls back to 0x8adc).
fn message_pages(text: &TextSys, id: u32) -> Option<Vec<Vec<Token>>> {
    let tables = [text.messages.as_ref(), text.world_messages.as_ref()];
    let b = tables.iter().flatten().find_map(|m| m.bytes(id)).or_else(|| text.messages.as_ref()?.bytes(0x8adc))?;
    Some(pages(b)).filter(|p| !p.is_empty())
}

/// `KH2_MSG=<hex message id>[:class]` opens a window with that message (system table first, then
/// the world's, as 0x190c40 searches) for screenshots; class 0 to 3 as in `CLASSES`.
fn forced(scene: &HudScene) -> Option<Request> {
    let v = std::env::var("KH2_MSG").ok()?;
    let mut p = v.split(':');
    let id = u32::from_str_radix(p.next()?.trim_start_matches("0x"), 16).ok()?;
    let class = p.next().and_then(|c| c.parse().ok()).unwrap_or(0usize).min(CLASSES.len() - 1);
    let tokens = scene.text.as_ref()?.lookup(id)?;
    Some(Request { tokens, class })
}

#[allow(clippy::too_many_arguments)]
pub fn update(
    mut win: ResMut<MsgWindow>,
    mut nodes: ResMut<WindowNodes>,
    mut scene: ResMut<HudScene>,
    mut forced_done: Local<bool>,
    mut script: Option<ResMut<ScriptWindow>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut se: MessageWriter<PlaySe>,
) {
    let Some(file) = scene.text.as_ref().and_then(|t| t.frame_file) else { return };
    let nodes = &mut *nodes;
    if !*forced_done {
        *forced_done = true;
        if let Some(r) = forced(&scene) {
            win.0 = Some(r);
        }
    }
    // A window opened by game code: its pages; the box is the largest page (0x191570).
    if let Some((id, class)) = script.as_mut().and_then(|s| s.open.take()) {
        match scene.text.as_ref().and_then(|t| message_pages(t, id)) {
            Some(p) => {
                win.0 = Some(Request { tokens: p[0].clone(), class: class.min(CLASSES.len() - 1) });
                nodes.pages = Some((p, 0));
                nodes.shown = None;
            }
            None => {
                warn!("hud: message {id:#x} not found; the window is not opened");
                if let Some(s) = script.as_mut() {
                    s.closed = true;
                }
            }
        }
    }
    let tail = *nodes.tail.get_or_insert_with(|| {
        let mut n = Node::new(file, None);
        n.visible = false;
        scene.add(n)
    });
    let frame = *nodes.frame.get_or_insert_with(|| {
        let mut n = Node::new(file, None);
        n.visible = false;
        scene.add(n)
    });
    let prompt = *nodes.prompt.get_or_insert_with(|| {
        let mut n = Node::new(file, None);
        n.visible = false;
        scene.add(n)
    });
    let pointer = *nodes.pointer.get_or_insert_with(|| {
        let mut n = Node::new(file, None);
        n.visible = false;
        scene.add(n)
    });
    // The closing states (4 to 7) run after the request is gone.
    if let Some(class) = nodes.closing {
        let c = &CLASSES[class];
        let at = window::place(c, nodes.text_box);
        if let Some(g) = scene.nodes[frame].player.group {
            scene.files[file].seq.resize_frame(g, at.size[0], at.size[1]);
        }
        match nodes.state {
            State::ClosingTail if !scene.nodes[tail].visible || scene.nodes[tail].player.finished() => {
                scene.nodes[tail].visible = false;
                scene.nodes[frame].label = None;
                scene.set_group(frame, window::frame_group(c.kind, Phase::Close));
                nodes.state = State::ClosingFrame;
            }
            State::ClosingFrame if scene.nodes[frame].player.finished() => {
                scene.nodes[frame].visible = false;
                scene.nodes[prompt].visible = false;
                nodes.closing = None;
                nodes.state = State::Off;
            }
            _ => {}
        }
        if win.0.is_none() {
            return;
        }
        // A new request ends the closing at once.
        nodes.closing = None;
    }
    // A new request (re)opens the window; none closes it at once.
    if win.0 != nodes.shown {
        // Another request (the tutorial's) replaces a game-opened window's pages.
        if let (Some((p, i)), Some(w)) = (&nodes.pages, &win.0)
            && p.get(*i) != Some(&w.tokens)
        {
            nodes.pages = None;
        }
        nodes.shown = win.0.clone();
        scene.nodes[frame].label = None;
        scene.nodes[prompt].visible = false;
        scene.nodes[pointer].visible = false;
        scene.nodes[tail].visible = false;
        match nodes.shown.clone() {
            Some(r) => {
                let c = &CLASSES[r.class];
                let style = style(&scene, r.class);
                let text = scene.text.as_ref().unwrap();
                // Measure the whole text (the typewriter does not shorten the box); with pages, the
                // largest page (0x191570 measures every page).
                let measure = |t: &[Token]| window::text_box(text.measure(&Label { tokens: t.to_vec(), style: Style { time: f32::MAX / 2.0, ..style.clone() }, offset: [0.0; 2] }));
                nodes.text_box = match &nodes.pages {
                    Some((p, _)) => p.iter().map(|t| measure(t)).fold([0, 0], |a, b| [a[0].max(b[0]), a[1].max(b[1])]),
                    None => measure(&r.tokens),
                };
                let at = window::place(c, nodes.text_box);
                info!(
                    "hud: message window class {} (ctor {:#x}) frame kind {}: {:?} text box {:?} frame at {:?} inner {:?} prompt at {:?}{}",
                    r.class,
                    c.ctor,
                    c.kind,
                    kh2_image::text::to_text(&r.tokens),
                    nodes.text_box,
                    at.frame,
                    at.size,
                    at.prompt,
                    nodes.pages.as_ref().map_or(String::new(), |(p, _)| format!(", {} page(s)", p.len()))
                );
                scene.nodes[frame].pos = [at.frame[0] as f32, at.frame[1] as f32];
                scene.nodes[frame].visible = true;
                scene.set_group(frame, window::frame_group(c.kind, Phase::Open));
                // The tail's group starts with the window (0x1aa628 in the constructor).
                if c.ctor == SPEECH_TAIL_CLASS {
                    scene.set_group(tail, TAIL_GROUP);
                }
                nodes.state = State::Opening;
            }
            None => {
                scene.nodes[frame].visible = false;
                nodes.pages = None;
                nodes.state = State::Off;
            }
        }
    }
    let Some(r) = nodes.shown.clone() else { return };
    let c = &CLASSES[r.class];
    let at = window::place(c, nodes.text_box);
    // Every draw stretches the frame around the text box (0x2387c0).
    if let Some(g) = scene.nodes[frame].player.group {
        scene.files[file].seq.resize_frame(g, at.size[0], at.size[1]);
    }
    // The tail is drawn from state 1 on at (x + w * -0.5, y - h / 2) (0x1ab370 -> 0x1aa6b8).
    if c.ctor == SPEECH_TAIL_CLASS && nodes.state != State::Opening {
        let n = &mut scene.nodes[tail];
        n.pos = [(at.frame[0] - nodes.text_box[0] / 2) as f32, (at.frame[1] - nodes.text_box[1] / 2) as f32];
        n.visible = true;
    }
    match nodes.state {
        State::Opening if scene.nodes[frame].player.finished() => {
            scene.set_group(frame, window::frame_group(c.kind, Phase::Idle));
            let style = style(&scene, r.class);
            let n = &mut scene.nodes[frame];
            // The text is drawn at the window's x, y; the node sits at the frame's centre.
            n.label = Some(Label { tokens: r.tokens.clone(), style, offset: [(at.text[0] - at.frame[0]) as f32, (at.text[1] - at.frame[1]) as f32] });
            n.label_time = 0.0;
            nodes.state = State::Typing;
            // The text's first draw runs its 0x0f command: the window's pointer (method +0x2c).
            if let Some((kind, x, y)) = pointer_of(&r.tokens) {
                let n = &mut scene.nodes[pointer];
                n.pos = [x as f32, y as f32];
                // Positions are in the 512 wide HUD space; the ring and deck sit at the screen edges (docs/hud.md, edge layout) [inferred: the pointer follows the side it is on].
                n.edge = if x > 256 { Edge::Right } else { Edge::Left };
                n.visible = true;
                scene.set_group(pointer, kind as usize + 1);
            }
        }
        State::Typing => {
            let n = &scene.nodes[frame];
            let done = match (&n.label, &scene.text) {
                (Some(l), Some(t)) => t.complete(&l.tokens, &l.style, n.label_time),
                _ => false,
            };
            if done {
                let p = &mut scene.nodes[prompt];
                p.pos = [at.prompt[0] as f32, at.prompt[1] as f32];
                p.visible = true;
                scene.set_group(prompt, PROMPT_GROUP);
                nodes.state = State::Waiting;
            }
        }
        // State 2 of a game-opened window: the button (0x1aa430, logical bits 0x40c; J or K here).
        State::Waiting if nodes.pages.is_some() && (keys.just_pressed(KeyCode::KeyJ) || keys.just_pressed(KeyCode::KeyK)) => {
            se.write(PlaySe::system(0x18));
            let (p, page) = nodes.pages.as_mut().expect("pages");
            if *page + 1 < p.len() {
                // 0x1aacd8 -> 0x1a8c78: the next page is typed (state 1 again).
                *page += 1;
                let tokens = p[*page].clone();
                let req = Request { tokens: tokens.clone(), class: r.class };
                win.0 = Some(req.clone());
                nodes.shown = Some(req);
                scene.nodes[prompt].visible = false;
                let n = &mut scene.nodes[frame];
                if let Some(l) = n.label.as_mut() {
                    l.tokens = tokens;
                }
                n.label_time = 0.0;
                nodes.state = State::Typing;
                info!("hud: message window page {} of {}", *page + 1, p.len());
            } else {
                // Method +0 (0x1ab328): prompt group 0x42, state 3, which the opener waits for; its
                // +0x20 (0x1aacb0) then starts the closing (state 4).
                scene.set_group(prompt, PROMPT_CLOSE_GROUP);
                if c.ctor == SPEECH_TAIL_CLASS {
                    scene.set_group(tail, TAIL_GROUP + 1);
                }
                nodes.closing = Some(r.class);
                nodes.state = State::ClosingTail;
                nodes.pages = None;
                nodes.shown = None;
                win.0 = None;
                if let Some(s) = script.as_mut() {
                    s.closed = true;
                }
                info!("hud: message window closed by the button (state 3, then the closing groups)");
            }
        }
        _ => {}
    }
}

/// The pointer a message starts: its first 0x0f command carries `(kind u8, x u16, y u16)` (0x1a8210
/// case 0xf calls the window's method +0x2c with them; its body 0x1aa858 starts group `kind + 1` of the
/// window sequence when `kind` is not 0 and stores x and y; the window's draw 0x1ab990 draws it there
/// through 0x1aa8c0). Returns `None` for kind 0 (no pointer).
pub fn pointer_of(tokens: &[Token]) -> Option<(u8, i32, i32)> {
    tokens.iter().find_map(|t| match t {
        Token::Callback(0x0f, a) if a.len() >= 5 && a[0] != 0 => Some((a[0], u16::from_le_bytes([a[1], a[2]]) as i32, u16::from_le_bytes([a[3], a[4]]) as i32)),
        _ => None,
    })
}

/// The text style of a window class: its font, size, colour (for the current world) and alignment.
fn style(scene: &HudScene, class: usize) -> Style {
    let c = &CLASSES[class];
    let text = scene.text.as_ref();
    Style {
        font: c.font,
        size: c.size,
        color: text.map(|t| t.color(c.color)).unwrap_or([0xff, 0xff, 0xff, 0x80]),
        align: c.align,
        world: text.map(|t| t.world).unwrap_or(0),
        ..Style::default()
    }
}
