//! Each item drawn once. The Throwables Layer draws the current scene itself
//! (its items with Theater's positions, 3D, moves and effects, then the
//! throwables), and OBS had drawn those same items just before, under it:
//! every item and its filters twice, OBS's copy hidden by the layer's black
//! backdrop. Two filters of Theater's skip that hidden copy:
//!
//! - `theater_scene_gate`, on the scene holding the layer: while OBS draws
//!   that scene, its items below the layer are Theater's to draw
//! - `theater_render_gate`, on each source the layer draws: it draws nothing
//!   then, and passes everything through otherwise (Theater's own pass, the
//!   items above the layer, a projector, the multiview, another canvas…)
//!
//! Not while a scene transition plays, nor when the layer isn't a visible
//! item of the scene itself: OBS draws everything then, as before. The pixel
//! scan and the redeems only ever use Theater's pass.

use std::collections::HashSet;
use std::ffi::{CStr, c_char, c_void};
use std::sync::atomic::{AtomicBool, Ordering};

use parking_lot::Mutex;

use crate::obs_source::trust_log;
use crate::script_filters::sym;

type P = *mut c_void;

const SCENE_GATE_ID: &CStr = c"theater_scene_gate";
const SOURCE_GATE_ID: &CStr = c"theater_render_gate";
/// What they're called in a source's Filters (OBS saves them with the scene
/// collection): how a saved one is recognized
const SCENE_GATE_NAME: &CStr = c"🎭 Theater: draws this scene once";
const SOURCE_GATE_NAME: &CStr = c"🎭 Theater: drawn once";
const LAYER_ID: &[u8] = b"theater_throwables_layer";

/// Drawing a scene whose items below the layer are Theater's (the render
/// thread only: an atomic for Sync, not for threads)
static SUPPRESS: AtomicBool = AtomicBool::new(false);
/// The layer drawing the scene itself
static THEATER_PASS: AtomicBool = AtomicBool::new(false);

/// OBS's obs_source_info, up to video_get_color_space (OBS 31 order)
#[repr(C)]
struct GateInfo {
    id: *const c_char,
    type_: u32,
    output_flags: u32,
    get_name: Option<unsafe extern "C" fn(P) -> *const c_char>,
    create: Option<unsafe extern "C" fn(P, P) -> P>,
    destroy: Option<unsafe extern "C" fn(P)>,
    get_width: P,
    get_height: P,
    get_defaults: P,
    get_properties: P,
    update: P,
    activate: P,
    deactivate: P,
    show: P,
    hide: P,
    video_tick: Option<unsafe extern "C" fn(P, f32)>,
    video_render: Option<unsafe extern "C" fn(P, P)>,
    filter_video: P,
    filter_audio: P,
    enum_active_sources: P,
    save: P,
    load: P,
    mouse_click: P,
    mouse_move: P,
    mouse_wheel: P,
    focus: P,
    key_click: P,
    filter_remove: P,
    type_data: P,
    free_type_data: P,
    audio_render: P,
    enum_all_sources: P,
    transition_start: P,
    transition_stop: P,
    get_defaults2: P,
    get_properties2: P,
    audio_mix: P,
    icon_type: i32,
    media_play_pause: P,
    media_restart: P,
    media_stop: P,
    media_next: P,
    media_previous: P,
    media_get_duration: P,
    media_get_time: P,
    media_set_time: P,
    media_get_state: P,
    version: u32,
    unversioned_id: *const c_char,
    missing_files: P,
    video_get_color_space: Option<unsafe extern "C" fn(P, usize, *const i32) -> i32>,
}

const OBS_SOURCE_TYPE_FILTER: u32 = 1;
const OBS_SOURCE_TYPE_TRANSITION: i32 = 2;
const OBS_SOURCE_VIDEO: u32 = 1 << 0;
/// Not offered in OBS's filter menus
const OBS_SOURCE_CAP_DISABLED: u32 = 1 << 10;
const OBS_TRANSITION_SOURCE_B: i32 = 1;

struct Fns {
    register_source: unsafe extern "C" fn(*const GateInfo, usize),
    skip_video_filter: unsafe extern "C" fn(P),
    filter_get_target: unsafe extern "C" fn(P) -> P,
    filter_get_parent: unsafe extern "C" fn(P) -> P,
    get_color_space: unsafe extern "C" fn(P, usize, *const i32) -> i32,
    scene_from_source: unsafe extern "C" fn(P) -> P,
    scene_enum_items: unsafe extern "C" fn(P, unsafe extern "C" fn(P, P, P) -> bool, P),
    sceneitem_get_source: unsafe extern "C" fn(P) -> P,
    sceneitem_visible: unsafe extern "C" fn(P) -> bool,
    source_get_id: unsafe extern "C" fn(P) -> *const c_char,
    source_get_type: unsafe extern "C" fn(P) -> i32,
    get_output_source: unsafe extern "C" fn(u32) -> P,
    transition_get_source: unsafe extern "C" fn(P, i32) -> P,
    source_release: unsafe extern "C" fn(P),
    create_private: unsafe extern "C" fn(*const c_char, *const c_char, P) -> P,
    filter_add: unsafe extern "C" fn(P, P),
    get_filter_by_name: unsafe extern "C" fn(P, *const c_char) -> P,
    filter_get_index: unsafe extern "C" fn(P, P) -> i32,
    filter_set_index: unsafe extern "C" fn(P, P, usize),
    current_scene: unsafe extern "C" fn() -> P,
}

fn fns() -> Option<&'static Fns> {
    static FNS: std::sync::OnceLock<Option<Fns>> = std::sync::OnceLock::new();
    FNS.get_or_init(|| {
        Some(Fns {
            register_source: sym(c"obs_register_source_s")?,
            skip_video_filter: sym(c"obs_source_skip_video_filter")?,
            filter_get_target: sym(c"obs_filter_get_target")?,
            filter_get_parent: sym(c"obs_filter_get_parent")?,
            get_color_space: sym(c"obs_source_get_color_space")?,
            scene_from_source: sym(c"obs_scene_from_source")?,
            scene_enum_items: sym(c"obs_scene_enum_items")?,
            sceneitem_get_source: sym(c"obs_sceneitem_get_source")?,
            sceneitem_visible: sym(c"obs_sceneitem_visible")?,
            source_get_id: sym(c"obs_source_get_id")?,
            source_get_type: sym(c"obs_source_get_type")?,
            get_output_source: sym(c"obs_get_output_source")?,
            transition_get_source: sym(c"obs_transition_get_source")?,
            source_release: sym(c"obs_source_release")?,
            create_private: sym(c"obs_source_create_private")?,
            filter_add: sym(c"obs_source_filter_add")?,
            get_filter_by_name: sym(c"obs_source_get_filter_by_name")?,
            filter_get_index: sym(c"obs_source_filter_get_index")?,
            filter_set_index: sym(c"obs_source_filter_set_index")?,
            current_scene: sym(c"obs_frontend_get_current_scene")?,
        })
    })
    .as_ref()
}

// ─── The two filters ────────────────────────────────────────────────────────

struct Gate {
    context: P,
    /// Scene gate: this frame, the scene's items below the layer are Theater's
    active: AtomicBool,
}

unsafe extern "C" fn scene_gate_name(_: P) -> *const c_char {
    c"Theater: draws its scene once".as_ptr()
}

unsafe extern "C" fn source_gate_name(_: P) -> *const c_char {
    c"Theater: drawn once".as_ptr()
}

unsafe extern "C" fn create(_settings: P, source: P) -> P {
    Box::into_raw(Box::new(Gate { context: source, active: AtomicBool::new(false) })) as P
}

unsafe extern "C" fn destroy(data: P) {
    if !data.is_null() {
        drop(unsafe { Box::from_raw(data as *mut Gate) });
    }
}

/// Its source's colour space: a filter without one would be taken for SDR
unsafe extern "C" fn color_space(data: P, count: usize, preferred: *const i32) -> i32 {
    let gate = unsafe { &*(data as *const Gate) };
    let Some(f) = fns() else { return 0 };
    let target = unsafe { (f.filter_get_target)(gate.context) };
    if target.is_null() {
        return if count > 0 && !preferred.is_null() { unsafe { *preferred } } else { 0 };
    }
    unsafe { (f.get_color_space)(target, count, preferred) }
}

unsafe extern "C" fn scene_gate_tick(data: P, _seconds: f32) {
    let gate = unsafe { &*(data as *const Gate) };
    let Some(f) = fns() else { return };
    let scene_source = unsafe { (f.filter_get_parent)(gate.context) };
    let active = !scene_source.is_null() && !transitioning(f) && unsafe { has_visible_layer(f, scene_source) };
    gate.active.store(active, Ordering::Relaxed);
}

unsafe extern "C" fn scene_gate_render(data: P, _effect: P) {
    let gate = unsafe { &*(data as *const Gate) };
    let Some(f) = fns() else { return };
    if gate.active.load(Ordering::Relaxed) {
        let before = SUPPRESS.swap(true, Ordering::Relaxed);
        unsafe { (f.skip_video_filter)(gate.context) };
        SUPPRESS.store(before, Ordering::Relaxed);
    } else {
        unsafe { (f.skip_video_filter)(gate.context) };
    }
}

unsafe extern "C" fn source_gate_render(data: P, _effect: P) {
    let gate = unsafe { &*(data as *const Gate) };
    if SUPPRESS.load(Ordering::Relaxed) && !THEATER_PASS.load(Ordering::Relaxed) {
        // OBS's copy, under the layer: Theater draws this one
        return;
    }
    if let Some(f) = fns() {
        unsafe { (f.skip_video_filter)(gate.context) };
    }
}

/// A scene transition playing (two scenes on screen): OBS draws it all.
fn transitioning(f: &Fns) -> bool {
    let output = unsafe { (f.get_output_source)(0) };
    if output.is_null() {
        return false;
    }
    let mut playing = false;
    if unsafe { (f.source_get_type)(output) } == OBS_SOURCE_TYPE_TRANSITION {
        let incoming = unsafe { (f.transition_get_source)(output, OBS_TRANSITION_SOURCE_B) };
        if !incoming.is_null() {
            playing = true;
            unsafe { (f.source_release)(incoming) };
        }
    }
    unsafe { (f.source_release)(output) };
    playing
}

unsafe extern "C" fn find_layer(_scene: P, item: P, param: P) -> bool {
    let (f, found) = unsafe { &mut *(param as *mut (&Fns, bool)) };
    let source = unsafe { (f.sceneitem_get_source)(item) };
    if !source.is_null() && unsafe { (f.sceneitem_visible)(item) } {
        let id = unsafe { (f.source_get_id)(source) };
        if !id.is_null() && unsafe { CStr::from_ptr(id) }.to_bytes() == LAYER_ID {
            *found = true;
            return false;
        }
    }
    true
}

/// The scene has the Throwables Layer as a visible item of its own.
unsafe fn has_visible_layer(f: &Fns, scene_source: P) -> bool {
    let scene = unsafe { (f.scene_from_source)(scene_source) };
    if scene.is_null() {
        return false;
    }
    let mut search: (&Fns, bool) = (f, false);
    unsafe { (f.scene_enum_items)(scene, find_layer, &mut search as *mut (&Fns, bool) as P) };
    search.1
}

/// Registers the two filters: when OBS loads Theater, before scenes load
/// (saved scenes keep them).
pub fn register() {
    let Some(f) = fns() else {
        trust_log("[Trust Core] ⚠️ libobs functions missing: items are drawn twice under the Throwables Layer");
        return;
    };
    for (id, name, tick, render) in [
        (SCENE_GATE_ID, scene_gate_name as unsafe extern "C" fn(P) -> *const c_char, Some(scene_gate_tick as unsafe extern "C" fn(P, f32)), scene_gate_render as unsafe extern "C" fn(P, P)),
        (SOURCE_GATE_ID, source_gate_name, None, source_gate_render),
    ] {
        let info = Box::leak(Box::new(GateInfo {
            id: id.as_ptr(),
            type_: OBS_SOURCE_TYPE_FILTER,
            output_flags: OBS_SOURCE_VIDEO | OBS_SOURCE_CAP_DISABLED,
            get_name: Some(name),
            create: Some(create),
            destroy: Some(destroy),
            get_width: std::ptr::null_mut(),
            get_height: std::ptr::null_mut(),
            get_defaults: std::ptr::null_mut(),
            get_properties: std::ptr::null_mut(),
            update: std::ptr::null_mut(),
            activate: std::ptr::null_mut(),
            deactivate: std::ptr::null_mut(),
            show: std::ptr::null_mut(),
            hide: std::ptr::null_mut(),
            video_tick: tick,
            video_render: Some(render),
            filter_video: std::ptr::null_mut(),
            filter_audio: std::ptr::null_mut(),
            enum_active_sources: std::ptr::null_mut(),
            save: std::ptr::null_mut(),
            load: std::ptr::null_mut(),
            mouse_click: std::ptr::null_mut(),
            mouse_move: std::ptr::null_mut(),
            mouse_wheel: std::ptr::null_mut(),
            focus: std::ptr::null_mut(),
            key_click: std::ptr::null_mut(),
            filter_remove: std::ptr::null_mut(),
            type_data: std::ptr::null_mut(),
            free_type_data: std::ptr::null_mut(),
            audio_render: std::ptr::null_mut(),
            enum_all_sources: std::ptr::null_mut(),
            transition_start: std::ptr::null_mut(),
            transition_stop: std::ptr::null_mut(),
            get_defaults2: std::ptr::null_mut(),
            get_properties2: std::ptr::null_mut(),
            audio_mix: std::ptr::null_mut(),
            icon_type: 0,
            media_play_pause: std::ptr::null_mut(),
            media_restart: std::ptr::null_mut(),
            media_stop: std::ptr::null_mut(),
            media_next: std::ptr::null_mut(),
            media_previous: std::ptr::null_mut(),
            media_get_duration: std::ptr::null_mut(),
            media_get_time: std::ptr::null_mut(),
            media_set_time: std::ptr::null_mut(),
            media_get_state: std::ptr::null_mut(),
            version: 0,
            unversioned_id: std::ptr::null(),
            missing_files: std::ptr::null_mut(),
            video_get_color_space: Some(color_space),
        }));
        unsafe { (f.register_source)(info, std::mem::size_of::<GateInfo>()) };
    }
    trust_log("[Trust Core] 🎬 Items under the Throwables Layer are drawn once (render gates registered)");
}

// ─── Putting them on ────────────────────────────────────────────────────────

/// Sources and scenes given their gate this run (a saved one is found by name)
static GATED: Mutex<Option<HashSet<usize>>> = Mutex::new(None);
/// The layer's ticks: every GATE_CHECK_EVERY of them, each gate is checked
/// still to be outermost
static TICKS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
const GATE_CHECK_EVERY: u64 = 30;

/// A gate (`id`, called `name`) on `source`, unless it has one; and from
/// time to time, that it's still outermost.
unsafe fn ensure_gate(f: &Fns, source: P, id: &CStr, name: &CStr) {
    if source.is_null() {
        return;
    }
    let new = GATED.lock().get_or_insert_with(HashSet::new).insert(source as usize);
    if !new && TICKS.load(Ordering::Relaxed) % GATE_CHECK_EVERY != 0 {
        return;
    }
    let existing = unsafe { (f.get_filter_by_name)(source, name.as_ptr()) };
    if !existing.is_null() {
        unsafe {
            keep_outermost(f, source, existing);
            (f.source_release)(existing);
        }
        return;
    }
    if !new {
        // Removed by hand: it stays removed (until OBS starts again)
        return;
    }
    let gate = unsafe { (f.create_private)(id.as_ptr(), name.as_ptr(), std::ptr::null_mut()) };
    if !gate.is_null() {
        // Added last, so it's outermost: when it draws nothing, neither do the
        // filters under it
        unsafe {
            (f.filter_add)(source, gate);
            (f.source_release)(gate);
        }
    }
}

/// The gate must stay the outermost filter. A filter another plugin adds
/// later, at runtime (Carouselle's 3D carousel, say), would otherwise run
/// first in OBS's pass, over the gate's empty picture, and OBS keeps a
/// filter's input for the whole frame: Theater's own pass then drew that
/// empty picture too (the carousel went blank).
unsafe fn keep_outermost(f: &Fns, source: P, gate: P) {
    if unsafe { (f.filter_get_index)(source, gate) } > 0 {
        unsafe { (f.filter_set_index)(source, gate, 0) };
        trust_log("[Trust Core] 🎬 A filter was added over Theater's render gate: the gate is outermost again");
    }
}

/// From the layer's tick: the current scene gets its gate (it does nothing
/// while the layer isn't a visible item of it).
pub fn tick() {
    TICKS.fetch_add(1, Ordering::Relaxed);
    let Some(f) = fns() else { return };
    let scene = unsafe { (f.current_scene)() };
    if scene.is_null() {
        return;
    }
    unsafe {
        if has_visible_layer(f, scene) {
            ensure_gate(f, scene, SCENE_GATE_ID, SCENE_GATE_NAME);
        }
        (f.source_release)(scene);
    }
}

/// From the layer's scene render: each source it draws gets its gate
/// (outside the crop guard, so skipping it skips that pass too).
pub fn ensure_source_gate(source: P) {
    if let Some(f) = fns() {
        unsafe { ensure_gate(f, source, SOURCE_GATE_ID, SOURCE_GATE_NAME) };
    }
}

/// While the layer draws the scene itself: every source draws.
pub struct TheaterPass(bool);

pub fn theater_pass() -> TheaterPass {
    TheaterPass(THEATER_PASS.swap(true, Ordering::Relaxed))
}

impl Drop for TheaterPass {
    fn drop(&mut self) {
        THEATER_PASS.store(self.0, Ordering::Relaxed);
    }
}

/// Held through the layer's render: once it's drawn (however its render
/// ends), the items above it are OBS's to draw as usual.
pub struct LayerDrawn;

impl Drop for LayerDrawn {
    fn drop(&mut self) {
        SUPPRESS.store(false, Ordering::Relaxed);
    }
}

/// The layer is gone: sources and scenes may be new ones at the same address.
pub fn forget() {
    GATED.lock().take();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_gate_info_matches_obs_31s_source_info() {
        // 64-bit, one 8-byte slot a field: id 0, type + output_flags 1,
        // get_name 2 … get_properties2 34, audio_mix 35, icon_type 36 (padded),
        // the nine media calls 37–45, version 46 (padded), unversioned_id 47,
        // missing_files 48, video_get_color_space 49
        assert_eq!(std::mem::offset_of!(GateInfo, get_name), 2 * 8);
        assert_eq!(std::mem::offset_of!(GateInfo, video_render), 15 * 8);
        assert_eq!(std::mem::offset_of!(GateInfo, type_data), 27 * 8);
        assert_eq!(std::mem::offset_of!(GateInfo, get_properties2), 34 * 8);
        assert_eq!(std::mem::offset_of!(GateInfo, icon_type), 36 * 8);
        assert_eq!(std::mem::offset_of!(GateInfo, version), 46 * 8);
        assert_eq!(std::mem::offset_of!(GateInfo, video_get_color_space), 49 * 8);
    }

    #[test]
    fn only_theaters_own_pass_draws_under_the_layer() {
        let pass = |suppress: bool, theater: bool| {
            SUPPRESS.store(suppress, Ordering::Relaxed);
            let _pass = theater.then(theater_pass);
            let draws = !(SUPPRESS.load(Ordering::Relaxed) && !THEATER_PASS.load(Ordering::Relaxed));
            SUPPRESS.store(false, Ordering::Relaxed);
            draws
        };
        assert!(pass(false, false), "outside the layer's scene: as usual");
        assert!(!pass(true, false), "OBS's copy under the layer: skipped");
        assert!(pass(true, true), "Theater's pass: drawn");
        assert!(!THEATER_PASS.load(Ordering::Relaxed), "the pass ends with its guard");
    }
}
