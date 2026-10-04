//! Showing and hiding scene items with a transition, in Theater Scene's own
//! renderer (the Throwables Layer draws every item itself, so OBS's own drawing
//! of an item's Show/Hide transition never reaches the screen).
//!
//! Two kinds:
//! - OBS's transitions (Fade, Swipe, Slide, Luma Wipe, Stinger…): an item's
//!   Show/Hide transition, set in OBS or by `set_visibility`. OBS starts it
//!   when the item's visibility changes; while it plays, the renderer draws the
//!   transition instead of the source (as OBS's own render_item does).
//! - Theater's effects (Pop, Zoom, Flip…): animations of the item's Studio
//!   transform, in 3D.
//!
//! OBS's visibility is the truth: an item is drawn while it's visible in OBS, or
//! while its hide transition or effect is still playing.

use std::collections::HashMap;
use std::ffi::{CStr, CString, c_char, c_void};
use std::sync::{LazyLock, OnceLock};
use std::time::{Duration, Instant};

use crate::obs_source::obs_symbol;

// ─── libobs ──────────────────────────────────────────────────────────────────

struct Fns {
    get_transition: unsafe extern "C" fn(*mut c_void, bool) -> *mut c_void,
    set_transition: unsafe extern "C" fn(*mut c_void, bool, *mut c_void),
    set_transition_duration: unsafe extern "C" fn(*mut c_void, bool, u32),
    transition_get_time: unsafe extern "C" fn(*mut c_void) -> f32,
    enum_transition_types: unsafe extern "C" fn(usize, *mut *const c_char) -> bool,
    get_source_output_flags: unsafe extern "C" fn(*const c_char) -> u32,
    source_get_display_name: unsafe extern "C" fn(*const c_char) -> *const c_char,
    get_source_by_name: unsafe extern "C" fn(*const c_char) -> *mut c_void,
    source_release: unsafe extern "C" fn(*mut c_void),
    scene_from_source: unsafe extern "C" fn(*const c_void) -> *mut c_void,
    scene_find_source_recursive: unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut c_void,
    sceneitem_visible: unsafe extern "C" fn(*const c_void) -> bool,
    sceneitem_set_visible: unsafe extern "C" fn(*mut c_void, bool) -> bool,
    source_create_private: unsafe extern "C" fn(*const c_char, *const c_char, *mut c_void) -> *mut c_void,
    data_create_from_json: unsafe extern "C" fn(*const c_char) -> *mut c_void,
    data_release: unsafe extern "C" fn(*mut c_void),
    get_module: unsafe extern "C" fn(*const c_char) -> *mut c_void,
    find_module_file: unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut c_char,
    bfree: unsafe extern "C" fn(*mut c_void),
}

// Function pointers into libobs, which outlives us.
unsafe impl Send for Fns {}
unsafe impl Sync for Fns {}

fn fns() -> Option<&'static Fns> {
    static FNS: OnceLock<Option<Fns>> = OnceLock::new();
    FNS.get_or_init(|| {
        fn sym<T>(name: &CStr) -> Option<T> {
            let p = obs_symbol(name);
            // SAFETY: T is the function pointer type of the libobs export `name`.
            (!p.is_null()).then(|| unsafe { std::mem::transmute_copy(&p) })
        }
        Some(Fns {
            get_transition: sym(c"obs_sceneitem_get_transition")?,
            set_transition: sym(c"obs_sceneitem_set_transition")?,
            set_transition_duration: sym(c"obs_sceneitem_set_transition_duration")?,
            transition_get_time: sym(c"obs_transition_get_time")?,
            enum_transition_types: sym(c"obs_enum_transition_types")?,
            get_source_output_flags: sym(c"obs_get_source_output_flags")?,
            source_get_display_name: sym(c"obs_source_get_display_name")?,
            get_source_by_name: sym(c"obs_get_source_by_name")?,
            source_release: sym(c"obs_source_release")?,
            scene_from_source: sym(c"obs_scene_from_source")?,
            scene_find_source_recursive: sym(c"obs_scene_find_source_recursive")?,
            sceneitem_visible: sym(c"obs_sceneitem_visible")?,
            sceneitem_set_visible: sym(c"obs_sceneitem_set_visible")?,
            source_create_private: sym(c"obs_source_create_private")?,
            data_create_from_json: sym(c"obs_data_create_from_json")?,
            data_release: sym(c"obs_data_release")?,
            get_module: sym(c"obs_get_module")?,
            find_module_file: sym(c"obs_find_module_file")?,
            bfree: sym(c"bfree")?,
        })
    })
    .as_ref()
}

fn cstring(s: &str) -> CString {
    CString::new(s.replace('\0', "")).unwrap_or_default()
}

fn string(s: *const c_char) -> String {
    if s.is_null() { String::new() } else { unsafe { CStr::from_ptr(s) }.to_string_lossy().into_owned() }
}

// ─── Theater's effects ───────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Effect {
    Pop,
    Zoom,
    Flip,
    Swing,
    Spin,
    Drop,
    Rise,
    SlideLeft,
    SlideRight,
}

impl Effect {
    const ALL: [(Effect, &'static str, &'static str); 9] = [
        (Effect::Pop, "pop", "Pop"),
        (Effect::Zoom, "zoom", "Zoom"),
        (Effect::Flip, "flip", "Flip (3D, vertical axis)"),
        (Effect::Swing, "swing", "Swing (3D, horizontal axis)"),
        (Effect::Spin, "spin", "Spin"),
        (Effect::Drop, "drop", "Drop (from above, bounces)"),
        (Effect::Rise, "rise", "Rise (from below)"),
        (Effect::SlideLeft, "slide_left", "Slide from the left"),
        (Effect::SlideRight, "slide_right", "Slide from the right"),
    ];

    fn from_id(id: &str) -> Option<Effect> {
        Effect::ALL.iter().find(|(_, key, _)| *key == id).map(|(e, _, _)| *e)
    }

    fn name(self) -> &'static str {
        Effect::ALL.iter().find(|(e, _, _)| *e == self).map(|(_, _, n)| *n).unwrap_or("effect")
    }
}

struct Playing {
    effect: Effect,
    showing: bool,
    start: Instant,
    duration: Duration,
}

/// Effects playing, by scene item (its address, stable while it exists).
static PLAYING: LazyLock<parking_lot::Mutex<HashMap<usize, Playing>>> = LazyLock::new(Default::default);

/// One frame of an effect.
#[derive(Clone, Copy)]
pub struct Frame {
    effect: Effect,
    showing: bool,
    /// 0 → 1 over the effect's duration.
    progress: f32,
}

fn ease_out_cubic(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}

fn ease_in_cubic(t: f32) -> f32 {
    t * t * t
}

fn ease_out_back(t: f32) -> f32 {
    let c1 = 1.70158;
    let c3 = c1 + 1.0;
    1.0 + c3 * (t - 1.0).powi(3) + c1 * (t - 1.0).powi(2)
}

fn ease_out_bounce(t: f32) -> f32 {
    let (n1, d1) = (7.5625, 2.75);
    if t < 1.0 / d1 {
        n1 * t * t
    } else if t < 2.0 / d1 {
        let t = t - 1.5 / d1;
        n1 * t * t + 0.75
    } else if t < 2.5 / d1 {
        let t = t - 2.25 / d1;
        n1 * t * t + 0.9375
    } else {
        let t = t - 2.625 / d1;
        n1 * t * t + 0.984375
    }
}

impl Frame {
    /// How much of the item is "in": 1 = in place, 0 = gone. Showing eases in
    /// (Pop overshoots, Drop bounces); hiding accelerates away.
    fn amount(&self) -> f32 {
        let p = self.progress.clamp(0.0, 1.0);
        if self.showing {
            match self.effect {
                Effect::Pop => ease_out_back(p),
                Effect::Drop => ease_out_bounce(p),
                _ => ease_out_cubic(p),
            }
        } else {
            1.0 - ease_in_cubic(p)
        }
    }

    /// Moves `src` (its Studio transform for this frame) along the effect.
    /// `canvas` is the canvas size, for effects that come from off-screen.
    pub fn apply(&self, src: &mut crate::StudioSource, canvas: (f32, f32)) {
        let a = self.amount();
        let gone = 1.0 - a;
        let scale_about_center = |src: &mut crate::StudioSource, s: f32| {
            let s = s.max(0.0);
            let (w, h) = (src.width, src.height);
            src.x += (w - w * s) * 0.5;
            src.y += (h - h * s) * 0.5;
            src.width = w * s;
            src.height = h * s;
        };
        match self.effect {
            Effect::Pop | Effect::Zoom => scale_about_center(src, a),
            Effect::Flip => src.yaw += gone * 90.0,
            Effect::Swing => src.pitch += gone * 90.0,
            Effect::Spin => {
                src.roll += gone * 360.0;
                scale_about_center(src, a);
            }
            Effect::Drop => src.y -= gone * (src.y + src.height),
            Effect::Rise => src.y += gone * (canvas.1 - src.y),
            Effect::SlideLeft => src.x -= gone * (src.x + src.width),
            Effect::SlideRight => src.x += gone * (canvas.0 - src.x),
        }
    }
}

/// The effect frame playing on `item` now, if any (finished ones are dropped).
pub fn frame(item: *mut c_void) -> Option<Frame> {
    let mut playing = PLAYING.lock();
    let key = item as usize;
    let p = playing.get(&key)?;
    let progress = p.start.elapsed().as_secs_f32() / p.duration.as_secs_f32().max(0.001);
    if progress >= 1.0 {
        playing.remove(&key);
        return None;
    }
    Some(Frame { effect: p.effect, showing: p.showing, progress })
}

// ─── The renderer's side ─────────────────────────────────────────────────────

/// What to draw for a scene item this frame, or None to skip it.
pub struct Draw {
    /// The item's source, or its OBS Show/Hide transition while that plays.
    pub source: *mut c_void,
    /// Theater's effect playing on it.
    pub effect: Option<Frame>,
    /// Its visibility in OBS.
    pub visible: bool,
    /// OBS's visibility changed since the last frame (eye toggle, websocket,
    /// Control Panel…): the Studio's copy should follow.
    pub flipped: bool,
}

impl Draw {
    /// A show/hide transition or effect is playing: drawn whatever the Studio
    /// copy of the visibility says, until it's done.
    pub fn playing(&self, item_source: *mut c_void) -> bool {
        self.effect.is_some() || self.source != item_source
    }
}

/// OBS's visibility of each item in the last frame, to see it change.
static LAST_VISIBLE: LazyLock<parking_lot::Mutex<HashMap<usize, bool>>> = LazyLock::new(Default::default);

/// Decides how to draw `item` (whose source is `source`): OBS's visibility,
/// plus a hide transition/effect still playing, plus a show transition in
/// place of the source.
pub fn draw(item: *mut c_void, source: *mut c_void) -> Option<Draw> {
    let Some(f) = fns() else {
        // Can't see transitions: plain OBS visibility
        return Some(Draw { source, effect: None, visible: true, flipped: false });
    };
    let visible = unsafe { (f.sceneitem_visible)(item) };
    let flipped = LAST_VISIBLE.lock().insert(item as usize, visible).is_some_and(|last| last != visible);
    if let Some(effect) = frame(item) {
        // An effect for the other direction was cut short by a visibility change
        if effect.showing == visible {
            return Some(Draw { source, effect: Some(effect), visible, flipped });
        }
    }
    let transition = unsafe { (f.get_transition)(item, visible) };
    let playing = !transition.is_null() && unsafe { (f.transition_get_time)(transition) } < 1.0;
    match (visible, playing) {
        (_, true) => Some(Draw { source: transition, effect: None, visible, flipped }),
        (true, false) => Some(Draw { source, effect: None, visible, flipped }),
        (false, false) => None,
    }
}

// ─── Lists for the Control Panel ─────────────────────────────────────────────

/// OBS's transition types (not deprecated or disabled), then Theater's effects:
/// [{"id": "obs:fade_transition" | "theater:pop", "name"}].
pub fn transitions_json() -> Vec<serde_json::Value> {
    const OBS_SOURCE_DEPRECATED: u32 = 1 << 8;
    const OBS_SOURCE_CAP_DISABLED: u32 = 1 << 10;
    let mut list = Vec::new();
    if let Some(f) = fns() {
        let mut idx = 0;
        let mut id: *const c_char = std::ptr::null();
        while unsafe { (f.enum_transition_types)(idx, &mut id) } {
            idx += 1;
            let flags = unsafe { (f.get_source_output_flags)(id) };
            if flags & (OBS_SOURCE_DEPRECATED | OBS_SOURCE_CAP_DISABLED) != 0 {
                continue;
            }
            let type_id = string(id);
            let name = string(unsafe { (f.source_get_display_name)(id) });
            list.push(serde_json::json!({ "id": format!("obs:{type_id}"), "name": if name.is_empty() { type_id } else { name } }));
        }
    }
    for (_, key, name) in Effect::ALL {
        list.push(serde_json::json!({ "id": format!("theater:{key}"), "name": name }));
    }
    list
}

/// OBS's Luma Wipe patterns: [{"file", "name"}], from obs-transitions' wipes.json.
pub fn luma_wipes_json() -> Vec<serde_json::Value> {
    let Some(f) = fns() else { return Vec::new() };
    let module = unsafe { (f.get_module)(c"obs-transitions".as_ptr()) };
    if module.is_null() {
        return Vec::new();
    }
    let path = unsafe { (f.find_module_file)(module, c"luma_wipes/wipes.json".as_ptr()) };
    if path.is_null() {
        return Vec::new();
    }
    let file = string(path);
    unsafe { (f.bfree)(path as *mut c_void) };
    let Ok(text) = std::fs::read_to_string(file) else { return Vec::new() };
    let Ok(serde_json::Value::Object(wipes)) = serde_json::from_str::<serde_json::Value>(&text) else { return Vec::new() };
    let mut list: Vec<(String, String)> = wipes
        .iter()
        .filter_map(|(key, file)| {
            let file = file.as_str()?.to_string();
            // "LumaWipe.Type.BarndoorBottomLeft" → "Barndoor Bottom Left"
            let raw = key.rsplit('.').next().unwrap_or(key);
            let mut name = String::new();
            for (i, c) in raw.chars().enumerate() {
                if i > 0 && c.is_uppercase() {
                    name.push(' ');
                }
                name.push(c);
            }
            Some((name, file))
        })
        .collect();
    list.sort();
    list.into_iter().map(|(name, file)| serde_json::json!({ "file": file, "name": name })).collect()
}

// ─── Setting visibility ──────────────────────────────────────────────────────

/// Shows, hides or toggles `source_name` in `scene_name` with `transition`:
/// "" or "obs:cut_transition" (instant), "obs:<type id>" with `settings` (that
/// transition's OBS settings, JSON), or "theater:<effect>". An OBS transition
/// becomes the item's Show/Hide transition, as if set in OBS. UI thread.
pub fn set_visibility(
    scene_name: &str,
    source_name: &str,
    action: &str,
    transition: &str,
    duration_ms: u32,
    settings: &str,
) -> Result<String, String> {
    let f = fns().ok_or("OBS functions unavailable")?;
    if source_name.is_empty() {
        return Err("no source selected".into());
    }
    let scene_c = cstring(scene_name);
    let scene_source = unsafe { (f.get_source_by_name)(scene_c.as_ptr()) };
    if scene_source.is_null() {
        return Err(format!("scene '{scene_name}' not found"));
    }
    let result = (|| {
        let scene = unsafe { (f.scene_from_source)(scene_source) };
        if scene.is_null() {
            return Err(format!("'{scene_name}' isn't a scene"));
        }
        let source_c = cstring(source_name);
        let item = unsafe { (f.scene_find_source_recursive)(scene, source_c.as_ptr()) };
        if item.is_null() {
            return Err(format!("'{source_name}' isn't in scene '{scene_name}'"));
        }
        let visible = unsafe { (f.sceneitem_visible)(item) };
        let show = match action {
            "show" => true,
            "hide" => false,
            _ => !visible,
        };
        let verb = if show { "shown" } else { "hidden" };
        if show == visible {
            return Ok(format!("'{source_name}' already {verb}"));
        }

        let duration_ms = duration_ms.max(1);
        let how = if let Some(effect_id) = transition.strip_prefix("theater:") {
            let effect = Effect::from_id(effect_id).ok_or(format!("unknown effect '{effect_id}'"))?;
            unsafe { (f.set_transition)(item, show, std::ptr::null_mut()) };
            PLAYING.lock().insert(
                item as usize,
                Playing { effect, showing: show, start: Instant::now(), duration: Duration::from_millis(duration_ms as u64) },
            );
            effect.name().to_string()
        } else {
            let type_id = transition.strip_prefix("obs:").unwrap_or(transition);
            if type_id.is_empty() || type_id == "cut_transition" {
                unsafe { (f.set_transition)(item, show, std::ptr::null_mut()) };
                "no transition".to_string()
            } else {
                let settings_c = cstring(if settings.trim().is_empty() { "{}" } else { settings });
                let data = unsafe { (f.data_create_from_json)(settings_c.as_ptr()) };
                let type_c = cstring(type_id);
                let created = unsafe { (f.source_create_private)(type_c.as_ptr(), c"Theater visibility".as_ptr(), data) };
                if !data.is_null() {
                    unsafe { (f.data_release)(data) };
                }
                if created.is_null() {
                    return Err(format!("OBS couldn't create a '{type_id}' transition"));
                }
                unsafe {
                    // The item keeps its own reference
                    (f.set_transition)(item, show, created);
                    (f.set_transition_duration)(item, show, duration_ms);
                    (f.source_release)(created);
                }
                string(unsafe { (f.source_get_display_name)(type_c.as_ptr()) })
            }
        };

        unsafe { (f.sceneitem_set_visible)(item, show) };
        // The Studio's own copy of the visibility
        for s in crate::state::get_state().studio_sources.write().iter_mut() {
            if s.name == source_name && (s.scene == scene_name || s.scene.is_empty()) {
                s.is_visible = show;
            }
        }
        Ok(format!("'{source_name}' {verb} ({how}, {duration_ms} ms)"))
    })();
    unsafe { (f.source_release)(scene_source) };
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(effect: Effect, showing: bool, progress: f32) -> Frame {
        Frame { effect, showing, progress }
    }

    #[test]
    fn effects_start_gone_and_end_in_place() {
        for (effect, _, _) in Effect::ALL {
            assert!(frame(effect, true, 0.0).amount().abs() < 1e-6, "{effect:?} starts gone when showing");
            assert!((frame(effect, true, 1.0).amount() - 1.0).abs() < 1e-4, "{effect:?} ends in place when showing");
            assert!((frame(effect, false, 0.0).amount() - 1.0).abs() < 1e-6, "{effect:?} starts in place when hiding");
            assert!(frame(effect, false, 1.0).amount().abs() < 1e-6, "{effect:?} ends gone when hiding");
        }
    }

    #[test]
    fn effect_ids_round_trip() {
        for (effect, key, _) in Effect::ALL {
            assert_eq!(Effect::from_id(key), Some(effect));
        }
        assert_eq!(Effect::from_id("nope"), None);
    }
}
