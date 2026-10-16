//! Rhai filters: OBS filters written as Rhai scripts (`*.rhai`), hosted by
//! Theater Scene.
//!
//! A script declares its filter and computes; Theater does every OBS call.
//! That's what the Lua filters got wrong: an OBS Lua script runs holding its
//! own lock, and an OBS call from inside it (a source lookup in a tick) could
//! deadlock against OBS saving, or a reload against the render thread. Here a
//! script never calls OBS, source lookups happen on the UI thread only, each
//! call is capped (a runaway loop is stopped, not hung), and an edited script
//! is reloaded live (a broken edit leaves the last good version running).
//!
//! A filter script:
//!
//! ```text
//! const ID = "my_filter";              // its OBS filter id (existing scenes find it by it)
//! const NAME = "My Filter";            // its name in OBS's filter list
//! const BLEND = "premultiplied";       // optional: how its output is drawn ("normal" otherwise)
//! const SHADER = `…effect code…`;      // or SHADER_FILE = "my_filter.effect", next to the script
//!
//! fn defaults() { #{ strength: 1.0 } }                          // settings and their defaults
//! fn properties() { [ slider("strength", "Strength", 0.0, 2.0, 0.01) ] }
//! fn tick(dt) { #{ strength: this.settings.strength } }        // the shader's values this frame,
//!                                                              // or () to draw the source untouched
//! ```
//!
//! `this` is the filter's own state, kept between calls: `this.settings`
//! (its settings), `this.enabled`, `this.width` / `this.height` (what it
//! draws: its source, through the filters before it), `this.source_name`,
//! `this.source_width` / `this.source_height` (its source's own size),
//! `this.canvas_width` / `this.canvas_height`, `this.time` (seconds since it
//! was created), plus anything the script stores in it. An `fn update()`
//! runs when its settings change (and when it's created); a map it returns
//! is written to its settings.
//!
//! Properties: `slider`, `int_slider`, `int_input`, `toggle`,
//! `choice(key, label, [[label, value], …])`, `text`, `info`, and
//! `source_list(key, label, filter_id)` (the sources carrying that filter).
//! `remote(property, source_key, filter_id, remote_key)` links a property to
//! the setting `remote_key` of the `filter_id` filter on the source that the
//! source_list `source_key` names: changing it there changes that filter,
//! and picking a source copies that filter's value in (put the source_list
//! first: OBS runs them in order when the properties open).
//!
//! `const SIZE = "canvas"` makes it as big as the canvas (a 360° view)
//! rather than its source. Filters share values with `share(key, value)` /
//! `shared_value(key)`; `publish(#{ _key: value })` writes runtime values
//! into its own settings for others to read (keys start with `_`: Theater's
//! Studio leaves those alone); `fill_canvas()` stretches its source over
//! the whole canvas in the current scene. Top-level constants are read by
//! Theater; the functions don't see them.
//!
//! `const KIND = "source"` makes it an OBS source rather than a filter: no
//! SHADER; its `tick(dt)` returns what to draw — boxes, text and pictures,
//! see script_draw — at `this.size` ([w, h]), and `fn on_event(ev)` gets
//! each message from the relay (script_feed: chat, alerts…), and from the
//! WebSocket at `this.feed` when it sets one (`"ws://host:port"`, from its
//! settings in `update()`: a game or app sending its own messages; changing
//! it switches; `feed_connected(url)` says whether it's connected), `fn
//! on_button(key)` a click on a `button(key, label)` property. `supporters()`
//! lists your supporters (Twitch subscribers, Patreon patrons, Ko-fi
//! supporters, from the Theater API plugins: supporters). More property
//! kinds: `color(key, label)` (0xAARRGGBB), `font_list(key, label)` and
//! `file(key, label)` / `file(key, label, "Sounds (*.wav *.mp3)")` (a file
//! picker). `play_sound(path)` / `play_sound(path, volume)` plays a sound
//! file through the Throwables Layer, like redeem sounds (a relative path is
//! taken from the script's folder).
//!
//! `const KIND = "dock"` makes it a dock in OBS, drawn with egui: its `fn
//! ui(ui)` draws it, see script_docks.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::{CStr, CString, c_char, c_void};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::SystemTime;

use parking_lot::{Mutex, RwLock};
use rhai::{AST, Array, CallFnOptions, Dynamic, Engine, ImmutableString, Map, Scope};

use crate::obs_source::{ObsVideoInfo, obs_symbol, trust_log};

/// Most operations one script call may take: a runaway loop is stopped (in
/// tens of milliseconds; a source laying out a whole chat stays well under).
const MAX_OPERATIONS: u64 = 2_000_000;

/// Property helpers every filter script can call (see the module doc).
const PRELUDE: &str = r#"
fn slider(key, label, min, max, step) { #{ kind: "slider", key: key, label: label, min: min, max: max, step: step } }
fn int_slider(key, label, min, max, step) { #{ kind: "int_slider", key: key, label: label, min: min, max: max, step: step } }
fn int_input(key, label, min, max, step) { #{ kind: "int_input", key: key, label: label, min: min, max: max, step: step } }
fn source_list(key, label, filter_id) { #{ kind: "source_list", key: key, label: label, filter: filter_id } }
fn remote(prop, source_key, filter_id, remote_key) { prop.remote = #{ source: source_key, filter: filter_id, key: remote_key }; prop }
fn toggle(key, label) { #{ kind: "toggle", key: key, label: label } }
fn choice(key, label, options) { #{ kind: "choice", key: key, label: label, options: options } }
fn text(key, label) { #{ kind: "text", key: key, label: label } }
fn info(label) { #{ kind: "info", label: label } }
fn color(key, label) { #{ kind: "color", key: key, label: label } }
fn button(key, label) { #{ kind: "button", key: key, label: label } }
fn font_list(key, label) { #{ kind: "font_list", key: key, label: label } }
fn file(key, label) { #{ kind: "file", key: key, label: label, filter: "" } }
fn file(key, label, filter) { #{ kind: "file", key: key, label: label, filter: filter } }
"#;

/// A colour 0xAARRGGBB from 0–255 channels and an alpha of 0–1.
fn rgba(r: i64, g: i64, b: i64, a: f64) -> i64 {
    let a = (a.clamp(0.0, 1.0) * 255.0).round() as i64;
    (a << 24) | ((r & 255) << 16) | ((g & 255) << 8) | (b & 255)
}

/// `color` (0xAARRGGBB) with its alpha multiplied by `a` (0–1).
fn with_alpha(color: i64, a: f64) -> i64 {
    let alpha = ((color >> 24) & 255) as f64 * a.clamp(0.0, 1.0);
    (color & 0xFF_FFFF) | ((alpha.round() as i64) << 24)
}

/// A colour from CSS-style text: "#RGB", "#RRGGBB", "#AARRGGBB"-less "#RRGGBBAA"
/// (CSS order) or a few names; `fallback` when it isn't one.
fn parse_color(text: &str, fallback: i64) -> i64 {
    let t = text.trim();
    let named = match t.to_ascii_lowercase().as_str() {
        "white" => Some(0xFFFF_FFFF_u32),
        "black" => Some(0xFF00_0000),
        "red" => Some(0xFFFF_0000),
        "green" => Some(0xFF00_8000),
        "blue" => Some(0xFF00_00FF),
        "yellow" => Some(0xFFFF_FF00),
        "orange" => Some(0xFFFF_A500),
        "purple" => Some(0xFF80_0080),
        "pink" => Some(0xFFFF_C0CB),
        "cyan" | "aqua" => Some(0xFF00_FFFF),
        "magenta" | "fuchsia" => Some(0xFFFF_00FF),
        "gray" | "grey" => Some(0xFF80_8080),
        _ => None,
    };
    if let Some(c) = named {
        return c as i64;
    }
    let Some(hex) = t.strip_prefix('#') else { return fallback };
    let digits: Vec<u32> = hex.chars().filter_map(|c| c.to_digit(16)).collect();
    if digits.len() != hex.len() {
        return fallback;
    }
    let pair = |i: usize| digits[i] * 16 + digits[i + 1];
    let (r, g, b, a) = match digits.len() {
        3 => (digits[0] * 17, digits[1] * 17, digits[2] * 17, 255),
        4 => (digits[0] * 17, digits[1] * 17, digits[2] * 17, digits[3] * 17),
        6 => (pair(0), pair(2), pair(4), 255),
        8 => (pair(0), pair(2), pair(4), pair(6)),
        _ => return fallback,
    };
    ((a << 24) | (r << 16) | (g << 8) | b) as i64
}

// ─── OBS ────────────────────────────────────────────────────────────────────

type P = *mut c_void;

/// libobs' struct obs_source_info (OBS 31), up to get_properties2: the
/// type_data field and the *2 callbacks that receive it tell a callback
/// which script's filter it serves.
#[repr(C)]
struct SourceInfo {
    id: *const c_char,
    type_: u32,
    output_flags: u32,
    get_name: Option<unsafe extern "C" fn(P) -> *const c_char>,
    create: Option<unsafe extern "C" fn(P, P) -> P>,
    destroy: Option<unsafe extern "C" fn(P)>,
    get_width: Option<unsafe extern "C" fn(P) -> u32>,
    get_height: Option<unsafe extern "C" fn(P) -> u32>,
    get_defaults: Option<unsafe extern "C" fn(P)>,
    get_properties: Option<unsafe extern "C" fn(P) -> P>,
    update: Option<unsafe extern "C" fn(P, P)>,
    activate: Option<unsafe extern "C" fn(P)>,
    deactivate: Option<unsafe extern "C" fn(P)>,
    show: Option<unsafe extern "C" fn(P)>,
    hide: Option<unsafe extern "C" fn(P)>,
    video_tick: Option<unsafe extern "C" fn(P, f32)>,
    video_render: Option<unsafe extern "C" fn(P, P)>,
    filter_video: Option<unsafe extern "C" fn(P, P) -> P>,
    filter_audio: Option<unsafe extern "C" fn(P, P) -> P>,
    enum_active_sources: Option<unsafe extern "C" fn(P, P, P)>,
    save: Option<unsafe extern "C" fn(P, P)>,
    load: Option<unsafe extern "C" fn(P, P)>,
    mouse_click: Option<unsafe extern "C" fn()>,
    mouse_move: Option<unsafe extern "C" fn()>,
    mouse_wheel: Option<unsafe extern "C" fn()>,
    focus: Option<unsafe extern "C" fn()>,
    key_click: Option<unsafe extern "C" fn()>,
    filter_remove: Option<unsafe extern "C" fn(P, P)>,
    type_data: P,
    free_type_data: Option<unsafe extern "C" fn(P)>,
    audio_render: Option<unsafe extern "C" fn()>,
    enum_all_sources: Option<unsafe extern "C" fn(P, P, P)>,
    transition_start: Option<unsafe extern "C" fn(P)>,
    transition_stop: Option<unsafe extern "C" fn(P)>,
    get_defaults2: Option<unsafe extern "C" fn(P, P)>,
    get_properties2: Option<unsafe extern "C" fn(P, P) -> P>,
}

const OBS_SOURCE_TYPE_INPUT: u32 = 0;
const OBS_SOURCE_TYPE_FILTER: u32 = 1;
/// Draws itself (video_render without an effect): Rhai sources
const OBS_SOURCE_CUSTOM_DRAW: u32 = 1 << 3;
const OBS_SOURCE_VIDEO: u32 = 1 << 0;
const GS_RGBA: u32 = 3;
const OBS_NO_DIRECT_RENDERING: u32 = 0;
const GS_BLEND_ONE: u32 = 1;
const GS_BLEND_INVSRCALPHA: u32 = 5;
const OBS_COMBO_TYPE_LIST: u32 = 2;
const OBS_COMBO_FORMAT_INT: u32 = 1;
const OBS_COMBO_FORMAT_STRING: u32 = 3;
const OBS_TEXT_DEFAULT: u32 = 0;
const OBS_TEXT_INFO: u32 = 3;
const OBS_PATH_FILE: u32 = 0;
const OBS_TASK_UI: u32 = 0;
const OBS_ALIGN_TOP_LEFT: u32 = 1 | 4;
const OBS_BOUNDS_STRETCH: u32 = 1;

type EnumFiltersProc = unsafe extern "C" fn(P, P, P);
type EnumSourcesProc = unsafe extern "C" fn(P, P) -> bool;
type ModifiedProc = unsafe extern "C" fn(P, P, P, P) -> bool;
type TaskProc = unsafe extern "C" fn(P);
type ClickedProc = unsafe extern "C" fn(P, P, P) -> bool;

/// libobs' struct gs_effect_param_info
#[repr(C)]
struct ParamInfo {
    name: *const c_char,
    kind: i32,
}

struct Fns {
    register_source: unsafe extern "C" fn(*const SourceInfo, usize),
    source_get_id: unsafe extern "C" fn(P) -> *const c_char,
    source_get_display_name: unsafe extern "C" fn(*const c_char) -> *const c_char,
    source_enabled: unsafe extern "C" fn(P) -> bool,
    filter_get_target: unsafe extern "C" fn(P) -> P,
    source_get_base_width: unsafe extern "C" fn(P) -> u32,
    source_get_base_height: unsafe extern "C" fn(P) -> u32,
    process_filter_begin: unsafe extern "C" fn(P, u32, u32) -> bool,
    process_filter_end: unsafe extern "C" fn(P, P, u32, u32),
    skip_video_filter: unsafe extern "C" fn(P),
    enter_graphics: unsafe extern "C" fn(),
    leave_graphics: unsafe extern "C" fn(),
    effect_create: unsafe extern "C" fn(*const c_char, *const c_char, *mut *mut c_char) -> P,
    effect_destroy: unsafe extern "C" fn(P),
    effect_get_param_by_name: unsafe extern "C" fn(P, *const c_char) -> P,
    effect_get_num_params: unsafe extern "C" fn(P) -> usize,
    effect_get_param_by_idx: unsafe extern "C" fn(P, usize) -> P,
    effect_get_param_info: unsafe extern "C" fn(P, *mut ParamInfo),
    effect_set_val: unsafe extern "C" fn(P, *const c_void, usize),
    blend_state_push: unsafe extern "C" fn(),
    blend_state_pop: unsafe extern "C" fn(),
    blend_function_separate: unsafe extern "C" fn(u32, u32, u32, u32),
    properties_create: unsafe extern "C" fn() -> P,
    add_float_slider: unsafe extern "C" fn(P, *const c_char, *const c_char, f64, f64, f64) -> P,
    add_int_slider: unsafe extern "C" fn(P, *const c_char, *const c_char, i32, i32, i32) -> P,
    add_bool: unsafe extern "C" fn(P, *const c_char, *const c_char) -> P,
    add_list: unsafe extern "C" fn(P, *const c_char, *const c_char, u32, u32) -> P,
    list_add_int: unsafe extern "C" fn(P, *const c_char, i64) -> usize,
    list_add_string: unsafe extern "C" fn(P, *const c_char, *const c_char) -> usize,
    add_text: unsafe extern "C" fn(P, *const c_char, *const c_char, u32) -> P,
    data_get_double: unsafe extern "C" fn(P, *const c_char) -> f64,
    data_get_int: unsafe extern "C" fn(P, *const c_char) -> i64,
    data_get_bool: unsafe extern "C" fn(P, *const c_char) -> bool,
    data_get_string: unsafe extern "C" fn(P, *const c_char) -> *const c_char,
    data_set_default_double: unsafe extern "C" fn(P, *const c_char, f64),
    data_set_default_int: unsafe extern "C" fn(P, *const c_char, i64),
    data_set_default_bool: unsafe extern "C" fn(P, *const c_char, bool),
    data_set_default_string: unsafe extern "C" fn(P, *const c_char, *const c_char),
    get_video_info: unsafe extern "C" fn(*mut ObsVideoInfo) -> bool,
    filter_get_parent: unsafe extern "C" fn(P) -> P,
    source_get_name: unsafe extern "C" fn(P) -> *const c_char,
    source_get_settings: unsafe extern "C" fn(P) -> P,
    source_update: unsafe extern "C" fn(P, P),
    get_source_by_name: unsafe extern "C" fn(*const c_char) -> P,
    source_get_ref: unsafe extern "C" fn(P) -> P,
    source_release: unsafe extern "C" fn(P),
    source_enum_filters: unsafe extern "C" fn(P, EnumFiltersProc, P),
    enum_sources: unsafe extern "C" fn(EnumSourcesProc, P),
    data_create: unsafe extern "C" fn() -> P,
    data_release: unsafe extern "C" fn(P),
    data_set_double: unsafe extern "C" fn(P, *const c_char, f64),
    data_set_int: unsafe extern "C" fn(P, *const c_char, i64),
    data_set_bool: unsafe extern "C" fn(P, *const c_char, bool),
    data_set_string: unsafe extern "C" fn(P, *const c_char, *const c_char),
    data_has_user_value: unsafe extern "C" fn(P, *const c_char) -> bool,
    add_int: unsafe extern "C" fn(P, *const c_char, *const c_char, i32, i32, i32) -> P,
    property_set_modified_callback2: unsafe extern "C" fn(P, ModifiedProc, P),
    properties_set_param: unsafe extern "C" fn(P, P, Option<TaskProc>),
    queue_task: unsafe extern "C" fn(u32, TaskProc, P, bool),
    add_color_alpha: unsafe extern "C" fn(P, *const c_char, *const c_char) -> P,
    add_path: unsafe extern "C" fn(P, *const c_char, *const c_char, u32, *const c_char, *const c_char) -> P,
    add_button2: unsafe extern "C" fn(P, *const c_char, *const c_char, ClickedProc, P) -> P,
}

/// The scene calls fill_canvas() needs (OBS's frontend: looked up when
/// first needed, on OBS's UI thread).
struct SceneFns {
    get_current_scene: unsafe extern "C" fn() -> P,
    scene_from_source: unsafe extern "C" fn(P) -> P,
    find_source_recursive: unsafe extern "C" fn(P, *const c_char) -> P,
    set_alignment: unsafe extern "C" fn(P, u32),
    set_pos: unsafe extern "C" fn(P, *const [f32; 2]),
    set_bounds: unsafe extern "C" fn(P, *const [f32; 2]),
    set_bounds_type: unsafe extern "C" fn(P, u32),
}

// Function pointers into libobs, which outlives us.
unsafe impl Send for Fns {}
unsafe impl Sync for Fns {}
unsafe impl Send for SceneFns {}
unsafe impl Sync for SceneFns {}

pub(crate) fn sym<T>(name: &CStr) -> Option<T> {
    let p = obs_symbol(name);
    // SAFETY: T is the function pointer type of the libobs export `name`.
    (!p.is_null()).then(|| unsafe { std::mem::transmute_copy(&p) })
}

fn scene_fns() -> Option<&'static SceneFns> {
    static FNS: OnceLock<Option<SceneFns>> = OnceLock::new();
    FNS.get_or_init(|| {
        Some(SceneFns {
            get_current_scene: sym(c"obs_frontend_get_current_scene")?,
            scene_from_source: sym(c"obs_scene_from_source")?,
            find_source_recursive: sym(c"obs_scene_find_source_recursive")?,
            set_alignment: sym(c"obs_sceneitem_set_alignment")?,
            set_pos: sym(c"obs_sceneitem_set_pos")?,
            set_bounds: sym(c"obs_sceneitem_set_bounds")?,
            set_bounds_type: sym(c"obs_sceneitem_set_bounds_type")?,
        })
    })
    .as_ref()
}

fn fns() -> Option<&'static Fns> {
    static FNS: OnceLock<Option<Fns>> = OnceLock::new();
    FNS.get_or_init(|| {
        Some(Fns {
            register_source: sym(c"obs_register_source_s")?,
            source_get_id: sym(c"obs_source_get_id")?,
            source_get_display_name: sym(c"obs_source_get_display_name")?,
            source_enabled: sym(c"obs_source_enabled")?,
            filter_get_target: sym(c"obs_filter_get_target")?,
            source_get_base_width: sym(c"obs_source_get_base_width")?,
            source_get_base_height: sym(c"obs_source_get_base_height")?,
            process_filter_begin: sym(c"obs_source_process_filter_begin")?,
            process_filter_end: sym(c"obs_source_process_filter_end")?,
            skip_video_filter: sym(c"obs_source_skip_video_filter")?,
            enter_graphics: sym(c"obs_enter_graphics")?,
            leave_graphics: sym(c"obs_leave_graphics")?,
            effect_create: sym(c"gs_effect_create")?,
            effect_destroy: sym(c"gs_effect_destroy")?,
            effect_get_param_by_name: sym(c"gs_effect_get_param_by_name")?,
            effect_get_num_params: sym(c"gs_effect_get_num_params")?,
            effect_get_param_by_idx: sym(c"gs_effect_get_param_by_idx")?,
            effect_get_param_info: sym(c"gs_effect_get_param_info")?,
            effect_set_val: sym(c"gs_effect_set_val")?,
            blend_state_push: sym(c"gs_blend_state_push")?,
            blend_state_pop: sym(c"gs_blend_state_pop")?,
            blend_function_separate: sym(c"gs_blend_function_separate")?,
            properties_create: sym(c"obs_properties_create")?,
            add_float_slider: sym(c"obs_properties_add_float_slider")?,
            add_int_slider: sym(c"obs_properties_add_int_slider")?,
            add_bool: sym(c"obs_properties_add_bool")?,
            add_list: sym(c"obs_properties_add_list")?,
            list_add_int: sym(c"obs_property_list_add_int")?,
            list_add_string: sym(c"obs_property_list_add_string")?,
            add_text: sym(c"obs_properties_add_text")?,
            data_get_double: sym(c"obs_data_get_double")?,
            data_get_int: sym(c"obs_data_get_int")?,
            data_get_bool: sym(c"obs_data_get_bool")?,
            data_get_string: sym(c"obs_data_get_string")?,
            data_set_default_double: sym(c"obs_data_set_default_double")?,
            data_set_default_int: sym(c"obs_data_set_default_int")?,
            data_set_default_bool: sym(c"obs_data_set_default_bool")?,
            data_set_default_string: sym(c"obs_data_set_default_string")?,
            get_video_info: sym(c"obs_get_video_info")?,
            filter_get_parent: sym(c"obs_filter_get_parent")?,
            source_get_name: sym(c"obs_source_get_name")?,
            source_get_settings: sym(c"obs_source_get_settings")?,
            source_update: sym(c"obs_source_update")?,
            get_source_by_name: sym(c"obs_get_source_by_name")?,
            source_get_ref: sym(c"obs_source_get_ref")?,
            source_release: sym(c"obs_source_release")?,
            source_enum_filters: sym(c"obs_source_enum_filters")?,
            enum_sources: sym(c"obs_enum_sources")?,
            data_create: sym(c"obs_data_create")?,
            data_release: sym(c"obs_data_release")?,
            data_set_double: sym(c"obs_data_set_double")?,
            data_set_int: sym(c"obs_data_set_int")?,
            data_set_bool: sym(c"obs_data_set_bool")?,
            data_set_string: sym(c"obs_data_set_string")?,
            data_has_user_value: sym(c"obs_data_has_user_value")?,
            add_int: sym(c"obs_properties_add_int")?,
            property_set_modified_callback2: sym(c"obs_property_set_modified_callback2")?,
            properties_set_param: sym(c"obs_properties_set_param")?,
            queue_task: sym(c"obs_queue_task")?,
            add_color_alpha: sym(c"obs_properties_add_color_alpha")?,
            add_path: sym(c"obs_properties_add_path")?,
            add_button2: sym(c"obs_properties_add_button2")?,
        })
    })
    .as_ref()
}

/// OBS's canvas size.
fn canvas_size(fns: &Fns) -> (u32, u32) {
    let mut info = ObsVideoInfo::default();
    if unsafe { (fns.get_video_info)(&mut info) } && info.base_width > 0 && info.base_height > 0 {
        (info.base_width, info.base_height)
    } else {
        (1920, 1080)
    }
}

/// A C string from OBS, as a String ("" for none).
unsafe fn string_from(s: *const c_char) -> String {
    if s.is_null() { String::new() } else { unsafe { CStr::from_ptr(s) }.to_string_lossy().into_owned() }
}

fn cstring(s: &str) -> CString {
    CString::new(s.replace('\0', "")).unwrap_or_default()
}

// ─── Scripts ────────────────────────────────────────────────────────────────

/// How a filter's output is drawn.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Blend {
    /// OBS's usual (straight alpha)
    Normal,
    /// The shader outputs premultiplied colour (ONE, INVSRCALPHA)
    Premultiplied,
}

/// How big a filter's output is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Size {
    /// Its source's size (OBS's usual)
    Source,
    /// The canvas's (a 360° view)
    Canvas,
}

/// What a script makes in OBS.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ScriptKind {
    /// A filter: its SHADER draws its source (tick gives its values)
    Filter,
    /// A source of its own: tick gives what to draw (script_draw)
    Source,
    /// A dock in OBS: ui draws it (script_docks)
    Dock,
}

/// A filter script, compiled: what Theater reads from it once.
#[derive(Debug, Clone)]
pub struct Compiled {
    pub id: String,
    pub name: String,
    pub kind: ScriptKind,
    /// Its folder: pictures and SHADER_FILE are taken from there
    pub dir: PathBuf,
    pub blend: Blend,
    pub size: Size,
    /// The effect code (SHADER, or SHADER_FILE's contents)
    pub shader: String,
    /// Its settings and their default values (defaults())
    pub defaults: Map,
    /// Its functions only (the constants are read already)
    ast: AST,
}

impl Compiled {
    pub(crate) fn has(&self, name: &str) -> bool {
        self.ast.iter_functions().any(|f| f.name == name)
    }
}

/// The engine every filter script runs in: capped, no file or system access
/// (Rhai has none unless the host adds it), and share/shared_value/log.
pub fn engine() -> &'static Engine {
    static ENGINE: OnceLock<Engine> = OnceLock::new();
    ENGINE.get_or_init(|| {
        let mut engine = Engine::new();
        engine.set_max_operations(MAX_OPERATIONS);
        engine.set_max_call_levels(32);
        engine.set_max_expr_depths(64, 32);
        engine.on_print(|s| trust_log(&format!("[Rhai] {}", s)));
        engine.on_debug(|s, src, pos| trust_log(&format!("[Rhai] {} {:?} {}", src.unwrap_or(""), pos, s)));
        engine.register_fn("share", |key: ImmutableString, value: Dynamic| {
            SHARED.write().insert(key.to_string(), value);
        });
        engine.register_fn("shared_value", |key: ImmutableString| -> Dynamic {
            SHARED.read().get(key.as_str()).cloned().unwrap_or(Dynamic::UNIT)
        });
        engine.register_fn("log", |msg: ImmutableString| trust_log(&format!("[Rhai] {}", msg)));
        engine.register_fn("publish", |values: Map| -> Result<(), Box<rhai::EvalAltResult>> {
            // Runtime values only: a filter rewriting a real setting every frame
            // would fight whoever changes it
            if let Some(key) = values.keys().find(|k| !k.starts_with('_')) {
                return Err(format!("publish: \"{}\" must start with _ (runtime values only)", key).into());
            }
            EFFECTS.with(|e| {
                if let Some(effects) = e.borrow_mut().as_mut() {
                    effects.published.extend(values.into_iter().map(|(k, v)| (k.to_string(), v)));
                }
            });
            Ok(())
        });
        engine.register_fn("fill_canvas", || {
            EFFECTS.with(|e| {
                if let Some(effects) = e.borrow_mut().as_mut() {
                    effects.fill_canvas = true;
                }
            })
        });
        // Colours (0xAARRGGBB)
        engine.register_fn("rgba", |r: i64, g: i64, b: i64, a: f64| rgba(r, g, b, a));
        engine.register_fn("rgba", |r: i64, g: i64, b: i64, a: i64| rgba(r, g, b, a as f64));
        engine.register_fn("with_alpha", |c: i64, a: f64| with_alpha(c, a));
        engine.register_fn("with_alpha", |c: i64, a: i64| with_alpha(c, a as f64));
        engine.register_fn("parse_color", |text: &str, fallback: i64| parse_color(text, fallback));
        // Sources' drawing: measure_text, image_size, font_families
        crate::script_draw::register(&mut engine);
        // Whether the relay (script_feed) is connected
        engine.register_fn("relay_connected", crate::script_feed::connected);
        engine.register_fn("feed_connected", |url: &str| crate::script_feed::url_connected(url.trim()));
        // Your supporters, from the Theater API plugins' lists (supporters):
        // [#{ name, platform, tier, amount, gift }]
        engine.register_fn("supporters", || -> Array {
            crate::supporters::list()
                .into_iter()
                .map(|s| {
                    let mut m = Map::new();
                    m.insert("name".into(), Dynamic::from(s.name));
                    m.insert("platform".into(), Dynamic::from(s.platform));
                    m.insert("tier".into(), Dynamic::from(s.tier));
                    m.insert("amount".into(), Dynamic::from(s.amount));
                    m.insert("gift".into(), Dynamic::from(s.gift));
                    Dynamic::from_map(m)
                })
                .collect()
        });
        // A sound through the Throwables Layer (decoded on a thread of its own)
        // Text to the clipboard (a button: "copy this look…")
        engine.register_fn("copy_text", copy_text);
        engine.register_fn("play_sound", |path: &str| play_sound(path, 1.0));
        engine.register_fn("play_sound", |path: &str, volume: f64| play_sound(path, volume));
        engine.register_fn("play_sound", |path: &str, volume: i64| play_sound(path, volume as f64));
        // Docks' drawing, and what they can have Theater do
        crate::script_docks::register(&mut engine);
        engine
    })
}

/// Puts `text` on the clipboard (a script's button: "copy this look…"). True
/// when it's there. Windows only (elsewhere: false).
fn copy_text(text: &str) -> bool {
    #[cfg(target_os = "windows")]
    {
        use std::ffi::c_void;
        unsafe extern "system" {
            fn OpenClipboard(owner: *mut c_void) -> i32;
            fn EmptyClipboard() -> i32;
            fn SetClipboardData(format: u32, data: *mut c_void) -> *mut c_void;
            fn CloseClipboard() -> i32;
            fn GlobalAlloc(flags: u32, bytes: usize) -> *mut c_void;
            fn GlobalLock(mem: *mut c_void) -> *mut c_void;
            fn GlobalUnlock(mem: *mut c_void) -> i32;
            fn GlobalFree(mem: *mut c_void) -> *mut c_void;
        }
        const CF_UNICODETEXT: u32 = 13;
        const GMEM_MOVEABLE: u32 = 0x0002;
        let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        unsafe {
            if OpenClipboard(std::ptr::null_mut()) == 0 {
                return false;
            }
            EmptyClipboard();
            let mem = GlobalAlloc(GMEM_MOVEABLE, wide.len() * 2);
            let mut ok = false;
            if !mem.is_null() {
                let p = GlobalLock(mem) as *mut u16;
                if !p.is_null() {
                    std::ptr::copy_nonoverlapping(wide.as_ptr(), p, wide.len());
                    GlobalUnlock(mem);
                    // The clipboard owns it once it's set
                    ok = !SetClipboardData(CF_UNICODETEXT, mem).is_null();
                }
                if !ok {
                    GlobalFree(mem);
                }
            }
            CloseClipboard();
            ok
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = text;
        false
    }
}

/// Plays `path` (relative: from the calling script's folder); false when
/// there's no such file.
fn play_sound(path: &str, volume: f64) -> bool {
    let path = path.trim();
    if path.is_empty() {
        return false;
    }
    let mut file = PathBuf::from(path);
    if file.is_relative() {
        if let Some(dir) = current_script_dir() {
            file = dir.join(file);
        }
    }
    if !file.is_file() {
        trust_log(&format!("[Rhai] ⚠️ play_sound: no file {}", file.display()));
        return false;
    }
    crate::sound::play_file(&file, volume.clamp(0.0, 4.0) as f32, false)
}

thread_local! {
    /// The folder of the script being called (image_size's relative files)
    static CURRENT_DIR: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
}

/// The folder of the script being called on this thread, if one is.
pub fn current_script_dir() -> Option<PathBuf> {
    CURRENT_DIR.with(|d| d.borrow().clone())
}

/// What a filter's call asked Theater to do in OBS (publish, fill_canvas):
/// done once the call is over, so a script never runs an OBS call itself.
#[derive(Default)]
struct Effects {
    published: Vec<(String, Dynamic)>,
    fill_canvas: bool,
}

thread_local! {
    static EFFECTS: RefCell<Option<Effects>> = const { RefCell::new(None) };
}

/// Calls `name` with `this` bound, collecting what it asks of OBS.
fn call_collecting(compiled: &Compiled, this: &mut Dynamic, name: &str, args: impl rhai::FuncArgs) -> (Result<Dynamic, String>, Effects) {
    EFFECTS.with(|e| *e.borrow_mut() = Some(Effects::default()));
    let result = call(compiled, Some(this), name, args);
    let effects = EFFECTS.with(|e| e.borrow_mut().take()).unwrap_or_default();
    (result, effects)
}

/// Values filters share (share / shared_value), e.g. a 360 Master's live camera
/// for its Layers. Held only for a lookup or an insert, never across an OBS
/// call.
static SHARED: std::sync::LazyLock<RwLock<HashMap<String, Dynamic>>> =
    std::sync::LazyLock::new(|| RwLock::new(HashMap::new()));

/// Compiles the filter script `source` (from the file at `path`, whose
/// folder a SHADER_FILE is taken from).
pub fn compile(source: &str, path: &Path) -> Result<Compiled, String> {
    let engine = engine();
    let script = engine.compile(source).map_err(|e| e.to_string())?;

    // Its constants: running the top level (constants and functions only)
    let mut scope = Scope::new();
    engine.run_ast_with_scope(&mut scope, &script).map_err(|e| e.to_string())?;
    let text = |name: &str| scope.get_value::<ImmutableString>(name).map(|s| s.to_string());
    let id = text("ID").filter(|s| !s.trim().is_empty()).ok_or("no `const ID = \"…\";`")?;
    let name = text("NAME").unwrap_or_else(|| id.clone());
    let blend = match text("BLEND").as_deref() {
        None | Some("normal") => Blend::Normal,
        Some("premultiplied") => Blend::Premultiplied,
        Some(other) => return Err(format!("BLEND \"{}\": \"normal\" or \"premultiplied\"", other)),
    };
    let size = match text("SIZE").as_deref() {
        None | Some("source") => Size::Source,
        Some("canvas") => Size::Canvas,
        Some(other) => return Err(format!("SIZE \"{}\": \"source\" or \"canvas\"", other)),
    };
    let kind = match text("KIND").as_deref() {
        None | Some("filter") => ScriptKind::Filter,
        Some("source") => ScriptKind::Source,
        Some("dock") => ScriptKind::Dock,
        Some(other) => return Err(format!("KIND \"{}\": \"filter\", \"source\" or \"dock\"", other)),
    };
    // The property helpers (button, toggle…) for filters and sources: a
    // dock's `ui.button(…)` would find them first
    let ast = if kind == ScriptKind::Dock {
        script
    } else {
        engine.compile(PRELUDE).map_err(|e| e.to_string())?.merge(&script)
    };
    let dir = path.parent().unwrap_or(Path::new("")).to_path_buf();
    let shader = match (text("SHADER"), text("SHADER_FILE")) {
        (Some(code), _) => code,
        (None, Some(file)) => {
            let file = dir.join(file);
            std::fs::read_to_string(&file).map_err(|e| format!("SHADER_FILE {}: {}", file.display(), e))?
        }
        // A source draws with tick's list instead, a dock with egui
        (None, None) if kind != ScriptKind::Filter => String::new(),
        (None, None) => return Err("no `const SHADER` (or SHADER_FILE)".to_string()),
    };

    let mut compiled = Compiled { id, name, kind, dir, blend, size, shader, defaults: Map::new(), ast: ast.clone_functions_only() };
    if compiled.has("defaults") {
        let defaults: Dynamic = call(&compiled, None, "defaults", ())?;
        compiled.defaults = defaults.try_cast::<Map>().ok_or("defaults() must return a map")?;
    }
    if kind == ScriptKind::Dock {
        if !compiled.has("ui") {
            return Err("no `fn ui(ui)`".to_string());
        }
    } else if !compiled.has("tick") {
        return Err("no `fn tick(dt)`".to_string());
    }
    Ok(compiled)
}

/// Calls `name` in `compiled`, with `this` bound when given.
pub(crate) fn call(compiled: &Compiled, this: Option<&mut Dynamic>, name: &str, args: impl rhai::FuncArgs) -> Result<Dynamic, String> {
    let mut options = CallFnOptions::new().eval_ast(false);
    if let Some(this) = this {
        options = options.bind_this_ptr(this);
    }
    let previous = CURRENT_DIR.with(|d| d.borrow_mut().replace(compiled.dir.clone()));
    let result = engine()
        .call_fn_with_options::<Dynamic>(options, &mut Scope::new(), &compiled.ast, name, args)
        .map_err(|e| format!("{}(): {}", name, e));
    CURRENT_DIR.with(|d| *d.borrow_mut() = previous);
    result
}

/// One property of a filter, as its script declared it.
#[derive(Debug, Clone, PartialEq)]
pub enum Prop {
    Slider { key: String, label: String, min: f64, max: f64, step: f64 },
    IntSlider { key: String, label: String, min: i64, max: i64, step: i64 },
    IntInput { key: String, label: String, min: i64, max: i64, step: i64 },
    Toggle { key: String, label: String },
    Choice { key: String, label: String, options: Vec<(String, ChoiceValue)> },
    /// The sources carrying a `filter` filter (every source for ""), by name
    SourceList { key: String, label: String, filter: String },
    Text { key: String, label: String },
    Info { label: String },
    /// A colour with its alpha; the script sees 0xAARRGGBB
    Color { key: String, label: String },
    /// Clicking it calls the script's on_button(key)
    Button { key: String, label: String },
    /// The font families text can be set in (script_draw::font_families)
    FontList { key: String, label: String },
    /// A file picker; `filter` is OBS's ("Sounds (*.wav *.mp3)"), "" for any
    File { key: String, label: String, filter: String },
}

/// A property linked to another filter's setting (see remote() in the
/// module doc).
#[derive(Debug, Clone, PartialEq)]
pub struct Remote {
    /// The setting (a source_list) naming the source that filter is on
    pub source_key: String,
    pub filter_id: String,
    /// Its setting this property drives
    pub key: String,
}

/// The value a choice's option stores in its setting.
#[derive(Debug, Clone, PartialEq)]
pub enum ChoiceValue {
    Int(i64),
    Text(String),
}

impl ChoiceValue {
    fn from_dynamic(value: &Dynamic) -> Self {
        match value.as_int().or_else(|_| value.as_float().map(|f| f as i64)) {
            Ok(i) if !value.is_string() => ChoiceValue::Int(i),
            _ => ChoiceValue::Text(value.to_string()),
        }
    }
}

impl Prop {
    fn from_map(map: &Map) -> Option<Self> {
        let s = |k: &str| map.get(k).map(|v| v.to_string()).unwrap_or_default();
        let f = |k: &str| map.get(k).and_then(|v| v.as_float().ok().or_else(|| v.as_int().ok().map(|i| i as f64))).unwrap_or(0.0);
        let i = |k: &str| map.get(k).and_then(|v| v.as_int().ok().or_else(|| v.as_float().ok().map(|f| f as i64))).unwrap_or(0);
        let (key, label) = (s("key"), s("label"));
        Some(match s("kind").as_str() {
            "slider" => Prop::Slider { key, label, min: f("min"), max: f("max"), step: f("step") },
            "int_slider" => Prop::IntSlider { key, label, min: i("min"), max: i("max"), step: i("step").max(1) },
            "int_input" => Prop::IntInput { key, label, min: i("min"), max: i("max"), step: i("step").max(1) },
            "source_list" => Prop::SourceList { key, label, filter: s("filter") },
            "toggle" => Prop::Toggle { key, label },
            "choice" => {
                let options = map
                    .get("options")?
                    .clone()
                    .try_cast::<Array>()?
                    .into_iter()
                    .filter_map(|o| {
                        let pair = o.try_cast::<Array>()?;
                        Some((pair.first()?.to_string(), ChoiceValue::from_dynamic(pair.get(1)?)))
                    })
                    .collect();
                Prop::Choice { key, label, options }
            }
            "text" => Prop::Text { key, label },
            "info" => Prop::Info { label },
            "color" => Prop::Color { key, label },
            "button" => Prop::Button { key, label },
            "font_list" => Prop::FontList { key, label },
            "file" => Prop::File { key, label, filter: s("filter") },
            _ => return None,
        })
    }

    /// The setting it edits (a button and an info have none).
    fn key(&self) -> Option<&str> {
        match self {
            Prop::Slider { key, .. }
            | Prop::IntSlider { key, .. }
            | Prop::IntInput { key, .. }
            | Prop::Toggle { key, .. }
            | Prop::Choice { key, .. }
            | Prop::SourceList { key, .. }
            | Prop::Text { key, .. }
            | Prop::Color { key, .. }
            | Prop::FontList { key, .. }
            | Prop::File { key, .. } => Some(key),
            Prop::Info { .. } | Prop::Button { .. } => None,
        }
    }
}

impl Remote {
    fn from_map(map: &Map) -> Option<Self> {
        let remote = map.get("remote")?.read_lock::<Map>()?.clone();
        let s = |k: &str| remote.get(k).map(|v| v.to_string()).filter(|v| !v.is_empty());
        Some(Remote { source_key: s("source")?, filter_id: s("filter")?, key: s("key")? })
    }
}

/// The properties `compiled` declares (properties()), each with the other
/// filter's setting it's linked to, if it is.
pub fn declared_properties(compiled: &Compiled) -> Result<Vec<(Prop, Option<Remote>)>, String> {
    if !compiled.has("properties") {
        return Ok(Vec::new());
    }
    let list = call(compiled, None, "properties", ())?
        .try_cast::<Array>()
        .ok_or("properties() must return an array")?;
    Ok(list
        .iter()
        .filter_map(|p| {
            let map = p.read_lock::<Map>()?;
            Some((Prop::from_map(&map)?, Remote::from_map(&map)))
        })
        .collect())
}

/// The properties `compiled` declares (properties()).
pub fn properties(compiled: &Compiled) -> Result<Vec<Prop>, String> {
    Ok(declared_properties(compiled)?.into_iter().map(|(p, _)| p).collect())
}

/// What kind of value a setting holds, from its default (or its property).
#[derive(Debug, Clone, Copy, PartialEq)]
enum SettingKind {
    Float,
    Int,
    Bool,
    Text,
    /// OBS keeps 0xAABBGGRR; the script sees 0xAARRGGBB
    Color,
}

/// 0xAARRGGBB ↔ 0xAABBGGRR (OBS's colour settings): red and blue swapped.
fn swap_red_blue(c: u32) -> u32 {
    (c & 0xFF00_FF00) | ((c & 0xFF) << 16) | ((c >> 16) & 0xFF)
}

fn setting_kinds(compiled: &Compiled, props: &[Prop]) -> Vec<(String, SettingKind)> {
    let mut kinds: Vec<(String, SettingKind)> = compiled
        .defaults
        .iter()
        .map(|(key, value)| {
            let kind = if value.is_bool() {
                SettingKind::Bool
            } else if value.is_int() {
                SettingKind::Int
            } else if value.is_string() {
                SettingKind::Text
            } else {
                SettingKind::Float
            };
            (key.to_string(), kind)
        })
        .collect();
    for prop in props {
        let Some(key) = prop.key() else { continue };
        // A colour's default is a number: its property says it's a colour
        if let Prop::Color { .. } = prop {
            match kinds.iter_mut().find(|(k, _)| k == key) {
                Some(entry) => entry.1 = SettingKind::Color,
                None => kinds.push((key.to_string(), SettingKind::Color)),
            }
            continue;
        }
        if kinds.iter().any(|(k, _)| k == key) {
            continue;
        }
        let kind = match prop {
            Prop::Slider { .. } => SettingKind::Float,
            Prop::IntSlider { .. } | Prop::IntInput { .. } => SettingKind::Int,
            Prop::Toggle { .. } => SettingKind::Bool,
            Prop::Choice { options, .. } if matches!(options.first(), Some((_, ChoiceValue::Text(_)))) => SettingKind::Text,
            Prop::Choice { .. } => SettingKind::Int,
            _ => SettingKind::Text,
        };
        kinds.push((key.to_string(), kind));
    }
    kinds
}

/// A setting's value, as OBS stores it.
#[derive(Debug, Clone, PartialEq)]
enum SettingValue {
    Float(f64),
    Int(i64),
    Bool(bool),
    Text(String),
    /// 0xAARRGGBB
    Color(u32),
}

impl SettingValue {
    /// A script's `value` as a setting of `kind` (of its own kind for None).
    fn from_dynamic(value: &Dynamic, kind: Option<SettingKind>) -> Option<Self> {
        let number = value.as_float().ok().or_else(|| value.as_int().ok().map(|i| i as f64));
        Some(match kind {
            Some(SettingKind::Float) => SettingValue::Float(number?),
            Some(SettingKind::Int) => SettingValue::Int(value.as_int().ok().or_else(|| number.map(|f| f.round() as i64))?),
            Some(SettingKind::Bool) => SettingValue::Bool(value.as_bool().ok()?),
            Some(SettingKind::Text) => SettingValue::Text(value.to_string()),
            Some(SettingKind::Color) => SettingValue::Color(value.as_int().ok().or_else(|| number.map(|f| f as i64))? as u32),
            None if value.is_bool() => SettingValue::Bool(value.as_bool().ok()?),
            None if value.is_int() => SettingValue::Int(value.as_int().ok()?),
            None if value.is_float() => SettingValue::Float(number?),
            None if value.is_string() => SettingValue::Text(value.to_string()),
            None => return None,
        })
    }

    fn kind(&self) -> SettingKind {
        match self {
            SettingValue::Float(_) => SettingKind::Float,
            SettingValue::Int(_) => SettingKind::Int,
            SettingValue::Bool(_) => SettingKind::Bool,
            SettingValue::Text(_) => SettingKind::Text,
            SettingValue::Color(_) => SettingKind::Color,
        }
    }

    fn to_dynamic(&self) -> Dynamic {
        match self {
            SettingValue::Float(f) => Dynamic::from_float(*f),
            SettingValue::Int(i) => Dynamic::from_int(*i),
            SettingValue::Bool(b) => Dynamic::from_bool(*b),
            SettingValue::Text(s) => Dynamic::from(s.clone()),
            SettingValue::Color(c) => Dynamic::from_int(*c as i64),
        }
    }

    /// Setting `key` of the OBS settings `data`, read as `kind`.
    unsafe fn read(fns: &Fns, data: P, key: &CStr, kind: SettingKind) -> Self {
        unsafe {
            match kind {
                SettingKind::Float => SettingValue::Float((fns.data_get_double)(data, key.as_ptr())),
                SettingKind::Int => SettingValue::Int((fns.data_get_int)(data, key.as_ptr())),
                SettingKind::Bool => SettingValue::Bool((fns.data_get_bool)(data, key.as_ptr())),
                SettingKind::Text => SettingValue::Text(string_from((fns.data_get_string)(data, key.as_ptr()))),
                SettingKind::Color => SettingValue::Color(swap_red_blue((fns.data_get_int)(data, key.as_ptr()) as u32)),
            }
        }
    }

    unsafe fn write(&self, fns: &Fns, data: P, key: &CStr) {
        unsafe {
            match self {
                SettingValue::Float(f) => (fns.data_set_double)(data, key.as_ptr(), *f),
                SettingValue::Int(i) => (fns.data_set_int)(data, key.as_ptr(), *i),
                SettingValue::Bool(b) => (fns.data_set_bool)(data, key.as_ptr(), *b),
                SettingValue::Text(s) => (fns.data_set_string)(data, key.as_ptr(), cstring(s).as_ptr()),
                SettingValue::Color(c) => (fns.data_set_int)(data, key.as_ptr(), swap_red_blue(*c) as i64),
            }
        }
    }
}

// ─── Registered filters ─────────────────────────────────────────────────────

/// A filter type Theater registered from a script file.
pub struct ScriptType {
    /// Its file (a rename in the Scripts tab moves it: file_renamed)
    path: RwLock<PathBuf>,
    id: CString,
    name: CString,
    compiled: RwLock<Arc<Compiled>>,
    /// Bumped by each reload, so instances remake their shader
    version: std::sync::atomic::AtomicU64,
    modified: Mutex<Option<SystemTime>>,
    /// Why the file's latest version isn't running, if it isn't
    pub error: Mutex<Option<String>>,
    instances: std::sync::atomic::AtomicUsize,
}

impl ScriptType {
    pub fn path(&self) -> PathBuf {
        self.path.read().clone()
    }
    pub fn id(&self) -> String {
        self.id.to_string_lossy().into_owned()
    }
    pub fn compiled(&self) -> Arc<Compiled> {
        self.compiled.read().clone()
    }
    pub fn instances(&self) -> usize {
        self.instances.load(std::sync::atomic::Ordering::Relaxed)
    }
    /// Bumped by each reload
    pub fn version(&self) -> u64 {
        self.version.load(std::sync::atomic::Ordering::SeqCst)
    }
}

/// A script file Theater looked at: registered, or why not.
#[derive(Debug, Clone)]
pub struct ScriptFile {
    pub path: PathBuf,
    pub id: Option<String>,
    /// Registered as an OBS filter (running, or reloading live)
    pub registered: bool,
    pub problem: Option<String>,
}

static TYPES: RwLock<Vec<&'static ScriptType>> = RwLock::new(Vec::new());
static FILES: RwLock<Vec<ScriptFile>> = RwLock::new(Vec::new());

/// The script files Theater looked at when OBS started, and how they fared.
pub fn files() -> Vec<ScriptFile> {
    let types = TYPES.read();
    FILES
        .read()
        .iter()
        .map(|f| {
            let mut f = f.clone();
            if let Some(ty) = types.iter().find(|t| t.path() == f.path) {
                f.problem = ty.error.lock().clone();
            }
            f
        })
        .collect()
}

/// The filter types registered from scripts.
pub fn types() -> Vec<&'static ScriptType> {
    TYPES.read().clone()
}

/// `ty`'s file was renamed to `new` (Scripts tab): it keeps running and
/// reloading from there. Its filter id doesn't change.
pub fn file_renamed(ty: &ScriptType, new: &Path) {
    let old = std::mem::replace(&mut *ty.path.write(), new.to_path_buf());
    for file in FILES.write().iter_mut().filter(|f| f.path == old) {
        file.path = new.to_path_buf();
    }
    trust_log(&format!("[Rhai] ✏ {} is now {}", old.display(), new.display()));
}

/// The folders Rhai filters are looked for in: the ones Theater ships, yours
/// (paths::user_filters_dir), and
/// the folders it remembers (remember_folder).
pub fn script_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![crate::studio_app::get_filters_dir(), crate::paths::user_filters_dir()];
    for dir in remembered_folders() {
        if !dirs.contains(&dir) {
            dirs.push(dir);
        }
    }
    dirs
}

fn folders_file() -> PathBuf {
    crate::paths::file("theater_script_folders.json")
}

/// The other folders Theater looks for Rhai filters in.
pub fn remembered_folders() -> Vec<PathBuf> {
    std::fs::read_to_string(folders_file())
        .ok()
        .and_then(|s| serde_json::from_str::<Vec<PathBuf>>(&s).ok())
        .unwrap_or_default()
}

/// Has Theater look for Rhai filters in `dir` too (from the next OBS start).
/// False if it did already.
pub fn remember_folder(dir: &Path) -> bool {
    let mut folders = remembered_folders();
    if script_dirs().iter().any(|d| d == dir) || folders.iter().any(|f| f == dir) {
        return false;
    }
    folders.push(dir.to_path_buf());
    let saved = serde_json::to_string_pretty(&folders).map_err(|e| e.to_string()).and_then(|s| std::fs::write(folders_file(), s).map_err(|e| e.to_string()));
    if let Err(e) = saved {
        trust_log(&format!("[Rhai] ⚠️ couldn't remember {}: {}", dir.display(), e));
    }
    true
}

/// Stops looking for Rhai filters in `dir` (from the next OBS start).
pub fn forget_folder(dir: &Path) {
    let folders: Vec<PathBuf> = remembered_folders().into_iter().filter(|f| f != dir).collect();
    let saved = serde_json::to_string_pretty(&folders).map_err(|e| e.to_string()).and_then(|s| std::fs::write(folders_file(), s).map_err(|e| e.to_string()));
    if let Err(e) = saved {
        trust_log(&format!("[Rhai] ⚠️ couldn't forget {}: {}", dir.display(), e));
    }
}

/// Folders of the scripts OBS loads (Tools → Scripts) that hold Rhai
/// filters: a Rhai port sitting next to the Lua filter it replaces is found
/// without being added, and stays found once that Lua script is removed.
fn remember_obs_script_folders() {
    let collection = crate::script_catalog::current_scene_collection_file()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok());
    let Some(collection) = collection else { return };
    for script in crate::script_catalog::obs_scripts(&collection) {
        let Some(dir) = script.parent() else { continue };
        let has_rhai = std::fs::read_dir(dir)
            .map(|entries| entries.flatten().any(|e| e.path().extension().is_some_and(|x| x.eq_ignore_ascii_case("rhai"))))
            .unwrap_or(false);
        if has_rhai && remember_folder(dir) {
            trust_log(&format!("[Rhai] 📂 Rhai filters next to OBS's scripts in {}: looking there from now on", dir.display()));
        }
    }
}

/// Registers an OBS filter for each `.rhai` filter script in script_dirs.
/// When OBS loads Theater: filter types can't be added once scenes load.
pub fn register_all() {
    let Some(fns) = fns() else {
        trust_log("[Rhai] ⚠️ libobs functions missing: no Rhai filters");
        return;
    };
    remember_obs_script_folders();
    let mut files = Vec::new();
    for dir in script_dirs() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        let mut paths: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("rhai")))
            .collect();
        paths.sort();
        for path in paths {
            files.push(register_file(fns, &path));
        }
    }
    let count = files.iter().filter(|f| f.registered).count();
    trust_log(&format!("[Rhai] {} filter script(s) registered of {}", count, files.len()));
    *FILES.write() = files;
    if count > 0 {
        start_reloader();
    }
}

fn register_file(fns: &Fns, path: &Path) -> ScriptFile {
    let fail = |id: Option<String>, problem: String| {
        trust_log(&format!("[Rhai] ⚠️ {}: {}", path.display(), problem));
        ScriptFile { path: path.to_path_buf(), id, registered: false, problem: Some(problem) }
    };
    let source = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => return fail(None, e.to_string()),
    };
    let compiled = match compile(&source, path) {
        Ok(c) => c,
        Err(e) => return fail(None, e),
    };
    let id = compiled.id.clone();
    if compiled.kind == ScriptKind::Dock {
        return register_dock(path, compiled);
    }
    let canvas_sized = compiled.size == Size::Canvas;
    let is_source = compiled.kind == ScriptKind::Source;
    let id_c = cstring(&id);
    // Taken already (another script, or Theater loaded twice): OBS keeps the first
    if !unsafe { (fns.source_get_display_name)(id_c.as_ptr()) }.is_null() {
        return fail(Some(id.clone()), format!("the filter id \"{}\" is taken already", id));
    }

    let ty: &'static ScriptType = Box::leak(Box::new(ScriptType {
        path: RwLock::new(path.to_path_buf()),
        name: cstring(&compiled.name),
        id: id_c,
        compiled: RwLock::new(Arc::new(compiled)),
        version: std::sync::atomic::AtomicU64::new(1),
        modified: Mutex::new(std::fs::metadata(path).and_then(|m| m.modified()).ok()),
        error: Mutex::new(None),
        instances: std::sync::atomic::AtomicUsize::new(0),
    }));
    let info = Box::leak(Box::new(SourceInfo {
        id: ty.id.as_ptr(),
        type_: if is_source { OBS_SOURCE_TYPE_INPUT } else { OBS_SOURCE_TYPE_FILTER },
        output_flags: if is_source { OBS_SOURCE_VIDEO | OBS_SOURCE_CUSTOM_DRAW } else { OBS_SOURCE_VIDEO },
        get_name: Some(get_name),
        create: Some(create),
        destroy: Some(destroy),
        get_width: if is_source {
            Some(own_width as unsafe extern "C" fn(P) -> u32)
        } else {
            canvas_sized.then_some(canvas_width as unsafe extern "C" fn(P) -> u32)
        },
        get_height: if is_source {
            Some(own_height as unsafe extern "C" fn(P) -> u32)
        } else {
            canvas_sized.then_some(canvas_height as unsafe extern "C" fn(P) -> u32)
        },
        get_defaults: None,
        get_properties: None,
        update: Some(update),
        activate: None,
        deactivate: None,
        show: None,
        hide: None,
        video_tick: Some(video_tick),
        video_render: Some(video_render),
        filter_video: None,
        filter_audio: None,
        enum_active_sources: None,
        save: None,
        load: None,
        mouse_click: None,
        mouse_move: None,
        mouse_wheel: None,
        focus: None,
        key_click: None,
        filter_remove: None,
        type_data: ty as *const ScriptType as P,
        free_type_data: None,
        audio_render: None,
        enum_all_sources: None,
        transition_start: None,
        transition_stop: None,
        get_defaults2: Some(get_defaults2),
        get_properties2: Some(get_properties2),
    }));
    unsafe { (fns.register_source)(info, std::mem::size_of::<SourceInfo>()) };
    TYPES.write().push(ty);
    let what = if is_source { "source" } else { "filter" };
    trust_log(&format!("[Rhai] 🧩 Registered {} \"{}\" ({}) from {}", what, ty.name.to_string_lossy(), id, path.display()));
    ScriptFile { path: path.to_path_buf(), id: Some(id), registered: true, problem: None }
}

/// A dock script: nothing to register in OBS (egui_docks makes its dock
/// while OBS loads), but it's listed and reloaded like the others.
fn register_dock(path: &Path, compiled: Compiled) -> ScriptFile {
    let id = compiled.id.clone();
    let taken = types().iter().any(|t| t.compiled().kind == ScriptKind::Dock && t.id() == id);
    if taken {
        let problem = format!("the dock id \"{}\" is taken already", id);
        trust_log(&format!("[Rhai] ⚠️ {}: {}", path.display(), problem));
        return ScriptFile { path: path.to_path_buf(), id: Some(id), registered: false, problem: Some(problem) };
    }
    let ty = leak_type(path, compiled);
    TYPES.write().push(ty);
    trust_log(&format!("[Rhai] 🧩 Registered dock \"{}\" ({}) from {}", ty.name.to_string_lossy(), id, path.display()));
    ScriptFile { path: path.to_path_buf(), id: Some(id), registered: true, problem: None }
}

/// A script's type, kept for the rest of the run.
fn leak_type(path: &Path, compiled: Compiled) -> &'static ScriptType {
    Box::leak(Box::new(ScriptType {
        path: RwLock::new(path.to_path_buf()),
        name: cstring(&compiled.name),
        id: cstring(&compiled.id),
        compiled: RwLock::new(Arc::new(compiled)),
        version: std::sync::atomic::AtomicU64::new(1),
        modified: Mutex::new(std::fs::metadata(path).and_then(|m| m.modified()).ok()),
        error: Mutex::new(None),
        instances: std::sync::atomic::AtomicUsize::new(0),
    }))
}

/// The script at `path` as a type, for tests: not registered anywhere.
#[cfg(test)]
pub(crate) fn test_type(path: &Path) -> Result<&'static ScriptType, String> {
    let source = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    Ok(leak_type(path, compile(&source, path)?))
}

/// Reloads `ty` now, even if its file's time didn't change (tests write
/// faster than it ticks).
#[cfg(test)]
pub(crate) fn test_reload(ty: &ScriptType) {
    *ty.modified.lock() = None;
    reload_if_changed(ty);
}

/// Reloads edited scripts: every second, off OBS's threads. A version that
/// doesn't compile, or changes the filter's id (OBS can't rename a filter
/// type), leaves the running one in place.
fn start_reloader() {
    static STARTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if STARTED.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    let spawned = std::thread::Builder::new().name("theater-rhai-reload".into()).spawn(|| loop {
        std::thread::sleep(std::time::Duration::from_secs(1));
        for ty in types() {
            reload_if_changed(ty);
        }
    });
    if let Err(e) = spawned {
        trust_log(&format!("[Rhai] ⚠️ no live reload: {}", e));
    }
}

/// Reloads `ty`'s file if it changed since it was last read.
pub fn reload_if_changed(ty: &ScriptType) {
    let modified = std::fs::metadata(ty.path()).and_then(|m| m.modified()).ok();
    {
        let mut last = ty.modified.lock();
        if *last == modified {
            return;
        }
        *last = modified;
    }
    let result = std::fs::read_to_string(ty.path())
        .map_err(|e| e.to_string())
        .and_then(|source| compile(&source, &ty.path()))
        .and_then(|compiled| {
            if compiled.id != ty.id() {
                Err(format!("its id changed to \"{}\": restart OBS to use it", compiled.id))
            } else if compiled.size != ty.compiled().size {
                Err("its SIZE changed: restart OBS to use it".to_string())
            } else if compiled.kind != ty.compiled().kind {
                Err("its KIND changed: restart OBS to use it".to_string())
            } else {
                Ok(compiled)
            }
        });
    match result {
        Ok(compiled) => {
            *ty.compiled.write() = Arc::new(compiled);
            ty.version.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            *ty.error.lock() = None;
            trust_log(&format!("[Rhai] 🔄 Reloaded {}", ty.path().display()));
        }
        Err(e) => {
            trust_log(&format!("[Rhai] ⚠️ {} not reloaded (the last version keeps running): {}", ty.path().display(), e));
            *ty.error.lock() = Some(e);
        }
    }
}

// ─── One filter in OBS ──────────────────────────────────────────────────────

struct Instance {
    ty: &'static ScriptType,
    source: P,
    /// Its own, for its buttons' clicks (PENDING_BUTTONS)
    id: u64,
    /// A source's size: width << 32 | height (this.size)
    size: std::sync::atomic::AtomicU64,
    /// A source's relay messages (script_feed), when it has on_event
    events: Option<crossbeam_channel::Receiver<Arc<str>>>,
    /// The WebSocket its script listens to (`this.feed`), and its messages
    feed: Mutex<Option<(String, crossbeam_channel::Receiver<Arc<str>>)>>,
    state: Mutex<InstanceState>,
}

/// Relay messages a source takes a tick, at most (the rest wait).
const MAX_EVENTS_PER_TICK: usize = 32;

/// Button clicks (instance id, button key) not handed to on_button yet.
static PENDING_BUTTONS: Mutex<Vec<(u64, String)>> = Mutex::new(Vec::new());

/// A relay message as the map a source's on_event gets: its JSON as it is
/// (type, user, message…), or #{ type: "raw", text: … } when it isn't JSON.
/// An alert always has `alert`, the shared shape (theater_shared::alert):
/// filled in here when its sender didn't.
pub(crate) fn event_map(text: &str) -> Map {
    let filled;
    let text = match serde_json::from_str::<serde_json::Value>(text) {
        Ok(v) if v.get("type").and_then(|t| t.as_str()) == Some("alert") && v.get("alert").is_none() => {
            filled = theater_shared::alert::with_alert(v).to_string();
            filled.as_str()
        }
        _ => text,
    };
    engine().parse_json(text, true).unwrap_or_else(|_| {
        let mut m = Map::new();
        m.insert("type".into(), Dynamic::from("raw"));
        m.insert("text".into(), Dynamic::from(text.to_string()));
        m
    })
}

struct InstanceState {
    /// The script's `this`
    this: Dynamic,
    /// The shader's values from the last tick; None draws the source untouched
    uniforms: Option<Vec<(CString, Dynamic)>>,
    /// The compiled version its shader and state were made for
    version: u64,
    effect: usize,
    effect_failed: bool,
    /// The last error logged, so each is logged once
    last_error: Option<String>,
    /// A source's drawing, from its last tick
    cmds: Vec<crate::script_draw::Cmd>,
    /// Where a source draws (freed in the graphics context)
    canvas: crate::script_draw::Canvas,
}

fn new_this() -> Dynamic {
    let mut this = Map::new();
    this.insert("settings".into(), Dynamic::from_map(Map::new()));
    this.insert("enabled".into(), Dynamic::TRUE);
    this.insert("time".into(), Dynamic::from_float(0.0));
    View::default().set_into(&mut this);
    Dynamic::from_map(this)
}

/// What a filter's script sees of OBS, set before each update and tick.
#[derive(Debug, Default, Clone)]
struct View {
    /// What it draws: its source, through the filters before it
    width: u32,
    height: u32,
    source_name: String,
    /// Its source's own size
    source_width: u32,
    source_height: u32,
    canvas: (u32, u32),
}

impl View {
    /// `instance`'s view: a filter's (of), or a source's own name and size.
    unsafe fn for_instance(fns: &Fns, instance: &Instance, compiled: &Compiled) -> Self {
        if compiled.kind == ScriptKind::Source {
            let (w, h) = instance.own_size();
            let name = unsafe { string_from((fns.source_get_name)(instance.source)) };
            View { width: w, height: h, source_name: name, source_width: w, source_height: h, canvas: canvas_size(fns) }
        } else {
            unsafe { View::of(fns, instance.source) }
        }
    }

    /// `filter`'s view of OBS. Takes no OBS lock: fine in a tick.
    unsafe fn of(fns: &Fns, filter: P) -> Self {
        unsafe {
            let target = (fns.filter_get_target)(filter);
            let (width, height) = if target.is_null() {
                (0, 0)
            } else {
                ((fns.source_get_base_width)(target), (fns.source_get_base_height)(target))
            };
            // No parent while it's being created
            let parent = (fns.filter_get_parent)(filter);
            let (source_name, source_width, source_height) = if parent.is_null() {
                (String::new(), 0, 0)
            } else {
                (string_from((fns.source_get_name)(parent)), (fns.source_get_base_width)(parent), (fns.source_get_base_height)(parent))
            };
            View { width, height, source_name, source_width, source_height, canvas: canvas_size(fns) }
        }
    }

    fn set_into(&self, this: &mut Map) {
        let int = |v: u32| Dynamic::from_int(v as i64);
        this.insert("width".into(), int(self.width));
        this.insert("height".into(), int(self.height));
        this.insert("source_name".into(), Dynamic::from(self.source_name.clone()));
        this.insert("source_width".into(), int(self.source_width));
        this.insert("source_height".into(), int(self.source_height));
        this.insert("canvas_width".into(), int(self.canvas.0));
        this.insert("canvas_height".into(), int(self.canvas.1));
    }
}

impl InstanceState {
    fn report(&mut self, ty: &ScriptType, error: Option<String>) {
        if let Some(e) = &error {
            if self.last_error.as_ref() != Some(e) {
                trust_log(&format!("[Rhai] ⚠️ {}: {}", ty.path().display(), e));
            }
        }
        self.last_error = error;
    }

    fn set(&mut self, key: &str, value: Dynamic) {
        if let Some(mut map) = self.this.write_lock::<Map>() {
            map.insert(key.into(), value);
        }
    }

    fn set_view(&mut self, view: &View) {
        if let Some(mut map) = self.this.write_lock::<Map>() {
            view.set_into(&mut map);
        }
    }
}

impl Instance {
    /// Does in OBS what a call of its script asked for (Effects), once the
    /// call is over. `settings`: its settings, when OBS handed them over.
    unsafe fn apply(&self, fns: &Fns, effects: Effects, settings: Option<P>, view: &View) {
        if !effects.published.is_empty() {
            let data = settings.unwrap_or_else(|| unsafe { (fns.source_get_settings)(self.source) });
            if !data.is_null() {
                for (key, value) in &effects.published {
                    let Some(value) = SettingValue::from_dynamic(value, None) else { continue };
                    let key = cstring(key);
                    // Only what changed: most frames publish the same values
                    let same = unsafe {
                        (fns.data_has_user_value)(data, key.as_ptr()) && SettingValue::read(fns, data, &key, value.kind()) == value
                    };
                    if !same {
                        unsafe { value.write(fns, data, &key) };
                    }
                }
                if settings.is_none() {
                    unsafe { (fns.data_release)(data) };
                }
            }
        }
        if effects.fill_canvas {
            if view.source_name.is_empty() {
                trust_log(&format!("[Rhai] ⚠️ {}: fill_canvas() before the filter is on a source", self.ty.path().display()));
            } else {
                queue_fill_canvas(fns, &view.source_name, view.canvas);
            }
        }
    }
}

/// A fill_canvas() waiting for OBS's UI thread.
struct FillCanvas {
    source: CString,
    canvas: (u32, u32),
}

fn queue_fill_canvas(fns: &Fns, source: &str, canvas: (u32, u32)) {
    let job = Box::into_raw(Box::new(FillCanvas { source: cstring(source), canvas })) as P;
    unsafe { (fns.queue_task)(OBS_TASK_UI, fill_canvas_task, job, false) };
}

/// On OBS's UI thread: stretches the source over the whole canvas in the
/// current scene (in a group too): top left at 0,0, the canvas as its bounds.
unsafe extern "C" fn fill_canvas_task(param: P) {
    let job = unsafe { Box::from_raw(param as *mut FillCanvas) };
    let (Some(fns), Some(scene_fns)) = (fns(), scene_fns()) else { return };
    unsafe {
        let scene_source = (scene_fns.get_current_scene)();
        if scene_source.is_null() {
            return;
        }
        let scene = (scene_fns.scene_from_source)(scene_source);
        let item = if scene.is_null() { std::ptr::null_mut() } else { (scene_fns.find_source_recursive)(scene, job.source.as_ptr()) };
        if item.is_null() {
            trust_log(&format!("[Rhai] fill_canvas: \"{}\" isn't in the current scene", job.source.to_string_lossy()));
        } else {
            (scene_fns.set_alignment)(item, OBS_ALIGN_TOP_LEFT);
            (scene_fns.set_pos)(item, &[0.0, 0.0]);
            (scene_fns.set_bounds)(item, &[job.canvas.0 as f32, job.canvas.1 as f32]);
            (scene_fns.set_bounds_type)(item, OBS_BOUNDS_STRETCH);
        }
        (fns.source_release)(scene_source);
    }
}

unsafe extern "C" fn get_name(type_data: P) -> *const c_char {
    let ty = unsafe { &*(type_data as *const ScriptType) };
    ty.name.as_ptr()
}

unsafe extern "C" fn canvas_width(_data: P) -> u32 {
    fns().map(|f| canvas_size(f).0).unwrap_or(1920)
}

unsafe extern "C" fn canvas_height(_data: P) -> u32 {
    fns().map(|f| canvas_size(f).1).unwrap_or(1080)
}

/// A source's size is what its script says (this.size).
unsafe extern "C" fn own_width(data: P) -> u32 {
    if data.is_null() { 0 } else { unsafe { &*(data as *const Instance) }.own_size().0 }
}

unsafe extern "C" fn own_height(data: P) -> u32 {
    if data.is_null() { 0 } else { unsafe { &*(data as *const Instance) }.own_size().1 }
}

impl Instance {
    fn own_size(&self) -> (u32, u32) {
        let packed = self.size.load(std::sync::atomic::Ordering::Relaxed);
        ((packed >> 32) as u32, packed as u32)
    }

    /// Takes the size a source's script asked for (this.size = [w, h]).
    fn note_size(&self, state: &InstanceState) {
        let asked = state.this.read_lock::<Map>().and_then(|m| {
            let size = m.get("size")?.read_lock::<Array>()?.clone();
            let n = |d: &Dynamic| d.as_int().ok().or_else(|| d.as_float().ok().map(|f| f.round() as i64));
            Some((n(size.first()?)?, n(size.get(1)?)?))
        });
        if let Some((w, h)) = asked {
            let (w, h) = (w.clamp(1, 8192) as u64, h.clamp(1, 8192) as u64);
            self.size.store((w << 32) | h, std::sync::atomic::Ordering::Relaxed);
        }
    }
}

/// A button property was clicked (OBS's UI thread): its source's script
/// hears of it at its next tick (on_button).
unsafe extern "C" fn button_clicked(_props: P, _property: P, link: P) -> bool {
    if !link.is_null() {
        if let Link::Button { instance, key } = unsafe { &*(link as *const Link) } {
            PENDING_BUTTONS.lock().push((*instance, key.clone()));
        }
    }
    false
}

/// The type a filter belongs to, by its id (create gets no type_data).
fn type_of(source: P) -> Option<&'static ScriptType> {
    let fns = fns()?;
    let id = unsafe { (fns.source_get_id)(source) };
    if id.is_null() {
        return None;
    }
    let id = unsafe { CStr::from_ptr(id) };
    TYPES.read().iter().copied().find(|t| t.id.as_c_str() == id)
}

unsafe extern "C" fn create(settings: P, source: P) -> P {
    let Some(ty) = type_of(source) else { return std::ptr::null_mut() };
    ty.instances.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let compiled = ty.compiled();
    // A source with on_event hears the relay (connecting on the first)
    let events = (compiled.kind == ScriptKind::Source && compiled.has("on_event")).then(crate::script_feed::subscribe);
    let instance = Box::new(Instance {
        ty,
        source,
        id: NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        size: std::sync::atomic::AtomicU64::new((400u64 << 32) | 600),
        events,
        feed: Mutex::new(None),
        state: Mutex::new(InstanceState {
            this: new_this(),
            uniforms: None,
            version: 0,
            effect: 0,
            effect_failed: false,
            last_error: None,
            cmds: Vec::new(),
            canvas: crate::script_draw::Canvas::new(),
        }),
    });
    let data = Box::into_raw(instance) as P;
    unsafe { update(data, settings) };
    data
}

unsafe extern "C" fn destroy(data: P) {
    if data.is_null() {
        return;
    }
    let instance = unsafe { Box::from_raw(data as *mut Instance) };
    instance.ty.instances.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
    let (effect, mut canvas) = {
        let mut state = instance.state.lock();
        (state.effect, std::mem::replace(&mut state.canvas, crate::script_draw::Canvas::new()))
    };
    if effect != 0 || !canvas.is_empty() {
        if let Some(fns) = fns() {
            // No script lock here (the Lua filters' trap): entering graphics is safe
            unsafe {
                (fns.enter_graphics)();
                if effect != 0 {
                    (fns.effect_destroy)(effect as P);
                }
                canvas.free();
                (fns.leave_graphics)();
            }
        }
    }
}

unsafe extern "C" fn get_defaults2(type_data: P, settings: P) {
    let (Some(fns), false) = (fns(), type_data.is_null()) else { return };
    let ty = unsafe { &*(type_data as *const ScriptType) };
    let compiled = ty.compiled();
    let props = properties(&compiled).unwrap_or_default();
    let kinds = setting_kinds(&compiled, &props);
    for (key, value) in &compiled.defaults {
        let is_color = kinds.iter().any(|(k, kind)| k == key.as_str() && *kind == SettingKind::Color);
        let key = cstring(key);
        unsafe {
            if let Ok(b) = value.as_bool() {
                (fns.data_set_default_bool)(settings, key.as_ptr(), b);
            } else if let Ok(i) = value.as_int() {
                // OBS keeps colours as 0xAABBGGRR
                let i = if is_color { swap_red_blue(i as u32) as i64 } else { i };
                (fns.data_set_default_int)(settings, key.as_ptr(), i);
            } else if let Ok(f) = value.as_float() {
                (fns.data_set_default_double)(settings, key.as_ptr(), f);
            } else if value.is_string() {
                let s = cstring(&value.to_string());
                (fns.data_set_default_string)(settings, key.as_ptr(), s.as_ptr());
            }
        }
    }
}

/// A property linked to another filter's setting (remote()), as its
/// properties' callbacks use it.
struct RemoteLink {
    /// This filter's setting
    local: CString,
    kind: SettingKind,
    /// This filter's setting naming the source the other filter is on
    source_key: CString,
    filter_id: String,
    /// The other filter's setting
    remote: CString,
}

impl RemoteLink {
    fn new(local: &str, kind: SettingKind, remote: &Remote) -> Self {
        RemoteLink {
            local: cstring(local),
            kind,
            source_key: cstring(&remote.source_key),
            filter_id: remote.filter_id.clone(),
            remote: cstring(&remote.key),
        }
    }
}

enum Link {
    /// Changing this setting changes the other filter's
    Push(RemoteLink),
    /// Picking a source copies in the other filter's values
    Pull(Vec<RemoteLink>),
    /// A button of the source `instance` (Instance::id)
    Button { instance: u64, key: String },
}

unsafe extern "C" fn free_links(param: P) {
    drop(unsafe { Box::from_raw(param as *mut Vec<Box<Link>>) });
}

/// `source`'s first filter of id `filter_id`, with a reference to release;
/// null if it has none.
unsafe fn find_filter(fns: &Fns, source: P, filter_id: &str) -> P {
    struct Search<'a> {
        fns: &'a Fns,
        id: &'a str,
        found: P,
    }
    unsafe extern "C" fn visit(_parent: P, child: P, param: P) {
        let search = unsafe { &mut *(param as *mut Search<'_>) };
        if search.found.is_null() && unsafe { string_from((search.fns.source_get_id)(child)) } == search.id {
            search.found = unsafe { (search.fns.source_get_ref)(child) };
        }
    }
    let mut search = Search { fns, id: filter_id, found: std::ptr::null_mut() };
    unsafe { (fns.source_enum_filters)(source, visit, &mut search as *mut Search<'_> as P) };
    search.found
}

/// The names of the sources carrying a `filter_id` filter (all of them for
/// ""). Takes OBS's source list lock: from its UI thread only, never a tick.
unsafe fn sources_with_filter(fns: &Fns, filter_id: &str) -> Vec<String> {
    unsafe extern "C" fn collect(param: P, source: P) -> bool {
        let list = unsafe { &mut *(param as *mut (&Fns, Vec<P>)) };
        // A reference each: their filters are looked at once OBS lets go of its list
        let source = unsafe { (list.0.source_get_ref)(source) };
        if !source.is_null() {
            list.1.push(source);
        }
        true
    }
    let mut list: (&Fns, Vec<P>) = (fns, Vec::new());
    unsafe { (fns.enum_sources)(collect, &mut list as *mut (&Fns, Vec<P>) as P) };
    let mut names: Vec<String> = Vec::new();
    for source in list.1 {
        unsafe {
            let carries = filter_id.is_empty() || {
                let filter = find_filter(fns, source, filter_id);
                if !filter.is_null() {
                    (fns.source_release)(filter);
                }
                !filter.is_null()
            };
            let name = string_from((fns.source_get_name)(source));
            if carries && !names.contains(&name) {
                names.push(name);
            }
            (fns.source_release)(source);
        }
    }
    names
}

/// The other filter `link` is linked to, for the settings `settings` (with
/// a reference to release); null if there's none. OBS's UI thread only.
unsafe fn linked_filter(fns: &Fns, settings: P, link: &RemoteLink) -> P {
    unsafe {
        let name = (fns.data_get_string)(settings, link.source_key.as_ptr());
        if name.is_null() || *name == 0 {
            return std::ptr::null_mut();
        }
        let source = (fns.get_source_by_name)(name);
        if source.is_null() {
            return std::ptr::null_mut();
        }
        let filter = find_filter(fns, source, &link.filter_id);
        (fns.source_release)(source);
        filter
    }
}

/// A linked property changed: the other filter gets its value.
unsafe extern "C" fn push_modified(link: P, _props: P, _property: P, settings: P) -> bool {
    let (Some(fns), false) = (fns(), link.is_null()) else { return false };
    let Link::Push(link) = (unsafe { &*(link as *const Link) }) else { return false };
    unsafe {
        let filter = linked_filter(fns, settings, link);
        if filter.is_null() {
            return false;
        }
        let value = SettingValue::read(fns, settings, &link.local, link.kind);
        let theirs = (fns.source_get_settings)(filter);
        let same = !theirs.is_null() && SettingValue::read(fns, theirs, &link.remote, link.kind) == value;
        if !theirs.is_null() {
            (fns.data_release)(theirs);
        }
        if !same {
            let change = (fns.data_create)();
            value.write(fns, change, &link.remote);
            (fns.source_update)(filter, change);
            (fns.data_release)(change);
        }
        (fns.source_release)(filter);
    }
    false
}

/// A source was picked: the properties linked to it take its filter's values.
unsafe extern "C" fn pull_modified(link: P, _props: P, _property: P, settings: P) -> bool {
    let (Some(fns), false) = (fns(), link.is_null()) else { return false };
    let Link::Pull(links) = (unsafe { &*(link as *const Link) }) else { return false };
    for link in links {
        unsafe {
            let filter = linked_filter(fns, settings, link);
            if filter.is_null() {
                continue;
            }
            let theirs = (fns.source_get_settings)(filter);
            if !theirs.is_null() {
                SettingValue::read(fns, theirs, &link.remote, link.kind).write(fns, settings, &link.local);
                (fns.data_release)(theirs);
            }
            (fns.source_release)(filter);
        }
    }
    // Shown again, with the values copied in
    true
}

unsafe extern "C" fn get_properties2(data: P, type_data: P) -> P {
    let Some(fns) = fns() else { return std::ptr::null_mut() };
    let props = unsafe { (fns.properties_create)() };
    if type_data.is_null() {
        return props;
    }
    let ty = unsafe { &*(type_data as *const ScriptType) };
    let compiled = ty.compiled();
    let list = match declared_properties(&compiled) {
        Ok(list) => list,
        Err(e) => {
            trust_log(&format!("[Rhai] ⚠️ {}: {}", ty.path().display(), e));
            return props;
        }
    };
    // Which source's properties these are (none for its type's defaults)
    let instance_id = if data.is_null() { 0 } else { unsafe { (*(data as *const Instance)).id } };
    let kinds = setting_kinds(&compiled, &list.iter().map(|(p, _)| p.clone()).collect::<Vec<_>>());
    let kind_of = |key: &str| kinds.iter().find(|(k, _)| k == key).map(|(_, kind)| *kind).unwrap_or(SettingKind::Float);
    let mut links: Vec<Box<Link>> = Vec::new();
    for (n, (prop, remote)) in list.iter().enumerate() {
        let property = unsafe {
            match prop {
                Prop::Slider { key, label, min, max, step } => {
                    (fns.add_float_slider)(props, cstring(key).as_ptr(), cstring(label).as_ptr(), *min, *max, *step)
                }
                Prop::IntSlider { key, label, min, max, step } => {
                    (fns.add_int_slider)(props, cstring(key).as_ptr(), cstring(label).as_ptr(), *min as i32, *max as i32, *step as i32)
                }
                Prop::IntInput { key, label, min, max, step } => {
                    (fns.add_int)(props, cstring(key).as_ptr(), cstring(label).as_ptr(), *min as i32, *max as i32, *step as i32)
                }
                Prop::Toggle { key, label } => (fns.add_bool)(props, cstring(key).as_ptr(), cstring(label).as_ptr()),
                Prop::Choice { key, label, options } => {
                    let texts = matches!(options.first(), Some((_, ChoiceValue::Text(_))));
                    let format = if texts { OBS_COMBO_FORMAT_STRING } else { OBS_COMBO_FORMAT_INT };
                    let list = (fns.add_list)(props, cstring(key).as_ptr(), cstring(label).as_ptr(), OBS_COMBO_TYPE_LIST, format);
                    for (text, value) in options {
                        match value {
                            ChoiceValue::Text(v) if texts => {
                                (fns.list_add_string)(list, cstring(text).as_ptr(), cstring(v).as_ptr());
                            }
                            ChoiceValue::Int(v) if !texts => {
                                (fns.list_add_int)(list, cstring(text).as_ptr(), *v);
                            }
                            // One list holds one kind of value: a mixed option is left out
                            _ => {}
                        }
                    }
                    list
                }
                Prop::SourceList { key, label, filter } => {
                    let list = (fns.add_list)(props, cstring(key).as_ptr(), cstring(label).as_ptr(), OBS_COMBO_TYPE_LIST, OBS_COMBO_FORMAT_STRING);
                    (fns.list_add_string)(list, c"(None)".as_ptr(), c"".as_ptr());
                    for name in sources_with_filter(fns, filter) {
                        let name = cstring(&name);
                        (fns.list_add_string)(list, name.as_ptr(), name.as_ptr());
                    }
                    list
                }
                Prop::Text { key, label } => (fns.add_text)(props, cstring(key).as_ptr(), cstring(label).as_ptr(), OBS_TEXT_DEFAULT),
                Prop::Info { label } => {
                    let key = cstring(&format!("info_{}", n));
                    (fns.add_text)(props, key.as_ptr(), cstring(label).as_ptr(), OBS_TEXT_INFO)
                }
                Prop::Color { key, label } => (fns.add_color_alpha)(props, cstring(key).as_ptr(), cstring(label).as_ptr()),
                Prop::File { key, label, filter } => {
                    let filter = cstring(if filter.is_empty() { "All files (*.*)" } else { filter });
                    (fns.add_path)(props, cstring(key).as_ptr(), cstring(label).as_ptr(), OBS_PATH_FILE, filter.as_ptr(), std::ptr::null())
                }
                Prop::FontList { key, label } => {
                    let list = (fns.add_list)(props, cstring(key).as_ptr(), cstring(label).as_ptr(), OBS_COMBO_TYPE_LIST, OBS_COMBO_FORMAT_STRING);
                    for family in crate::script_draw::font_families() {
                        let family = cstring(&family);
                        (fns.list_add_string)(list, family.as_ptr(), family.as_ptr());
                    }
                    list
                }
                Prop::Button { key, label } => {
                    let link = Box::new(Link::Button { instance: instance_id, key: key.clone() });
                    let name = cstring(&format!("button_{}", key));
                    let button = (fns.add_button2)(props, name.as_ptr(), cstring(label).as_ptr(), button_clicked, &*link as *const Link as P);
                    links.push(link);
                    button
                }
            }
        };
        if property.is_null() {
            continue;
        }
        // Linked to another filter's setting: changing it here changes that one
        if let (Some(remote), Some(key)) = (remote, prop.key()) {
            let link = Box::new(Link::Push(RemoteLink::new(key, kind_of(key), remote)));
            unsafe { (fns.property_set_modified_callback2)(property, push_modified, &*link as *const Link as P) };
            links.push(link);
        }
        // Picking a source here copies in the values linked to it
        if let Prop::SourceList { key, .. } = prop {
            let pulls: Vec<RemoteLink> = list
                .iter()
                .filter_map(|(p, r)| {
                    let (r, k) = (r.as_ref()?, p.key()?);
                    (r.source_key == *key).then(|| RemoteLink::new(k, kind_of(k), r))
                })
                .collect();
            if !pulls.is_empty() {
                let link = Box::new(Link::Pull(pulls));
                unsafe { (fns.property_set_modified_callback2)(property, pull_modified, &*link as *const Link as P) };
                links.push(link);
            }
        }
    }
    // The links live as long as these properties (OBS frees them with them)
    if !links.is_empty() {
        let owned = Box::into_raw(Box::new(links)) as P;
        unsafe { (fns.properties_set_param)(props, owned, Some(free_links)) };
    }
    props
}

unsafe extern "C" fn update(data: P, settings: P) {
    let (Some(fns), false) = (fns(), data.is_null()) else { return };
    let instance = unsafe { &*(data as *const Instance) };
    let compiled = instance.ty.compiled();
    let props = properties(&compiled).unwrap_or_default();
    let kinds = setting_kinds(&compiled, &props);
    let mut values = Map::new();
    for (key, kind) in &kinds {
        let value = unsafe { SettingValue::read(fns, settings, &cstring(key), *kind) };
        values.insert(key.as_str().into(), value.to_dynamic());
    }
    let view = unsafe { View::for_instance(fns, instance, &compiled) };
    let effects = {
        let mut state = instance.state.lock();
        state.set("settings", Dynamic::from_map(values));
        state.set_view(&view);
        if !compiled.has("update") {
            return;
        }
        let (result, effects) = call_collecting(&compiled, &mut state.this, "update", ());
        match result {
            Ok(changes) => {
                // The settings it changes
                if let Some(changes) = changes.try_cast::<Map>() {
                    let kind_of = |k: &str| kinds.iter().find(|(key, _)| key == k).map(|(_, kind)| *kind);
                    let mut written = Map::new();
                    for (key, value) in changes {
                        let Some(value) = SettingValue::from_dynamic(&value, kind_of(&key)) else { continue };
                        unsafe { value.write(fns, settings, &cstring(&key)) };
                        written.insert(key, value.to_dynamic());
                    }
                    if let Some(mut this) = state.this.write_lock::<Map>() {
                        if let Some(mut current) = this.get_mut("settings").and_then(|s| s.write_lock::<Map>()) {
                            current.extend(written);
                        }
                    }
                }
                state.report(instance.ty, None);
            }
            Err(e) => state.report(instance.ty, Some(e)),
        }
        instance.note_size(&state);
        effects
    };
    unsafe { instance.apply(fns, effects, Some(settings), &view) };
}

unsafe extern "C" fn video_tick(data: P, seconds: f32) {
    let (Some(fns), false) = (fns(), data.is_null()) else { return };
    let instance = unsafe { &*(data as *const Instance) };
    let compiled = instance.ty.compiled();
    // OBS's side first, before the script: its sizes and whether it's on
    let view = unsafe { View::for_instance(fns, instance, &compiled) };
    let enabled = unsafe { (fns.source_enabled)(instance.source) };
    // A source's button clicks and relay messages since its last tick
    let clicks: Vec<String> = if compiled.kind == ScriptKind::Source && compiled.has("on_button") {
        let mut pending = PENDING_BUTTONS.lock();
        let (mine, others): (Vec<_>, Vec<_>) = pending.drain(..).partition(|(id, _)| *id == instance.id);
        *pending = others;
        mine.into_iter().map(|(_, key)| key).collect()
    } else {
        Vec::new()
    };
    let mut messages: Vec<Arc<str>> = instance.events.as_ref().map(|rx| rx.try_iter().take(MAX_EVENTS_PER_TICK).collect()).unwrap_or_default();

    let mut all_effects = Vec::new();
    {
        let mut state = instance.state.lock();
        state.set("enabled", Dynamic::from_bool(enabled));
        state.set_view(&view);
        let time = state
            .this
            .read_lock::<Map>()
            .and_then(|m| m.get("time").and_then(|t| t.as_float().ok()))
            .unwrap_or(0.0);
        state.set("time", Dynamic::from_float(time + seconds as f64));
        // Its own WebSocket (this.feed): listened to, switched when it
        // changes, and its messages with the relay's
        if compiled.kind == ScriptKind::Source && compiled.has("on_event") {
            let url = state
                .this
                .read_lock::<Map>()
                .and_then(|m| m.get("feed").and_then(|f| f.clone().into_string().ok()))
                .map(|u| u.trim().to_string())
                .unwrap_or_default();
            let mut feed = instance.feed.lock();
            if feed.as_ref().map(|(u, _)| u.as_str()) != Some(url.as_str()) {
                *feed = (!url.is_empty()).then(|| (url.clone(), crate::script_feed::subscribe_url(&url)));
            }
            if let Some((_, rx)) = feed.as_ref() {
                messages.extend(rx.try_iter().take(MAX_EVENTS_PER_TICK));
            }
        }
        for key in clicks {
            let (result, effects) = call_collecting(&compiled, &mut state.this, "on_button", (key,));
            if let Err(e) = result {
                state.report(instance.ty, Some(e));
            }
            all_effects.push(effects);
        }
        for message in messages {
            let (result, effects) = call_collecting(&compiled, &mut state.this, "on_event", (Dynamic::from_map(event_map(&message)),));
            if let Err(e) = result {
                state.report(instance.ty, Some(e));
            }
            all_effects.push(effects);
        }
        let (result, effects) = call_collecting(&compiled, &mut state.this, "tick", (seconds as f64,));
        match result {
            // A source: what to draw
            Ok(drawing) if compiled.kind == ScriptKind::Source => {
                state.cmds = drawing.try_cast::<Array>().map(|list| crate::script_draw::parse(&list)).unwrap_or_default();
                state.report(instance.ty, None);
            }
            Ok(values) => {
                state.uniforms = values.try_cast::<Map>().map(|map| {
                    map.into_iter().map(|(name, value)| (cstring(&name), value)).collect()
                });
                state.report(instance.ty, None);
            }
            Err(e) => {
                state.uniforms = None;
                state.report(instance.ty, Some(e));
            }
        }
        instance.note_size(&state);
        all_effects.push(effects);
    }
    for effects in all_effects {
        unsafe { instance.apply(fns, effects, None, &view) };
    }
}

unsafe extern "C" fn video_render(data: P, _effect: P) {
    let (Some(fns), false) = (fns(), data.is_null()) else { return };
    let instance = unsafe { &*(data as *const Instance) };
    let source = instance.source;
    let compiled = instance.ty.compiled();
    // A source draws its list (script_draw), clipped to its size
    if compiled.kind == ScriptKind::Source {
        let size = instance.own_size();
        let mut state = instance.state.lock();
        let state = &mut *state;
        unsafe { crate::script_draw::render(&mut state.canvas, &state.cmds, size) };
        return;
    }
    let version = instance.ty.version.load(std::sync::atomic::Ordering::SeqCst);

    // Graphics context held here: remake the shader after a reload
    let (effect, uniforms) = {
        let mut state = instance.state.lock();
        if state.version != version {
            if state.effect != 0 {
                unsafe { (fns.effect_destroy)(state.effect as P) };
            }
            state.effect = 0;
            state.effect_failed = false;
            state.version = version;
        }
        if state.effect == 0 && !state.effect_failed {
            let code = cstring(&compiled.shader);
            let name = cstring(&format!("{}.effect", compiled.id));
            let mut error: *mut c_char = std::ptr::null_mut();
            let effect = unsafe { (fns.effect_create)(code.as_ptr(), name.as_ptr(), &mut error) };
            if effect.is_null() {
                state.effect_failed = true;
                let why = unsafe { string_from(error) };
                state.report(instance.ty, Some(format!("the shader didn't compile: {}", why.trim())));
            } else {
                unsafe { zero_params(fns, effect) };
                state.effect = effect as usize;
            }
        }
        (state.effect as P, state.uniforms.clone())
    };
    // No lock held from here: rendering the source may run other filters
    let Some(uniforms) = uniforms.filter(|_| !effect.is_null()) else {
        unsafe { (fns.skip_video_filter)(source) };
        return;
    };
    // 0, 0: its source's size
    let (width, height) = if compiled.size == Size::Canvas { canvas_size(fns) } else { (0, 0) };
    unsafe {
        if !(fns.process_filter_begin)(source, GS_RGBA, OBS_NO_DIRECT_RENDERING) {
            return;
        }
        for (name, value) in &uniforms {
            set_param(fns, effect, name, value);
        }
        if compiled.blend == Blend::Premultiplied {
            (fns.blend_state_push)();
            (fns.blend_function_separate)(GS_BLEND_ONE, GS_BLEND_INVSRCALPHA, GS_BLEND_ONE, GS_BLEND_INVSRCALPHA);
        }
        (fns.process_filter_end)(source, effect, width, height);
        if compiled.blend == Blend::Premultiplied {
            (fns.blend_state_pop)();
        }
    }
}

/// The size in bytes of a shader parameter of kind `kind`
/// (enum gs_shader_param_type), None for those Theater doesn't set.
fn param_size(kind: i32) -> Option<usize> {
    match kind {
        1..=3 => Some(4), // bool, float, int
        5 | 8 => Some(8), // vec2, int2
        6 | 9 => Some(12), // vec3, int3
        7 | 10 => Some(16), // vec4, int4
        _ => None,        // unknown, string, matrix, texture
    }
}

/// Every number setting of `effect` starts at 0: Direct3D won't draw with a
/// shader while any of them has never been set.
unsafe fn zero_params(fns: &Fns, effect: P) {
    let zeros = [0u8; 16];
    for i in 0..unsafe { (fns.effect_get_num_params)(effect) } {
        let param = unsafe { (fns.effect_get_param_by_idx)(effect, i) };
        if param.is_null() {
            continue;
        }
        let mut info = ParamInfo { name: std::ptr::null(), kind: 0 };
        unsafe { (fns.effect_get_param_info)(param, &mut info) };
        if let Some(size) = param_size(info.kind) {
            unsafe { (fns.effect_set_val)(param, zeros.as_ptr() as *const c_void, size) };
        }
    }
}

/// Sets the shader parameter `name` from a script value: a number, a bool,
/// or an array of numbers for a vector.
unsafe fn set_param(fns: &Fns, effect: P, name: &CStr, value: &Dynamic) {
    let param = unsafe { (fns.effect_get_param_by_name)(effect, name.as_ptr()) };
    if param.is_null() {
        return;
    }
    let mut info = ParamInfo { name: std::ptr::null(), kind: 0 };
    unsafe { (fns.effect_get_param_info)(param, &mut info) };
    let Some(size) = param_size(info.kind) else { return };
    let bytes = param_bytes(info.kind, value);
    if bytes.len() == size {
        unsafe { (fns.effect_set_val)(param, bytes.as_ptr() as *const c_void, size) };
    }
}

/// A script value as the bytes of a shader parameter of kind `kind`.
fn param_bytes(kind: i32, value: &Dynamic) -> Vec<u8> {
    let number = |v: &Dynamic| v.as_float().ok().or_else(|| v.as_int().ok().map(|i| i as f64)).or_else(|| v.as_bool().ok().map(|b| b as i32 as f64));
    let numbers: Vec<f64> = match value.clone().try_cast::<Array>() {
        Some(array) => array.iter().filter_map(number).collect(),
        None => number(value).into_iter().collect(),
    };
    let ints = |n: usize| numbers.iter().take(n).flat_map(|v| (*v as i32).to_ne_bytes()).collect::<Vec<u8>>();
    let floats = |n: usize| numbers.iter().take(n).flat_map(|v| (*v as f32).to_ne_bytes()).collect::<Vec<u8>>();
    match kind {
        1 | 3 => ints(1),
        2 => floats(1),
        5 => floats(2),
        6 => floats(3),
        7 => floats(4),
        8 => ints(2),
        9 => ints(3),
        10 => ints(4),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn transparency_fix() -> Option<Compiled> {
        let path = crate::studio_app::get_filters_dir().join("transparency_fix.rhai");
        let source = std::fs::read_to_string(&path).ok()?;
        Some(compile(&source, &path).expect("transparency_fix.rhai compiles"))
    }

    #[test]
    fn a_filter_script_declares_its_filter() {
        let source = r#"
            const ID = "test_filter";
            const NAME = "Test Filter";
            const BLEND = "premultiplied";
            const SHADER = `uniform float strength;`;
            fn defaults() { #{ strength: 1.0, mode: 2, on: true, label: "x" } }
            fn properties() { [ slider("strength", "Strength", 0, 2, 0.01), choice("mode", "Mode", [["A", 0], ["B", 2]]), toggle("on", "On"), info("Hello") ] }
            fn tick(dt) { this.count = (this.count ?? 0) + 1; #{ strength: this.settings.strength * 2.0, count: this.count } }
        "#;
        let c = compile(source, Path::new("test.rhai")).unwrap();
        assert_eq!((c.id.as_str(), c.name.as_str(), c.blend), ("test_filter", "Test Filter", Blend::Premultiplied));
        assert_eq!(c.shader, "uniform float strength;");
        let props = properties(&c).unwrap();
        assert_eq!(props[0], Prop::Slider { key: "strength".into(), label: "Strength".into(), min: 0.0, max: 2.0, step: 0.01 });
        assert!(matches!(&props[1], Prop::Choice { options, .. } if options.len() == 2));
        assert_eq!(props[3], Prop::Info { label: "Hello".into() });
        let kinds = setting_kinds(&c, &props);
        assert!(kinds.contains(&("strength".into(), SettingKind::Float)));
        assert!(kinds.contains(&("mode".into(), SettingKind::Int)));
        assert!(kinds.contains(&("on".into(), SettingKind::Bool)));
        assert!(kinds.contains(&("label".into(), SettingKind::Text)));

        // tick keeps its own state in `this` between calls
        let mut this = new_this();
        let mut settings = Map::new();
        settings.insert("strength".into(), Dynamic::from_float(0.25));
        this.write_lock::<Map>().unwrap().insert("settings".into(), Dynamic::from_map(settings));
        let _ = call(&c, Some(&mut this), "tick", (0.03f64,)).unwrap();
        let out = call(&c, Some(&mut this), "tick", (0.03f64,)).unwrap().try_cast::<Map>().unwrap();
        assert_eq!(out["strength"].as_float().unwrap(), 0.5);
        assert_eq!(out["count"].as_int().unwrap(), 2);
    }

    #[test]
    fn broken_or_runaway_scripts_are_refused() {
        let p = Path::new("x.rhai");
        assert!(compile("const NAME = \"x\"; const SHADER = ``; fn tick(dt) {}", p).unwrap_err().contains("ID"));
        assert!(compile("const ID = \"x\"; fn tick(dt) {}", p).unwrap_err().contains("SHADER"));
        assert!(compile("const ID = \"x\"; const SHADER = ``;", p).unwrap_err().contains("tick"));
        assert!(compile("const ID = \"x\"; const SHADER = ``; fn tick(dt) { let a = ; }", p).is_err());
        let spin = compile("const ID = \"x\"; const SHADER = ``; fn tick(dt) { loop { } }", p).unwrap();
        let err = call(&spin, Some(&mut new_this()), "tick", (0.0f64,)).unwrap_err();
        assert!(err.contains("Too many operations"), "{}", err);
        // No file or system access
        assert!(engine().eval::<Dynamic>(r#"open_file("C:/x.txt")"#).is_err());
    }

    #[test]
    fn filters_share_values() {
        engine().eval::<()>(r#"share("test/camera", #{ pan: 12.5 })"#).unwrap();
        let pan: f64 = engine().eval(r#"shared_value("test/camera").pan"#).unwrap();
        assert_eq!(pan, 12.5);
        assert!(engine().eval::<Dynamic>(r#"shared_value("test/nothing")"#).unwrap().is_unit());
    }

    #[test]
    fn shader_values_become_parameter_bytes() {
        assert_eq!(param_bytes(2, &Dynamic::from_float(1.5)), 1.5f32.to_ne_bytes().to_vec());
        assert_eq!(param_bytes(2, &Dynamic::from_int(2)), 2.0f32.to_ne_bytes().to_vec());
        assert_eq!(param_bytes(1, &Dynamic::TRUE), 1i32.to_ne_bytes().to_vec());
        let v: Array = vec![Dynamic::from_float(1.0), Dynamic::from_int(2)];
        let bytes = param_bytes(5, &Dynamic::from_array(v));
        assert_eq!(bytes, [1.0f32.to_ne_bytes(), 2.0f32.to_ne_bytes()].concat());
        // A vector given too few numbers isn't set
        assert_eq!(param_bytes(7, &Dynamic::from_float(1.0)).len(), 4);
    }

    #[test]
    fn transparency_fix_ports_the_lua_filter() {
        let Some(c) = transparency_fix() else { return };
        assert_eq!(c.id, "theater_transparency_fix_filter");
        assert_eq!(c.blend, Blend::Premultiplied);
        assert!(c.shader.contains("technique Draw") && c.shader.contains("PSFix"));
        assert_eq!(c.defaults["alpha_mode"].as_int().unwrap(), 2);
        assert_eq!(c.defaults["alpha_boost"].as_float().unwrap(), 1.0);
        // Its shader is the Lua version's
        let lua = std::fs::read_to_string(crate::studio_app::get_filters_dir().join("transparency_fix.lua")).unwrap_or_default();
        if let Some(lua_shader) = crate::obs_source::extract_shader_code_from_lua(&lua) {
            assert_eq!(c.shader.trim(), lua_shader.trim());
        }

        let tick = |mode: i64, boost: f64| {
            let mut this = new_this();
            let mut settings = Map::new();
            settings.insert("alpha_mode".into(), Dynamic::from_int(mode));
            settings.insert("alpha_boost".into(), Dynamic::from_float(boost));
            this.write_lock::<Map>().unwrap().insert("settings".into(), Dynamic::from_map(settings));
            call(&c, Some(&mut this), "tick", (0.03f64,)).unwrap()
        };
        // Standard: the source is drawn untouched
        assert!(tick(0, 1.0).is_unit());
        let out = tick(2, 3.0).try_cast::<Map>().unwrap();
        assert_eq!(out["alpha_mode"].as_int().unwrap(), 2);
        assert_eq!(out["alpha_boost"].as_float().unwrap(), 3.0);
    }

    #[test]
    fn filters_publish_runtime_values_and_ask_for_obs_work() {
        let source = r#"
            const ID = "t"; const SHADER = ``;
            fn update() { if this.settings.run { fill_canvas(); #{ run: false, size: this.source_width } } }
            fn tick(dt) { publish(#{ _live: this.time }); () }
        "#;
        let c = compile(source, Path::new("t.rhai")).unwrap();
        let mut this = new_this();
        View { source_name: "Cam".into(), source_width: 640, ..View::default() }.set_into(&mut this.write_lock::<Map>().unwrap());
        let mut settings = Map::new();
        settings.insert("run".into(), Dynamic::TRUE);
        this.write_lock::<Map>().unwrap().insert("settings".into(), Dynamic::from_map(settings));
        let (changes, effects) = call_collecting(&c, &mut this, "update", ());
        let changes = changes.unwrap().try_cast::<Map>().unwrap();
        assert!(effects.fill_canvas);
        assert_eq!((changes["run"].as_bool(), changes["size"].as_int()), (Ok(false), Ok(640)));
        let (_, effects) = call_collecting(&c, &mut this, "tick", (0.1f64,));
        assert_eq!(effects.published.len(), 1);
        assert_eq!(effects.published[0].0, "_live");
        assert!(!effects.fill_canvas);

        // A real setting can't be published over every frame
        let bad = compile("const ID = \"t\"; const SHADER = ``; fn tick(dt) { publish(#{ pan: 1.0 }) }", Path::new("t.rhai")).unwrap();
        let (result, effects) = call_collecting(&bad, &mut new_this(), "tick", (0.1f64,));
        assert!(result.unwrap_err().contains("must start with _"));
        assert!(effects.published.is_empty());
    }

    #[test]
    fn remote_properties_and_source_lists_are_declared() {
        let source = r#"
            const ID = "t"; const SHADER = ``; const SIZE = "canvas";
            fn properties() { [
                source_list("master", "Master", "m_filter"),
                int_input("w", "Width", 1, 8192, 1),
                remote(slider("m_pan", "Pan", -180.0, 180.0, 0.1), "master", "m_filter", "pan"),
            ] }
            fn tick(dt) { () }
        "#;
        let c = compile(source, Path::new("t.rhai")).unwrap();
        assert_eq!(c.size, Size::Canvas);
        let props = declared_properties(&c).unwrap();
        assert_eq!(props[0], (Prop::SourceList { key: "master".into(), label: "Master".into(), filter: "m_filter".into() }, None));
        assert!(matches!(props[1], (Prop::IntInput { min: 1, max: 8192, .. }, None)));
        assert_eq!(props[2].1, Some(Remote { source_key: "master".into(), filter_id: "m_filter".into(), key: "pan".into() }));
        let kinds = setting_kinds(&c, &properties(&c).unwrap());
        assert!(kinds.contains(&("master".into(), SettingKind::Text)) && kinds.contains(&("w".into(), SettingKind::Int)));
        assert!(compile("const ID = \"t\"; const SHADER = ``; const SIZE = \"huge\"; fn tick(dt) {}", Path::new("t.rhai")).unwrap_err().contains("SIZE"));
    }

    /// Theatre 360's folder, when it's on this machine.
    fn theatre_360_dir() -> Option<PathBuf> {
        let mut candidates = script_dirs();
        if let Some(parent) = crate::paths::data_dir().parent() {
            candidates.push(parent.join("Theater - 360"));
        }
        candidates
            .into_iter()
            .find(|d| ["theatre_360_master", "theatre_360_layer"].iter().all(|n| d.join(format!("{}.rhai", n)).is_file() && d.join(format!("{}.lua", n)).is_file()))
    }

    fn compile_file(path: &Path) -> Compiled {
        compile(&std::fs::read_to_string(path).unwrap(), path).unwrap_or_else(|e| panic!("{}: {}", path.display(), e))
    }

    /// Just enough of obslua for the 360 Lua filters' create, update, tick
    /// and render: settings are tables, and render records what it sets.
    const LUA_OBS: &str = r#"
        obslua = {
            OBS_SOURCE_TYPE_FILTER = 1, OBS_SOURCE_VIDEO = 1, GS_RGBA = 3, OBS_NO_DIRECT_RENDERING = 0,
            obs_data_get_double = function(s, k) return s[k] or 0.0 end,
            obs_data_get_int = function(s, k) return s[k] or 0 end,
            obs_data_get_bool = function(s, k) return s[k] == true end,
            obs_data_get_string = function(s, k) return s[k] or "" end,
            obs_data_set_double = function(s, k, v) s[k] = v end,
            obs_data_set_bool = function(s, k, v) s[k] = v end,
            obs_data_set_int = function(s, k, v) s[k] = v end,
            obs_source_enabled = function(src) return src.enabled end,
            obs_source_get_settings = function(src) return nil end,
            obs_filter_get_parent = function(src) return nil end,
            obs_get_source_by_name = function(name) return nil end,
            obs_source_release = function() end,
            vec2 = function() return {} end,
            vec2_set = function(v, x, y) v.x = x; v.y = y end,
            obs_video_info = function() return {} end,
            obs_get_video_info = function(vi) vi.base_width = 1920; vi.base_height = 1080; return true end,
            gs_effect_create = function() return {} end,
            gs_effect_create_from_file = function() return {} end,
            gs_effect_destroy = function() end,
            gs_effect_get_param_by_name = function(effect, name) return name end,
            gs_effect_set_vec2 = function(p, v) uniforms[p] = { v.x, v.y } end,
            gs_effect_set_float = function(p, f) uniforms[p] = f end,
            obs_source_process_filter_begin = function() return true end,
            obs_source_process_filter_end = function() end,
            obs_source_skip_video_filter = function() end,
            obs_register_source = function() end,
        }
        uniforms = {}
    "#;

    type Settings = Vec<(&'static str, SettingValue)>;

    fn f(v: f64) -> SettingValue {
        SettingValue::Float(v)
    }

    /// A Lua filter run with OBS mocked (LUA_OBS).
    struct LuaFilter {
        lua: mlua::Lua,
        data: mlua::Table,
        settings: mlua::Table,
        source: mlua::Table,
    }

    impl LuaFilter {
        fn new(path: &Path, settings: &Settings) -> Self {
            let lua = mlua::Lua::new();
            lua.load(LUA_OBS).exec().unwrap();
            lua.load(std::fs::read_to_string(path).unwrap()).set_name(path.display().to_string()).exec().unwrap();
            let table = lua.create_table().unwrap();
            let source = lua.create_table().unwrap();
            source.set("enabled", true).unwrap();
            let mut filter = LuaFilter { data: lua.create_table().unwrap(), lua, settings: table, source };
            filter.set(settings);
            filter.data = filter.info("create").call((filter.settings.clone(), filter.source.clone())).unwrap();
            filter
        }

        fn info(&self, name: &str) -> mlua::Function {
            self.lua.globals().get::<mlua::Table>("filter_info").unwrap().get(name).unwrap()
        }

        fn set(&self, settings: &Settings) {
            for (key, value) in settings {
                match value {
                    SettingValue::Float(v) => self.settings.set(*key, *v),
                    SettingValue::Int(v) => self.settings.set(*key, *v),
                    SettingValue::Bool(v) => self.settings.set(*key, *v),
                    SettingValue::Text(v) => self.settings.set(*key, v.as_str()),
                    SettingValue::Color(v) => self.settings.set(*key, *v),
                }
                .unwrap();
            }
        }

        fn update(&self, changes: &Settings) {
            self.set(changes);
            self.info("update").call::<()>((self.data.clone(), self.settings.clone())).unwrap();
        }

        /// A frame: its tick, then what its render sets in the shader.
        fn frame(&self, dt: f64, enabled: bool) -> HashMap<String, Vec<f64>> {
            self.source.set("enabled", enabled).unwrap();
            self.info("video_tick").call::<()>((self.data.clone(), dt)).unwrap();
            self.lua.globals().set("uniforms", self.lua.create_table().unwrap()).unwrap();
            self.info("video_render").call::<()>((self.data.clone(), mlua::Value::Nil)).unwrap();
            let uniforms: mlua::Table = self.lua.globals().get("uniforms").unwrap();
            uniforms
                .pairs::<String, mlua::Value>()
                .map(|pair| {
                    let (name, value) = pair.unwrap();
                    let numbers = match value {
                        mlua::Value::Table(t) => t.sequence_values::<f64>().map(|v| v.unwrap()).collect(),
                        mlua::Value::Number(n) => vec![n],
                        mlua::Value::Integer(i) => vec![i as f64],
                        other => panic!("{}: {:?}", name, other),
                    };
                    (name, numbers)
                })
                .collect()
        }

        fn number(&self, key: &str) -> f64 {
            self.settings.get(key).unwrap()
        }
    }

    /// A Rhai filter run as Theater runs it, without OBS.
    struct RhaiFilter {
        compiled: Compiled,
        this: Dynamic,
        settings: Map,
    }

    impl RhaiFilter {
        fn new(compiled: Compiled, settings: &Settings, view: View) -> Self {
            let mut this = new_this();
            view.set_into(&mut this.write_lock::<Map>().unwrap());
            let mut filter = RhaiFilter { compiled, this, settings: Map::new() };
            filter.update(settings);
            filter
        }

        fn update(&mut self, changes: &Settings) -> Effects {
            for (key, value) in changes {
                self.settings.insert((*key).into(), value.to_dynamic());
            }
            self.this.write_lock::<Map>().unwrap().insert("settings".into(), Dynamic::from_map(self.settings.clone()));
            let (result, effects) = call_collecting(&self.compiled, &mut self.this, "update", ());
            // What it returns is written to its settings
            if let Some(written) = result.unwrap().try_cast::<Map>() {
                self.settings.extend(written);
            }
            effects
        }

        fn frame(&mut self, dt: f64, enabled: bool) -> (HashMap<String, Vec<f64>>, Effects) {
            self.this.write_lock::<Map>().unwrap().insert("enabled".into(), Dynamic::from_bool(enabled));
            let (result, effects) = call_collecting(&self.compiled, &mut self.this, "tick", (dt,));
            let number = |v: &Dynamic| v.as_float().ok().or_else(|| v.as_int().ok().map(|i| i as f64)).unwrap();
            let uniforms = result
                .unwrap()
                .try_cast::<Map>()
                .unwrap()
                .into_iter()
                .map(|(name, value)| {
                    let numbers = match value.clone().try_cast::<Array>() {
                        Some(array) => array.iter().map(number).collect(),
                        None => vec![number(&value)],
                    };
                    (name.to_string(), numbers)
                })
                .collect();
            (uniforms, effects)
        }

        fn number(&self, key: &str) -> f64 {
            let v = &self.settings[key];
            v.as_float().ok().or_else(|| v.as_int().ok().map(|i| i as f64)).unwrap()
        }
    }

    fn assert_same_shader_values(step: &str, lua: &HashMap<String, Vec<f64>>, rhai: &HashMap<String, Vec<f64>>) {
        assert!(!lua.is_empty(), "{}: the Lua render set nothing", step);
        for (name, expected) in lua {
            let got = rhai.get(name).unwrap_or_else(|| panic!("{}: the Rhai one doesn't set {}", step, name));
            assert_eq!(got.len(), expected.len(), "{}: {}", step, name);
            for (g, e) in got.iter().zip(expected) {
                assert!((g - e).abs() < 1e-9, "{}: {} is {:?} in Rhai, {:?} in Lua", step, name, got, expected);
            }
        }
    }

    const CANVAS: View = View { width: 0, height: 0, source_name: String::new(), source_width: 0, source_height: 0, canvas: (1920, 1080) };

    #[test]
    fn the_360_master_port_moves_its_camera_as_the_lua_one_did() {
        let Some(dir) = theatre_360_dir() else { return };
        let compiled = compile_file(&dir.join("theatre_360_master.rhai"));
        assert_eq!((compiled.id.as_str(), compiled.size), ("theatre_360_master_filter", Size::Canvas));
        let effect = std::fs::read_to_string(dir.join("theatre_360_master.effect")).unwrap();
        assert_eq!(compiled.shader, effect);

        // The user's Master, as saved in their scene collection
        let start: Settings = vec![
            ("pan", f(35.0)), ("tilt", f(-10.0)), ("fov", f(60.0)), ("smoothing", f(0.3)),
            ("enable_rotation", SettingValue::Bool(false)), ("rot_pan", f(-108.0)), ("rot_speed", f(4.0)),
        ];
        let lua = LuaFilter::new(&dir.join("theatre_360_master.lua"), &start);
        let mut rhai = RhaiFilter::new(compiled, &start, CANVAS);
        let dt = 1.0 / 30.0;
        let steps: Vec<(&str, Settings, usize, bool)> = vec![
            ("still", vec![], 20, true),
            ("rotation on", vec![("enable_rotation", SettingValue::Bool(true)), ("rot_speed", f(40.0))], 30, true),
            ("rotating, filter off", vec![], 10, false),
            ("rotating again", vec![], 10, true),
            ("rotation off", vec![("enable_rotation", SettingValue::Bool(false))], 20, true),
            ("across 180°", vec![("pan", f(170.0))], 15, true),
            ("backwards rotation", vec![("enable_rotation", SettingValue::Bool(true)), ("rot_speed", f(-90.0)), ("rot_pan", f(25.0))], 45, true),
            ("rotation off with a new pan", vec![("enable_rotation", SettingValue::Bool(false)), ("pan", f(-120.0))], 25, true),
            ("tilt and zoom", vec![("tilt", f(40.0)), ("fov", f(120.0)), ("smoothing", f(0.0))], 3, true),
        ];
        for (step, changes, frames, enabled) in steps {
            if !changes.is_empty() {
                lua.update(&changes);
                rhai.update(&changes);
                // Rotation off: both keep the view, as their pan setting too
                assert!((lua.number("pan") - rhai.number("pan")).abs() < 1e-9, "{}: pan {} vs {}", step, lua.number("pan"), rhai.number("pan"));
            }
            for n in 0..frames {
                let (rhai_values, effects) = rhai.frame(dt, enabled);
                let lua_values = lua.frame(dt, enabled);
                assert_same_shader_values(&format!("{} #{}", step, n), &lua_values, &rhai_values);
                // Its live camera, for Theater's Studio
                let live: HashMap<&str, f64> = effects
                    .published
                    .iter()
                    .filter_map(|(k, v)| Some((k.as_str(), v.as_float().ok()?)))
                    .collect();
                let lua_pan: f64 = lua.data.get("effective_pan").unwrap();
                assert!((live["_live_pan"] - lua_pan).abs() < 1e-9, "{}: live pan", step);
                assert!(effects.published.iter().any(|(k, v)| k == "_has_live" && v.as_bool() == Ok(true)));
            }
        }
    }

    #[test]
    fn the_360_layer_port_draws_as_the_lua_one_did() {
        let Some(dir) = theatre_360_dir() else { return };
        let compiled = compile_file(&dir.join("theatre_360_layer.rhai"));
        assert_eq!((compiled.id.as_str(), compiled.size), ("theatre_360_layer_filter", Size::Canvas));
        // The Lua version's shader, word for word (spaces at line ends aside)
        let lua_source = std::fs::read_to_string(dir.join("theatre_360_layer.lua")).unwrap();
        let start = lua_source.find("local SHADER_CODE = [[").unwrap() + "local SHADER_CODE = [[".len();
        let end = start + lua_source[start..].find("]]").unwrap();
        let lines = |s: &str| s.trim().lines().map(|l| l.trim_end().to_string()).collect::<Vec<_>>();
        assert_eq!(lines(&compiled.shader), lines(&lua_source[start..end]));

        // Every setting the Lua one has, of the same kind: saved Layers load as they are
        let props = declared_properties(&compiled).unwrap();
        let kinds = setting_kinds(&compiled, &props.iter().map(|(p, _)| p.clone()).collect::<Vec<_>>());
        for (key, kind) in [
            ("master_source", SettingKind::Text), ("scale_x", SettingKind::Float), ("scale_y", SettingKind::Float),
            ("x_offset", SettingKind::Float), ("y_offset", SettingKind::Float), ("distance", SettingKind::Float),
            ("auto_setup", SettingKind::Bool), ("native_w", SettingKind::Int), ("native_h", SettingKind::Int),
            ("m_pan", SettingKind::Float), ("m_tilt", SettingKind::Float), ("m_fov", SettingKind::Float),
            ("enable_rotation", SettingKind::Bool), ("rot_pan", SettingKind::Float), ("rot_speed", SettingKind::Float),
        ] {
            assert!(kinds.contains(&(key.to_string(), kind)), "{} as {:?}", key, kind);
        }
        // Its Remote Master Control drives the Master's settings, as Theater's Studio expects
        let remotes: Vec<(String, String)> = props
            .iter()
            .filter_map(|(p, r)| Some((p.key()?.to_string(), r.as_ref()?.key.clone())))
            .collect();
        for (local, master) in [("m_pan", "pan"), ("m_tilt", "tilt"), ("m_fov", "fov"), ("enable_rotation", "enable_rotation"), ("rot_pan", "rot_pan"), ("rot_speed", "rot_speed")] {
            assert!(remotes.contains(&(local.to_string(), master.to_string())), "{} → {}", local, master);
        }
        assert!(matches!(&props[0].0, Prop::SourceList { key, .. } if key == "master_source"), "the Master picker comes first");

        // Standalone (no Master): its own camera, rotating, the user's TV Layer's placement
        let start: Settings = vec![
            ("master_source", SettingValue::Text(String::new())), ("x_offset", f(-136.2)), ("y_offset", f(-4.4)),
            ("scale_x", f(3.0)), ("scale_y", f(1.0)), ("distance", f(2.2)), ("auto_setup", SettingValue::Bool(false)),
            ("native_w", SettingValue::Int(1280)), ("native_h", SettingValue::Int(720)),
            ("m_pan", f(-140.5)), ("m_tilt", f(0.0)), ("m_fov", f(60.0)),
            ("enable_rotation", SettingValue::Bool(true)), ("rot_pan", f(10.0)), ("rot_speed", f(-30.0)),
        ];
        let lua = LuaFilter::new(&dir.join("theatre_360_layer.lua"), &start);
        let mut rhai = RhaiFilter::new(compiled.clone(), &start, CANVAS);
        for n in 0..40 {
            assert_same_shader_values(&format!("standalone #{}", n), &lua.frame(1.0 / 30.0, true), &rhai.frame(1.0 / 30.0, true).0);
        }
        let changes: Settings = vec![("enable_rotation", SettingValue::Bool(false)), ("m_fov", f(200.0)), ("x_offset", f(179.0))];
        lua.update(&changes);
        rhai.update(&changes);
        for n in 0..5 {
            assert_same_shader_values(&format!("still #{}", n), &lua.frame(1.0 / 30.0, true), &rhai.frame(1.0 / 30.0, true).0);
        }

        // Seen through a Master: its live camera
        let master = [("pan", 12.5), ("tilt", -3.0), ("fov", 70.0), ("width", 1920.0), ("height", 1080.0)];
        let linked: Settings = vec![("master_source", SettingValue::Text("Media360Test".into()))];
        lua.update(&linked);
        let shared = lua.lua.create_table().unwrap();
        for (k, v) in master {
            shared.set(k, v).unwrap();
        }
        let state = lua.lua.create_table().unwrap();
        state.set("Media360Test", shared).unwrap();
        lua.lua.globals().get::<mlua::Table>("package").unwrap().get::<mlua::Table>("loaded").unwrap().set("theatre_360_shared_state", state).unwrap();
        rhai.update(&linked);
        engine().eval::<()>(r#"share("360/Media360Test", #{ pan: 12.5, tilt: -3.0, fov: 70.0, width: 1920, height: 1080 })"#).unwrap();
        assert_same_shader_values("through a Master", &lua.frame(1.0 / 30.0, true), &rhai.frame(1.0 / 30.0, true).0);

        // Auto-setup: its source's size, stretched over the canvas
        let mut placed = RhaiFilter::new(compiled, &start, View { source_name: "TV".into(), source_width: 1280, source_height: 720, ..CANVAS });
        let effects = placed.update(&vec![("auto_setup", SettingValue::Bool(true))]);
        assert!(effects.fill_canvas);
        assert_eq!((placed.number("native_w"), placed.number("native_h")), (1280.0, 720.0));
        assert_eq!(placed.settings["auto_setup"].as_bool(), Ok(false));
    }

    /// The Lua copies of the Rhai filters don't register an id that's taken:
    /// OBS frees a refused Lua filter definition but keeps it listed, and
    /// crashed unloading scripts on exit (obs_lua_script_unload).
    #[test]
    fn lua_copies_step_aside_for_the_rhai_filters() {
        let mut files = vec![crate::studio_app::get_filters_dir().join("transparency_fix.lua")];
        if let Some(dir) = theatre_360_dir() {
            files.push(dir.join("theatre_360_master.lua"));
            files.push(dir.join("theatre_360_layer.lua"));
        }
        for path in files.into_iter().filter(|p| p.is_file()) {
            for taken in [true, false] {
                let lua = mlua::Lua::new();
                lua.load(LUA_OBS).exec().unwrap();
                lua.load(format!(
                    r#"
                    registered = 0
                    obslua.obs_source_get_display_name = function(id) if {} then return "Rhai" end return nil end
                    obslua.obs_register_source = function(info) registered = registered + 1 end
                    obslua.obs_frontend_add_event_callback = function(cb) end
                    "#,
                    taken
                ))
                .exec()
                .unwrap();
                lua.load(std::fs::read_to_string(&path).unwrap()).set_name(path.display().to_string()).exec().unwrap();
                lua.globals().get::<mlua::Function>("script_load").unwrap().call::<()>(mlua::Value::Nil).unwrap();
                let registered: i64 = lua.globals().get("registered").unwrap();
                assert_eq!(registered, if taken { 0 } else { 1 }, "{} with its id {}", path.display(), if taken { "taken" } else { "free" });
            }
        }
    }

    #[test]
    fn a_360_master_reaches_its_layers() {
        let Some(dir) = theatre_360_dir() else { return };
        let settings: Settings = vec![
            ("pan", f(-60.0)), ("tilt", f(15.0)), ("fov", f(80.0)), ("smoothing", f(0.0)),
            ("enable_rotation", SettingValue::Bool(false)), ("rot_pan", f(0.0)), ("rot_speed", f(10.0)),
        ];
        let mut master = RhaiFilter::new(compile_file(&dir.join("theatre_360_master.rhai")), &settings, View { source_name: "MasterReachTest".into(), ..CANVAS });
        master.frame(1.0 / 30.0, true);
        let layer_settings: Settings = vec![
            ("master_source", SettingValue::Text("MasterReachTest".into())), ("x_offset", f(0.0)), ("y_offset", f(0.0)),
            ("scale_x", f(1.0)), ("scale_y", f(1.0)), ("distance", f(1.0)), ("auto_setup", SettingValue::Bool(false)),
            ("native_w", SettingValue::Int(500)), ("native_h", SettingValue::Int(500)),
            ("m_pan", f(0.0)), ("m_tilt", f(0.0)), ("m_fov", f(90.0)),
            ("enable_rotation", SettingValue::Bool(false)), ("rot_pan", f(0.0)), ("rot_speed", f(10.0)),
        ];
        let mut layer = RhaiFilter::new(compile_file(&dir.join("theatre_360_layer.rhai")), &layer_settings, CANVAS);
        let (values, _) = layer.frame(1.0 / 30.0, true);
        let pan_tilt = &values["master_pan_tilt"];
        assert!((pan_tilt[0] - (-60.0f64).to_radians()).abs() < 1e-9 && (pan_tilt[1] - 15.0f64.to_radians()).abs() < 1e-9);
        assert!((values["master_fov"][0] - 80.0f64.to_radians()).abs() < 1e-9);
    }

    #[test]
    fn the_chat_keeps_up_with_a_busy_stream() {
        let Some(mut chat) = Chat::new() else { return };
        for n in 0..60 {
            chat.event(&format!(
                r#"{{"type":"chat","source":"Twitch","username":"Viewer{}","badges":"[SUB] [VIP]","message":"message number {} with a few more words to wrap onto a second line [twitch_emote:25] ok"}}"#,
                n, n
            ));
        }
        // The first frame lays all of them out
        let started = std::time::Instant::now();
        let first = chat.frame(1.0);
        let layout = started.elapsed();
        assert!(!first.is_empty());
        // (Their emote can't load in the tests: laid out once more with its name)
        chat.frame(0.3);
        // Then each frame only places them
        let started = std::time::Instant::now();
        for _ in 0..30 {
            chat.frame(1.0 / 30.0);
        }
        let per_frame = started.elapsed() / 30;
        let kept = chat.this.read_lock::<Map>().unwrap()["messages"].clone().try_cast::<Array>().unwrap().len();
        println!("50 messages: laid out in {:?}, then {:?} a frame", layout, per_frame);
        assert_eq!(kept, 50, "the oldest ones go past Messages kept");
        assert!(per_frame < std::time::Duration::from_millis(8), "{:?} a frame", per_frame);
    }

    /// Every command in `cmds`, groups opened.
    fn flatten(cmds: &[crate::script_draw::Cmd], out: &mut Vec<crate::script_draw::Cmd>) {
        for c in cmds {
            match c {
                crate::script_draw::Cmd::Group { children, .. } => flatten(children, out),
                other => out.push(other.clone()),
            }
        }
    }

    /// The chat as OBS runs it: settings (what update writes into them kept),
    /// relay messages, frames.
    struct Chat {
        compiled: Compiled,
        this: Dynamic,
        settings: Map,
        time: f64,
    }

    impl Chat {
        fn new() -> Option<Self> {
            Self::load("theater_chat.rhai")
        }

        /// Another source script of the filters folder, in the same harness.
        fn load(file: &str) -> Option<Self> {
            let path = crate::studio_app::get_filters_dir().join(file);
            let source = std::fs::read_to_string(&path).ok()?;
            let compiled = compile(&source, &path).unwrap_or_else(|e| panic!("{}: {}", file, e));
            let settings = compiled.defaults.clone();
            let mut chat = Chat { compiled, this: new_this(), settings, time: 0.0 };
            chat.update(&[]);
            Some(chat)
        }

        fn update(&mut self, changes: &[(&str, Dynamic)]) -> Map {
            for (k, v) in changes {
                self.settings.insert((*k).into(), v.clone());
            }
            self.this.write_lock::<Map>().unwrap().insert("settings".into(), Dynamic::from_map(self.settings.clone()));
            let (result, _) = call_collecting(&self.compiled, &mut self.this, "update", ());
            let written = result.unwrap().try_cast::<Map>().unwrap_or_default();
            self.settings.extend(written.clone());
            written
        }

        fn call(&mut self, name: &str, arg: Dynamic) {
            let (result, _) = call_collecting(&self.compiled, &mut self.this, name, (arg,));
            result.unwrap_or_else(|e| panic!("{}: {}", name, e));
        }

        fn event(&mut self, json: &str) {
            self.call("on_event", Dynamic::from_map(event_map(json)));
        }

        /// `seconds` later, what it draws.
        fn frame(&mut self, seconds: f64) -> Vec<crate::script_draw::Cmd> {
            let mut out = Vec::new();
            flatten(&self.frame_grouped(seconds), &mut out);
            out
        }

        /// `seconds` later, what it draws, its groups kept.
        fn frame_grouped(&mut self, seconds: f64) -> Vec<crate::script_draw::Cmd> {
            self.time += seconds;
            self.this.write_lock::<Map>().unwrap().insert("time".into(), Dynamic::from_float(self.time));
            let (result, _) = call_collecting(&self.compiled, &mut self.this, "tick", (seconds,));
            let list = result.unwrap_or_else(|e| panic!("tick: {}", e)).try_cast::<Array>().unwrap_or_default();
            crate::script_draw::parse(&list)
        }
    }

    /// Every group's clip and blending in `cmds`, nested ones too.
    fn group_looks(cmds: &[crate::script_draw::Cmd], out: &mut Vec<(Option<[f32; 4]>, bool)>) {
        for c in cmds {
            if let crate::script_draw::Cmd::Group { clip, add, children, .. } = c {
                out.push((*clip, *add));
                group_looks(children, out);
            }
        }
    }

    #[test]
    fn legacy_alerts_card_comes_in_with_the_light_bar_and_lightning() {
        use crate::script_draw::Cmd;
        let Some(mut alerts) = Chat::load("legacy_alert.rhai") else { return };
        alerts.event(r#"{"type":"alert","source":"TwitchParser","event":"Twitch cheer","trigger":"Twitch cheer 500 bits","data":{"username":"Morty","bits":"500","message":"take my bits"}}"#);

        // It comes: a bolt across the card, the bar's added light, the card shown from the bar
        let early = alerts.frame_grouped(0.05);
        let mut drawn = Vec::new();
        flatten(&early, &mut drawn);
        let bolt = drawn.iter().find_map(|c| match c {
            Cmd::Line { points, .. } if points.len() >= 5 => Some(points.clone()),
            _ => None,
        });
        let bolt = bolt.expect("the lightning bolt");
        let xs: Vec<f32> = bolt.iter().map(|p| p.0).collect();
        let ys: Vec<f32> = bolt.iter().map(|p| p.1).collect();
        let span = |v: &[f32]| v.iter().cloned().fold(f32::MIN, f32::max) - v.iter().cloned().fold(f32::MAX, f32::min);
        assert!(span(&xs) > 200.0 && span(&xs) > span(&ys) * 3.0, "a horizontal bolt across the card");
        let mut looks = Vec::new();
        group_looks(&early, &mut looks);
        assert!(looks.iter().any(|(_, add)| *add), "added light");
        let clip_x = |looks: &[(Option<[f32; 4]>, bool)]| looks.iter().find_map(|(clip, _)| clip.map(|c| c[0]));
        let start = clip_x(&looks).expect("the card clipped behind the bar");

        let mid = alerts.frame_grouped(0.4);
        let mut looks = Vec::new();
        group_looks(&mid, &mut looks);
        assert!(clip_x(&looks).expect("still sweeping") < start - 50.0, "the bar moved right to left");

        // Then the whole card, its words there
        let done = alerts.frame_grouped(1.0);
        let mut looks = Vec::new();
        group_looks(&done, &mut looks);
        assert!(looks.iter().all(|(clip, add)| clip.is_none() && !add), "{looks:?}");
        let words = texts(&alerts.frame(0.1)).join("|");
        assert!(words.contains("Morty") && words.contains("cheered 500 bits!"), "{words}");
    }

    #[test]
    fn legacy_chat_copies_the_look_of_an_old_overlay_link() {
        let Some(mut chat) = Chat::load("legacy_chat.rhai") else { return };
        // A link as chat_overlay.html makes it (its settings after the #)
        let link = "file:///C:/Users/me/Chat/chat_overlay.html#global_opacity=0.9&bg_r=10&bg_g=5&bg_b=20&bgopacity=0.9\
            &msg_r=0&msg_g=255&msg_b=0&username_color_mode=custom&user_r=0&user_g=255&user_b=255&bubble_style=left-border\
            &layout_mode=stacked&padding=6&border_radius=0&font=%27Roboto+Mono%27%2C+monospace&fontsize=15&font_weight_user=700\
            &font_weight_msg=400&shadow=false&emotes=true&user_badges=false&user_images=true&clean=false&animation=fade\
            &body_color_mode=custom&header_bg_r=10&header_bg_g=5&header_bg_b=20&header_bgopacity=0.95&header_color_mode=custom\
            &preset=cyberpunk&bubble_width=0&user_bubble_width=0&bubble_overlap=4&border_r=255&border_g=0&border_b=128\
            &border_opacity=0.5&border_top=0&border_right=0&border_bottom_w=2&border_left_w=3";
        let written = chat.update(&[("overlay_url", Dynamic::from(link.to_string()))]);
        let s = &chat.settings;
        let int = |k: &str| s[k].as_int().unwrap_or_else(|_| panic!("{k}: {:?}", s[k]));
        let text = |k: &str| s[k].to_string();
        assert_eq!(int("bg") as u32, 0xE60A0514, "the bubble: rgb 10,5,20 at 0.9");
        assert_eq!(int("msg_color") as u32, 0xFF00FF00);
        assert_eq!(int("border_color") as u32, 0x80FF0080);
        assert_eq!((text("font"), int("fontsize"), int("padding"), int("border_radius")), ("Roboto Mono".into(), 15, 6, 0));
        assert_eq!((text("bubble_style"), text("layout_mode"), text("username_color_mode")), ("left-border".into(), "stacked".into(), "custom".into()));
        assert_eq!((int("border_bottom"), int("border_left"), int("border_top")), (2, 3, 0));
        assert!(!s["shadow"].as_bool().unwrap() && !s["user_badges"].as_bool().unwrap() && s["user_images"].as_bool().unwrap());
        assert_eq!(text("animation"), "bloom", "its own entrance stays");
        // Its theme is marked as applied: its preset doesn't paint over the link's colours
        assert_eq!((text("preset"), text("applied_preset")), ("cyberpunk".into(), "cyberpunk".into()));
        assert!(written.contains_key("bg") && written.get("applied_url").is_some_and(|u| u.to_string() == link), "saved");

        // Changed afterwards, it stays changed (the same link isn't copied again)
        chat.update(&[("padding", Dynamic::from(12_i64))]);
        assert_eq!(chat.settings["padding"].as_int().unwrap(), 12);
    }

    #[test]
    fn legacy_chats_light_bar_reveals_a_message_and_lightning_strikes() {
        use crate::script_draw::Cmd;
        let Some(mut chat) = Chat::load("legacy_chat.rhai") else { return };
        chat.event(r#"{"type":"chat","source":"Twitch","username":"Morty","message":"light bar test"}"#);

        // Just in: the bolt, the glowing bar (added light), the message shown
        // only from the bar (a clip near its right end)
        let early = chat.frame_grouped(0.05);
        let mut drawn = Vec::new();
        flatten(&early, &mut drawn);
        // Across the message, sideways: far wider than it is tall
        let bolt = drawn.iter().find_map(|c| match c {
            Cmd::Line { points, .. } if points.len() >= 5 => Some(points.clone()),
            _ => None,
        });
        let bolt = bolt.expect("the lightning bolt");
        let span = |pick: fn(&(f32, f32)) -> f32| {
            let v: Vec<f32> = bolt.iter().map(pick).collect();
            v.iter().cloned().fold(f32::MIN, f32::max) - v.iter().cloned().fold(f32::MAX, f32::min)
        };
        let (wide, tall) = (span(|p| p.0), span(|p| p.1));
        assert!(wide > 100.0 && wide > tall * 3.0, "a horizontal bolt: {wide} wide, {tall} tall");
        let mut looks = Vec::new();
        group_looks(&early, &mut looks);
        assert!(looks.iter().any(|(_, add)| *add), "the bar and bolt add their light");
        let clip_x = |looks: &[(Option<[f32; 4]>, bool)]| looks.iter().find_map(|(clip, _)| clip.map(|c| c[0]));
        let start = clip_x(&looks).expect("the message clipped behind the bar");

        // Most of the way through the 0.6 s sweep the bar is further left:
        // more of it shown
        let mid = chat.frame_grouped(0.4);
        let mut looks = Vec::new();
        group_looks(&mid, &mut looks);
        let halfway = clip_x(&looks).expect("still sweeping");
        assert!(halfway < start - 20.0, "the bar moved right to left: {start} → {halfway}");
        let mut drawn = Vec::new();
        flatten(&mid, &mut drawn);
        assert!(!drawn.iter().any(|c| matches!(c, Cmd::Line { .. })), "the lightning was gone after 0.4 s");

        // Done: the whole message, no clip, no bar
        let done = chat.frame_grouped(1.0);
        let mut looks = Vec::new();
        group_looks(&done, &mut looks);
        assert!(looks.iter().all(|(clip, add)| clip.is_none() && !add), "{looks:?}");
        assert!(texts(&chat.frame(0.1)).join("|").contains("light bar test"));
    }

    #[test]
    fn the_thanks_roll_up_grouped_and_a_new_supporter_joins() {
        let Some(mut thanks) = Chat::load("theater_thanks.rhai") else { return };
        assert_eq!(thanks.compiled.kind, ScriptKind::Source);
        // Nobody yet (no Theater API plugin in the tests): it says so as it rolls in
        let words = texts(&thanks.frame(3.0)).join("|");
        assert!(words.contains("Legendary Supporters") && words.contains("Seeking legends…"), "{words}");

        // The test names, by platform: Twitch first, then Patreon, then Ko-fi, names alphabetical
        thanks.call("on_button", Dynamic::from("test".to_string()));
        thanks.update(&[("speed", Dynamic::from_float(400.0)), ("height", Dynamic::from_int(2160))]);
        let mut seen: Vec<String> = Vec::new();
        for _ in 0..12 {
            for t in texts(&thanks.frame(0.5)) {
                if !seen.contains(&t) {
                    seen.push(t);
                }
            }
        }
        let at = |name: &str| seen.iter().position(|t| t == name).unwrap_or_else(|| panic!("{name} not in {seen:?}"));
        assert!(at("TWITCH SUBSCRIBERS") < at("KappaKing") && at("KappaKing") < at("StreamFan42"), "{seen:?}");
        assert!(at("StreamFan42") < at("PATREON SUPPORTERS") && at("PATREON SUPPORTERS") < at("KO-FI SUPPORTERS"));
        assert!(seen.contains(&"TWITCH".to_string()), "a platform badge");

        // By tier: Twitch Tier 3 before Tier 1
        thanks.update(&[("grouping", Dynamic::from("tier".to_string()))]);
        thanks.call("on_button", Dynamic::from("restart".to_string()));
        let mut seen: Vec<String> = Vec::new();
        for _ in 0..12 {
            for t in texts(&thanks.frame(0.5)) {
                if !seen.contains(&t) {
                    seen.push(t);
                }
            }
        }
        let at = |name: &str| seen.iter().position(|t| t == name).unwrap_or_else(|| panic!("{name} not in {seen:?}"));
        assert!(at("TWITCH TIER 3") < at("TWITCH TIER 1"), "{seen:?}");
        assert!(at("GOLD") < at("SILVER"), "Patreon tiers by what they give: {seen:?}");

        // Someone pledging during the stream joins; a test alert doesn't
        thanks.event(r#"{"type":"alert","alert":{"kind":"pledge","platform":"patreon","user":"NewPatron","tier":"Gold","amount":10}}"#);
        thanks.event(r#"{"type":"alert","alert":{"kind":"donation","platform":"kofi","user":"Tester","amount":5,"test":true}}"#);
        thanks.call("on_button", Dynamic::from("restart".to_string()));
        let mut seen: Vec<String> = Vec::new();
        for _ in 0..14 {
            seen.extend(texts(&thanks.frame(0.5)));
        }
        assert!(seen.contains(&"NewPatron".to_string()) && !seen.contains(&"Tester".to_string()));

        // Along the bottom: one line, clipped to its bar
        thanks.update(&[("direction", Dynamic::from("horizontal".to_string())), ("speed", Dynamic::from_float(300.0))]);
        thanks.call("on_button", Dynamic::from("restart".to_string()));
        let mut seen: Vec<String> = Vec::new();
        for _ in 0..30 {
            seen.extend(texts(&thanks.frame(0.5)));
        }
        assert!(seen.contains(&"KappaKing".to_string()) && seen.iter().any(|t| t == "•"), "{seen:?}");

        // Fireworks: sparks drawn with added light; the hearts rise
        let mut looks = Vec::new();
        group_looks(&thanks.frame_grouped(2.0), &mut looks);
        assert!(looks.iter().any(|(_, add)| *add));
        let heart_ys = |cmds: &[crate::script_draw::Cmd]| -> Vec<f32> {
            cmds.iter().filter_map(|c| match c {
                crate::script_draw::Cmd::Text { key, y, .. } if key.text == "♥" => Some(*y),
                _ => None,
            }).collect()
        };
        let before = heart_ys(&thanks.frame(0.1));
        let after = heart_ys(&thanks.frame(1.0));
        assert!(!before.is_empty() && before.iter().zip(&after).filter(|(b, a)| *a < *b).count() > before.len() / 2, "{before:?} → {after:?}");
    }

    #[test]
    fn legacy_thanks_gives_a_new_supporter_a_banner_with_the_light_bar_and_lightning() {
        use crate::script_draw::Cmd;
        let Some(mut thanks) = Chat::load("legacy_thanks.rhai") else { return };
        thanks.event(r#"{"type":"alert","alert":{"kind":"pledge","platform":"patreon","user":"NewPatron","tier":"Gold","amount":10}}"#);
        // It comes: the lightning across it, the banner shown from the bar
        let early = thanks.frame(0.05);
        assert!(early.iter().any(|c| matches!(c, Cmd::Line { points, .. } if points.len() >= 5)), "the lightning bolt");
        let words = texts(&thanks.frame(1.5)).join("|");
        assert!(words.contains("Thank you, NewPatron!") && words.contains("Patreon · Gold"), "{words}");
        // Gone after its seconds; the new patron rolls in the credits
        thanks.update(&[("speed", Dynamic::from_float(400.0))]);
        let words = texts(&thanks.frame(5.0)).join("|");
        assert!(!words.contains("Thank you, NewPatron!"), "{words}");
        let mut seen = Vec::new();
        for _ in 0..8 {
            seen.extend(texts(&thanks.frame(0.5)));
        }
        assert!(seen.contains(&"NewPatron".to_string()), "{seen:?}");
        // A test alert gets no banner
        thanks.event(r#"{"type":"alert","alert":{"kind":"donation","platform":"kofi","user":"Tester","amount":5,"test":true}}"#);
        assert!(!texts(&thanks.frame(1.0)).join("|").contains("Tester"));
    }

    #[test]
    fn the_chats_give_their_look_to_vtuber_friends() {
        for file in ["theater_chat.rhai", "legacy_chat.rhai"] {
            let Some(chat) = Chat::load(file) else { continue };
            let code = call(&chat.compiled, None, "vf_look_code", (Dynamic::from_map(chat.settings.clone()),)).unwrap().into_string().unwrap();
            let json = code.strip_prefix("VTF-LOOK ").unwrap_or_else(|| panic!("{file}: {code}"));
            let look: serde_json::Value = serde_json::from_str(json).unwrap_or_else(|e| panic!("{file}: {e}: {json}"));
            assert_eq!(look["vf_look"], 1, "{file}");
            assert!(look["font"].is_string() && look["bg"].is_i64() && look["border"].as_array().map(|b| b.len()) == Some(4), "{file}: {look}");
        }
    }

    fn texts(cmds: &[crate::script_draw::Cmd]) -> Vec<String> {
        cmds.iter().filter_map(|c| match c {
            crate::script_draw::Cmd::Text { key, .. } => Some(key.text.clone()),
            _ => None,
        }).collect()
    }

    #[test]
    fn the_alerts_show_theater_readers_alerts_one_at_a_time() {
        let Some(mut alerts) = Chat::load("theater_alerts.rhai") else { return };
        assert_eq!(alerts.compiled.kind, ScriptKind::Source);
        let size: Array = alerts.this.read_lock::<Map>().unwrap()["size"].clone().try_cast().unwrap();
        assert_eq!((size[0].as_int().unwrap(), size[1].as_int().unwrap()), (900, 320));

        // A cheer and a raid, as Theater Reader sends them; a chat line isn't an alert
        alerts.event(r#"{"type":"alert","source":"TwitchParser","event":"Twitch cheer","trigger":"Twitch cheer 500 bits","data":{"username":"Morty","bits":"500","message":"take my bits"}}"#);
        alerts.event(r#"{"type":"alert","source":"TwitchParser","event":"Twitch raid","trigger":"Twitch raid","data":{"username":"BigStreamer","viewers":"1280"}}"#);
        alerts.event(r#"{"type":"chat","source":"Twitch","username":"Chatter","message":"hello"}"#);
        // The same cheer again at once, and a redeem's IRC copy: neither shows
        alerts.event(r#"{"type":"alert","source":"TwitchParser","event":"Twitch cheer","trigger":"Twitch cheer 500 bits","data":{"username":"Morty","bits":"500","message":"take my bits"}}"#);
        alerts.event(r#"{"type":"alert","source":"TwitchParser","event":"Twitch redeem (irc)","trigger":"Twitch redeem 1234abcd...","data":{"username":"Morty","reward_id":"1234abcd"}}"#);

        // (It starts see-through, popping in)
        alerts.frame(0.1);
        let words = texts(&alerts.frame(0.5)).join("|");
        assert!(words.contains("Morty") && words.contains("cheered 500 bits!") && words.contains("take my bits"), "{words}");
        assert!(!words.contains("BigStreamer") && !words.contains("Chatter"), "one at a time, alerts only: {words}");
        let queued = alerts.this.read_lock::<Map>().unwrap()["queue"].clone().try_cast::<Array>().unwrap().len();
        assert_eq!(queued, 1, "the raid waits; the repeat and the IRC copy were dropped");

        // The cheer's time is up (7 s): the raid, after a breath
        alerts.frame(7.0);
        alerts.frame(0.5);
        let words = texts(&alerts.frame(0.5)).join("|");
        assert!(words.contains("BigStreamer") && words.contains("is raiding with 1,280 viewers!"), "{words}");

        // The test button queues a sample of each kind
        alerts.frame(8.0);
        alerts.call("on_button", Dynamic::from("test".to_string()));
        alerts.frame(0.5);
        let words = texts(&alerts.frame(0.5)).join("|");
        assert!(words.contains("StreamFan42") && words.contains("resubscribed!") && words.contains("Tier 1"), "{words}");

        // What each kind says
        let says = |json: &str| -> String {
            let a = call(&alerts.compiled, None, "alert_of", (Dynamic::from_map(event_map(json)),)).unwrap();
            if a.is_unit() {
                return "()".into();
            }
            let a = a.read_lock::<Map>().unwrap().clone();
            format!("{}: {} {} [{}] {}", a["kind"], a["user"], a["action"], a["detail"], a["platform"])
        };
        assert_eq!(
            says(r#"{"type":"alert","source":"TwitchParser","event":"Twitch sub","data":{"username":"Gifter","type":"subgift","plan":"2000"}}"#),
            "gift: Gifter gifted a sub! [Tier 2] twitch"
        );
        assert_eq!(
            says(r#"{"type":"alert","source":"TwitchParser","event":"Twitch redeem (pubsub)","data":{"display_name":"Ann","reward_title":"Hydrate!","cost":1500}}"#),
            "redeem: Ann redeemed Hydrate! [1,500 points] twitch"
        );
        assert_eq!(
            says(r#"{"type":"alert","source":"YouTubeParser","event":"paid_message","data":{"author":"Tube","amount":"$5.00","message":"hi"}}"#),
            "superchat: Tube sent a Super Chat! [$5.00] youtube"
        );
        assert_eq!(
            says(r#"{"type":"alert","source":"TwitchParser","event":"Twitch hype train","data":{"level":3,"progress":450,"goal":1000}}"#),
            "hype: Hype Train Level 3! [45% to the next level] twitch"
        );
        assert_eq!(says(r#"{"type":"alert","source":"KickParser","event":"Kick follow","data":{"username":"Kicker"}}"#), "follow: Kicker followed! [] kick");
        // A sender that hands the shared alert itself (a Theater API plugin): used as it is
        assert_eq!(
            says(r#"{"type":"alert","alert":{"kind":"resub","platform":"twitch","user":"Ann","tier":"Tier 3","months":14},"data":{"username":"not used"}}"#),
            "sub: Ann resubscribed! [Tier 3 · 14 months] twitch"
        );
        assert_eq!(
            says(r#"{"type":"alert","alert":{"kind":"donation","platform":"kofi","user":"Kind","amount":5,"currency":"$"}}"#),
            "superchat: Kind donated $5! [] kofi"
        );
        // Theater API · Patreon and · Ko-fi: a pledge with its tier, an ended one (no card), a shop order
        assert_eq!(
            says(r#"{"type":"alert","alert":{"kind":"pledge","platform":"patreon","user":"Pat","amount":5,"currency":"USD","tier":"Gold","raw":{"pledge_amount":"$5.00"}}}"#),
            "superchat: Pat pledged $5! [Gold] patreon"
        );
        // Currency codes: a symbol where there's a common one, else after the amount
        assert_eq!(
            says(r#"{"type":"alert","alert":{"kind":"donation","platform":"kofi","user":"Eu","amount":5,"currency":"EUR","raw":{"amount":"5.00"}}}"#),
            "superchat: Eu donated €5.00! [] kofi"
        );
        assert_eq!(
            says(r#"{"type":"alert","alert":{"kind":"donation","platform":"kofi","user":"Ch","amount":7.5,"currency":"CHF"}}"#),
            "superchat: Ch donated 7.5 CHF! [] kofi"
        );
        assert_eq!(says(r#"{"type":"alert","alert":{"kind":"pledge_end","platform":"patreon","user":"Pat"}}"#), "()");
        assert_eq!(
            says(r#"{"type":"alert","alert":{"kind":"shop_order","platform":"kofi","user":"Buyer","title":"2x Sticker","amount":10}}"#),
            "superchat: Buyer bought 2x Sticker! [] kofi"
        );
        assert_eq!(says(r#"{"type":"chat","username":"x","message":"y"}"#), "()");
        // Theater Reader's tester: "testserver" in the event, the number in `amount`
        assert_eq!(
            says(r#"{"type":"alert","source":"Test UI","event":"Twitch testserver cheer","trigger":"Twitch testserver cheer","data":{"is_test":true,"username":"TestUser","amount":"500"}}"#),
            "cheer: TestUser cheered 500 bits! [] twitch"
        );
        assert_eq!(
            says(r#"{"type":"alert","source":"Test UI","event":"Twitch testserver redeem Hydrate","trigger":"Twitch testserver redeem Hydrate","data":{"is_test":true,"username":"TestUser","amount":"500"}}"#),
            "redeem: TestUser redeemed Hydrate [] twitch"
        );
        assert_eq!(
            says(r#"{"type":"alert","source":"Test UI","event":"Twitch testserver raid","trigger":"Twitch testserver raid","data":{"is_test":true,"username":"TestUser","amount":"42"}}"#),
            "raid: TestUser is raiding with 42 viewers! [] twitch"
        );
        // The trigger's second word is the channel, whatever its name
        assert_eq!(
            says(r#"{"type":"alert","source":"Test UI","event":"Twitch subzero cheer","trigger":"Twitch subzero cheer","data":{"username":"A","amount":"5"}}"#),
            "cheer: A cheered 5 bits! [] twitch"
        );
        assert_eq!(
            says(r#"{"type":"alert","source":"KickParser","event":"Kick redeem","trigger":"Kick redeemking redeem Dance","data":{"username":"B"}}"#),
            "redeem: B redeemed Dance [] kick"
        );
        // What Theater Reader doesn't know isn't an alert
        assert_eq!(says(r#"{"type":"alert","source":"TwitchParser","event":"Twitch other","data":{}}"#), "()");

        // Looking like the chat: its look over the card's own
        let style = alerts.this.read_lock::<Map>().unwrap()["style"].clone();
        let look = |json: &str| Dynamic::from_map(engine().parse_json(json, true).unwrap());
        let matched = |look: Dynamic| -> Map {
            call(&alerts.compiled, None, "chat_look", (style.clone(), look)).unwrap().try_cast::<Map>().unwrap()
        };
        let left_border = matched(look(r#"{"stamp":7,"bg":-16777216,"bg_mode":"custom","text":-1,"name":-65536,"name_mode":"custom",
            "font":"Roboto Mono","weight_user":600,"radius":0,"shadow":false,"bubble":"left-border","border":[1,1,1,1],"border_color":-1,"opacity":0.5}"#));
        assert_eq!(left_border["font"].to_string(), "Roboto Mono");
        assert_eq!(left_border["frame"].to_string(), "bar", "the chat's left-border bubble");
        assert_eq!(left_border["radius"].as_float(), Ok(0.0));
        assert_eq!(left_border["accent_mode"].to_string(), "custom", "the chat's one name colour");
        assert_eq!(left_border["name_weight"].as_int(), Ok(600));
        assert_eq!(left_border["opacity"].as_float(), Ok(0.5));
        assert_eq!(left_border["look_stamp"].as_int(), Ok(7));
        let bubble = matched(look(r#"{"stamp":8,"bg":-16777216,"bg_mode":"transparent","text":-1,"name":0,"name_mode":"platform",
            "font":"Inter","weight_user":700,"radius":6,"shadow":true,"bubble":"bubble","border":[0,2,0,2],"border_color":-1,"opacity":1.0}"#));
        assert_eq!(bubble["frame"].to_string(), "none");
        assert_eq!(bubble["bg"].as_int(), Ok(0), "a transparent chat: a transparent card");
        assert_eq!(bubble["border"].clone().try_cast::<Array>().map(|b| b.len()), Some(4), "the chat's own borders");
        assert_eq!(bubble["accent_mode"].to_string(), "platform");
        // No chat: the card's own look
        assert_eq!(call(&alerts.compiled, None, "chat_look", (style.clone(), Dynamic::UNIT)).unwrap().try_cast::<Map>().unwrap()["font"].to_string(), "Poppins");

        // A sound picker per kind: OBS file properties for sounds
        let props = properties(&alerts.compiled).unwrap();
        let sound_sub = props.iter().find_map(|p| match p {
            Prop::File { key, filter, .. } if key == "sound_sub" => Some(filter.clone()),
            _ => None,
        });
        assert!(sound_sub.is_some_and(|f| f.contains("*.wav") && f.contains("*.mp3")), "{:?}", props);
        assert!(!play_sound("", 1.0) && !play_sound("no such sound.wav", 1.0));
    }

    #[test]
    fn the_split_header_puts_the_badges_at_the_bubbles_right_edge() {
        let Some(mut chat) = Chat::new() else { return };
        let message = r##"{"type":"chat","source":"Twitch","username":"Mo","color":"#9146FF","badges":"[MOD]","message":"a long enough message to make this bubble much wider than its header line"}"##;
        // Where the name, the MOD badge and the bubble are (message-local)
        let placed = |drawn: &[crate::script_draw::Cmd]| -> (f32, f32, f32, f32) {
            let (bx, bw) = drawn
                .iter()
                .filter_map(|c| match c {
                    crate::script_draw::Cmd::Rect { x, w, h, .. } if *h > 30.0 => Some((*x, *w)),
                    _ => None,
                })
                .fold((0.0, 0.0), |a, b| if b.1 > a.1 { b } else { a });
            let text_x = |s: &str| {
                drawn.iter().find_map(|c| match c {
                    crate::script_draw::Cmd::Text { x, key, .. } if key.text == s => Some(*x),
                    _ => None,
                })
            };
            (bx, bw, text_x("Mo").expect("the name"), text_x("⚔").expect("the MOD badge"))
        };

        chat.update(&[("layout_mode", Dynamic::from("stacked-split".to_string()))]);
        chat.event(message);
        let (bx, bw, name, badge) = placed(&chat.frame(1.0));
        assert!(bw > 250.0, "a wide bubble: {bw}");
        assert!(name < bx + 20.0, "the name on the left: {name} in {bx}+{bw}");
        assert!(badge > bx + bw - 45.0, "the badge at the right edge: {badge} in {bx}+{bw}");

        // A fixed bubble width: the line is that wide
        chat.update(&[("bubble_width", Dynamic::from_int(420))]);
        chat.event(r##"{"type":"chat","source":"Twitch","username":"Mo","color":"#9146FF","badges":"[MOD]","message":"short"}"##);
        let drawn = chat.frame(1.0);
        let last: Vec<_> = drawn.iter().rev().take(40).cloned().collect::<Vec<_>>().into_iter().rev().collect();
        let (bx, bw, _, badge) = placed(&last);
        assert_eq!(bw, 420.0);
        assert!(badge > bx + bw - 45.0, "fixed width: the badge at its right edge: {badge} in {bx}+{bw}");
    }

    #[test]
    fn the_chat_shares_its_look_for_the_alerts() {
        let Some(_chat) = Chat::new() else { return };
        // (Other chats in other tests share theirs too: only its shape is checked)
        let look = SHARED.read().get("theater_chat_look").cloned().expect("the chat shares its look");
        let look = look.try_cast::<Map>().unwrap();
        for key in ["stamp", "bg", "bg_mode", "text", "name", "name_mode", "font", "weight_user", "radius", "shadow", "bubble", "border", "border_color", "opacity"] {
            assert!(look.contains_key(key), "{key} in {look:?}");
        }
        assert!(look["stamp"].as_int().unwrap() >= 1);
    }

    #[test]
    fn the_chat_clears_on_a_clear_chat_message() {
        let Some(mut chat) = Chat::new() else { return };
        chat.event(r#"{"type":"chat","source":"Twitch","user":"Anna","message":"first line"}"#);
        chat.event(r#"{"type":"chat","source":"Twitch","user":"Ben","message":"second line"}"#);
        let words = texts(&chat.frame(0.5)).join("|");
        assert!(words.contains("first line") && words.contains("second line"), "{}", words);
        // (A dock's Clear button sends it)
        chat.event(r#"{"type":"clear_chat"}"#);
        chat.frame(1.0);
        let words = texts(&chat.frame(0.1)).join("|");
        assert!(!words.contains("first line") && !words.contains("second line"), "{}", words);
    }

    #[test]
    fn a_dock_script_compiles_without_the_filters_helpers() {
        let path = Path::new("dock.rhai");
        let dock = compile("const ID = \"d\"; const KIND = \"dock\"; fn ui(ui) { ui.button(\"x\"); }", path).unwrap();
        assert_eq!(dock.kind, ScriptKind::Dock);
        assert!(dock.shader.is_empty());
        assert!(!dock.has("toggle"), "the property helpers stay out: ui.toggle… would find them");
        let err = compile("const ID = \"d\"; const KIND = \"dock\"; fn tick(dt) { () }", path).unwrap_err();
        assert!(err.contains("fn ui(ui)"), "{}", err);
    }

    #[test]
    fn the_chat_shows_relay_messages() {
        let Some(mut chat) = Chat::new() else { return };
        assert_eq!(chat.compiled.kind, ScriptKind::Source);
        assert_eq!(chat.compiled.id, "theater_chat_source", "the Lua chat's id: sources made with it come back");
        assert_eq!(chat.settings["applied_preset"].to_string(), "glass", "a new chat gets its theme");
        let size: Array = chat.this.read_lock::<Map>().unwrap()["size"].clone().try_cast().unwrap();
        assert_eq!((size[0].as_int().unwrap(), size[1].as_int().unwrap()), (450, 700));

        // A message's text and emote tags, split
        let parts = call(&chat.compiled, None, "tokenize", ("hello chat [twitch_emote:25] gg [x] [emote:9:Wave]".to_string(),)).unwrap();
        let parts: Array = parts.try_cast().unwrap();
        let described: Vec<String> = parts.iter().map(|p| {
            let m = p.read_lock::<Map>().unwrap();
            format!("{}:{}", m["t"], m.get("s").or_else(|| m.get("src")).map(|v| v.to_string()).unwrap_or_default())
        }).collect();
        assert_eq!(described, [
            "text:hello chat ",
            "emote:https://static-cdn.jtvnw.net/emoticons/v2/25/default/dark/2.0",
            "text: gg [x] ",
            "emote:https://files.kick.com/emotes/9/fullsize",
        ]);
        let words: Array = call(&chat.compiled, None, "word_items", ("hello chat ".to_string(), Dynamic::from_int(-1), Dynamic::from_int(400), chat.this.read_lock::<Map>().unwrap()["style"].clone())).unwrap().try_cast().unwrap();
        assert_eq!(words.len(), 2, "one item a word: {:?}", words);
        let style = chat.this.read_lock::<Map>().unwrap()["style"].clone();
        let message = call(&chat.compiled, None, "message_of", (Dynamic::from_map(event_map(r#"{"type":"chat","user":"A","message":"one two [twitch_emote:25]"}"#)), Dynamic::from_float(0.0), Dynamic::from_int(1))).unwrap();
        let body = call(&chat.compiled, None, "body_items", (message.clone(), style.clone())).unwrap();
        let body = body.read_lock::<Map>().unwrap().clone();
        let items: Array = body["items"].clone().try_cast().unwrap();
        assert_eq!(items.len(), 3, "two words and the emote: {:?}", body);

        chat.event(r##"{"type":"chat","source":"Twitch","username":"Morty","color":"#9146FF","badges":"[MOD] [SUB] [badge:https://static-cdn.jtvnw.net/badges/v1/x/1:Founder]","message":"hello chat [twitch_emote:4242] gg","msg_id":"abc"}"##);
        let drawn = chat.frame(1.0);
        let words = texts(&drawn).join("|");
        assert!(words.contains("Morty"), "{}", words);
        assert!(words.contains("hello chat"), "{}", words);
        assert!(words.contains("⚔") && words.contains("★"), "the MOD and SUB badges: {}", words);
        assert!(drawn.iter().any(|c| matches!(c, crate::script_draw::Cmd::Image { src, .. } if src.contains("emoticons/v2/4242"))), "the emote");
        assert!(drawn.iter().any(|c| matches!(c, crate::script_draw::Cmd::Image { src, .. } if src.contains("badges/v1/x/1"))), "the picture badge");
        // The bubble: the glass theme's background, rounded
        assert!(drawn.iter().any(|c| matches!(c, crate::script_draw::Cmd::Rect { fill, radius, .. } if *fill == 0x7314141E && *radius == 12.0)));

        // A whisper, and plain text that looks like a tag
        chat.event(r#"{"type":"whisper","source":"Twitch","user":"Sneaky","message":"psst [not a tag] ok"}"#);
        let words = texts(&chat.frame(1.0)).join("|");
        assert!(words.contains("[WHISPER]") && words.contains("Sneaky") && words.contains("[not a tag]"), "{}", words);

        // Clean mode: a flagged message slides away
        chat.update(&[("clean", Dynamic::TRUE)]);
        chat.event(r#"{"type":"moderation","payload":{"trigger":"AI FLAGGED toxic","msg_id":"abc"}}"#);
        chat.frame(0.1);
        let words = texts(&chat.frame(0.5)).join("|");
        assert!(!words.contains("Morty") && words.contains("Sneaky"), "{}", words);

        // Other relay messages (alerts, redeems) aren't chat
        chat.event(r#"{"type":"event","trigger":"Twitch Morty redeem Throw Tomato"}"#);
        chat.event("not json at all");
        assert_eq!(texts(&chat.frame(0.1)).iter().filter(|t| t.contains("Tomato")).count(), 0);

        // A theme picked: its settings are written
        let written = chat.update(&[("preset", Dynamic::from("terminal"))]);
        assert_eq!(written["font"].to_string(), "Roboto Mono");
        assert_eq!(written["bubble_style"].to_string(), "left-border");
        assert!(texts(&chat.frame(0.1)).iter().any(|t| t.contains("Sneaky")), "laid out again in the new theme");

        // Every layout and bubble style draws
        for layout in ["inline", "stacked", "stacked-split", "inline-align", "stacked-align"] {
            for bubble in ["bubble", "double-bubble", "left-border", "bottom-line", "minimal"] {
                chat.update(&[("layout_mode", Dynamic::from(layout)), ("bubble_style", Dynamic::from(bubble))]);
                let words = texts(&chat.frame(0.1)).join("|");
                assert!(words.contains("Sneaky") && words.contains("psst"), "{} / {}: {}", layout, bubble, words);
            }
        }

        // Expiry: gone after its time and the fade
        chat.update(&[("expiry", Dynamic::from_int(5))]);
        chat.call("on_button", Dynamic::from("test"));
        assert!(texts(&chat.frame(0.1)).iter().any(|t| t.contains("StreamFan42")), "the test button's message");
        chat.frame(5.0);
        let words = texts(&chat.frame(0.5)).join("|");
        assert!(!words.contains("StreamFan42") && !words.contains("Sneaky"), "{}", words);
    }

    #[test]
    fn rosia_chat_draws_with_rose_wrapped_gray_border_and_8bit_style() {
        use crate::script_draw::Cmd;
        let Some(mut chat) = Chat::load("Rosia_chat.rhai") else { return };
        assert_eq!(chat.compiled.kind, ScriptKind::Source);
        assert_eq!(chat.compiled.id, "rosia_chat_source");
        assert_eq!(chat.compiled.name, "🌹 Rosia Chat");
        assert_eq!(chat.settings["font"].to_string(), "Roboto Mono");
        assert_eq!(chat.settings["bubble_style"].to_string(), "rose-frame");

        chat.event(r#"{"type":"chat","source":"Twitch","username":"RosiaFan","message":"hello roses"}"#);
        let cmds = chat.frame(0.5);
        let words = texts(&cmds).join("|");
        assert!(words.contains("RosiaFan") && words.contains("hello roses"), "{}", words);

        // Verify gray border and rose/vine rects are drawn
        let mut drawn = Vec::new();
        flatten(&cmds, &mut drawn);
        let rects: Vec<_> = drawn.iter().filter_map(|c| match c {
            Cmd::Rect { fill, .. } => Some(*fill),
            _ => None,
        }).collect();
        assert!(rects.len() > 15, "stone gray border, studs, vines, and roses are drawn: {}", rects.len());

        // Check that rose petal colors are drawn
        let rose_petals = rects.iter().any(|fill| {
            let r = (fill >> 16) & 0xFF;
            let g = (fill >> 8) & 0xFF;
            r > 160 && g < 100
        });
        assert!(rose_petals, "rose petals must be drawn with red/rose tones: {rects:x?}");

        // Check that rose petal lip lines and vine winding lines are drawn
        let lines: Vec<_> = drawn.iter().filter_map(|c| match c {
            Cmd::Line { color, .. } => Some(*color),
            _ => None,
        }).collect();
        assert!(lines.len() >= 5, "rose petal rims, veins, and winding vine loops drawn: {}", lines.len());
    }
}

