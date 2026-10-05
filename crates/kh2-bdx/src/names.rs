//! Names and signatures of the host functions that have been analysed (docs/bdx.md lists the evidence
//! for each), and names of the events the game raises. Other functions keep their numbers.
//!
//! A host function is `group.index`. `args` is the number of words popped; `ret` says a word is pushed.
//! The signature table is the analysis result; [`crate::exe::HostTable`] reads the game's own table and the
//! tests check the two agree.

pub struct HostFn {
    pub group: u16,
    pub index: u16,
    pub name: &'static str,
    pub args: u8,
    pub ret: bool,
}

const fn h(group: u16, index: u16, name: &'static str, args: u8, ret: bool) -> HostFn {
    HostFn { group, index, name, args, ret }
}

pub static HOST_FUNCS: &[HostFn] = &[
    // group 0: math and random numbers, pure functions
    h(0, 3, "frame_time", 0, true),
    h(0, 4, "vec_add", 2, true),
    h(0, 5, "vec_sub", 2, true),
    h(0, 6, "vec_length", 1, true),
    h(0, 7, "vec_normalize", 1, true),
    h(0, 13, "vec_rotate_y", 2, true),
    h(0, 16, "rand_int", 1, true),
    h(0, 17, "rand_float", 1, true),
    h(0, 18, "rand_range", 2, true),
    h(0, 36, "vec_scale", 2, true),
    h(0, 79, "release_effect", 1, false),
    // group 1: the object being scripted ("self")
    h(1, 3, "player", 0, true),
    h(1, 4, "face_direction", 2, false),
    h(1, 5, "modes_clear", 1, false),
    h(1, 6, "modes_register", 12, false),
    h(1, 7, "modes_attach", 2, false),
    h(1, 8, "set_mode", 2, false),
    h(1, 9, "queue_mode", 2, false),
    h(1, 10, "mode_pending", 1, true),
    h(1, 11, "play_motion", 3, false),
    h(1, 12, "play_motion_checked", 3, false),
    h(1, 14, "motion_done", 1, true),
    h(1, 15, "motion_state", 1, true),
    h(1, 19, "fade_out", 2, false),
    h(1, 20, "fade_in", 2, false),
    h(1, 23, "global_byte_33e830", 0, true),
    h(1, 28, "hide", 1, false),
    h(1, 34, "event_active", 0, true),
    h(1, 39, "spawn_param", 2, true),
    h(1, 55, "start_move", 5, false),
    h(1, 56, "on_screen", 1, true),
    h(1, 57, "airborne", 1, true),
    h(1, 60, "busy", 1, true),
    h(1, 61, "play_motion_at", 4, false),
    h(1, 62, "obj_vec_860", 1, true),
    h(1, 64, "obj_vec_850", 1, true),
    h(1, 65, "set_flag_26", 1, false),
    h(1, 67, "motion_length", 2, true),
    h(1, 68, "motion_end", 2, true),
    h(1, 70, "set_flag", 2, false),
    h(1, 71, "clear_flag", 2, false),
    h(1, 78, "hit_stun_b", 1, true),
    h(1, 79, "face_toward", 2, false),
    h(1, 80, "turn_toward", 3, true),
    h(1, 87, "spawn_attached", 4, true),
    h(1, 89, "set_velocity", 3, false),
    h(1, 92, "ground_marker", 2, false),
    h(1, 104, "set_flag_768", 1, false),
    h(1, 105, "clear_flag_768", 1, false),
    h(1, 110, "spawn_kind", 1, true),
    h(1, 111, "set_move", 3, false),
    h(1, 119, "probe_ground", 4, true),
    h(1, 120, "target_position", 1, true),
    h(1, 121, "set_target_mode", 3, false),
    h(1, 124, "flag_124", 1, true),
    h(1, 127, "modes_reset", 1, false),
    h(1, 128, "mover_method", 2, false),
    h(1, 131, "free_target", 1, true),
    h(1, 133, "obj_method_44", 1, false),
    h(1, 134, "obj_method_48", 1, false),
    h(1, 147, "position", 1, true),
    h(1, 168, "start_move_alt", 5, false),
    h(1, 171, "global_flag_4", 0, true),
    h(1, 201, "facing_vector", 1, true),
    h(1, 206, "global_flag_80", 0, true),
    h(1, 301, "global_byte_301", 0, true),
    h(1, 309, "flag_309", 1, true),
    // group 2: enemy-side functions
    h(2, 5, "hit_stun", 1, true),
    h(2, 8, "hit_is_real", 1, true),
    h(2, 9, "set_obj_param", 3, false),
    h(2, 20, "request_2_20", 2, true),
    h(2, 23, "target", 1, true),
    h(2, 88, "last_attacker", 1, true),
];

pub fn lookup(group: u16, index: u16) -> Option<&'static HostFn> {
    HOST_FUNCS.iter().find(|f| f.group == group && f.index == index)
}

pub fn host_name(group: u16, index: u16) -> Option<&'static str> {
    lookup(group, index).map(|f| f.name)
}

/// Events the game starts in a script. The numbers come from the call sites in the executable.
pub fn event_name(id: u32) -> &'static str {
    match id {
        0 => "init",
        1 => "main",
        6 => "damage",
        11 => "event_0b",
        13 => "event_begin",
        14 => "event_end",
        15 => "event_0f",
        16 => "event_10",
        19 => "event_13",
        24 => "event_18",
        25 => "event_19",
        26 => "event_1a",
        27 => "event_1b",
        _ => "?",
    }
}
