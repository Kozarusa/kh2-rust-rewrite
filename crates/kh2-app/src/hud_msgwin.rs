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
//! The port draws one window at a time from the `MsgWindow` resource. Not ported: closing (states
//! 4 to 7: prompt group 0x42, the closing group, and the button that triggers it), page breaks
//! (only the first page of a message is shown), the selection-slot prompt (0x1aa128, used when
//! the text has 0x15 slots), the speech tail of 0x1ab9e0 (0x1aa628: group 47 at x + width x
//! {-0.5, -0.15, 0.15, 0.5}[n], y -/+ height / 2), and the child window of 0x1ab708/0x1abad0.

use bevy::prelude::*;
use kh2_image::text::{Style, Token};
use kh2_image::window::{self, Phase, CLASSES, PROMPT_GROUP};

use super::hud_gfx::{Edge, HudScene, Node};
use super::hud_text::Label;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum State {
    #[default]
    Off,
    Opening,
    Typing,
    Waiting,
}

#[derive(Resource, Default)]
pub struct WindowNodes {
    frame: Option<usize>,
    prompt: Option<usize>,
    /// The pointer arrow a message's 0x0f command starts (`pointer_of`).
    pointer: Option<usize>,
    state: State,
    shown: Option<Request>,
    /// Text box size (even pixels) of the request shown.
    text_box: [i32; 2],
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

pub fn update(mut win: ResMut<MsgWindow>, mut nodes: ResMut<WindowNodes>, mut scene: ResMut<HudScene>, mut forced_done: Local<bool>) {
    let Some(file) = scene.text.as_ref().and_then(|t| t.frame_file) else { return };
    if !*forced_done {
        *forced_done = true;
        if let Some(r) = forced(&scene) {
            win.0 = Some(r);
        }
    }
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
    // A new request (re)opens the window; none closes it at once.
    if win.0 != nodes.shown {
        nodes.shown = win.0.clone();
        scene.nodes[frame].label = None;
        scene.nodes[prompt].visible = false;
        scene.nodes[pointer].visible = false;
        match nodes.shown.clone() {
            Some(r) => {
                let c = &CLASSES[r.class];
                let style = style(&scene, r.class);
                let text = scene.text.as_ref().unwrap();
                // Measure the whole text (the typewriter does not shorten the box).
                let size = text.measure(&Label { tokens: r.tokens.clone(), style: Style { time: f32::MAX / 2.0, ..style }, offset: [0.0; 2] });
                nodes.text_box = window::text_box(size);
                let at = window::place(c, nodes.text_box);
                info!(
                    "hud: message window class {} (ctor {:#x}) frame kind {}: {:?} text box {:?} frame at {:?} inner {:?} prompt at {:?}",
                    r.class, c.ctor, c.kind, kh2_image::text::to_text(&r.tokens), nodes.text_box, at.frame, at.size, at.prompt
                );
                scene.nodes[frame].pos = [at.frame[0] as f32, at.frame[1] as f32];
                scene.nodes[frame].visible = true;
                scene.set_group(frame, window::frame_group(c.kind, Phase::Open));
                nodes.state = State::Opening;
            }
            None => {
                scene.nodes[frame].visible = false;
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
        color: text.map(|t| t.color(c.color)).unwrap_or([0x80; 4]),
        align: c.align,
        world: text.map(|t| t.world).unwrap_or(0),
        ..Style::default()
    }
}
