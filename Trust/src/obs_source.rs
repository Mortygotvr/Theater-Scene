use std::ffi::{c_char, c_void, CStr, CString};

static SOURCE_ID: &[u8] = b"theater_throwables_layer\0";
static SOURCE_NAME: &[u8] = b"\xF0\x9F\x8E\xAC Theater Scene \xE2\x80\x94 Throwables Layer\0";

#[repr(C)]
pub struct ObsSourceInfo {
    pub id: *const c_char,
    pub type_: u32,
    pub output_flags: u32,
    pub get_name: Option<unsafe extern "C" fn(*mut c_void) -> *const c_char>,
    pub create: Option<unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void>,
    pub destroy: Option<unsafe extern "C" fn(*mut c_void)>,
    pub get_width: Option<unsafe extern "C" fn(*mut c_void) -> u32>,
    pub get_height: Option<unsafe extern "C" fn(*mut c_void) -> u32>,
    pub defaults: Option<unsafe extern "C" fn(*mut c_void)>,
    pub get_properties: Option<unsafe extern "C" fn(*mut c_void) -> *mut c_void>,
    pub update: Option<unsafe extern "C" fn(*mut c_void, *mut c_void)>,
    pub activate: Option<unsafe extern "C" fn(*mut c_void)>,
    pub deactivate: Option<unsafe extern "C" fn(*mut c_void)>,
    pub show: Option<unsafe extern "C" fn(*mut c_void)>,
    pub hide: Option<unsafe extern "C" fn(*mut c_void)>,
    pub video_tick: Option<unsafe extern "C" fn(*mut c_void, f32)>,
    pub video_render: Option<unsafe extern "C" fn(*mut c_void, *mut c_void)>,
}

unsafe impl Sync for ObsSourceInfo {}
unsafe impl Send for ObsSourceInfo {}

static TICK_COUNT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static RENDER_COUNT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static LAST_PREVIEW_TIME: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static LAST_TICK_SECONDS: std::sync::atomic::AtomicU32 =
    std::sync::atomic::AtomicU32::new(0x3c888889); // ~0.016666668f32 (1/60th second default)
static RENDER_ACTIVE_FLAG: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn notify_preview_active() {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    LAST_PREVIEW_TIME.store(now, std::sync::atomic::Ordering::Relaxed);
}

pub fn is_preview_active() -> bool {
    true
}

type LuaRenderCb = unsafe extern "C" fn(data: *mut c_void, effect: *mut c_void);
static LUA_RENDER_CB: parking_lot::RwLock<Option<LuaRenderCb>> = parking_lot::RwLock::new(None);

#[unsafe(no_mangle)]
pub extern "C" fn trust_set_source_render_callback(cb: LuaRenderCb) {
    *LUA_RENDER_CB.write() = Some(cb);
    println!("[Trust Source] 🔗 Connected Lua/Native render callback to OBS Source!");
}

unsafe extern "C" fn source_get_name(_data: *mut c_void) -> *const c_char {
    SOURCE_NAME.as_ptr() as *const c_char
}

unsafe extern "C" fn source_create(_settings: *mut c_void, source: *mut c_void) -> *mut c_void {
    println!("[Trust Source] 🚀 Custom OBS Source instance created!");
    // Redeem sounds play as this source's audio (it's registered with OBS_SOURCE_AUDIO)
    crate::sound::layer_created(source);
    1 as *mut c_void // Non-null dummy context pointer
}

#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct ObsVideoInfo {
    pub graphics_module: *const c_char,
    pub fps_num: u32,
    pub fps_den: u32,
    pub base_width: u32,
    pub base_height: u32,
    pub output_width: u32,
    pub output_height: u32,
    pub output_format: u32,
    pub adapter: u32,
    pub gpu_conversion: bool,
    pub colorspace: u32,
    pub range: u32,
    pub scale_type: u32,
}

type ObsGetVideoInfoFn = unsafe extern "C" fn(info: *mut ObsVideoInfo) -> bool;
static CANVAS_RESOLUTION: parking_lot::RwLock<(u32, u32)> = parking_lot::RwLock::new((1920, 1080));

pub fn set_canvas_resolution(w: u32, h: u32) {
    if w > 0 && h > 0 {
        *CANVAS_RESOLUTION.write() = (w, h);
    }
}

pub fn get_obs_canvas_resolution() -> (u32, u32) {
    let res = *CANVAS_RESOLUTION.read();
    if res.0 > 0 && res.1 > 0 && (res.0 != 1920 || res.1 != 1080) {
        return res;
    }

    let mut handle = unsafe { dlopen(b"libobs.so.0\0".as_ptr() as *const c_char, 1) };
    if handle.is_null() {
        handle = unsafe { dlopen(b"libobs.so\0".as_ptr() as *const c_char, 1) };
    }
    if !handle.is_null() {
        let sym_init = unsafe { dlsym(handle, b"obs_initialized\0".as_ptr() as *const c_char) };
        if !sym_init.is_null() {
            let is_init: unsafe extern "C" fn() -> bool = unsafe { std::mem::transmute(sym_init) };
            if unsafe { is_init() } {
                let sym_info = unsafe { dlsym(handle, b"obs_get_video_info\0".as_ptr() as *const c_char) };
                if !sym_info.is_null() {
                    let get_info: ObsGetVideoInfoFn = unsafe { std::mem::transmute(sym_info) };
                    let mut ovi = ObsVideoInfo::default();
                    if unsafe { get_info(&mut ovi) } && ovi.base_width > 0 && ovi.base_height > 0 {
                        *CANVAS_RESOLUTION.write() = (ovi.base_width, ovi.base_height);
                        return (ovi.base_width, ovi.base_height);
                    }
                }
            }
        }
    }
    res
}

struct ObsGraphicsFns {
    gs_texrender_create: unsafe extern "C" fn(format: u32, zsformat: u32) -> *mut c_void,
    gs_texrender_destroy: unsafe extern "C" fn(texrender: *mut c_void),
    gs_texrender_begin: unsafe extern "C" fn(texrender: *mut c_void, cx: u32, cy: u32) -> bool,
    gs_texrender_end: unsafe extern "C" fn(texrender: *mut c_void),
    gs_texrender_reset: unsafe extern "C" fn(texrender: *mut c_void),
    gs_texrender_get_texture: unsafe extern "C" fn(texrender: *const c_void) -> *mut c_void,
    gs_stagesurface_create: unsafe extern "C" fn(width: u32, height: u32, color_format: u32) -> *mut c_void,
    gs_stagesurface_destroy: unsafe extern "C" fn(stagesurf: *mut c_void),
    gs_stage_texture: unsafe extern "C" fn(dst: *mut c_void, src: *mut c_void),
    gs_stagesurface_map: unsafe extern "C" fn(stagesurf: *mut c_void, data: *mut *mut u8, linesize: *mut u32) -> bool,
    gs_stagesurface_unmap: unsafe extern "C" fn(stagesurf: *mut c_void),
    obs_source_draw: unsafe extern "C" fn(image: *mut c_void, x: i32, y: i32, cx: u32, cy: u32, flip: bool),
    obs_get_base_effect: unsafe extern "C" fn(effect: u32) -> *mut c_void,
    gs_effect_loop: unsafe extern "C" fn(effect: *mut c_void, name: *const c_char) -> bool,
    gs_blend_state_push: unsafe extern "C" fn(),
    gs_blend_state_pop: unsafe extern "C" fn(),
    gs_reset_blend_state: Option<unsafe extern "C" fn()>,
    gs_blend_function: Option<unsafe extern "C" fn(src: u32, dst: u32)>,
    gs_clear: unsafe extern "C" fn(clear_flags: u32, color: *const f32, depth: f32, stencil: u8),
    obs_source_video_render: unsafe extern "C" fn(source: *mut c_void),
    gs_ortho: unsafe extern "C" fn(left: f32, right: f32, top: f32, bottom: f32, z_near: f32, z_far: f32),
    gs_projection_push: unsafe extern "C" fn(),
    gs_projection_pop: unsafe extern "C" fn(),
    gs_matrix_push: unsafe extern "C" fn(),
    gs_matrix_pop: unsafe extern "C" fn(),
    gs_matrix_identity: unsafe extern "C" fn(),
    gs_matrix_mul: unsafe extern "C" fn(matrix: *const c_void),
    gs_matrix_translate3f: Option<unsafe extern "C" fn(x: f32, y: f32, z: f32)>,
    gs_matrix_scale3f: Option<unsafe extern "C" fn(x: f32, y: f32, z: f32)>,
    gs_matrix_rotaa4f: Option<unsafe extern "C" fn(x: f32, y: f32, z: f32, rad: f32)>,
    gs_enable_blending: unsafe extern "C" fn(enable: bool),
    obs_get_source_by_name: unsafe extern "C" fn(name: *const c_char) -> *mut c_void,
    obs_source_release: unsafe extern "C" fn(source: *mut c_void),
    obs_source_get_width: unsafe extern "C" fn(source: *mut c_void) -> u32,
    obs_source_get_height: unsafe extern "C" fn(source: *mut c_void) -> u32,
    obs_source_create_private: Option<unsafe extern "C" fn(id: *const c_char, name: *const c_char, settings: *mut c_void) -> *mut c_void>,
    obs_data_create: Option<unsafe extern "C" fn() -> *mut c_void>,
    obs_data_release: Option<unsafe extern "C" fn(data: *mut c_void)>,
    obs_data_set_int: Option<unsafe extern "C" fn(data: *mut c_void, name: *const c_char, val: i64)>,
    gs_enable_framebuffer_srgb: Option<unsafe extern "C" fn(enable: bool)>,
    gs_set_linear_srgb: Option<unsafe extern "C" fn(enable: bool) -> bool>,
    gs_effect_get_param_by_name: Option<unsafe extern "C" fn(effect: *mut c_void, name: *const c_char) -> *mut c_void>,
    gs_effect_set_color: Option<unsafe extern "C" fn(param: *mut c_void, argb: u32)>,
    gs_draw_sprite: Option<unsafe extern "C" fn(tex: *mut c_void, flip: u32, width: u32, height: u32)>,
    gs_texture_create: Option<unsafe extern "C" fn(width: u32, height: u32, color_format: u32, levels: u32, data: *const *const u8, flags: u32) -> *mut c_void>,
    gs_texture_destroy: Option<unsafe extern "C" fn(tex: *mut c_void)>,
    gs_effect_create: Option<unsafe extern "C" fn(effect_string: *const c_char, filename: *const c_char, error_string: *mut *mut c_char) -> *mut c_void>,
    gs_effect_destroy: Option<unsafe extern "C" fn(effect: *mut c_void)>,
    gs_effect_set_float: Option<unsafe extern "C" fn(param: *mut c_void, val: f32)>,
    gs_effect_get_num_params: Option<unsafe extern "C" fn(effect: *mut c_void) -> usize>,
    gs_effect_get_param_by_idx: Option<unsafe extern "C" fn(effect: *mut c_void, param: usize) -> *mut c_void>,
    gs_effect_get_param_info: Option<unsafe extern "C" fn(param: *mut c_void, info: *mut GsEffectParamInfo)>,
    gs_effect_set_val: Option<unsafe extern "C" fn(param: *mut c_void, val: *const c_void, size: usize)>,
    gs_get_render_target: Option<unsafe extern "C" fn() -> *mut c_void>,
    gs_copy_texture: Option<unsafe extern "C" fn(dst: *mut c_void, src: *mut c_void)>,
    gs_effect_set_texture: Option<unsafe extern "C" fn(param: *mut c_void, tex: *mut c_void)>,
    obs_get_main_texture: Option<unsafe extern "C" fn() -> *mut c_void>,
}

#[repr(C, align(16))]
pub struct GsMatrix4 {
    pub x: [f32; 4],
    pub y: [f32; 4],
    pub z: [f32; 4],
    pub t: [f32; 4],
}

/// libobs' struct gs_effect_param_info
#[repr(C)]
pub struct GsEffectParamInfo {
    pub name: *const c_char,
    /// enum gs_shader_param_type
    pub kind: i32,
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn trust_apply_target_perspective(d: f32, cx: f32, cy: f32) -> bool {
    if d.abs() < 1e-4 {
        return false;
    }
    if GRAPHICS_FNS.read().is_none() {
        if let Some(fns) = unsafe { init_graphics_fns() } {
            *GRAPHICS_FNS.write() = Some(fns);
        }
    }
    let fns_opt = GRAPHICS_FNS.read();
    if let Some(ref fns) = *fns_opt {
        let mat = GsMatrix4 {
            x: [1.0, 0.0, 0.0, 0.0],
            y: [0.0, 1.0, 0.0, 0.0],
            z: [cx / d, cy / d, 1.0, 1.0 / d],
            t: [cx, cy, 0.0, 1.0],
        };
        unsafe {
            (fns.gs_matrix_mul)(&mat as *const GsMatrix4 as *const c_void);
        }
        true
    } else {
        false
    }
}

static GRAPHICS_FNS: parking_lot::RwLock<Option<ObsGraphicsFns>> = parking_lot::RwLock::new(None);
static TEXRENDER: parking_lot::Mutex<Option<usize>> = parking_lot::Mutex::new(None);
/// The combined render (texrender, canvas size it was made for), drawn and
/// shown in the same frame. It used to go through a delay ring meant to show
/// it N frames late; N was always 0 and nothing set it.
static COMPOSITE_TEXRENDER: parking_lot::Mutex<(usize, (u32, u32))> = parking_lot::Mutex::new((0, (0, 0)));
/// How many staging surfaces a read-back (the Studio preview's, each target
/// scan's) goes through in turn: each frame's picture is staged into one and
/// the oldest is read back, STAGING_DEPTH - 1 frames after it was staged.
/// Reading back the surface staged the same frame made OBS's render thread
/// wait for the GPU to finish the whole frame, every frame. Reading last
/// frame's still made it wait whenever the GPU ran more than a frame behind
/// (another program using it): up to 15 ms a frame, and OBS dropped to ~17
/// fps. Three frames back (100 ms at 30 fps) the GPU is nearly always done.
const STAGING_DEPTH: usize = 4;

/// Staging surfaces a picture is read back through (see STAGING_DEPTH).
struct StagingRing {
    surfs: [usize; STAGING_DEPTH],
    /// The frame (this ring's count) each surface's picture was staged on;
    /// None: nothing waiting in it
    staged_at: [Option<u64>; STAGING_DEPTH],
    /// Frames counted so far: turns, and frames skipped (skip)
    frames: u64,
    dims: (u32, u32),
}

impl StagingRing {
    const EMPTY: Self = Self { surfs: [0; STAGING_DEPTH], staged_at: [None; STAGING_DEPTH], frames: 0, dims: (0, 0) };

    /// This frame's (surface to stage the picture into, the newest picture
    /// staged at least STAGING_DEPTH - 1 frames ago to read back, if any),
    /// (re)made for a (w × h) picture. Staged every frame, that's the one
    /// staged STAGING_DEPTH - 1 frames ago; every few frames (skip), the last
    /// one old enough, so the picture is never much older than that.
    unsafe fn turn(&mut self, fns: &ObsGraphicsFns, w: u32, h: u32) -> Option<(*mut c_void, Option<*mut c_void>)> {
        if self.dims != (w, h) || self.surfs.contains(&0) {
            unsafe { self.destroy(fns) };
            for surf in &mut self.surfs {
                let ptr = unsafe { (fns.gs_stagesurface_create)(w, h, 3) }; // GS_RGBA = 3
                if ptr.is_null() {
                    return None;
                }
                *surf = ptr as usize;
            }
            self.dims = (w, h);
        }
        let (stage, read) = staging_turn(&mut self.staged_at, self.frames);
        self.frames += 1;
        Some((self.surfs[stage] as *mut c_void, read.map(|i| self.surfs[i] as *mut c_void)))
    }

    /// A frame this ring's picture isn't staged on (the scan runs every few frames).
    fn skip(&mut self) {
        self.frames += 1;
    }

    unsafe fn destroy(&mut self, fns: &ObsGraphicsFns) {
        for surf in &mut self.surfs {
            if *surf != 0 {
                unsafe { (fns.gs_stagesurface_destroy)(*surf as *mut c_void) };
                *surf = 0;
            }
        }
        self.staged_at = [None; STAGING_DEPTH];
        self.dims = (0, 0);
    }
}

/// For a StagingRing whose surfaces were staged on `staged_at` (None: free),
/// on frame `now`: (the surface to stage into, the one to read back). The
/// read is the newest staged at least STAGING_DEPTH - 1 frames ago (older
/// ones are dropped: stale); the stage is a free surface other than it, or
/// else the oldest one still waiting.
fn staging_turn(staged_at: &mut [Option<u64>; STAGING_DEPTH], now: u64) -> (usize, Option<usize>) {
    let old_enough = |at: u64| now.saturating_sub(at) >= STAGING_DEPTH as u64 - 1;
    let read = (0..STAGING_DEPTH).filter(|&i| staged_at[i].is_some_and(old_enough)).max_by_key(|&i| staged_at[i]);
    // Read back now, and older ones never will be: their surfaces are free
    if let Some(r) = read {
        let newest = staged_at[r];
        for at in staged_at.iter_mut() {
            if at.is_some() && *at <= newest {
                *at = None;
            }
        }
    }
    let stage = (0..STAGING_DEPTH)
        .filter(|&i| Some(i) != read)
        .min_by_key(|&i| staged_at[i].map_or(0, |at| at + 1))
        .unwrap_or(0);
    staged_at[stage] = Some(now);
    (stage, read)
}

/// The Studio preview's copy of the combined render.
static PREVIEW_STAGING: parking_lot::Mutex<StagingRing> = parking_lot::Mutex::new(StagingRing::EMPTY);

/// Where the render thread copies a read-back frame before handing it to the
/// Studio (swapped, not copied, into studio_composite_frame).
static PREVIEW_BACK_BUFFER: parking_lot::Mutex<Vec<u8>> = parking_lot::Mutex::new(Vec::new());

/// Largest size the Studio preview's copy is made at. The preview panel shows
/// it scaled down anyway, and a full 1440p canvas is ~15 MB a frame to read
/// back, copy, convert and upload again.
/// Its width (⚡ Performance), the height following in 16:9.
static PREVIEW_MAX_WIDTH: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1280);
/// The Studio preview's copy is made one frame in this many.
static PREVIEW_EVERY: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// The Studio preview's sizes (the widest): (name, width).
pub const PREVIEW_SIZES: [(&str, u32); 3] = [("1280 × 720", 1280), ("960 × 540", 960), ("640 × 360", 640)];
/// How often it's copied: (name, one frame in).
pub const PREVIEW_RATES: [(&str, u64); 2] = [("Every frame", 1), ("Every 2nd frame", 2)];

pub fn studio_preview() -> (u32, u64) {
    (
        PREVIEW_MAX_WIDTH.load(std::sync::atomic::Ordering::Relaxed),
        PREVIEW_EVERY.load(std::sync::atomic::Ordering::Relaxed),
    )
}

pub fn set_studio_preview(width: u32, every: u64) {
    let (width, every) = (width.clamp(320, 3840), every.clamp(1, 4));
    PREVIEW_MAX_WIDTH.store(width, std::sync::atomic::Ordering::Relaxed);
    PREVIEW_EVERY.store(every, std::sync::atomic::Ordering::Relaxed);
    save_setting("studio_preview", serde_json::json!({ "width": width, "every": every }));
}

/// The preview copy's size for a canvas: the canvas scaled down (never up) to
/// fit the chosen size, same shape.
fn preview_size(cw: u32, ch: u32) -> (u32, u32) {
    if cw == 0 || ch == 0 {
        return (cw, ch);
    }
    let max_w = PREVIEW_MAX_WIDTH.load(std::sync::atomic::Ordering::Relaxed);
    let max_h = max_w * 9 / 16;
    let scale = (max_w as f32 / cw as f32)
        .min(max_h as f32 / ch as f32)
        .min(1.0);
    (((cw as f32 * scale).round() as u32).max(1), ((ch as f32 * scale).round() as u32).max(1))
}

/// Where the combined render is drawn at the preview's size before staging.
static PREVIEW_SCALE_TEXRENDER: parking_lot::Mutex<usize> = parking_lot::Mutex::new(0);

/// `tex` (the combined render, canvas-sized) drawn at the preview's size, on
/// the GPU; `tex` itself when that's already its size. Null if it failed.
unsafe fn scale_for_preview(fns: &ObsGraphicsFns, tex: *mut c_void, cw: u32, ch: u32, pw: u32, ph: u32) -> *mut c_void {
    if (pw, ph) == (cw, ch) {
        return tex;
    }
    let mut tr = PREVIEW_SCALE_TEXRENDER.lock();
    if *tr == 0 {
        let ptr = unsafe { (fns.gs_texrender_create)(3, 0) }; // GS_RGBA, GS_ZS_NONE
        if ptr.is_null() {
            return std::ptr::null_mut();
        }
        *tr = ptr as usize;
    }
    let texrender = *tr as *mut c_void;
    unsafe {
        (fns.gs_texrender_reset)(texrender);
        if !(fns.gs_texrender_begin)(texrender, pw, ph) {
            return std::ptr::null_mut();
        }
        let clear_color = [0.0f32, 0.0, 0.0, 0.0];
        (fns.gs_clear)(1, clear_color.as_ptr(), 0.0, 0);
        (fns.gs_projection_push)();
        (fns.gs_ortho)(0.0, cw as f32, 0.0, ch as f32, -100.0, 100.0);
        (fns.gs_matrix_push)();
        (fns.gs_matrix_identity)();
        (fns.gs_blend_state_push)();
        (fns.gs_enable_blending)(false);
        let effect = (fns.obs_get_base_effect)(0); // OBS_EFFECT_DEFAULT
        if !effect.is_null() {
            while (fns.gs_effect_loop)(effect, b"Draw\0".as_ptr() as *const c_char) {
                (fns.obs_source_draw)(tex, 0, 0, cw, ch, false);
            }
        }
        (fns.gs_blend_state_pop)();
        (fns.gs_matrix_pop)();
        (fns.gs_projection_pop)();
        (fns.gs_texrender_end)(texrender);
        (fns.gs_texrender_get_texture)(texrender)
    }
}

/// Where OBS's frame time goes in Theater's renderer, logged to
/// trust_debug.log every 10 s: the Throwables Layer's whole render (it draws
/// every scene item again), the target scans within it (GPU read-back and
/// mask scan of each target source), and the Studio preview's copy.
struct RenderTimes {
    since: std::time::Instant,
    frames: u32,
    total: std::time::Duration,
    worst: std::time::Duration,
    scans: std::time::Duration,
    scan_count: u32,
    /// Of the scans: waiting for the GPU to hand back the target's pixels.
    scan_readback: std::time::Duration,
    preview: std::time::Duration,
    /// Target pictures scanned, and how many differed from that target's
    /// last one: an animated avatar whose source delivers fewer new frames
    /// (its app throttled while minimized) shows few new ones while the
    /// layer still draws every frame.
    pictures: u32,
    new_pictures: u32,
    /// The most frames in a row a target showed the same picture: even with
    /// as many new ones, a source delivering them unevenly (two, a pause,
    /// one late) shows as runs of repeats — the avatar judders.
    longest_repeat: u32,
}

impl RenderTimes {
    fn new() -> Self {
        Self {
            since: std::time::Instant::now(),
            frames: 0,
            total: std::time::Duration::ZERO,
            worst: std::time::Duration::ZERO,
            scans: std::time::Duration::ZERO,
            scan_count: 0,
            scan_readback: std::time::Duration::ZERO,
            preview: std::time::Duration::ZERO,
            pictures: 0,
            new_pictures: 0,
            longest_repeat: 0,
        }
    }
}

static RENDER_TIMES: parking_lot::Mutex<Option<RenderTimes>> = parking_lot::Mutex::new(None);

/// The last 10 s of RenderTimes, for the Studio's Settings tab.
#[derive(Debug, Clone, Copy)]
pub struct RenderSummary {
    /// Theater's whole render, per frame
    pub render_ms: f64,
    /// Of it, the target scans, per frame (frames without one count too)
    pub scan_ms: f64,
    pub scans_per_frame: f64,
    pub fps: f64,
    pub at: std::time::Instant,
}

static LAST_RENDER_SUMMARY: parking_lot::Mutex<Option<RenderSummary>> = parking_lot::Mutex::new(None);

/// Theater's render cost over its last 10 s, if it has rendered that long.
pub fn render_summary() -> Option<RenderSummary> {
    *LAST_RENDER_SUMMARY.lock()
}

/// Notes a target's scanned picture, whether it's a new one (a sample of its
/// bytes, every 61st so all four channels, against the last picture's), and
/// for how many frames in a row it has been the same.
fn note_target_picture(name: &str, rgba: &[u8]) {
    // Per target: its last picture's sample, and frames in a row it's shown
    static LAST: parking_lot::Mutex<Option<std::collections::HashMap<String, (u64, u32)>>> = parking_lot::Mutex::new(None);
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in rgba.iter().step_by(61) {
        hash = (hash ^ b as u64).wrapping_mul(0x0100_0000_01b3);
    }
    let (changed, run) = {
        let mut last = LAST.lock();
        let entry = last
            .get_or_insert_with(std::collections::HashMap::new)
            .entry(name.to_string())
            .or_insert((!hash, 0));
        let changed = entry.0 != hash;
        *entry = (hash, if changed { 1 } else { entry.1 + 1 });
        (changed, entry.1)
    };
    with_render_times(|t| {
        t.pictures += 1;
        t.new_pictures += changed as u32;
        t.longest_repeat = t.longest_repeat.max(run);
    });
}

/// Times the Throwables Layer's render (every way out of it).
struct RenderTimer(std::time::Instant);

impl Drop for RenderTimer {
    fn drop(&mut self) {
        note_render_time(self.0.elapsed());
    }
}

/// Times one target scan.
struct ScanTimer(std::time::Instant);

impl Drop for ScanTimer {
    fn drop(&mut self) {
        let took = self.0.elapsed();
        with_render_times(|t| {
            t.scans += took;
            t.scan_count += 1;
        });
    }
}

fn with_render_times(f: impl FnOnce(&mut RenderTimes)) {
    let mut times = RENDER_TIMES.lock();
    let t = times.get_or_insert_with(RenderTimes::new);
    f(t);
}

fn note_render_time(took: std::time::Duration) {
    let mut report = None;
    with_render_times(|t| {
        t.frames += 1;
        t.total += took;
        t.worst = t.worst.max(took);
        if t.since.elapsed() >= std::time::Duration::from_secs(10) && t.frames > 0 {
            let ms = |d: std::time::Duration| d.as_secs_f64() * 1000.0;
            let n = t.frames as f64;
            *LAST_RENDER_SUMMARY.lock() = Some(RenderSummary {
                render_ms: ms(t.total) / n,
                scan_ms: ms(t.scans) / n,
                scans_per_frame: t.scan_count as f64 / n,
                fps: n / t.since.elapsed().as_secs_f64(),
                at: std::time::Instant::now(),
            });
            report = Some(format!(
                "[Trust Render] {} frames in {:.1}s: render avg {:.2} ms (worst {:.2}), target scans avg {:.2} ms ({:.1}/frame, GPU read-back wait {:.2} ms of it), target pictures {} new of {} (longest same {} frames), Studio preview copy avg {:.2} ms, Studio {}",
                t.frames,
                t.since.elapsed().as_secs_f64(),
                ms(t.total) / n,
                ms(t.worst),
                ms(t.scans) / n,
                t.scan_count as f64 / n,
                ms(t.scan_readback) / n,
                t.new_pictures,
                t.pictures,
                t.longest_repeat,
                ms(t.preview) / n,
                if crate::studio_app::is_studio_native_open() { "open" } else { "closed" },
            ));
            *t = RenderTimes::new();
        }
    });
    if let Some(line) = report {
        trust_log(&line);
    }
}

/// This frame's (surface to stage into, the oldest staged one to read back,
/// if any), (re)made for the canvas size.
unsafe fn preview_staging_turn(fns: &ObsGraphicsFns, cw: u32, ch: u32) -> Option<(*mut c_void, Option<*mut c_void>)> {
    unsafe { PREVIEW_STAGING.lock().turn(fns, cw, ch) }
}

fn destroy_preview_staging(fns: &ObsGraphicsFns) {
    unsafe { PREVIEW_STAGING.lock().destroy(fns) };
}

/// Reads back a staged frame and hands it to the Studio's preview. Never
/// waits for the Studio: while it is still turning the last frame into its
/// texture, this frame is simply not handed over. (Waiting here held OBS's
/// graphics lock for as long as the Studio took.)
unsafe fn publish_preview_frame(fns: &ObsGraphicsFns, surf: *mut c_void, cw: u32, ch: u32, canvas: (u32, u32)) {
    let mut data_ptr: *mut u8 = std::ptr::null_mut();
    let mut linesize: u32 = 0;
    if !unsafe { (fns.gs_stagesurface_map)(surf, &mut data_ptr, &mut linesize) } {
        return;
    }
    let (w, h) = (cw as usize, ch as usize);
    let row = w * 4;
    let mut back = PREVIEW_BACK_BUFFER.lock();
    let copied = !data_ptr.is_null() && linesize as usize >= row;
    if copied {
        let stride = linesize as usize;
        back.resize(row * h, 0);
        if stride == row {
            back.copy_from_slice(unsafe { std::slice::from_raw_parts(data_ptr, row * h) });
        } else {
            for y in 0..h {
                let src = unsafe { std::slice::from_raw_parts(data_ptr.add(y * stride), row) };
                back[y * row..(y + 1) * row].copy_from_slice(src);
            }
        }
    }
    unsafe { (fns.gs_stagesurface_unmap)(surf) };
    if !copied {
        return;
    }
    let Some(mut comp_lock) = crate::state::get_state().studio_composite_frame.try_write() else { return };
    let comp = comp_lock.get_or_insert_with(|| crate::state::TargetPreviewTexture {
        width: cw,
        height: ch,
        rgba: Vec::new(),
        frame_id: 0,
        base_width: canvas.0,
        base_height: canvas.1,
    });
    comp.width = cw;
    comp.height = ch;
    comp.base_width = canvas.0;
    comp.base_height = canvas.1;
    std::mem::swap(&mut comp.rgba, &mut *back);
    comp.frame_id = comp.frame_id.wrapping_add(1);
}
static ATTACHED_GUARD_FILTERS: parking_lot::Mutex<Option<std::collections::HashMap<usize, usize>>> =
    parking_lot::Mutex::new(None);

unsafe fn init_graphics_fns() -> Option<ObsGraphicsFns> {
    let mut handle = unsafe { dlopen(b"libobs.so.0\0".as_ptr() as *const c_char, 1) };
    if handle.is_null() {
        handle = unsafe { dlopen(b"libobs.so\0".as_ptr() as *const c_char, 1) };
    }
    if handle.is_null() {
        return None;
    }

    macro_rules! load_sym {
        ($name:literal) => {{
            let sym = unsafe { dlsym(handle, concat!($name, "\0").as_ptr() as *const c_char) };
            if sym.is_null() { return None; }
            unsafe { std::mem::transmute(sym) }
        }};
    }

    macro_rules! opt_sym {
        ($name:literal) => {{
            let sym = unsafe { dlsym(handle, concat!($name, "\0").as_ptr() as *const c_char) };
            if !sym.is_null() { Some(unsafe { std::mem::transmute(sym) }) } else { None }
        }};
    }

    Some(ObsGraphicsFns {
        gs_texrender_create: load_sym!("gs_texrender_create"),
        gs_texrender_destroy: load_sym!("gs_texrender_destroy"),
        gs_texrender_begin: load_sym!("gs_texrender_begin"),
        gs_texrender_end: load_sym!("gs_texrender_end"),
        gs_texrender_reset: load_sym!("gs_texrender_reset"),
        gs_texrender_get_texture: load_sym!("gs_texrender_get_texture"),
        gs_stagesurface_create: load_sym!("gs_stagesurface_create"),
        gs_stagesurface_destroy: load_sym!("gs_stagesurface_destroy"),
        gs_stage_texture: load_sym!("gs_stage_texture"),
        gs_stagesurface_map: load_sym!("gs_stagesurface_map"),
        gs_stagesurface_unmap: load_sym!("gs_stagesurface_unmap"),
        obs_source_draw: load_sym!("obs_source_draw"),
        obs_get_base_effect: load_sym!("obs_get_base_effect"),
        gs_effect_loop: load_sym!("gs_effect_loop"),
        gs_blend_state_push: load_sym!("gs_blend_state_push"),
        gs_blend_state_pop: load_sym!("gs_blend_state_pop"),
        gs_reset_blend_state: opt_sym!("gs_reset_blend_state"),
        gs_blend_function: opt_sym!("gs_blend_function"),
        gs_clear: load_sym!("gs_clear"),
        obs_source_video_render: load_sym!("obs_source_video_render"),
        gs_ortho: load_sym!("gs_ortho"),
        gs_projection_push: load_sym!("gs_projection_push"),
        gs_projection_pop: load_sym!("gs_projection_pop"),
        gs_matrix_push: load_sym!("gs_matrix_push"),
        gs_matrix_pop: load_sym!("gs_matrix_pop"),
        gs_matrix_identity: load_sym!("gs_matrix_identity"),
        gs_matrix_mul: load_sym!("gs_matrix_mul"),
        gs_matrix_translate3f: opt_sym!("gs_matrix_translate3f"),
        gs_matrix_scale3f: opt_sym!("gs_matrix_scale3f"),
        gs_matrix_rotaa4f: opt_sym!("gs_matrix_rotaa4f"),
        gs_enable_blending: load_sym!("gs_enable_blending"),
        obs_get_source_by_name: load_sym!("obs_get_source_by_name"),
        obs_source_release: load_sym!("obs_source_release"),
        obs_source_get_width: load_sym!("obs_source_get_width"),
        obs_source_get_height: load_sym!("obs_source_get_height"),
        obs_source_create_private: opt_sym!("obs_source_create_private"),
        obs_data_create: opt_sym!("obs_data_create"),
        obs_data_release: opt_sym!("obs_data_release"),
        obs_data_set_int: opt_sym!("obs_data_set_int"),
        gs_enable_framebuffer_srgb: opt_sym!("gs_enable_framebuffer_srgb"),
        gs_set_linear_srgb: opt_sym!("gs_set_linear_srgb"),
        gs_effect_get_param_by_name: opt_sym!("gs_effect_get_param_by_name"),
        gs_effect_set_color: opt_sym!("gs_effect_set_color"),
        gs_draw_sprite: opt_sym!("gs_draw_sprite"),
        gs_texture_create: opt_sym!("gs_texture_create"),
        gs_texture_destroy: opt_sym!("gs_texture_destroy"),
        gs_effect_create: opt_sym!("gs_effect_create"),
        gs_effect_destroy: opt_sym!("gs_effect_destroy"),
        gs_effect_set_float: opt_sym!("gs_effect_set_float"),
        gs_effect_get_num_params: opt_sym!("gs_effect_get_num_params"),
        gs_effect_get_param_by_idx: opt_sym!("gs_effect_get_param_by_idx"),
        gs_effect_get_param_info: opt_sym!("gs_effect_get_param_info"),
        gs_effect_set_val: opt_sym!("gs_effect_set_val"),
        gs_get_render_target: opt_sym!("gs_get_render_target"),
        gs_copy_texture: opt_sym!("gs_copy_texture"),
        gs_effect_set_texture: opt_sym!("gs_effect_set_texture"),
        obs_get_main_texture: opt_sym!("obs_get_main_texture"),
    })
}

unsafe extern "C" fn source_destroy(_data: *mut c_void) {
    println!("[Trust Source] 🛑 Source destroyed in OBS.");
    unsafe {
        if let Some(ref fns) = *GRAPHICS_FNS.read() {
            if let Some(tr) = TEXRENDER.lock().take() {
                (fns.gs_texrender_destroy)(tr as *mut c_void);
            }
            {
                let mut composite = COMPOSITE_TEXRENDER.lock();
                if composite.0 != 0 {
                    (fns.gs_texrender_destroy)(composite.0 as *mut c_void);
                }
                *composite = (0, (0, 0));
            }
            destroy_target_staging(fns);
            destroy_target_scan_texrenders(fns);
            if let Some(tr) = REACTION_TEXRENDER.lock().take() {
                (fns.gs_texrender_destroy)(tr as *mut c_void);
            }
            if let Some(tr) = REACTION_OUT_TEXRENDER.lock().take() {
                (fns.gs_texrender_destroy)(tr as *mut c_void);
            }
            destroy_preview_staging(fns);
            if let Some(destroy_tex) = fns.gs_texture_destroy {
                if let Some(map) = GLOW_TEX_CACHE.lock().take() {
                    for (_, ptr) in map {
                        destroy_tex(ptr as *mut c_void);
                    }
                }
                #[cfg(target_os = "windows")]
                if let Some(map) = FIRE_GLOW_TEX_CACHE.lock().take() {
                    for (_, ptr) in map {
                        destroy_tex(ptr as *mut c_void);
                    }
                }
            }
        }
        if let Some(mut filters) = ATTACHED_GUARD_FILTERS.lock().take() {
            filters.clear();
        }
        crate::render_gate::forget();
        STUDIO_SCREEN_CANVAS.lock().free();
    }
}

unsafe extern "C" fn source_get_width(_data: *mut c_void) -> u32 {
    get_obs_canvas_resolution().0
}

unsafe extern "C" fn source_get_height(_data: *mut c_void) -> u32 {
    get_obs_canvas_resolution().1
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct ObsVec2 {
    pub x: f32,
    pub y: f32,
}

#[repr(C)]
pub struct ObsSourceArray {
    pub array: *mut *mut c_void,
    pub num: usize,
    pub capacity: usize,
}

#[repr(C)]
pub struct ObsSourceList {
    pub sources: ObsSourceArray,
}

#[derive(Clone, Copy)]
pub struct ObsSceneFns {
    pub obs_get_source_by_name: unsafe extern "C" fn(*const c_char) -> *mut c_void,
    pub obs_scene_from_source: unsafe extern "C" fn(*mut c_void) -> *mut c_void,
    pub obs_scene_find_source_recursive: unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut c_void,
    pub obs_scene_find_source: unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut c_void,
    pub obs_sceneitem_get_source: unsafe extern "C" fn(*mut c_void) -> *mut c_void,
    pub obs_sceneitem_set_pos: unsafe extern "C" fn(*mut c_void, *const ObsVec2),
    pub obs_sceneitem_set_rot: unsafe extern "C" fn(*mut c_void, f32),
    pub obs_sceneitem_set_scale: unsafe extern "C" fn(*mut c_void, *const ObsVec2),
    pub obs_source_get_width: unsafe extern "C" fn(*mut c_void) -> u32,
    pub obs_source_get_height: unsafe extern "C" fn(*mut c_void) -> u32,
    pub obs_source_get_name: unsafe extern "C" fn(*mut c_void) -> *const c_char,
    pub obs_source_release: unsafe extern "C" fn(*mut c_void),
    pub obs_frontend_get_current_scene: Option<unsafe extern "C" fn() -> *mut c_void>,
    pub obs_frontend_set_current_scene: Option<unsafe extern "C" fn(*mut c_void)>,
    pub obs_sceneitem_set_locked: Option<unsafe extern "C" fn(*mut c_void, bool)>,
    pub obs_sceneitem_set_visible: Option<unsafe extern "C" fn(*mut c_void, bool)>,
    pub obs_sceneitem_locked: Option<unsafe extern "C" fn(*mut c_void) -> bool>,
    pub obs_sceneitem_visible: Option<unsafe extern "C" fn(*mut c_void) -> bool>,
    pub obs_frontend_get_scenes: Option<unsafe extern "C" fn(*mut ObsSourceList)>,
    pub bfree: Option<unsafe extern "C" fn(*mut c_void)>,
    pub obs_scene_enum_items: Option<unsafe extern "C" fn(*mut c_void, Option<unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) -> bool>, *mut c_void)>,
    pub obs_sceneitem_get_pos: Option<unsafe extern "C" fn(*mut c_void, *mut ObsVec2)>,
    pub obs_sceneitem_get_scale: Option<unsafe extern "C" fn(*mut c_void, *mut ObsVec2)>,
    pub obs_sceneitem_get_rot: Option<unsafe extern "C" fn(*mut c_void) -> f32>,
    pub obs_source_get_id: Option<unsafe extern "C" fn(*mut c_void) -> *const c_char>,
    pub obs_sceneitem_get_draw_transform: Option<unsafe extern "C" fn(*mut c_void, *mut GsMatrix4)>,
    pub obs_source_get_base_width: Option<unsafe extern "C" fn(*mut c_void) -> u32>,
    pub obs_source_get_base_height: Option<unsafe extern "C" fn(*mut c_void) -> u32>,
    pub obs_source_filter_add: Option<unsafe extern "C" fn(*mut c_void, *mut c_void)>,
    pub obs_source_filter_remove: Option<unsafe extern "C" fn(*mut c_void, *mut c_void)>,
    pub obs_source_filter_count: Option<unsafe extern "C" fn(*mut c_void) -> usize>,
    pub obs_source_get_filter_by_name: Option<unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut c_void>,
    pub obs_source_enum_filters: Option<unsafe extern "C" fn(*mut c_void, Option<unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void)>, *mut c_void)>,
    pub obs_source_get_settings: Option<unsafe extern "C" fn(*mut c_void) -> *mut c_void>,
    pub obs_source_update: Option<unsafe extern "C" fn(*mut c_void, *mut c_void)>,
    pub obs_source_enabled: Option<unsafe extern "C" fn(*mut c_void) -> bool>,
    pub obs_source_set_enabled: Option<unsafe extern "C" fn(*mut c_void, bool)>,
    pub obs_data_get_json: Option<unsafe extern "C" fn(*mut c_void) -> *const c_char>,
    pub obs_data_create_from_json: Option<unsafe extern "C" fn(*const c_char) -> *mut c_void>,
    pub obs_data_release: Option<unsafe extern "C" fn(*mut c_void)>,
    pub obs_source_get_output_flags: Option<unsafe extern "C" fn(*mut c_void) -> u32>,
    pub obs_source_get_volume: Option<unsafe extern "C" fn(*mut c_void) -> f32>,
    pub obs_source_set_volume: Option<unsafe extern "C" fn(*mut c_void, f32)>,
    pub obs_volmeter_create: Option<unsafe extern "C" fn(fader_type: i32) -> *mut c_void>,
    pub obs_volmeter_destroy: Option<unsafe extern "C" fn(*mut c_void)>,
    pub obs_volmeter_attach_source: Option<unsafe extern "C" fn(*mut c_void, *mut c_void) -> bool>,
    pub obs_volmeter_add_callback: Option<unsafe extern "C" fn(*mut c_void, VolmeterCb, *mut c_void)>,
    pub obs_volmeter_remove_callback: Option<unsafe extern "C" fn(*mut c_void, VolmeterCb, *mut c_void)>,
    pub obs_volmeter_get_nr_channels: Option<unsafe extern "C" fn(*mut c_void) -> i32>,
    pub obs_sceneitem_get_id: Option<unsafe extern "C" fn(*mut c_void) -> i64>,
    pub obs_scene_find_sceneitem_by_id: Option<unsafe extern "C" fn(*mut c_void, i64) -> *mut c_void>,
}

/// obs_volmeter_updated_t: per-channel levels in dBFS (arrays of MAX_AUDIO_CHANNELS)
pub type VolmeterCb = unsafe extern "C" fn(param: *mut c_void, magnitude: *const f32, peak: *const f32, input_peak: *const f32);

static SCENE_FNS: parking_lot::RwLock<Option<ObsSceneFns>> = parking_lot::RwLock::new(None);

pub fn init_scene_fns() -> Option<ObsSceneFns> {
    let mut handle = unsafe { dlopen(b"libobs.so.0\0".as_ptr() as *const c_char, 1) };
    if handle.is_null() {
        handle = unsafe { dlopen(b"libobs.so\0".as_ptr() as *const c_char, 1) };
    }
    if handle.is_null() {
        handle = unsafe { dlopen(b"libobs.so.30\0".as_ptr() as *const c_char, 1) };
    }
    if handle.is_null() {
        // Fallback to searching process global symbols (RTLD_DEFAULT)
        handle = std::ptr::null_mut();
    }

    // Verify OBS core is actually running and initialized
    let sym_init = b"obs_initialized\0";
    let init_ptr = unsafe {
        let mut p = if !handle.is_null() { dlsym(handle, sym_init.as_ptr() as *const c_char) } else { std::ptr::null_mut() };
        if p.is_null() { p = dlsym(std::ptr::null_mut(), sym_init.as_ptr() as *const c_char); }
        p
    };
    if !init_ptr.is_null() {
        let obs_init_fn: unsafe extern "C" fn() -> bool = unsafe { std::mem::transmute(init_ptr) };
        if !unsafe { obs_init_fn() } {
            return None;
        }
    } else {
        return None;
    }

    macro_rules! load_sym {
        ($name:literal) => {{
            let mut sym = if !handle.is_null() {
                unsafe { dlsym(handle, concat!($name, "\0").as_ptr() as *const c_char) }
            } else {
                std::ptr::null_mut()
            };
            if sym.is_null() {
                sym = unsafe { dlsym(std::ptr::null_mut(), concat!($name, "\0").as_ptr() as *const c_char) };
            }
            if sym.is_null() {
                eprintln!("[Trust Core] ⚠️ Failed to load symbol: {}", $name);
                return None;
            }
            unsafe { std::mem::transmute(sym) }
        }};
    }

    macro_rules! opt_sym {
        ($name:literal) => {{
            let mut sym = if !handle.is_null() {
                unsafe { dlsym(handle, concat!($name, "\0").as_ptr() as *const c_char) }
            } else {
                std::ptr::null_mut()
            };
            if sym.is_null() {
                sym = unsafe { dlsym(std::ptr::null_mut(), concat!($name, "\0").as_ptr() as *const c_char) };
            }
            if !sym.is_null() { Some(unsafe { std::mem::transmute(sym) }) } else { None }
        }};
    }

    let opt_set_locked: Option<unsafe extern "C" fn(*mut c_void, bool)> = opt_sym!("obs_sceneitem_set_locked");
    let opt_set_vis: Option<unsafe extern "C" fn(*mut c_void, bool)> = opt_sym!("obs_sceneitem_set_visible");
    let opt_locked: Option<unsafe extern "C" fn(*mut c_void) -> bool> = opt_sym!("obs_sceneitem_locked");
    let opt_vis: Option<unsafe extern "C" fn(*mut c_void) -> bool> = opt_sym!("obs_sceneitem_visible");
    let opt_bfree: Option<unsafe extern "C" fn(*mut c_void)> = opt_sym!("bfree");
    let opt_filter_add: Option<unsafe extern "C" fn(*mut c_void, *mut c_void)> = opt_sym!("obs_source_filter_add");
    let opt_filter_remove: Option<unsafe extern "C" fn(*mut c_void, *mut c_void)> = opt_sym!("obs_source_filter_remove");
    let opt_filter_count: Option<unsafe extern "C" fn(*mut c_void) -> usize> = opt_sym!("obs_source_filter_count");
    let opt_get_filter: Option<unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut c_void> = opt_sym!("obs_source_get_filter_by_name");

    let mut handle_frontend = unsafe { dlopen(b"libobs-frontend-api.so.0\0".as_ptr() as *const c_char, 1) };
    if handle_frontend.is_null() {
        handle_frontend = unsafe { dlopen(b"libobs-frontend-api.so\0".as_ptr() as *const c_char, 1) };
    }
    if handle_frontend.is_null() {
        handle_frontend = unsafe { dlopen(b"libobs-frontend-api.so.30\0".as_ptr() as *const c_char, 1) };
    }

    let get_cur_scene: Option<unsafe extern "C" fn() -> *mut c_void> = {
        let mut sym = if !handle_frontend.is_null() {
            unsafe { dlsym(handle_frontend, b"obs_frontend_get_current_scene\0".as_ptr() as *const c_char) }
        } else {
            std::ptr::null_mut()
        };
        if sym.is_null() {
            sym = unsafe { dlsym(std::ptr::null_mut(), b"obs_frontend_get_current_scene\0".as_ptr() as *const c_char) };
        }
        if !sym.is_null() { Some(unsafe { std::mem::transmute(sym) }) } else { None }
    };

    let get_scenes: Option<unsafe extern "C" fn(*mut ObsSourceList)> = {
        let mut sym = if !handle_frontend.is_null() {
            unsafe { dlsym(handle_frontend, b"obs_frontend_get_scenes\0".as_ptr() as *const c_char) }
        } else {
            std::ptr::null_mut()
        };
        if sym.is_null() {
            sym = unsafe { dlsym(std::ptr::null_mut(), b"obs_frontend_get_scenes\0".as_ptr() as *const c_char) };
        }
        if !sym.is_null() { Some(unsafe { std::mem::transmute(sym) }) } else { None }
    };

    let set_cur_scene: Option<unsafe extern "C" fn(*mut c_void)> = {
        let mut sym = if !handle_frontend.is_null() {
            unsafe { dlsym(handle_frontend, b"obs_frontend_set_current_scene\0".as_ptr() as *const c_char) }
        } else {
            std::ptr::null_mut()
        };
        if sym.is_null() {
            sym = unsafe { dlsym(std::ptr::null_mut(), b"obs_frontend_set_current_scene\0".as_ptr() as *const c_char) };
        }
        if !sym.is_null() { Some(unsafe { std::mem::transmute(sym) }) } else { None }
    };

    let fns = ObsSceneFns {
        obs_get_source_by_name: load_sym!("obs_get_source_by_name"),
        obs_scene_from_source: load_sym!("obs_scene_from_source"),
        obs_scene_find_source_recursive: load_sym!("obs_scene_find_source_recursive"),
        obs_scene_find_source: load_sym!("obs_scene_find_source"),
        obs_sceneitem_get_source: load_sym!("obs_sceneitem_get_source"),
        obs_sceneitem_set_pos: load_sym!("obs_sceneitem_set_pos"),
        obs_sceneitem_set_rot: load_sym!("obs_sceneitem_set_rot"),
        obs_sceneitem_set_scale: load_sym!("obs_sceneitem_set_scale"),
        obs_source_get_width: load_sym!("obs_source_get_width"),
        obs_source_get_height: load_sym!("obs_source_get_height"),
        obs_source_get_name: load_sym!("obs_source_get_name"),
        obs_source_release: load_sym!("obs_source_release"),
        obs_frontend_get_current_scene: get_cur_scene,
        obs_frontend_set_current_scene: set_cur_scene,
        obs_sceneitem_set_locked: opt_set_locked,
        obs_sceneitem_set_visible: opt_set_vis,
        obs_sceneitem_locked: opt_locked,
        obs_sceneitem_visible: opt_vis,
        obs_frontend_get_scenes: get_scenes,
        bfree: opt_bfree,
        obs_scene_enum_items: opt_sym!("obs_scene_enum_items"),
        obs_sceneitem_get_pos: opt_sym!("obs_sceneitem_get_pos"),
        obs_sceneitem_get_scale: opt_sym!("obs_sceneitem_get_scale"),
        obs_sceneitem_get_rot: opt_sym!("obs_sceneitem_get_rot"),
        obs_source_get_id: opt_sym!("obs_source_get_id"),
        obs_sceneitem_get_draw_transform: opt_sym!("obs_sceneitem_get_draw_transform"),
        obs_source_get_base_width: opt_sym!("obs_source_get_base_width"),
        obs_source_get_base_height: opt_sym!("obs_source_get_base_height"),
        obs_source_filter_add: opt_filter_add,
        obs_source_filter_remove: opt_filter_remove,
        obs_source_filter_count: opt_filter_count,
        obs_source_get_filter_by_name: opt_get_filter,
        obs_source_enum_filters: opt_sym!("obs_source_enum_filters"),
        obs_source_get_settings: opt_sym!("obs_source_get_settings"),
        obs_source_update: opt_sym!("obs_source_update"),
        obs_source_enabled: opt_sym!("obs_source_enabled"),
        obs_source_set_enabled: opt_sym!("obs_source_set_enabled"),
        obs_data_get_json: opt_sym!("obs_data_get_json"),
        obs_data_create_from_json: opt_sym!("obs_data_create_from_json"),
        obs_data_release: opt_sym!("obs_data_release"),
        obs_source_get_output_flags: opt_sym!("obs_source_get_output_flags"),
        obs_source_get_volume: opt_sym!("obs_source_get_volume"),
        obs_source_set_volume: opt_sym!("obs_source_set_volume"),
        obs_volmeter_create: opt_sym!("obs_volmeter_create"),
        obs_volmeter_destroy: opt_sym!("obs_volmeter_destroy"),
        obs_volmeter_attach_source: opt_sym!("obs_volmeter_attach_source"),
        obs_volmeter_add_callback: opt_sym!("obs_volmeter_add_callback"),
        obs_volmeter_remove_callback: opt_sym!("obs_volmeter_remove_callback"),
        obs_volmeter_get_nr_channels: opt_sym!("obs_volmeter_get_nr_channels"),
        obs_sceneitem_get_id: opt_sym!("obs_sceneitem_get_id"),
        obs_scene_find_sceneitem_by_id: opt_sym!("obs_scene_find_sceneitem_by_id"),
    };
    // Stored once: the table never changes. Taking the write lock on every
    // call (the Studio calls this every 300 ms) froze OBS: parking_lot queues
    // new readers behind a waiting writer, so when it landed while the render
    // thread held a read and was about to take a nested one, the two threads
    // waited on each other, and OBS's UI then waited on the render thread.
    if SCENE_FNS.read().is_none() {
        *SCENE_FNS.write() = Some(fns);
    }
    Some(fns)
}

pub fn get_or_init_scene_fns() -> Option<ObsSceneFns> {
    if let Some(f) = *SCENE_FNS.read() {
        return Some(f);
    }
    init_scene_fns()
}

fn find_scene_item(fns: &ObsSceneFns, scene_name: &str, source_name: &str) -> (*mut c_void, *mut c_void) {
    // `source_name` is a Studio name: resolve it to that exact scene item by id
    // (so an extra copy like "Elgato #2" gets its own item, not the first one
    // with that source), falling back to a source-name search.
    let obs_name = studio_key_source_name(source_name);
    let is_extra_copy = obs_name != source_name;
    let c_name = match std::ffi::CString::new(obs_name.as_str()) {
        Ok(s) => s,
        Err(_) => return (std::ptr::null_mut(), std::ptr::null_mut()),
    };

    let check_scene_src = |sc_source: *mut c_void, sc_name: &str| -> (*mut c_void, *mut c_void) {
        if sc_source.is_null() { return (std::ptr::null_mut(), std::ptr::null_mut()); }
        let scene = unsafe { (fns.obs_scene_from_source)(sc_source) };
        if !scene.is_null() {
            if let Some(find_by_id) = fns.obs_scene_find_sceneitem_by_id {
                let lookup = || {
                    SCENE_ITEM_KEYS.read().get(sc_name).and_then(|keys| keys.by_key.get(source_name).map(|(_, id)| *id))
                };
                let mut item_id = lookup();
                if item_id.is_none() && is_extra_copy {
                    refresh_scene_item_keys(fns, sc_name, scene);
                    item_id = lookup();
                }
                if let Some(id) = item_id {
                    let item = unsafe { find_by_id(scene, id) };
                    if !item.is_null() {
                        return (sc_source, item);
                    }
                }
                if is_extra_copy {
                    // Never fall back to the first copy for an extra copy's name
                    return (std::ptr::null_mut(), std::ptr::null_mut());
                }
            }
            let mut item = unsafe { (fns.obs_scene_find_source_recursive)(scene, c_name.as_ptr()) };
            if item.is_null() {
                item = unsafe { (fns.obs_scene_find_source)(scene, c_name.as_ptr()) };
            }
            if !item.is_null() {
                return (sc_source, item);
            }
        }
        (std::ptr::null_mut(), std::ptr::null_mut())
    };

    // 1. If explicit scene_name is provided, search within that scene first:
    if !scene_name.is_empty() {
        if let Ok(c_s) = std::ffi::CString::new(scene_name) {
            let s = unsafe { (fns.obs_get_source_by_name)(c_s.as_ptr()) };
            let (sc, it) = check_scene_src(s, scene_name);
            if !it.is_null() { return (sc, it); }
            if !s.is_null() { unsafe { (fns.obs_source_release)(s); } }
        }
    }

    // 2. Try configured studio_scene_name if different:
    let s_name = crate::state::get_state().studio_scene_name.read().clone();
    if !s_name.is_empty() && s_name != scene_name {
        if let Ok(c_s) = std::ffi::CString::new(s_name.as_str()) {
            let s = unsafe { (fns.obs_get_source_by_name)(c_s.as_ptr()) };
            let (sc, it) = check_scene_src(s, &s_name);
            if !it.is_null() { return (sc, it); }
            if !s.is_null() { unsafe { (fns.obs_source_release)(s); } }
        }
    }

    (std::ptr::null_mut(), std::ptr::null_mut())
}

pub fn query_obs_sceneitem_transform(scene_name: &str, source_name: &str) -> Option<(f32, f32, f32, f32, f32)> {
    let fns = get_or_init_scene_fns()?;

    let (sc_src, item) = find_scene_item(&fns, scene_name, source_name);
    if item.is_null() {
        if !sc_src.is_null() {
            unsafe { (fns.obs_source_release)(sc_src); }
        }
        return None;
    }

    let mut pos = ObsVec2 { x: 0.0, y: 0.0 };
    if let Some(get_pos) = fns.obs_sceneitem_get_pos {
        unsafe { get_pos(item, &mut pos); }
    }

    let mut scale = ObsVec2 { x: 1.0, y: 1.0 };
    if let Some(get_scale) = fns.obs_sceneitem_get_scale {
        unsafe { get_scale(item, &mut scale); }
    }

    let rot = if let Some(get_rot) = fns.obs_sceneitem_get_rot {
        unsafe { get_rot(item) }
    } else {
        0.0
    };

    let base_src = unsafe { (fns.obs_sceneitem_get_source)(item) };

    let (bw, bh) = if !base_src.is_null() {
        let w = unsafe { (fns.obs_source_get_width)(base_src) };
        let h = unsafe { (fns.obs_source_get_height)(base_src) };
        (w, h)
    } else {
        (0, 0)
    };

    if !sc_src.is_null() {
        unsafe { (fns.obs_source_release)(sc_src); }
    }

    let w = if bw > 0 { bw as f32 * scale.x } else { 0.0 };
    let h = if bh > 0 { bh as f32 * scale.y } else { 0.0 };

    Some((pos.x, pos.y, w, h, rot))
}

pub fn reset_studio_source_to_obs(scene_name: &str, src: &mut crate::StudioSource) {
    let live_obs_xf = if !src.match_obs {
        query_obs_sceneitem_transform(scene_name, &src.name)
    } else {
        None
    };

    let (target_x, target_y, target_w, target_h, target_rot) = if let Some((ox, oy, ow, oh, orot)) = live_obs_xf {
        let w = if ow > 0.0 { ow } else if src.default_width > 0.0 { src.default_width } else { src.width.max(100.0) };
        let h = if oh > 0.0 { oh } else if src.default_height > 0.0 { src.default_height } else { src.height.max(100.0) };
        src.default_x = ox;
        src.default_y = oy;
        src.default_width = w;
        src.default_height = h;
        src.default_roll = orot;
        (ox, oy, w, h, orot)
    } else {
        let w = if src.default_width > 0.0 { src.default_width } else { src.width.max(100.0) };
        let h = if src.default_height > 0.0 { src.default_height } else { src.height.max(100.0) };
        (src.default_x, src.default_y, w, h, src.default_roll)
    };

    src.x = target_x;
    src.y = target_y;
    src.width = target_w;
    src.height = target_h;
    src.roll = target_rot;
    src.pitch = 0.0;
    src.yaw = 0.0;

    println!(
        "[Trust Studio] 🔄 Reset '{}' to OBS transform: pos=({:.1}, {:.1}), size={:.1}x{:.1}, roll={:.1}°, pitch=0°, yaw=0°",
        src.name, src.x, src.y, src.width, src.height, src.roll
    );
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct SourceFilterInfo {
    pub name: String,
    pub id: String,
    pub enabled: bool,
    pub settings: serde_json::Value,
}

unsafe extern "C" fn enum_filter_callback(
    _parent: *mut c_void,
    child: *mut c_void,
    param: *mut c_void,
) {
    if child.is_null() || param.is_null() { return; }
    let list = unsafe { &mut *(param as *mut Vec<SourceFilterInfo>) };
    let fns = match SCENE_FNS.read().as_ref() {
        Some(f) => *f,
        None => return,
    };

    let c_name = unsafe { (fns.obs_source_get_name)(child) };
    let name = if !c_name.is_null() {
        unsafe { CStr::from_ptr(c_name).to_string_lossy().to_string() }
    } else {
        String::new()
    };

    // Ignore internal guard filter
    if name == "theater_guard_filter" {
        return;
    }

    let id = if let Some(get_id) = fns.obs_source_get_id {
        let c_id = unsafe { get_id(child) };
        if !c_id.is_null() {
            unsafe { CStr::from_ptr(c_id).to_string_lossy().to_string() }
        } else {
            String::new()
        }
    } else {
        String::new()
    };

    let enabled = if let Some(is_enabled) = fns.obs_source_enabled {
        unsafe { is_enabled(child) }
    } else {
        true
    };

    let mut settings_val = serde_json::Value::Object(serde_json::Map::new());
    if let (Some(get_settings), Some(get_json), Some(release_data)) = (
        fns.obs_source_get_settings,
        data_get_json_with_defaults().or(fns.obs_data_get_json),
        fns.obs_data_release,
    ) {
        let settings_ptr = unsafe { get_settings(child) };
        if !settings_ptr.is_null() {
            let json_c = unsafe { get_json(settings_ptr) };
            if !json_c.is_null() {
                if let Ok(json_str) = unsafe { CStr::from_ptr(json_c).to_str() } {
                    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(json_str) {
                        settings_val = parsed;
                    }
                }
            }
            unsafe { release_data(settings_ptr); }
        }
    }

    list.push(SourceFilterInfo {
        name,
        id,
        enabled,
        settings: settings_val,
    });
}

/// obs_data_get_json_with_defaults (OBS 30+): a filter's settings including
/// the ones it was never given (still at their default), as OBS's own filter
/// properties show them — so the Studio lists them too and a pose stores them,
/// and applying the pose puts them back. Plain obs_data_get_json leaves them out.
fn data_get_json_with_defaults() -> Option<unsafe extern "C" fn(*mut c_void) -> *const c_char> {
    static SYM: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    let sym = *SYM.get_or_init(|| obs_symbol(c"obs_data_get_json_with_defaults") as usize);
    (sym != 0).then(|| unsafe {
        std::mem::transmute::<usize, unsafe extern "C" fn(*mut c_void) -> *const c_char>(sym)
    })
}

pub fn query_source_filters(source_name: &str) -> Vec<SourceFilterInfo> {
    let fns = match get_or_init_scene_fns() {
        Some(f) => f,
        None => return Vec::new(),
    };
    let enum_filters = match fns.obs_source_enum_filters {
        Some(f) => f,
        None => return Vec::new(),
    };
    let c_name = match CString::new(studio_key_source_name(source_name)) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let src = unsafe { (fns.obs_get_source_by_name)(c_name.as_ptr()) };
    if src.is_null() {
        return Vec::new();
    }

    let mut filters = Vec::new();
    unsafe {
        enum_filters(src, Some(enum_filter_callback), &mut filters as *mut _ as *mut c_void);
        (fns.obs_source_release)(src);
    }
    filters
}

const OBS_SOURCE_AUDIO: u32 = 1 << 1;

/// Mixer volume (linear multiplier, 1.0 = 0 dB) of a source that has audio;
/// None for video-only sources (their volume means nothing) or unknown names.
pub fn query_source_volume(source_name: &str) -> Option<f32> {
    let fns = get_or_init_scene_fns()?;
    let (get_flags, get_volume) = (fns.obs_source_get_output_flags?, fns.obs_source_get_volume?);
    let c_name = CString::new(studio_key_source_name(source_name)).ok()?;
    let src = unsafe { (fns.obs_get_source_by_name)(c_name.as_ptr()) };
    if src.is_null() {
        return None;
    }
    let volume = unsafe {
        if get_flags(src) & OBS_SOURCE_AUDIO != 0 { Some(get_volume(src)) } else { None }
    };
    unsafe { (fns.obs_source_release)(src); }
    volume
}

pub fn set_source_volume(source_name: &str, volume: f32) -> bool {
    let Some(fns) = get_or_init_scene_fns() else { return false };
    let Some(set_volume) = fns.obs_source_set_volume else { return false };
    let Ok(c_name) = CString::new(studio_key_source_name(source_name)) else { return false };
    let src = unsafe { (fns.obs_get_source_by_name)(c_name.as_ptr()) };
    if src.is_null() {
        return false;
    }
    unsafe {
        set_volume(src, volume.max(0.0));
        (fns.obs_source_release)(src);
    }
    true
}

pub const MAX_AUDIO_CHANNELS: usize = 8;
const OBS_FADER_LOG: i32 = 2;

/// Live post-fader levels (dBFS, -inf = silence) of the one source the
/// Studio's 🔊 Audio meter watches — the same numbers OBS's mixer meters show.
#[derive(Clone, Copy, Debug)]
pub struct AudioLevels {
    pub channels: usize,
    pub magnitude: [f32; MAX_AUDIO_CHANNELS],
    pub peak: [f32; MAX_AUDIO_CHANNELS],
    updated_ms: u64,
}

impl AudioLevels {
    const SILENT: AudioLevels = AudioLevels {
        channels: 0,
        magnitude: [f32::NEG_INFINITY; MAX_AUDIO_CHANNELS],
        peak: [f32::NEG_INFINITY; MAX_AUDIO_CHANNELS],
        updated_ms: 0,
    };
}

/// (obs_volmeter_t*, watched source name)
static AUDIO_METER: parking_lot::Mutex<Option<(usize, String)>> = parking_lot::Mutex::new(None);
static AUDIO_LEVELS: parking_lot::Mutex<AudioLevels> = parking_lot::Mutex::new(AudioLevels::SILENT);

fn unix_now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Runs on OBS's audio thread — just stores the levels.
unsafe extern "C" fn audio_meter_cb(_param: *mut c_void, magnitude: *const f32, peak: *const f32, _input_peak: *const f32) {
    if magnitude.is_null() || peak.is_null() {
        return;
    }
    let (magnitude, peak) = unsafe {
        (
            std::slice::from_raw_parts(magnitude, MAX_AUDIO_CHANNELS),
            std::slice::from_raw_parts(peak, MAX_AUDIO_CHANNELS),
        )
    };
    let mut levels = AUDIO_LEVELS.lock();
    levels.magnitude.copy_from_slice(magnitude);
    levels.peak.copy_from_slice(peak);
    levels.updated_ms = unix_now_ms();
}

fn stop_audio_meter_locked(meter: &mut Option<(usize, String)>) {
    let Some((volmeter, _)) = meter.take() else { return };
    let Some(fns) = get_or_init_scene_fns() else { return };
    unsafe {
        if let Some(remove_cb) = fns.obs_volmeter_remove_callback {
            remove_cb(volmeter as *mut c_void, audio_meter_cb, std::ptr::null_mut());
        }
        if let Some(destroy) = fns.obs_volmeter_destroy {
            destroy(volmeter as *mut c_void);
        }
    }
    *AUDIO_LEVELS.lock() = AudioLevels::SILENT;
}

/// Points the Studio's level meter at `source_name` (an OBS volmeter, like the
/// mixer's). Cheap to call every frame — only re-attaches when the name changes.
pub fn watch_audio_levels(source_name: &str) {
    let mut meter = AUDIO_METER.lock();
    if meter.as_ref().is_some_and(|(_, name)| name == source_name) {
        return;
    }
    stop_audio_meter_locked(&mut meter);

    let Some(fns) = get_or_init_scene_fns() else { return };
    let (Some(create), Some(attach), Some(add_cb), Some(destroy)) = (
        fns.obs_volmeter_create,
        fns.obs_volmeter_attach_source,
        fns.obs_volmeter_add_callback,
        fns.obs_volmeter_destroy,
    ) else {
        return;
    };
    let Ok(c_name) = CString::new(studio_key_source_name(source_name)) else { return };
    let src = unsafe { (fns.obs_get_source_by_name)(c_name.as_ptr()) };
    if src.is_null() {
        return;
    }
    let volmeter = unsafe { create(OBS_FADER_LOG) };
    // The volmeter keeps its own (weak) reference to the source
    let attached = !volmeter.is_null() && unsafe { attach(volmeter, src) };
    unsafe { (fns.obs_source_release)(src); }
    if !attached {
        if !volmeter.is_null() {
            unsafe { destroy(volmeter); }
        }
        return;
    }
    unsafe { add_cb(volmeter, audio_meter_cb, std::ptr::null_mut()); }
    *meter = Some((volmeter as usize, source_name.to_string()));
}

/// Stops the Studio's level meter (no source with audio selected, or window hidden).
pub fn stop_audio_levels() {
    stop_audio_meter_locked(&mut AUDIO_METER.lock());
}

/// The watched source's current levels; silent if it hasn't produced audio in the last 300 ms.
pub fn audio_levels() -> AudioLevels {
    let meter = AUDIO_METER.lock();
    let Some((volmeter, _)) = meter.as_ref() else { return AudioLevels::SILENT };
    let channels = get_or_init_scene_fns()
        .and_then(|fns| fns.obs_volmeter_get_nr_channels)
        .map(|get| unsafe { get(*volmeter as *mut c_void) })
        .unwrap_or(2)
        .clamp(1, MAX_AUDIO_CHANNELS as i32) as usize;
    drop(meter);
    let mut levels = *AUDIO_LEVELS.lock();
    levels.channels = channels;
    if unix_now_ms().saturating_sub(levels.updated_ms) > 300 {
        levels.magnitude = [f32::NEG_INFINITY; MAX_AUDIO_CHANNELS];
        levels.peak = [f32::NEG_INFINITY; MAX_AUDIO_CHANNELS];
    }
    levels
}

pub fn update_source_filter_settings(source_name: &str, filter_name: &str, new_json: &str) -> bool {
    let fns = match get_or_init_scene_fns() {
        Some(f) => f,
        None => return false,
    };
    let c_src = match CString::new(studio_key_source_name(source_name)) {
        Ok(s) => s,
        Err(_) => return false,
    };
    let c_flt = match CString::new(filter_name) {
        Ok(s) => s,
        Err(_) => return false,
    };
    let src = unsafe { (fns.obs_get_source_by_name)(c_src.as_ptr()) };
    if src.is_null() { return false; }

    let filter = if let Some(get_flt) = fns.obs_source_get_filter_by_name {
        unsafe { get_flt(src, c_flt.as_ptr()) }
    } else {
        std::ptr::null_mut()
    };

    let mut ok = false;
    if !filter.is_null() {
        if let (Some(from_json), Some(update), Some(release)) = (
            fns.obs_data_create_from_json,
            fns.obs_source_update,
            fns.obs_data_release,
        ) {
            if let Ok(c_json) = CString::new(new_json) {
                let data = unsafe { from_json(c_json.as_ptr()) };
                if !data.is_null() {
                    unsafe {
                        update(filter, data);
                        release(data);
                    }
                    ok = true;
                }
            }
        }
        unsafe { (fns.obs_source_release)(filter); }
    }
    unsafe { (fns.obs_source_release)(src); }
    ok
}

pub fn set_source_filter_enabled(source_name: &str, filter_name: &str, enabled: bool) -> bool {
    let fns = match get_or_init_scene_fns() {
        Some(f) => f,
        None => return false,
    };
    let set_enabled = match fns.obs_source_set_enabled {
        Some(f) => f,
        None => return false,
    };
    let c_src = match CString::new(studio_key_source_name(source_name)) {
        Ok(s) => s,
        Err(_) => return false,
    };
    let c_flt = match CString::new(filter_name) {
        Ok(s) => s,
        Err(_) => return false,
    };
    let src = unsafe { (fns.obs_get_source_by_name)(c_src.as_ptr()) };
    if src.is_null() { return false; }

    let filter = if let Some(get_flt) = fns.obs_source_get_filter_by_name {
        unsafe { get_flt(src, c_flt.as_ptr()) }
    } else {
        std::ptr::null_mut()
    };

    let mut ok = false;
    if !filter.is_null() {
        unsafe {
            set_enabled(filter, enabled);
            (fns.obs_source_release)(filter);
        }
        ok = true;
    }
    unsafe { (fns.obs_source_release)(src); }
    ok
}

/// Scene switches queued to OBS's UI thread (switch_obs_current_scene) and not
/// done yet. Until they are, OBS still reports the old scene, so
/// refresh_obs_scenes leaves studio_scene_name alone rather than flipping the
/// Studio back to it.
static PENDING_SCENE_SWITCHES: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// A scene list refresh is queued to OBS's UI thread and hasn't run yet.
static SCENE_LIST_REFRESH_QUEUED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Runs `task(param)` on OBS's UI thread without waiting for it (inline when
/// already there). False if obs_queue_task isn't available; the task didn't run.
pub(crate) fn queue_ui_task(task: ObsTaskFn, param: *mut c_void) -> bool {
    let core_handle = open_libobs_core();
    let queue_task_ptr = unsafe { frontend_sym(core_handle, b"obs_queue_task\0") };
    if queue_task_ptr.is_null() {
        return false;
    }
    let queue_task: ObsQueueTaskFn = unsafe { std::mem::transmute(queue_task_ptr) };
    unsafe { queue_task(OBS_TASK_UI, task, param, false); }
    true
}

/// Reads OBS's current scene into studio_scene_name, and has the UI thread
/// refresh studio_available_scenes. Safe from any thread: the scene list comes
/// from OBS's Qt scene list widget, which only the UI thread may read (from the
/// Studio's thread it raced OBS changing that list during a scene switch).
pub fn refresh_obs_scenes() {
    let fns = match init_scene_fns() {
        Some(f) => f,
        None => return,
    };

    // 1. Query current active scene from OBS frontend
    if let Some(get_cur) = fns.obs_frontend_get_current_scene {
        let s = unsafe { get_cur() };
        if !s.is_null() {
            let name_ptr = unsafe { (fns.obs_source_get_name)(s) };
            if !name_ptr.is_null() {
                let name = unsafe { CStr::from_ptr(name_ptr).to_string_lossy().to_string() };
                let switching = PENDING_SCENE_SWITCHES.load(std::sync::atomic::Ordering::SeqCst) > 0;
                if !name.is_empty() && !switching {
                    let mut cur = crate::state::get_state().studio_scene_name.write();
                    if *cur != name {
                        *cur = name;
                    }
                }
            }
            unsafe { (fns.obs_source_release)(s); }
        }
    }

    // 2. Query all scenes in collection, on the UI thread
    if SCENE_LIST_REFRESH_QUEUED.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    if !queue_ui_task(run_scene_list_refresh_on_ui_thread, std::ptr::null_mut()) {
        SCENE_LIST_REFRESH_QUEUED.store(false, std::sync::atomic::Ordering::SeqCst);
    }
}

unsafe extern "C" fn run_scene_list_refresh_on_ui_thread(_param: *mut c_void) {
    SCENE_LIST_REFRESH_QUEUED.store(false, std::sync::atomic::Ordering::SeqCst);
    refresh_obs_scene_list();
}

/// Reads the scene list into studio_available_scenes. UI thread only.
fn refresh_obs_scene_list() {
    let fns = match init_scene_fns() {
        Some(f) => f,
        None => return,
    };
    if let (Some(get_scenes), Some(free_fn)) = (fns.obs_frontend_get_scenes, fns.bfree) {
        let mut list = ObsSourceList {
            sources: ObsSourceArray {
                array: std::ptr::null_mut(),
                num: 0,
                capacity: 0,
            },
        };
        unsafe { get_scenes(&mut list); }
        if list.sources.num > 0 && !list.sources.array.is_null() {
            let mut scenes = Vec::new();
            for i in 0..list.sources.num {
                let s = unsafe { *list.sources.array.add(i) };
                if !s.is_null() {
                    let name_ptr = unsafe { (fns.obs_source_get_name)(s) };
                    if !name_ptr.is_null() {
                        let name = unsafe { CStr::from_ptr(name_ptr).to_string_lossy().to_string() };
                        if !name.is_empty() && !scenes.contains(&name) {
                            scenes.push(name);
                        }
                    }
                    unsafe { (fns.obs_source_release)(s); }
                }
            }
            unsafe { free_fn(list.sources.array as *mut c_void); }
            if !scenes.is_empty() {
                *crate::state::get_state().studio_available_scenes.write() = scenes;
            }
        }
    }
}

/// Has OBS's UI thread switch to `scene_name`, without waiting for it. From
/// any other thread (the Studio's), obs_frontend_set_current_scene blocks until
/// the UI thread is done — the Studio then stops pumping its window and can end
/// up waiting on OBS while OBS waits on it. True if the switch was queued.
pub fn switch_obs_current_scene(scene_name: &str) -> bool {
    if scene_name.is_empty() { return false; }
    let fns = match init_scene_fns() {
        Some(f) => f,
        None => return false,
    };
    if fns.obs_frontend_set_current_scene.is_none() {
        eprintln!("[Trust] ⚠️ obs_frontend_set_current_scene not available");
        return false;
    }
    let c_name = match std::ffi::CString::new(scene_name) {
        Ok(s) => s,
        Err(_) => return false,
    };
    let scene_source = unsafe { (fns.obs_get_source_by_name)(c_name.as_ptr()) };
    if scene_source.is_null() {
        eprintln!("[Trust] ⚠️ Scene source '{}' not found in OBS", scene_name);
        return false;
    }
    unsafe { (fns.obs_source_release)(scene_source); }

    PENDING_SCENE_SWITCHES.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let param = Box::into_raw(Box::new(c_name)) as *mut c_void;
    if !queue_ui_task(run_scene_switch_on_ui_thread, param) {
        // No obs_queue_task: switch from here, as before
        unsafe { run_scene_switch_on_ui_thread(param); }
    }
    true
}

unsafe extern "C" fn run_scene_switch_on_ui_thread(param: *mut c_void) {
    let c_name = unsafe { Box::from_raw(param as *mut CString) };
    if let Some(fns) = init_scene_fns() {
        if let Some(set_current) = fns.obs_frontend_set_current_scene {
            // Looked up again: the scene may have been removed since it was queued
            let scene_source = unsafe { (fns.obs_get_source_by_name)(c_name.as_ptr()) };
            if !scene_source.is_null() {
                unsafe {
                    set_current(scene_source);
                    (fns.obs_source_release)(scene_source);
                }
                println!("[Trust] 🎬 Switched OBS current scene to '{}'", c_name.to_string_lossy());
            }
        }
    }
    PENDING_SCENE_SWITCHES.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
}

#[derive(Debug, Clone)]
pub struct ObsSceneItemSummary {
    pub name: String,
    pub is_visible: bool,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub roll: f32,
    /// OBS scene item id (unique within its scene, kept across restarts); 0 if unavailable
    pub item_id: i64,
}

/// Studio name for each scene item: its source's name — or, when the same
/// source is placed more than once in a scene (e.g. one capture card shown
/// twice), "Name #2", "Name #3" for the extra copies, ordered by OBS scene item
/// id so each copy keeps its name across restarts. Everything the Studio keys
/// by name (positions, poses, the transform queue) then keeps the copies apart
/// instead of forcing one set of coordinates onto both.
/// Input and output are (item id, source name) / (item id, source name, Studio name).
pub fn studio_item_keys(items: &[(i64, String)]) -> Vec<(i64, String, String)> {
    let mut ids_by_name: std::collections::HashMap<&str, Vec<i64>> = std::collections::HashMap::new();
    for (id, name) in items {
        ids_by_name.entry(name.as_str()).or_default().push(*id);
    }
    for ids in ids_by_name.values_mut() {
        ids.sort_unstable();
    }
    items
        .iter()
        .map(|(id, name)| {
            let rank = ids_by_name[name.as_str()].iter().position(|x| x == id).unwrap_or(0);
            let key = if rank == 0 { name.clone() } else { format!("{} #{}", name, rank + 1) };
            (*id, name.clone(), key)
        })
        .collect()
}

/// Studio names of one scene's items, both ways.
#[derive(Default, Clone)]
struct SceneItemKeys {
    /// scene item id -> Studio name
    by_id: std::collections::HashMap<i64, String>,
    /// Studio name -> (OBS source name, scene item id)
    by_key: std::collections::HashMap<String, (String, i64)>,
}

/// Per scene name. Refreshed whenever the Studio syncs a scene and each frame the layer renders one.
static SCENE_ITEM_KEYS: std::sync::LazyLock<parking_lot::RwLock<std::collections::HashMap<String, SceneItemKeys>>> =
    std::sync::LazyLock::new(|| parking_lot::RwLock::new(std::collections::HashMap::new()));

unsafe extern "C" fn collect_item_ids_cb(_scene: *mut c_void, item: *mut c_void, param: *mut c_void) -> bool {
    if item.is_null() || param.is_null() {
        return true;
    }
    let list = unsafe { &mut *(param as *mut Vec<(i64, String)>) };
    if let Some(ref fns) = *SCENE_FNS.read() {
        let src = unsafe { (fns.obs_sceneitem_get_source)(item) };
        let id = fns.obs_sceneitem_get_id.map(|get| unsafe { get(item) }).unwrap_or(0);
        if !src.is_null() && id != 0 {
            let name_ptr = unsafe { (fns.obs_source_get_name)(src) };
            if !name_ptr.is_null() {
                list.push((id, unsafe { CStr::from_ptr(name_ptr) }.to_string_lossy().to_string()));
            }
        }
    }
    true
}

fn store_scene_item_keys(scene_name: &str, items: &[(i64, String)]) {
    let mut keys = SceneItemKeys::default();
    for (id, source_name, key) in studio_item_keys(items) {
        keys.by_id.insert(id, key.clone());
        keys.by_key.insert(key, (source_name, id));
    }
    SCENE_ITEM_KEYS.write().insert(scene_name.to_string(), keys);
}

/// Re-reads which items `scene` holds and refreshes their Studio names.
fn refresh_scene_item_keys(fns: &ObsSceneFns, scene_name: &str, scene: *mut c_void) {
    let Some(enum_items) = fns.obs_scene_enum_items else { return };
    if scene.is_null() || fns.obs_sceneitem_get_id.is_none() {
        return;
    }
    let mut items: Vec<(i64, String)> = Vec::new();
    unsafe { enum_items(scene, Some(collect_item_ids_cb), &mut items as *mut _ as *mut c_void); }
    store_scene_item_keys(scene_name, &items);
}

/// Studio name of a scene item (its source name unless it's an extra copy).
fn studio_key_for_item(scene_name: &str, item_id: i64, source_name: &str) -> String {
    SCENE_ITEM_KEYS
        .read()
        .get(scene_name)
        .and_then(|keys| keys.by_id.get(&item_id).cloned())
        .unwrap_or_else(|| source_name.to_string())
}

/// The OBS source behind a Studio name ("Elgato #2" -> "Elgato"); the name
/// itself when it isn't an extra copy. For source-level calls (filters,
/// volume, meter), which both copies share.
pub fn studio_key_source_name(key: &str) -> String {
    SCENE_ITEM_KEYS
        .read()
        .values()
        .find_map(|keys| keys.by_key.get(key).map(|(source_name, _)| source_name.clone()))
        .unwrap_or_else(|| key.to_string())
}

unsafe extern "C" fn enum_items_collector_cb(
    _scene: *mut c_void,
    item: *mut c_void,
    param: *mut c_void,
) -> bool {
    if item.is_null() || param.is_null() { return true; }
    unsafe {
        let list = &mut *(param as *mut Vec<ObsSceneItemSummary>);
        if let Some(ref fns) = *SCENE_FNS.read() {
            let src = (fns.obs_sceneitem_get_source)(item);
            if !src.is_null() {
                let mut is_throwables = false;
                if let Some(get_id) = fns.obs_source_get_id {
                    let id_ptr = get_id(src);
                    if !id_ptr.is_null() {
                        let id_bytes = CStr::from_ptr(id_ptr).to_bytes();
                        if id_bytes == b"theater_throwables_layer" {
                            is_throwables = true;
                        }
                    }
                }
                let name_ptr = (fns.obs_source_get_name)(src);
                let name = if !name_ptr.is_null() {
                    CStr::from_ptr(name_ptr).to_string_lossy().to_string()
                } else {
                    String::new()
                };
                if name.contains("Throwables Layer") {
                    is_throwables = true;
                }
                if is_throwables {
                    let is_vis = if let Some(vis_fn) = fns.obs_sceneitem_visible {
                        vis_fn(item)
                    } else {
                        true
                    };
                    crate::state::get_state().studio_throwables_visible.store(is_vis, std::sync::atomic::Ordering::Relaxed);
                }
                if !is_throwables 
                    && !name.is_empty() 
                    && !name.contains("Hitbox Visualizer") 
                {
                    let is_vis = if let Some(vis_fn) = fns.obs_sceneitem_visible {
                        vis_fn(item)
                    } else {
                        true
                    };
                        let mut pos = ObsVec2::default();
                        if let Some(get_pos) = fns.obs_sceneitem_get_pos {
                            get_pos(item, &mut pos);
                        }
                        let mut scale = ObsVec2 { x: 1.0, y: 1.0 };
                        if let Some(get_scale) = fns.obs_sceneitem_get_scale {
                            get_scale(item, &mut scale);
                        }
                        let rot = if let Some(get_rot) = fns.obs_sceneitem_get_rot {
                            get_rot(item)
                        } else {
                            0.0
                        };
                        let raw_w = (fns.obs_source_get_width)(src) as f32;
                        let raw_h = (fns.obs_source_get_height)(src) as f32;
                        let w = if raw_w > 0.0 { raw_w * scale.x } else { 100.0 };
                        let h = if raw_h > 0.0 { raw_h * scale.y } else { 100.0 };

                        list.push(ObsSceneItemSummary {
                            name,
                            is_visible: is_vis,
                            x: pos.x,
                            y: pos.y,
                            width: w,
                            height: h,
                            roll: rot,
                            item_id: fns.obs_sceneitem_get_id.map(|get| get(item)).unwrap_or(0),
                        });
                    }
                }
            }
        }
    true
}

pub fn sort_studio_sources_in_good_order() {
    let state = crate::state::get_state();
    let mut sources = state.studio_sources.write();
    // Sort by slot ascending (1, 2, 3...) so Slot 1 (top of visual stack) is at the top.
    // Unassigned items (slot 0) are placed after slotted items.
    sources.sort_by(|a, b| {
        let a_slot = if a.slot > 0 { a.slot } else { u32::MAX };
        let b_slot = if b.slot > 0 { b.slot } else { u32::MAX };
        a_slot.cmp(&b_slot)
    });
}

/// Mirrors `scene_name`'s items from OBS into the Studio's sources. It only
/// reads the scene: it doesn't put a pose back (opening the Studio, or OBS
/// switching scenes, moved sources to their last pose; picking a scene in
/// the Studio's dropdown still does, see load_and_apply_last_pose_on_load).
pub fn sync_studio_sources_for_scene(scene_name: &str) {
    if scene_name.is_empty() { return; }

    let fns = match init_scene_fns() {
        Some(f) => f,
        None => {
            sort_studio_sources_in_good_order();
            return;
        }
    };

    let c_scene = match std::ffi::CString::new(scene_name) {
        Ok(c) => c,
        Err(_) => return,
    };

    let scene_src = unsafe { (fns.obs_get_source_by_name)(c_scene.as_ptr()) };
    if scene_src.is_null() {
        sort_studio_sources_in_good_order();
        return;
    }

    let scene = unsafe { (fns.obs_scene_from_source)(scene_src) };
    if scene.is_null() {
        unsafe { (fns.obs_source_release)(scene_src); }
        sort_studio_sources_in_good_order();
        return;
    }

    let mut obs_items: Vec<ObsSceneItemSummary> = Vec::new();
    if let Some(enum_items) = fns.obs_scene_enum_items {
        unsafe {
            enum_items(
                scene,
                Some(enum_items_collector_cb),
                &mut obs_items as *mut _ as *mut c_void,
            );
        }
    }
    unsafe { (fns.obs_source_release)(scene_src); }

    // obs_scene_enum_items traverses from bottom-most (layer 0) to top-most (layer N-1).
    // Reversing puts the topmost visual layer (top of OBS Sources dock) first!
    obs_items.reverse();

    // Extra copies of the same source get their own Studio entry ("Elgato #2")
    let id_names: Vec<(i64, String)> = obs_items.iter()
        .filter(|i| i.item_id != 0)
        .map(|i| (i.item_id, i.name.clone()))
        .collect();
    store_scene_item_keys(scene_name, &id_names);
    for item in obs_items.iter_mut().filter(|i| i.item_id != 0) {
        item.name = studio_key_for_item(scene_name, item.item_id, &item.name);
    }

    if obs_items.is_empty() {
        sort_studio_sources_in_good_order();
        return;
    }

    let state = crate::state::get_state();
    let current_saved_sources = state.studio_sources.read().clone();
    let disk_targets = crate::studio_app::read_disk_targets();

    let mut new_sources: Vec<crate::StudioSource> = Vec::new();
    for (idx, item) in obs_items.iter().enumerate() {
        let slot = (idx + 1) as u32;

        let saved_opt = current_saved_sources.iter()
            .find(|s| s.name == item.name)
            .cloned()
            .or_else(|| disk_targets.iter().find(|s| s.name == item.name).cloned());

        if let Some(mut s) = saved_opt {
            s.scene = scene_name.to_string();
            s.slot = slot;
            s.is_visible = item.is_visible;
            new_sources.push(s);
        } else {
            new_sources.push(crate::StudioSource {
                scene: scene_name.to_string(),
                name: item.name.clone(),
                slot,
                x: item.x,
                y: item.y,
                width: item.width,
                height: item.height,
                roll: item.roll,
                pitch: 0.0,
                yaw: 0.0,
                is_target: false,
                match_obs: true,
                is_visible: item.is_visible,
                file_path: String::new(),
                default_x: item.x,
                default_y: item.y,
                default_width: item.width,
                default_height: item.height,
                default_roll: item.roll,
            });
        }
    }

    // Notify updates for transforms
    for s in &new_sources {
        crate::queue_studio_transform_update(s);
    }

    *state.studio_sources.write() = new_sources.clone();
    crate::studio_app::save_targets_to_disk(&new_sources);

    // Ensure selected source is valid
    let mut cur_sel = state.studio_selected_source.write();
    if !new_sources.iter().any(|s| s.name == *cur_sel) {
        if let Some(first_target) = new_sources.iter().find(|s| s.is_target) {
            *cur_sel = first_target.name.clone();
        } else if let Some(first) = new_sources.first() {
            *cur_sel = first.name.clone();
        } else {
            cur_sel.clear();
        }
    }
}

struct ThrowablesLayerFinder {
    target_item: *mut c_void,
    is_visible: bool,
}

unsafe extern "C" fn find_throwables_enum_cb(
    _scene: *mut c_void,
    item: *mut c_void,
    param: *mut c_void,
) -> bool {
    if item.is_null() || param.is_null() { return true; }
    unsafe {
        let finder = &mut *(param as *mut ThrowablesLayerFinder);
        let fns = match SCENE_FNS.read().as_ref() {
            Some(f) => *f,
            None => return true,
        };
        let src = (fns.obs_sceneitem_get_source)(item);
        if !src.is_null() {
            let mut is_throwables = false;
            if let Some(get_id) = fns.obs_source_get_id {
                let id_ptr = get_id(src);
                if !id_ptr.is_null() {
                    let id_bytes = CStr::from_ptr(id_ptr).to_bytes();
                    if id_bytes == b"theater_throwables_layer" {
                        is_throwables = true;
                    }
                }
            }
            if !is_throwables {
                let name_ptr = (fns.obs_source_get_name)(src);
                if !name_ptr.is_null() {
                    let name = CStr::from_ptr(name_ptr).to_string_lossy();
                    if name.contains("Throwables Layer") {
                        is_throwables = true;
                    }
                }
            }
            if is_throwables {
                finder.target_item = item;
                finder.is_visible = if let Some(vis_fn) = fns.obs_sceneitem_visible {
                    vis_fn(item)
                } else {
                    true
                };
                return false; // Found, stop search
            }
        }
    }
    true
}

pub fn get_throwables_layer_status(scene_name: &str) -> (bool, bool) {
    let fns = match init_scene_fns() {
        Some(f) => f,
        None => return (false, true),
    };

    let check_scene = |sc_src: *mut c_void| -> Option<(bool, bool)> {
        if sc_src.is_null() { return None; }
        let scene = unsafe { (fns.obs_scene_from_source)(sc_src) };
        if scene.is_null() { return None; }
        let mut finder = ThrowablesLayerFinder {
            target_item: std::ptr::null_mut(),
            is_visible: true,
        };
        if let Some(enum_items) = fns.obs_scene_enum_items {
            unsafe {
                enum_items(
                    scene,
                    Some(find_throwables_enum_cb),
                    &mut finder as *mut _ as *mut c_void,
                );
            }
        }
        if !finder.target_item.is_null() {
            Some((true, finder.is_visible))
        } else {
            None
        }
    };

    let mut res = None;
    if !scene_name.is_empty() {
        if let Ok(c_s) = std::ffi::CString::new(scene_name) {
            let s = unsafe { (fns.obs_get_source_by_name)(c_s.as_ptr()) };
            res = check_scene(s);
            if !s.is_null() { unsafe { (fns.obs_source_release)(s); } }
        }
    }

    if res.is_none() {
        if let Some(get_cur) = fns.obs_frontend_get_current_scene {
            let s = unsafe { get_cur() };
            res = check_scene(s);
            if !s.is_null() { unsafe { (fns.obs_source_release)(s); } }
        }
    }

    if let Some((found, vis)) = res {
        crate::state::get_state().studio_throwables_visible.store(vis, std::sync::atomic::Ordering::Relaxed);
        (found, vis)
    } else {
        (false, true)
    }
}

pub fn set_throwables_layer_visible_in_obs(scene_name: &str, visible: bool) -> bool {
    let fns = match init_scene_fns() {
        Some(f) => f,
        None => return false,
    };
    let set_vis = match fns.obs_sceneitem_set_visible {
        Some(f) => f,
        None => return false,
    };

    let modify_scene = |sc_src: *mut c_void| -> bool {
        if sc_src.is_null() { return false; }
        let scene = unsafe { (fns.obs_scene_from_source)(sc_src) };
        if scene.is_null() { return false; }
        let mut finder = ThrowablesLayerFinder {
            target_item: std::ptr::null_mut(),
            is_visible: true,
        };
        if let Some(enum_items) = fns.obs_scene_enum_items {
            unsafe {
                enum_items(
                    scene,
                    Some(find_throwables_enum_cb),
                    &mut finder as *mut _ as *mut c_void,
                );
            }
        }
        if !finder.target_item.is_null() {
            unsafe { set_vis(finder.target_item, visible); }
            crate::state::get_state().studio_throwables_visible.store(visible, std::sync::atomic::Ordering::Relaxed);
            println!("[Trust OBS] 👁 Set Throwables Layer visibility to {}", visible);
            return true;
        }
        false
    };

    let mut ok = false;
    if !scene_name.is_empty() {
        if let Ok(c_s) = std::ffi::CString::new(scene_name) {
            let s = unsafe { (fns.obs_get_source_by_name)(c_s.as_ptr()) };
            ok = modify_scene(s);
            if !s.is_null() { unsafe { (fns.obs_source_release)(s); } }
        }
    }

    if !ok {
        if let Some(get_cur) = fns.obs_frontend_get_current_scene {
            let s = unsafe { get_cur() };
            ok = modify_scene(s);
            if !s.is_null() { unsafe { (fns.obs_source_release)(s); } }
        }
    }

    ok
}

pub fn apply_obs_sceneitem_transform_full(
    fns: &ObsSceneFns,
    scene_name: &str,
    source_name: &str,
    x: f32,
    y: f32,
    roll: f32,
    width: f32,
    height: f32,
    match_obs: bool,
    is_visible: bool,
) -> bool {
    let (sc_source, item) = find_scene_item(fns, scene_name, source_name);
    if item.is_null() {
        if !sc_source.is_null() {
            unsafe { (fns.obs_source_release)(sc_source); }
        }
        println!("[Trust OBS Sync] ⚠️ Scene item '{}' not found in scene '{}'!", source_name, scene_name);
        return false;
    }

    println!("[Trust OBS Sync] 🔄 Updating '{}': match_obs={}, visible={}, x={:.1}, y={:.1}", source_name, match_obs, is_visible, x, y);

    // 1. Visibility toggle
    if let Some(set_vis) = fns.obs_sceneitem_set_visible {
        unsafe { set_vis(item, is_visible); }
    }

    // Base native dimensions of source
    let src = unsafe { (fns.obs_sceneitem_get_source)(item) };
    let (bw, bh) = if !src.is_null() {
        (unsafe { (fns.obs_source_get_width)(src) }, unsafe { (fns.obs_source_get_height)(src) })
    } else {
        (0, 0)
    };

    if match_obs {
        // Sync OBS ON: Unlock item and apply transform
        if let Some(set_locked) = fns.obs_sceneitem_set_locked {
            unsafe { set_locked(item, false); }
        }
        let pos = ObsVec2 { x, y };
        unsafe { (fns.obs_sceneitem_set_pos)(item, &pos); }
        unsafe { (fns.obs_sceneitem_set_rot)(item, roll); }

        if width > 0.0 && height > 0.0 && bw > 0 && bh > 0 {
            let scale = ObsVec2 {
                x: width / bw as f32,
                y: height / bh as f32,
            };
            unsafe { (fns.obs_sceneitem_set_scale)(item, &scale); }
        }
        println!("[Trust OBS Sync] ✅ Synced to OBS: '{}' unlocked and moved to ({:.1}, {:.1}), roll={:.1}°", source_name, x, y, roll);
    } else {
        // Sync OBS OFF: Reset to default_xform and Lock item in OBS!
        let (def_x, def_y, def_w, def_h, def_roll) = {
            let sources = crate::state::get_state().studio_sources.read();
            if let Some(s) = sources.iter().find(|s| s.name == source_name) {
                (s.default_x, s.default_y, s.default_width, s.default_height, s.default_roll)
            } else {
                (x, y, width, height, roll)
            }
        };

        if let Some(set_locked) = fns.obs_sceneitem_set_locked {
            unsafe { set_locked(item, false); }
        }
        let pos = ObsVec2 { x: def_x, y: def_y };
        unsafe { (fns.obs_sceneitem_set_pos)(item, &pos); }
        unsafe { (fns.obs_sceneitem_set_rot)(item, def_roll); }

        if def_w > 0.0 && def_h > 0.0 && bw > 0 && bh > 0 {
            let scale = ObsVec2 {
                x: def_w / bw as f32,
                y: def_h / bh as f32,
            };
            unsafe { (fns.obs_sceneitem_set_scale)(item, &scale); }
        }

        if let Some(set_locked) = fns.obs_sceneitem_set_locked {
            unsafe { set_locked(item, true); }
        }
        println!("[Trust OBS Sync] 🔒 Reset in OBS: '{}' locked at default ({:.1}, {:.1}), roll={:.1}°", source_name, def_x, def_y, def_roll);
    }

    if !sc_source.is_null() {
        unsafe { (fns.obs_source_release)(sc_source); }
    }
    true
}

pub fn apply_obs_sceneitem_transform(
    source_name: &str,
    x: f32,
    y: f32,
    roll: f32,
    width: f32,
    height: f32,
) -> bool {
    let fns = match get_or_init_scene_fns() {
        Some(f) => f,
        None => return false,
    };
    let cur_scene = crate::state::get_state().studio_scene_name.read().clone();
    apply_obs_sceneitem_transform_full(&fns, &cur_scene, source_name, x, y, roll, width, height, true, true)
}

static PENDING_TRANSFORMS_APPLY_LOCK: parking_lot::Mutex<()> = parking_lot::Mutex::new(());

pub fn process_pending_studio_transforms() {
    // Non-blocking: this drains PENDING_STUDIO_TRANSFORMS and calls real
    // libobs sceneitem functions for each entry. It's reachable from the
    // video-io thread every tick (unconditionally, from source_video_tick)
    // and from whichever thread is running load_studio_targets_from_disk
    // (once per loaded source, via queue_studio_transform_update) — those
    // two can overlap during startup. Two threads calling libobs scene
    // functions at once is what deadlocked things before, so if another
    // thread is already draining the queue, skip: nothing is lost, the
    // entries stay in PENDING_STUDIO_TRANSFORMS and get picked up on the
    // very next tick (every video frame, so effectively immediately).
    let _apply_guard = match PENDING_TRANSFORMS_APPLY_LOCK.try_lock() {
        Some(g) => g,
        None => return,
    };
    let fns = match get_or_init_scene_fns() {
        Some(f) => f,
        None => return,
    };

    let updates: Vec<crate::StudioTransformUpdate> = {
        let mut pending = crate::PENDING_STUDIO_TRANSFORMS.lock();
        if pending.is_empty() {
            return;
        }
        pending.drain().map(|(_, v)| v).collect()
    };

    let mut unapplied = Vec::new();
    for update in updates {
        let scene = unsafe {
            std::ffi::CStr::from_ptr(update.scene.as_ptr())
                .to_string_lossy()
                .to_string()
        };
        let name = unsafe {
            std::ffi::CStr::from_ptr(update.name.as_ptr())
                .to_string_lossy()
                .to_string()
        };
        if name.is_empty() { continue; }

        // Keep studio_sources in sync so video_render and rust_render_scene_item_cb render the exact same transforms
        {
            let mut sources = crate::state::get_state().studio_sources.write();
            if let Some(s) = sources.iter_mut().find(|s| s.name == name) {
                s.x = update.x;
                s.y = update.y;
                s.width = update.width;
                s.height = update.height;
                s.roll = update.roll;
                s.pitch = update.pitch;
                s.yaw = update.yaw;
                s.match_obs = update.match_obs;
                s.is_visible = update.is_visible;
            }
        }

        let success = apply_obs_sceneitem_transform_full(
            &fns,
            &scene,
            &name,
            update.x,
            update.y,
            update.roll,
            update.width,
            update.height,
            update.match_obs,
            update.is_visible,
        );
        if !success {
            let key = if scene.is_empty() { name.clone() } else { format!("{}:{}", scene, name) };
            unapplied.push((key, update));
        }
    }

    if !unapplied.is_empty() {
        let mut pending = crate::PENDING_STUDIO_TRANSFORMS.lock();
        for (key, update) in unapplied {
            pending.entry(key).or_insert(update);
        }
    }
}

unsafe extern "C" fn source_video_tick(_data: *mut c_void, seconds: f32) {
    let (cw, ch) = get_obs_canvas_resolution();
    let count = TICK_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    if count % 300 == 0 {
        println!("[Trust Source] ⏱️ video_tick running (dt={:.3}s, total_ticks={}, canvas={}x{})", seconds, count, cw, ch);
    }
    let dt = seconds.clamp(0.001, 0.1);
    LAST_TICK_SECONDS.store(dt.to_bits(), std::sync::atomic::Ordering::Relaxed);
    // The current scene's items are drawn by this layer only (render_gate)
    crate::render_gate::tick();

    // Fallback: If source_video_render did not run since the last tick (e.g. source is hidden in OBS scene),
    // step physics here so physics/effects simulation doesn't stall when hidden.
    let rendered = RENDER_ACTIVE_FLAG.swap(false, std::sync::atomic::Ordering::Relaxed);
    if !rendered {
        let state = crate::state::get_state();
        state.physics.write().step(dt, cw as f32, ch as f32);

        let dt_ms = dt * 1000.0;
        let hitboxes = state.hitboxes.read();
        let slots = state.target_slots.read();
        let masks = state.masks.read();
        state.effects.write().step(dt_ms, cw as f32, ch as f32, &*hitboxes, &*slots, &*masks);
    }

    if crate::state::get_state().studio_sources.read().is_empty() {
        crate::studio_app::load_studio_targets_from_disk();
    }
    crate::studio_app::tick_active_transition_standalone();
    process_pending_studio_transforms();
}

struct RenderSceneItemContext<'a> {
    scene_fns: &'a ObsSceneFns,
    gfx_fns: &'a ObsGraphicsFns,
    scene_name: String,
}

fn is_game_capture_source(scene_fns: &ObsSceneFns, source: *mut c_void) -> bool {
    if source.is_null() {
        return false;
    }
    if let Some(get_id) = scene_fns.obs_source_get_id {
        let id_ptr = unsafe { get_id(source) };
        if !id_ptr.is_null() {
            let id = unsafe { std::ffi::CStr::from_ptr(id_ptr).to_string_lossy() };
            if id == "vkcapture-source"
                || id.starts_with("game_capture")
                || id.starts_with("game-capture")
                || id.starts_with("linux-vkcapture")
                || id.contains("vkcapture")
                || id.contains("glcapture")
            {
                return true;
            }
        }
    }
    let name_ptr = unsafe { (scene_fns.obs_source_get_name)(source) };
    if !name_ptr.is_null() {
        let name = unsafe { std::ffi::CStr::from_ptr(name_ptr).to_string_lossy() };
        if name.contains("Capture de jeu")
            || name.contains("Game Capture")
            || name.contains("game_capture")
        {
            return true;
        }
    }
    false
}

fn ensure_source_filter_guard(
    scene_fns: &ObsSceneFns,
    gfx_fns: &ObsGraphicsFns,
    source: *mut c_void,
) {
    if source.is_null() {
        return;
    }

    let filter_add = match scene_fns.obs_source_filter_add {
        Some(f) => f,
        None => return,
    };
    let create_priv = match gfx_fns.obs_source_create_private {
        Some(f) => f,
        None => return,
    };

    let src_key = source as usize;
    let mut guard_lock = ATTACHED_GUARD_FILTERS.lock();
    let guard = guard_lock.get_or_insert_with(std::collections::HashMap::new);
    if guard.contains_key(&src_key) {
        return;
    }

    // Check if the source already has our guard filter attached
    let filter_name = b"theater_guard_filter\0".as_ptr() as *const c_char;
    if let Some(get_filter) = scene_fns.obs_source_get_filter_by_name {
        let existing = unsafe { get_filter(source, filter_name) };
        if !existing.is_null() {
            let existing_id = if let Some(get_id) = scene_fns.obs_source_get_id {
                let id_ptr = unsafe { get_id(existing) };
                if !id_ptr.is_null() {
                    unsafe { std::ffi::CStr::from_ptr(id_ptr).to_string_lossy().to_string() }
                } else {
                    String::new()
                }
            } else {
                String::new()
            };

            // If any source still has an old SDR color_filter guard, upgrade to crop_filter!
            if existing_id == "color_filter" {
                if let Some(filter_remove) = scene_fns.obs_source_filter_remove {
                    unsafe { filter_remove(source, existing); }
                }
                unsafe { (scene_fns.obs_source_release)(existing); }
                println!("[Trust Filter Guard] 🔄 Upgrading guard filter from color_filter to universal crop_filter");
            } else {
                guard.insert(src_key, existing as usize);
                unsafe { (scene_fns.obs_source_release)(existing); }
                return;
            }
        }
    }

    // Check if the source already has any filter attached (user-added Color Correction, Chroma Key, Crop, etc.)
    if let Some(count_fn) = scene_fns.obs_source_filter_count {
        let count = unsafe { count_fn(source) };
        if count > 0 {
            guard.insert(src_key, 0);
            return;
        }
    }

    // Attach private isolated render filter:
    // User requested: "all filter should be crop" (universal 1:1 identity passthrough, wide gamut / SDR / dmabuf compatible)
    let filter_id = b"crop_filter\0".as_ptr() as *const c_char;

    let priv_filter = unsafe { create_priv(filter_id, filter_name, std::ptr::null_mut()) };
    if !priv_filter.is_null() {
        unsafe {
            filter_add(source, priv_filter);
            (scene_fns.obs_source_release)(priv_filter);
        }
        guard.insert(src_key, priv_filter as usize);
        let name_ptr = unsafe { (scene_fns.obs_source_get_name)(source) };
        let sname = if !name_ptr.is_null() {
            unsafe { std::ffi::CStr::from_ptr(name_ptr).to_string_lossy() }
        } else {
            std::borrow::Cow::Borrowed("unknown")
        };
        println!("[Trust Filter Guard] 🛡️ Attached universal isolated render barrier [crop_filter] to '{}'", sname);
    }
}

unsafe extern "C" fn rust_render_scene_item_cb(
    _scene: *mut c_void,
    item: *mut c_void,
    param: *mut c_void,
) -> bool {
    if item.is_null() || param.is_null() {
        return true;
    }
    let ctx = unsafe { &*(param as *const RenderSceneItemContext) };
    let scene_fns = ctx.scene_fns;
    let gfx_fns = ctx.gfx_fns;

    unsafe {
        let source = (scene_fns.obs_sceneitem_get_source)(item);
        if source.is_null() {
            return true;
        }

        let name_ptr = (scene_fns.obs_source_get_name)(source);
        let name = if !name_ptr.is_null() {
            std::ffi::CStr::from_ptr(name_ptr).to_string_lossy()
        } else {
            std::borrow::Cow::Borrowed("")
        };

        // Skip self to prevent recursion
        let mut is_self = false;
        if let Some(get_id) = scene_fns.obs_source_get_id {
            let id_ptr = get_id(source);
            if !id_ptr.is_null() {
                let id_bytes = std::ffi::CStr::from_ptr(id_ptr).to_bytes();
                if id_bytes == b"theater_throwables_layer" {
                    is_self = true;
                }
            }
        }
        if name.contains("Throwables Layer") {
            is_self = true;
        }
        if is_self {
            return true;
        }

        // 🛡️ Ensure isolated render barrier filter is attached to ALL sources:
        ensure_source_filter_guard(scene_fns, gfx_fns, source);
        // Drawn here only, not also by OBS under the layer (render_gate)
        crate::render_gate::ensure_source_gate(source);

        // OBS's visibility, plus a Show/Hide transition or effect still
        // playing (see item_fx): what to draw, if anything. `draw_source` is the
        // item's source, or its OBS transition while that plays.
        let Some(draw) = crate::item_fx::draw(item, source) else {
            return true;
        };
        let draw_source = draw.source;
        let playing = draw.playing(source);

        // Studio name of this exact item — an extra copy of a source placed
        // twice has its own ("Elgato #2"), so each copy keeps its coordinates
        let item_id = scene_fns.obs_sceneitem_get_id.map(|get| get(item)).unwrap_or(0);
        let item_key = studio_key_for_item(&ctx.scene_name, item_id, name.as_ref());

        // (Loaded by source_video_tick if still empty: never from inside a
        // scene's render, see source_video_render)
        let mut studio_source = crate::state::get_state()
            .studio_sources
            .read()
            .iter()
            .find(|s| s.name == item_key)
            .cloned();
        // Visibility changed in OBS (eye toggle, websocket, Control Panel): the
        // Studio's copy follows. Studio-only hiding (custom Lua scripts, poses
        // restored after a redeem) doesn't change OBS, so it still holds.
        if draw.flipped {
            if let Some(src) = studio_source.as_mut() {
                src.is_visible = draw.visible;
                let mut sources = crate::state::get_state().studio_sources.write();
                if let Some(s) = sources.iter_mut().find(|s| s.name == item_key) {
                    s.is_visible = draw.visible;
                }
            }
        }

        if name.contains("Navigateur") {
            static NAV_LOG_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let c = NAV_LOG_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            if c < 5 || c % 90 == 0 {
                match &studio_source {
                    Some(s) => println!(
                        "[Trust Render Diag] '{}' -> 3D-perspective branch, src.x={:.1} src.y={:.1} src.w={:.1} src.h={:.1} match_obs={} pitch={:.1} yaw={:.1} roll={:.1}",
                        name, s.x, s.y, s.width, s.height, s.match_obs, s.pitch, s.yaw, s.roll
                    ),
                    None => println!("[Trust Render Diag] '{}' -> STANDARD branch (no studio_source match!)", name),
                }
            }
        }

        // Render with 3D perspective divide, pitch, yaw, roll, and resize for ALL sources regardless of target status!
        if let Some(mut src) = studio_source {
            if !src.is_visible && !playing {
                return true;
            }

            let mut bw = (scene_fns.obs_source_get_width)(source) as f32;
            if bw <= 0.0 {
                if let Some(get_base_w) = scene_fns.obs_source_get_base_width {
                    bw = get_base_w(source) as f32;
                }
            }
            let mut bh = (scene_fns.obs_source_get_height)(source) as f32;
            if bh <= 0.0 {
                if let Some(get_base_h) = scene_fns.obs_source_get_base_height {
                    bh = get_base_h(source) as f32;
                }
            }
            // Audio sources (and e.g. an unhooked capture) have no size in OBS
            let obs_has_size = bw > 0.0 && bh > 0.0;
            if bw <= 0.0 {
                bw = if src.width > 0.0 { src.width } else { 100.0 };
            }
            if bh <= 0.0 {
                bh = if src.height > 0.0 { src.height } else { 100.0 };
            }

            let is_script_active = crate::custom_lua::is_custom_script_active()
                || crate::custom_lua::is_source_under_script(&src.name);

            if src.match_obs && !is_script_active {
                let mut pos = ObsVec2 { x: 0.0, y: 0.0 };
                if let Some(get_pos) = scene_fns.obs_sceneitem_get_pos {
                    get_pos(item, &mut pos);
                    src.x = pos.x;
                    src.y = pos.y;
                }
                let mut scale = ObsVec2 { x: 1.0, y: 1.0 };
                if let Some(get_scale) = scene_fns.obs_sceneitem_get_scale {
                    get_scale(item, &mut scale);
                    // Only sources with a real size in OBS take their size from
                    // it. For size-less ones bw/bh fall back to the Studio's own
                    // width/height, so this fed that size back into itself every
                    // frame (width *= scale): an audio source scaled != 1 in OBS
                    // ran off to infinity/NaN, which then broke saved poses.
                    if obs_has_size {
                        src.width = bw * scale.x;
                        src.height = bh * scale.y;
                    }
                }
                if let Some(get_rot) = scene_fns.obs_sceneitem_get_rot {
                    src.roll = get_rot(item);
                }

                // Sync live transform back to state.studio_sources if modified in OBS
                let state = crate::state::get_state();
                let need_update = {
                    let sources = state.studio_sources.read();
                    if let Some(s) = sources.iter().find(|s| s.name == src.name) {
                        (s.x - src.x).abs() > 0.01
                            || (s.y - src.y).abs() > 0.01
                            || (s.width - src.width).abs() > 0.01
                            || (s.height - src.height).abs() > 0.01
                            || (s.roll - src.roll).abs() > 0.01
                    } else {
                        false
                    }
                };
                if need_update {
                    let mut sources = state.studio_sources.write();
                    if let Some(s) = sources.iter_mut().find(|s| s.name == src.name) {
                        s.x = src.x;
                        s.y = src.y;
                        s.width = src.width;
                        s.height = src.height;
                        s.roll = src.roll;
                    }
                }
            }

            // A show/hide effect moves this frame's transform only (after the
            // OBS sync above, so it's never written back)
            if let Some(effect) = draw.effect {
                if src.width <= 0.0 {
                    src.width = bw;
                }
                if src.height <= 0.0 {
                    src.height = bh;
                }
                let (canvas_w, canvas_h) = get_obs_canvas_resolution();
                effect.apply(&mut src, (canvas_w as f32, canvas_h as f32));
            }

            // (An effect can shrink the size to 0: that's drawn as nothing, not full size)
            let w = if src.width > 0.0 || draw.effect.is_some() { src.width.max(0.0) } else { bw };
            let h = if src.height > 0.0 || draw.effect.is_some() { src.height.max(0.0) } else { bh };
            // Screen Shake (see state::ScreenShakeState): this custom compositor
            // always redraws every item into a fixed, full-canvas offscreen
            // texture, so the shake has to nudge each item's *drawn* position
            // here — moving the real OBS scene item's own position has no
            // effect on this render pass (it reads `src.x`/`src.y` from our own
            // `studio_sources` bookkeeping, never the live OBS transform).
            let (shake_x, shake_y) = crate::state::get_state().get_screen_shake_offset();
            let mut cx = src.x + w * 0.5 + shake_x;
            let mut cy = src.y + h * 0.5 + shake_y;
            let sx = w / bw;
            let sy = h / bh;

            // Avatar Impact Reaction. Bounce/PixelDrip are real per-pixel shaders
            // (ported straight from filters/squish_oneshot.lua and
            // filters/pixel_drip.lua — same shader source, compiled by Rust
            // instead of by an attached OBS Lua filter). Jolt stays a cheap
            // jitter+flash approximation since no shader reference exists for it.
            let reaction = crate::state::get_state().get_avatar_reaction_progress(&item_key);
            let mut jolt_flash: Option<(f32, f32, f32, f32, f32)> = None; // (cx, cy, w, h, alpha)
            let mut shader_reaction: Option<crate::state::ActiveReaction> = None;
            if let Some(reaction) = reaction {
                match reaction.kind {
                    crate::physics::ImpactReactionKind::Bounce
                    | crate::physics::ImpactReactionKind::PixelDrip
                    | crate::physics::ImpactReactionKind::Custom(_) => {
                        shader_reaction = Some(reaction);
                    }
                    crate::physics::ImpactReactionKind::Jolt => {
                        let (progress, amount) = (reaction.progress, reaction.amount);
                        // Rapid decaying jitter + a white flash drawn after the source.
                        let decay = (1.0 - progress).max(0.0);
                        let jitter_amp = amount * 10.0 * decay;
                        cx += (progress * 90.0).sin() * jitter_amp;
                        cy += (progress * 130.0 + 1.3).cos() * jitter_amp;
                        jolt_flash = Some((cx, cy, w, h, amount * decay * 0.8));
                    }
                    _ => {}
                }
            }

            let mut flat_tex: *mut c_void = std::ptr::null_mut();
            if src.is_target {
                flat_tex = capture_target_layer_flat(gfx_fns, &src.name, bw as u32, bh as u32, || {
                    (gfx_fns.obs_source_video_render)(draw_source);
                });
            }

            // For shader-based reactions, capture the avatar with its own
            // rotation/perspective already baked in (into a padded buffer, so
            // drips/squish have room to spill) *before* the shader ever runs —
            // see `capture_rotated_source_to_texture` for why this order matters.
            let mut captured_tex: *mut c_void = std::ptr::null_mut();
            let mut reaction_effect: *mut c_void = std::ptr::null_mut();
            let mut cap_w = bw;
            let mut cap_h = bh;
            let mut pad_x = 1.0f32;
            let mut pad_y = 1.0f32;
            if let Some(kind) = shader_reaction.as_ref().map(|r| r.kind) {
                reaction_effect = get_or_create_reaction_effect(gfx_fns, kind);
                if !reaction_effect.is_null() {
                    // Generous room for PixelDrip, Bounce, and Custom shaders (e.g. Swirl in Doom)
                    // to spill out freely into the scene with zero rectangular border clipping.
                    let (px, py) = match kind {
                        crate::physics::ImpactReactionKind::PixelDrip => (2.0, 3.0),
                        _ => (2.5, 2.5),
                    };
                    pad_x = px;
                    pad_y = py;
                    let disp_w = if src.width > 0.0 { src.width } else { bw };
                    let disp_h = if src.height > 0.0 { src.height } else { bh };
                    let max_cap_w = (bw * pad_x).min(1024.0).max(64.0);
                    let max_cap_h = (bh * pad_y).min(1024.0).max(64.0);
                    let min_cap_w = bw.min(256.0).min(max_cap_w);
                    let min_cap_h = bh.min(256.0).min(max_cap_h);
                    let target_cap_w = (disp_w * pad_x * 1.5).max(min_cap_w).min(max_cap_w);
                    let target_cap_h = (disp_h * pad_y * 1.5).max(min_cap_h).min(max_cap_h);
                    cap_w = target_cap_w.round().max(64.0);
                    cap_h = target_cap_h.round().max(64.0);
                    let avatar_w = cap_w / pad_x;
                    let avatar_h = cap_h / pad_y;
                    captured_tex = capture_rotated_source_to_texture(
                        gfx_fns,
                        draw_source,
                        cap_w as u32,
                        cap_h as u32,
                        bw,
                        bh,
                        avatar_w,
                        avatar_h,
                        src.roll,
                        src.pitch,
                        src.yaw,
                        flat_tex,
                    );
                    if captured_tex.is_null() {
                        cap_w = bw;
                        cap_h = bh;
                        pad_x = 1.0;
                        pad_y = 1.0;
                    }
                }
            }
            let using_shader_capture = !captured_tex.is_null();

            let mut reacted_tex: *mut c_void = std::ptr::null_mut();
            let shader_hits = shader_reaction
                .as_ref()
                .map(|r| ReactionShaderHits::new(r, (w, h), (pad_x, pad_y), (src.roll, src.pitch, src.yaw)))
                .unwrap_or_default();
            if using_shader_capture {
                if let Some(ref reaction) = shader_reaction {
                    reacted_tex = capture_reaction_shader_to_texture(
                        gfx_fns,
                        reaction_effect,
                        captured_tex,
                        cap_w as u32,
                        cap_h as u32,
                        reaction,
                        &shader_hits,
                    );
                }
            }

            (gfx_fns.gs_matrix_push)();
            (gfx_fns.gs_blend_state_push)();

            // 1. Move to the center of the rendered bounding box
            if let Some(translate) = gfx_fns.gs_matrix_translate3f {
                translate(cx, cy, 0.0);
            }

            // 2 & 3. Perspective divide + rotation — already baked into
            // `captured_tex` above when a shader reaction is active, so only
            // apply them here for the plain (non-reacting) draw path.
            if !using_shader_capture {
                let d = 1000.0f32;
                if src.pitch.abs() > 1e-3 || src.yaw.abs() > 1e-3 {
                    let mat = GsMatrix4 {
                        x: [1.0, 0.0, 0.0, 0.0],
                        y: [0.0, 1.0, 0.0, 0.0],
                        z: [0.0, 0.0, 1.0, 1.0 / d],
                        t: [0.0, 0.0, 0.0, 1.0],
                    };
                    (gfx_fns.gs_matrix_mul)(&mat as *const GsMatrix4 as *const c_void);
                }
                if let Some(rotaa) = gfx_fns.gs_matrix_rotaa4f {
                    if src.roll.abs() > 1e-4 {
                        rotaa(0.0, 0.0, 1.0, src.roll.to_radians());
                    }
                    if src.pitch.abs() > 1e-3 {
                        rotaa(1.0, 0.0, 0.0, src.pitch.to_radians());
                    }
                    if src.yaw.abs() > 1e-3 {
                        rotaa(0.0, 1.0, 0.0, src.yaw.to_radians());
                    }
                }
            }

            let (draw_sx, draw_sy) = if using_shader_capture {
                ((w * pad_x) / cap_w, (h * pad_y) / cap_h)
            } else {
                (sx, sy)
            };

            // 4. Scale to target width/height
            if let Some(scale_fn) = gfx_fns.gs_matrix_scale3f {
                scale_fn(draw_sx, draw_sy, 1.0);
            }

            // 5. Shift back from center to local origin (the padded capture's
            // own center when using a reaction shader, else the native source).
            if let Some(translate) = gfx_fns.gs_matrix_translate3f {
                translate(-cap_w * 0.5, -cap_h * 0.5, 0.0);
            }

            // 6. Render the target source — either plain, or (when a Bounce/
            // PixelDrip/Swirl reaction captured it above) through that reaction's
            // custom per-pixel shader instead.
            if !reacted_tex.is_null() {
                let effect = (gfx_fns.obs_get_base_effect)(0);
                if !effect.is_null() {
                    while (gfx_fns.gs_effect_loop)(effect, b"Draw\0".as_ptr() as *const c_char) {
                        (gfx_fns.obs_source_draw)(reacted_tex, 0, 0, cap_w as u32, cap_h as u32, false);
                    }
                }
            } else if using_shader_capture {
                if let Some(ref reaction) = shader_reaction {
                    draw_reaction_shader(gfx_fns, reaction_effect, captured_tex, cap_w, cap_h, reaction, &shader_hits);
                }
            } else {
                let is_gc = is_game_capture_source(scene_fns, source);
                if is_gc {
                    if let Some(set_linear) = gfx_fns.gs_set_linear_srgb {
                        set_linear(true);
                    }
                }

                if src.is_target && !flat_tex.is_null() {
                    let effect = (gfx_fns.obs_get_base_effect)(0);
                    if !effect.is_null() {
                        while (gfx_fns.gs_effect_loop)(effect, b"Draw\0".as_ptr() as *const c_char) {
                            (gfx_fns.obs_source_draw)(flat_tex, 0, 0, bw.max(1.0) as u32, bh.max(1.0) as u32, false);
                        }
                    }
                } else {
                    (gfx_fns.obs_source_video_render)(draw_source);
                }

                if is_gc {
                    if let Some(set_linear) = gfx_fns.gs_set_linear_srgb {
                        set_linear(false);
                    }
                }
            }

            (gfx_fns.gs_blend_state_pop)();
            (gfx_fns.gs_matrix_pop)();
            if let Some(enable_srgb) = gfx_fns.gs_enable_framebuffer_srgb {
                enable_srgb(false);
            }
            if let Some(set_linear) = gfx_fns.gs_set_linear_srgb {
                set_linear(false);
            }

            // Jolt's white flash overlay, drawn on top of the just-rendered avatar.
            if let Some((fx, fy, fw, fh, falpha)) = jolt_flash {
                if falpha > 0.01 {
                    draw_solid_quad(gfx_fns, fx, fy, fw, fh, 0.0, pack_argb(1.0, 1.0, 1.0, falpha));
                }
            }

            // Target scanning pipeline:
            // Staged and scanned AFTER canvas rendering is complete to avoid OpenGL pipeline stalls/flicker.
            // If a reaction shader (Swirl, Bounce, Squish, PixelDrip) is active, scan `reacted_tex`
            // so the collision mask, hitbox, and target_textures deform dynamically with the effect.
            // Otherwise, scan `flat_tex` so that OBS source filters (Chroma Key, Crop, Color Correction)
            // are fully reflected in the collision mask and targeting.
            if src.is_target {
                let config = *crate::state::get_state().config.read();
                if !reacted_tex.is_null() {
                    let eff_w = w * pad_x;
                    let eff_h = h * pad_y;
                    let rx = cx - eff_w * 0.5;
                    let ry = cy - eff_h * 0.5;
                    let transform = crate::types::SourceTransform {
                        pos_x: rx,
                        pos_y: ry,
                        scale_x: eff_w / cap_w,
                        scale_y: eff_h / cap_h,
                        crop_left: 0,
                        crop_top: 0,
                        crop_right: 0,
                        crop_bottom: 0,
                        rot_deg: 0.0,
                    };
                    let base_w = (cap_w / pad_x).round().max(1.0) as u32;
                    let base_h = (cap_h / pad_y).round().max(1.0) as u32;
                    stage_and_scan_target_layer_with_transform(
                        gfx_fns,
                        &src.name,
                        reacted_tex,
                        cap_w as u32,
                        cap_h as u32,
                        &transform,
                        0.0,
                        0.0,
                        &config,
                        base_w,
                        base_h,
                    );
                } else if !flat_tex.is_null() {
                    let display_w = if src.width > 0.0 { src.width } else { bw as f32 };
                    let display_h = if src.height > 0.0 { src.height } else { bh as f32 };
                    let scale_x = if bw > 0.0 { display_w / bw } else { 1.0 };
                    let scale_y = if bh > 0.0 { display_h / bh } else { 1.0 };
                    let transform = crate::types::SourceTransform {
                        pos_x: cx - display_w * 0.5,
                        pos_y: cy - display_h * 0.5,
                        scale_x,
                        scale_y,
                        crop_left: 0,
                        crop_top: 0,
                        crop_right: 0,
                        crop_bottom: 0,
                        rot_deg: src.roll,
                    };
                    stage_and_scan_target_layer_with_transform(
                        gfx_fns,
                        &src.name,
                        flat_tex,
                        bw as u32,
                        bh as u32,
                        &transform,
                        src.pitch,
                        src.yaw,
                        &config,
                        bw as u32,
                        bh as u32,
                    );
                }
            }
        } else {
            // Standard OBS scene items (browser sources, background scenes, cameras):
            if let Some(ref src) = studio_source {
                if !src.is_visible && !playing {
                    return true;
                }
            }

            (gfx_fns.gs_matrix_push)();
            (gfx_fns.gs_blend_state_push)();

            // Screen Shake: applied here too (not just the 3D-perspective
            // branch above) as an outermost world-space translate, so items
            // without their own StudioSource entry — plain native scene
            // items drawn via OBS's own draw_transform — shake along with
            // everything else instead of sitting still while other layers move.
            let (shake_x, shake_y) = crate::state::get_state().get_screen_shake_offset();
            if let Some(translate) = gfx_fns.gs_matrix_translate3f {
                translate(shake_x, shake_y, 0.0);
            }

            // Use OBS native draw_transform if available
            if let Some(get_draw_xf) = scene_fns.obs_sceneitem_get_draw_transform {
                let mut draw_xf = GsMatrix4 {
                    x: [0.0; 4],
                    y: [0.0; 4],
                    z: [0.0; 4],
                    t: [0.0; 4],
                };
                get_draw_xf(item, &mut draw_xf);
                (gfx_fns.gs_matrix_mul)(&draw_xf as *const GsMatrix4 as *const c_void);
            } else {
                let mut pos = ObsVec2 { x: 0.0, y: 0.0 };
                if let Some(get_pos) = scene_fns.obs_sceneitem_get_pos {
                    get_pos(item, &mut pos);
                }
                let mut scale = ObsVec2 { x: 1.0, y: 1.0 };
                if let Some(get_scale) = scene_fns.obs_sceneitem_get_scale {
                    get_scale(item, &mut scale);
                }
                let rot = if let Some(get_rot) = scene_fns.obs_sceneitem_get_rot {
                    get_rot(item)
                } else {
                    0.0
                };

                if let Some(translate) = gfx_fns.gs_matrix_translate3f {
                    translate(pos.x, pos.y, 0.0);
                }
                if rot.abs() > 1e-4 {
                    if let Some(rotaa) = gfx_fns.gs_matrix_rotaa4f {
                        rotaa(0.0, 0.0, 1.0, rot.to_radians());
                    }
                }
                if (scale.x - 1.0).abs() > 1e-4 || (scale.y - 1.0).abs() > 1e-4 {
                    if let Some(scale_fn) = gfx_fns.gs_matrix_scale3f {
                        scale_fn(scale.x, scale.y, 1.0);
                    }
                }
            }

            let is_gc = is_game_capture_source(scene_fns, source);
            if is_gc {
                if let Some(set_linear) = gfx_fns.gs_set_linear_srgb {
                    set_linear(true);
                }
            }

            (gfx_fns.obs_source_video_render)(draw_source);

            if is_gc {
                if let Some(set_linear) = gfx_fns.gs_set_linear_srgb {
                    set_linear(false);
                }
            }

            (gfx_fns.gs_blend_state_pop)();
            (gfx_fns.gs_matrix_pop)();
            if let Some(enable_srgb) = gfx_fns.gs_enable_framebuffer_srgb {
                enable_srgb(false);
            }
            if let Some(set_linear) = gfx_fns.gs_set_linear_srgb {
                set_linear(false);
            }
        }
    }

    true
}

/// Paint every active throwable, shatter particle, fluid/fire particle, lightning
/// bolt, and lightning impact bloom directly into the current OBS render target.
///
/// Previously physics/effects were only *stepped* here (see `source_video_tick`)
/// while the actual drawing lived exclusively in the separate Scene Studio egui
/// window (`studio_app.rs`), so a test redeem only ever appeared in that overlay
/// window and never in the real composited "Throwables Layer" source that goes
/// out on stream. Drawing it here means the Scene Studio live preview — which
/// captures its image straight from this source's composited texture — picks it
/// up for free, so its own duplicate egui-side drawing has been removed.
/// Process-wide cache of compiled avatar-reaction shaders (Bounce/PixelDrip),
/// loaded lazily from `filters/*.effect` on first use and reused forever after.
/// Those files are the exact same shaders already proven working as standalone
/// OBS Lua filters (`filters/squish_oneshot.lua`, `filters/pixel_drip.lua`) —
/// this just compiles the same shader source via Rust instead of Lua.
static REACTION_EFFECTS: parking_lot::Mutex<Option<std::collections::HashMap<crate::physics::ImpactReactionKind, usize>>> =
    parking_lot::Mutex::new(None);

/// Reactions whose script was edited: their shader is remade (on the render
/// thread, which holds the graphics context) the next time it's drawn.
static STALE_REACTIONS: parking_lot::Mutex<Vec<crate::physics::ImpactReactionKind>> = parking_lot::Mutex::new(Vec::new());

/// Makes every reaction built from the script at `path` pick up its edits
/// (the Studio's Scripts tab: Reload, or the file changing).
pub fn reload_reaction_script(path: &std::path::Path) {
    let kinds = crate::physics::custom_reactions_for_script(path);
    let mut stale = STALE_REACTIONS.lock();
    for kind in kinds {
        if !stale.contains(&kind) {
            stale.push(kind);
        }
    }
}

/// Dedicated offscreen render target used to capture a reacting avatar's own
/// pixels in isolation (at its native resolution) before distorting them
/// through a custom shader — reused across frames/targets, resized as needed.
static REACTION_TEXRENDER: parking_lot::Mutex<Option<usize>> = parking_lot::Mutex::new(None);
static REACTION_TEXRENDER_DIMS: parking_lot::Mutex<(u32, u32)> = parking_lot::Mutex::new((0, 0));
static REACTION_OUT_TEXRENDER: parking_lot::Mutex<Option<usize>> = parking_lot::Mutex::new(None);
static REACTION_OUT_TEXRENDER_DIMS: parking_lot::Mutex<(u32, u32)> = parking_lot::Mutex::new((0, 0));

/// Reaction shaders that failed to load, so trust_debug.log gets each once
/// rather than every frame the reaction is active.
static REACTION_EFFECT_FAILURES: parking_lot::Mutex<Option<std::collections::HashSet<crate::physics::ImpactReactionKind>>> =
    parking_lot::Mutex::new(None);

fn log_reaction_effect_failure(kind: crate::physics::ImpactReactionKind, msg: &str) {
    eprintln!("{}", msg);
    if REACTION_EFFECT_FAILURES.lock().get_or_insert_with(std::collections::HashSet::new).insert(kind) {
        trust_log(msg);
    }
}

/// Cache of generic uniform defaults extracted from a `Custom` script's own
/// `get_defaults`/`obs_data_set_default_double` calls (e.g. `squish_amount`,
/// `drip_speed`) — Bounce/PixelDrip don't need this since their uniforms are
/// set explicitly in `draw_reaction_shader`.
static REACTION_CUSTOM_DEFAULTS: parking_lot::Mutex<Option<std::collections::HashMap<crate::physics::ImpactReactionKind, Vec<(String, f32)>>>> =
    parking_lot::Mutex::new(None);

/// How many hits a `Custom` script's shader takes: the length of its
/// `uniform float4 hits[N];` (see hits_array_len), by reaction.
static REACTION_HITS_LEN: parking_lot::Mutex<Option<std::collections::HashMap<crate::physics::ImpactReactionKind, usize>>> =
    parking_lot::Mutex::new(None);

/// N in a shader's `uniform float4 hits[N];`. Direct3D copies exactly what
/// Theater sends into the shader's settings, unchecked: sending more than
/// the shader declared would write past them, so it's read from the shader.
pub(crate) fn hits_array_len(shader: &str) -> Option<usize> {
    let key = "uniform float4 hits[";
    let start = shader.find(key)? + key.len();
    let digits = &shader[start..];
    let end = digits.find(']')?;
    digits[..end].trim().parse::<usize>().ok().filter(|&n| n > 0 && n <= crate::state::MAX_REACTION_HITS)
}

/// Compiled `filters/metaball.effect`, hot-reloaded if the file on disk changes.
static METABALL_EFFECT: parking_lot::Mutex<Option<(usize, std::time::SystemTime)>> = parking_lot::Mutex::new(None);
static SPRITE_ALPHA_EFFECT: parking_lot::Mutex<Option<usize>> = parking_lot::Mutex::new(None);

/// Offscreen render target the fluid particles' round influence sprites are
/// additively accumulated into (canvas-sized, reset every frame) before the
/// metaball threshold shader turns that field into a single liquid blob.
static FLUID_FIELD_TEXRENDER: parking_lot::Mutex<Option<usize>> = parking_lot::Mutex::new(None);
static FLUID_FIELD_DIMS: parking_lot::Mutex<(u32, u32)> = parking_lot::Mutex::new((0, 0));

/// Monotonic clock fluid metaball noise animates against — doesn't need to
/// track real per-frame dt, just needs to keep moving.
static METABALL_START: std::sync::LazyLock<std::time::Instant> = std::sync::LazyLock::new(std::time::Instant::now);

/// Private black `color_source`, canvas-sized, recreated only on resize —
/// rendered as a real source (first, before any scene item) so the combined
/// render has a genuine opaque black backdrop instead of relying on each
/// individual source's own background.
static BLACK_BACKDROP_SOURCE: parking_lot::Mutex<Option<usize>> = parking_lot::Mutex::new(None);
static BLACK_BACKDROP_DIMS: parking_lot::Mutex<(u32, u32)> = parking_lot::Mutex::new((0, 0));

/// Per-target-name flat (un-rotated) capture buffers, sized to each source's
/// own native width/height — never the full canvas, so this stays cheap
/// regardless of how many targets exist. Mirrors `capture_rotated_source_to_texture`
/// (proven safe to nest inside the outer composite texrender) but skips the
/// pitch/yaw/roll matrix ops: this capture is the source's own layer, alone,
/// with nothing else ever drawn on it, so it can be scanned directly with
/// plain alpha-based matching. The caller then draws this captured texture
/// back through its own existing transform (translate/rotate/scale), exactly
/// like `draw_reaction_shader` already does for avatar reaction captures —
/// so the visual result on the shared canvas is unchanged.
static TARGET_FLAT_TEXRENDERS: parking_lot::Mutex<Option<std::collections::HashMap<String, usize>>> = parking_lot::Mutex::new(None);
static TARGET_FLAT_DIMS: parking_lot::Mutex<Option<std::collections::HashMap<String, (u32, u32)>>> = parking_lot::Mutex::new(None);

unsafe fn capture_target_layer_flat(
    fns: &ObsGraphicsFns,
    name: &str,
    bw: u32,
    bh: u32,
    draw_fn: impl Fn(),
) -> *mut c_void {
    if bw == 0 || bh == 0 {
        return std::ptr::null_mut();
    }
    let mut tr_map_lock = TARGET_FLAT_TEXRENDERS.lock();
    let mut dims_map_lock = TARGET_FLAT_DIMS.lock();
    let tr_map = tr_map_lock.get_or_insert_with(std::collections::HashMap::new);
    let dims_map = dims_map_lock.get_or_insert_with(std::collections::HashMap::new);

    if dims_map.get(name).copied() != Some((bw, bh)) {
        if let Some(old) = tr_map.remove(name) {
            unsafe { (fns.gs_texrender_destroy)(old as *mut c_void); }
        }
    }
    let texrender = match tr_map.get(name).copied() {
        Some(ptr) => ptr as *mut c_void,
        None => {
            let ptr = unsafe { (fns.gs_texrender_create)(3 /* GS_RGBA */, 0) };
            if ptr.is_null() {
                return std::ptr::null_mut();
            }
            tr_map.insert(name.to_string(), ptr as usize);
            ptr
        }
    };
    dims_map.insert(name.to_string(), (bw, bh));
    drop(tr_map_lock);
    drop(dims_map_lock);

    unsafe {
        (fns.gs_texrender_reset)(texrender);
        if !(fns.gs_texrender_begin)(texrender, bw, bh) {
            return std::ptr::null_mut();
        }
        let clear_color = [0.0f32, 0.0, 0.0, 0.0];
        (fns.gs_clear)(1, clear_color.as_ptr(), 0.0, 0);
        (fns.gs_projection_push)();
        (fns.gs_ortho)(0.0, bw as f32, 0.0, bh as f32, -32768.0, 32768.0);
        (fns.gs_blend_state_push)();
        if let Some(reset_blend) = fns.gs_reset_blend_state {
            reset_blend();
        }
        (fns.gs_enable_blending)(true);
        (fns.gs_matrix_push)();
        (fns.gs_matrix_identity)();
    }

    draw_fn();

    unsafe {
        (fns.gs_matrix_pop)();
        (fns.gs_blend_state_pop)();
        (fns.gs_projection_pop)();
        (fns.gs_texrender_end)(texrender);
        (fns.gs_texrender_get_texture)(texrender)
    }
}

/// A target's two staging surfaces, used in turn (like the Studio preview's):
/// each frame its picture is staged into one while the other — staged the
/// frame before, so the GPU is long done with it — is read back and scanned.
/// Reading back the picture staged in the same frame made OBS's render thread
/// wait for the GPU to finish everything drawn so far (~20 ms a frame: OBS
/// fell to ~30 fps). The mask is one frame behind the picture for it.
/// Each target's scan read-back (see STAGING_DEPTH).
static TARGET_STAGING: parking_lot::Mutex<Option<std::collections::HashMap<String, StagingRing>>> =
    parking_lot::Mutex::new(None);

fn destroy_target_staging(fns: &ObsGraphicsFns) {
    if let Some(mut map) = TARGET_STAGING.lock().take() {
        for ring in map.values_mut() {
            unsafe { ring.destroy(fns) };
        }
    }
}

/// This frame's (surface to stage `name`'s picture into, the oldest staged
/// one to scan, if any), (re)made for its size.
/// Whether `name` is scanned this frame (every scan_every()-th frame); a
/// skipped frame is counted here, a scanned one by its target_staging_turn.
fn target_scan_due(name: &str) -> bool {
    let every = scan_every();
    if every <= 1 {
        return true;
    }
    let mut map = TARGET_STAGING.lock();
    let map = map.get_or_insert_with(std::collections::HashMap::new);
    let ring = map.entry(name.to_string()).or_insert(StagingRing::EMPTY);
    if ring.frames % every == 0 {
        true
    } else {
        ring.skip();
        false
    }
}

unsafe fn target_staging_turn(fns: &ObsGraphicsFns, name: &str, bw: u32, bh: u32) -> Option<(*mut c_void, Option<*mut c_void>)> {
    let mut map = TARGET_STAGING.lock();
    let map = map.get_or_insert_with(std::collections::HashMap::new);
    let ring = map.entry(name.to_string()).or_insert(StagingRing::EMPTY);
    unsafe { ring.turn(fns, bw, bh) }
}

/// The most pixels a target scan reads back and scans. At full size a
/// 2141×1357 avatar cost OBS's render thread 9–19 ms every frame (read-back,
/// scan and copies of ~12 MB) — worse while the CPU was busy streaming — and
/// OBS skipped frames. A collision shape doesn't need that detail: the GPU
/// shrinks the picture first. 400 000 still cost ~4 ms a frame; at 100 000 a
/// target filling the 1920×1080 canvas is scanned at 416×234 (exactly 16:9),
/// a mask pixel per ~4.6 canvas pixels, far finer than anything thrown at it.
/// The Studio lets people pick it (MASK_RESOLUTIONS); this is the default.
const DEFAULT_SCAN_MAX_PIXELS: u64 = 100_000;

/// The mask resolutions the Studio offers: (name, most pixels scanned).
pub const MASK_RESOLUTIONS: [(&str, u64); 5] = [
    ("Low", 50_000),
    ("Normal", DEFAULT_SCAN_MAX_PIXELS),
    ("High", 200_000),
    ("Very high", 400_000),
    ("Full size (slow)", u64::MAX),
];

/// The chosen mask resolution (most pixels a target is scanned at), read by
/// the render thread without a lock.
static SCAN_MAX_PIXELS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(DEFAULT_SCAN_MAX_PIXELS);

fn settings_file() -> std::path::PathBuf {
    crate::paths::file("theater_settings.json")
}

/// How often the Studio offers to scan targets: (name, every how many frames).
/// Less often is lighter on OBS's render thread; the mask then follows a
/// moving target a little later.
pub const MASK_RATES: [(&str, u64); 4] = [
    ("Every frame", 1),
    ("Every 2nd frame", 2),
    ("Every 3rd frame", 3),
    ("Every 5th frame", 5),
];

/// Targets are scanned every this many frames (the render thread reads it
/// without a lock).
static SCAN_EVERY: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

pub fn mask_pixels() -> u64 {
    SCAN_MAX_PIXELS.load(std::sync::atomic::Ordering::Relaxed)
}

pub fn scan_every() -> u64 {
    SCAN_EVERY.load(std::sync::atomic::Ordering::Relaxed).max(1)
}

fn read_settings() -> serde_json::Value {
    std::fs::read_to_string(settings_file())
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .filter(|v| v.is_object())
        .unwrap_or_else(|| serde_json::json!({}))
}

pub(crate) fn save_setting(key: &str, value: serde_json::Value) {
    let path = settings_file();
    let mut settings = read_settings();
    settings[key] = value;
    let written = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string()).and_then(|s| std::fs::write(&path, s).map_err(|e| e.to_string()));
    if let Err(e) = written {
        trust_log(&format!("[Trust Studio] ⚠️ Could not save {} to {}: {}", key, path.display(), e));
    }
}

/// Reads the saved mask resolution, scan rate and detection (when OBS loads
/// Theater).
pub fn load_mask_settings() {
    let settings = read_settings();
    if let Some(n) = settings.get("mask_pixels").and_then(|n| n.as_u64()).filter(|n| *n >= 1_000) {
        SCAN_MAX_PIXELS.store(n, std::sync::atomic::Ordering::Relaxed);
    }
    if let Some(n) = settings.get("mask_every").and_then(|n| n.as_u64()).filter(|n| (1..=60).contains(n)) {
        SCAN_EVERY.store(n, std::sync::atomic::Ordering::Relaxed);
    }
    if let Some(detection) = settings.get("detection") {
        *crate::state::get_state().config.write() = detection_from_json(detection);
    }
    if let Some(preview) = settings.get("studio_preview") {
        if let Some(width) = preview.get("width").and_then(|n| n.as_u64()).filter(|n| (320..=3840).contains(n)) {
            PREVIEW_MAX_WIDTH.store(width as u32, std::sync::atomic::Ordering::Relaxed);
        }
        if let Some(every) = preview.get("every").and_then(|n| n.as_u64()).filter(|n| (1..=4).contains(n)) {
            PREVIEW_EVERY.store(every, std::sync::atomic::Ordering::Relaxed);
        }
    }
    if let Some(screen) = settings.get("studio_screen") {
        if let Some(on) = screen.get("on").and_then(|b| b.as_bool()) {
            STUDIO_SCREEN.store(on, std::sync::atomic::Ordering::Relaxed);
        }
        if let Some(live) = screen.get("even_live").and_then(|b| b.as_bool()) {
            STUDIO_SCREEN_LIVE.store(live, std::sync::atomic::Ordering::Relaxed);
        }
    }
}

// ─── The "Studio in use" screen ─────────────────────────────────────────────

/// While the Studio is open, OBS shows a "Studio in use" screen (the layer
/// still draws the scene, for the Studio: masks, redeems and physics carry
/// on). On unless turned off; not while OBS is live unless asked.
static STUDIO_SCREEN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);
static STUDIO_SCREEN_LIVE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static STUDIO_SCREEN_CANVAS: parking_lot::Mutex<crate::script_draw::Canvas> =
    parking_lot::Mutex::new(crate::script_draw::Canvas::new());

/// (shown, even while live)
pub fn studio_screen_settings() -> (bool, bool) {
    (
        STUDIO_SCREEN.load(std::sync::atomic::Ordering::Relaxed),
        STUDIO_SCREEN_LIVE.load(std::sync::atomic::Ordering::Relaxed),
    )
}

pub fn set_studio_screen(on: bool, even_live: bool) {
    STUDIO_SCREEN.store(on, std::sync::atomic::Ordering::Relaxed);
    STUDIO_SCREEN_LIVE.store(even_live, std::sync::atomic::Ordering::Relaxed);
    save_setting("studio_screen", serde_json::json!({ "on": on, "even_live": even_live }));
}

/// Streaming, recording or the virtual camera on: what OBS shows is seen.
fn obs_live() -> bool {
    type Active = unsafe extern "C" fn() -> bool;
    static FNS: std::sync::OnceLock<[Option<Active>; 3]> = std::sync::OnceLock::new();
    let fns = FNS.get_or_init(|| {
        [
            crate::script_filters::sym(c"obs_frontend_streaming_active"),
            crate::script_filters::sym(c"obs_frontend_recording_active"),
            crate::script_filters::sym(c"obs_frontend_virtualcam_active"),
        ]
    });
    fns.iter().flatten().any(|active| unsafe { active() })
}

/// This frame, OBS gets the "Studio in use" screen rather than the scene.
fn studio_screen_now() -> bool {
    STUDIO_SCREEN.load(std::sync::atomic::Ordering::Relaxed)
        && crate::studio_app::is_studio_in_use()
        && (STUDIO_SCREEN_LIVE.load(std::sync::atomic::Ordering::Relaxed) || !obs_live())
}

/// Black, and what's going on, centred. In the layer's render.
unsafe fn draw_studio_screen(cw: u32, ch: u32) {
    use crate::script_draw::{Cmd, TextKey, measure};
    let (w, h) = (cw as f32, ch as f32);
    let line = |text: &str, px: f32, weight: u16, color: u32| {
        let (tw, th, _) = measure(text, "Inter", px, weight);
        let key = TextKey { text: text.to_string(), font: "Inter".into(), size: (px * 64.0) as u32, weight, color, shadow: false };
        (key, tw, th)
    };
    let (title, title_w, title_h) = line("Studio in use", (h * 0.07).max(16.0), 700, 0xFFFFFFFF);
    let note_px = (h * 0.026).max(10.0);
    let (note, note_w, note_h) = line("Theater is showing the scene in its Studio: close or minimize it to bring it back here.", note_px, 400, 0xFF9CA3AF);
    let top = (h - title_h - note_px - note_h) / 2.0;
    let cmds = [
        Cmd::Rect { x: 0.0, y: 0.0, w, h, fill: 0xFF000000, radius: 0.0, border: [0.0; 4], border_color: 0, soft: 0.0, alpha: 1.0 },
        Cmd::Text { x: (w - title_w) / 2.0, y: top, key: title, alpha: 1.0 },
        Cmd::Text { x: (w - note_w) / 2.0, y: top + title_h + note_px, key: note, alpha: 1.0 },
    ];
    unsafe { crate::script_draw::render(&mut STUDIO_SCREEN_CANVAS.lock(), &cmds, (cw, ch)) };
}

/// Sets the mask resolution: the next scans use it, and it's saved.
pub fn set_mask_pixels(n: u64) {
    let n = n.max(1_000);
    SCAN_MAX_PIXELS.store(n, std::sync::atomic::Ordering::Relaxed);
    save_setting("mask_pixels", serde_json::json!(n));
}

/// A whole-number setting from theater_settings.json, if it's there.
pub fn setting_u64(key: &str) -> Option<u64> {
    read_settings().get(key).and_then(|n| n.as_u64())
}

/// The Studio's saved refresh rate (frames a second), if there is one.
pub fn saved_studio_fps() -> Option<u32> {
    read_settings().get("studio_fps").and_then(|n| n.as_u64()).filter(|n| (15..=240).contains(n)).map(|n| n as u32)
}

/// Saves the Studio's refresh rate.
pub fn save_studio_fps(fps: u32) {
    save_setting("studio_fps", serde_json::json!(fps.clamp(15, 240)));
}

/// Sets how often targets are scanned (every `n` frames), and saves it.
pub fn set_scan_every(n: u64) {
    let n = n.clamp(1, 60);
    SCAN_EVERY.store(n, std::sync::atomic::Ordering::Relaxed);
    save_setting("mask_every", serde_json::json!(n));
}

/// Sets how targets' outlines are found (the next scans use it), and saves it.
pub fn set_detection(config: crate::types::TrackingConfig) {
    *crate::state::get_state().config.write() = config;
    save_setting("detection", detection_to_json(&config));
}

fn detection_to_json(c: &crate::types::TrackingConfig) -> serde_json::Value {
    serde_json::json!({
        "mode": c.mode,
        "alpha_threshold": c.alpha_threshold,
        "chroma": [c.chroma_r, c.chroma_g, c.chroma_b],
        "threshold": c.threshold,
    })
}

/// Saved detection settings; what's missing or out of range keeps its default.
fn detection_from_json(v: &serde_json::Value) -> crate::types::TrackingConfig {
    let mut c = crate::types::TrackingConfig::default();
    let byte = |v: Option<&serde_json::Value>| v.and_then(|n| n.as_u64()).filter(|n| *n <= 255).map(|n| n as u8);
    if let Some(mode) = byte(v.get("mode")).filter(|m| *m <= 2) {
        c.mode = mode;
    }
    if let Some(a) = byte(v.get("alpha_threshold")) {
        c.alpha_threshold = a;
    }
    if let Some(t) = byte(v.get("threshold")) {
        c.threshold = t;
    }
    if let Some([r, g, b]) = v.get("chroma").and_then(|a| a.as_array()).map(|a| [0, 1, 2].map(|i| byte(a.get(i)))) {
        if let (Some(r), Some(g), Some(b)) = (r, g, b) {
            (c.chroma_r, c.chroma_g, c.chroma_b) = (r, g, b);
        }
    }
    c
}

/// The size a (bw × bh) target picture is scanned at: its own, or shrunk
/// (keeping its shape) to the chosen mask resolution. From its own size only,
/// not how it's shown: an animation resizing it would remake the scan's GPU
/// surfaces every frame.
fn scan_size(bw: u32, bh: u32) -> (u32, u32) {
    scan_size_within(bw, bh, mask_pixels())
}

/// scan_size, for a mask resolution of `max_pixels`.
fn scan_size_within(bw: u32, bh: u32, max_pixels: u64) -> (u32, u32) {
    let pixels = bw as u64 * bh as u64;
    if pixels <= max_pixels {
        return (bw, bh);
    }
    // Exactly its ratio when it has a small one (16:9 → 416×234): square
    // scan pixels, the same scale both ways
    fn gcd(a: u64, b: u64) -> u64 {
        if b == 0 { a } else { gcd(b, a % b) }
    }
    let g = gcd(bw as u64, bh as u64);
    let (rw, rh) = (bw as u64 / g, bh as u64 / g);
    let k = ((max_pixels as f64 / (rw * rh) as f64).sqrt().floor() as u64).min(g);
    // …unless that ratio is so coarse it would waste much of the budget
    if k >= 1 && rw * k * rh * k * 10 >= max_pixels * 9 {
        return ((rw * k) as u32, (rh * k) as u32);
    }
    let f = (max_pixels as f64 / pixels as f64).sqrt();
    (((bw as f64 * f).floor() as u32).clamp(1, bw), ((bh as f64 * f).floor() as u32).clamp(1, bh))
}

/// A target picture's scan `transform` and source size (`base`, which the
/// Studio pads against) for scanning it shrunk from `full` to `scanned`: each
/// scanned pixel covers more of the canvas, the base shrinks alike.
fn shrunk_scan_transform(
    transform: &crate::types::SourceTransform,
    base: (u32, u32),
    full: (u32, u32),
    scanned: (u32, u32),
) -> (crate::types::SourceTransform, (u32, u32)) {
    if full == scanned {
        return (*transform, base);
    }
    let fx = full.0 as f32 / scanned.0.max(1) as f32;
    let fy = full.1 as f32 / scanned.1.max(1) as f32;
    let mut t = *transform;
    t.scale_x *= fx;
    t.scale_y *= fy;
    let base = (((base.0 as f32 / fx).round() as u32).max(1), ((base.1 as f32 / fy).round() as u32).max(1));
    (t, base)
}

/// Per target: the texrender its picture is shrunk into for scanning, and its size.
static TARGET_SCAN_TEXRENDERS: parking_lot::Mutex<Option<std::collections::HashMap<String, (usize, (u32, u32))>>> =
    parking_lot::Mutex::new(None);

fn destroy_target_scan_texrenders(fns: &ObsGraphicsFns) {
    if let Some(map) = TARGET_SCAN_TEXRENDERS.lock().take() {
        for (texrender, _) in map.into_values() {
            unsafe { (fns.gs_texrender_destroy)(texrender as *mut c_void) };
        }
    }
}

/// `tex`, a target's picture, drawn by the GPU at (sw × sh) for scanning —
/// its alpha copied, not blended. Null if it can't be.
unsafe fn shrink_for_scan(fns: &ObsGraphicsFns, name: &str, tex: *mut c_void, sw: u32, sh: u32) -> *mut c_void {
    let effect = unsafe { (fns.obs_get_base_effect)(0) };
    if effect.is_null() {
        return std::ptr::null_mut();
    }
    let texrender = {
        let mut map = TARGET_SCAN_TEXRENDERS.lock();
        let map = map.get_or_insert_with(std::collections::HashMap::new);
        match map.get(name).copied() {
            Some((ptr, dims)) if dims == (sw, sh) => ptr as *mut c_void,
            existing => {
                if let Some((old, _)) = existing {
                    unsafe { (fns.gs_texrender_destroy)(old as *mut c_void) };
                    map.remove(name);
                }
                let ptr = unsafe { (fns.gs_texrender_create)(3 /* GS_RGBA */, 0) };
                if ptr.is_null() {
                    return std::ptr::null_mut();
                }
                map.insert(name.to_string(), (ptr as usize, (sw, sh)));
                ptr
            }
        }
    };
    unsafe {
        (fns.gs_texrender_reset)(texrender);
        if !(fns.gs_texrender_begin)(texrender, sw, sh) {
            return std::ptr::null_mut();
        }
        let clear_color = [0.0f32, 0.0, 0.0, 0.0];
        (fns.gs_clear)(1, clear_color.as_ptr(), 0.0, 0);
        (fns.gs_projection_push)();
        (fns.gs_ortho)(0.0, sw as f32, 0.0, sh as f32, -32768.0, 32768.0);
        (fns.gs_blend_state_push)();
        (fns.gs_enable_blending)(false);
        (fns.gs_matrix_push)();
        (fns.gs_matrix_identity)();
        while (fns.gs_effect_loop)(effect, b"Draw\0".as_ptr() as *const c_char) {
            (fns.obs_source_draw)(tex, 0, 0, sw, sh, false);
        }
        (fns.gs_matrix_pop)();
        (fns.gs_blend_state_pop)();
        (fns.gs_projection_pop)();
        (fns.gs_texrender_end)(texrender);
        (fns.gs_texrender_get_texture)(texrender)
    }
}

/// Stages a captured layer (either flat or reaction-deformed) back to CPU and scans it
/// into `state.masks`, `state.hitboxes`, and `state.target_textures` — at
/// scan_size, the GPU shrinking it first when it's bigger.
unsafe fn stage_and_scan_target_layer_with_transform(
    fns: &ObsGraphicsFns,
    name: &str,
    tex: *mut c_void,
    bw: u32,
    bh: u32,
    transform: &crate::types::SourceTransform,
    pitch: f32,
    yaw: f32,
    config: &crate::types::TrackingConfig,
    base_w: u32,
    base_h: u32,
) {
    if tex.is_null() || bw == 0 || bh == 0 {
        return;
    }
    // Scanned every few frames, when chosen so: the mask stays as it is between
    if !target_scan_due(name) {
        return;
    }
    let _timer = ScanTimer(std::time::Instant::now());

    let (sw, sh) = scan_size(bw, bh);
    let tex = if (sw, sh) == (bw, bh) {
        tex
    } else {
        let small = unsafe { shrink_for_scan(fns, name, tex, sw, sh) };
        if small.is_null() {
            return;
        }
        small
    };
    let (transform, (base_w, base_h)) = shrunk_scan_transform(transform, (base_w, base_h), (bw, bh), (sw, sh));
    let (bw, bh) = (sw, sh);
    let transform = &transform;

    let Some((stage_into, read_back)) = (unsafe { target_staging_turn(fns, name, bw, bh) }) else { return };

    unsafe {
        (fns.gs_stage_texture)(stage_into, tex);
        // The picture staged STAGING_DEPTH - 1 frames ago (none yet on a target's first frames)
        let Some(stagesurf) = read_back else { return };
        let readback_started = std::time::Instant::now();
        let mut data_ptr: *mut u8 = std::ptr::null_mut();
        let mut linesize: u32 = 0;
        let mapped = (fns.gs_stagesurface_map)(stagesurf, &mut data_ptr, &mut linesize);
        let readback = readback_started.elapsed();
        with_render_times(|t| t.scan_readback += readback);
        if mapped {
            if !data_ptr.is_null() && linesize >= bw * 4 {
                let stride = linesize as usize;
                let w = bw as usize;
                let h = bh as usize;
                static REUSABLE_STAGING_BUF: parking_lot::Mutex<Vec<u8>> = parking_lot::Mutex::new(Vec::new());
                let mut buf_lock = REUSABLE_STAGING_BUF.lock();
                let needed_len = w * h * 4;
                if buf_lock.len() != needed_len {
                    buf_lock.resize(needed_len, 0);
                }
                if stride == w * 4 {
                    buf_lock.copy_from_slice(std::slice::from_raw_parts(data_ptr, needed_len));
                } else {
                    for row in 0..h {
                        let src_offset = row * stride;
                        let dst_offset = row * (w * 4);
                        let src_slice = std::slice::from_raw_parts(data_ptr.add(src_offset), w * 4);
                        buf_lock[dst_offset..dst_offset + (w * 4)].copy_from_slice(src_slice);
                    }
                }
                note_target_picture(name, &buf_lock);
                scan_target_source_layer(name, &buf_lock, bw, bh, bw * 4, transform, pitch, yaw, config, base_w, base_h);
            }
            (fns.gs_stagesurface_unmap)(stagesurf);
        }
    }
}

unsafe fn stage_and_scan_target_layer(
    fns: &ObsGraphicsFns,
    src: &crate::StudioSource,
    tex: *mut c_void,
    bw: u32,
    bh: u32,
    config: &crate::types::TrackingConfig,
) {
    let display_w = if src.width > 0.0 { src.width } else { bw as f32 };
    let display_h = if src.height > 0.0 { src.height } else { bh as f32 };
    let scale_x = if bw > 0 { display_w / bw as f32 } else { 1.0 };
    let scale_y = if bh > 0 { display_h / bh as f32 } else { 1.0 };
    let transform = crate::types::SourceTransform {
        pos_x: src.x,
        pos_y: src.y,
        scale_x,
        scale_y,
        crop_left: 0,
        crop_top: 0,
        crop_right: 0,
        crop_bottom: 0,
        rot_deg: src.roll,
    };
    unsafe {
        stage_and_scan_target_layer_with_transform(fns, &src.name, tex, bw, bh, &transform, src.pitch, src.yaw, config, bw, bh);
    }
}

unsafe fn get_or_create_black_backdrop(fns: &ObsGraphicsFns, cw: u32, ch: u32) -> *mut c_void {
    let (Some(create_priv), Some(data_create), Some(set_int)) =
        (fns.obs_source_create_private, fns.obs_data_create, fns.obs_data_set_int)
    else {
        return std::ptr::null_mut();
    };

    let mut src_lock = BLACK_BACKDROP_SOURCE.lock();
    let mut dims_lock = BLACK_BACKDROP_DIMS.lock();

    if dims_lock.0 == cw && dims_lock.1 == ch {
        if let Some(ptr) = *src_lock {
            return ptr as *mut c_void;
        }
    }

    if let Some(old) = src_lock.take() {
        unsafe { (fns.obs_source_release)(old as *mut c_void); }
    }

    let settings = unsafe { data_create() };
    if settings.is_null() {
        return std::ptr::null_mut();
    }
    unsafe {
        set_int(settings, b"color\0".as_ptr() as *const c_char, 0xFF000000u32 as i64);
        set_int(settings, b"width\0".as_ptr() as *const c_char, cw as i64);
        set_int(settings, b"height\0".as_ptr() as *const c_char, ch as i64);
    }
    let src = unsafe {
        create_priv(
            b"color_source\0".as_ptr() as *const c_char,
            b"trust_black_backdrop\0".as_ptr() as *const c_char,
            settings,
        )
    };
    if let Some(data_release) = fns.obs_data_release {
        unsafe { data_release(settings); }
    }

    if !src.is_null() {
        *src_lock = Some(src as usize);
        *dims_lock = (cw, ch);
    }
    src
}

/// Snapshot of "everything drawn to the current render target so far this
/// frame" — i.e. the rest of the scene, since the Throwables Layer draws on
/// top of it — used so the fluid metaball can sample and warp what's
/// actually behind it for a real visual-distortion/refraction look, instead
/// of just a flat tinted shape. GPU APIs don't allow simultaneously reading
/// from and rendering into the same texture, so this copy is required
/// before sampling; recreated only on canvas resize.
static BACKDROP_COPY_TEX: parking_lot::Mutex<Option<usize>> = parking_lot::Mutex::new(None);
static BACKDROP_COPY_DIMS: parking_lot::Mutex<(u32, u32)> = parking_lot::Mutex::new((0, 0));

/// Returns null (caller falls back to the flat-color look) if the required
/// functions aren't available on this libobs build, or nothing is currently
/// bound as a render target.
unsafe fn capture_backdrop_copy(fns: &ObsGraphicsFns, canvas_w: u32, canvas_h: u32) -> *mut c_void {
    let (Some(get_rt), Some(copy_tex), Some(create)) =
        (fns.gs_get_render_target, fns.gs_copy_texture, fns.gs_texture_create)
    else {
        return std::ptr::null_mut();
    };
    if canvas_w == 0 || canvas_h == 0 {
        return std::ptr::null_mut();
    }
    let current_rt = unsafe { get_rt() };
    if current_rt.is_null() {
        return std::ptr::null_mut();
    }

    let mut tex_lock = BACKDROP_COPY_TEX.lock();
    let mut dims_lock = BACKDROP_COPY_DIMS.lock();
    if tex_lock.is_none() || *dims_lock != (canvas_w, canvas_h) {
        if let Some(old) = tex_lock.take() {
            if let Some(destroy) = fns.gs_texture_destroy {
                unsafe { destroy(old as *mut c_void); }
            }
        }
        let blank = vec![0u8; (canvas_w as usize) * (canvas_h as usize) * 4];
        let mip_levels: [*const u8; 1] = [blank.as_ptr()];
        let ptr = unsafe { create(canvas_w, canvas_h, 3 /* GS_RGBA */, 1, mip_levels.as_ptr(), 0) };
        if ptr.is_null() {
            return std::ptr::null_mut();
        }
        *tex_lock = Some(ptr as usize);
        *dims_lock = (canvas_w, canvas_h);
    }
    let dst = tex_lock.unwrap() as *mut c_void;
    unsafe { copy_tex(dst, current_rt); }
    dst
}

/// Extracts the shader source between the `[[` `]]` pair that follows a
/// `shader_code = ` assignment in an OBS Lua filter script — the same shader
/// text that script already passes to `obs.gs_effect_create` itself.
pub(crate) fn extract_shader_code_from_lua(lua_source: &str) -> Option<String> {
    let idx = lua_source.find("shader_code")?;
    let after = &lua_source[idx..];
    let open_rel = after.find("[[")?;
    let body = &after[open_rel + 2..];
    let close_rel = body.find("]]")?;
    Some(body[..close_rel].to_string())
}

/// Extracts `obs.obs_data_set_default_double(settings, "name", value)` pairs
/// from a Lua filter script — a generic way to discover a custom script's own
/// tunable uniforms (and reasonable defaults for them) without hardcoding
/// per-script knowledge the way Bounce/PixelDrip's uniforms are hardcoded.
pub(crate) fn extract_default_floats_from_lua(lua_source: &str) -> Vec<(String, f32)> {
    let mut out = Vec::new();
    let key = "obs_data_set_default_double(settings, \"";
    let mut search_from = 0usize;
    while let Some(rel_pos) = lua_source[search_from..].find(key) {
        let pos = search_from + rel_pos;
        let after = &lua_source[pos + key.len()..];
        if let Some(name_end) = after.find('"') {
            let name = after[..name_end].to_string();
            let after_name = &after[name_end + 1..];
            if let Some(close_paren) = after_name.find(')') {
                let args_tail = &after_name[..close_paren];
                if let Some(comma) = args_tail.find(',') {
                    let val_str = args_tail[comma + 1..].trim();
                    if let Ok(val) = val_str.parse::<f32>() {
                        out.push((name, val));
                    }
                }
            }
        }
        search_from = pos + key.len();
    }
    out
}

/// Gives each number, vector and bool setting of `effect` the value 0. Until a
/// setting has a value, Direct3D refuses to draw with the shader ("Not all
/// shader parameters were set") and the reacting target vanishes — so a
/// script's settings that Theater doesn't set start at 0 instead. Textures and
/// matrices are OBS's own to set.
unsafe fn zero_effect_params(fns: &ObsGraphicsFns, effect: *mut c_void) {
    let (Some(count), Some(by_idx), Some(info), Some(set_val)) = (
        fns.gs_effect_get_num_params,
        fns.gs_effect_get_param_by_idx,
        fns.gs_effect_get_param_info,
        fns.gs_effect_set_val,
    ) else {
        return;
    };
    let zeros = [0u8; 16];
    for i in 0..unsafe { count(effect) } {
        let param = unsafe { by_idx(effect, i) };
        if param.is_null() {
            continue;
        }
        let mut param_info = GsEffectParamInfo { name: std::ptr::null(), kind: 0 };
        unsafe { info(param, &mut param_info) };
        let size = match param_info.kind {
            1..=3 => 4,   // bool, float, int
            5 | 8 => 8,   // vec2, int2
            6 | 9 => 12,  // vec3, int3
            7 | 10 => 16, // vec4, int4
            _ => continue, // unknown, string, matrix, texture
        };
        unsafe { set_val(param, zeros.as_ptr() as *const c_void, size) };
    }
}

/// Compiles (once) and returns the custom shader for a Bounce/PixelDrip/Custom
/// avatar reaction. Returns null if the file is missing or fails to compile —
/// callers must fall back to the plain (undistorted) draw in that case.
unsafe fn get_or_create_reaction_effect(fns: &ObsGraphicsFns, kind: crate::physics::ImpactReactionKind) -> *mut c_void {
    // Its script was edited: drop the old shader (and its read defaults) to remake both
    let stale = {
        let mut stale = STALE_REACTIONS.lock();
        stale.iter().position(|k| *k == kind).map(|i| stale.remove(i)).is_some()
    };
    if stale {
        let old = REACTION_EFFECTS.lock().as_mut().and_then(|m| m.remove(&kind));
        if let (Some(ptr), Some(destroy)) = (old, fns.gs_effect_destroy) {
            unsafe { destroy(ptr as *mut c_void) };
        }
        if let Some(failed) = REACTION_EFFECT_FAILURES.lock().as_mut() {
            failed.remove(&kind);
        }
    }
    {
        let cache = REACTION_EFFECTS.lock();
        if let Some(map) = cache.as_ref() {
            if let Some(&ptr) = map.get(&kind) {
                return ptr as *mut c_void;
            }
        }
    }
    let Some(create) = fns.gs_effect_create else { return std::ptr::null_mut(); };

    let (path, is_lua) = match kind {
        crate::physics::ImpactReactionKind::Bounce => (crate::studio_app::get_filters_dir().join("squish_oneshot.effect").to_string_lossy().into_owned(), false),
        crate::physics::ImpactReactionKind::PixelDrip => (crate::studio_app::get_filters_dir().join("pixel_drip.effect").to_string_lossy().into_owned(), false),
        crate::physics::ImpactReactionKind::Custom(id) => {
            match crate::physics::custom_reaction_script_path(id) {
                Some(p) => (p, true),
                None => return std::ptr::null_mut(),
            }
        }
        _ => return std::ptr::null_mut(),
    };

    let file_text = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            log_reaction_effect_failure(kind, &format!("[Trust] Could not read reaction shader '{}': {}", path, e));
            return std::ptr::null_mut();
        }
    };
    let source = if is_lua {
        match extract_shader_code_from_lua(&file_text) {
            Some(s) => s,
            None => {
                log_reaction_effect_failure(kind, &format!("[Trust] Could not find a `shader_code = [[ ... ]]` block in '{}'", path));
                return std::ptr::null_mut();
            }
        }
    } else {
        file_text.clone()
    };
    if is_lua {
        let defaults = extract_default_floats_from_lua(&file_text);
        let mut dcache = REACTION_CUSTOM_DEFAULTS.lock();
        dcache.get_or_insert_with(std::collections::HashMap::new).insert(kind, defaults);
        let mut lens = REACTION_HITS_LEN.lock();
        let lens = lens.get_or_insert_with(std::collections::HashMap::new);
        match hits_array_len(&source) {
            Some(n) => lens.insert(kind, n),
            None => lens.remove(&kind),
        };
    }

    let Ok(c_source) = std::ffi::CString::new(source) else { return std::ptr::null_mut(); };
    let effect = unsafe { create(c_source.as_ptr(), std::ptr::null(), std::ptr::null_mut()) };
    if effect.is_null() {
        log_reaction_effect_failure(kind, &format!("[Trust] Failed to compile reaction shader for {:?} ('{}')", kind, path));
        return std::ptr::null_mut();
    }
    unsafe { zero_effect_params(fns, effect) };
    let mut cache = REACTION_EFFECTS.lock();
    cache.get_or_insert_with(std::collections::HashMap::new).insert(kind, effect as usize);
    effect
}

/// Renders `source` alone (no scene transform applied) into a reusable
/// offscreen texture at `(w, h)`, returning that texture — used to capture a
/// reacting avatar's own pixels before distorting them through a custom
/// shader. The texrender is destroyed and recreated only when size changes.
/// Renders `source` into a padded offscreen buffer of size `(cap_w, cap_h)`,
/// with the avatar's own perspective+rotation (pitch/yaw/roll) already baked
/// in — centered at native `(bw, bh)` resolution within that padding. This is
/// what makes a reaction shader rotation-correct: "down" in the shader's UV
/// space now means true screen-down regardless of the avatar's roll, because
/// the rotation already happened here as a pixel-level operation, *before*
/// the shader ever sees the texture. Scale to final on-screen size is
/// deliberately left for the caller's own outer transform, applied *after*
/// the shader runs, so the shader always operates near the avatar's native
/// resolution regardless of how big it's ultimately displayed.
unsafe fn capture_rotated_source_to_texture(
    fns: &ObsGraphicsFns,
    source: *mut c_void,
    cap_w: u32,
    cap_h: u32,
    bw: f32,
    bh: f32,
    avatar_w: f32,
    avatar_h: f32,
    roll: f32,
    pitch: f32,
    yaw: f32,
    flat_tex: *mut c_void,
) -> *mut c_void {
    if cap_w == 0 || cap_h == 0 {
        return std::ptr::null_mut();
    }
    let mut tr_lock = REACTION_TEXRENDER.lock();
    let mut dims_lock = REACTION_TEXRENDER_DIMS.lock();

    if tr_lock.is_none() || *dims_lock != (cap_w, cap_h) {
        if let Some(old) = tr_lock.take() {
            unsafe { (fns.gs_texrender_destroy)(old as *mut c_void); }
        }
        let ptr = unsafe { (fns.gs_texrender_create)(3 /* GS_RGBA */, 0) };
        if ptr.is_null() {
            return std::ptr::null_mut();
        }
        *tr_lock = Some(ptr as usize);
        *dims_lock = (cap_w, cap_h);
    }
    let texrender = tr_lock.unwrap() as *mut c_void;

    unsafe {
        (fns.gs_texrender_reset)(texrender);
        if !(fns.gs_texrender_begin)(texrender, cap_w, cap_h) {
            return std::ptr::null_mut();
        }
        let clear_color = [0.0f32, 0.0, 0.0, 0.0];
        (fns.gs_clear)(1, clear_color.as_ptr(), 0.0, 0);
        (fns.gs_projection_push)();
        (fns.gs_ortho)(0.0, cap_w as f32, 0.0, cap_h as f32, -32768.0, 32768.0);
        (fns.gs_blend_state_push)();
        if let Some(reset_blend) = fns.gs_reset_blend_state {
            reset_blend();
        }
        (fns.gs_enable_blending)(true);

        (fns.gs_matrix_push)();
        (fns.gs_matrix_identity)();
        // Same perspective-divide + rotation sequence as the normal avatar
        // draw (see rust_render_scene_item_cb steps 1-3) — just targeting this
        // padded capture instead of the main composite, and stopping short of
        // the final canvas scale/position (the caller applies those after).
        if let Some(translate) = fns.gs_matrix_translate3f {
            translate(cap_w as f32 * 0.5, cap_h as f32 * 0.5, 0.0);
        }
        let d = 1000.0f32;
        if pitch.abs() > 1e-3 || yaw.abs() > 1e-3 {
            let mat = GsMatrix4 {
                x: [1.0, 0.0, 0.0, 0.0],
                y: [0.0, 1.0, 0.0, 0.0],
                z: [0.0, 0.0, 1.0, 1.0 / d],
                t: [0.0, 0.0, 0.0, 1.0],
            };
            (fns.gs_matrix_mul)(&mat as *const GsMatrix4 as *const c_void);
        }
        if let Some(rotaa) = fns.gs_matrix_rotaa4f {
            if roll.abs() > 1e-4 {
                rotaa(0.0, 0.0, 1.0, roll.to_radians());
            }
            if pitch.abs() > 1e-3 {
                rotaa(1.0, 0.0, 0.0, pitch.to_radians());
            }
            if yaw.abs() > 1e-3 {
                rotaa(0.0, 1.0, 0.0, yaw.to_radians());
            }
        }
        let sx = avatar_w / bw.max(1.0);
        let sy = avatar_h / bh.max(1.0);
        if let Some(scale_fn) = fns.gs_matrix_scale3f {
            scale_fn(sx, sy, 1.0);
        }
        if let Some(translate) = fns.gs_matrix_translate3f {
            translate(-bw * 0.5, -bh * 0.5, 0.0);
        }

        if !flat_tex.is_null() {
            let effect = (fns.obs_get_base_effect)(0);
            if !effect.is_null() {
                while (fns.gs_effect_loop)(effect, b"Draw\0".as_ptr() as *const c_char) {
                    (fns.obs_source_draw)(flat_tex, 0, 0, bw.max(1.0) as u32, bh.max(1.0) as u32, false);
                }
            }
        } else {
            (fns.obs_source_video_render)(source);
        }

        (fns.gs_matrix_pop)();
        (fns.gs_blend_state_pop)();
        (fns.gs_projection_pop)();
        (fns.gs_texrender_end)(texrender);
    }

    unsafe { (fns.gs_texrender_get_texture)(texrender) }
}

/// What a reaction shader gets about its target besides time and its own
/// settings: where it was hit, in the shader's UV (the padded capture), with
/// each hit's age in seconds; and how many canvas pixels the UV's 0..1 and the
/// target span, so shapes come out round and sized to the target.
#[derive(Default)]
struct ReactionShaderHits {
    hits: Vec<(f32, f32, f32)>,
    view_px: (f32, f32),
    target_px: (f32, f32),
}

impl ReactionShaderHits {
    /// `reaction`'s hits (in the target's own frame) placed on its capture:
    /// the target `size` on the canvas, turned by `rotation` (roll, pitch,
    /// yaw), in the middle of a capture `pad` times its size.
    fn new(reaction: &crate::state::ActiveReaction, size: (f32, f32), pad: (f32, f32), rotation: (f32, f32, f32)) -> Self {
        let (w, h) = size;
        let view_px = ((w * pad.0).max(1.0), (h * pad.1).max(1.0));
        let (roll, pitch, yaw) = rotation;
        let hits = reaction
            .hits
            .iter()
            .map(|&(x, y, age)| {
                let (px, py) = crate::tracker::project_3d_canvas((x - 0.5) * w, (y - 0.5) * h, 0.0, 0.0, roll, pitch, yaw);
                (0.5 + px / view_px.0, 0.5 + py / view_px.1, age)
            })
            .collect();
        Self { hits, view_px, target_px: (w, h) }
    }
}

/// Draws a captured avatar texture through its compiled reaction shader,
/// mapping our per-target progress/amount/duration state onto that shader's
/// own uniforms (see `filters/squish_oneshot.effect` / `filters/pixel_drip.effect`).
/// A Custom script's shader also gets, if it declares them: `duration` (s),
/// `uniform float4 hits[N]` (each hit's x, y and age — see ReactionShaderHits
/// — oldest first) with `hit_count` of them used, `view_width`/`view_height`
/// and `target_width`/`target_height` (canvas pixels).
unsafe fn draw_reaction_shader(
    fns: &ObsGraphicsFns,
    effect: *mut c_void,
    tex: *mut c_void,
    bw: f32,
    bh: f32,
    reaction: &crate::state::ActiveReaction,
    hits: &ReactionShaderHits,
) {
    let (Some(get_param), Some(set_float)) = (fns.gs_effect_get_param_by_name, fns.gs_effect_set_float) else {
        return;
    };
    if effect.is_null() || tex.is_null() {
        return;
    }
    let (kind, progress, amount, custom_params) = (reaction.kind, reaction.progress, reaction.amount, &reaction.custom_params);
    let duration_s = (reaction.duration_ms.max(1) as f32) / 1000.0;
    let elapsed_s = progress * duration_s;

    let set = |name: &[u8], val: f32| unsafe {
        let param = get_param(effect, name.as_ptr() as *const c_char);
        if !param.is_null() {
            set_float(param, val);
        }
    };
    let set_named = |name: &str, val: f32| {
        if let Ok(cname) = std::ffi::CString::new(name) {
            unsafe {
                let param = get_param(effect, cname.as_ptr());
                if !param.is_null() {
                    set_float(param, val);
                }
            }
        }
    };

    match kind {
        crate::physics::ImpactReactionKind::Bounce => {
            const MAX_PHASE: f32 = 18.8495559; // 3 full wobble cycles (6*PI) — matches squish_oneshot.effect
            set(b"squish_amount\0", amount);
            set(b"squish_speed\0", MAX_PHASE / duration_s.max(0.05));
            // The expanded capture padding (see call site) provides generous room,
            // so scale_factor stays 1.0 (true native scale without artificial zoom).
            set(b"scale_factor\0", 1.0);
        }
        crate::physics::ImpactReactionKind::PixelDrip => {
            set(b"drip_speed\0", 1.0 / duration_s.max(0.1));
            set(b"randomness\0", amount.clamp(0.0, 1.0));
            // The expanded capture padding provides melt room with native 1.0 scale.
            set(b"scale_factor\0", 1.0);
        }
        crate::physics::ImpactReactionKind::Custom(_) => {
            // Generic path: start from whatever tunable uniforms the script
            // itself declared defaults for (see extract_default_floats_from_lua),
            // then apply the user's own overrides (from the Studio's per-param
            // sliders) on top of those, by name.
            let dcache = REACTION_CUSTOM_DEFAULTS.lock();
            if let Some(defaults) = dcache.as_ref().and_then(|m| m.get(&kind)) {
                for (name, val) in defaults {
                    if name != "time" {
                        let effective = custom_params.iter().find(|(n, _)| n == name).map(|(_, v)| *v).unwrap_or(*val);
                        set_named(name, effective);
                    }
                }
            }
            drop(dcache);
            // Where it was hit and how big things are (see ReactionShaderHits)
            set(b"duration\0", duration_s);
            set(b"view_width\0", hits.view_px.0);
            set(b"view_height\0", hits.view_px.1);
            set(b"target_width\0", hits.target_px.0);
            set(b"target_height\0", hits.target_px.1);
            // The hits: all the shader's `hits` array holds (the newest, when
            // there are more), and exactly its size (see hits_array_len)
            let len = REACTION_HITS_LEN.lock().as_ref().and_then(|m| m.get(&kind).copied());
            let hits_param = unsafe { get_param(effect, b"hits\0".as_ptr() as *const c_char) };
            match (len, fns.gs_effect_set_val) {
                (Some(len), Some(set_val)) if !hits_param.is_null() => {
                    let newest = &hits.hits[hits.hits.len().saturating_sub(len)..];
                    let mut packed = vec![0.0f32; len * 4];
                    for (slot, &(x, y, age)) in packed.chunks_exact_mut(4).zip(newest) {
                        slot[..3].copy_from_slice(&[x, y, age]);
                    }
                    unsafe { set_val(hits_param, packed.as_ptr() as *const c_void, packed.len() * 4) };
                    set(b"hit_count\0", newest.len() as f32);
                }
                _ => set(b"hit_count\0", 0.0),
            }
        }
        _ => {}
    }
    // Always drive "time" ourselves last so the effect actually animates over
    // real elapsed time, regardless of whatever (if any) default it declared.
    set(b"time\0", elapsed_s);

    unsafe {
        while (fns.gs_effect_loop)(effect, b"Draw\0".as_ptr() as *const c_char) {
            (fns.obs_source_draw)(tex, 0, 0, bw.max(1.0) as u32, bh.max(1.0) as u32, false);
        }
    }
}

/// Renders `captured_tex` through its reaction shader into an isolated offscreen
/// texture of size `(cap_w, cap_h)`. This produces a pure RGBA buffer containing the
/// live swirled, squished, bounced, or melted avatar pixels, which can then be:
/// 1. Drawn directly onto the composite canvas via `obs_source_draw`.
/// 2. Staged and scanned for pixel-accurate collision masks (`state.masks`) and hitboxes.
unsafe fn capture_reaction_shader_to_texture(
    fns: &ObsGraphicsFns,
    effect: *mut c_void,
    captured_tex: *mut c_void,
    cap_w: u32,
    cap_h: u32,
    reaction: &crate::state::ActiveReaction,
    hits: &ReactionShaderHits,
) -> *mut c_void {
    if cap_w == 0 || cap_h == 0 || effect.is_null() || captured_tex.is_null() {
        return std::ptr::null_mut();
    }
    let mut tr_lock = REACTION_OUT_TEXRENDER.lock();
    let mut dims_lock = REACTION_OUT_TEXRENDER_DIMS.lock();

    if tr_lock.is_none() || *dims_lock != (cap_w, cap_h) {
        if let Some(old) = tr_lock.take() {
            unsafe { (fns.gs_texrender_destroy)(old as *mut c_void); }
        }
        let ptr = unsafe { (fns.gs_texrender_create)(3 /* GS_RGBA */, 0) };
        if ptr.is_null() {
            return std::ptr::null_mut();
        }
        *tr_lock = Some(ptr as usize);
        *dims_lock = (cap_w, cap_h);
    }
    let texrender = tr_lock.unwrap() as *mut c_void;
    drop(tr_lock);
    drop(dims_lock);

    unsafe {
        (fns.gs_texrender_reset)(texrender);
        if !(fns.gs_texrender_begin)(texrender, cap_w, cap_h) {
            return std::ptr::null_mut();
        }
        let clear_color = [0.0f32, 0.0, 0.0, 0.0];
        (fns.gs_clear)(1, clear_color.as_ptr(), 0.0, 0);
        (fns.gs_projection_push)();
        (fns.gs_ortho)(0.0, cap_w as f32, 0.0, cap_h as f32, -32768.0, 32768.0);
        (fns.gs_blend_state_push)();
        if let Some(reset_blend) = fns.gs_reset_blend_state {
            reset_blend();
        }
        (fns.gs_enable_blending)(true);
        (fns.gs_matrix_push)();
        (fns.gs_matrix_identity)();

        draw_reaction_shader(fns, effect, captured_tex, cap_w as f32, cap_h as f32, reaction, hits);

        (fns.gs_matrix_pop)();
        (fns.gs_blend_state_pop)();
        (fns.gs_projection_pop)();
        (fns.gs_texrender_end)(texrender);
        (fns.gs_texrender_get_texture)(texrender)
    }
}

/// Small process-wide cache of pre-baked radial-gradient glow sprites, keyed by
/// (r, g, b, alpha-bucket) so a lightning impact's live fade-in/out can still
/// look smooth without recreating a GPU texture every single frame.
static GLOW_TEX_CACHE: parking_lot::Mutex<Option<std::collections::HashMap<(u8, u8, u8, u8), usize>>> =
    parking_lot::Mutex::new(None);

const GLOW_TEX_SIZE: usize = 64;

/// Fire's own glow sprites (same keying as GLOW_TEX_CACHE), baked with
/// FIRE_GLOW_FALLOFF instead of the shared 1.8 curve.
#[cfg(target_os = "windows")]
static FIRE_GLOW_TEX_CACHE: parking_lot::Mutex<Option<std::collections::HashMap<(u8, u8, u8, u8), usize>>> =
    parking_lot::Mutex::new(None);

/// With the shared steep 1.8 falloff, fire on Windows (Direct3D) reads much
/// smaller than the same flames on Linux (OpenGL): most of each wisp sprite is
/// its faint outer halo, which all but vanishes when blended as-is. This is the
/// 1.8 curve as it looks once sRGB-encoded (1.8 / 2.2), bringing that halo back.
#[cfg(target_os = "windows")]
const FIRE_GLOW_FALLOFF: f32 = 1.8 / 2.2;

/// Returns a cached (or freshly baked) square RGBA texture holding a soft,
/// circular radial gradient of the given color scaled to the given peak alpha.
/// A hard-edged quad can't read as "spheric" or gradually fading — this gives
/// a genuinely round, softly-degrading glow instead.
unsafe fn get_or_create_radial_glow_tex(fns: &ObsGraphicsFns, r: u8, g: u8, b: u8, alpha_bucket: u8) -> *mut c_void {
    unsafe { get_or_create_radial_glow_tex_with(fns, &GLOW_TEX_CACHE, r, g, b, alpha_bucket, 1.8) }
}

/// Glow sprite for fire wisps and sparks (see FIRE_GLOW_FALLOFF).
unsafe fn get_or_create_fire_glow_tex(fns: &ObsGraphicsFns, r: u8, g: u8, b: u8, alpha_bucket: u8) -> *mut c_void {
    #[cfg(target_os = "windows")]
    {
        unsafe { get_or_create_radial_glow_tex_with(fns, &FIRE_GLOW_TEX_CACHE, r, g, b, alpha_bucket, FIRE_GLOW_FALLOFF) }
    }
    #[cfg(not(target_os = "windows"))]
    {
        unsafe { get_or_create_radial_glow_tex(fns, r, g, b, alpha_bucket) }
    }
}

unsafe fn get_or_create_radial_glow_tex_with(
    fns: &ObsGraphicsFns,
    cache_lock: &parking_lot::Mutex<Option<std::collections::HashMap<(u8, u8, u8, u8), usize>>>,
    r: u8,
    g: u8,
    b: u8,
    alpha_bucket: u8,
    falloff_exp: f32,
) -> *mut c_void {
    let key = (r, g, b, alpha_bucket);
    {
        let cache = cache_lock.lock();
        if let Some(map) = cache.as_ref() {
            if let Some(&ptr) = map.get(&key) {
                return ptr as *mut c_void;
            }
        }
    }
    let Some(create) = fns.gs_texture_create else {
        return std::ptr::null_mut();
    };

    let center = (GLOW_TEX_SIZE as f32 - 1.0) * 0.5;
    let peak = alpha_bucket as f32 / 255.0;
    let mut pixels = vec![0u8; GLOW_TEX_SIZE * GLOW_TEX_SIZE * 4];
    for y in 0..GLOW_TEX_SIZE {
        for x in 0..GLOW_TEX_SIZE {
            let dx = x as f32 - center;
            let dy = y as f32 - center;
            let dist = (dx * dx + dy * dy).sqrt() / center;
            let t = (1.0 - dist).clamp(0.0, 1.0);
            let falloff = t.powf(falloff_exp); // soft, gradual fade toward the edge
            let idx = (y * GLOW_TEX_SIZE + x) * 4;
            pixels[idx] = r;
            pixels[idx + 1] = g;
            pixels[idx + 2] = b;
            pixels[idx + 3] = (falloff * peak * 255.0) as u8;
        }
    }

    let row_ptr: *const u8 = pixels.as_ptr();
    let mip_levels: [*const u8; 1] = [row_ptr];
    let tex = unsafe {
        create(GLOW_TEX_SIZE as u32, GLOW_TEX_SIZE as u32, 3 /* GS_RGBA */, 1, mip_levels.as_ptr(), 0)
    };
    if !tex.is_null() {
        let mut cache = cache_lock.lock();
        cache.get_or_insert_with(std::collections::HashMap::new).insert(key, tex as usize);
    }
    tex
}

static METABALL_FIELD_TEX: parking_lot::Mutex<Option<usize>> = parking_lot::Mutex::new(None);

/// Dedicated field-contribution sprite for metaball rendering — pure white,
/// full alpha, with a much wider/flatter falloff (t^0.5) than the fire/
/// lightning glow texture's steep t^1.8 curve. A metaball "field" needs
/// generous reach so consecutive particles' contributions still overlap
/// meaningfully well past their own visual radius; the steep glow falloff
/// (fine for a tight, bright point-light look) drops off too fast for that,
/// so particles spaced even a bit apart along a stream never crossed the
/// threshold *between* them and rendered as separate un-fused circles.
unsafe fn get_or_create_metaball_field_tex(fns: &ObsGraphicsFns) -> *mut c_void {
    {
        let cache = METABALL_FIELD_TEX.lock();
        if let Some(&ptr) = cache.as_ref() {
            return ptr as *mut c_void;
        }
    }
    let Some(create) = fns.gs_texture_create else {
        return std::ptr::null_mut();
    };
    let center = (GLOW_TEX_SIZE as f32 - 1.0) * 0.5;
    let mut pixels = vec![0u8; GLOW_TEX_SIZE * GLOW_TEX_SIZE * 4];
    for y in 0..GLOW_TEX_SIZE {
        for x in 0..GLOW_TEX_SIZE {
            let dx = x as f32 - center;
            let dy = y as f32 - center;
            let dist = (dx * dx + dy * dy).sqrt() / center;
            let t = (1.0 - dist).clamp(0.0, 1.0);
            let falloff = t.powf(0.5);
            let idx = (y * GLOW_TEX_SIZE + x) * 4;
            pixels[idx] = 255;
            pixels[idx + 1] = 255;
            pixels[idx + 2] = 255;
            pixels[idx + 3] = (falloff * 255.0) as u8;
        }
    }
    let row_ptr: *const u8 = pixels.as_ptr();
    let mip_levels: [*const u8; 1] = [row_ptr];
    let tex = unsafe {
        create(GLOW_TEX_SIZE as u32, GLOW_TEX_SIZE as u32, 3 /* GS_RGBA */, 1, mip_levels.as_ptr(), 0)
    };
    if !tex.is_null() {
        *METABALL_FIELD_TEX.lock() = Some(tex as usize);
    }
    tex
}

/// Keyed by path (not a single slot) so two simultaneously-active redeems
/// configured with different water textures each get their own cached GPU
/// texture instead of thrashing one shared slot back and forth every frame.
static WATER_TEXTURE_CACHE: parking_lot::Mutex<Option<std::collections::HashMap<String, usize>>> = parking_lot::Mutex::new(None);

/// Loads (and caches, keyed by path) a redeem's configured water texture
/// image as a GPU texture, replacing the metaball's procedural surface-bump
/// noise when set. Returns null if no path is configured, or loading/
/// decoding failed, in which case the caller falls back to the procedural
/// noise.
unsafe fn get_or_load_water_texture(fns: &ObsGraphicsFns, path: &str) -> *mut c_void {
    if path.is_empty() {
        return std::ptr::null_mut();
    }

    {
        let cache = WATER_TEXTURE_CACHE.lock();
        if let Some(map) = cache.as_ref() {
            if let Some(&ptr) = map.get(path) {
                return ptr as *mut c_void;
            }
        }
    }

    let Some(create) = fns.gs_texture_create else { return std::ptr::null_mut(); };
    let img = match image::open(&path) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("[Trust] Could not load water texture '{}': {}", path, e);
            return std::ptr::null_mut();
        }
    };
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    let pixels = rgba.into_raw();
    let mip_levels: [*const u8; 1] = [pixels.as_ptr()];
    let tex = unsafe { create(w, h, 3 /* GS_RGBA */, 1, mip_levels.as_ptr(), 0) };
    if tex.is_null() {
        eprintln!("[Trust] Failed to create GPU texture for water texture '{}'", path);
        return std::ptr::null_mut();
    }

    let mut cache = WATER_TEXTURE_CACHE.lock();
    cache.get_or_insert_with(std::collections::HashMap::new).insert(path.to_string(), tex as usize);
    tex
}

struct RedeemTextureEntry {
    frames: Vec<(usize, u32, u32)>, // (tex as usize, width, height)
}

static REDEEM_TEXTURE_CACHE: parking_lot::Mutex<Option<std::collections::HashMap<u32, RedeemTextureEntry>>> = parking_lot::Mutex::new(None);

unsafe fn get_or_create_redeem_texture(
    fns: &ObsGraphicsFns,
    redeem_id: u32,
    cur_frame: u32,
) -> Option<(*mut c_void, u32, u32)> {
    if redeem_id == 0 {
        return None;
    }

    {
        let cache = REDEEM_TEXTURE_CACHE.lock();
        if let Some(map) = cache.as_ref() {
            if let Some(entry) = map.get(&redeem_id) {
                if !entry.frames.is_empty() {
                    let idx = (cur_frame as usize) % entry.frames.len();
                    let (tex_usize, w, h) = entry.frames[idx];
                    return Some((tex_usize as *mut c_void, w, h));
                }
            }
        }
    }

    let create = fns.gs_texture_create?;

    let state = crate::state::get_state();
    let img_src_opt = {
        let p = state.physics.read();
        p.redeem_images.get(&redeem_id).cloned()
    };

    let img_src = match img_src_opt {
        Some(src) if !src.is_empty() => Some(src),
        _ => {
            let key_opt = {
                let p = state.physics.read();
                p.id_to_redeem_key.get(&redeem_id).cloned()
            };
            if let Some(key) = key_opt {
                let found = crate::physics::redeem_image_src(&key);
                if let Some(ref s) = found {
                    state.physics.write().redeem_images.insert(redeem_id, s.clone());
                }
                found
            } else {
                None
            }
        }
    }?;

    use base64::Engine;
    let raw_bytes: Vec<u8> = if let Some(idx) = img_src.find(";base64,") {
        let b64 = &img_src[idx + 8..];
        base64::prelude::BASE64_STANDARD.decode(b64.trim()).ok()?
    } else if std::path::Path::new(&img_src).is_file() {
        std::fs::read(&img_src).ok()?
    } else if let Ok(b) = base64::prelude::BASE64_STANDARD.decode(img_src.trim()) {
        b
    } else {
        // Named relative to your redeems, your data or what ships with Theater
        std::fs::read(crate::paths::find_file(&img_src)?).ok()?
    };

    let mut frames_vec = Vec::new();

    // Check if it's an animated GIF
    use image::codecs::gif::GifDecoder;
    use image::AnimationDecoder;
    if let Ok(decoder) = GifDecoder::new(std::io::Cursor::new(&raw_bytes)) {
        if let Ok(frames) = decoder.into_frames().collect_frames() {
            for frame in frames {
                let rgba = frame.into_buffer();
                let (w, h) = rgba.dimensions();
                if w > 0 && h > 0 {
                    let pixels = rgba.into_raw();
                    let mip_levels: [*const u8; 1] = [pixels.as_ptr()];
                    let tex = unsafe { create(w, h, 3 /* GS_RGBA */, 1, mip_levels.as_ptr(), 0) };
                    if !tex.is_null() {
                        frames_vec.push((tex as usize, w, h));
                    }
                }
            }
        }
    }

    if frames_vec.is_empty() {
        if let Ok(img) = image::load_from_memory(&raw_bytes) {
            let rgba = img.to_rgba8();
            let (w, h) = rgba.dimensions();
            if w > 0 && h > 0 {
                let pixels = rgba.into_raw();
                let mip_levels: [*const u8; 1] = [pixels.as_ptr()];
                let tex = unsafe { create(w, h, 3 /* GS_RGBA */, 1, mip_levels.as_ptr(), 0) };
                if !tex.is_null() {
                    frames_vec.push((tex as usize, w, h));
                }
            }
        }
    }

    if frames_vec.is_empty() {
        eprintln!("[Trust] Failed to decode image into GPU texture for redeem {}", redeem_id);
        return None;
    }

    let ret = {
        let idx = (cur_frame as usize) % frames_vec.len();
        let (tex_usize, w, h) = frames_vec[idx];
        (tex_usize as *mut c_void, w, h)
    };

    let mut cache = REDEEM_TEXTURE_CACHE.lock();
    cache.get_or_insert_with(std::collections::HashMap::new).insert(
        redeem_id,
        RedeemTextureEntry { frames: frames_vec },
    );

    println!("[Trust Source] 🖼️ Loaded GPU texture for redeem {} ({}x{}, {} frames)", redeem_id, ret.1, ret.2, ret.0 as usize);
    Some(ret)
}

/// Draws a textured quad (straight alpha, default "Draw" technique) centered at (x, y).
unsafe fn draw_glow_sprite(fns: &ObsGraphicsFns, tex: *mut c_void, x: f32, y: f32, size: f32) {
    unsafe { draw_glow_sprite_ex(fns, tex, x, y, size, size, 0.0, 1.0); }
}

unsafe fn get_or_create_sprite_alpha_effect(fns: &ObsGraphicsFns) -> *mut c_void {
    {
        let cache = SPRITE_ALPHA_EFFECT.lock();
        if let Some(&ptr) = cache.as_ref() {
            return ptr as *mut c_void;
        }
    }
    let Some(create) = fns.gs_effect_create else { return std::ptr::null_mut(); };
    let source = "
uniform float4x4 ViewProj;
uniform texture2d image;
uniform float alpha = 1.0;

sampler_state def_sampler {
    Filter   = Linear;
    AddressU = Clamp;
    AddressV = Clamp;
};

struct VertInOut {
    float4 pos : POSITION;
    float2 uv  : TEXCOORD0;
};

VertInOut VSDefault(VertInOut vert_in)
{
    VertInOut vert_out;
    vert_out.pos = mul(float4(vert_in.pos.xyz, 1.0), ViewProj);
    vert_out.uv  = vert_in.uv;
    return vert_out;
}

float4 PSDraw(VertInOut vert_in) : TARGET
{
    float4 rgba = image.Sample(def_sampler, vert_in.uv);
    rgba.rgb *= alpha;
    rgba.a *= alpha;
    return rgba;
}

technique Draw
{
    pass
    {
        vertex_shader = VSDefault(vert_in);
        pixel_shader  = PSDraw(vert_in);
    }
}
";
    let Ok(c_source) = std::ffi::CString::new(source) else { return std::ptr::null_mut(); };
    let effect = unsafe { create(c_source.as_ptr(), std::ptr::null(), std::ptr::null_mut()) };
    if effect.is_null() {
        eprintln!("[Trust] Failed to compile sprite alpha shader");
        return std::ptr::null_mut();
    }
    *SPRITE_ALPHA_EFFECT.lock() = Some(effect as usize);
    effect
}

/// Like `draw_glow_sprite`, but lets the (round, soft-gradient) glow texture be
/// stretched into a non-uniform oval, rotated, and faded by alpha.
unsafe fn draw_glow_sprite_ex(fns: &ObsGraphicsFns, tex: *mut c_void, x: f32, y: f32, w: f32, h: f32, angle_rad: f32, alpha: f32) {
    if tex.is_null() || w < 0.5 || h < 0.5 || alpha <= 0.001 {
        return;
    }
    let (effect, has_alpha_param) = if alpha < 0.999 {
        let eff = unsafe { get_or_create_sprite_alpha_effect(fns) };
        if !eff.is_null() {
            (eff, true)
        } else {
            (unsafe { (fns.obs_get_base_effect)(0) }, false)
        }
    } else {
        (unsafe { (fns.obs_get_base_effect)(0) }, false)
    };
    if effect.is_null() {
        return;
    }
    if let Some(get_param) = fns.gs_effect_get_param_by_name {
        if let Some(set_tex) = fns.gs_effect_set_texture {
            let img_param = unsafe { get_param(effect, b"image\0".as_ptr() as *const c_char) };
            if !img_param.is_null() {
                unsafe { set_tex(img_param, tex); }
            }
        }
        if has_alpha_param {
            if let Some(set_float) = fns.gs_effect_set_float {
                let a_param = unsafe { get_param(effect, b"alpha\0".as_ptr() as *const c_char) };
                if !a_param.is_null() {
                    unsafe { set_float(a_param, alpha); }
                }
            }
        }
    }
    unsafe {
        (fns.gs_matrix_push)();
        if let Some(translate) = fns.gs_matrix_translate3f {
            translate(x, y, 0.0);
        }
        if angle_rad.abs() > 0.0001 {
            if let Some(rotaa) = fns.gs_matrix_rotaa4f {
                rotaa(0.0, 0.0, 1.0, angle_rad);
            }
        }
        if let Some(translate) = fns.gs_matrix_translate3f {
            translate(-w * 0.5, -h * 0.5, 0.0);
        }
        if let Some(draw_sprite) = fns.gs_draw_sprite {
            while (fns.gs_effect_loop)(effect, b"Draw\0".as_ptr() as *const c_char) {
                draw_sprite(tex, 0, w.max(1.0) as u32, h.max(1.0) as u32);
            }
        } else {
            (fns.obs_source_draw)(tex, 0, 0, w.max(1.0) as u32, h.max(1.0) as u32, false);
        }
        (fns.gs_matrix_pop)();
    }
}

fn pack_argb(r: f32, g: f32, b: f32, a: f32) -> u32 {
    let ri = (r.clamp(0.0, 1.0) * 255.0) as u32;
    let gi = (g.clamp(0.0, 1.0) * 255.0) as u32;
    let bi = (b.clamp(0.0, 1.0) * 255.0) as u32;
    let ai = (a.clamp(0.0, 1.0) * 255.0) as u32;
    (ai << 24) | (ri << 16) | (gi << 8) | bi
}

/// Resolved once: get_filters_dir() probes the filesystem, and the metaball
/// effect is looked up every frame water is on screen.
static METABALL_EFFECT_PATH: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
    crate::studio_app::get_filters_dir().join("metaball.effect").to_string_lossy().into_owned()
});

unsafe fn get_or_create_metaball_effect(fns: &ObsGraphicsFns) -> *mut c_void {
    let path = METABALL_EFFECT_PATH.as_str();
    let mtime = std::fs::metadata(path)
        .and_then(|m| m.modified())
        .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
    {
        let cache = METABALL_EFFECT.lock();
        if let Some(&(ptr, cached_mtime)) = cache.as_ref() {
            if cached_mtime == mtime && ptr != 0 {
                return ptr as *mut c_void;
            }
        }
    }
    let Some(create) = fns.gs_effect_create else { return std::ptr::null_mut(); };
    let source = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[Trust] Could not read metaball shader '{}': {}", path, e);
            return std::ptr::null_mut();
        }
    };
    let Ok(c_source) = std::ffi::CString::new(source) else { return std::ptr::null_mut(); };
    let effect = unsafe { create(c_source.as_ptr(), std::ptr::null(), std::ptr::null_mut()) };
    if effect.is_null() {
        eprintln!("[Trust] Failed to compile metaball shader ('{}')", path);
        return std::ptr::null_mut();
    }
    let mut cache = METABALL_EFFECT.lock();
    if let Some((old_ptr, _)) = cache.take() {
        if old_ptr != 0 {
            if let Some(destroy) = fns.gs_effect_destroy {
                unsafe { destroy(old_ptr as *mut c_void); }
            }
        }
    }
    *cache = Some((effect as usize, mtime));
    effect
}

/// Additively accumulates every fluid particle's soft round influence into a
/// canvas-sized offscreen field texture — the raw material a metaball
/// threshold pass turns into one connected liquid blob instead of a pile of
/// separate circles. `particles` is (x, y, radius) in world/canvas pixels;
/// color doesn't matter here (the field is a pure white/alpha intensity map,
/// tinted later in `draw_fluid_metaball`), which is also what lets particles
/// from multiple engines sharing the same color merge into one pass.
unsafe fn render_fluid_field(
    fns: &ObsGraphicsFns,
    particles: &[(f32, f32, f32)],
    canvas_w: u32,
    canvas_h: u32,
    metaball_size: f32,
) -> *mut c_void {
    if canvas_w == 0 || canvas_h == 0 {
        return std::ptr::null_mut();
    }
    let mut tr_lock = FLUID_FIELD_TEXRENDER.lock();
    let mut dims_lock = FLUID_FIELD_DIMS.lock();

    if tr_lock.is_none() || *dims_lock != (canvas_w, canvas_h) {
        if let Some(old) = tr_lock.take() {
            unsafe { (fns.gs_texrender_destroy)(old as *mut c_void); }
        }
        let ptr = unsafe { (fns.gs_texrender_create)(3 /* GS_RGBA */, 0) };
        if ptr.is_null() {
            return std::ptr::null_mut();
        }
        *tr_lock = Some(ptr as usize);
        *dims_lock = (canvas_w, canvas_h);
    }
    let texrender = tr_lock.unwrap() as *mut c_void;

    // Wide, flat-falloff field-contribution sprite — its alpha channel IS the
    // field contribution, additively summed below (see the dedicated
    // function's doc comment for why this isn't the fire/lightning glow tex).
    let field_tex = unsafe { get_or_create_metaball_field_tex(fns) };

    unsafe {
        (fns.gs_texrender_reset)(texrender);
        if !(fns.gs_texrender_begin)(texrender, canvas_w, canvas_h) {
            return std::ptr::null_mut();
        }
        let clear_color = [0.0f32, 0.0, 0.0, 0.0];
        (fns.gs_clear)(1, clear_color.as_ptr(), 0.0, 0);
        (fns.gs_projection_push)();
        (fns.gs_ortho)(0.0, canvas_w as f32, 0.0, canvas_h as f32, -32768.0, 32768.0);
        (fns.gs_blend_state_push)();
        if let Some(reset_blend) = fns.gs_reset_blend_state {
            reset_blend();
        }
        (fns.gs_enable_blending)(true);
        if let Some(blend_func) = fns.gs_blend_function {
            // ONE, ONE (not SRCALPHA, ONE) — this pass only cares about the
            // resulting ALPHA channel as a scalar field, and SRCALPHA as the
            // src factor also multiplies into the alpha channel's own blend,
            // squaring each sprite's contribution (dst_a += src_a * src_a)
            // instead of summing it linearly. For typical overlap alphas
            // (~0.3) that's 0.09 instead of 0.3 per particle — the field
            // barely built up at all, so it could cross the metaball
            // threshold nowhere. Pure ONE,ONE sums alpha linearly; the
            // resulting (garbage, saturated) RGB is never read.
            blend_func(1, 1);
        }

        let size_mult = metaball_size;
        for &(x, y, rad) in particles {
            if rad > 0.5 {
                // Much larger than the particle's own visual size — particles
                // are emitted at intervals along the stream, and if each
                // one's field only reaches a little past its own radius,
                // consecutive particles never overlap enough to cross the
                // threshold *between* them, showing as a "string of beads"
                // instead of one connected liquid shape.
                draw_glow_sprite(fns, field_tex, x, y, rad * size_mult);
            }
        }

        (fns.gs_blend_state_pop)();
        (fns.gs_projection_pop)();
        (fns.gs_texrender_end)(texrender);
    }

    unsafe { (fns.gs_texrender_get_texture)(texrender) }
}

/// Thresholds the accumulated fluid field (see `render_fluid_field`) into a
/// single round, organically-deformed liquid blob tinted by `color`, and
/// composites it onto the main output at (0, 0) size (canvas_w, canvas_h) —
/// the field texture is already 1:1 aligned to canvas pixel coordinates.
unsafe fn draw_fluid_metaball(
    fns: &ObsGraphicsFns,
    field_tex: *mut c_void,
    backdrop_tex: *mut c_void,
    canvas_w: u32,
    canvas_h: u32,
    color: (u8, u8, u8, f32),
    flow: (f32, f32),
    visual: &crate::fluid::WaterVisualSettings,
) {
    if field_tex.is_null() {
        return;
    }
    let effect = unsafe { get_or_create_metaball_effect(fns) };
    if effect.is_null() {
        return;
    }
    let (Some(get_param), Some(set_float)) = (fns.gs_effect_get_param_by_name, fns.gs_effect_set_float) else {
        return;
    };
    let set = |name: &[u8], val: f32| unsafe {
        let param = get_param(effect, name.as_ptr() as *const c_char);
        if !param.is_null() {
            set_float(param, val);
        }
    };
    // Low enough that the weak "saddle" field between two particles that
    // only partially overlap still crosses it and fuses them — a single
    // particle's own center is still comfortably above this (its field peaks
    // near 1.0 thanks to render_fluid_field's linear alpha accumulation).
    set(b"threshold\0", 0.14);
    // A crisp, defined liquid edge instead of a soft glow-fade — just enough
    // softness (a couple pixels' worth of field range) to anti-alias the
    // boundary, not the wide gradient a fog-like falloff would give.
    set(b"edge_softness\0", 0.025);
    // Toon outline stroke — the field band this far below `threshold` draws
    // as a bold white rim instead of the shaded body, matching a stylized
    // anime-splash look (flat cel-shaded fill + ink outline) rather than the
    // photoreal glossy water look this used to chase.
    set(b"outline_width\0", 0.05);
    set(b"noise_strength\0", 18.0);
    set(b"noise_scale\0", 22.0);
    set(b"time\0", METABALL_START.elapsed().as_secs_f32());
    set(b"resolution_x\0", canvas_w as f32);
    set(b"resolution_y\0", canvas_h as f32);
    set(b"flow_dir_x\0", flow.0);
    set(b"flow_dir_y\0", flow.1);
    set(b"round_edge\0", if visual.round_edge { 1.0 } else { 0.0 });

    // Real visual distortion: refract whatever's actually behind the water
    // (captured just before this draw, since we can't read the render
    // target we're also writing to) instead of just filling with flat
    // color. Falls back to the flat-color look if unsupported.
    let have_backdrop = !backdrop_tex.is_null() && fns.gs_effect_set_texture.is_some();
    set(b"has_backdrop\0", if have_backdrop { 1.0 } else { 0.0 });
    let r_strength = if visual.refraction_strength > 0.0 { visual.refraction_strength } else { 350.0 };
    set(b"refract_strength\0", r_strength);
    set(b"refraction_enabled\0", if visual.refraction_enabled { 1.0 } else { 0.0 });
    if have_backdrop {
        if let Some(set_tex) = fns.gs_effect_set_texture {
            unsafe {
                let param = get_param(effect, b"backdrop\0".as_ptr() as *const c_char);
                if !param.is_null() {
                    set_tex(param, backdrop_tex);
                }
            }
        }
    }

    // Custom water texture image for this redeem — replaces the procedural
    // surface-bump noise entirely when configured.
    let water_tex = match visual.texture_path.as_deref() {
        Some(path) => unsafe { get_or_load_water_texture(fns, path) },
        None => std::ptr::null_mut(),
    };
    let have_water_tex = !water_tex.is_null() && fns.gs_effect_set_texture.is_some();
    set(b"has_water_tex\0", if have_water_tex { 1.0 } else { 0.0 });
    set(b"water_tex_scale\0", visual.texture_scale);
    set(b"water_tex_scroll_speed\0", visual.texture_scroll_speed);
    if have_water_tex {
        if let Some(set_tex) = fns.gs_effect_set_texture {
            unsafe {
                let param = get_param(effect, b"water_tex\0".as_ptr() as *const c_char);
                if !param.is_null() {
                    set_tex(param, water_tex);
                }
            }
        }
    }

    if let Some(set_color) = fns.gs_effect_set_color {
        unsafe {
            let param = get_param(effect, b"fluid_color\0".as_ptr() as *const c_char);
            if !param.is_null() {
                set_color(param, pack_argb(color.0 as f32 / 255.0, color.1 as f32 / 255.0, color.2 as f32 / 255.0, color.3));
            }
        }
    }

    unsafe {
        (fns.gs_blend_state_push)();
        if let Some(reset_blend) = fns.gs_reset_blend_state {
            reset_blend();
        }
        (fns.gs_enable_blending)(true);
        while (fns.gs_effect_loop)(effect, b"Draw\0".as_ptr() as *const c_char) {
            (fns.obs_source_draw)(field_tex, 0, 0, canvas_w, canvas_h, false);
        }
        (fns.gs_blend_state_pop)();
    }
}

/// Draws a solid-color (OBS_EFFECT_SOLID) rectangle centered at (x, y), optionally
/// rotated (radians), given an already-resolved effect/color-param pair. Prefer
/// this in tight loops (many quads per frame) over `draw_solid_quad`, which
/// re-resolves the effect/param on every call.
unsafe fn draw_solid_quad_with(fns: &ObsGraphicsFns, effect: *mut c_void, color_param: *mut c_void, x: f32, y: f32, w: f32, h: f32, angle_rad: f32, argb: u32) {
    let Some(set_color) = fns.gs_effect_set_color else { return };
    let Some(draw_sprite) = fns.gs_draw_sprite else { return };
    if w < 0.5 || h < 0.5 || effect.is_null() || color_param.is_null() {
        return;
    }
    unsafe {
        set_color(color_param, argb);
        (fns.gs_matrix_push)();
        if let Some(translate) = fns.gs_matrix_translate3f {
            translate(x, y, 0.0);
        }
        if angle_rad.abs() > 0.0001 {
            if let Some(rotaa) = fns.gs_matrix_rotaa4f {
                rotaa(0.0, 0.0, 1.0, angle_rad);
            }
        }
        if let Some(translate) = fns.gs_matrix_translate3f {
            translate(-w * 0.5, -h * 0.5, 0.0);
        }
        while (fns.gs_effect_loop)(effect, b"Solid\0".as_ptr() as *const c_char) {
            draw_sprite(std::ptr::null_mut(), 0, w.max(1.0) as u32, h.max(1.0) as u32);
        }
        (fns.gs_matrix_pop)();
    }
}

/// One-off convenience wrapper around `draw_solid_quad_with` for call sites
/// that only draw a single quad (e.g. the Jolt flash overlay) and don't need
/// to amortize the effect/param lookup across many calls.
unsafe fn draw_solid_quad(fns: &ObsGraphicsFns, x: f32, y: f32, w: f32, h: f32, angle_rad: f32, argb: u32) {
    let Some(get_param) = fns.gs_effect_get_param_by_name else { return };
    let effect = unsafe { (fns.obs_get_base_effect)(3) }; // OBS_EFFECT_SOLID
    if effect.is_null() {
        return;
    }
    let color_param = unsafe { get_param(effect, b"color\0".as_ptr() as *const c_char) };
    unsafe { draw_solid_quad_with(fns, effect, color_param, x, y, w, h, angle_rad, argb); }
}

/// TEMP fire-size diagnostics (trust_debug.log) — remove once resolved.
/// (last log ms, layer renders since then)
static FIRE_DIAG: parking_lot::Mutex<(u64, u32)> = parking_lot::Mutex::new((0, 0));

fn log_fire_diagnostics(wisps: &[f32], sparks: &[f32]) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let mut diag = FIRE_DIAG.lock();
    diag.1 += 1;
    if wisps.is_empty() && sparks.is_empty() {
        return;
    }
    if diag.0 == 0 || now < diag.0 {
        *diag = (now, 0);
        return;
    }
    let elapsed = now - diag.0;
    if elapsed < 2000 {
        return;
    }
    let stats = |buf: &[f32]| -> (usize, f32, f32, f32) {
        let (mut n, mut sum_r, mut max_r, mut sum_a) = (0usize, 0.0f32, 0.0f32, 0.0f32);
        for c in buf.chunks_exact(10) {
            n += 1;
            sum_r += c[2];
            max_r = max_r.max(c[2]);
            sum_a += c[6];
        }
        let d = n.max(1) as f32;
        (n, sum_r / d, max_r, sum_a / d)
    };
    let (wn, wr, wmax, wa) = stats(wisps);
    let (sn, sr, _, _) = stats(sparks);
    let dt_ms = f32::from_bits(LAST_TICK_SECONDS.load(std::sync::atomic::Ordering::Relaxed)) * 1000.0;
    let (cw, ch) = get_obs_canvas_resolution();
    trust_log(&format!(
        "[Trust Fire] {} wisps: drawn radius mean {:.1} max {:.1}px, alpha mean {:.2} | {} sparks: radius mean {:.1}px | {:.1} layer renders/s, tick dt {:.1}ms, canvas {}x{}",
        wn, wr, wmax, wa, sn, sr, diag.1 as f32 * 1000.0 / elapsed as f32, dt_ms, cw, ch
    ));
    *diag = (now, 0);
}

unsafe fn render_throwable_layer(fns: &ObsGraphicsFns) {
    let (Some(get_param), Some(_set_color), Some(_draw_sprite)) = (
        fns.gs_effect_get_param_by_name,
        fns.gs_effect_set_color,
        fns.gs_draw_sprite,
    ) else {
        return;
    };

    let effect = unsafe { (fns.obs_get_base_effect)(3) }; // OBS_EFFECT_SOLID
    if effect.is_null() {
        return;
    }
    let color_param = unsafe { get_param(effect, b"color\0".as_ptr() as *const c_char) };
    if color_param.is_null() {
        return;
    }

    // Draws a solid-color rectangle centered at (x, y), optionally rotated (radians).
    let draw_quad = |x: f32, y: f32, w: f32, h: f32, angle_rad: f32, argb: u32| {
        unsafe { draw_solid_quad_with(fns, effect, color_param, x, y, w, h, angle_rad, argb); }
    };

    let state = crate::state::get_state();

    // 1. Fluid & fire particles — same underlying particle system, deliberately
    // different treatment: fluid stays flat translucent solid-color quads
    // (normal alpha blend). Fire is reworked to actually look like fire,
    // following filters/ reference conventions (WebGLFireStream.js): flame
    // wisps are soft glowing ovals oriented along their velocity (a leaning/
    // stretching look, "angle = atan2(vx,-vy)" per the reference), sparks are
    // small round bright dots, and both composite additively so overlapping
    // flame actually brightens/glows instead of just stacking flat color.
    let mut fluid_pbuf = Vec::new();
    let mut fire_wisp_pbuf = Vec::new();
    let mut fire_spark_pbuf = Vec::new();
    state.effects.read().fill_particle_buf_split3(&mut fluid_pbuf, &mut fire_wisp_pbuf, &mut fire_spark_pbuf);
    log_fire_diagnostics(&fire_wisp_pbuf, &fire_spark_pbuf);

    // Metaball surface: accumulate every particle's round influence into a
    // field texture, then threshold that field into one connected, organically
    // deformed liquid blob — instead of the flat, separate squares a plain
    // per-particle quad draw would give. All fluid particles currently share
    // one configured RGB color, so a single field+composite pass covers them;
    // if RGB genuinely differs (e.g. two simultaneous streams from different
    // redeems), particles are grouped by RGB first so each group still gets
    // its own correctly-tinted blob.
    //
    // Grouping deliberately ignores ALPHA: each particle's alpha is
    // `life * color_a`, and life fades independently per particle (slightly
    // different decay rates), so nearby particles along the very same stream
    // routinely carry different quantized alpha bytes. Grouping on the full
    // (R,G,B,A) tuple split them into many tiny per-alpha groups that each
    // got their own isolated field pass — particles that should have fused
    // into one shape never even shared a texture together, so no threshold
    // or radius tuning could have fixed it. Track the group's *maximum*
    // alpha instead, so a still-fading-in/out stream isn't washed out by its
    // own faded tail-end particles.
    if !fluid_pbuf.is_empty() {
        let (canvas_w, canvas_h) = get_obs_canvas_resolution();
        // Snapshot the scene-so-far BEFORE we draw anything ourselves, so the
        // metaball shader can sample and warp what's actually behind it for
        // a real visual-distortion look, if a group's own settings want it.
        // Captured once regardless (cheap relative to everything else here)
        // rather than per-group, since which groups need it isn't known
        // until after they're resolved below. Null if unsupported — the
        // shader falls back to its flat-tinted look in that case.
        let backdrop_tex = unsafe { capture_backdrop_copy(fns, canvas_w, canvas_h) };
        // Grouped by (engine_id, color, visual-settings index) so
        // two simultaneous streams (different redeems, or multiple custom Lua triggers)
        // never bleed into each other, merge their metaballs across the canvas,
        // average opposing flow velocities, or overwrite each other's alpha fading states.
        // Stored in a BTreeMap to guarantee deterministic draw ordering (oldest engine first,
        // newer on top) and prevent frame-to-frame z-fighting or flickering.
        // (particles, vx_sum, vy_sum, count, max_alpha)
        let mut groups: std::collections::BTreeMap<(u32, u8, u8, u8, u32), (Vec<(f32, f32, f32)>, f32, f32, u32, f32)> = std::collections::BTreeMap::new();
        for chunk in fluid_pbuf.chunks_exact(11).take(4096) {
            let (px, py, rad) = (chunk[0], chunk[1], chunk[2]);
            let (r, g, b, a) = (chunk[3], chunk[4], chunk[5], chunk[6]);
            let (vx, vy) = (chunk[7], chunk[8]);
            let settings_idx = chunk[9] as u32;
            let engine_id = chunk[10] as u32;
            if a > 0.02 && rad > 0.5 {
                let key = pack_argb(r, g, b, 1.0).to_be_bytes();
                let entry = groups.entry((engine_id, key[1], key[2], key[3], settings_idx)).or_insert_with(|| (Vec::new(), 0.0, 0.0, 0, 0.0));
                entry.0.push((px, py, rad));
                entry.1 += vx;
                entry.2 += vy;
                entry.3 += 1;
                entry.4 = entry.4.max(a);
            }
        }
        for ((_engine_id, r, g, b, settings_idx), (particles, vx_sum, vy_sum, count, max_alpha)) in groups {
            let visual = crate::fluid::water_visual_settings(settings_idx).unwrap_or_default();
            let field_tex = unsafe { render_fluid_field(fns, &particles, canvas_w, canvas_h, visual.metaball_size) };
            if !field_tex.is_null() {
                let n = count.max(1) as f32;
                let flow = (vx_sum / n, vy_sum / n);
                let group_backdrop = if visual.refraction_enabled || visual.texture_path.is_some() { backdrop_tex } else { std::ptr::null_mut() };
                unsafe { draw_fluid_metaball(fns, field_tex, group_backdrop, canvas_w, canvas_h, (r, g, b, max_alpha), flow, &visual); }
            }
        }
    }

    if !fire_wisp_pbuf.is_empty() || !fire_spark_pbuf.is_empty() {
        if let Some(blend_func) = fns.gs_blend_function {
            unsafe { blend_func(4, 1); } // GS_BLEND_SRCALPHA, GS_BLEND_ONE -> additive glow
        }

        // Glow Size (the 10th float) stretches each glow from the hot core's
        // size (0) past the default (1, 1.6 × 2.6 radii) — see FireConfig::glow_size
        for chunk in fire_wisp_pbuf.chunks_exact(10).take(2048) {
            let (px, py, rad) = (chunk[0], chunk[1], chunk[2]);
            let (r, g, b, a) = (chunk[3], chunk[4], chunk[5], chunk[6]);
            let (vx, vy) = (chunk[7], chunk[8]);
            let glow = chunk[9];
            if a <= 0.02 || rad <= 0.5 {
                continue;
            }
            let angle = vx.atan2(-vy); // orient the wisp's long axis along its velocity
            let alpha_bucket = (a.clamp(0.0, 1.0) * 255.0) as u8;

            let outer_r = (r.clamp(0.0, 1.0) * 255.0) as u8;
            let outer_g = (g.clamp(0.0, 1.0) * 255.0) as u8;
            let outer_b = (b.clamp(0.0, 1.0) * 255.0) as u8;
            let outer_tex = unsafe { get_or_create_fire_glow_tex(fns, outer_r, outer_g, outer_b, alpha_bucket) };
            let (outer_w, outer_h) = (rad * 0.8 * (1.0 + glow), rad * 1.3 * (1.0 + glow));
            unsafe { draw_glow_sprite_ex(fns, outer_tex, px, py, outer_w, outer_h, angle, 1.0); }

            // Hot near-white core, tinted from the same color, smaller and centered.
            let hot_r = ((r * 0.5 + 0.5).clamp(0.0, 1.0) * 255.0) as u8;
            let hot_g = ((g * 0.5 + 0.5).clamp(0.0, 1.0) * 255.0) as u8;
            let hot_b = ((b * 0.5 + 0.5).clamp(0.0, 1.0) * 255.0) as u8;
            let hot_tex = unsafe { get_or_create_fire_glow_tex(fns, hot_r, hot_g, hot_b, alpha_bucket) };
            unsafe { draw_glow_sprite_ex(fns, hot_tex, px, py, rad * 0.8, rad * 1.3, angle, 1.0); }
        }

        for chunk in fire_spark_pbuf.chunks_exact(10).take(1024) {
            let (px, py, rad) = (chunk[0], chunk[1], chunk[2]);
            let (r, g, b, a) = (chunk[3], chunk[4], chunk[5], chunk[6]);
            let glow = chunk[9];
            if a <= 0.02 || rad <= 0.5 {
                continue;
            }
            let r8 = (r.clamp(0.0, 1.0) * 255.0) as u8;
            let g8 = (g.clamp(0.0, 1.0) * 255.0) as u8;
            let b8 = (b.clamp(0.0, 1.0) * 255.0) as u8;
            let tex = unsafe { get_or_create_fire_glow_tex(fns, r8, g8, b8, (a.clamp(0.0, 1.0) * 255.0) as u8) };
            unsafe { draw_glow_sprite(fns, tex, px, py, rad * 0.8 * (1.0 + glow)); }
        }

        if let Some(blend_func) = fns.gs_blend_function {
            unsafe { blend_func(4, 5); } // restore GS_BLEND_SRCALPHA, GS_BLEND_INVSRCALPHA
        }
    }

    // 2. Lightning bolt segments, drawn as thick rotated rectangles. Core stroke
    // uses normal alpha blending (crisp bright line); the wider glow duplicates
    // are composited additively so they actually bloom/brighten instead of just
    // washing out as a faint translucent overlay (their old, invisible-looking behavior).
    let mut core_sbuf = Vec::new();
    let mut glow_sbuf = Vec::new();
    state.effects.read().fill_segment_buf_split(&mut core_sbuf, &mut glow_sbuf);

    let draw_segments = |buf: &[f32]| {
        for chunk in buf.chunks_exact(9) {
            let (x1, y1, x2, y2) = (chunk[0], chunk[1], chunk[2], chunk[3]);
            let (r, g, b, a, w) = (chunk[4], chunk[5], chunk[6], chunk[7], chunk[8]);
            if a > 0.02 {
                let (dx, dy) = (x2 - x1, y2 - y1);
                let len = (dx * dx + dy * dy).sqrt().max(1.0);
                let angle = dy.atan2(dx);
                draw_quad((x1 + x2) * 0.5, (y1 + y2) * 0.5, len, w.max(1.5), angle, pack_argb(r, g, b, a));
            }
        }
    };

    draw_segments(&core_sbuf);
    if !glow_sbuf.is_empty() {
        if let Some(blend_func) = fns.gs_blend_function {
            unsafe { blend_func(4, 1); } // GS_BLEND_SRCALPHA, GS_BLEND_ONE -> additive bloom
        }
        draw_segments(&glow_sbuf);
        if let Some(blend_func) = fns.gs_blend_function {
            unsafe { blend_func(4, 5); } // restore GS_BLEND_SRCALPHA, GS_BLEND_INVSRCALPHA
        }
    }

    // 3. Lightning impact flash: a genuinely round, gradually-fading radial glow
    // (texture-based gradient) instead of hard-edged concentric squares/rings.
    {
        let slots_guard = state.target_slots.read();
        let masks_guard = state.masks.read();
        let mut ibuf: Vec<f32> = Vec::new();
        state.effects.read().fill_lightning_impact_buf(&mut ibuf, &*slots_guard, &*masks_guard);
        for chunk in ibuf.chunks_exact(17) {
            let (tx, ty) = (chunk[0], chunk[1]);
            let (r, g, b, alpha, base_radius) = (chunk[2], chunk[3], chunk[4], chunk[5], chunk[6]);
            if alpha < 0.02 || base_radius < 1.0 {
                continue;
            }
            let r8 = (r.clamp(0.0, 1.0) * 255.0) as u8;
            let g8 = (g.clamp(0.0, 1.0) * 255.0) as u8;
            let b8 = (b.clamp(0.0, 1.0) * 255.0) as u8;
            let bucket = (alpha.clamp(0.0, 1.0) * 255.0) as u8;

            // Big, soft outer glow — noticeably larger & more gradual than before.
            let outer_tex = unsafe { get_or_create_radial_glow_tex(fns, r8, g8, b8, bucket) };
            unsafe { draw_glow_sprite(fns, outer_tex, tx, ty, base_radius * 2.6); }

            // Small hot core, tinted toward white.
            let core_r = ((r * 0.6 + 0.4).clamp(0.0, 1.0) * 255.0) as u8;
            let core_g = ((g * 0.6 + 0.4).clamp(0.0, 1.0) * 255.0) as u8;
            let core_b = ((b * 0.6 + 0.4).clamp(0.0, 1.0) * 255.0) as u8;
            let core_tex = unsafe { get_or_create_radial_glow_tex(fns, core_r, core_g, core_b, bucket) };
            unsafe { draw_glow_sprite(fns, core_tex, tx, ty, base_radius * 0.9); }
        }
    }

    // Reset blend state so throwables use normal alpha blending
    if let Some(reset_blend) = fns.gs_reset_blend_state {
        unsafe { reset_blend(); }
    } else if let Some(blend_func) = fns.gs_blend_function {
        unsafe { blend_func(4, 5); } // GS_BLEND_SRCALPHA, GS_BLEND_INVSRCALPHA
    }

    // 4. Physics throwables & shatter particles
    let total_objs = {
        let p = state.physics.read();
        p.throwables.len() + p.particles.len()
    };
    let buf_len = total_objs.max(2048);
    let mut instances = vec![crate::physics::RenderInstance {
        x: 0.0, y: 0.0, rotation: 0.0, scale: 1.0, cur_frame: 0, redeem_id: 0,
        is_particle: 0, r: 1.0, g: 1.0, b: 1.0, a: 1.0, size: 0.0,
    }; buf_len];
    let inst_count = state.physics.read().get_render_instances(&mut instances);
    for inst in &instances[..inst_count] {
        if inst.is_particle == 1 {
            let size = inst.size.max(2.0);
            draw_quad(inst.x, inst.y, size, size, 0.0, pack_argb(inst.r, inst.g, inst.b, inst.a));
        } else {
            if let Some((tex, tw, th)) = unsafe { get_or_create_redeem_texture(fns, inst.redeem_id, inst.cur_frame) } {
                let base_size = 64.0 * inst.scale;
                let max_dim = (tw.max(th) as f32).max(1.0);
                let draw_w = (tw as f32 / max_dim) * base_size;
                let draw_h = (th as f32 / max_dim) * base_size;
                let angle_rad = inst.rotation.to_radians();
                unsafe {
                    draw_glow_sprite_ex(fns, tex, inst.x, inst.y, draw_w, draw_h, angle_rad, inst.a);
                }
            } else {
                // Placeholder sprite (matches the Scene Studio reference look) until
                // real per-redeem sprite textures are wired up for the native render.
                let size = (32.0 * inst.scale).max(10.0);
                draw_quad(inst.x, inst.y, size + 4.0, size + 4.0, 0.0, pack_argb(1.0, 1.0, 1.0, 0.94 * inst.a));
                draw_quad(inst.x, inst.y, size, size, 0.0, pack_argb(1.0, 0.549, 0.157, 0.94 * inst.a));
            }
        }
    }
}

unsafe extern "C" fn source_video_render(data: *mut c_void, effect: *mut c_void) {
    let _timer = RenderTimer(std::time::Instant::now());
    // However it ends: the items above the layer are OBS's to draw as usual
    let _drawn = crate::render_gate::LayerDrawn;
    let count = RENDER_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    if count % 300 == 0 {
        println!("[Trust Source] 🎨 video_render running in OBS (frame={})", count);
    }
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    crate::state::get_state().studio_throwables_visible.store(true, std::sync::atomic::Ordering::Relaxed);
    crate::state::get_state().studio_throwables_last_render_ms.store(now_ms, std::sync::atomic::Ordering::Relaxed);

    // NEVER look sources up by name (obs_get_source_by_name, which takes OBS's
    // source list lock) anywhere in this render: it runs inside the scene's
    // render, holding the scene's lock, while OBS saving the scene collection
    // (autosave, exit) holds the source list lock and waits for that scene's
    // lock — OBS froze. Pending Studio moves and the Studio's targets are
    // applied and loaded from source_video_tick instead, outside any scene lock.

    // Lazy load graphics function pointers
    if GRAPHICS_FNS.read().is_none() {
        if let Some(fns) = unsafe { init_graphics_fns() } {
            *GRAPHICS_FNS.write() = Some(fns);
        }
    }

    let (cw, ch) = get_obs_canvas_resolution();

    let fns_opt = GRAPHICS_FNS.read();
    if let Some(ref fns) = *fns_opt {
        // The combined render's texture, made anew when the canvas size changes
        let mut composite = COMPOSITE_TEXRENDER.lock();
        if composite.0 != 0 && composite.1 != (cw, ch) {
            unsafe { (fns.gs_texrender_destroy)(composite.0 as *mut c_void); }
            composite.0 = 0;
        }
        if composite.0 == 0 {
            let ptr = unsafe { (fns.gs_texrender_create)(3, 0) }; // GS_RGBA = 3, GS_ZS_NONE = 0
            if ptr.is_null() {
                return;
            }
            *composite = (ptr as usize, (cw, ch));
        }
        let texrender = composite.0 as *mut c_void;

        // Copied back to the CPU only for the Studio's preview, so only while
        // it's open, and at the preview's size
        let (pw, ph) = preview_size(cw, ch);
        let preview_staging = if !crate::studio_app::is_studio_native_open() {
            None
        } else if count % PREVIEW_EVERY.load(std::sync::atomic::Ordering::Relaxed).max(1) == 0 {
            unsafe { preview_staging_turn(fns, pw, ph) }
        } else {
            // Not this frame (⚡ Performance): the frames in between still count
            PREVIEW_STAGING.lock().skip();
            None
        };

        if !texrender.is_null() {
            unsafe { (fns.gs_texrender_reset)(texrender); }
        }

        if !texrender.is_null() && unsafe { (fns.gs_texrender_begin)(texrender, cw, ch) } {
            // Theater's own pass: every source draws in it (render_gate)
            let _pass = crate::render_gate::theater_pass();
            let clear_color = [0.0f32, 0.0, 0.0, 0.0];
            unsafe {
                (fns.gs_clear)(1, clear_color.as_ptr(), 0.0, 0);

                // Extended camera near & far planes to prevent 3D perspective depth cropping
                (fns.gs_projection_push)();
                (fns.gs_ortho)(0.0, cw as f32, 0.0, ch as f32, -32768.0, 32768.0);

                // A real black color_source, rendered first — the combined
                // render's background is a genuine source like everything
                // else, not a raw quad or a per-item clear. Toggleable from
                // the Studio toolbar (default on).
                let black_bg = if crate::state::get_state().studio_black_backdrop_enabled.load(std::sync::atomic::Ordering::Relaxed) {
                    get_or_create_black_backdrop(fns, cw, ch)
                } else {
                    std::ptr::null_mut()
                };
                if !black_bg.is_null() {
                    (fns.obs_source_video_render)(black_bg);
                }
            }

            // 100% Native Pure Rust Scene Item Rendering
            unsafe {
                (fns.gs_blend_state_push)();
                if let Some(reset_blend) = fns.gs_reset_blend_state {
                    reset_blend();
                }
                (fns.gs_enable_blending)(true);
            }

            // A copy of the table, not a read lock held through the whole
            // render (which re-reads it in nested calls)
            let scene_fns_copy = get_or_init_scene_fns();
            if let Some(ref scene_fns) = scene_fns_copy {
                // The scene live in OBS. No lookup by name when there's none
                // (OBS starting or closing): nothing to draw then, and a lookup
                // here could freeze OBS (see the top of this function).
                let scene_source = if let Some(get_cur) = scene_fns.obs_frontend_get_current_scene {
                    unsafe { get_cur() }
                } else {
                    std::ptr::null_mut()
                };

                if !scene_source.is_null() {
                    let cur_scene_name = {
                        let name_ptr = unsafe { (scene_fns.obs_source_get_name)(scene_source) };
                        if !name_ptr.is_null() {
                            unsafe { std::ffi::CStr::from_ptr(name_ptr).to_string_lossy().to_string() }
                        } else {
                            String::new()
                        }
                    };

                    if !cur_scene_name.is_empty() {
                        let prev_sc = crate::state::get_state().studio_scene_name.read().clone();
                        if prev_sc != cur_scene_name {
                            println!("[Trust OBS] 🎬 Active scene switched from '{}' to '{}'", prev_sc, cur_scene_name);
                            *crate::state::get_state().studio_scene_name.write() = cur_scene_name.clone();
                        }
                    }

                    let scene = unsafe { (scene_fns.obs_scene_from_source)(scene_source) };
                    if !scene.is_null() {
                        // Keep the per-item Studio names current (a copy added in OBS gets its own)
                        refresh_scene_item_keys(scene_fns, &cur_scene_name, scene);
                        if let Some(enum_items) = scene_fns.obs_scene_enum_items {
                            let ctx = RenderSceneItemContext {
                                scene_fns,
                                gfx_fns: fns,
                                scene_name: cur_scene_name,
                            };
                            unsafe {
                                enum_items(
                                    scene,
                                    Some(rust_render_scene_item_cb),
                                    &ctx as *const _ as *mut c_void,
                                );
                            }
                        }
                    }
                    unsafe { (scene_fns.obs_source_release)(scene_source); }
                }
            }

            // 🎯 FRAME-MASK LOCKSTEP SYNCHRONIZATION:
            // All scene items and targets for THIS frame have now been rendered and scanned
            // into `state.masks` and `state.hitboxes`. We step physics and effects right now
            // using the fresh mask from this exact frame, guaranteeing 100% lockstep without
            // the 1-frame tick/render skew!
            {
                let raw_dt_bits = LAST_TICK_SECONDS.load(std::sync::atomic::Ordering::Relaxed);
                let dt = f32::from_bits(raw_dt_bits).clamp(0.001, 0.1);
                let dt_ms = dt * 1000.0;
                let state = crate::state::get_state();
                state.physics.write().step(dt, cw as f32, ch as f32);

                let hitboxes = state.hitboxes.read();
                let slots = state.target_slots.read();
                let masks = state.masks.read();
                state.effects.write().step(dt_ms, cw as f32, ch as f32, &*hitboxes, &*slots, &*masks);
                RENDER_ACTIVE_FLAG.store(true, std::sync::atomic::Ordering::Relaxed);
            }

            unsafe { render_throwable_layer(fns); }

            if let Some(cb) = *LUA_RENDER_CB.read() {
                unsafe { cb(data, effect); }
            }
            unsafe {
                (fns.gs_blend_state_pop)();
                (fns.gs_projection_pop)();
                (fns.gs_texrender_end)(texrender);
            }

            let tex = unsafe { (fns.gs_texrender_get_texture)(texrender) };
            drop(composite);
            if !tex.is_null() {
                unsafe {
                    (fns.gs_blend_state_push)();
                    if let Some(reset_blend) = fns.gs_reset_blend_state {
                        reset_blend();
                    }
                    (fns.gs_enable_blending)(true);
                    if let Some(blend_func) = fns.gs_blend_function {
                        blend_func(1, 5); // GS_BLEND_ONE, GS_BLEND_INVSRCALPHA
                    }
                }

                let draw_effect = if !effect.is_null() {
                    effect
                } else {
                    unsafe { (fns.obs_get_base_effect)(0) }
                };
                // Screen Shake is applied to every real scene item's OBS position
                // instead (see `apply_screen_shake_to_scene`, called from
                // `source_video_tick`) — jittering only this blit would just rumble
                // this one source's own redrawn copy, not the sources OBS actually
                // composites independently (avatar, other images, nested scenes).
                if studio_screen_now() {
                    // OBS gets the "Studio in use" screen; the Studio, this frame
                    unsafe { draw_studio_screen(cw, ch) };
                } else if !draw_effect.is_null() {
                    unsafe {
                        while (fns.gs_effect_loop)(draw_effect, b"Draw\0".as_ptr() as *const c_char) {
                            (fns.obs_source_draw)(tex, 0, 0, cw, ch, false);
                        }
                    }
                } else {
                    unsafe { (fns.obs_source_draw)(tex, 0, 0, cw, ch, false); }
                }

                unsafe {
                    (fns.gs_blend_state_pop)();
                }

                // The Studio preview's copy: this frame staged, an older one read back (StagingRing)
                if let Some((stage_into, read_back)) = preview_staging {
                    let started = std::time::Instant::now();
                    unsafe {
                        let small = scale_for_preview(fns, tex, cw, ch, pw, ph);
                        if !small.is_null() {
                            (fns.gs_stage_texture)(stage_into, small);
                        }
                        if let Some(surf) = read_back {
                            publish_preview_frame(fns, surf, pw, ph, (cw, ch));
                        }
                    }
                    let took = started.elapsed();
                    with_render_times(|t| t.preview += took);
                }
            }
            return;
        }
    }

    // Direct fallback render if offscreen capture is unavailable
    if let Some(cb) = *LUA_RENDER_CB.read() {
        unsafe { cb(data, effect); }
    }
}

static OBS_SOURCE_INFO: ObsSourceInfo = ObsSourceInfo {
    id: SOURCE_ID.as_ptr() as *const c_char,
    type_: 0, // OBS_SOURCE_TYPE_INPUT
    // OBS_SOURCE_VIDEO | OBS_SOURCE_AUDIO (redeem sounds play as its audio, see sound.rs) | OBS_SOURCE_INTERACTION
    output_flags: (1 << 0) | (1 << 1) | (1 << 5),
    get_name: Some(source_get_name),
    create: Some(source_create),
    destroy: Some(source_destroy),
    get_width: Some(source_get_width),
    get_height: Some(source_get_height),
    defaults: None,
    get_properties: None,
    update: None,
    activate: None,
    deactivate: None,
    show: None,
    hide: None,
    video_tick: Some(source_video_tick),
    video_render: Some(source_video_render),
};

type ObsRegisterSourceFn = unsafe extern "C" fn(info: *const ObsSourceInfo, size: usize);

#[cfg(target_os = "linux")]
unsafe extern "C" {
    fn dlopen(filename: *const c_char, flags: i32) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
}

pub fn trust_log(msg: &str) {
    // Not from the tests: their lines landed among OBS's own and read as OBS's
    #[cfg(all(target_os = "windows", not(test)))]
    {
        use std::io::Write;
        #[repr(C)]
        #[derive(Default)]
        struct SystemTime {
            year: u16,
            month: u16,
            day_of_week: u16,
            day: u16,
            hour: u16,
            minute: u16,
            second: u16,
            milliseconds: u16,
        }
        unsafe extern "system" {
            fn GetLocalTime(time: *mut SystemTime);
        }
        if let Ok(appdata) = std::env::var("APPDATA") {
            let log_dir = std::path::PathBuf::from(appdata).join("obs-studio").join("logs");
            let _ = std::fs::create_dir_all(&log_dir);
            let log_path = log_dir.join("trust_debug.log");
            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(log_path) {
                // Timed like OBS's own log, to read the two side by side
                let mut t = SystemTime::default();
                unsafe { GetLocalTime(&mut t) };
                let _ = writeln!(f, "{:02}:{:02}:{:02}.{:03}: {}", t.hour, t.minute, t.second, t.milliseconds, msg);
            }
        }
        let wide: Vec<u16> = msg.encode_utf16().chain(std::iter::once(0)).collect();
        unsafe {
            unsafe extern "system" {
                fn OutputDebugStringW(lpOutputString: *const u16);
            }
            OutputDebugStringW(wide.as_ptr());
        }
    }
    println!("{}", msg);
}

#[cfg(target_os = "windows")]
unsafe fn dlopen(filename: *const c_char, _flags: i32) -> *mut c_void {
    unsafe extern "system" {
        fn LoadLibraryA(lpLibFileName: *const c_char) -> *mut c_void;
        fn GetModuleHandleA(lpModuleName: *const c_char) -> *mut c_void;
        fn SetDllDirectoryA(lpPathName: *const c_char) -> i32;
        fn GetLastError() -> u32;
    }
    unsafe {
        if filename.is_null() {
            return GetModuleHandleA(std::ptr::null());
        }
        let cstr = CStr::from_ptr(filename);
        let s = cstr.to_string_lossy();
        let win_name = if s.contains("obs-frontend-api") {
            "obs-frontend-api.dll\0"
        } else if s.contains("libobs") || s.contains("obs.dll") {
            "obs.dll\0"
        } else {
            let h = GetModuleHandleA(filename);
            if !h.is_null() {
                trust_log(&format!("[Trust DL] GetModuleHandleA hit for '{}': {:p}", s, h));
                return h;
            }
            SetDllDirectoryA(b"C:\\Program Files\\obs-studio\\bin\\64bit\0".as_ptr() as *const c_char);
            let res = LoadLibraryA(filename);
            if res.is_null() {
                let err = GetLastError();
                trust_log(&format!("[Trust DL] ⚠️ LoadLibraryA failed for '{}', error={}", s, err));
            } else {
                trust_log(&format!("[Trust DL] ✅ LoadLibraryA succeeded for '{}', handle={:p}", s, res));
            }
            return res;
        };
        let h = GetModuleHandleA(win_name.as_ptr() as *const c_char);
        if !h.is_null() {
            return h;
        }
        LoadLibraryA(win_name.as_ptr() as *const c_char)
    }
}


#[cfg(target_os = "windows")]
unsafe fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void {
    unsafe extern "system" {
        fn GetProcAddress(hModule: *mut c_void, lpProcName: *const c_char) -> *mut c_void;
        fn GetModuleHandleA(lpModuleName: *const c_char) -> *mut c_void;
    }
    unsafe {
        if !handle.is_null() {
            return GetProcAddress(handle, symbol);
        }
        // Handle is NULL: probe obs.dll, obs-frontend-api.dll, then host executable
        let mut sym = std::ptr::null_mut();
        let obs = GetModuleHandleA(b"obs.dll\0".as_ptr() as *const c_char);
        if !obs.is_null() {
            sym = GetProcAddress(obs, symbol);
            if !sym.is_null() { return sym; }
        }
        let frontend = GetModuleHandleA(b"obs-frontend-api.dll\0".as_ptr() as *const c_char);
        if !frontend.is_null() {
            sym = GetProcAddress(frontend, symbol);
            if !sym.is_null() { return sym; }
        }
        let main_exe = GetModuleHandleA(std::ptr::null());
        if !main_exe.is_null() {
            sym = GetProcAddress(main_exe, symbol);
        }
        sym
    }
}


static REGISTERED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[unsafe(no_mangle)]
pub extern "C" fn trust_register_obs_source() -> bool {
    let sym_init = b"obs_initialized\0";
    let sym_name = b"obs_register_source_s\0";
    
    // Search in libobs.so.0, libobs.so, libobs.so.30, or process memory
    let mut handle = unsafe { dlopen(b"libobs.so.0\0".as_ptr() as *const c_char, 1) };
    if handle.is_null() {
        handle = unsafe { dlopen(b"libobs.so\0".as_ptr() as *const c_char, 1) };
    }
    if handle.is_null() {
        handle = unsafe { dlopen(b"libobs.so.30\0".as_ptr() as *const c_char, 1) };
    }
    if handle.is_null() {
        handle = std::ptr::null_mut();
    }

    // Verify that OBS core is actually running and initialized
    let init_ptr = unsafe {
        let mut p = if !handle.is_null() { dlsym(handle, sym_init.as_ptr() as *const c_char) } else { std::ptr::null_mut() };
        if p.is_null() { p = dlsym(std::ptr::null_mut(), sym_init.as_ptr() as *const c_char); }
        p
    };
    if !init_ptr.is_null() {
        let obs_init_fn: unsafe extern "C" fn() -> bool = unsafe { std::mem::transmute(init_ptr) };
        if !unsafe { obs_init_fn() } {
            // OBS core is null (running in standalone test process without OBS host)
            return false;
        }
    } else {
        return false;
    }

    if REGISTERED.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return true; // Already registered once!
    }

    // theater_chat.lua loads a second copy of this DLL, whose trust_init lands
    // here too, and OBS doesn't refuse a source type registered twice (it
    // shows up twice in Add Source). Register it once per OBS, not per copy.
    let get_display_name_ptr = unsafe { dlsym(std::ptr::null_mut(), b"obs_source_get_display_name\0".as_ptr() as *const c_char) };
    if !get_display_name_ptr.is_null() {
        let get_display_name: unsafe extern "C" fn(*const c_char) -> *const c_char = unsafe { std::mem::transmute(get_display_name_ptr) };
        if !unsafe { get_display_name(SOURCE_ID.as_ptr() as *const c_char) }.is_null() {
            trust_log("[Trust Core] 🎬 Throwables Layer source already registered (another copy of Theater Scene); not registering it twice.");
            return true;
        }
    }

    crate::studio_app::load_studio_targets_from_disk();

    let mut fn_ptr = if !handle.is_null() {
        unsafe { dlsym(handle, sym_name.as_ptr() as *const c_char) }
    } else {
        std::ptr::null_mut()
    };
    if fn_ptr.is_null() {
        fn_ptr = unsafe { dlsym(std::ptr::null_mut(), sym_name.as_ptr() as *const c_char) };
    }

    if !fn_ptr.is_null() {
        let register_fn: ObsRegisterSourceFn = unsafe { std::mem::transmute(fn_ptr) };
        unsafe {
            register_fn(&OBS_SOURCE_INFO, std::mem::size_of::<ObsSourceInfo>());
        }
        println!("[Trust Core] 🎬 Registered OBS Source: 🎬 Theater Scene — Throwables Layer");
        true
    } else {
        println!("[Trust Core] ⚠️ obs_register_source_s not found");
        false
    }
}

pub(crate) type ObsFrontendCb = unsafe extern "C" fn(private_data: *mut c_void);
type ObsFrontendGetMainWindowFn = unsafe extern "C" fn() -> *mut c_void;
pub(crate) type ObsTaskFn = unsafe extern "C" fn(param: *mut c_void);
pub(crate) type ObsQueueTaskFn = unsafe extern "C" fn(task_type: u32, task: ObsTaskFn, param: *mut c_void, wait: bool);
type TrustUiSetMenuTitleFn = unsafe extern "C" fn(title: *const c_char);
pub(crate) type TrustUiAddMenuActionFn =
    unsafe extern "C" fn(action_title: *const c_char, callback: ObsFrontendCb, user_data: *mut c_void);

pub(crate) const OBS_TASK_UI: u32 = 0;

unsafe extern "C" fn open_studio_window_menu_cb(_private_data: *mut c_void) {
    crate::trust_studio_open_native_window();
}

pub(crate) fn open_frontend_api() -> *mut c_void {
    let mut handle = unsafe { dlopen(b"obs-frontend-api.dll\0".as_ptr() as *const c_char, 1) };
    if handle.is_null() {
        handle = unsafe { dlopen(b"libobs-frontend-api.so.0\0".as_ptr() as *const c_char, 1) };
    }
    if handle.is_null() {
        handle = unsafe { dlopen(b"libobs-frontend-api.so\0".as_ptr() as *const c_char, 1) };
    }
    if handle.is_null() {
        handle = unsafe { dlopen(b"libobs-frontend-api.so.30\0".as_ptr() as *const c_char, 1) };
    }
    handle
}

pub(crate) fn open_libobs_core() -> *mut c_void {
    let mut handle = unsafe { dlopen(b"obs.dll\0".as_ptr() as *const c_char, 1) };
    if handle.is_null() {
        handle = unsafe { dlopen(b"libobs.so.0\0".as_ptr() as *const c_char, 1) };
    }
    if handle.is_null() {
        handle = unsafe { dlopen(b"libobs.so\0".as_ptr() as *const c_char, 1) };
    }
    if handle.is_null() {
        handle = unsafe { dlopen(b"libobs.so.30\0".as_ptr() as *const c_char, 1) };
    }
    handle
}

/// Locates the general-purpose Qt UI toolkit (trimmed qt_dialog.cpp build) —
/// not an OBS module itself, just a plain library Rust dlopens for the
/// widget/window/dock/menu-bar functions it doesn't reimplement natively.
pub(crate) fn open_trust_qt_lib() -> *mut c_void {
    #[cfg(target_os = "windows")]
    {
        unsafe extern "system" {
            fn GetModuleHandleExW(dwFlags: u32, lpModuleName: *const u16, phModule: *mut *mut std::ffi::c_void) -> i32;
            fn GetModuleFileNameW(hModule: *mut std::ffi::c_void, lpFilename: *mut u16, nSize: u32) -> u32;
        }
        let mut h_module: *mut std::ffi::c_void = std::ptr::null_mut();
        unsafe {
            GetModuleHandleExW(0x00000004 | 0x00000002, open_trust_qt_lib as *const u16, &mut h_module);
        }
        if !h_module.is_null() {
            let mut buf = [0u16; 1024];
            let len = unsafe { GetModuleFileNameW(h_module, buf.as_mut_ptr(), buf.len() as u32) };
            if len > 0 {
                let dll_path = std::path::PathBuf::from(String::from_utf16_lossy(&buf[..len as usize]));
                if let Some(dir) = dll_path.parent() {
                    let candidate = dir.join("trust_qt.dll");
                    if candidate.exists() {
                        if let Ok(cstr) = CString::new(candidate.to_string_lossy().to_string()) {
                            let handle = unsafe { dlopen(cstr.as_ptr(), 1) };
                            if !handle.is_null() {
                                trust_log(&format!("[Trust Core] 🎬 Loaded Qt toolkit relative to module: {}", candidate.display()));
                                return handle;
                            }
                        }
                    }
                }
            }
        }
    }

    // Then the plugin's usual folders, then the system's library search
    let mut candidates: Vec<String> = Vec::new();
    #[cfg(target_os = "windows")]
    for base in ["PROGRAMDATA", "APPDATA"] {
        if let Ok(dir) = std::env::var(base) {
            candidates.push(format!(r"{dir}\obs-studio\plugins\theater-scene\bin\64bit\trust_qt.dll"));
        }
    }
    #[cfg(not(target_os = "windows"))]
    if let Ok(home) = std::env::var("HOME") {
        candidates.push(format!("{home}/.config/obs-studio/plugins/theater-scene/bin/64bit/libtrust_qt.so"));
    }
    candidates.extend(["trust_qt.dll", "libtrust_qt.so"].map(String::from));
    for p in candidates {
        let Ok(c_path) = CString::new(p.as_str()) else { continue };
        let handle = unsafe { dlopen(c_path.as_ptr(), 1) };
        if !handle.is_null() {
            trust_log(&format!("[Trust Core] 🎬 Loaded Qt toolkit from: {}", p));
            return handle;
        }
    }
    trust_log("[Trust Core] ⚠️ Could not locate trust_qt library in any search path");
    std::ptr::null_mut()
}

pub(crate) unsafe fn frontend_sym(handle: *mut c_void, name: &[u8]) -> *mut c_void {
    let mut p = if !handle.is_null() { unsafe { dlsym(handle, name.as_ptr() as *const c_char) } } else { std::ptr::null_mut() };
    if p.is_null() {
        p = unsafe { dlsym(std::ptr::null_mut(), name.as_ptr() as *const c_char) };
    }
    p
}

struct MenuRegistrationCtx {
    set_title_fn: TrustUiSetMenuTitleFn,
    add_action_fn: TrustUiAddMenuActionFn,
    menu_title: CString,
    actions: Vec<(CString, ObsFrontendCb)>,
}

unsafe extern "C" fn run_menu_registration_on_ui_thread(param: *mut c_void) {
    let ctx = unsafe { Box::from_raw(param as *mut MenuRegistrationCtx) };
    unsafe {
        (ctx.set_title_fn)(ctx.menu_title.as_ptr());
        for (title, cb) in &ctx.actions {
            (ctx.add_action_fn)(title.as_ptr(), *cb, std::ptr::null_mut());
        }
    }
    // After the menu's own: registering it clears Theater's items
    crate::egui_docks::menu_ready(ctx.add_action_fn);
    trust_log(&format!("[Trust Core] 🎬 Registered '🎬 Theater Scene' menu with {} action(s).", ctx.actions.len()));
}

/// Creates a dedicated "🎬 Theater Scene" top-level menu (via the Qt UI
/// toolkit's QMenuBar-based functions) with a "Scene Studio Window" action,
/// instead of nesting under OBS's built-in Tools menu.
pub fn setup_frontend_tools_menu() {
    std::thread::spawn(|| {
        trust_log("[Trust Core] 🎬 setup_frontend_tools_menu background thread started");
        let frontend_handle = open_frontend_api();
        let get_main_window_ptr = if !frontend_handle.is_null() {
            unsafe { frontend_sym(frontend_handle, b"obs_frontend_get_main_window\0") }
        } else {
            std::ptr::null_mut()
        };

        if !get_main_window_ptr.is_null() {
            let get_main_window: ObsFrontendGetMainWindowFn = unsafe { std::mem::transmute(get_main_window_ptr) };
            for _ in 0..150 {
                if !unsafe { get_main_window() }.is_null() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        }
        trust_log("[Trust Core] 🎬 MainWindow ready check completed");

unsafe extern "C" fn run_tools_menu_registration_on_ui_thread(_param: *mut c_void) {
    let frontend_handle = open_frontend_api();
    if !frontend_handle.is_null() {
        let add_tools_item_ptr = unsafe { frontend_sym(frontend_handle, b"obs_frontend_add_tools_menu_item\0") };
        if !add_tools_item_ptr.is_null() {
            type ObsAddToolsMenuItemFn = unsafe extern "C" fn(name: *const c_char, cb: ObsFrontendCb, data: *mut c_void);
            let add_tools_item: ObsAddToolsMenuItemFn = unsafe { std::mem::transmute(add_tools_item_ptr) };
            let title = CString::new("🎬 Theater Scene Studio").unwrap();
            unsafe {
                add_tools_item(title.as_ptr(), open_studio_window_menu_cb, std::ptr::null_mut());
            }
            println!("[Trust Core] 🎬 Registered '🎬 Theater Scene Studio' in OBS Tools menu.");
        } else {
            println!("[Trust Core] ⚠️ obs_frontend_add_tools_menu_item symbol not found");
        }
    } else {
        println!("[Trust Core] ⚠️ frontend_handle is null for Tools menu item registration");
    }
}

        let qt_lib = open_trust_qt_lib();
        if qt_lib.is_null() {
            trust_log("[Trust Core] ⚠️ libtrust_qt not found for custom top-level menu; registering directly in OBS Tools menu");
            let core_handle = open_libobs_core();
            let queue_task_ptr = if !core_handle.is_null() {
                unsafe { frontend_sym(core_handle, b"obs_queue_task\0") }
            } else {
                std::ptr::null_mut()
            };
            if !queue_task_ptr.is_null() {
                let queue_task: ObsQueueTaskFn = unsafe { std::mem::transmute(queue_task_ptr) };
                unsafe {
                    queue_task(OBS_TASK_UI, run_tools_menu_registration_on_ui_thread, std::ptr::null_mut(), false);
                }
            } else {
                unsafe {
                    run_tools_menu_registration_on_ui_thread(std::ptr::null_mut());
                }
            }
            return;
        }
        let set_title_ptr = unsafe { frontend_sym(qt_lib, b"trust_ui_set_menu_title\0") };
        let add_action_ptr = unsafe { frontend_sym(qt_lib, b"trust_ui_add_menu_action\0") };
        if set_title_ptr.is_null() || add_action_ptr.is_null() {
            trust_log("[Trust Core] ⚠️ trust_ui_set_menu_title/add_menu_action not found in trust_qt library");
            return;
        }
        let set_title_fn: TrustUiSetMenuTitleFn = unsafe { std::mem::transmute(set_title_ptr) };
        let add_action_fn: TrustUiAddMenuActionFn = unsafe { std::mem::transmute(add_action_ptr) };
        let menu_title = CString::new("🎬 Theater Scene").unwrap();
        let actions = vec![
            (CString::new("Scene Studio Window").unwrap(), open_studio_window_menu_cb as ObsFrontendCb),
            (CString::new("Scene Switching Dock").unwrap(), show_scene_dock_menu_cb as ObsFrontendCb),
            (CString::new("Redeems Dock").unwrap(), show_redeem_dock_menu_cb as ObsFrontendCb),
            // (📡 Chat & Alerts is Theater Reader's, a plugin of its own, in its own group)
        ];

        let core_handle = open_libobs_core();
        let queue_task_ptr = if !core_handle.is_null() {
            unsafe { frontend_sym(core_handle, b"obs_queue_task\0") }
        } else {
            std::ptr::null_mut()
        };

        let ctx = Box::new(MenuRegistrationCtx { set_title_fn, add_action_fn, menu_title, actions });
        if !queue_task_ptr.is_null() {
            let queue_task: ObsQueueTaskFn = unsafe { std::mem::transmute(queue_task_ptr) };
            let param = Box::into_raw(ctx) as *mut c_void;
            unsafe {
                queue_task(OBS_TASK_UI, run_menu_registration_on_ui_thread, param, false);
            }
            trust_log("[Trust Core] 🎬 Queued '🎬 Theater Scene' top-level menu registration to OBS UI thread");
        } else {
            trust_log("[Trust Core] ⚠️ obs_queue_task not found; registering menu without UI-thread marshaling");
            let param = Box::into_raw(ctx) as *mut c_void;
            unsafe {
                run_menu_registration_on_ui_thread(param);
            }
        }
    });
}

pub(crate) type TrustUiCreateDockFn = unsafe extern "C" fn(dock_id: *const c_char, title: *const c_char) -> u32;
pub(crate) type TrustUiShowDockFn = unsafe extern "C" fn(dock_handle: u32);
type TrustUiAddButtonFn = unsafe extern "C" fn(
    container_handle: u32,
    label: *const c_char,
    style_type: *const c_char,
    callback: ObsFrontendCb,
    user_data: *mut c_void,
) -> u32;

static SCENE_DOCK_HANDLE: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
static REDEEM_DOCK_HANDLE: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

pub(crate) fn show_dock_by_handle(handle: u32) {
    if handle == 0 {
        return;
    }
    let qt_lib = open_trust_qt_lib();
    if qt_lib.is_null() {
        return;
    }
    let show_dock_ptr = unsafe { frontend_sym(qt_lib, b"trust_ui_show_dock\0") };
    if show_dock_ptr.is_null() {
        return;
    }
    let show_dock_fn: TrustUiShowDockFn = unsafe { std::mem::transmute(show_dock_ptr) };
    unsafe {
        show_dock_fn(handle);
    }
}

unsafe extern "C" fn show_scene_dock_menu_cb(_private_data: *mut c_void) {
    show_dock_by_handle(SCENE_DOCK_HANDLE.load(std::sync::atomic::Ordering::SeqCst));
}

unsafe extern "C" fn show_redeem_dock_menu_cb(_private_data: *mut c_void) {
    show_dock_by_handle(REDEEM_DOCK_HANDLE.load(std::sync::atomic::Ordering::SeqCst));
}

/// Button click handler for the Scene Switching dock. user_data is a leaked
/// `Box<CString>` holding the coordinate set's stable `id` (not an index, so
/// it stays correct even if sets are reordered — though this dock's button
/// list itself is only built once at startup, see setup_action_docks).
unsafe extern "C" fn scene_switch_button_cb(user_data: *mut c_void) {
    if user_data.is_null() {
        return;
    }
    let id_cstring = unsafe { Box::from_raw(user_data as *mut CString) };
    let id = id_cstring.to_string_lossy().to_string();
    std::mem::forget(id_cstring); // button may be clicked again later

    let target_set = crate::studio_app::SCENE_COORDINATE_SETS
        .read()
        .iter()
        .find(|s| s.id == id)
        .cloned();
    if let Some(set) = target_set {
        let sources = crate::state::get_state().studio_sources.read().clone();
        println!("[Trust Core] 🎬 Scene switch button: '{}'", set.name);
        crate::studio_app::start_scene_transition(&set, &sources);
    }
}

/// Button click handler for the Redeems dock. user_data is a leaked
/// `Box<CString>` holding the redeem's filename (its stable identifier).
unsafe extern "C" fn redeem_button_cb(user_data: *mut c_void) {
    if user_data.is_null() {
        return;
    }
    let filename_cstring = unsafe { Box::from_raw(user_data as *mut CString) };
    let filename = filename_cstring.to_string_lossy().to_string();
    std::mem::forget(filename_cstring);

    let config = crate::studio_app::GLOBAL_REDEEMS
        .read()
        .iter()
        .find(|r| r.filename == filename)
        .map(|r| r.config.clone());
    if let Some(config) = config {
        if let Some(json_str) = crate::studio_app::build_redeem_dispatch_json(&filename, &config) {
            let ok = crate::studio_app::dispatch_test_redeem_json(&json_str);
            println!("[Trust Core] 🎁 Redeem button: '{}' (ok={})", config.name, ok);
        }
    }
}

type TrustUiClearContainerFn = unsafe extern "C" fn(container_handle: u32);

struct ActionDocksCtx {
    create_dock_fn: TrustUiCreateDockFn,
    add_button_fn: TrustUiAddButtonFn,
    show_dock_fn: TrustUiShowDockFn,
    /// None with an older trust_qt build — the Scene Switching dock then can't rebuild its buttons
    clear_container_fn: Option<TrustUiClearContainerFn>,
}

pub(crate) type ObsFrontendEventCb = unsafe extern "C" fn(event: i32, private_data: *mut c_void);
pub(crate) type ObsFrontendEventCbRegFn = unsafe extern "C" fn(callback: ObsFrontendEventCb, private_data: *mut c_void);

// enum obs_frontend_event values
const OBS_FRONTEND_EVENT_SCENE_CHANGED: i32 = 8;
const OBS_FRONTEND_EVENT_SCENE_COLLECTION_CHANGED: i32 = 13;
pub(crate) const OBS_FRONTEND_EVENT_EXIT: i32 = 17;
const OBS_FRONTEND_EVENT_FINISHED_LOADING: i32 = 26;

/// trust_qt entry points the Scene Switching dock uses to (re)build its buttons after startup.
#[derive(Clone, Copy)]
struct SceneDockFns {
    add_button: TrustUiAddButtonFn,
    clear_container: Option<TrustUiClearContainerFn>,
}
static SCENE_DOCK_FNS: parking_lot::Mutex<Option<SceneDockFns>> = parking_lot::Mutex::new(None);

/// What the dock's buttons currently show: (scene, [(set id, set name)]).
static SCENE_DOCK_SHOWN: parking_lot::Mutex<Option<(String, Vec<(String, String)>)>> = parking_lot::Mutex::new(None);

/// A libobs (or obs-frontend-api) export by name, or null.
pub(crate) fn obs_symbol(name: &CStr) -> *mut c_void {
    unsafe { dlsym(std::ptr::null_mut(), name.as_ptr()) }
}

/// Name of the current OBS scene, or None until the scene collection has
/// loaded (which happens after the docks are created). Keeps
/// studio_scene_name in sync, like refresh_obs_scenes does.
pub(crate) fn current_obs_scene_name() -> Option<String> {
    let fns = init_scene_fns()?;
    let get_cur = fns.obs_frontend_get_current_scene?;
    let s = unsafe { get_cur() };
    if s.is_null() {
        return None;
    }
    let name_ptr = unsafe { (fns.obs_source_get_name)(s) };
    let name = if name_ptr.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(name_ptr).to_string_lossy().to_string() }
    };
    unsafe { (fns.obs_source_release)(s); }
    if name.is_empty() {
        return None;
    }
    let mut cur = crate::state::get_state().studio_scene_name.write();
    if *cur != name {
        *cur = name.clone();
    }
    Some(name)
}

/// (Re)fills the Scene Switching dock with one button per coordinate set saved
/// for the current OBS scene — the same scene filter the Studio's Coordinate
/// Sets list applies, so poses saved for other scenes (or another machine's
/// scenes) don't show up. No-op if the buttons already match. UI thread only.
unsafe fn refresh_scene_dock_buttons() {
    let handle = SCENE_DOCK_HANDLE.load(std::sync::atomic::Ordering::SeqCst);
    let Some(fns) = *SCENE_DOCK_FNS.lock() else { return };
    if handle == 0 {
        return;
    }

    let mut shown = SCENE_DOCK_SHOWN.lock();
    let Some(clear) = fns.clear_container else {
        // Older trust_qt without trust_ui_clear_container: buttons can't be
        // removed again, so keep the old behavior — every set, built once.
        if shown.is_none() {
            let all: Vec<(String, String)> = crate::studio_app::SCENE_COORDINATE_SETS
                .read()
                .iter()
                .map(|s| (s.id.clone(), s.name.clone()))
                .collect();
            unsafe { add_scene_dock_buttons(fns.add_button, handle, &all); }
            *shown = Some((String::new(), all));
        }
        return;
    };

    let scene = current_obs_scene_name().unwrap_or_default();
    let wanted = scene_dock_poses(&crate::studio_app::SCENE_COORDINATE_SETS.read(), &scene);
    if shown.as_ref().is_some_and(|(s, sets)| *s == scene && *sets == wanted) {
        return;
    }
    if shown.is_some() {
        unsafe { clear(handle); }
    }
    unsafe { add_scene_dock_buttons(fns.add_button, handle, &wanted); }
    trust_log(&format!("[Trust Core] 🎬 Scene Switching dock: {} pose(s) for scene '{}'.", wanted.len(), scene));
    *shown = Some((scene, wanted));
}

/// (id, name) of the poses the Scene Switching dock shows for `scene`: the
/// Studio's Coordinate Sets filter (sets saved for that scene, plus sets with
/// no scene recorded). Nothing while there's no current scene yet.
fn scene_dock_poses(sets: &[crate::studio_app::SceneCoordinateSet], scene: &str) -> Vec<(String, String)> {
    if scene.is_empty() {
        return Vec::new();
    }
    sets.iter()
        .filter(|s| s.scene == scene || s.scene.is_empty())
        .map(|s| (s.id.clone(), s.name.clone()))
        .collect()
}

unsafe fn add_scene_dock_buttons(add_button: TrustUiAddButtonFn, handle: u32, sets: &[(String, String)]) {
    for (id, name) in sets {
        let Ok(label) = CString::new(name.clone()) else { continue };
        let Ok(id) = CString::new(id.clone()) else { continue };
        let user_data = Box::into_raw(Box::new(id)) as *mut c_void;
        unsafe {
            add_button(handle, label.as_ptr(), std::ptr::null(), scene_switch_button_cb, user_data);
        }
    }
}

/// OBS delivers frontend events on the UI thread.
unsafe extern "C" fn scene_dock_frontend_event_cb(event: i32, _private_data: *mut c_void) {
    match event {
        OBS_FRONTEND_EVENT_FINISHED_LOADING | OBS_FRONTEND_EVENT_SCENE_CHANGED | OBS_FRONTEND_EVENT_SCENE_COLLECTION_CHANGED => {
            unsafe { refresh_scene_dock_buttons(); }
        }
        OBS_FRONTEND_EVENT_EXIT => {
            // The dock is torn down with the main window; stop touching it
            *SCENE_DOCK_FNS.lock() = None;
            let remove_ptr = unsafe { frontend_sym(open_frontend_api(), b"obs_frontend_remove_event_callback\0") };
            if !remove_ptr.is_null() {
                let remove: ObsFrontendEventCbRegFn = unsafe { std::mem::transmute(remove_ptr) };
                unsafe { remove(scene_dock_frontend_event_cb, std::ptr::null_mut()); }
            }
        }
        _ => {}
    }
}

unsafe extern "C" fn run_scene_dock_refresh_on_ui_thread(_param: *mut c_void) {
    unsafe { refresh_scene_dock_buttons(); }
}

/// Rebuilds the Scene Switching dock after poses were added/renamed/deleted
/// (called when the Studio saves). Safe from any thread — hops to the UI thread.
pub fn request_scene_dock_refresh() {
    if SCENE_DOCK_FNS.lock().is_none() {
        return;
    }
    let core_handle = open_libobs_core();
    let queue_task_ptr = if !core_handle.is_null() {
        unsafe { frontend_sym(core_handle, b"obs_queue_task\0") }
    } else {
        std::ptr::null_mut()
    };
    if !queue_task_ptr.is_null() {
        let queue_task: ObsQueueTaskFn = unsafe { std::mem::transmute(queue_task_ptr) };
        unsafe { queue_task(OBS_TASK_UI, run_scene_dock_refresh_on_ui_thread, std::ptr::null_mut(), false); }
    }
}

unsafe extern "C" fn run_action_docks_setup_on_ui_thread(param: *mut c_void) {
    let ctx = unsafe { Box::from_raw(param as *mut ActionDocksCtx) };

    let scene_dock_id = CString::new("theater_scene_dock").unwrap();
    let scene_dock_title = CString::new("🎬 Scene Switching").unwrap();
    let scene_dock_handle = unsafe { (ctx.create_dock_fn)(scene_dock_id.as_ptr(), scene_dock_title.as_ptr()) };
    if scene_dock_handle != 0 {
        SCENE_DOCK_HANDLE.store(scene_dock_handle, std::sync::atomic::Ordering::SeqCst);
        *SCENE_DOCK_FNS.lock() = Some(SceneDockFns {
            add_button: ctx.add_button_fn,
            clear_container: ctx.clear_container_fn,
        });
        // Usually no scene yet at this point (the collection loads later) —
        // FINISHED_LOADING / SCENE_CHANGED fill the dock in once it has one.
        unsafe { refresh_scene_dock_buttons(); }
        let add_cb_ptr = unsafe { frontend_sym(open_frontend_api(), b"obs_frontend_add_event_callback\0") };
        if !add_cb_ptr.is_null() {
            let add_cb: ObsFrontendEventCbRegFn = unsafe { std::mem::transmute(add_cb_ptr) };
            unsafe { add_cb(scene_dock_frontend_event_cb, std::ptr::null_mut()); }
        } else {
            trust_log("[Trust Core] ⚠️ obs_frontend_add_event_callback not found; Scene Switching dock won't follow scene changes");
        }
        unsafe {
            (ctx.show_dock_fn)(scene_dock_handle);
        }
        trust_log("[Trust Core] 🎬 Scene Switching dock created.");
    } else {
        trust_log("[Trust Core] ⚠️ trust_ui_create_dock returned a null handle (scene dock)");
    }

    let redeem_dock_id = CString::new("theater_redeem_dock").unwrap();
    let redeem_dock_title = CString::new("🎁 Redeems").unwrap();
    let redeem_dock_handle = unsafe { (ctx.create_dock_fn)(redeem_dock_id.as_ptr(), redeem_dock_title.as_ptr()) };
    if redeem_dock_handle != 0 {
        let redeems = crate::studio_app::GLOBAL_REDEEMS.read();
        let count = redeems.len();
        for item in redeems.iter() {
            let label_text = if item.config.name.is_empty() { item.filename.clone() } else { item.config.name.clone() };
            let Ok(label) = CString::new(label_text) else { continue };
            let Ok(filename) = CString::new(item.filename.clone()) else { continue };
            let user_data = Box::into_raw(Box::new(filename)) as *mut c_void;
            unsafe {
                (ctx.add_button_fn)(redeem_dock_handle, label.as_ptr(), std::ptr::null(), redeem_button_cb, user_data);
            }
        }
        drop(redeems);
        unsafe {
            (ctx.show_dock_fn)(redeem_dock_handle);
        }
        REDEEM_DOCK_HANDLE.store(redeem_dock_handle, std::sync::atomic::Ordering::SeqCst);
        trust_log(&format!("[Trust Core] 🎁 Redeems dock created with {} button(s).", count));
    } else {
        trust_log("[Trust Core] ⚠️ trust_ui_create_dock returned a null handle (redeem dock)");
    }

    // Now too, for the same reason: OBS's saved layout places them
    crate::egui_docks::create_docks(ctx.create_dock_fn, ctx.show_dock_fn);
}

/// Two real embedded OBS docks (QDockWidget, via libtrust_qt.so's already
/// existing, unmodified trust_ui_create_dock/add_button/show_dock): one
/// button per saved coordinate set ("Scene Switching") and one per loaded
/// redeem ("Redeems"), each doing exactly what its Studio-window "Test"
/// button does. Native QPushButtons — no egui/tiny-skia text rendering
/// involved, so none of the font-atlas rasterization issues apply here.
///
/// The Scene Switching dock shows the poses saved for the current OBS scene
/// and rebuilds on scene changes and Studio saves (refresh_scene_dock_buttons).
/// The Redeems list is built once at startup from GLOBAL_REDEEMS; adding/
/// removing a redeem later doesn't add/remove a button until next restart.
fn main_window_ready() -> bool {
    let frontend_handle = open_frontend_api();
    if frontend_handle.is_null() {
        return false;
    }
    let get_main_window_ptr = unsafe { frontend_sym(frontend_handle, b"obs_frontend_get_main_window\0") };
    if get_main_window_ptr.is_null() {
        return false;
    }
    let get_main_window: ObsFrontendGetMainWindowFn = unsafe { std::mem::transmute(get_main_window_ptr) };
    !unsafe { get_main_window() }.is_null()
}

/// trust_qt's dock functions, or None if the library or one of them is missing.
fn action_docks_ctx() -> Option<ActionDocksCtx> {
    let qt_lib = open_trust_qt_lib();
    if qt_lib.is_null() {
        return None;
    }
    let create_dock_ptr = unsafe { frontend_sym(qt_lib, b"trust_ui_create_dock\0") };
    let add_button_ptr = unsafe { frontend_sym(qt_lib, b"trust_ui_add_button\0") };
    let show_dock_ptr = unsafe { frontend_sym(qt_lib, b"trust_ui_show_dock\0") };
    if create_dock_ptr.is_null() || add_button_ptr.is_null() || show_dock_ptr.is_null() {
        return None;
    }
    let clear_container_ptr = unsafe { frontend_sym(qt_lib, b"trust_ui_clear_container\0") };
    Some(ActionDocksCtx {
        create_dock_fn: unsafe { std::mem::transmute(create_dock_ptr) },
        add_button_fn: unsafe { std::mem::transmute(add_button_ptr) },
        show_dock_fn: unsafe { std::mem::transmute(show_dock_ptr) },
        clear_container_fn: if clear_container_ptr.is_null() {
            None
        } else {
            Some(unsafe { std::mem::transmute::<*mut c_void, TrustUiClearContainerFn>(clear_container_ptr) })
        },
    })
}

pub fn setup_action_docks() {
    // obs_module_load runs on OBS's UI thread with its main window already
    // there: the docks are created right away, before OBS restores its saved
    // dock layout, so that layout places them. Created later (below: after OBS
    // finished starting), each was added to the right-hand side of the window
    // and then floated out of it, and Qt re-split that side: the other docks
    // there (the Control Panel's) came back at another size than they were saved.
    if main_window_ready() {
        match action_docks_ctx() {
            Some(ctx) => {
                trust_log("[Trust Core] 🎬 Creating action docks during module load (before OBS restores the dock layout)");
                unsafe { run_action_docks_setup_on_ui_thread(Box::into_raw(Box::new(ctx)) as *mut c_void) };
            }
            None => trust_log("[Trust Core] ⚠️ trust_qt library or its dock functions not found for action docks"),
        }
        return;
    }
    std::thread::spawn(|| {
        trust_log("[Trust Core] 🎬 setup_action_docks background thread started");
        for _ in 0..150 {
            if main_window_ready() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }

        let Some(ctx) = action_docks_ctx() else {
            trust_log("[Trust Core] ⚠️ trust_qt library or its dock functions not found for action docks");
            return;
        };
        let ctx = Box::new(ctx);

        let core_handle = open_libobs_core();
        let queue_task_ptr = if !core_handle.is_null() {
            unsafe { frontend_sym(core_handle, b"obs_queue_task\0") }
        } else {
            std::ptr::null_mut()
        };

        let param = Box::into_raw(ctx) as *mut c_void;
        if !queue_task_ptr.is_null() {
            let queue_task: ObsQueueTaskFn = unsafe { std::mem::transmute(queue_task_ptr) };
            unsafe {
                queue_task(OBS_TASK_UI, run_action_docks_setup_on_ui_thread, param, false);
            }
            trust_log("[Trust Core] 🎬 Queued action docks setup to OBS UI thread");
        } else {
            trust_log("[Trust Core] ⚠️ obs_queue_task not found; creating docks without UI-thread marshaling");
            unsafe {
                run_action_docks_setup_on_ui_thread(param);
            }
        }
    });
}

/// Direct CPU pixel scan for a target's individual source layer
pub fn scan_target_source_layer(
    name: &str,
    layer_rgba: &[u8],
    width: u32,
    height: u32,
    stride: u32,
    transform: &crate::types::SourceTransform,
    pitch: f32,
    yaw: f32,
    config: &crate::types::TrackingConfig,
    base_width: u32,
    base_height: u32,
) {
    let (hitbox, mask, pixel_grid) = crate::tracker::scan_rgba_with_3d_transform(
        layer_rgba,
        width,
        height,
        stride,
        transform,
        pitch,
        yaw,
        config,
    );

    let state = crate::state::get_state();
    state.hitboxes.write().insert(name.to_string(), hitbox);
    state.masks.write().insert(name.to_string(), mask);
    state.pixel_grids.write().insert(name.to_string(), pixel_grid);

    // The target's picture, for the Studio's mask overlay and anything that
    // rescans it later. Refreshed in place (no new allocation each frame), and
    // never waited for: while a reader holds it (the Studio builds its overlay
    // from it), this frame's picture is skipped — the render thread waiting
    // there held up all of OBS's rendering.
    let Some(mut tg_lock) = state.target_textures.try_write() else { return };
    let tex = tg_lock.entry(name.to_string()).or_insert_with(|| crate::state::TargetPreviewTexture {
        width,
        height,
        rgba: Vec::new(),
        frame_id: 0,
        base_width: width,
        base_height: height,
    });
    let row = width as usize * 4;
    let s = stride as usize;
    tex.rgba.clear();
    if s == row && layer_rgba.len() >= row * height as usize {
        tex.rgba.extend_from_slice(&layer_rgba[..row * height as usize]);
    } else {
        for y in 0..height as usize {
            let row_start = y * s;
            if row_start + row <= layer_rgba.len() {
                tex.rgba.extend_from_slice(&layer_rgba[row_start..row_start + row]);
            }
        }
    }
    tex.width = width;
    tex.height = height;
    tex.base_width = if base_width > 0 { base_width } else { width };
    tex.base_height = if base_height > 0 { base_height } else { height };
    tex.frame_id = tex.frame_id.wrapping_add(1);
}

/// Scans each target's hitbox/mask directly from the already-rendered full
/// scene composite (the same frame the Scene Studio preview uses), instead of
/// re-rendering each target source in isolation. The isolated-render approach
/// (scan_target_sources_on_gpu) turned out to return the wrong source's
/// pixels when two async sources (e.g. two browser sources) were scanned
/// back-to-back in the same frame; cropping each target's region out of one
/// single correct composite render sidesteps that entirely.

pub fn scan_one_target_from_layer(rgba: &[u8], cw: u32, ch: u32, src: &crate::StudioSource, config: &crate::types::TrackingConfig) {
    if rgba.len() < (cw * ch * 4) as usize || cw == 0 || ch == 0 {
        return;
    }
    if !src.is_target || !src.is_visible || src.width <= 0.0 || src.height <= 0.0 {
        return;
    }

    let w = (src.width.round() as u32).max(1);
    let h = (src.height.round() as u32).max(1);
    let cx = src.x + src.width * 0.5;
    let cy = src.y + src.height * 0.5;

    // Sample the target from the layer frame in its canonical unrotated local space (w x h).
    // This ensures the stored mask and preview texture are in local space, allowing 3D yaw/pitch/roll
    // transformations to rotate and flip them correctly at any angle (including yaw/pitch > 90°).
    let mut unrotated_rgba = vec![0u8; (w * h * 4) as usize];

    for y in 0..h {
        let ly = (y as f32 + 0.5) - h as f32 * 0.5;
        for x in 0..w {
            let lx = (x as f32 + 0.5) - w as f32 * 0.5;
            let (px, py) = crate::tracker::project_3d_canvas(lx, ly, cx, cy, src.roll, src.pitch, src.yaw);
            let sx = px.round() as i32;
            let sy = py.round() as i32;

            if sx >= 0 && sx < cw as i32 && sy >= 0 && sy < ch as i32 {
                let src_idx = (sy as usize * cw as usize + sx as usize) * 4;
                if src_idx + 3 < rgba.len() {
                    let r = rgba[src_idx];
                    let g = rgba[src_idx + 1];
                    let b = rgba[src_idx + 2];
                    let a = rgba[src_idx + 3];

                    let dst_idx = (y as usize * w as usize + x as usize) * 4;
                    unrotated_rgba[dst_idx] = r;
                    unrotated_rgba[dst_idx + 1] = g;
                    unrotated_rgba[dst_idx + 2] = b;
                    unrotated_rgba[dst_idx + 3] = a;
                }
            }
        }
    }

    let transform = crate::types::SourceTransform {
        pos_x: src.x,
        pos_y: src.y,
        scale_x: 1.0,
        scale_y: 1.0,
        crop_left: 0,
        crop_top: 0,
        crop_right: 0,
        crop_bottom: 0,
        rot_deg: src.roll,
    };

    scan_target_source_layer(
        &src.name,
        &unrotated_rgba,
        w,
        h,
        w * 4,
        &transform,
        src.pitch,
        src.yaw,
        config,
        w,
        h,
    );
}

pub fn scan_targets_from_composite(rgba: &[u8], cw: u32, ch: u32) {
    if rgba.len() < (cw * ch * 4) as usize || cw == 0 || ch == 0 {
        return;
    }
    let state = crate::state::get_state();
    let sources = state.studio_sources.read().clone();
    let config = *state.config.read();

    for src in sources {
        scan_one_target_from_layer(rgba, cw, ch, &src, &config);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn test_studio_preview_copy_is_the_canvas_scaled_down_to_fit() {
        assert_eq!(preview_size(2560, 1440), (1280, 720));
        assert_eq!(preview_size(1920, 1080), (1280, 720));
        // Never scaled up
        assert_eq!(preview_size(1280, 720), (1280, 720));
        assert_eq!(preview_size(640, 360), (640, 360));
        // Other shapes keep theirs, within both limits
        assert_eq!(preview_size(1080, 1920), (405, 720));
        assert_eq!(preview_size(3840, 1080), (1280, 360));
    }

    fn pose(id: &str, name: &str, scene: &str) -> crate::studio_app::SceneCoordinateSet {
        crate::studio_app::SceneCoordinateSet {
            id: id.into(),
            name: name.into(),
            trigger: String::new(),
            scene: scene.into(),
            easing: Default::default(),
            duration_ms: 800,
            sources: Vec::new(),
        }
    }

    #[test]
    fn test_scene_dock_hides_poses_saved_for_other_scenes() {
        // Three poses saved for Linux's "Scène", none for the current "Streaming" scene
        let sets = vec![pose("a", "Pose 1", "Scène"), pose("b", "Pose 2", "Scène"), pose("c", "Pose 3", "Scène")];
        assert!(scene_dock_poses(&sets, "Streaming").is_empty());
        assert_eq!(scene_dock_poses(&sets, "Scène").len(), 3);
    }

    #[test]
    fn test_scene_dock_shows_current_scene_and_sceneless_poses_only() {
        let sets = vec![pose("a", "Intro", "Streaming"), pose("b", "Other", "Scène"), pose("c", "Any", "")];
        assert_eq!(
            scene_dock_poses(&sets, "Streaming"),
            vec![("a".to_string(), "Intro".to_string()), ("c".to_string(), "Any".to_string())]
        );
    }

    #[test]
    fn test_same_source_placed_twice_gets_two_studio_names() {
        // One capture card placed twice (item ids 12 and 5) next to a lone source; order as OBS lists them
        let items = vec![(12, "Elgato".to_string()), (7, "Tv".to_string()), (5, "Elgato".to_string())];
        let keys = studio_item_keys(&items);
        assert_eq!(keys, vec![
            (12, "Elgato".to_string(), "Elgato #2".to_string()),
            (7, "Tv".to_string(), "Tv".to_string()),
            (5, "Elgato".to_string(), "Elgato".to_string()),
        ]);

        // Names follow item ids, not layer order: reordering in OBS keeps each copy's name
        let reordered = vec![(5, "Elgato".to_string()), (12, "Elgato".to_string()), (7, "Tv".to_string())];
        let by_id: std::collections::HashMap<i64, String> = studio_item_keys(&reordered).into_iter().map(|(id, _, k)| (id, k)).collect();
        assert_eq!(by_id[&12], "Elgato #2");
        assert_eq!(by_id[&5], "Elgato");

        let three = studio_item_keys(&[(3, "Cam".into()), (9, "Cam".into()), (4, "Cam".into())]);
        let names: Vec<&str> = three.iter().map(|(_, _, k)| k.as_str()).collect();
        assert_eq!(names, ["Cam", "Cam #3", "Cam #2"]);
    }

    #[test]
    fn test_extra_copy_maps_back_to_its_item_and_source() {
        store_scene_item_keys("DupTestScene", &[(12, "Elgato".into()), (5, "Elgato".into()), (7, "Tv".into())]);
        assert_eq!(studio_key_for_item("DupTestScene", 12, "Elgato"), "Elgato #2");
        assert_eq!(studio_key_for_item("DupTestScene", 5, "Elgato"), "Elgato");
        // Source-level calls (filters, volume) act on the shared source
        assert_eq!(studio_key_source_name("Elgato #2"), "Elgato");
        assert_eq!(studio_key_source_name("Tv"), "Tv");
        // Unknown items / names fall back to the plain source name
        assert_eq!(studio_key_for_item("DupTestScene", 99, "Mic"), "Mic");
        assert_eq!(studio_key_source_name("Not A Copy"), "Not A Copy");
    }

    #[test]
    fn test_scene_dock_empty_before_a_scene_is_loaded() {
        let sets = vec![pose("a", "Pose 1", "Scène"), pose("c", "Any", "")];
        assert!(scene_dock_poses(&sets, "").is_empty());
    }

    static RENDER_CALL_COUNT: AtomicUsize = AtomicUsize::new(0);
    static BLEND_PUSH_COUNT: AtomicUsize = AtomicUsize::new(0);
    static BLEND_POP_COUNT: AtomicUsize = AtomicUsize::new(0);
    static MATRIX_PUSH_COUNT: AtomicUsize = AtomicUsize::new(0);
    static MATRIX_POP_COUNT: AtomicUsize = AtomicUsize::new(0);

    unsafe extern "C" fn mock_render(_src: *mut c_void) {
        RENDER_CALL_COUNT.fetch_add(1, Ordering::SeqCst);
    }
    unsafe extern "C" fn mock_blend_push() {
        BLEND_PUSH_COUNT.fetch_add(1, Ordering::SeqCst);
    }
    unsafe extern "C" fn mock_blend_pop() {
        BLEND_POP_COUNT.fetch_add(1, Ordering::SeqCst);
    }
    unsafe extern "C" fn mock_matrix_push() {
        MATRIX_PUSH_COUNT.fetch_add(1, Ordering::SeqCst);
    }
    unsafe extern "C" fn mock_matrix_pop() {
        MATRIX_POP_COUNT.fetch_add(1, Ordering::SeqCst);
    }
    unsafe extern "C" fn mock_matrix_mul(_m: *const c_void) {}
    unsafe extern "C" fn mock_get_zero_dim(_src: *mut c_void) -> u32 { 0 }
    unsafe extern "C" fn mock_get_browser_name(_src: *mut c_void) -> *const c_char { b"Navigateur web 2\0".as_ptr() as *const c_char }
    unsafe extern "C" fn mock_get_source(item: *mut c_void) -> *mut c_void { item }
    unsafe extern "C" fn mock_get_draw_xf(_item: *mut c_void, xf: *mut GsMatrix4) {
        unsafe { *xf = GsMatrix4 { x: [1.0; 4], y: [1.0; 4], z: [1.0; 4], t: [1.0; 4] }; }
    }
    unsafe extern "C" fn mock_dummy_null(_name: *const c_char) -> *mut c_void { std::ptr::null_mut() }
    unsafe extern "C" fn mock_dummy_null_src(_p: *mut c_void) -> *mut c_void { std::ptr::null_mut() }
    unsafe extern "C" fn mock_dummy_null_recurse(_p: *mut c_void, _name: *const c_char) -> *mut c_void { std::ptr::null_mut() }
    unsafe extern "C" fn mock_dummy_pos(_p: *mut c_void, _pos: *const ObsVec2) {}
    unsafe extern "C" fn mock_dummy_rot(_p: *mut c_void, _rot: f32) {}
    unsafe extern "C" fn mock_dummy_scale(_p: *mut c_void, _scale: *const ObsVec2) {}
    unsafe extern "C" fn mock_dummy_void(_p: *mut c_void) {}
    unsafe extern "C" fn mock_dummy_clear(_f: u32, _c: *const f32, _d: f32, _s: u8) {}
    unsafe extern "C" fn mock_dummy_ortho(_l: f32, _r: f32, _t: f32, _b: f32, _zn: f32, _zf: f32) {}
    unsafe extern "C" fn mock_dummy_no_arg() {}
    unsafe extern "C" fn mock_dummy_bool(_b: bool) {}
    unsafe extern "C" fn mock_dummy_draw(_img: *mut c_void, _x: i32, _y: i32, _cx: u32, _cy: u32, _flip: bool) {}
    unsafe extern "C" fn mock_dummy_effect(_e: u32) -> *mut c_void { std::ptr::null_mut() }
    unsafe extern "C" fn mock_dummy_effect_loop(_e: *mut c_void, _n: *const c_char) -> bool { false }
    unsafe extern "C" fn mock_dummy_texrender_begin(_t: *mut c_void, _cx: u32, _cy: u32) -> bool { false }
    unsafe extern "C" fn mock_dummy_texrender_create(_f: u32, _z: u32) -> *mut c_void { std::ptr::null_mut() }
    unsafe extern "C" fn mock_dummy_texrender_get(_t: *const c_void) -> *mut c_void { std::ptr::null_mut() }
    unsafe extern "C" fn mock_dummy_stagesurf_create(_w: u32, _h: u32, _f: u32) -> *mut c_void { std::ptr::null_mut() }
    unsafe extern "C" fn mock_dummy_stage_tex(_dst: *mut c_void, _src: *mut c_void) {}
    unsafe extern "C" fn mock_dummy_stagesurf_map(_s: *mut c_void, _d: *mut *mut u8, _l: *mut u32) -> bool { false }

    #[test]
    fn test_browser_source_renders_even_with_zero_dim_and_isolates_graphics_state() {
        RENDER_CALL_COUNT.store(0, Ordering::SeqCst);
        BLEND_PUSH_COUNT.store(0, Ordering::SeqCst);
        BLEND_POP_COUNT.store(0, Ordering::SeqCst);
        MATRIX_PUSH_COUNT.store(0, Ordering::SeqCst);
        MATRIX_POP_COUNT.store(0, Ordering::SeqCst);

        let scene_fns = ObsSceneFns {
            obs_get_source_by_name: mock_dummy_null,
            obs_scene_from_source: mock_dummy_null_src,
            obs_scene_find_source_recursive: mock_dummy_null_recurse,
            obs_scene_find_source: mock_dummy_null_recurse,
            obs_sceneitem_get_source: mock_get_source,
            obs_sceneitem_set_pos: mock_dummy_pos,
            obs_sceneitem_set_rot: mock_dummy_rot,
            obs_sceneitem_set_scale: mock_dummy_scale,
            obs_source_get_width: mock_get_zero_dim,
            obs_source_get_height: mock_get_zero_dim,
            obs_source_get_name: mock_get_browser_name,
            obs_source_release: mock_dummy_void,
            obs_frontend_get_current_scene: None,
            obs_frontend_set_current_scene: None,
            obs_sceneitem_set_locked: None,
            obs_sceneitem_set_visible: None,
            obs_sceneitem_locked: None,
            obs_sceneitem_visible: None,
            obs_frontend_get_scenes: None,
            bfree: None,
            obs_scene_enum_items: None,
            obs_sceneitem_get_pos: None,
            obs_sceneitem_get_scale: None,
            obs_sceneitem_get_rot: None,
            obs_source_get_id: None,
            obs_sceneitem_get_draw_transform: Some(mock_get_draw_xf),
            obs_source_get_base_width: None,
            obs_source_get_base_height: None,
            obs_source_filter_add: None,
            obs_source_filter_remove: None,
            obs_source_filter_count: None,
            obs_source_get_filter_by_name: None,
            obs_source_enum_filters: None,
            obs_source_get_output_flags: None,
            obs_source_get_volume: None,
            obs_source_set_volume: None,
            obs_volmeter_create: None,
            obs_volmeter_destroy: None,
            obs_volmeter_attach_source: None,
            obs_volmeter_add_callback: None,
            obs_volmeter_remove_callback: None,
            obs_volmeter_get_nr_channels: None,
            obs_sceneitem_get_id: None,
            obs_scene_find_sceneitem_by_id: None,
            obs_source_get_settings: None,
            obs_source_update: None,
            obs_source_enabled: None,
            obs_source_set_enabled: None,
            obs_data_get_json: None,
            obs_data_create_from_json: None,
            obs_data_release: None,
        };

        let gfx_fns = ObsGraphicsFns {
            gs_texrender_create: mock_dummy_texrender_create,
            gs_texrender_destroy: mock_dummy_void,
            gs_texrender_begin: mock_dummy_texrender_begin,
            gs_texrender_end: mock_dummy_void,
            gs_texrender_reset: mock_dummy_void,
            gs_texrender_get_texture: mock_dummy_texrender_get,
            gs_stagesurface_create: mock_dummy_stagesurf_create,
            gs_stagesurface_destroy: mock_dummy_void,
            gs_stage_texture: mock_dummy_stage_tex,
            gs_stagesurface_map: mock_dummy_stagesurf_map,
            gs_stagesurface_unmap: mock_dummy_void,
            obs_source_draw: mock_dummy_draw,
            obs_get_base_effect: mock_dummy_effect,
            gs_effect_loop: mock_dummy_effect_loop,
            gs_blend_state_push: mock_blend_push,
            gs_blend_state_pop: mock_blend_pop,
            gs_reset_blend_state: None,
            gs_blend_function: None,
            gs_clear: mock_dummy_clear,
            obs_source_video_render: mock_render,
            gs_ortho: mock_dummy_ortho,
            gs_projection_push: mock_dummy_no_arg,
            gs_projection_pop: mock_dummy_no_arg,
            gs_matrix_push: mock_matrix_push,
            gs_matrix_pop: mock_matrix_pop,
            gs_matrix_identity: mock_dummy_no_arg,
            gs_matrix_mul: mock_matrix_mul,
            gs_matrix_translate3f: None,
            gs_matrix_scale3f: None,
            gs_matrix_rotaa4f: None,
            gs_enable_blending: mock_dummy_bool,
            obs_get_source_by_name: mock_dummy_null,
            obs_source_release: mock_dummy_void,
            obs_source_get_width: mock_get_zero_dim,
            obs_source_get_height: mock_get_zero_dim,
            obs_source_create_private: None,
            obs_data_create: None,
            obs_data_release: None,
            obs_data_set_int: None,
            gs_enable_framebuffer_srgb: None,
            gs_set_linear_srgb: None,
            gs_effect_get_param_by_name: None,
            gs_effect_set_color: None,
            gs_draw_sprite: None,
            gs_texture_create: None,
            gs_texture_destroy: None,
            gs_effect_create: None,
            gs_effect_destroy: None,
            gs_effect_set_float: None,
            gs_effect_get_num_params: None,
            gs_effect_get_param_by_idx: None,
            gs_effect_get_param_info: None,
            gs_effect_set_val: None,
            gs_get_render_target: None,
            gs_copy_texture: None,
            gs_effect_set_texture: None,
            obs_get_main_texture: None,
        };

        let ctx = RenderSceneItemContext {
            scene_fns: &scene_fns,
            gfx_fns: &gfx_fns,
            scene_name: "Scène".to_string(),
        };

        let mut dummy_item = 0x1234usize as *mut c_void;
        let res = unsafe {
            rust_render_scene_item_cb(
                std::ptr::null_mut(),
                dummy_item,
                &ctx as *const _ as *mut c_void,
            )
        };

        assert!(res, "Callback must return true to continue enumerating scene items");
        assert_eq!(RENDER_CALL_COUNT.load(Ordering::SeqCst), 1, "Browser source must be rendered via obs_source_video_render");
        assert_eq!(BLEND_PUSH_COUNT.load(Ordering::SeqCst), 1, "gs_blend_state_push must be called to isolate graphics state");
        assert_eq!(BLEND_POP_COUNT.load(Ordering::SeqCst), 1, "gs_blend_state_pop must be called to prevent state leakage");
        assert_eq!(MATRIX_PUSH_COUNT.load(Ordering::SeqCst), 1, "gs_matrix_push must be called for clean transform");
        assert_eq!(MATRIX_POP_COUNT.load(Ordering::SeqCst), 1, "gs_matrix_pop must be called for clean transform");
    }

    static TEST_GUARD_MUTEX: parking_lot::Mutex<()> = parking_lot::Mutex::new(());
    static FILTER_ADD_COUNT: AtomicUsize = AtomicUsize::new(0);
    unsafe extern "C" fn mock_filter_add(_source: *mut c_void, _filter: *mut c_void) {
        FILTER_ADD_COUNT.fetch_add(1, Ordering::SeqCst);
    }
    unsafe extern "C" fn mock_filter_count_zero(_source: *mut c_void) -> usize { 0 }
    unsafe extern "C" fn mock_filter_count_one(_source: *mut c_void) -> usize { 1 }
    unsafe extern "C" fn mock_create_priv(_id: *const c_char, _name: *const c_char, _s: *mut c_void) -> *mut c_void {
        0x5678usize as *mut c_void
    }

    #[test]
    fn test_ensure_source_filter_guard_attaches_private_filter_when_no_filter_present() {
        let _lock = TEST_GUARD_MUTEX.lock();
        FILTER_ADD_COUNT.store(0, Ordering::SeqCst);

        let scene_fns = ObsSceneFns {
            obs_get_source_by_name: mock_dummy_null,
            obs_scene_from_source: mock_dummy_null_src,
            obs_scene_find_source_recursive: mock_dummy_null_recurse,
            obs_scene_find_source: mock_dummy_null_recurse,
            obs_sceneitem_get_source: mock_get_source,
            obs_sceneitem_set_pos: mock_dummy_pos,
            obs_sceneitem_set_rot: mock_dummy_rot,
            obs_sceneitem_set_scale: mock_dummy_scale,
            obs_source_get_width: mock_get_zero_dim,
            obs_source_get_height: mock_get_zero_dim,
            obs_source_get_name: mock_get_browser_name,
            obs_source_release: mock_dummy_void,
            obs_frontend_get_current_scene: None,
            obs_frontend_set_current_scene: None,
            obs_sceneitem_set_locked: None,
            obs_sceneitem_set_visible: None,
            obs_sceneitem_locked: None,
            obs_sceneitem_visible: None,
            obs_frontend_get_scenes: None,
            bfree: None,
            obs_scene_enum_items: None,
            obs_sceneitem_get_pos: None,
            obs_sceneitem_get_scale: None,
            obs_sceneitem_get_rot: None,
            obs_source_get_id: None,
            obs_sceneitem_get_draw_transform: None,
            obs_source_get_base_width: None,
            obs_source_get_base_height: None,
            obs_source_filter_add: Some(mock_filter_add),
            obs_source_filter_remove: None,
            obs_source_filter_count: Some(mock_filter_count_zero),
            obs_source_get_filter_by_name: None,
            obs_source_enum_filters: None,
            obs_source_get_output_flags: None,
            obs_source_get_volume: None,
            obs_source_set_volume: None,
            obs_volmeter_create: None,
            obs_volmeter_destroy: None,
            obs_volmeter_attach_source: None,
            obs_volmeter_add_callback: None,
            obs_volmeter_remove_callback: None,
            obs_volmeter_get_nr_channels: None,
            obs_sceneitem_get_id: None,
            obs_scene_find_sceneitem_by_id: None,
            obs_source_get_settings: None,
            obs_source_update: None,
            obs_source_enabled: None,
            obs_source_set_enabled: None,
            obs_data_get_json: None,
            obs_data_create_from_json: None,
            obs_data_release: None,
        };

        let gfx_fns = ObsGraphicsFns {
            gs_texrender_create: mock_dummy_texrender_create,
            gs_texrender_destroy: mock_dummy_void,
            gs_texrender_begin: mock_dummy_texrender_begin,
            gs_texrender_end: mock_dummy_void,
            gs_texrender_reset: mock_dummy_void,
            gs_texrender_get_texture: mock_dummy_texrender_get,
            gs_stagesurface_create: mock_dummy_stagesurf_create,
            gs_stagesurface_destroy: mock_dummy_void,
            gs_stage_texture: mock_dummy_stage_tex,
            gs_stagesurface_map: mock_dummy_stagesurf_map,
            gs_stagesurface_unmap: mock_dummy_void,
            obs_source_draw: mock_dummy_draw,
            obs_get_base_effect: mock_dummy_effect,
            gs_effect_loop: mock_dummy_effect_loop,
            gs_blend_state_push: mock_blend_push,
            gs_blend_state_pop: mock_blend_pop,
            gs_reset_blend_state: None,
            gs_blend_function: None,
            gs_clear: mock_dummy_clear,
            obs_source_video_render: mock_render,
            gs_ortho: mock_dummy_ortho,
            gs_projection_push: mock_dummy_no_arg,
            gs_projection_pop: mock_dummy_no_arg,
            gs_matrix_push: mock_matrix_push,
            gs_matrix_pop: mock_matrix_pop,
            gs_matrix_identity: mock_dummy_no_arg,
            gs_matrix_mul: mock_matrix_mul,
            gs_matrix_translate3f: None,
            gs_matrix_scale3f: None,
            gs_matrix_rotaa4f: None,
            gs_enable_blending: mock_dummy_bool,
            obs_get_source_by_name: mock_dummy_null,
            obs_source_release: mock_dummy_void,
            obs_source_get_width: mock_get_zero_dim,
            obs_source_get_height: mock_get_zero_dim,
            obs_source_create_private: Some(mock_create_priv),
            obs_data_create: None,
            obs_data_release: None,
            obs_data_set_int: None,
            gs_enable_framebuffer_srgb: None,
            gs_set_linear_srgb: None,
            gs_effect_get_param_by_name: None,
            gs_effect_set_color: None,
            gs_draw_sprite: None,
            gs_texture_create: None,
            gs_texture_destroy: None,
            gs_effect_create: None,
            gs_effect_destroy: None,
            gs_effect_set_float: None,
            gs_effect_get_num_params: None,
            gs_effect_get_param_by_idx: None,
            gs_effect_get_param_info: None,
            gs_effect_set_val: None,
            gs_get_render_target: None,
            gs_copy_texture: None,
            gs_effect_set_texture: None,
            obs_get_main_texture: None,
        };

        let dummy_src = 0x9999usize as *mut c_void;
        unsafe {
            ensure_source_filter_guard(&scene_fns, &gfx_fns, dummy_src);
        }
        assert_eq!(FILTER_ADD_COUNT.load(Ordering::SeqCst), 1, "Private filter must be attached when source has 0 filters");

        // Calling again on the same source must be a no-op (cached in registry):
        unsafe {
            ensure_source_filter_guard(&scene_fns, &gfx_fns, dummy_src);
        }
        assert_eq!(FILTER_ADD_COUNT.load(Ordering::SeqCst), 1, "Must be idempotent and not re-attach");

        // Source that already has a filter must not have guard attached:
        let mut scene_fns_with_filter = scene_fns;
        scene_fns_with_filter.obs_source_filter_count = Some(mock_filter_count_one);
        let dummy_src2 = 0x8888usize as *mut c_void;
        ensure_source_filter_guard(&scene_fns_with_filter, &gfx_fns, dummy_src2);
        assert_eq!(FILTER_ADD_COUNT.load(Ordering::SeqCst), 1, "Must not attach if source already has a filter");
    }

    #[test]
    fn test_game_capture_attaches_crop_filter_guard_and_detection() {
        let _lock = TEST_GUARD_MUTEX.lock();
        unsafe extern "C" fn mock_get_vk_id(_s: *mut c_void) -> *const c_char {
            b"vkcapture-source\0".as_ptr() as *const c_char
        }
        unsafe extern "C" fn mock_get_vk_name(_s: *mut c_void) -> *const c_char {
            b"Capture de jeu\0".as_ptr() as *const c_char
        }

        let scene_fns = ObsSceneFns {
            obs_get_source_by_name: mock_dummy_null,
            obs_scene_from_source: mock_dummy_null_src,
            obs_scene_find_source_recursive: mock_dummy_null_recurse,
            obs_scene_find_source: mock_dummy_null_recurse,
            obs_sceneitem_get_source: mock_get_source,
            obs_sceneitem_set_pos: mock_dummy_pos,
            obs_sceneitem_set_rot: mock_dummy_rot,
            obs_sceneitem_set_scale: mock_dummy_scale,
            obs_source_get_width: mock_get_zero_dim,
            obs_source_get_height: mock_get_zero_dim,
            obs_source_get_name: mock_get_vk_name,
            obs_source_release: mock_dummy_void,
            obs_frontend_get_current_scene: None,
            obs_frontend_set_current_scene: None,
            obs_sceneitem_set_locked: None,
            obs_sceneitem_set_visible: None,
            obs_sceneitem_locked: None,
            obs_sceneitem_visible: None,
            obs_frontend_get_scenes: None,
            bfree: None,
            obs_scene_enum_items: None,
            obs_sceneitem_get_pos: None,
            obs_sceneitem_get_scale: None,
            obs_sceneitem_get_rot: None,
            obs_source_get_id: Some(mock_get_vk_id),
            obs_sceneitem_get_draw_transform: None,
            obs_source_get_base_width: None,
            obs_source_get_base_height: None,
            obs_source_filter_add: Some(mock_filter_add),
            obs_source_filter_remove: None,
            obs_source_filter_count: Some(mock_filter_count_zero),
            obs_source_get_filter_by_name: None,
            obs_source_enum_filters: None,
            obs_source_get_output_flags: None,
            obs_source_get_volume: None,
            obs_source_set_volume: None,
            obs_volmeter_create: None,
            obs_volmeter_destroy: None,
            obs_volmeter_attach_source: None,
            obs_volmeter_add_callback: None,
            obs_volmeter_remove_callback: None,
            obs_volmeter_get_nr_channels: None,
            obs_sceneitem_get_id: None,
            obs_scene_find_sceneitem_by_id: None,
            obs_source_get_settings: None,
            obs_source_update: None,
            obs_source_enabled: None,
            obs_source_set_enabled: None,
            obs_data_get_json: None,
            obs_data_create_from_json: None,
            obs_data_release: None,
        };

        let dummy_src = 0x7777usize as *mut c_void;
        assert!(is_game_capture_source(&scene_fns, dummy_src), "Must detect vkcapture-source");

        let prev_filter_adds = FILTER_ADD_COUNT.load(Ordering::SeqCst);
        let gfx_fns = ObsGraphicsFns {
            gs_texrender_create: mock_dummy_texrender_create,
            gs_texrender_destroy: mock_dummy_void,
            gs_texrender_begin: mock_dummy_texrender_begin,
            gs_texrender_end: mock_dummy_void,
            gs_texrender_reset: mock_dummy_void,
            gs_texrender_get_texture: mock_dummy_texrender_get,
            gs_stagesurface_create: mock_dummy_stagesurf_create,
            gs_stagesurface_destroy: mock_dummy_void,
            gs_stage_texture: mock_dummy_stage_tex,
            gs_stagesurface_map: mock_dummy_stagesurf_map,
            gs_stagesurface_unmap: mock_dummy_void,
            obs_source_draw: mock_dummy_draw,
            obs_get_base_effect: mock_dummy_effect,
            gs_effect_loop: mock_dummy_effect_loop,
            gs_blend_state_push: mock_blend_push,
            gs_blend_state_pop: mock_blend_pop,
            gs_reset_blend_state: None,
            gs_blend_function: None,
            gs_clear: mock_dummy_clear,
            obs_source_video_render: mock_render,
            gs_ortho: mock_dummy_ortho,
            gs_projection_push: mock_dummy_no_arg,
            gs_projection_pop: mock_dummy_no_arg,
            gs_matrix_push: mock_matrix_push,
            gs_matrix_pop: mock_matrix_pop,
            gs_matrix_identity: mock_dummy_no_arg,
            gs_matrix_mul: mock_matrix_mul,
            gs_matrix_translate3f: None,
            gs_matrix_scale3f: None,
            gs_matrix_rotaa4f: None,
            gs_enable_blending: mock_dummy_bool,
            obs_get_source_by_name: mock_dummy_null,
            obs_source_release: mock_dummy_void,
            obs_source_get_width: mock_get_zero_dim,
            obs_source_get_height: mock_get_zero_dim,
            obs_source_create_private: Some(mock_create_priv),
            obs_data_create: None,
            obs_data_release: None,
            obs_data_set_int: None,
            gs_enable_framebuffer_srgb: None,
            gs_set_linear_srgb: None,
            gs_effect_get_param_by_name: None,
            gs_effect_set_color: None,
            gs_draw_sprite: None,
            gs_texture_create: None,
            gs_texture_destroy: None,
            gs_effect_create: None,
            gs_effect_destroy: None,
            gs_effect_set_float: None,
            gs_effect_get_num_params: None,
            gs_effect_get_param_by_idx: None,
            gs_effect_get_param_info: None,
            gs_effect_set_val: None,
            gs_get_render_target: None,
            gs_copy_texture: None,
            gs_effect_set_texture: None,
            obs_get_main_texture: None,
        };

        ensure_source_filter_guard(&scene_fns, &gfx_fns, dummy_src);
        assert_eq!(
            FILTER_ADD_COUNT.load(Ordering::SeqCst),
            prev_filter_adds + 1,
            "Game capture source must have private crop_filter attached to force rendering"
        );
    }

    #[test]
    fn test_all_sources_receive_crop_filter_and_allow_3d_transform_regardless_of_target_status() {
        let _lock = TEST_GUARD_MUTEX.lock();
        
        static CREATED_FILTER_ID: parking_lot::Mutex<String> = parking_lot::Mutex::new(String::new());
        unsafe extern "C" fn mock_create_crop_priv(id: *const c_char, _name: *const c_char, _s: *mut c_void) -> *mut c_void {
            if !id.is_null() {
                let id_str = unsafe { std::ffi::CStr::from_ptr(id) }.to_string_lossy().to_string();
                *CREATED_FILTER_ID.lock() = id_str;
            }
            0x5678usize as *mut c_void
        }

        let scene_fns = ObsSceneFns {
            obs_get_source_by_name: mock_dummy_null,
            obs_scene_from_source: mock_dummy_null_src,
            obs_scene_find_source_recursive: mock_dummy_null_recurse,
            obs_scene_find_source: mock_dummy_null_recurse,
            obs_sceneitem_get_source: mock_get_source,
            obs_sceneitem_set_pos: mock_dummy_pos,
            obs_sceneitem_set_rot: mock_dummy_rot,
            obs_sceneitem_set_scale: mock_dummy_scale,
            obs_source_get_width: mock_get_zero_dim,
            obs_source_get_height: mock_get_zero_dim,
            obs_source_get_name: mock_get_browser_name,
            obs_source_release: mock_dummy_void,
            obs_frontend_get_current_scene: None,
            obs_frontend_set_current_scene: None,
            obs_sceneitem_set_locked: None,
            obs_sceneitem_set_visible: None,
            obs_sceneitem_locked: None,
            obs_sceneitem_visible: None,
            obs_frontend_get_scenes: None,
            bfree: None,
            obs_scene_enum_items: None,
            obs_sceneitem_get_pos: None,
            obs_sceneitem_get_scale: None,
            obs_sceneitem_get_rot: None,
            obs_source_get_id: None,
            obs_sceneitem_get_draw_transform: None,
            obs_source_get_base_width: None,
            obs_source_get_base_height: None,
            obs_source_filter_add: Some(mock_filter_add),
            obs_source_filter_remove: None,
            obs_source_filter_count: Some(mock_filter_count_zero),
            obs_source_get_filter_by_name: None,
            obs_source_enum_filters: None,
            obs_source_get_output_flags: None,
            obs_source_get_volume: None,
            obs_source_set_volume: None,
            obs_volmeter_create: None,
            obs_volmeter_destroy: None,
            obs_volmeter_attach_source: None,
            obs_volmeter_add_callback: None,
            obs_volmeter_remove_callback: None,
            obs_volmeter_get_nr_channels: None,
            obs_sceneitem_get_id: None,
            obs_scene_find_sceneitem_by_id: None,
            obs_source_get_settings: None,
            obs_source_update: None,
            obs_source_enabled: None,
            obs_source_set_enabled: None,
            obs_data_get_json: None,
            obs_data_create_from_json: None,
            obs_data_release: None,
        };

        let gfx_fns = ObsGraphicsFns {
            gs_texrender_create: mock_dummy_texrender_create,
            gs_texrender_destroy: mock_dummy_void,
            gs_texrender_begin: mock_dummy_texrender_begin,
            gs_texrender_end: mock_dummy_void,
            gs_texrender_reset: mock_dummy_void,
            gs_texrender_get_texture: mock_dummy_texrender_get,
            gs_stagesurface_create: mock_dummy_stagesurf_create,
            gs_stagesurface_destroy: mock_dummy_void,
            gs_stage_texture: mock_dummy_stage_tex,
            gs_stagesurface_map: mock_dummy_stagesurf_map,
            gs_stagesurface_unmap: mock_dummy_void,
            obs_source_draw: mock_dummy_draw,
            obs_get_base_effect: mock_dummy_effect,
            gs_effect_loop: mock_dummy_effect_loop,
            gs_blend_state_push: mock_blend_push,
            gs_blend_state_pop: mock_blend_pop,
            gs_reset_blend_state: None,
            gs_blend_function: None,
            gs_clear: mock_dummy_clear,
            obs_source_video_render: mock_render,
            gs_ortho: mock_dummy_ortho,
            gs_projection_push: mock_dummy_no_arg,
            gs_projection_pop: mock_dummy_no_arg,
            gs_matrix_push: mock_matrix_push,
            gs_matrix_pop: mock_matrix_pop,
            gs_matrix_identity: mock_dummy_no_arg,
            gs_matrix_mul: mock_matrix_mul,
            gs_matrix_translate3f: None,
            gs_matrix_scale3f: None,
            gs_matrix_rotaa4f: None,
            gs_enable_blending: mock_dummy_bool,
            obs_get_source_by_name: mock_dummy_null,
            obs_source_release: mock_dummy_void,
            obs_source_get_width: mock_get_zero_dim,
            obs_source_get_height: mock_get_zero_dim,
            obs_source_create_private: Some(mock_create_crop_priv),
            obs_data_create: None,
            obs_data_release: None,
            obs_data_set_int: None,
            gs_enable_framebuffer_srgb: None,
            gs_set_linear_srgb: None,
            gs_effect_get_param_by_name: None,
            gs_effect_set_color: None,
            gs_draw_sprite: None,
            gs_texture_create: None,
            gs_texture_destroy: None,
            gs_effect_create: None,
            gs_effect_destroy: None,
            gs_effect_set_float: None,
            gs_effect_get_num_params: None,
            gs_effect_get_param_by_idx: None,
            gs_effect_get_param_info: None,
            gs_effect_set_val: None,
            gs_get_render_target: None,
            gs_copy_texture: None,
            gs_effect_set_texture: None,
            obs_get_main_texture: None,
        };

        let dummy_src = 0x6666usize as *mut c_void;
        ensure_source_filter_guard(&scene_fns, &gfx_fns, dummy_src);
        assert_eq!(*CREATED_FILTER_ID.lock(), "crop_filter", "ALL source filters must be crop_filter");
    }

    #[test]
    fn test_target_scanning_continues_during_active_shader_reaction() {
        let state = crate::state::get_state();
        let target_name = "ShaderReactingAvatar";
        
        let src = crate::StudioSource {
            scene: "Scène".to_string(),
            name: target_name.to_string(),
            slot: 1,
            x: 100.0,
            y: 150.0,
            width: 200.0,
            height: 300.0,
            roll: 0.0,
            pitch: 0.0,
            yaw: 0.0,
            is_target: true,
            match_obs: false,
            is_visible: true,
            file_path: String::new(),
            default_x: 100.0,
            default_y: 150.0,
            default_width: 200.0,
            default_height: 300.0,
            default_roll: 0.0,
        };
        state.studio_sources.write().push(src.clone());

        // Trigger shader reaction (Bounce)
        state.trigger_avatar_reaction(target_name, crate::physics::ImpactReactionKind::Bounce, 2000, 0.4, Vec::new(), None);
        assert!(state.get_avatar_reaction_progress(target_name).is_some(), "Avatar reaction must be active");

        // Provide 100x100 solid pixel buffer
        let w = 100u32;
        let h = 100u32;
        let mut rgba = vec![0u8; (w * h * 4) as usize];
        for y in 20..80 {
            for x in 20..80 {
                let idx = ((y * w + x) * 4) as usize;
                rgba[idx] = 255;
                rgba[idx + 1] = 100;
                rgba[idx + 2] = 100;
                rgba[idx + 3] = 255;
            }
        }

        let transform = crate::types::SourceTransform {
            pos_x: src.x,
            pos_y: src.y,
            scale_x: src.width / w as f32,
            scale_y: src.height / h as f32,
            crop_left: 0,
            crop_top: 0,
            crop_right: 0,
            crop_bottom: 0,
            rot_deg: src.roll,
        };
        let config = crate::types::TrackingConfig::default();

        // Scan the target layer
        scan_target_source_layer(target_name, &rgba, w, h, w * 4, &transform, src.pitch, src.yaw, &config, w, h);

        // Verify that hitboxes, masks, and target textures are fully active and not frozen
        let hitboxes = state.hitboxes.read();
        let hb = hitboxes.get(target_name).expect("Hitbox must be present for reacting avatar");
        assert!(hb.active, "Hitbox must be active during reaction");
        assert!(hb.mass > 0, "Hitbox mass must be greater than 0");

        let target_textures = state.target_textures.read();
        let tex = target_textures.get(target_name).expect("Target texture must be updated");
        assert_eq!(tex.width, w);
        assert_eq!(tex.height, h);
        assert!(!tex.rgba.is_empty());
    }

    #[test]
    fn test_target_pictures_count_new_ones_and_repeats() {
        *RENDER_TIMES.lock() = Some(RenderTimes::new());
        let (a, b) = (vec![1u8; 400], vec![2u8; 400]);
        for picture in [&a, &a, &a, &b, &b, &a] {
            note_target_picture("PictureCountTarget", picture);
        }
        let t = RENDER_TIMES.lock().take().unwrap();
        assert_eq!((t.pictures, t.new_pictures, t.longest_repeat), (6, 3, 3));
    }

    #[test]
    fn test_shrunk_scan_finds_the_same_shape_on_the_canvas() {
        // An avatar-sized picture: an opaque ellipse on transparency
        let (bw, bh) = (2141u32, 1357u32);
        let picture = |w: u32, h: u32| -> Vec<u8> {
            let mut rgba = vec![0u8; (w * h * 4) as usize];
            for y in 0..h {
                for x in 0..w {
                    let (u, v) = ((x as f32 + 0.5) / w as f32 - 0.55, (y as f32 + 0.5) / h as f32 - 0.5);
                    if (u / 0.3).powi(2) + (v / 0.4).powi(2) < 1.0 {
                        rgba[((y * w + x) * 4 + 3) as usize] = 255;
                    }
                }
            }
            rgba
        };
        let (sw, sh) = scan_size_within(bw, bh, DEFAULT_SCAN_MAX_PIXELS);
        assert!(sw as u64 * sh as u64 <= DEFAULT_SCAN_MAX_PIXELS && (sw as f32 / sh as f32 - bw as f32 / bh as f32).abs() < 0.01);
        assert_eq!(scan_size_within(320, 240, DEFAULT_SCAN_MAX_PIXELS), (320, 240), "small pictures are scanned as they are");
        // A target filling the canvas: exactly 16:9, a mask pixel per ~4.6 canvas pixels
        assert_eq!(scan_size_within(1920, 1080, DEFAULT_SCAN_MAX_PIXELS), (416, 234));
        assert_eq!(scan_size_within(2560, 1440, DEFAULT_SCAN_MAX_PIXELS), (416, 234));
        assert_eq!(scan_size_within(1080, 1920, DEFAULT_SCAN_MAX_PIXELS), (234, 416), "portrait too");
        assert_eq!(scan_size_within(4096, 2048, DEFAULT_SCAN_MAX_PIXELS), (446, 223), "2:1");
        // No small ratio (2141:1357): as close as rounding allows, within the budget
        let (w, h) = scan_size_within(2141, 1357, DEFAULT_SCAN_MAX_PIXELS);
        assert!(w as u64 * h as u64 <= DEFAULT_SCAN_MAX_PIXELS && w as u64 * h as u64 >= DEFAULT_SCAN_MAX_PIXELS * 98 / 100);
        // Every mask resolution the Studio offers stays within its budget, keeps the ratio
        for (name, budget) in MASK_RESOLUTIONS {
            let (w, h) = scan_size_within(1920, 1080, budget);
            assert!(w as u64 * h as u64 <= budget && w * 9 == h * 16, "{}: {}×{}", name, w, h);
        }
        assert_eq!(scan_size_within(1920, 1080, u64::MAX), (1920, 1080), "full size: as it is");

        let transform = crate::types::SourceTransform {
            pos_x: 100.0, pos_y: 50.0, scale_x: 0.45, scale_y: 0.45,
            crop_left: 0, crop_top: 0, crop_right: 0, crop_bottom: 0, rot_deg: 15.0,
        };
        let config = crate::types::TrackingConfig::default();
        let full = crate::tracker::scan_rgba_with_3d_transform(&picture(bw, bh), bw, bh, bw * 4, &transform, 10.0, -20.0, &config);
        let (shrunk_t, base) = shrunk_scan_transform(&transform, (bw, bh), (bw, bh), (sw, sh));
        assert_eq!(base, (sw, sh));
        let small = crate::tracker::scan_rgba_with_3d_transform(&picture(sw, sh), sw, sh, sw * 4, &shrunk_t, 10.0, -20.0, &config);

        let (a, b) = (&full.0, &small.0);
        for (what, x, y) in [
            ("x", a.world_x, b.world_x), ("y", a.world_y, b.world_y),
            ("width", a.world_width, b.world_width), ("height", a.world_height, b.world_height),
            ("center x", a.world_center_x, b.world_center_x), ("center y", a.world_center_y, b.world_center_y),
        ] {
            assert!((x - y).abs() < 3.0, "{}: full {} vs shrunk {}", what, x, y);
        }
        // The masks agree on the canvas away from the edge
        let (cx, cy) = (a.world_center_x, a.world_center_y);
        assert!(full.1.is_solid_world(cx, cy) && small.1.is_solid_world(cx, cy));
        assert!(!full.1.is_solid_world(cx + 600.0, cy) && !small.1.is_solid_world(cx + 600.0, cy));
    }

    #[test]
    fn detection_settings_are_saved_and_read_back() {
        let c = crate::types::TrackingConfig { mode: 1, alpha_threshold: 30, chroma_r: 10, chroma_g: 200, chroma_b: 40, threshold: 55, precision: 8 };
        assert_eq!(detection_from_json(&detection_to_json(&c)), c);
        // What's missing or out of range keeps its default
        let partial = detection_from_json(&serde_json::json!({ "mode": 7, "alpha_threshold": 300, "threshold": 90 }));
        let d = crate::types::TrackingConfig::default();
        assert_eq!((partial.mode, partial.alpha_threshold, partial.threshold), (d.mode, d.alpha_threshold, 90));
    }

    #[test]
    fn staging_reads_back_pictures_three_frames_old_at_any_scan_rate() {
        for every in [1u64, 2, 3, 5] {
            let mut staged_at = [None; STAGING_DEPTH];
            // The frame whose picture each surface holds
            let mut holds = [None::<u64>; STAGING_DEPTH];
            let mut lags = Vec::new();
            for frame in (0..90u64).filter(|f| f % every == 0) {
                let (stage, read) = staging_turn(&mut staged_at, frame);
                if let Some(r) = read {
                    assert_ne!(stage, r, "every {}: a surface is never staged into while it's read back", every);
                    lags.push(frame - holds[r].expect("a picture in it"));
                }
                holds[stage] = Some(frame);
            }
            // Never younger than STAGING_DEPTH - 1 frames (the GPU may not be done), and
            // no older than the first scan frame after that
            let expected = (STAGING_DEPTH as u64 - 1).div_ceil(every) * every;
            assert!(!lags.is_empty() && lags.iter().all(|&l| l == expected), "every {}: {:?}", every, lags);
        }
    }

    #[test]
    fn test_bite_vanish_gives_theater_its_shader_and_settings() {
        let path = crate::studio_app::get_filters_dir().join("bite_vanish.lua");
        let Ok(source) = std::fs::read_to_string(&path) else { return };
        let shader = extract_shader_code_from_lua(&source).expect("a shader");
        assert!(shader.contains("uniform float hit_count;") && shader.contains("technique Draw"), "the whole shader");
        assert_eq!(hits_array_len(&shader), Some(1024));
        let names: Vec<String> = extract_default_floats_from_lua(&source).into_iter().map(|(n, _)| n).collect();
        assert_eq!(names, ["bite_size", "teeth_count", "teeth_depth", "chomp_speed", "heal_start", "edge_darkening"]);
        mlua::Lua::new().load(&source).set_name("bite_vanish.lua").into_function().expect("the Lua compiles");
    }

    #[test]
    fn test_reaction_hits_add_up_and_heal() {
        let state = crate::state::get_state();
        let kind = crate::physics::ImpactReactionKind::Custom(0xB17E);
        let target = "BiteHitsTarget";
        state.trigger_avatar_reaction(target, kind, 5000, 0.0, Vec::new(), Some((0.2, 0.3)));
        state.trigger_avatar_reaction(target, kind, 5000, 0.0, Vec::new(), None);
        state.trigger_avatar_reaction(target, kind, 5000, 0.0, Vec::new(), Some((0.8, 0.6)));
        let hits = state.get_avatar_reaction_progress(target).unwrap().hits;
        assert_eq!(hits.iter().map(|&(x, y, _)| (x, y)).collect::<Vec<_>>(), [(0.2, 0.3), (0.8, 0.6)]);
        // Healed hits (past the duration) are dropped; another reaction starts over
        state.trigger_avatar_reaction(target, kind, 0, 0.0, Vec::new(), Some((0.5, 0.5)));
        assert_eq!(state.avatar_reactions.read()[target].hits.len(), 1);
        state.trigger_avatar_reaction(target, crate::physics::ImpactReactionKind::Bounce, 5000, 0.4, Vec::new(), Some((0.1, 0.1)));
        assert_eq!(state.avatar_reactions.read()[target].hits.len(), 1);
        let max = crate::state::MAX_REACTION_HITS;
        for i in 0..max + 10 {
            state.trigger_avatar_reaction(target, kind, 60000, 0.0, Vec::new(), Some((i as f32, 0.5)));
        }
        let hits = state.get_avatar_reaction_progress(target).unwrap().hits;
        assert_eq!(hits.len(), max);
        assert_eq!((hits[0].0, hits[max - 1].0), (10.0, (max + 9) as f32), "the newest are kept");
        state.avatar_reactions.write().remove(target);
    }

    #[test]
    fn test_hits_array_len_reads_the_shaders_own_size() {
        assert_eq!(hits_array_len("uniform float hit_count;\nuniform float4 hits[1024];    // x, y"), Some(1024));
        assert_eq!(hits_array_len("uniform float4 hits[ 64 ];"), Some(64));
        assert_eq!(hits_array_len("uniform float4 hits[99999];"), None, "more than a shader can hold");
        assert_eq!(hits_array_len("uniform float4 hits[N];"), None);
        assert_eq!(hits_array_len("uniform float hit_0_x;"), None);
    }

    #[test]
    fn test_target_mask_sees_filter_transparency_and_shader_reaction() {
        let state = crate::state::get_state();
        let target_name = "FilteredAvatar";

        let src = crate::StudioSource {
            scene: "Scène".to_string(),
            name: target_name.to_string(),
            slot: 1,
            x: 200.0,
            y: 300.0,
            width: 100.0,
            height: 100.0,
            roll: 0.0,
            pitch: 0.0,
            yaw: 0.0,
            is_target: true,
            match_obs: true,
            is_visible: true,
            file_path: String::new(),
            default_x: 200.0,
            default_y: 300.0,
            default_width: 100.0,
            default_height: 100.0,
            default_roll: 0.0,
        };
        state.studio_sources.write().push(src.clone());

        // Create a 100x100 buffer where only a circular region in the middle is solid (e.g., Chroma Key / Swirl filter result)
        // Outer pixels are alpha = 0 (filtered out), inner pixels are alpha = 255.
        let w = 100u32;
        let h = 100u32;
        let mut filtered_rgba = vec![0u8; (w * h * 4) as usize];
        let center_x = 50.0f32;
        let center_y = 50.0f32;
        let radius = 25.0f32;

        for y in 0..h {
            for x in 0..w {
                let dx = x as f32 - center_x;
                let dy = y as f32 - center_y;
                let dist = (dx * dx + dy * dy).sqrt();
                let idx = ((y * w + x) * 4) as usize;
                if dist <= radius {
                    filtered_rgba[idx] = 200;
                    filtered_rgba[idx + 1] = 220;
                    filtered_rgba[idx + 2] = 255;
                    filtered_rgba[idx + 3] = 255; // Solid avatar
                } else {
                    filtered_rgba[idx] = 0;
                    filtered_rgba[idx + 1] = 0;
                    filtered_rgba[idx + 2] = 0;
                    filtered_rgba[idx + 3] = 0; // Filtered / keyed out transparency
                }
            }
        }

        let transform = crate::types::SourceTransform {
            pos_x: src.x,
            pos_y: src.y,
            scale_x: 1.0,
            scale_y: 1.0,
            crop_left: 0,
            crop_top: 0,
            crop_right: 0,
            crop_bottom: 0,
            rot_deg: 0.0,
        };
        let config = crate::types::TrackingConfig::default();

        scan_target_source_layer(target_name, &filtered_rgba, w, h, w * 4, &transform, 0.0, 0.0, &config, w, h);

        let masks = state.masks.read();
        let mask = masks.get(target_name).expect("Mask must be registered for target");

        // Center of circle at world coords (200 + 50 = 250, 300 + 50 = 350) must be solid!
        assert!(mask.is_solid_world(250.0, 350.0), "Center of circle should be solid");

        // Points inside the 25px radius must be solid
        assert!(mask.is_solid_world(260.0, 350.0), "Inside radius should be solid");
        assert!(mask.is_solid_world(250.0, 360.0), "Inside radius should be solid");

        // Points outside the 25px radius (e.g. corner at 210, 310) were keyed/filtered out and MUST NOT be solid!
        assert!(!mask.is_solid_world(210.0, 310.0), "Corner outside circle must NOT be solid (filter respected)");
        assert!(!mask.is_solid_world(290.0, 390.0), "Opposite corner outside circle must NOT be solid");
        assert!(!mask.is_solid_world(210.0, 390.0), "Corner outside circle must NOT be solid");
    }

    #[test]
    fn test_padded_reaction_scale_and_hitbox_proportions() {
        let state = crate::state::get_state();
        let target_name = "PaddedAvatar";

        let bw = 100u32;
        let bh = 100u32;
        let pad_ratio = 2.5f32;
        let cap_w = (bw as f32 * pad_ratio) as u32; // 250
        let cap_h = (bh as f32 * pad_ratio) as u32; // 250

        let src = crate::StudioSource {
            scene: "Scène".to_string(),
            name: target_name.to_string(),
            slot: 1,
            x: 200.0,
            y: 300.0,
            width: 100.0,
            height: 100.0,
            roll: 0.0,
            pitch: 0.0,
            yaw: 0.0,
            is_target: true,
            match_obs: true,
            is_visible: true,
            file_path: String::new(),
            default_x: 200.0,
            default_y: 300.0,
            default_width: 100.0,
            default_height: 100.0,
            default_roll: 0.0,
        };
        state.studio_sources.write().push(src.clone());

        // In the 250x250 padded buffer, the avatar is in the center [100..150, 100..150] (size 50x50)
        let mut padded_rgba = vec![0u8; (cap_w * cap_h * 4) as usize];
        for y in 100..150 {
            for x in 100..150 {
                let idx = ((y * cap_w + x) * 4) as usize;
                padded_rgba[idx] = 255;
                padded_rgba[idx + 1] = 100;
                padded_rgba[idx + 2] = 50;
                padded_rgba[idx + 3] = 255;
            }
        }

        let eff_w = cap_w as f32 * 1.0;
        let eff_h = cap_h as f32 * 1.0;
        let cx = src.x + src.width * 0.5;
        let cy = src.y + src.height * 0.5;
        let transform = crate::types::SourceTransform {
            pos_x: cx - eff_w * 0.5,
            pos_y: cy - eff_h * 0.5,
            scale_x: 1.0,
            scale_y: 1.0,
            crop_left: 0,
            crop_top: 0,
            crop_right: 0,
            crop_bottom: 0,
            rot_deg: 0.0,
        };
        let config = crate::types::TrackingConfig::default();

        scan_target_source_layer(target_name, &padded_rgba, cap_w, cap_h, cap_w * 4, &transform, 0.0, 0.0, &config, bw, bh);

        let target_textures = state.target_textures.read();
        let tex = target_textures.get(target_name).expect("Texture must exist");
        assert_eq!(tex.width, 250);
        assert_eq!(tex.height, 250);
        assert_eq!(tex.base_width, 100);
        assert_eq!(tex.base_height, 100);
        assert!((tex.pad_ratio_x() - 2.5).abs() < 1e-4);
        assert!((tex.pad_ratio_y() - 2.5).abs() < 1e-4);
        assert!(tex.is_padded_reaction());

        // Verify hitbox
        let hitboxes = state.hitboxes.read();
        let hb = hitboxes.get(target_name).expect("Hitbox must exist");
        assert!(hb.active);
        assert_eq!(hb.local_x as u32, 100);
        assert_eq!(hb.local_width as u32, 50);

        // Verify Studio scaling formula
        let sw = src.width; // 100.0
        let mesh_w = sw * tex.pad_ratio_x(); // 250.0
        let u0 = hb.local_x / tex.width as f32; // 100 / 250 = 0.4
        let u1 = (hb.local_x + hb.local_width) / tex.width as f32; // 150 / 250 = 0.6
        let h_lx0 = (u0 - 0.5) * mesh_w; // -25.0
        let h_lx1 = (u1 - 0.5) * mesh_w; // 25.0
        let on_screen_hb_width = h_lx1 - h_lx0;
        assert!((on_screen_hb_width - 50.0).abs() < 1e-3, "Hitbox must be 1:1 on-screen (50px), got {}", on_screen_hb_width);
    }

    #[test]
    fn test_frame_mask_lockstep_physics_synchronization() {
        // 1. Verify dt conversion and storage
        let test_dt = 0.0166667f32;
        LAST_TICK_SECONDS.store(test_dt.to_bits(), std::sync::atomic::Ordering::Relaxed);
        let read_dt = f32::from_bits(LAST_TICK_SECONDS.load(std::sync::atomic::Ordering::Relaxed));
        assert!((read_dt - test_dt).abs() < 1e-6);

        // 2. Verify RENDER_ACTIVE_FLAG state machine
        // Simulate normal render cycle:
        // Render marks active flag = true
        RENDER_ACTIVE_FLAG.store(true, std::sync::atomic::Ordering::Relaxed);
        assert!(RENDER_ACTIVE_FLAG.load(std::sync::atomic::Ordering::Relaxed));

        // Next tick consumes the flag
        let rendered = RENDER_ACTIVE_FLAG.swap(false, std::sync::atomic::Ordering::Relaxed);
        assert!(rendered, "Render was active, tick should not do fallback step");
        assert!(!RENDER_ACTIVE_FLAG.load(std::sync::atomic::Ordering::Relaxed));

        // Subsequent tick with NO intervening render (source hidden in OBS):
        let rendered_second_tick = RENDER_ACTIVE_FLAG.swap(false, std::sync::atomic::Ordering::Relaxed);
        assert!(!rendered_second_tick, "No render occurred, tick MUST do fallback step");

        // 3. Verify instant collision response on lockstep step:
        let state = crate::state::get_state();
        let target_name = "LockstepTarget";

        let transform = crate::types::SourceTransform {
            pos_x: 500.0,
            pos_y: 500.0,
            scale_x: 1.0,
            scale_y: 1.0,
            rot_deg: 0.0,
            crop_left: 0,
            crop_top: 0,
            crop_right: 0,
            crop_bottom: 0,
        };
        // Setup a 100x100 solid mask at (500, 500)
        let mut mask = crate::tracker::CollisionMask::new(100, 100, transform);
        for y in 0..100 {
            for x in 0..100 {
                mask.set_solid_local(x, y, true);
            }
        }
        let hb = crate::types::Hitbox {
            world_x: 500.0,
            world_y: 500.0,
            world_width: 100.0,
            world_height: 100.0,
            active: true,
            ..Default::default()
        };

        // Scan/insert into state
        state.hitboxes.write().insert(target_name.to_string(), hb);
        state.masks.write().insert(target_name.to_string(), mask);
        state.target_slots.write().insert(1, target_name.to_string());

        // Spawn a throwable flying directly toward (550, 550)
        let pkg = crate::physics::SpawnPackage {
            name: Some("LockstepThrow".to_string()),
            action: Some("throwable".to_string()),
            collision: Some("bounce".to_string()),
            target: Some(target_name.to_string()),
            speed: Some(15.0),
            scale: Some(1.0),
            gravity: Some(0.0),
            ..Default::default()
        };
        let th_id = state.physics.write().spawn(pkg, 1920.0, 1080.0);
        assert!(th_id > 0);

        {
            let mut physics = state.physics.write();
            if let Some(t) = physics.throwables.iter_mut().find(|t| t.id == th_id) {
                t.x = 550.0;
                t.y = 550.0; // Place right inside the solid mask
                // …and keep it there: it still had the speed of a throw from
                // a random spawn point, and from some of them one step carried
                // it ~50 px, out of the 100 px mask (the test failed at random)
                t.vx = 0.0;
                t.vy = 0.0;
            }
        }

        // Stepping physics right now (as done in video_render right after scan)
        // must detect the collision immediately against this frame's mask
        {
            let mut physics = state.physics.write();
            physics.step(0.016667, 1920.0, 1080.0);
            let updated_th = physics.throwables.iter().find(|t| t.id == th_id).unwrap();
            assert!(updated_th.has_hit, "Throwable must hit the fresh mask in the exact same frame!");
        }
    }
}

