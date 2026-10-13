pub mod types;
pub mod tracker;
pub mod reader_client;
pub mod state;
pub mod physics;
pub mod fluid;
pub mod fire;
pub mod lightning;
pub mod effects;
pub mod font;
pub mod obs_source;
pub mod obs_procs;
pub mod item_fx;
pub mod filter_props;
pub mod sound;
pub mod studio_app;
pub mod custom_lua;
pub mod lua_shield;
pub mod redeem_package;
pub mod script_catalog;
pub mod script_filters;
pub mod script_draw;
pub mod script_feed;
pub mod egui_docks;
pub mod script_docks;
pub mod render_gate;
pub mod trigger_builder;
pub mod supporters;
pub mod paths;

use std::collections::HashMap;
use std::ffi::{c_char, CStr, CString};
use std::slice;
use base64::prelude::*;
use crate::state::get_state;
use crate::tracker::{scan_rgba_with_transform, CollisionMask};
use crate::types::{GizmoDragMode, Hitbox, Point2D, SourceTransform};

/// Initialize the Trust runtime.
#[unsafe(no_mangle)]
pub extern "C" fn trust_init() -> bool {
    let _ = get_state();
    let _ = crate::obs_source::trust_register_obs_source();
    let count = crate::studio_app::load_studio_targets_from_disk();
    let r_count = crate::studio_app::reload_global_redeems();
    println!("[Trust Core] Runtime initialized with {} studio targets and {} redeems.", count, r_count);
    true
}

/// Shutdown the Trust runtime and close background connections.
#[unsafe(no_mangle)]
pub extern "C" fn trust_shutdown() {
    let state = get_state();
    state.reader_client.stop();
    println!("[Trust Core] Runtime shutdown complete.");
}

// ============================================================================
// OBS native plugin module entry points — this makes libtrust.so itself the
// module OBS loads directly (deployed as theater-scene.so), replacing the
// old C++ qt_dialog.cpp module. The Tools-menu item is registered via
// libobs-frontend-api's plain C function directly, no Qt/Lua bridge needed.
// ============================================================================

#[unsafe(no_mangle)]
pub extern "C" fn obs_module_ver() -> u32 {
    (30u32 << 24) | (0u32 << 16) | 0u32
}

#[unsafe(no_mangle)]
pub extern "C" fn obs_module_set_pointer(module: *mut std::ffi::c_void) {
    crate::paths::set_module(module);
}

/// Panics in the plugin's own threads (e.g. the Studio window's) otherwise
/// vanish without a trace on Windows — OBS has no console. Log them to
/// trust_debug.log with location and thread, then run the default hook.
fn install_panic_logger() {
    static INSTALLED: std::sync::Once = std::sync::Once::new();
    INSTALLED.call_once(|| {
        let default_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let thread = std::thread::current();
            crate::obs_source::trust_log(&format!(
                "[Trust PANIC] thread '{}': {}",
                thread.name().unwrap_or("<unnamed>"),
                info
            ));
            default_hook(info);
        }));
    });
}

/// Keeps this copy of the DLL loaded until OBS's process exits. The Tokio worker
/// threads (Theater Reader client) outlive the DLL being unloaded, and OBS
/// crashed on exit when one woke up after it was unmapped — e.g. when
/// obs-starter kills Theater-reader and its WebSocket closes.
///
/// Done from DllMain because OBS isn't the only loader: theater_chat.lua
/// ffi.loads its own copy (from %APPDATA%), which never gets obs_module_load
/// and is unloaded with the Lua scripts.
#[cfg(target_os = "windows")]
fn pin_module() {
    unsafe extern "system" {
        fn GetModuleHandleExW(dwFlags: u32, lpModuleName: *const u16, phModule: *mut *mut std::ffi::c_void) -> i32;
    }
    const GET_MODULE_HANDLE_EX_FLAG_PIN: u32 = 0x00000001;
    const GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS: u32 = 0x00000004;
    let mut h_module: *mut std::ffi::c_void = std::ptr::null_mut();
    unsafe {
        GetModuleHandleExW(
            GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_PIN,
            pin_module as *const u16,
            &mut h_module,
        );
    }
}

#[cfg(target_os = "windows")]
#[unsafe(no_mangle)]
pub extern "system" fn DllMain(_module: *mut std::ffi::c_void, reason: u32, _reserved: *mut std::ffi::c_void) -> i32 {
    const DLL_PROCESS_ATTACH: u32 = 1;
    if reason == DLL_PROCESS_ATTACH {
        pin_module();
    }
    1
}

#[unsafe(no_mangle)]
pub extern "C" fn obs_module_load() -> bool {
    install_panic_logger();
    // Before anything reads your poses, redeems or settings
    crate::paths::import_old_data();
    // Before trust_init, which loads the redeems and signals the list change
    crate::obs_procs::register();
    trust_init();
    // Here, not in trust_init (theater_chat.lua's own copy calls that too):
    // filter types must be registered before OBS loads the scenes using them
    crate::script_filters::register_all();
    crate::obs_source::load_mask_settings();
    // Before scenes load too: saved ones hold these filters
    crate::render_gate::register();
    // Chat and alerts: whatever serves the relay (the Theater Reader plugin,
    // the Theater Reader app…); the trigger lists learn from them
    crate::trigger_builder::install();
    let state = get_state();
    state.reader_client.start(&state.runtime, "127.0.0.1".to_string(), crate::script_feed::relay_port());
    crate::obs_source::trust_log("[Trust Core] 🚀 Native module initialized (theater-scene.dll).");
    crate::obs_source::setup_frontend_tools_menu();
    crate::obs_source::setup_action_docks();
    true
}

#[unsafe(no_mangle)]
pub extern "C" fn obs_module_unload() {
    trust_shutdown();
    println!("[Trust Core] 🛑 Module unloaded cleanly.");
}

/// Connect to the Theater Reader WebSocket server.
#[unsafe(no_mangle)]
pub extern "C" fn trust_connect_reader(port: u16) -> bool {
    let state = get_state();
    let p = if port == 0 { 41837 } else { port };
    state.reader_client.start(&state.runtime, "127.0.0.1".to_string(), p);
    true
}

/// Check if currently connected to Theater Reader.
#[unsafe(no_mangle)]
pub extern "C" fn trust_is_reader_connected() -> bool {
    get_state().reader_client.is_connected()
}

/// Update tracking configuration (mode 0 = Alpha Transparency, 1 = Chroma HSV, 2 = Brightness).
#[unsafe(no_mangle)]
pub extern "C" fn trust_update_config(
    mode: u8,
    alpha_threshold: u8,
    r: u8,
    g: u8,
    b: u8,
    threshold: u8,
    precision: u8,
) {
    let state = get_state();
    let mut cfg = state.config.write();
    cfg.mode = mode;
    cfg.alpha_threshold = alpha_threshold;
    cfg.chroma_r = r;
    cfg.chroma_g = g;
    cfg.chroma_b = b;
    cfg.threshold = threshold;
    cfg.precision = precision.clamp(1, 10);
    println!(
        "[Trust Core] Config updated: Mode={}, AlphaThresh={}, Color=({}, {}, {}), Thresh={}, Precision={}",
        mode, alpha_threshold, r, g, b, threshold, precision
    );
}

/// Process an RGBA pixel buffer for a target with OBS stream-space transforms.
#[unsafe(no_mangle)]
pub extern "C" fn trust_process_rgba_transformed(
    target_name: *const c_char,
    data: *const u8,
    width: u32,
    height: u32,
    stride: u32,
    transform: SourceTransform,
) -> Hitbox {
    if data.is_null() || width == 0 || height == 0 || stride == 0 {
        return Hitbox::default();
    }

    let name = if !target_name.is_null() {
        unsafe { CStr::from_ptr(target_name).to_string_lossy().to_string() }
    } else {
        "Avatar".to_string()
    };

    let total_bytes = (height * stride) as usize;
    let slice = unsafe { slice::from_raw_parts(data, total_bytes) };

    let state = get_state();
    let config = *state.config.read();

    let (hitbox, mask, pixel_grid) = scan_rgba_with_transform(slice, width, height, stride, &transform, &config);

    // Update global multi-target cache
    state.hitboxes.write().insert(name.clone(), hitbox);
    state.masks.write().insert(name.clone(), mask);
    state.pixel_grids.write().insert(name.clone(), pixel_grid);

    // Save full-color RGBA texture for preview
    let mut rgba_copy = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        let row_start = (y * stride) as usize;
        let row_end = row_start + (width * 4) as usize;
        if row_end <= slice.len() {
            rgba_copy.extend_from_slice(&slice[row_start..row_end]);
        }
    }
    state.target_textures.write().insert(name.clone(), crate::state::TargetPreviewTexture {
        width,
        height,
        rgba: rgba_copy,
        frame_id: 0,
        base_width: width,
        base_height: height,
    });

    if hitbox.active {
        // Debounced or active log
        println!(
            "[Trust] Tracked '{}': World=({:.1}, {:.1}) {:.1}x{:.1}, Local=({:.1}, {:.1}) {:.1}x{:.1}, Mass={}",
            name, hitbox.world_x, hitbox.world_y, hitbox.world_width, hitbox.world_height,
            hitbox.local_x, hitbox.local_y, hitbox.local_width, hitbox.local_height, hitbox.mass
        );
    }

    hitbox
}

/// Load an image file directly (PNG, JPEG, WebP) and compute its exact non-transparent hitbox with transform.
#[unsafe(no_mangle)]
pub extern "C" fn trust_process_image_file(
    target_name: *const c_char,
    file_path: *const c_char,
    transform: SourceTransform,
) -> Hitbox {
    if target_name.is_null() || file_path.is_null() {
        return Hitbox::default();
    }

    let name = unsafe { CStr::from_ptr(target_name).to_string_lossy().to_string() };
    let path = unsafe { CStr::from_ptr(file_path).to_string_lossy().to_string() };

    let img = match image::open(&path) {
        Ok(i) => i.to_rgba8(),
        Err(e) => {
            eprintln!("[Trust] Could not load image file '{}': {}", path, e);
            return Hitbox::default();
        }
    };

    let width = img.width();
    let height = img.height();
    let stride = width * 4;
    let raw = img.into_raw();

    let mut transform = transform;
    if transform.scale_x.abs() < f32::EPSILON {
        transform.scale_x = 1.0;
    }
    if transform.scale_y.abs() < f32::EPSILON {
        transform.scale_y = 1.0;
    }

    let state = get_state();
    let config = *state.config.read();

    let (hitbox, mask, pixel_grid) = scan_rgba_with_transform(&raw, width, height, stride, &transform, &config);

    println!("[Trust] 🎯 Loaded collision mask for '{}': {}x{} pixels, solid points: {}", name, width, height, pixel_grid.len());

    state.hitboxes.write().insert(name.clone(), hitbox);
    state.masks.write().insert(name.clone(), mask);
    state.pixel_grids.write().insert(name.clone(), pixel_grid);
    state.target_textures.write().insert(name.clone(), crate::state::TargetPreviewTexture {
        width,
        height,
        rgba: raw,
        frame_id: 0,
        base_width: width,
        base_height: height,
    });

    hitbox
}

/// Update the transform of an already loaded target mask and recompute hitbox
#[unsafe(no_mangle)]
pub extern "C" fn trust_update_target_transform(
    target_name: *const c_char,
    transform: SourceTransform,
) -> bool {
    if target_name.is_null() {
        return false;
    }
    let name = unsafe { CStr::from_ptr(target_name).to_string_lossy() };
    let state = get_state();

    // Fast-path: If collision mask already exists, update its transform in-place!
    // NEVER re-scan the multi-megapixel raw RGBA texture just because position/scale/rotation changed!
    {
        // hitboxes before masks — the same order every other reader/writer of both takes
        let mut hitboxes = state.hitboxes.write();
        let mut masks = state.masks.write();
        if let Some(mask) = masks.get_mut(name.as_ref()) {
            mask.transform = transform;
            if let Some(hb) = hitboxes.get_mut(name.as_ref()) {
                // Keeps the target point on the mask's center of mass, not the middle of the source
                crate::tracker::reproject_hitbox(hb, mask);
            }
            return true;
        }
    }

    // Fallback: Initial scan only when mask has never been generated
    let config = *state.config.read();
    let textures = state.target_textures.read();
    if let Some(tex) = textures.get(name.as_ref()) {
        let (hitbox, mask, pixel_grid) = scan_rgba_with_transform(&tex.rgba, tex.width, tex.height, tex.width * 4, &transform, &config);
        state.hitboxes.write().insert(name.to_string(), hitbox);
        state.masks.write().insert(name.to_string(), mask);
        state.pixel_grids.write().insert(name.to_string(), pixel_grid);
        true
    } else {
        false
    }
}

/// Retrieve the cached full-color texture info for a target.
#[unsafe(no_mangle)]
pub extern "C" fn trust_get_target_texture_info(
    target_name: *const c_char,
    out_width: *mut u32,
    out_height: *mut u32,
) -> bool {
    if target_name.is_null() || out_width.is_null() || out_height.is_null() {
        return false;
    }
    let name = unsafe { CStr::from_ptr(target_name).to_string_lossy() };
    let state = get_state();
    let textures = state.target_textures.read();
    if let Some(tex) = textures.get(name.as_ref()) {
        unsafe {
            *out_width = tex.width;
            *out_height = tex.height;
        }
        true
    } else {
        false
    }
}

/// Copy the full-color RGBA texture bytes for a target.
#[unsafe(no_mangle)]
pub extern "C" fn trust_copy_target_texture_rgba(
    target_name: *const c_char,
    out_buf: *mut u8,
    max_len: usize,
) -> usize {
    if target_name.is_null() || out_buf.is_null() || max_len == 0 {
        return 0;
    }
    let name = unsafe { CStr::from_ptr(target_name).to_string_lossy() };
    let state = get_state();
    let textures = state.target_textures.read();
    if let Some(tex) = textures.get(name.as_ref()) {
        let copy_len = tex.rgba.len().min(max_len);
        unsafe {
            std::ptr::copy_nonoverlapping(tex.rgba.as_ptr(), out_buf, copy_len);
        }
        copy_len
    } else {
        0
    }
}

/// Retrieve the sampled pixel grid point cloud for a target.
#[unsafe(no_mangle)]
pub extern "C" fn trust_get_pixel_grid(
    target_name: *const c_char,
    out_points: *mut Point2D,
    max_points: u32,
) -> u32 {
    if target_name.is_null() || out_points.is_null() || max_points == 0 {
        return 0;
    }

    let name = unsafe { CStr::from_ptr(target_name).to_string_lossy() };
    let state = get_state();
    let grids = state.pixel_grids.read();

    if let Some(grid) = grids.get(name.as_ref()) {
        let count = grid.len().min(max_points as usize);
        unsafe {
            std::ptr::copy_nonoverlapping(grid.as_ptr(), out_points, count);
        }
        count as u32
    } else {
        0
    }
}

/// Render the binary collision mask of a target into an RGBA buffer.
#[unsafe(no_mangle)]
pub extern "C" fn trust_render_mask_rgba(
    target_name: *const c_char,
    out_rgba_buf: *mut u8,
    width: u32,
    height: u32,
    stride: u32,
    solid_r: u8,
    solid_g: u8,
    solid_b: u8,
    solid_a: u8,
    bg_r: u8,
    bg_g: u8,
    bg_b: u8,
    bg_a: u8,
) -> bool {
    if out_rgba_buf.is_null() || width == 0 || height == 0 || stride == 0 {
        return false;
    }

    let name = if !target_name.is_null() {
        unsafe { CStr::from_ptr(target_name).to_string_lossy().to_string() }
    } else {
        String::new()
    };

    let state = get_state();
    let slots = state.target_slots.read();
    let masks = state.masks.read();

    let mask_opt = if !name.is_empty() {
        crate::resolve_target_mask(&name, &*slots, &*masks).map(|(_, m)| m)
            .or_else(|| masks.get(&name))
    } else {
        slots.get(&1).and_then(|tname| masks.get(tname))
            .or_else(|| masks.values().next())
    };

    if let Some(mask) = mask_opt {
        if mask.width != width || mask.height != height {
            return false;
        }
        let total_bytes = (height * stride) as usize;
        let slice = unsafe { slice::from_raw_parts_mut(out_rgba_buf, total_bytes) };
        mask.render_rgba(
            slice,
            stride as usize,
            [solid_r, solid_g, solid_b, solid_a],
            [bg_r, bg_g, bg_b, bg_a],
        );
        true
    } else {
        false
    }
}

/// Legacy process without explicit transform (defaults to 1:1 scale at origin).
#[unsafe(no_mangle)]
pub extern "C" fn trust_process_rgba(
    target_name: *const c_char,
    data: *const u8,
    width: u32,
    height: u32,
    stride: u32,
) -> Hitbox {
    trust_process_rgba_transformed(target_name, data, width, height, stride, SourceTransform::default())
}

/// Directly register or update a target's stream-space Hitbox.
#[unsafe(no_mangle)]
pub extern "C" fn trust_set_target_hitbox(target_name: *const c_char, hitbox: Hitbox) {
    if target_name.is_null() { return; }
    let name = unsafe { CStr::from_ptr(target_name).to_string_lossy().to_string() };
    let state = get_state();
    state.hitboxes.write().insert(name, hitbox);
}

/// Retrieve the cached Hitbox for a target by name.
#[unsafe(no_mangle)]
pub extern "C" fn trust_get_hitbox(target_name: *const c_char, out_hitbox: *mut Hitbox) -> bool {
    if target_name.is_null() || out_hitbox.is_null() {
        return false;
    }

    let name = unsafe { CStr::from_ptr(target_name).to_string_lossy() };
    let state = get_state();
    let slots = state.target_slots.read();
    let hitboxes = state.hitboxes.read();

    if let Some((_, hb)) = resolve_target_hitbox(name.trim(), &slots, &hitboxes) {
        unsafe {
            *out_hitbox = *hb;
        }
        true
    } else {
        unsafe {
            *out_hitbox = Hitbox::default();
        }
        false
    }
}

/// Retrieve the cached SourceTransform for a target by slot or name.
#[unsafe(no_mangle)]
pub extern "C" fn trust_get_target_transform(
    target_name: *const c_char,
    out_transform: *mut crate::types::SourceTransform,
) -> bool {
    if out_transform.is_null() {
        return false;
    }
    let name = if !target_name.is_null() {
        unsafe { CStr::from_ptr(target_name).to_string_lossy().to_string() }
    } else {
        String::new()
    };

    let state = get_state();
    let slots = state.target_slots.read();
    let masks = state.masks.read();

    let mask_opt = if !name.is_empty() {
        crate::resolve_target_mask(&name, &*slots, &*masks).map(|(_, m)| m)
            .or_else(|| masks.get(&name))
    } else {
        slots.get(&1).and_then(|tname| masks.get(tname))
            .or_else(|| masks.values().next())
    };

    if let Some(mask) = mask_opt {
        unsafe {
            *out_transform = mask.transform;
        }
        true
    } else {
        false
    }
}

/// Test if a specific world (stream-space) pixel coordinate is solid on the target's collision mask.
#[unsafe(no_mangle)]
pub extern "C" fn trust_is_pixel_solid(target_name: *const c_char, world_x: f32, world_y: f32) -> bool {
    if target_name.is_null() || world_x < 0.0 || world_y < 0.0 {
        return false;
    }

    let name = unsafe { CStr::from_ptr(target_name).to_string_lossy() };
    let state = get_state();
    let masks = state.masks.read();

    if let Some(mask) = masks.get(name.as_ref()) {
        mask.is_solid_world(world_x, world_y)
    } else {
        false
    }
}

/// Get the number of currently tracked targets.
#[unsafe(no_mangle)]
pub extern "C" fn trust_get_target_count() -> u32 {
    get_state().hitboxes.read().len() as u32
}

/// Retrieve the name of a tracked target by index.
#[unsafe(no_mangle)]
pub extern "C" fn trust_get_target_name(index: u32, out_name_buf: *mut c_char, max_len: usize) -> bool {
    if out_name_buf.is_null() || max_len == 0 {
        return false;
    }

    let state = get_state();
    let hitboxes = state.hitboxes.read();
    let names: Vec<&String> = hitboxes.keys().collect();

    if let Some(name) = names.get(index as usize) {
        let bytes = name.as_bytes();
        let copy_len = bytes.len().min(max_len.saturating_sub(1));
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), out_name_buf as *mut u8, copy_len);
            *out_name_buf.add(copy_len) = 0;
        }
        true
    } else {
        false
    }
}

/// Remove a target from tracking by name.
#[unsafe(no_mangle)]
pub extern "C" fn trust_remove_target(target_name: *const c_char) {
    if target_name.is_null() { return; }
    let name = unsafe { CStr::from_ptr(target_name).to_string_lossy().to_string() };
    let state = get_state();
    state.hitboxes.write().remove(&name);
    state.masks.write().remove(&name);
    state.pixel_grids.write().remove(&name);
}

/// Clear all tracked targets.
#[unsafe(no_mangle)]
pub extern "C" fn trust_clear_targets() {
    let state = get_state();
    state.hitboxes.write().clear();
    state.masks.write().clear();
    state.pixel_grids.write().clear();
}

/// Poll the next stream event for a specific script client (0=theater, 1=manager, 2=editor).
#[unsafe(no_mangle)]
pub extern "C" fn trust_poll_event_client(client_id: u32, out_json_buf: *mut c_char, max_len: usize) -> bool {
    if out_json_buf.is_null() || max_len == 0 {
        return false;
    }

    let state = get_state();
    let rx = state.get_or_create_client_receiver(client_id);
    match rx.try_recv() {
        Ok(event) => {
            if let Ok(json_str) = serde_json::to_string(&event) {
                let bytes = json_str.as_bytes();
                let copy_len = bytes.len().min(max_len.saturating_sub(1));

                unsafe {
                    std::ptr::copy_nonoverlapping(bytes.as_ptr(), out_json_buf as *mut u8, copy_len);
                    *out_json_buf.add(copy_len) = 0; // Null terminator
                }
                true
            } else {
                false
            }
        }
        Err(_) => false,
    }
}

/// Poll the next stream event from the default queue (client 0).
#[unsafe(no_mangle)]
pub extern "C" fn trust_poll_event(out_json_buf: *mut c_char, max_len: usize) -> bool {
    trust_poll_event_client(0, out_json_buf, max_len)
}

/// Get the number of pending stream events for client.
#[unsafe(no_mangle)]
pub extern "C" fn trust_get_event_count_client(client_id: u32) -> u32 {
    let state = get_state();
    let rx = state.get_or_create_client_receiver(client_id);
    rx.len() as u32
}

/// Get the number of pending stream events in default queue.
#[unsafe(no_mangle)]
pub extern "C" fn trust_get_event_count() -> u32 {
    trust_get_event_count_client(0)
}

/// Inject and broadcast a simulated or test event across all scripts: a
/// Theater Reader message (JSON), or plain text taken as an event's trigger
/// string (e.g. "Twitch Mortygotvr redeem Throw Tomato"). Chat messages fire
/// no trigger (see studio_app::process_trigger_event); Rhai sources (the
/// chat) get it as if the relay had sent it.
#[unsafe(no_mangle)]
pub extern "C" fn trust_inject_event(json_str: *const c_char) -> bool {
    if json_str.is_null() { return false; }
    let s = unsafe { CStr::from_ptr(json_str).to_string_lossy().to_string() };
    crate::script_feed::publish(&s);

    let Some(event) = crate::reader_client::parse_reader_message(&s) else { return false };
    crate::studio_app::process_trigger_event(&event);
    get_state().broadcast_event(event);
    true
}

/// Register active triggers currently being edited/captured in the Editor.
#[unsafe(no_mangle)]
pub extern "C" fn trust_set_editor_triggers(triggers_csv: *const c_char) {
    if triggers_csv.is_null() {
        get_state().clear_editor_triggers();
        return;
    }
    let s = unsafe { CStr::from_ptr(triggers_csv).to_string_lossy() };
    get_state().set_editor_triggers(&s);
}

/// Check if a trigger is currently being captured/handled by the Editor.
#[unsafe(no_mangle)]
pub extern "C" fn trust_is_editor_trigger(trigger_str: *const c_char) -> bool {
    if trigger_str.is_null() { return false; }
    let s = unsafe { CStr::from_ptr(trigger_str).to_string_lossy() };
    get_state().is_editor_trigger(&s)
}

/// Clear active editor triggers.
#[unsafe(no_mangle)]
pub extern "C" fn trust_clear_editor_triggers() {
    get_state().clear_editor_triggers();
}

use image::codecs::gif::GifDecoder;
use image::AnimationDecoder;

/// Helper to decode any image format (including animated GIF) into a standardized 256px-height frame sequence.
fn decode_and_resize_frames(bytes: &[u8]) -> (Vec<image::RgbaImage>, u32) {
    // 1. Try animated GIF decoding
    if let Ok(decoder) = GifDecoder::new(std::io::Cursor::new(bytes)) {
        if let Ok(frames) = decoder.into_frames().collect_frames() {
            if !frames.is_empty() {
                let delay = frames[0].delay();
                let (numer, denom) = delay.numer_denom_ms();
                let delay_ms = if denom > 0 && numer > 0 { (numer / denom).max(20) } else { 100 };
                let mut rgba_frames = Vec::with_capacity(frames.len());
                for frame in frames {
                    let rgba = frame.into_buffer();
                    let orig_w = rgba.width();
                    let orig_h = rgba.height();
                    let resized = if orig_h != 256 && orig_h > 0 {
                        let new_w = ((orig_w as f32 / orig_h as f32) * 256.0).round().max(1.0) as u32;
                        image::imageops::resize(&rgba, new_w, 256, image::imageops::FilterType::Lanczos3)
                    } else {
                        rgba
                    };
                    rgba_frames.push(resized);
                }
                return (rgba_frames, delay_ms);
            }
        }
    }

    // 2. Static / single frame fallback (PNG, JPG, WebP, single-frame GIF)
    if let Ok(img) = image::load_from_memory(bytes) {
        let rgba = img.to_rgba8();
        let orig_w = rgba.width();
        let orig_h = rgba.height();
        let resized = if orig_h != 256 && orig_h > 0 {
            let new_w = ((orig_w as f32 / orig_h as f32) * 256.0).round().max(1.0) as u32;
            image::imageops::resize(&rgba, new_w, 256, image::imageops::FilterType::Lanczos3)
        } else {
            rgba
        };
        return (vec![resized], 100);
    }

    (Vec::new(), 100)
}

/// Decode a base64 or Data-URL image (including animated GIFs) into a 256px-height RGBA spritesheet buffer.
#[unsafe(no_mangle)]
pub extern "C" fn trust_decode_image_base64(
    base64_str: *const c_char,
    out_rgba_buf: *mut u8,
    max_bytes: u32,
    out_frame_width: *mut u32,
    out_frame_height: *mut u32,
    out_frame_count: *mut u32,
    out_frame_delay_ms: *mut u32,
) -> u32 {
    if base64_str.is_null() || out_rgba_buf.is_null() || max_bytes == 0 {
        return 0;
    }

    let input = unsafe { CStr::from_ptr(base64_str).to_string_lossy() };
    let raw_b64 = if let Some(idx) = input.find(";base64,") {
        &input[idx + 8..]
    } else {
        input.as_ref()
    };

    let bytes = match BASE64_STANDARD.decode(raw_b64.trim()) {
        Ok(b) => b,
        Err(_) => return 0,
    };

    let (frames, delay_ms) = decode_and_resize_frames(&bytes);
    if frames.is_empty() {
        return 0;
    }

    let frame_w = frames[0].width();
    let frame_h = frames[0].height();
    let count = frames.len() as u32;
    let total_w = frame_w * count;
    let total_bytes = (total_w * frame_h * 4) as usize;

    if total_bytes > max_bytes as usize {
        return 0;
    }

    let mut spritesheet = image::RgbaImage::new(total_w, frame_h);
    for (i, frame) in frames.iter().enumerate() {
        image::imageops::overlay(&mut spritesheet, frame, (i as u32 * frame_w) as i64, 0);
    }

    unsafe {
        if !out_frame_width.is_null() { *out_frame_width = frame_w; }
        if !out_frame_height.is_null() { *out_frame_height = frame_h; }
        if !out_frame_count.is_null() { *out_frame_count = count; }
        if !out_frame_delay_ms.is_null() { *out_frame_delay_ms = delay_ms; }
        std::ptr::copy_nonoverlapping(spritesheet.as_raw().as_ptr(), out_rgba_buf, total_bytes);
    }

    total_bytes as u32
}

/// Load an image from a local file path into raw RGBA32 pixels (auto-resized to 256px height).
#[unsafe(no_mangle)]
pub extern "C" fn trust_load_image_file(
    file_path: *const c_char,
    out_rgba_buf: *mut u8,
    max_bytes: u32,
    out_width: *mut u32,
    out_height: *mut u32,
) -> u32 {
    if file_path.is_null() || out_rgba_buf.is_null() || max_bytes == 0 {
        return 0;
    }

    let path = unsafe { CStr::from_ptr(file_path).to_string_lossy() };
    let img = match image::open(path.as_ref()) {
        Ok(i) => i.to_rgba8(),
        Err(_) => return 0,
    };

    let orig_w = img.width();
    let orig_h = img.height();
    let img = if orig_h != 256 && orig_h > 0 {
        let new_w = ((orig_w as f32 / orig_h as f32) * 256.0).round().max(1.0) as u32;
        image::imageops::resize(&img, new_w, 256, image::imageops::FilterType::Lanczos3)
    } else {
        img
    };

    let w = img.width();
    let h = img.height();
    let total_bytes = (w * h * 4) as usize;

    if total_bytes > max_bytes as usize {
        return 0;
    }

    unsafe {
        if !out_width.is_null() { *out_width = w; }
        if !out_height.is_null() { *out_height = h; }
        std::ptr::copy_nonoverlapping(img.as_raw().as_ptr(), out_rgba_buf, total_bytes);
    }

    total_bytes as u32
}

/// Read a local file and format it as a base64 Data-URL string.
/// For images: auto-resizes to 256px height PNG with RGBA alpha channel.
/// Returns string length written, or 0 on error.
#[unsafe(no_mangle)]
pub extern "C" fn trust_encode_file_base64(
    file_path: *const c_char,
    mime_type: *const c_char,
    out_buf: *mut c_char,
    max_len: usize,
) -> u32 {
    if file_path.is_null() || out_buf.is_null() || max_len == 0 {
        return 0;
    }

    let path = unsafe { CStr::from_ptr(file_path).to_string_lossy() };
    let mime = if !mime_type.is_null() {
        unsafe { CStr::from_ptr(mime_type).to_string_lossy().to_string() }
    } else {
        "application/octet-stream".to_string()
    };

    let is_gif = path.ends_with(".gif") || mime.contains("gif");
    let is_image = mime.starts_with("image/") || path.ends_with(".png") || path.ends_with(".jpg") || path.ends_with(".jpeg") || path.ends_with(".webp") || is_gif;
    let (final_mime, bytes) = if is_gif {
        ("image/gif".to_string(), std::fs::read(path.as_ref()).unwrap_or_default())
    } else if is_image {
        if let Ok(img) = image::open(path.as_ref()) {
            let rgba = img.to_rgba8();
            let orig_w = rgba.width();
            let orig_h = rgba.height();
            let resized = if orig_h != 256 && orig_h > 0 {
                let new_w = ((orig_w as f32 / orig_h as f32) * 256.0).round().max(1.0) as u32;
                image::imageops::resize(&rgba, new_w, 256, image::imageops::FilterType::Lanczos3)
            } else {
                rgba
            };
            let mut png_bytes = Vec::new();
            let mut cursor = std::io::Cursor::new(&mut png_bytes);
            if resized.write_to(&mut cursor, image::ImageFormat::Png).is_ok() {
                ("image/png".to_string(), png_bytes)
            } else {
                (mime, std::fs::read(path.as_ref()).unwrap_or_default())
            }
        } else {
            (mime, std::fs::read(path.as_ref()).unwrap_or_default())
        }
    } else {
        (mime, match std::fs::read(path.as_ref()) {
            Ok(b) => b,
            Err(_) => return 0,
        })
    };

    let b64 = BASE64_STANDARD.encode(&bytes);
    let data_url = format!("data:{};base64,{}", final_mime, b64);
    let data_bytes = data_url.as_bytes();

    let copy_len = data_bytes.len().min(max_len.saturating_sub(1));
    unsafe {
        std::ptr::copy_nonoverlapping(data_bytes.as_ptr(), out_buf as *mut u8, copy_len);
        *out_buf.add(copy_len) = 0;
    }

    copy_len as u32
}

pub fn play_audio_base64_str(input: &str, _volume: f32) -> bool {
    let raw_b64 = if let Some(idx) = input.find(";base64,") {
        &input[idx + 8..]
    } else {
        input
    };

    let bytes = match BASE64_STANDARD.decode(raw_b64.trim()) {
        Ok(b) => b,
        Err(_) => return false,
    };

    let tmp_dir = std::env::temp_dir();
    let ext = if input.contains("audio/ogg") { "ogg" } else if input.contains("audio/wav") { "wav" } else { "mp3" };
    let tmp_path = tmp_dir.join(format!("theater_snd_{}.{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis(), ext));
    
    if std::fs::write(&tmp_path, &bytes).is_err() {
        return false;
    }

    // Through OBS (in the stream's mix, and monitored); the copy is removed once played
    #[cfg(target_os = "windows")]
    {
        crate::sound::play_file(&tmp_path, 1.0, true)
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::thread::spawn(move || {
            let _ = std::process::Command::new("pw-play")
                .arg(&tmp_path)
                .output()
                .or_else(|_| std::process::Command::new("paplay").arg(&tmp_path).output())
                .or_else(|_| std::process::Command::new("aplay").arg(&tmp_path).output());
            let _ = std::fs::remove_file(&tmp_path);
        });
        true
    }
}

/// Play audio from a base64 string or data URL asynchronously.
#[unsafe(no_mangle)]
pub extern "C" fn trust_play_audio_base64(base64_str: *const c_char, volume: f32) -> bool {
    if base64_str.is_null() { return false; }
    let input = unsafe { CStr::from_ptr(base64_str).to_string_lossy() };
    play_audio_base64_str(&input, volume)
}

/// Play audio from a local file path asynchronously.
#[unsafe(no_mangle)]
pub extern "C" fn trust_play_audio_file(file_path: *const c_char, _volume: f32) -> bool {
    if file_path.is_null() { return false; }
    let path = unsafe { CStr::from_ptr(file_path).to_string_lossy().to_string() };

    // Through OBS (in the stream's mix, and monitored)
    #[cfg(target_os = "windows")]
    {
        crate::sound::play_file(std::path::Path::new(&path), 1.0, false)
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::thread::spawn(move || {
            let _ = std::process::Command::new("pw-play")
                .arg(&path)
                .output()
                .or_else(|_| std::process::Command::new("paplay").arg(&path).output())
                .or_else(|_| std::process::Command::new("aplay").arg(&path).output());
        });
        true
    }
}

/// Universal audio playback helper: accepts base64 data URIs, raw base64, absolute or relative file paths.
pub fn play_sound_str(sound_str: &str) -> bool {
    let s = sound_str.trim();
    if s.is_empty() {
        return false;
    }

    if s.starts_with("data:") || s.contains(";base64,") || s.len() > 512 {
        play_audio_base64_str(s, 1.0)
    } else {
        let p = std::path::Path::new(s);
        if p.is_file() {
            if let Ok(c_path) = std::ffi::CString::new(s) {
                return trust_play_audio_file(c_path.as_ptr(), 1.0);
            }
        }
        // Named relative to your redeems, your data or what ships with Theater
        if let Some(found) = crate::paths::find_file(s) {
            if let Some(found_str) = found.to_str() {
                if let Ok(c_path) = std::ffi::CString::new(found_str) {
                    return trust_play_audio_file(c_path.as_ptr(), 1.0);
                }
            }
        }
        if let Some(b64) = crate::studio_app::resolve_and_encode_asset(s) {
            return play_audio_base64_str(&b64, 1.0);
        }
        if let Ok(c_path) = std::ffi::CString::new(s) {
            trust_play_audio_file(c_path.as_ptr(), 1.0)
        } else {
            false
        }
    }
}

/// Play audio from either a base64 string, data URL, or local file path asynchronously.
#[unsafe(no_mangle)]
pub extern "C" fn trust_play_sound(sound_str: *const c_char, _volume: f32) -> bool {
    if sound_str.is_null() { return false; }
    let s = unsafe { CStr::from_ptr(sound_str).to_string_lossy() };
    play_sound_str(&s)
}

/// Read a text or JSON file from disk into a C string buffer.
#[unsafe(no_mangle)]
pub extern "C" fn trust_read_file_text(
    file_path: *const c_char,
    out_buf: *mut c_char,
    max_len: usize,
) -> u32 {
    if file_path.is_null() || out_buf.is_null() || max_len == 0 {
        return 0;
    }
    let path = unsafe { CStr::from_ptr(file_path).to_string_lossy() };
    let content = match std::fs::read_to_string(path.as_ref()) {
        Ok(s) => s,
        Err(_) => return 0,
    };
    let bytes = content.as_bytes();
    let copy_len = bytes.len().min(max_len.saturating_sub(1));
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), out_buf as *mut u8, copy_len);
        *out_buf.add(copy_len) = 0;
    }
    copy_len as u32
}

/// Write a text or JSON string to disk at file_path.
#[unsafe(no_mangle)]
pub extern "C" fn trust_write_file_text(
    file_path: *const c_char,
    text_content: *const c_char,
) -> bool {
    if file_path.is_null() || text_content.is_null() {
        return false;
    }
    let path = unsafe { CStr::from_ptr(file_path).to_string_lossy() };
    let content = unsafe { CStr::from_ptr(text_content).to_string_lossy() };
    
    if let Some(parent) = std::path::Path::new(path.as_ref()).parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    std::fs::write(path.as_ref(), content.as_ref()).is_ok()
}

/// List all .json files in a directory and return a JSON array string e.g. `["tomato", "gremlin", "nom"]`.
#[unsafe(no_mangle)]
pub extern "C" fn trust_list_redeem_keys(
    dir_path: *const c_char,
    out_buf: *mut c_char,
    max_len: usize,
) -> u32 {
    if dir_path.is_null() || out_buf.is_null() || max_len == 0 {
        return 0;
    }
    let dir = unsafe { CStr::from_ptr(dir_path).to_string_lossy() };
    let mut keys = Vec::new();

    if let Ok(entries) = std::fs::read_dir(dir.as_ref()) {
        for entry in entries.flatten() {
            let path = entry.path();
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if ext.eq_ignore_ascii_case("json") || ext.eq_ignore_ascii_case("rdm") {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    if !keys.contains(&stem.to_string()) {
                        keys.push(stem.to_string());
                    }
                }
            }
        }
    }
    keys.sort();

    let json_array = serde_json::to_string(&keys).unwrap_or_else(|_| "[]".to_string());
    let bytes = json_array.as_bytes();
    let copy_len = bytes.len().min(max_len.saturating_sub(1));
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), out_buf as *mut u8, copy_len);
        *out_buf.add(copy_len) = 0;
    }
    copy_len as u32
}

/// Dispatch a redeem JSON payload through the exact same synchronous path
/// every other redeem invocation uses (the Studio's own Test button, the
/// Redeems dock button, and a real trigger match all call
/// dispatch_test_redeem_json directly) — this is that same call, exposed so
/// a script's own Lua logic (e.g. one call per particle in a composed
/// effect) can use the real physics/effects engine directly and
/// synchronously via LuaJIT's FFI, with no queue or polling in between.
#[unsafe(no_mangle)]
pub extern "C" fn trust_trigger_test_redeem(json_str: *const c_char) -> bool {
    if json_str.is_null() { return false; }
    let s = unsafe { CStr::from_ptr(json_str).to_string_lossy().to_string() };
    crate::studio_app::dispatch_test_redeem_json(&s)
}

/// Register an ordered target slot mapping (e.g. 1 -> "Image", 2 -> "Image 2", 3 -> "Image 3").
#[unsafe(no_mangle)]
pub extern "C" fn trust_set_target_slot(slot: usize, target_name: *const c_char) -> bool {
    if target_name.is_null() || slot == 0 { return false; }
    let name = unsafe { CStr::from_ptr(target_name).to_string_lossy().to_string() };
    if name.is_empty() || name == "-- (None) --" {
        get_state().target_slots.write().remove(&slot);
    } else {
        get_state().target_slots.write().insert(slot, name);
    }
    true
}

/// Clear all registered target slot mappings.
#[unsafe(no_mangle)]
pub extern "C" fn trust_clear_target_slots() {
    get_state().target_slots.write().clear();
}

/// Get the source name registered for a specific target slot (1, 2, 3, etc.).
#[unsafe(no_mangle)]
pub extern "C" fn trust_get_target_slot_name(
    slot: usize,
    out_buf: *mut c_char,
    max_len: usize,
) -> u32 {
    if out_buf.is_null() || max_len == 0 || slot == 0 { return 0; }
    let slots = get_state().target_slots.read();
    if let Some(name) = slots.get(&slot) {
        let bytes = name.as_bytes();
        let copy_len = bytes.len().min(max_len.saturating_sub(1));
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), out_buf as *mut u8, copy_len);
            *out_buf.add(copy_len) = 0;
        }
        copy_len as u32
    } else {
        0
    }
}

/// Resolve a target slot string ("1", "2", "random", or name) to the active tracked source name.
#[unsafe(no_mangle)]
pub fn resolve_target_hitbox<'a>(
    target: &str,
    slots: &'a HashMap<usize, String>,
    hitboxes: &'a HashMap<String, Hitbox>,
) -> Option<(&'a str, &'a Hitbox)> {
    // 0. Random target
    if target.eq_ignore_ascii_case("random") {
        let active_tgts: Vec<(&'a str, &'a Hitbox)> = hitboxes.iter()
            .filter(|(_, hb)| hb.active && hb.world_width > 0.0)
            .map(|(k, hb)| (k.as_str(), hb))
            .collect();
        if !active_tgts.is_empty() {
            let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().subsec_nanos();
            let idx = (nanos as usize) % active_tgts.len();
            return Some(active_tgts[idx]);
        }
    }
    // 1. Direct match
    if let Some((k, hb)) = hitboxes.get_key_value(target) {
        if hb.active { return Some((k.as_str(), hb)); }
    }
    // 2. Case-insensitive match in hitboxes
    for (k, v) in hitboxes.iter() {
        if v.active && k.eq_ignore_ascii_case(target) {
            return Some((k.as_str(), v));
        }
    }
    // 3. Slot number resolution
    if let Ok(slot_num) = target.trim().parse::<usize>() {
        if let Some(slot_name) = slots.get(&slot_num) {
            if let Some((k, hb)) = hitboxes.get_key_value(slot_name) {
                if hb.active { return Some((k.as_str(), hb)); }
            }
            for (k, v) in hitboxes.iter() {
                if v.active && k.eq_ignore_ascii_case(slot_name) {
                    return Some((k.as_str(), v));
                }
            }
        }
    }
    // 4. Default / Avatar slot 1 fallback (only if target is empty, "1", or "avatar")
    if target.is_empty() || target == "1" || target.eq_ignore_ascii_case("avatar") || target.eq_ignore_ascii_case("default") {
        if let Some(slot1_name) = slots.get(&1) {
            if let Some((k, hb)) = hitboxes.get_key_value(slot1_name) {
                if hb.active { return Some((k.as_str(), hb)); }
            }
            for (k, v) in hitboxes.iter() {
                if v.active && k.eq_ignore_ascii_case(slot1_name) {
                    return Some((k.as_str(), v));
                }
            }
        }
        // Final fallback: pick any active tracked hitbox only for generic/empty requests
        for (k, v) in hitboxes.iter() {
            if v.active && v.world_width > 0.0 {
                return Some((k.as_str(), v));
            }
        }
    }
    None
}

pub fn resolve_target_mask<'a>(
    target: &str,
    slots: &'a HashMap<usize, String>,
    masks: &'a HashMap<String, CollisionMask>,
) -> Option<(&'a str, &'a CollisionMask)> {
    if let Some((k, m)) = masks.get_key_value(target) {
        return Some((k.as_str(), m));
    }
    for (k, v) in masks.iter() {
        if k.eq_ignore_ascii_case(target) {
            return Some((k.as_str(), v));
        }
    }
    if let Ok(slot_num) = target.trim().parse::<usize>() {
        if let Some(slot_name) = slots.get(&slot_num) {
            if let Some((k, m)) = masks.get_key_value(slot_name) {
                return Some((k.as_str(), m));
            }
            for (k, v) in masks.iter() {
                if k.eq_ignore_ascii_case(slot_name) {
                    return Some((k.as_str(), v));
                }
            }
        }
    }
    if target.is_empty() || target == "1" || target.eq_ignore_ascii_case("avatar") || target.eq_ignore_ascii_case("default") {
        if let Some(slot1_name) = slots.get(&1) {
            if let Some((k, m)) = masks.get_key_value(slot1_name) {
                return Some((k.as_str(), m));
            }
            for (k, v) in masks.iter() {
                if k.eq_ignore_ascii_case(slot1_name) {
                    return Some((k.as_str(), v));
                }
            }
        }
    }
    None
}

pub extern "C" fn trust_resolve_target(
    slot_or_name: *const c_char,
    out_buf: *mut c_char,
    max_len: usize,
) -> u32 {
    if out_buf.is_null() || max_len == 0 { return 0; }
    let req = if !slot_or_name.is_null() {
        unsafe { CStr::from_ptr(slot_or_name).to_string_lossy().trim().to_string() }
    } else {
        "1".to_string()
    };

    let slots = get_state().target_slots.read();
    let hitboxes = get_state().hitboxes.read();

    let resolved = if req.eq_ignore_ascii_case("random") || req.eq_ignore_ascii_case("rand") {
        // Collect all candidate names from target_slots or active hitboxes
        let mut candidates: Vec<String> = Vec::new();
        for (_, name) in slots.iter() {
            if !name.is_empty() && !candidates.contains(name) {
                candidates.push(name.clone());
            }
        }
        if candidates.is_empty() {
            for name in hitboxes.keys() {
                if !candidates.contains(name) {
                    candidates.push(name.clone());
                }
            }
        }
        if !candidates.is_empty() {
            let idx = (std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as usize) % candidates.len();
            candidates[idx].clone()
        } else {
            slots.get(&1).cloned().unwrap_or_else(|| "Avatar".to_string())
        }
    } else if let Some((matched_name, _)) = resolve_target_hitbox(&req, &slots, &hitboxes) {
        matched_name.to_string()
    } else if let Ok(num) = req.parse::<usize>() {
        slots.get(&num).cloned().unwrap_or_else(|| {
            slots.get(&1).cloned().unwrap_or_else(|| "Avatar".to_string())
        })
    } else {
        req
    };

    let bytes = resolved.as_bytes();
    let copy_len = bytes.len().min(max_len.saturating_sub(1));
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), out_buf as *mut u8, copy_len);
        *out_buf.add(copy_len) = 0;
    }
    copy_len as u32
}

/// Register a redeem key and return its unique numeric ID for O(1) rendering lookup.
#[unsafe(no_mangle)]
pub extern "C" fn trust_physics_register_redeem(key: *const c_char) -> u32 {
    if key.is_null() { return 0; }
    let key_str = unsafe { CStr::from_ptr(key).to_string_lossy().to_string() };
    get_state().physics.write().register_redeem(&key_str)
}

/// Spawn a throwable object into the Rust physics engine from JSON.
#[unsafe(no_mangle)]
pub extern "C" fn trust_physics_spawn(
    params_json: *const c_char,
    canvas_w: f32,
    canvas_h: f32,
) -> u64 {
    if params_json.is_null() { return 0; }
    let json_str = unsafe { CStr::from_ptr(params_json).to_string_lossy().to_string() };
    if let Ok(pkg) = serde_json::from_str::<crate::physics::SpawnPackage>(&json_str) {
        get_state().physics.write().spawn(pkg, canvas_w, canvas_h)
    } else {
        0
    }
}

/// Advance the physics simulation AND all effect engines by dt seconds.
#[unsafe(no_mangle)]
pub extern "C" fn trust_physics_step(
    dt_seconds: f32,
    canvas_w: f32,
    canvas_h: f32,
) {
    get_state().physics.write().step(dt_seconds, canvas_w, canvas_h);
    // Also step all active effect engines with live target hitboxes and masks
    let dt_ms = dt_seconds * 1000.0;
    let hitboxes = get_state().hitboxes.read();
    let slots = get_state().target_slots.read();
    let masks = get_state().masks.read();
    get_state().effects.write().step(dt_ms, canvas_w, canvas_h, &*hitboxes, &*slots, &*masks);
}

#[unsafe(no_mangle)]
pub extern "C" fn trust_physics_tick(dt: f32) {
    let (cw, ch) = crate::obs_source::get_obs_canvas_resolution();
    trust_physics_step(dt, cw as f32, ch as f32);
}

/// Get batch array of render instances for GPU rendering.
#[unsafe(no_mangle)]
pub extern "C" fn trust_physics_get_instances(
    out_buf: *mut crate::physics::RenderInstance,
    max_count: usize,
) -> usize {
    if out_buf.is_null() || max_count == 0 { return 0; }
    let slice = unsafe { std::slice::from_raw_parts_mut(out_buf, max_count) };
    get_state().physics.read().get_render_instances(slice)
}

/// Get count of active throwables and particles.
#[unsafe(no_mangle)]
pub extern "C" fn trust_physics_get_count(
    out_throwables: *mut u32,
    out_particles: *mut u32,
) {
    let phys = get_state().physics.read();
    if !out_throwables.is_null() {
        unsafe { *out_throwables = phys.throwables.len() as u32; }
    }
    if !out_particles.is_null() {
        unsafe { *out_particles = phys.particles.len() as u32; }
    }
}

/// Drain and retrieve all recent throwable collision impact events.
#[unsafe(no_mangle)]
pub extern "C" fn trust_physics_drain_impacts(
    out_buf: *mut crate::physics::ThrowableImpactEvent,
    max_count: usize,
) -> usize {
    if out_buf.is_null() || max_count == 0 { return 0; }
    let slice = unsafe { std::slice::from_raw_parts_mut(out_buf, max_count) };
    get_state().physics.write().drain_impacts(slice)
}

/// Clear all active throwables, particles, and effect engines.
#[unsafe(no_mangle)]
pub extern "C" fn trust_physics_clear() {
    get_state().physics.write().clear();
    get_state().effects.write().clear();
}

// ============================================================================
// 💧🔥⚡ Effect Engine FFI (fluid / fire / lightning)
// ============================================================================

/// Trace a ray from (start_x, start_y) toward target center to find the first
/// contact point with the avatar's pixel-perfect alpha mask or outer hitbox boundary.
pub fn find_ray_mask_intersection(
    start_x: f32,
    start_y: f32,
    center_x: f32,
    center_y: f32,
    mask: Option<&CollisionMask>,
    hitbox: Option<&Hitbox>,
) -> (f32, f32) {
    let dx = center_x - start_x;
    let dy = center_y - start_y;
    let total_dist = (dx * dx + dy * dy).sqrt();
    if total_dist < 1.0 {
        return (center_x, center_y);
    }

    let dir_x = dx / total_dist;
    let dir_y = dy / total_dist;

    // 1. Pixel-perfect alpha mask ray-march (sample every 2.0 pixels along the ray)
    if let Some(m) = mask {
        let max_steps = ((total_dist + 100.0) / 2.0) as usize;
        for s in 0..max_steps {
            let dist = s as f32 * 2.0;
            let rx = start_x + dir_x * dist;
            let ry = start_y + dir_y * dist;
            if m.is_solid_world(rx, ry) {
                return (rx, ry);
            }
        }
    }

    // 2. Hitbox AABB boundary intersection fallback
    if let Some(hb) = hitbox {
        if hb.active && hb.world_width > 0.0 && hb.world_height > 0.0 {
            let min_x = hb.world_x;
            let max_x = hb.world_x + hb.world_width;
            let min_y = hb.world_y;
            let max_y = hb.world_y + hb.world_height;

            let mut tmin = 0.0f32;
            let mut tmax = total_dist;

            if dir_x.abs() > 1e-6 {
                let t1 = (min_x - start_x) / dir_x;
                let t2 = (max_x - start_x) / dir_x;
                let (t_near, t_far) = if t1 < t2 { (t1, t2) } else { (t2, t1) };
                tmin = tmin.max(t_near);
                tmax = tmax.min(t_far);
            }

            if dir_y.abs() > 1e-6 {
                let t1 = (min_y - start_y) / dir_y;
                let t2 = (max_y - start_y) / dir_y;
                let (t_near, t_far) = if t1 < t2 { (t1, t2) } else { (t2, t1) };
                tmin = tmin.max(t_near);
                tmax = tmax.min(t_far);
            }

            if tmin <= tmax && tmin >= 0.0 && tmin < total_dist {
                return (start_x + dir_x * tmin, start_y + dir_y * tmin);
            }
        }
    }

    (center_x, center_y)
}

/// Spawn a new effect. effect_type: "fluid", "fire", or "lightning" (also accepts webgl-* aliases).
/// config_json: JSON object with effect parameters (see SphConfig / FireConfig / LightningConfig).
/// Returns an opaque effect handle (unused for now; future cancel support).
#[unsafe(no_mangle)]
pub extern "C" fn trust_effect_spawn(
    effect_type: *const c_char,
    start_x: f32,
    start_y: f32,
    target: *const c_char,
    config_json: *const c_char,
) -> u32 {
    use crate::effects::EffectKind;
    use crate::fluid::SphConfig;
    use crate::fire::FireConfig;
    use crate::lightning::LightningConfig;

    if effect_type.is_null() {
        return 0;
    }
    let type_str = unsafe { CStr::from_ptr(effect_type).to_string_lossy() };
    let target_name = if target.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(target).to_string_lossy().into_owned() }
    };

    // Parse JSON config (allow empty/null → use defaults)
    let json_map: std::collections::HashMap<String, serde_json::Value> = if config_json.is_null() {
        std::collections::HashMap::new()
    } else {
        let s = unsafe { CStr::from_ptr(config_json).to_string_lossy() };
        serde_json::from_str(&s).unwrap_or_default()
    };

    // Get target hitbox center and cached hitbox
    let (center_x, center_y, hb_opt_cloned) = {
        let slots = get_state().target_slots.read();
        let hitboxes = get_state().hitboxes.read();
        if let Some((_, hb)) = resolve_target_hitbox(&target_name, &slots, &hitboxes) {
            let c = if hb.active && hb.world_center_x > 0.0 && hb.world_center_y > 0.0 {
                (hb.world_center_x, hb.world_center_y)
            } else if hb.active && hb.world_width > 0.0 {
                (hb.world_x + hb.world_width * 0.5, hb.world_y + hb.world_height * 0.5)
            } else {
                (960.0, 540.0)
            };
            (c.0, c.1, Some(hb.clone()))
        } else {
            let sources = get_state().studio_sources.read();
            if let Some(src) = sources.iter().find(|s| s.name.eq_ignore_ascii_case(&target_name) || (target_name.is_empty() && s.is_target)) {
                (src.x + src.width * 0.5, src.y + src.height * 0.5, None)
            } else if let Ok(s_num) = target_name.parse::<u32>() {
                if let Some(src) = sources.iter().find(|s| s.slot == s_num) {
                    (src.x + src.width * 0.5, src.y + src.height * 0.5, None)
                } else {
                    (960.0, 540.0, None)
                }
            } else {
                (960.0, 540.0, None)
            }
        }
    };

    let kind = EffectKind::from_str(&type_str);
    let mut mgr = get_state().effects.write();

    match kind {
        EffectKind::Fluid => {
            let config = SphConfig::from_json_map(&json_map);
            let (sx, sy) = if config.stream_mode == "top" {
                (center_x, -20.0)
            } else {
                (start_x, start_y)
            };
            mgr.spawn_fluid(sx, sy, center_x, center_y, target_name, config)
        }
        EffectKind::Fire => {
            let config = FireConfig::from_json_map(&json_map);
            // The mask's center of mass (center_x/center_y), like every other effect
            let (fire_cx, fire_cy) = (center_x, center_y);
            let (sx, sy) = if config.stream_mode == "top" {
                (fire_cx, fire_cy + config.offset_y)
            } else {
                let (cw_r, ch_r) = crate::obs_source::get_obs_canvas_resolution();
                let canvas_w = cw_r.max(1) as f32;
                let canvas_h = ch_r.max(1) as f32;
                match json_map.get("fireSpawn").and_then(|v| v.as_str()).unwrap_or("tc") {
                    "tl" => (canvas_w * 0.05, -50.0),
                    "tr" => (canvas_w * 0.95, -50.0),
                    "ml" => (-50.0, canvas_h * 0.5),
                    "mr" => (canvas_w + 50.0, canvas_h * 0.5),
                    "tc" => (fire_cx, -50.0),
                    _ => (start_x, start_y),
                }
            };
            mgr.spawn_fire(sx, sy, fire_cx, fire_cy, target_name, config)
        }
        EffectKind::Lightning => {
            let config = LightningConfig::from_json_map(&json_map);
            // Ray-march to find first contact with alpha mask surface / perimeter
            let (tx, ty) = {
                let slots = get_state().target_slots.read();
                let masks = get_state().masks.read();
                let mask_opt = resolve_target_mask(&target_name, &slots, &masks).map(|(_, m)| m);
                find_ray_mask_intersection(start_x, start_y, center_x, center_y, mask_opt, hb_opt_cloned.as_ref())
            };
            mgr.spawn_lightning(start_x, start_y, tx, ty, target_name, config)
        }
    }
}

/// Total number of active particles across all fluid + fire engines.
/// Use this to pre-size the particle buffer before calling trust_effect_get_particles.
#[unsafe(no_mangle)]
pub extern "C" fn trust_effect_particle_count() -> u32 {
    get_state().effects.read().particle_count() as u32
}

/// Copy all particle data into caller-provided buffer.
/// Format: [x, y, size, r, g, b, alpha, vx, vy] × count  (9 f32 per particle).
/// Returns actual particle count written.
#[unsafe(no_mangle)]
pub extern "C" fn trust_effect_get_particles(
    out_buf: *mut f32,
    max_particles: u32,
) -> u32 {
    if out_buf.is_null() || max_particles == 0 {
        return 0;
    }
    let max = max_particles as usize;
    let out = unsafe { std::slice::from_raw_parts_mut(out_buf, max * 11) };
    let actual = get_state().effects.read().fill_particle_buf(out, max);
    actual as u32
}

/// Total number of active lightning segments.
#[unsafe(no_mangle)]
pub extern "C" fn trust_effect_segment_count() -> u32 {
    get_state().effects.read().segment_count() as u32
}

/// Copy all lightning segment data into caller-provided buffer.
/// Format: [x1, y1, x2, y2, r, g, b, alpha, width] × count  (9 f32 per segment).
/// Returns actual segment count written.
#[unsafe(no_mangle)]
pub extern "C" fn trust_effect_get_segments(
    out_buf: *mut f32,
    max_segments: u32,
) -> u32 {
    if out_buf.is_null() || max_segments == 0 {
        return 0;
    }
    let mut buf: Vec<f32> = Vec::with_capacity(max_segments as usize * 9);
    get_state().effects.read().fill_segment_buf(&mut buf);
    let actual = (buf.len() / 9).min(max_segments as usize);
    let copy_floats = actual * 9;
    let out = unsafe { std::slice::from_raw_parts_mut(out_buf, copy_floats) };
    out.copy_from_slice(&buf[..copy_floats]);
    actual as u32
}

/// Total number of active lightning strike impacts.
#[unsafe(no_mangle)]
pub extern "C" fn trust_effect_lightning_impact_count() -> u32 {
    get_state().effects.read().lightning_impact_count() as u32
}

/// Copy all lightning impact flash data into caller-provided buffer.
/// Format: [tx, ty, r, g, b, alpha, radius, has_mask, xf_pos_x, xf_pos_y, xf_scale_x, xf_scale_y, xf_rot_deg, mask_w, mask_h, crop_l, crop_t] × count (17 f32 per impact).
/// Returns actual impact count written.
#[unsafe(no_mangle)]
pub extern "C" fn trust_effect_get_lightning_impacts(
    out_buf: *mut f32,
    max_impacts: u32,
) -> u32 {
    if out_buf.is_null() || max_impacts == 0 {
        return 0;
    }
    let slots = get_state().target_slots.read();
    let masks = get_state().masks.read();
    let mut buf: Vec<f32> = Vec::with_capacity(max_impacts as usize * 17);
    get_state().effects.read().fill_lightning_impact_buf(&mut buf, &*slots, &*masks);
    let actual = (buf.len() / 17).min(max_impacts as usize);
    let copy_floats = actual * 17;
    let out = unsafe { std::slice::from_raw_parts_mut(out_buf, copy_floats) };
    out.copy_from_slice(&buf[..copy_floats]);
    actual as u32
}

/// Drain and retrieve all active fluid impact events.
/// Format: [x, y, kind, intensity] × count (4 f32 per impact).
/// kind: 1 = Head/body impact crown burst, 2 = Ground settling ripple.
/// Returns number of impacts written.
#[unsafe(no_mangle)]
pub extern "C" fn trust_effect_get_impacts(
    out_buf: *mut f32,
    max_impacts: u32,
) -> u32 {
    if out_buf.is_null() || max_impacts == 0 {
        return 0;
    }
    let mut mgr = get_state().effects.write();
    let impacts = mgr.drain_impacts();
    let count = impacts.len().min(max_impacts as usize);
    for (i, imp) in impacts.iter().take(count).enumerate() {
        let base = i * 4;
        unsafe {
            *out_buf.add(base + 0) = imp.x;
            *out_buf.add(base + 1) = imp.y;
            *out_buf.add(base + 2) = imp.kind as f32;
            *out_buf.add(base + 3) = imp.intensity;
        }
    }
    count as u32
}

// ============================================================================
// 🎬 Studio Scene & Render Feed Engine
// ============================================================================

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StudioSource {
    #[serde(default)]
    pub scene: String,
    pub name: String,
    pub slot: u32,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub roll: f32,
    pub pitch: f32,
    pub yaw: f32,
    pub is_target: bool,
    pub match_obs: bool,
    pub is_visible: bool,
    pub file_path: String,
    pub default_x: f32,
    pub default_y: f32,
    pub default_width: f32,
    pub default_height: f32,
    pub default_roll: f32,
}

#[unsafe(no_mangle)]
pub extern "C" fn trust_studio_clear_sources() {
    get_state().studio_sources.write().clear();
}

#[unsafe(no_mangle)]
pub extern "C" fn trust_studio_add_source(
    name: *const c_char,
    slot: u32,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    roll: f32,
    is_target: bool,
) {
    trust_studio_add_source_full(name, slot, x, y, width, height, roll, 0.0, 0.0, is_target, true);
}

#[unsafe(no_mangle)]
pub extern "C" fn trust_studio_add_source_ex(
    name: *const c_char,
    slot: u32,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    roll: f32,
    is_target: bool,
    match_obs: bool,
) {
    trust_studio_add_source_full(name, slot, x, y, width, height, roll, 0.0, 0.0, is_target, match_obs);
}

#[unsafe(no_mangle)]
pub extern "C" fn trust_studio_add_source_full(
    name: *const c_char,
    slot: u32,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    roll: f32,
    pitch: f32,
    yaw: f32,
    is_target: bool,
    match_obs: bool,
) {
    trust_studio_add_source_v2(name, slot, x, y, width, height, roll, pitch, yaw, is_target, match_obs, true);
}

#[unsafe(no_mangle)]
pub extern "C" fn trust_studio_add_source_v2(
    name: *const c_char,
    slot: u32,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    roll: f32,
    pitch: f32,
    yaw: f32,
    is_target: bool,
    match_obs: bool,
    is_visible: bool,
) {
    if name.is_null() { return; }
    let s_name = unsafe { CStr::from_ptr(name).to_string_lossy().to_string() };
    let cur_scene = get_state().studio_scene_name.read().clone();
    let cur_scene = if cur_scene.is_empty() { "Scène".to_string() } else { cur_scene };
    let mut sources = get_state().studio_sources.write();
    if let Some(existing) = sources.iter_mut().find(|s| s.name == s_name && (s.scene.is_empty() || s.scene == cur_scene)) {
        existing.scene = cur_scene;
        existing.slot = slot;
        existing.x = x;
        existing.y = y;
        existing.width = width;
        existing.height = height;
        existing.roll = roll;
        existing.pitch = pitch;
        existing.yaw = yaw;
        existing.is_target = is_target;
        existing.match_obs = match_obs;
        existing.is_visible = is_visible;
    } else {
        sources.push(StudioSource {
            scene: cur_scene,
            name: s_name,
            slot,
            x,
            y,
            width,
            height,
            roll,
            pitch,
            yaw,
            is_target,
            match_obs,
            is_visible,
            file_path: String::new(),
            default_x: x,
            default_y: y,
            default_width: width,
            default_height: height,
            default_roll: roll,
        });
    }
}


#[unsafe(no_mangle)]
pub extern "C" fn trust_studio_is_source_visible(name: *const c_char) -> bool {
    if name.is_null() { return true; }
    let s_name = unsafe { CStr::from_ptr(name).to_string_lossy().to_string() };
    let sources = get_state().studio_sources.read();
    if let Some(src) = sources.iter().find(|s| s.name == s_name) {
        src.is_visible
    } else {
        true
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn trust_studio_set_source_visibility(name: *const c_char, visible: bool) -> bool {
    if name.is_null() { return false; }
    let s_name = unsafe { CStr::from_ptr(name).to_string_lossy().to_string() };
    let mut sources = get_state().studio_sources.write();
    if let Some(src) = sources.iter_mut().find(|s| s.name == s_name) {
        src.is_visible = visible;
        true
    } else {
        false
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn trust_studio_set_source_custom(
    name: *const c_char,
    match_obs: bool,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    roll: f32,
) -> bool {
    trust_studio_set_source_custom_full(name, match_obs, x, y, width, height, roll, 0.0, 0.0)
}

#[unsafe(no_mangle)]
pub extern "C" fn trust_studio_set_source_custom_full(
    name: *const c_char,
    match_obs: bool,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    roll: f32,
    pitch: f32,
    yaw: f32,
) -> bool {
    if name.is_null() { return false; }
    let s_name = unsafe { CStr::from_ptr(name).to_string_lossy().to_string() };
    let state = get_state();
    let mut sources = state.studio_sources.write();
    if let Some(src) = sources.iter_mut().find(|s| s.name == s_name) {
        src.match_obs = match_obs;
        src.x = x;
        src.y = y;
        src.width = width;
        src.height = height;
        src.roll = roll;
        src.pitch = pitch;
        src.yaw = yaw;

        let mut hitboxes = state.hitboxes.write();
        let mut masks = state.masks.write();
        if let Some(mask) = masks.get_mut(&s_name) {
            mask.transform.pos_x = x;
            mask.transform.pos_y = y;
            mask.transform.rot_deg = roll;
            mask.pitch = pitch;
            mask.yaw = yaw;
            if mask.width > 0 && mask.height > 0 {
                mask.transform.scale_x = width / mask.width as f32;
                mask.transform.scale_y = height / mask.height as f32;
            }
            if let Some(hb) = hitboxes.get_mut(&s_name) {
                // Keeps the target point on the mask's center of mass, not the middle of the source
                crate::tracker::reproject_hitbox(hb, mask);
            }
        } else if let Some(hb) = hitboxes.get_mut(&s_name) {
            // No mask scanned yet, so no center of mass to follow: track the source rect
            hb.world_x = x;
            hb.world_y = y;
            hb.world_width = width;
            hb.world_height = height;
            hb.world_center_x = x + width / 2.0;
            hb.world_center_y = y + height / 2.0;
        }
        drop(masks);
        let src_copy = src.clone();
        crate::studio_app::update_last_sent_pose_for_source(&s_name, x, y, width, height, roll, pitch, yaw, src.is_visible);
        drop(sources);
        crate::queue_studio_transform_update(&src_copy);
        true
    } else {
        false
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn trust_studio_get_source_custom(
    name: *const c_char,
    out_x: *mut f32,
    out_y: *mut f32,
    out_w: *mut f32,
    out_h: *mut f32,
    out_roll: *mut f32,
    out_match_obs: *mut bool,
) -> bool {
    let mut dummy_pitch = 0.0f32;
    let mut dummy_yaw = 0.0f32;
    trust_studio_get_source_custom_full(name, out_x, out_y, out_w, out_h, out_roll, &mut dummy_pitch, &mut dummy_yaw, out_match_obs)
}

#[unsafe(no_mangle)]
pub extern "C" fn trust_studio_get_source_custom_full(
    name: *const c_char,
    out_x: *mut f32,
    out_y: *mut f32,
    out_w: *mut f32,
    out_h: *mut f32,
    out_roll: *mut f32,
    out_pitch: *mut f32,
    out_yaw: *mut f32,
    out_match_obs: *mut bool,
) -> bool {
    if name.is_null() { return false; }
    let s_name = unsafe { CStr::from_ptr(name).to_string_lossy().to_string() };
    let state = get_state();
    let sources = state.studio_sources.read();
    if let Some(src) = sources.iter().find(|s| s.name == s_name) {
        if !out_x.is_null() { unsafe { *out_x = src.x; } }
        if !out_y.is_null() { unsafe { *out_y = src.y; } }
        if !out_w.is_null() { unsafe { *out_w = src.width; } }
        if !out_h.is_null() { unsafe { *out_h = src.height; } }
        if !out_roll.is_null() { unsafe { *out_roll = src.roll; } }
        if !out_pitch.is_null() { unsafe { *out_pitch = src.pitch; } }
        if !out_yaw.is_null() { unsafe { *out_yaw = src.yaw; } }
        if !out_match_obs.is_null() { unsafe { *out_match_obs = src.match_obs; } }
        true
    } else {
        false
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn trust_studio_get_selected_source(
    out_name: *mut c_char,
    max_len: usize,
) -> bool {
    if out_name.is_null() || max_len == 0 { return false; }
    let sel = get_state().studio_selected_source.read().clone();
    if sel.is_empty() { return false; }
    let bytes = sel.as_bytes();
    let copy_len = bytes.len().min(max_len - 1);
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), out_name as *mut u8, copy_len);
        *out_name.add(copy_len) = 0;
    }
    true
}

pub type StudioTransformCb = unsafe extern "C" fn(
    name: *const c_char,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    roll: f32,
    pitch: f32,
    yaw: f32,
    match_obs: bool,
);

static STUDIO_TRANSFORM_CB: parking_lot::RwLock<Option<StudioTransformCb>> = parking_lot::RwLock::new(None);

#[unsafe(no_mangle)]
pub extern "C" fn trust_studio_set_transform_callback(cb: StudioTransformCb) {
    *STUDIO_TRANSFORM_CB.write() = Some(cb);
    println!("[Trust Studio] 🔗 Connected Studio Transform Callback to Lua!");
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct StudioTransformUpdate {
    pub scene: [c_char; 64],
    pub name: [c_char; 64],
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub roll: f32,
    pub pitch: f32,
    pub yaw: f32,
    pub match_obs: bool,
    pub is_visible: bool,
}

pub static PENDING_STUDIO_TRANSFORMS: std::sync::LazyLock<parking_lot::Mutex<std::collections::HashMap<String, StudioTransformUpdate>>> =
    std::sync::LazyLock::new(|| parking_lot::Mutex::new(std::collections::HashMap::new()));

pub fn queue_studio_transform_update(src: &StudioSource) {
    let mut pending = PENDING_STUDIO_TRANSFORMS.lock();
    let mut c_scene = [0 as c_char; 64];
    let sc_bytes = src.scene.as_bytes();
    let sc_len = sc_bytes.len().min(63);
    for i in 0..sc_len {
        c_scene[i] = sc_bytes[i] as c_char;
    }

    let mut c_name = [0 as c_char; 64];
    let bytes = src.name.as_bytes();
    let copy_len = bytes.len().min(63);
    for i in 0..copy_len {
        c_name[i] = bytes[i] as c_char;
    }
    let key = if src.scene.is_empty() {
        src.name.clone()
    } else {
        format!("{}:{}", src.scene, src.name)
    };
    pending.insert(key, StudioTransformUpdate {
        scene: c_scene,
        name: c_name,
        x: src.x,
        y: src.y,
        width: src.width,
        height: src.height,
        roll: src.roll,
        pitch: src.pitch,
        yaw: src.yaw,
        match_obs: src.match_obs,
        is_visible: src.is_visible,
    });
    drop(pending);
    crate::obs_source::process_pending_studio_transforms();
}

#[unsafe(no_mangle)]
pub extern "C" fn trust_studio_poll_transform(out: *mut StudioTransformUpdate) -> bool {
    if out.is_null() {
        return false;
    }
    let mut pending = PENDING_STUDIO_TRANSFORMS.lock();
    if let Some(key) = pending.keys().next().cloned() {
        if let Some(update) = pending.remove(&key) {
            unsafe {
                *out = update;
            }
            return true;
        }
    }
    false
}

#[unsafe(no_mangle)]
pub extern "C" fn trust_studio_queue_transform(
    name: *const c_char,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    roll: f32,
    pitch: f32,
    yaw: f32,
    match_obs: bool,
    is_visible: bool,
) {
    if name.is_null() { return; }
    let s_name = unsafe { CStr::from_ptr(name).to_string_lossy().to_string() };
    let cur_scene = get_state().studio_scene_name.read().clone();
    let cur_scene = if cur_scene.is_empty() { "Scène".to_string() } else { cur_scene };

    let mut c_scene = [0 as c_char; 64];
    let sc_bytes = cur_scene.as_bytes();
    let sc_len = sc_bytes.len().min(63);
    for i in 0..sc_len {
        c_scene[i] = sc_bytes[i] as c_char;
    }

    let mut pending = PENDING_STUDIO_TRANSFORMS.lock();
    let mut c_name = [0 as c_char; 64];
    let bytes = s_name.as_bytes();
    let copy_len = bytes.len().min(63);
    for i in 0..copy_len {
        c_name[i] = bytes[i] as c_char;
    }
    let key = format!("{}:{}", cur_scene, s_name);
    pending.insert(key, StudioTransformUpdate {
        scene: c_scene,
        name: c_name,
        x,
        y,
        width,
        height,
        roll,
        pitch,
        yaw,
        match_obs,
        is_visible,
    });
}

fn draw_line_clipped(
    buf: &mut [u8],
    canvas_w: u32,
    canvas_h: u32,
    mut x0: i32,
    mut y0: i32,
    x1: i32,
    y1: i32,
    color: [u8; 3],
    off_x: u32,
    off_y: u32,
    draw_w: u32,
    draw_h: u32,
) {
    let dx = (x1 - x0).abs();
    let dy = -(y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    let min_x = off_x as i32;
    let max_x = (off_x + draw_w) as i32;
    let min_y = off_y as i32;
    let max_y = (off_y + draw_h) as i32;
    let req_len = buf.len();

    loop {
        if x0 >= min_x && x0 < max_x && y0 >= min_y && y0 < max_y && (x0 as u32) < canvas_w && (y0 as u32) < canvas_h {
            let idx = ((y0 as u32 * canvas_w + x0 as u32) * 4) as usize;
            if idx + 2 < req_len {
                buf[idx] = color[0];
                buf[idx + 1] = color[1];
                buf[idx + 2] = color[2];
            }
        }
        if x0 == x1 && y0 == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x0 += sx;
        }
        if e2 <= dx {
            err += dx;
            y0 += sy;
        }
    }
}

#[allow(dead_code)]
fn draw_handle_rect(
    buf: &mut [u8],
    canvas_w: u32,
    canvas_h: u32,
    cx: i32,
    cy: i32,
    size: i32,
    fill_color: [u8; 3],
    border_color: [u8; 3],
    off_x: u32,
    off_y: u32,
    draw_w: u32,
    draw_h: u32,
) {
    let hs = size / 2;
    let min_x = off_x as i32;
    let max_x = (off_x + draw_w) as i32;
    let min_y = off_y as i32;
    let max_y = (off_y + draw_h) as i32;
    let req_len = buf.len();

    for dy in -hs..=hs {
        for dx in -hs..=hs {
            let px = cx + dx;
            let py = cy + dy;
            if px >= min_x && px < max_x && py >= min_y && py < max_y && (px as u32) < canvas_w && (py as u32) < canvas_h {
                let idx = ((py as u32 * canvas_w + px as u32) * 4) as usize;
                if idx + 2 < req_len {
                    let is_border = dx == -hs || dx == hs || dy == -hs || dy == hs;
                    let c = if is_border { border_color } else { fill_color };
                    buf[idx] = c[0];
                    buf[idx + 1] = c[1];
                    buf[idx + 2] = c[2];
                }
            }
        }
    }
}

fn draw_circle_ring(
    buf: &mut [u8],
    canvas_w: u32,
    canvas_h: u32,
    cx: i32,
    cy: i32,
    radius: i32,
    thickness: i32,
    color: [u8; 3],
    off_x: u32,
    off_y: u32,
    draw_w: u32,
    draw_h: u32,
) {
    let half_th = thickness / 2;
    for r in (radius - half_th)..=(radius + half_th) {
        if r <= 0 { continue; }
        let mut x = r;
        let mut y = 0;
        let mut err = 0;
        let min_x = off_x as i32;
        let max_x = (off_x + draw_w) as i32;
        let min_y = off_y as i32;
        let max_y = (off_y + draw_h) as i32;
        let req_len = buf.len();

        let mut set_pix = |px: i32, py: i32| {
            if px >= min_x && px < max_x && py >= min_y && py < max_y && (px as u32) < canvas_w && (py as u32) < canvas_h {
                let idx = ((py as u32 * canvas_w + px as u32) * 4) as usize;
                if idx + 2 < req_len {
                    buf[idx] = color[0];
                    buf[idx + 1] = color[1];
                    buf[idx + 2] = color[2];
                }
            }
        };

        while x >= y {
            set_pix(cx + x, cy + y);
            set_pix(cx + y, cy + x);
            set_pix(cx - y, cy + x);
            set_pix(cx - x, cy + y);
            set_pix(cx - x, cy - y);
            set_pix(cx - y, cy - x);
            set_pix(cx + y, cy - x);
            set_pix(cx + x, cy - y);

            if err <= 0 {
                y += 1;
                err += 2 * y + 1;
            }
            if err > 0 {
                x -= 1;
                err -= 2 * x + 1;
            }
        }
    }
}

fn draw_filled_circle(
    buf: &mut [u8],
    canvas_w: u32,
    canvas_h: u32,
    cx: i32,
    cy: i32,
    radius: i32,
    fill_color: [u8; 3],
    border_color: [u8; 3],
    off_x: u32,
    off_y: u32,
    draw_w: u32,
    draw_h: u32,
) {
    let min_x = off_x as i32;
    let max_x = (off_x + draw_w) as i32;
    let min_y = off_y as i32;
    let max_y = (off_y + draw_h) as i32;
    let r2 = radius * radius;
    let req_len = buf.len();
    let border_r = (radius - 2).max(0);
    let border_r2 = border_r * border_r;

    for dy in -radius..=radius {
        for dx in -radius..=radius {
            let d2 = dx * dx + dy * dy;
            if d2 <= r2 {
                let px = cx + dx;
                let py = cy + dy;
                if px >= min_x && px < max_x && py >= min_y && py < max_y && (px as u32) < canvas_w && (py as u32) < canvas_h {
                    let idx = ((py as u32 * canvas_w + px as u32) * 4) as usize;
                    if idx + 2 < req_len {
                        let is_border = radius >= 5 && d2 >= border_r2;
                        let c = if is_border { border_color } else { fill_color };
                        buf[idx] = c[0];
                        buf[idx + 1] = c[1];
                        buf[idx + 2] = c[2];
                    }
                }
            }
        }
    }
}

fn draw_ellipse_ring(
    buf: &mut [u8],
    canvas_w: u32,
    canvas_h: u32,
    cx: i32,
    cy: i32,
    rx: f32,
    ry: f32,
    rot_rad: f32,
    thickness: i32,
    color: [u8; 3],
    off_x: u32,
    off_y: u32,
    draw_w: u32,
    draw_h: u32,
) {
    let cos_rot = rot_rad.cos();
    let sin_rot = rot_rad.sin();
    let steps = 64;
    let half_th = thickness / 2;

    for dr in -half_th..=half_th {
        let cur_rx = (rx + dr as f32).max(1.0);
        let cur_ry = (ry + dr as f32).max(1.0);
        let mut prev_pt: Option<(i32, i32)> = None;

        for i in 0..=steps {
            let theta = (i as f32 / steps as f32) * std::f32::consts::TAU;
            let lx = cur_rx * theta.cos();
            let ly = cur_ry * theta.sin();
            let px = cx as f32 + lx * cos_rot - ly * sin_rot;
            let py = cy as f32 + lx * sin_rot + ly * cos_rot;
            let pt = (px.round() as i32, py.round() as i32);
            if let Some(prev) = prev_pt {
                draw_line_clipped(buf, canvas_w, canvas_h, prev.0, prev.1, pt.0, pt.1, color, off_x, off_y, draw_w, draw_h);
            }
            prev_pt = Some(pt);
        }
    }
}

static MANUAL_CTRL: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static MANUAL_SHIFT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static MANUAL_ALT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[cfg(target_os = "linux")]
unsafe extern "C" {
    fn dlsym(handle: *mut std::ffi::c_void, symbol: *const std::ffi::c_char) -> *mut std::ffi::c_void;
}

#[cfg(target_os = "windows")]
unsafe extern "system" {
    fn GetAsyncKeyState(vKey: i32) -> i16;
}

#[unsafe(no_mangle)]
pub extern "C" fn trust_gui_set_modifiers(ctrl: bool, shift: bool, alt: bool) {
    MANUAL_CTRL.store(ctrl, std::sync::atomic::Ordering::Relaxed);
    MANUAL_SHIFT.store(shift, std::sync::atomic::Ordering::Relaxed);
    MANUAL_ALT.store(alt, std::sync::atomic::Ordering::Relaxed);
}

#[unsafe(no_mangle)]
pub extern "C" fn trust_gui_mouse_move_ex(x: f32, y: f32, ctrl: bool, shift: bool, alt: bool) {
    MANUAL_CTRL.store(ctrl, std::sync::atomic::Ordering::Relaxed);
    MANUAL_SHIFT.store(shift, std::sync::atomic::Ordering::Relaxed);
    MANUAL_ALT.store(alt, std::sync::atomic::Ordering::Relaxed);
    handle_studio_mouse_move(x, y);
}

#[unsafe(no_mangle)]
pub extern "C" fn trust_gui_mouse_button_ex(down: bool, ctrl: bool, shift: bool, alt: bool) {
    MANUAL_CTRL.store(ctrl, std::sync::atomic::Ordering::Relaxed);
    MANUAL_SHIFT.store(shift, std::sync::atomic::Ordering::Relaxed);
    MANUAL_ALT.store(alt, std::sync::atomic::Ordering::Relaxed);
    handle_studio_mouse_button(down);
}

pub fn get_keyboard_modifiers() -> (bool, bool, bool) {
    let m_ctrl = MANUAL_CTRL.load(std::sync::atomic::Ordering::Relaxed);
    let m_shift = MANUAL_SHIFT.load(std::sync::atomic::Ordering::Relaxed);
    let m_alt = MANUAL_ALT.load(std::sync::atomic::Ordering::Relaxed);
    if m_ctrl || m_shift || m_alt {
        return (m_ctrl, m_shift, m_alt);
    }

    #[cfg(target_os = "windows")]
    unsafe {
        // VK_CONTROL = 0x11, VK_SHIFT = 0x10, VK_MENU = 0x12
        let ctrl = (GetAsyncKeyState(0x11) as u16 & 0x8000) != 0;
        let shift = (GetAsyncKeyState(0x10) as u16 & 0x8000) != 0;
        let alt = (GetAsyncKeyState(0x12) as u16 & 0x8000) != 0;
        return (ctrl, shift, alt);
    }

    #[cfg(target_os = "linux")]
    {
        // Dynamic resolution of Qt's QGuiApplication::keyboardModifiers()
        static QT_MODS_FN: std::sync::OnceLock<Option<unsafe extern "C" fn() -> u32>> = std::sync::OnceLock::new();
        let q_fn = QT_MODS_FN.get_or_init(|| {
            unsafe {
                let sym = dlsym(std::ptr::null_mut(), b"_ZN15QGuiApplication17keyboardModifiersEv\0".as_ptr() as *const _);
                if !sym.is_null() {
                    Some(std::mem::transmute::<*mut std::ffi::c_void, unsafe extern "C" fn() -> u32>(sym))
                } else {
                    None
                }
            }
        });

        if let Some(f) = q_fn {
            let mods = unsafe { f() };
            let ctrl = (mods & 0x04000000) != 0;
            let shift = (mods & 0x02000000) != 0;
            let alt = (mods & 0x08000000) != 0;
            return (ctrl, shift, alt);
        }

        (false, false, false)
    }
}

fn handle_studio_mouse_button(down: bool) {
    let state = get_state();
    let (cw, ch, off_x, off_y, draw_w, draw_h) = *state.studio_viewport.read();
    if draw_w == 0 || draw_h == 0 || cw == 0 || ch == 0 {
        return;
    }
    let scale_x = draw_w as f32 / cw as f32;
    let scale_y = draw_h as f32 / ch as f32;

    let mut drag_state = state.studio_drag_state.write();
    let (mx, my) = *state.studio_pointer_pos.read();
    let (is_ctrl, _is_shift, _) = get_keyboard_modifiers();

    if down {
        let selected_name = state.studio_selected_source.read().clone();
        let sources = state.studio_sources.read().clone();

        let mut hit_mode = GizmoDragMode::None;
        let mut hit_target = None;

        // 1. First test Gizmo handles on currently selected source
        if !selected_name.is_empty() {
            if let Some(src) = sources.iter().find(|s| s.name == selected_name) {
                let sx = off_x as f32 + src.x * scale_x;
                let sy = off_y as f32 + src.y * scale_y;
                let sw = (src.width * scale_x).max(10.0);
                let sh = (src.height * scale_y).max(10.0);
                let scx = sx + sw / 2.0;
                let scy = sy + sh / 2.0;

                let dx = mx - scx;
                let dy = my - scy;
                let rad = (-src.roll).to_radians();
                let cos_a = rad.cos();
                let sin_a = rad.sin();
                let lx = dx * cos_a - dy * sin_a;
                let ly = dx * sin_a + dy * cos_a;
                let dist = (dx * dx + dy * dy).sqrt();

                // 1. White Center: radius <= 12px -> move freely on plane
                // ONLY the white spot in the middle can move the target normally!
                if dist <= 12.0 {
                    hit_mode = GizmoDragMode::PanFree;
                    hit_target = Some(src.clone());
                // 2. White Scale Ring: radius 48px (within 6px) -> uniform scale
                } else if (dist - 48.0).abs() <= 6.0 {
                    hit_mode = GizmoDragMode::Scale;
                    hit_target = Some(src.clone());
                // 3. Three Rotation Rings: exact Euclidean distance calculation
                } else {
                    let d_blue = (dist - 34.0).abs();

                    let red_grad = ((2.0 * lx / (34.0 * 34.0)).powi(2) + (2.0 * ly / (14.0 * 14.0)).powi(2) + 1e-6).sqrt();
                    let d_red = (((lx / 34.0).powi(2) + (ly / 14.0).powi(2) - 1.0).abs()) / red_grad;

                    let green_grad = ((2.0 * lx / (14.0 * 14.0)).powi(2) + (2.0 * ly / (34.0 * 34.0)).powi(2) + 1e-6).sqrt();
                    let d_green = (((lx / 14.0).powi(2) + (ly / 34.0).powi(2) - 1.0).abs()) / green_grad;

                    let min_rot_dist = d_blue.min(d_red).min(d_green);
                    if min_rot_dist <= 6.0 {
                        if (d_blue - min_rot_dist).abs() < 1e-3 {
                            hit_mode = GizmoDragMode::RotateRoll;
                        } else if (d_red - min_rot_dist).abs() < 1e-3 {
                            hit_mode = GizmoDragMode::RotateYaw;
                        } else {
                            hit_mode = GizmoDragMode::RotatePitch;
                        }
                        hit_target = Some(src.clone());
                    } else if is_ctrl && lx.abs() <= sw / 2.0 && ly.abs() <= sh / 2.0 {
                        // Blender Ctrl+Click: Ctrl + click anywhere inside target initiates movement!
                        hit_mode = GizmoDragMode::PanFree;
                        hit_target = Some(src.clone());
                    }
                }
            }
        }

        // 2. If not hitting current gizmo, test other sources to switch selection
        if hit_mode == GizmoDragMode::None {
            for src in sources.iter().rev() {
                if !src.is_visible { continue; }
                let sx = off_x as f32 + src.x * scale_x;
                let sy = off_y as f32 + src.y * scale_y;
                let sw = (src.width * scale_x).max(10.0);
                let sh = (src.height * scale_y).max(10.0);
                let scx = sx + sw / 2.0;
                let scy = sy + sh / 2.0;

                let dx = mx - scx;
                let dy = my - scy;
                let rad = (-src.roll).to_radians();
                let cos_a = rad.cos();
                let sin_a = rad.sin();
                let lx = dx * cos_a - dy * sin_a;
                let ly = dx * sin_a + dy * cos_a;

                if lx.abs() <= sw / 2.0 && ly.abs() <= sh / 2.0 {
                    *state.studio_selected_source.write() = src.name.clone();
                    if is_ctrl {
                        // Blender Ctrl+Click: Immediately activates movement on target
                        hit_mode = GizmoDragMode::PanFree;
                        hit_target = Some(src.clone());
                    } else {
                        // Normal click only selects the target
                        hit_mode = GizmoDragMode::None;
                    }
                    break;
                }
            }
        }

        if let Some(target) = hit_target {
            drag_state.mode = hit_mode;
            drag_state.target_name = target.name.clone();
            drag_state.start_mx = mx;
            drag_state.start_my = my;
            drag_state.start_x = target.x;
            drag_state.start_y = target.y;
            drag_state.start_w = target.width;
            drag_state.start_h = target.height;
            drag_state.start_roll = target.roll;
            drag_state.start_pitch = target.pitch;
            drag_state.start_yaw = target.yaw;
        } else {
            drag_state.mode = GizmoDragMode::None;
        }
    } else {
        // Button released
        if drag_state.mode != GizmoDragMode::None {
            drag_state.mode = GizmoDragMode::None;
            let target_name = drag_state.target_name.clone();
            let src_opt = state.studio_sources.read().iter().find(|s| s.name == target_name).cloned();
            if let Some(src) = src_opt {
                let cb_opt = *STUDIO_TRANSFORM_CB.read();
                if let Some(cb) = cb_opt {
                    let c_name = CString::new(src.name.as_str()).unwrap_or_default();
                    unsafe {
                        cb(
                            c_name.as_ptr(),
                            src.x,
                            src.y,
                            src.width,
                            src.height,
                            src.roll,
                            src.pitch,
                            src.yaw,
                            src.match_obs,
                        );
                    }
                }
            }
        }
    }
}

fn handle_studio_mouse_move(mx: f32, my: f32) {
    let state = get_state();
    *state.studio_pointer_pos.write() = (mx, my);

    let (cw, ch, off_x, off_y, draw_w, draw_h) = *state.studio_viewport.read();
    if draw_w == 0 || draw_h == 0 || cw == 0 || ch == 0 {
        return;
    }
    let scale_x = draw_w as f32 / cw as f32;
    let scale_y = draw_h as f32 / ch as f32;

    let drag_state = state.studio_drag_state.read().clone();
    if drag_state.mode == GizmoDragMode::None {
        return;
    }

    let (is_ctrl, is_shift, _) = get_keyboard_modifiers();

    // Blender Precision Mode (Shift): 1/10th speed
    let speed_mult = if is_shift { 0.1 } else { 1.0 };
    let delta_screen_x = (mx - drag_state.start_mx) * speed_mult;
    let delta_screen_y = (my - drag_state.start_my) * speed_mult;
    let delta_world_x = delta_screen_x / scale_x;
    let delta_world_y = delta_screen_y / scale_y;

    {
        let mut sources = state.studio_sources.write();
        if let Some(src) = sources.iter_mut().find(|s| s.name == drag_state.target_name) {
            let start_center_x = drag_state.start_x + drag_state.start_w / 2.0;
            let start_center_y = drag_state.start_y + drag_state.start_h / 2.0;
            let start_scx = off_x as f32 + start_center_x * scale_x;
            let start_scy = off_y as f32 + start_center_y * scale_y;

            let rad = (-drag_state.start_roll).to_radians();
            let cos_a = rad.cos();
            let sin_a = rad.sin();

            match drag_state.mode {
                GizmoDragMode::PanFree | GizmoDragMode::Move => {
                    let raw_x = drag_state.start_x + delta_world_x;
                    let raw_y = drag_state.start_y + delta_world_y;
                    if is_ctrl {
                        // Blender Snapping (Ctrl): 10px grid snap (or 1px with Shift)
                        let grid = if is_shift { 1.0 } else { 10.0 };
                        src.x = (raw_x / grid).round() * grid;
                        src.y = (raw_y / grid).round() * grid;
                    } else {
                        src.x = raw_x;
                        src.y = raw_y;
                    }
                }
                GizmoDragMode::Scale => {
                    // Uniform scaling via white scale ring: center pivot stays invariant
                    let d0 = (drag_state.start_mx - start_scx).hypot(drag_state.start_my - start_scy).max(10.0);
                    let d1 = (mx - start_scx).hypot(my - start_scy);
                    let raw_factor = (d1 / d0).clamp(0.05, 20.0);
                    let factor = 1.0 + (raw_factor - 1.0) * speed_mult;
                    let eff_factor = if is_ctrl {
                        // Blender Snapping (Ctrl): 0.05 scale snap (or 0.01 with Shift)
                        let step = if is_shift { 0.01 } else { 0.05 };
                        (factor / step).round() * step
                    } else {
                        factor
                    };
                    let new_w = (drag_state.start_w * eff_factor).max(10.0);
                    let new_h = (drag_state.start_h * eff_factor).max(10.0);
                    src.x = start_center_x - new_w / 2.0;
                    src.y = start_center_y - new_h / 2.0;
                    src.width = new_w;
                    src.height = new_h;
                }
                GizmoDragMode::RotateRoll | GizmoDragMode::Rotate => {
                    // Blue Ring: in-plane Z-axis rotation (Roll)
                    let start_angle = (drag_state.start_my - start_scy).atan2(drag_state.start_mx - start_scx).to_degrees();
                    let cur_angle = (my - start_scy).atan2(mx - start_scx).to_degrees();
                    let delta_angle = (cur_angle - start_angle) * speed_mult;
                    let raw_roll = ((drag_state.start_roll + delta_angle) % 360.0 + 360.0) % 360.0;
                    src.roll = if is_ctrl {
                        // Blender Snapping (Ctrl): 5.0 deg snap (or 1.0 deg with Shift)
                        let step = if is_shift { 1.0 } else { 5.0 };
                        ((raw_roll / step).round() * step) % 360.0
                    } else {
                        raw_roll
                    };
                }
                GizmoDragMode::RotatePitch => {
                    // Green Ring: Vertical (14x34) ellipse tilts Pitch
                    let start_dx = drag_state.start_mx - start_scx;
                    let start_dy = drag_state.start_my - start_scy;
                    let cur_dx = mx - start_scx;
                    let cur_dy = my - start_scy;

                    let s_lx = start_dx * cos_a - start_dy * sin_a;
                    let s_ly = start_dx * sin_a + start_dy * cos_a;
                    let c_lx = cur_dx * cos_a - cur_dy * sin_a;
                    let c_ly = cur_dx * sin_a + cur_dy * cos_a;

                    let phi_start = (s_ly / 34.0).atan2(s_lx / 14.0);
                    let phi_cur = (c_ly / 34.0).atan2(c_lx / 14.0);
                    let delta_deg = (phi_cur - phi_start).to_degrees() * speed_mult;
                    let raw_pitch = ((drag_state.start_pitch + delta_deg) % 360.0 + 360.0) % 360.0;
                    src.pitch = if is_ctrl {
                        let step = if is_shift { 1.0 } else { 5.0 };
                        ((raw_pitch / step).round() * step) % 360.0
                    } else {
                        raw_pitch
                    };
                }
                GizmoDragMode::RotateYaw => {
                    // Red Ring: Horizontal (34x14) ellipse turns Yaw
                    let start_dx = drag_state.start_mx - start_scx;
                    let start_dy = drag_state.start_my - start_scy;
                    let cur_dx = mx - start_scx;
                    let cur_dy = my - start_scy;

                    let s_lx = start_dx * cos_a - start_dy * sin_a;
                    let s_ly = start_dx * sin_a + start_dy * cos_a;
                    let c_lx = cur_dx * cos_a - cur_dy * sin_a;
                    let c_ly = cur_dx * sin_a + cur_dy * cos_a;

                    let phi_start = (s_ly / 14.0).atan2(s_lx / 34.0);
                    let phi_cur = (c_ly / 14.0).atan2(c_lx / 34.0);
                    let delta_deg = (phi_cur - phi_start).to_degrees() * speed_mult;
                    let raw_yaw = ((drag_state.start_yaw + delta_deg) % 360.0 + 360.0) % 360.0;
                    src.yaw = if is_ctrl {
                        let step = if is_shift { 1.0 } else { 5.0 };
                        ((raw_yaw / step).round() * step) % 360.0
                    } else {
                        raw_yaw
                    };
                }
                GizmoDragMode::None => {}
            }

            let cur_center_x = src.x + src.width / 2.0;
            let cur_center_y = src.y + src.height / 2.0;

            let mut hitboxes = state.hitboxes.write();

            // Immediately update Rust CollisionMask so silhouette mask follows 1:1 without losing it!
            let mut masks = state.masks.write();
            if let Some(mask) = masks.get_mut(&src.name) {
                mask.transform.pos_x = src.x;
                mask.transform.pos_y = src.y;
                mask.transform.rot_deg = src.roll;
                mask.pitch = src.pitch;
                mask.yaw = src.yaw;
                if mask.width > 0 && mask.height > 0 {
                    mask.transform.scale_x = src.width / mask.width as f32;
                    mask.transform.scale_y = src.height / mask.height as f32;
                }
                // Immediately update Rust hitbox so physics/collisions track 1:1 — re-derived
                // from the mask so the target point stays on its center of mass
                if let Some(hb) = hitboxes.get_mut(&src.name) {
                    crate::tracker::reproject_hitbox(hb, mask);
                }
            } else if let Some(hb) = hitboxes.get_mut(&src.name) {
                // No mask scanned yet, so no center of mass to follow: track the source rect
                hb.world_x = src.x;
                hb.world_y = src.y;
                hb.world_width = src.width;
                hb.world_height = src.height;
                hb.world_center_x = cur_center_x;
                hb.world_center_y = cur_center_y;
            }

        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn trust_is_preview_active() -> bool {
    crate::obs_source::is_preview_active()
}

#[unsafe(no_mangle)]
pub extern "C" fn trust_studio_open_native_window() {
    println!("[Trust Core] 🎬 Launching native egui Scene Studio window...");
    crate::studio_app::launch_studio_native();
}

#[unsafe(no_mangle)]
pub extern "C" fn trust_studio_open_editor_tab() {
    println!("[Trust Core] 🎨 Opening Studio Redeem & FX Editor tab...");
    crate::studio_app::switch_studio_tab(crate::studio_app::StudioActiveTab::RedeemEditor);
    crate::studio_app::launch_studio_native();
}

#[unsafe(no_mangle)]
pub extern "C" fn trust_is_studio_window_open() -> bool {
    crate::studio_app::is_studio_native_open()
}

#[unsafe(no_mangle)]
pub extern "C" fn trust_studio_load_targets_from_disk() -> bool {
    crate::studio_app::load_studio_targets_from_disk() > 0
}

pub static PENDING_STUDIO_SCENE_CHANGE: std::sync::LazyLock<parking_lot::Mutex<Option<String>>> =
    std::sync::LazyLock::new(|| parking_lot::Mutex::new(None));

pub fn queue_studio_scene_change(scene_name: &str) {
    *PENDING_STUDIO_SCENE_CHANGE.lock() = Some(scene_name.to_string());
}

pub fn take_studio_scene_change() -> Option<String> {
    PENDING_STUDIO_SCENE_CHANGE.lock().take()
}

#[unsafe(no_mangle)]
pub extern "C" fn trust_studio_sync_scene_sources(scene_name: *const c_char) {
    if scene_name.is_null() { return; }
    let s_name = unsafe { CStr::from_ptr(scene_name).to_string_lossy().to_string() };
    crate::obs_source::sync_studio_sources_for_scene(&s_name);
}

#[unsafe(no_mangle)]
pub extern "C" fn trust_studio_set_available_scenes(scenes_csv: *const c_char) {
    if scenes_csv.is_null() { return; }
    let s = unsafe { CStr::from_ptr(scenes_csv).to_string_lossy() };
    let list: Vec<String> = s.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect();
    *get_state().studio_available_scenes.write() = list;
}

#[unsafe(no_mangle)]
pub extern "C" fn trust_studio_poll_scene_change(out_name: *mut c_char, max_len: usize) -> bool {
    if out_name.is_null() || max_len == 0 { return false; }
    let mut pending = PENDING_STUDIO_SCENE_CHANGE.lock();
    if let Some(scene) = pending.take() {
        let bytes = scene.as_bytes();
        let len = bytes.len().min(max_len - 1);
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), out_name as *mut u8, len);
            *out_name.add(len) = 0;
        }
        return true;
    }
    false
}

#[unsafe(no_mangle)]
pub extern "C" fn trust_studio_set_scene(scene_name: *const c_char) {
    if scene_name.is_null() { return; }
    let s_name = unsafe { CStr::from_ptr(scene_name).to_string_lossy().to_string() };
    *get_state().studio_scene_name.write() = s_name;
}

#[unsafe(no_mangle)]
pub extern "C" fn trust_studio_get_scene_name(out_buf: *mut c_char, max_len: usize) -> bool {
    if out_buf.is_null() || max_len == 0 { return false; }
    let s_name = get_state().studio_scene_name.read().clone();
    if s_name.is_empty() { return false; }
    if let Ok(c_s) = std::ffi::CString::new(s_name) {
        let bytes = c_s.as_bytes_with_nul();
        if bytes.len() <= max_len {
            unsafe {
                std::ptr::copy_nonoverlapping(bytes.as_ptr(), out_buf as *mut u8, bytes.len());
            }
            return true;
        }
    }
    false
}

#[unsafe(no_mangle)]
pub extern "C" fn trust_studio_refresh_scenes() {
    crate::obs_source::refresh_obs_scenes();
}

#[unsafe(no_mangle)]
pub extern "C" fn trust_studio_set_selected_source(name: *const c_char) {
    let s_name = if !name.is_null() {
        unsafe { CStr::from_ptr(name).to_string_lossy().to_string() }
    } else {
        String::new()
    };
    *get_state().studio_selected_source.write() = s_name;
}

#[unsafe(no_mangle)]
pub extern "C" fn trust_studio_update_composite(
    rgba: *const u8,
    width: u32,
    height: u32,
    len: usize,
) -> bool {
    if rgba.is_null() || width == 0 || height == 0 || len == 0 {
        return false;
    }
    let slice = unsafe { std::slice::from_raw_parts(rgba, len) };
    let mut data = Vec::with_capacity(len);
    data.extend_from_slice(slice);
    let mut guard = get_state().studio_composite_frame.write();
    let next_id = guard.as_ref().map(|t| t.frame_id.wrapping_add(1)).unwrap_or(1);
    *guard = Some(crate::state::TargetPreviewTexture {
        width,
        height,
        rgba: data,
        frame_id: next_id,
        base_width: width,
        base_height: height,
    });
    true
}

#[unsafe(no_mangle)]
pub extern "C" fn trust_set_canvas_resolution(w: u32, h: u32) {
    crate::obs_source::set_canvas_resolution(w, h);
}

#[unsafe(no_mangle)]
pub extern "C" fn trust_get_canvas_resolution(out_w: *mut u32, out_h: *mut u32) -> bool {
    let (w, h) = crate::obs_source::get_obs_canvas_resolution();
    if !out_w.is_null() {
        unsafe { *out_w = w; }
    }
    if !out_h.is_null() {
        unsafe { *out_h = h; }
    }
    true
}

#[unsafe(no_mangle)]
pub extern "C" fn trust_studio_set_show_hitbox(show: bool) {
    get_state().studio_show_hitbox.store(show, std::sync::atomic::Ordering::Relaxed);
    println!("[Trust Studio] 🎯 Target Mask (Hitbox) overlay visibility set to: {}", show);
}

#[unsafe(no_mangle)]
pub extern "C" fn trust_studio_get_show_hitbox() -> bool {
    get_state().studio_show_hitbox.load(std::sync::atomic::Ordering::Relaxed)
}

#[unsafe(no_mangle)]
pub extern "C" fn trust_studio_render_preview(
    canvas_w: u32,
    canvas_h: u32,
    out_rgba: *mut u8,
    max_len: usize,
) -> u32 {
    let required_len = (canvas_w * canvas_h * 4) as usize;
    if out_rgba.is_null() || max_len < required_len || canvas_w == 0 || canvas_h == 0 {
        return 0;
    }

    crate::obs_source::notify_preview_active();

    let buf = unsafe { slice::from_raw_parts_mut(out_rgba, required_len) };
    
    // 1. Fill card background color (#1e293b) so outer margins blend seamlessly into the card
    for chunk in buf.chunks_exact_mut(4) {
        chunk[0] = 30;
        chunk[1] = 41;
        chunk[2] = 59;
        chunk[3] = 255;
    }

    // Determine canvas resolution and proportion
    let comp_guard = get_state().studio_composite_frame.read();
    let comp_ref = comp_guard.as_ref();
    let (cw, ch) = if let Some(comp) = comp_ref {
        if comp.width > 0 && comp.height > 0 && !comp.rgba.is_empty() {
            (comp.width, comp.height)
        } else {
            crate::obs_source::get_obs_canvas_resolution()
        }
    } else {
        crate::obs_source::get_obs_canvas_resolution()
    };

    let aspect_src = (cw as f32) / (ch.max(1) as f32);
    let aspect_dst = canvas_w as f32 / canvas_h as f32;

    let (draw_w, draw_h, off_x, off_y) = if aspect_dst > aspect_src {
        let dh = canvas_h;
        let dw = ((dh as f32) * aspect_src).round() as u32;
        let ox = (canvas_w.saturating_sub(dw)) / 2;
        (dw, dh, ox, 0)
    } else {
        let dw = canvas_w;
        let dh = ((dw as f32) / aspect_src).round() as u32;
        let oy = (canvas_h.saturating_sub(dh)) / 2;
        (dw, dh, 0, oy)
    };

    if draw_w < 2 || draw_h < 2 {
        return 1;
    }

    *get_state().studio_viewport.write() = (cw, ch, off_x, off_y, draw_w, draw_h);

    // 2. Fill the stream canvas tightly with pure black (#000000)
    for y in 0..draw_h {
        let dst_y = off_y + y;
        if dst_y >= canvas_h { break; }
        let row_start = ((dst_y * canvas_w + off_x) * 4) as usize;
        let row_len = (draw_w * 4) as usize;
        if row_start + row_len <= required_len {
            buf[row_start..row_start + row_len].fill(0);
            for chunk in buf[row_start..row_start + row_len].chunks_exact_mut(4) {
                chunk[3] = 255;
            }
        }
    }

    // 3. Render live composite frame if available
    let has_live_frame = if let Some(comp) = comp_ref {
        if comp.width > 0 && comp.height > 0 && !comp.rgba.is_empty() {
            // Blit live composite frame onto the stream canvas
            for y in 0..draw_h {
                let dst_y = off_y + y;
                if dst_y >= canvas_h { break; }
                let src_y = ((y as f32 / draw_h as f32) * comp.height as f32) as u32;
                if src_y >= comp.height { continue; }

                for x in 0..draw_w {
                    let dst_x = off_x + x;
                    if dst_x >= canvas_w { break; }
                    let src_x = ((x as f32 / draw_w as f32) * comp.width as f32) as u32;
                    if src_x >= comp.width { continue; }

                    let src_idx = ((src_y * comp.width + src_x) * 4) as usize;
                    let dst_idx = ((dst_y * canvas_w + dst_x) * 4) as usize;
                    if src_idx + 3 < comp.rgba.len() && dst_idx + 3 < required_len {
                        let a = comp.rgba[src_idx + 3] as u32;
                        if a == 255 {
                            buf[dst_idx] = comp.rgba[src_idx];
                            buf[dst_idx + 1] = comp.rgba[src_idx + 1];
                            buf[dst_idx + 2] = comp.rgba[src_idx + 2];
                        } else if a > 0 {
                            buf[dst_idx] = ((comp.rgba[src_idx] as u32 * a) / 255) as u8;
                            buf[dst_idx + 1] = ((comp.rgba[src_idx + 1] as u32 * a) / 255) as u8;
                            buf[dst_idx + 2] = ((comp.rgba[src_idx + 2] as u32 * a) / 255) as u8;
                        }
                        buf[dst_idx + 3] = 255;
                    }
                }
            }
            true
        } else {
            false
        }
    } else {
        false
    };

    // 4. Target Mask (Hitbox) & Fallback Wireframe Overlay
    let show_hitbox = get_state().studio_show_hitbox.load(std::sync::atomic::Ordering::Relaxed);

    if show_hitbox || !has_live_frame {
        let sources_guard = get_state().studio_sources.read();
        let sources = &*sources_guard;
        let selected_name_guard = get_state().studio_selected_source.read();
        let selected_name = &*selected_name_guard;
        let hitboxes_guard = get_state().hitboxes.read();
        let hitboxes = &*hitboxes_guard;
        let masks_guard = get_state().masks.read();
        let masks = &*masks_guard;

        // If in fallback mode without live video feed, render background non-target source textures
        if !has_live_frame {
            let textures = get_state().target_textures.read();
            for src in sources.iter() {
                if src.is_target { continue; }
                let sx = off_x as i32 + (src.x * (draw_w as f32 / cw as f32)) as i32;
                let sy = off_y as i32 + (src.y * (draw_h as f32 / ch as f32)) as i32;
                let sw = ((src.width * (draw_w as f32 / cw as f32)) as i32).max(10);
                let sh = ((src.height * (draw_h as f32 / ch as f32)) as i32).max(10);

                if let Some(tex) = textures.get(&src.name) {
                    if tex.width > 0 && tex.height > 0 && !tex.rgba.is_empty() {
                        for dy in 0..sh {
                            let py = sy + dy;
                            if py < off_y as i32 || py >= (off_y + draw_h) as i32 || (py as u32) >= canvas_h { continue; }
                            let src_v = (dy as f32 / sh as f32 * tex.height as f32) as u32;
                            if src_v >= tex.height { continue; }

                            for dx in 0..sw {
                                let px = sx + dx;
                                if px < off_x as i32 || px >= (off_x + draw_w) as i32 || (px as u32) >= canvas_w { continue; }
                                let src_u = (dx as f32 / sw as f32 * tex.width as f32) as u32;
                                if src_u >= tex.width { continue; }

                                let src_idx = ((src_v * tex.width + src_u) * 4) as usize;
                                if src_idx + 3 < tex.rgba.len() {
                                    let a = tex.rgba[src_idx + 3] as u32;
                                    if a > 0 {
                                        let dst_idx = ((py as u32 * canvas_w + px as u32) * 4) as usize;
                                        if dst_idx + 3 < required_len {
                                            let inv_a = 255 - a;
                                            buf[dst_idx] = ((tex.rgba[src_idx] as u32 * a + buf[dst_idx] as u32 * inv_a) / 255) as u8;
                                            buf[dst_idx + 1] = ((tex.rgba[src_idx + 1] as u32 * a + buf[dst_idx + 1] as u32 * inv_a) / 255) as u8;
                                            buf[dst_idx + 2] = ((tex.rgba[src_idx + 2] as u32 * a + buf[dst_idx + 2] as u32 * inv_a) / 255) as u8;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Draw Target Masks and Hitboxes
        let mut target_names = std::collections::BTreeSet::new();
        for src in sources.iter() {
            if src.is_target {
                target_names.insert(src.name.clone());
            }
        }
        if target_names.is_empty() {
            for (name, hb) in hitboxes.iter() {
                if hb.active && hb.world_width > 0.0 && hb.world_height > 0.0 {
                    target_names.insert(name.clone());
                }
            }
        }
        if !selected_name.is_empty() {
            target_names.insert(selected_name.clone());
        }

        for name in target_names {
            let is_selected = !selected_name.is_empty() && name == *selected_name;
            let (hx, hy, hw, hh, h_roll, h_pitch, h_yaw, is_match_obs) = if let Some(src) = sources.iter().find(|s| s.name == name) {
                (src.x, src.y, src.width, src.height, src.roll, src.pitch, src.yaw, src.match_obs)
            } else if let Some(hb) = hitboxes.get(&name) {
                (hb.world_x, hb.world_y, hb.world_width, hb.world_height, 0.0, 0.0, 0.0, true)
            } else {
                continue;
            };

            let sx = off_x as i32 + ((hx * (draw_w as f32 / cw as f32)).round() as i32);
            let sy = off_y as i32 + ((hy * (draw_h as f32 / ch as f32)).round() as i32);
            let sw = ((hw * (draw_w as f32 / cw as f32)).round() as i32).max(4);
            let sh = ((hh * (draw_h as f32 / ch as f32)).round() as i32).max(4);
            let scx = sx + sw / 2;
            let scy = sy + sh / 2;

            let (cr, cg, cb) = if is_selected {
                (245u8, 158u8, 11u8) // Amber #F59E0B
            } else {
                (56u8, 189u8, 248u8) // Neon Cyan #38BDF8
            };

            // 3D Transformed Target Corners (Orthographic Affine Projection matching OBS gs_ortho)
            let hw_s = sw as f32 / 2.0;
            let hh_s = sh as f32 / 2.0;

            let rx = h_pitch.to_radians();
            let ry = h_yaw.to_radians();
            let rz = h_roll.to_radians();

            let cos_rx = rx.cos();
            let sin_rx = rx.sin();
            let cos_ry = ry.cos();
            let sin_ry = ry.sin();
            let cos_rz = rz.cos();
            let sin_rz = rz.sin();

            let base_corners = [
                (-hw_s, -hh_s),
                ( hw_s, -hh_s),
                ( hw_s,  hh_s),
                (-hw_s,  hh_s),
            ];

            let corners: [(f32, f32); 4] = std::array::from_fn(|i| {
                let (bx, by) = base_corners[i];
                // 1. Ry(yaw)
                let x1 = bx * cos_ry;
                let y1 = by;
                let z1 = -bx * sin_ry;

                // 2. Rx(pitch)
                let x2 = x1;
                let y2 = y1 * cos_rx - z1 * sin_rx;
                let z2 = y1 * sin_rx + z1 * cos_rx;

                // 3. Rz(roll)
                let x3 = x2 * cos_rz - y2 * sin_rz;
                let y3 = x2 * sin_rz + y2 * cos_rz;
                let z3 = z2;

                let d = crate::tracker::PERSPECTIVE_D;
                let k = d / (d + z3).max(10.0);
                (scx as f32 + x3 * k, scy as f32 + y3 * k)
            });

            let (pad_rx, pad_ry, is_reacted) = {
                let textures = get_state().target_textures.read();
                textures.get(&name).map(|t| (t.pad_ratio_x(), t.pad_ratio_y(), t.is_padded_reaction())).unwrap_or((1.0, 1.0, false))
            };

            // If target is in Custom Position (!is_match_obs), blit its image at custom position if available
            if !is_match_obs {
                let textures = get_state().target_textures.read();
                if let Some(tex) = textures.get(&name) {
                    if tex.width > 0 && tex.height > 0 && !tex.rgba.is_empty() {
                        let (b_rx, b_ry, b_rz) = if is_reacted {
                            (0.0f32, 0.0f32, 0.0f32)
                        } else {
                            (rx, ry, rz)
                        };
                        let (cos_b_rx, sin_b_rx) = (b_rx.cos(), b_rx.sin());
                        let (cos_b_ry, sin_b_ry) = (b_ry.cos(), b_ry.sin());
                        let (cos_b_rz, sin_b_rz) = (b_rz.cos(), b_rz.sin());

                        let ux = cos_b_ry * cos_b_rz - sin_b_ry * sin_b_rx * sin_b_rz;
                        let uy = cos_b_ry * sin_b_rz + sin_b_ry * sin_b_rx * cos_b_rz;
                        let uz = -sin_b_ry * cos_b_rx;

                        let vx = -cos_b_rx * sin_b_rz;
                        let vy = cos_b_rx * cos_b_rz;
                        let vz = sin_b_rx;

                        let eff_sw = sw as f32 * pad_rx;
                        let eff_sh = sh as f32 * pad_ry;
                        let d = crate::tracker::PERSPECTIVE_D;
                        let max_extent = (((eff_sw * eff_sw + eff_sh * eff_sh) as f32).sqrt().ceil() as i32) / 2 + 16;

                        for dy in -max_extent..=max_extent {
                            let py = scy + dy;
                            if py < off_y as i32 || py >= (off_y + draw_h) as i32 || (py as u32) >= canvas_h { continue; }
                            let fdy = dy as f32;
                            for dx in -max_extent..=max_extent {
                                let px = scx + dx;
                                if px < off_x as i32 || px >= (off_x + draw_w) as i32 || (px as u32) >= canvas_w { continue; }
                                let fdx = dx as f32;

                                let a00 = ux - (fdx / d) * uz;
                                let a01 = vx - (fdx / d) * vz;
                                let a10 = uy - (fdy / d) * uz;
                                let a11 = vy - (fdy / d) * vz;

                                let det = a00 * a11 - a01 * a10;
                                if det.abs() < 1e-6 { continue; }

                                let unrot_x = (fdx * a11 - fdy * a01) / det;
                                let unrot_y = (a00 * fdy - a10 * fdx) / det;

                                let norm_u = unrot_x / eff_sw + 0.5;
                                let norm_v = unrot_y / eff_sh + 0.5;

                                if norm_u >= 0.0 && norm_u < 1.0 && norm_v >= 0.0 && norm_v < 1.0 {
                                    let src_u = (norm_u * tex.width as f32) as u32;
                                    let src_v = (norm_v * tex.height as f32) as u32;
                                    if src_u < tex.width && src_v < tex.height {
                                        let src_idx = ((src_v * tex.width + src_u) * 4) as usize;
                                        if src_idx + 3 < tex.rgba.len() {
                                            let a = tex.rgba[src_idx + 3] as u32;
                                            if a > 0 {
                                                let dst_idx = ((py as u32 * canvas_w + px as u32) * 4) as usize;
                                                if dst_idx + 3 < required_len {
                                                    let inv_a = 255 - a;
                                                    buf[dst_idx] = ((tex.rgba[src_idx] as u32 * a + buf[dst_idx] as u32 * inv_a) / 255) as u8;
                                                    buf[dst_idx + 1] = ((tex.rgba[src_idx + 1] as u32 * a + buf[dst_idx + 1] as u32 * inv_a) / 255) as u8;
                                                    buf[dst_idx + 2] = ((tex.rgba[src_idx + 2] as u32 * a + buf[dst_idx + 2] as u32 * inv_a) / 255) as u8;
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // A. Solid Pixel CollisionMask Silhouette & Contour Highlight
            let (scan_x0, scan_x1, scan_y0, scan_y1) = {
                let mut min_x = corners[0].0;
                let mut max_x = corners[0].0;
                let mut min_y = corners[0].1;
                let mut max_y = corners[0].1;
                for c in &corners[1..] {
                    if c.0 < min_x { min_x = c.0; }
                    if c.0 > max_x { max_x = c.0; }
                    if c.1 < min_y { min_y = c.1; }
                    if c.1 > max_y { max_y = c.1; }
                }
                if is_reacted {
                    let expand_x = (max_x - min_x) * (pad_rx - 1.0) * 0.5;
                    let expand_y = (max_y - min_y) * (pad_ry - 1.0) * 0.5;
                    min_x -= expand_x;
                    max_x += expand_x;
                    min_y -= expand_y;
                    max_y += expand_y;
                }
                (
                    (min_x.floor() as i32).clamp(off_x as i32, (off_x + draw_w) as i32),
                    (max_x.ceil() as i32).clamp(off_x as i32, (off_x + draw_w) as i32),
                    (min_y.floor() as i32).clamp(off_y as i32, (off_y + draw_h) as i32),
                    (max_y.ceil() as i32).clamp(off_y as i32, (off_y + draw_h) as i32),
                )
            };

            let scan_w = (scan_x1 - scan_x0).max(0) as usize;
            let scan_h = (scan_y1 - scan_y0).max(0) as usize;

            if scan_w > 0 && scan_h > 0 {
                let mut solid_grid = vec![false; scan_w * scan_h];
                let mut has_any_solid = false;
                for dy in 0..scan_h {
                    let py = scan_y0 + dy as i32;
                    if py < off_y as i32 || py >= (off_y + draw_h) as i32 || (py as u32) >= canvas_h { continue; }
                    let wy = (py - off_y as i32) as f32 * (ch as f32 / draw_h as f32);
                    for dx in 0..scan_w {
                        let px = scan_x0 + dx as i32;
                        if px < off_x as i32 || px >= (off_x + draw_w) as i32 || (px as u32) >= canvas_w { continue; }
                        let wx = (px - off_x as i32) as f32 * (cw as f32 / draw_w as f32);

                        // Exact CollisionMask query (evaluates live local alpha, chroma-key, crop, and 3D Euler perspective)
                        let is_solid = if let Some(mask) = masks.get(&name) {
                            mask.is_solid_world(wx, wy)
                        } else {
                            false
                        };

                        if is_solid {
                            solid_grid[dy * scan_w + dx] = true;
                            has_any_solid = true;
                        }
                    }
                }

                if has_any_solid {
                    for dy in 0..scan_h {
                        let py = scan_y0 + dy as i32;
                        if py < off_y as i32 || py >= (off_y + draw_h) as i32 || (py as u32) >= canvas_h { continue; }
                        for dx in 0..scan_w {
                            let px = scan_x0 + dx as i32;
                            if px < off_x as i32 || px >= (off_x + draw_w) as i32 || (px as u32) >= canvas_w { continue; }
                            let g_idx = dy * scan_w + dx;
                            if solid_grid[g_idx] {
                                let dst_idx = ((py as u32 * canvas_w + px as u32) * 4) as usize;
                                if dst_idx + 3 < required_len {
                                    let is_edge = dx == 0 || dx == scan_w - 1 || dy == 0 || dy == scan_h - 1
                                        || !solid_grid[dy * scan_w + (dx - 1)]
                                        || !solid_grid[dy * scan_w + (dx + 1)]
                                        || !solid_grid[(dy - 1) * scan_w + dx]
                                        || !solid_grid[(dy + 1) * scan_w + dx];

                                    if is_edge {
                                        let (er, eg, eb) = if is_selected { (255u8, 240u8, 150u8) } else { (180u8, 245u8, 255u8) };
                                        buf[dst_idx] = er;
                                        buf[dst_idx + 1] = eg;
                                        buf[dst_idx + 2] = eb;
                                    } else {
                                        let alpha = 135u32; // ~53% opacity
                                        let inv_a = 255 - alpha;
                                        buf[dst_idx] = ((cr as u32 * alpha + buf[dst_idx] as u32 * inv_a) / 255) as u8;
                                        buf[dst_idx + 1] = ((cg as u32 * alpha + buf[dst_idx + 1] as u32 * inv_a) / 255) as u8;
                                        buf[dst_idx + 2] = ((cb as u32 * alpha + buf[dst_idx + 2] as u32 * inv_a) / 255) as u8;
                                    }
                                }
                            }
                        }
                    }
                }
            }


            let frame_color = if is_selected {
                [245u8, 158u8, 11u8] // Amber #F59E0B
            } else {
                [56u8, 189u8, 248u8] // Neon Cyan #38BDF8
            };

            for i in 0..4 {
                let (p0, p1) = (corners[i], corners[(i + 1) % 4]);
                let x0 = p0.0.round() as i32;
                let y0 = p0.1.round() as i32;
                let x1 = p1.0.round() as i32;
                let y1 = p1.1.round() as i32;
                draw_line_clipped(buf, canvas_w, canvas_h, x0, y0, x1, y1, frame_color, off_x, off_y, draw_w, draw_h);
                if is_selected {
                    draw_line_clipped(buf, canvas_w, canvas_h, x0 + 1, y0, x1 + 1, y1, frame_color, off_x, off_y, draw_w, draw_h);
                }
            }

            if is_selected {
                for i in 0..4 {
                    let p = corners[i];
                    draw_handle_rect(buf, canvas_w, canvas_h, p.0.round() as i32, p.1.round() as i32, 6, [255, 255, 255], [245, 158, 11], off_x, off_y, draw_w, draw_h);

                    let p_next = corners[(i + 1) % 4];
                    let mx = ((p.0 + p_next.0) / 2.0).round() as i32;
                    let my = ((p.1 + p_next.1) / 2.0).round() as i32;
                    draw_handle_rect(buf, canvas_w, canvas_h, mx, my, 4, [255, 255, 255], [245, 158, 11], off_x, off_y, draw_w, draw_h);
                }
            }

            // C. Transform Gizmo: 3 Rotation Rings, White Scale Ring, White Center (NO ARROWS)
            if is_selected {
                // 1. White Ring for Scale (radius 48px, thickness 3, crisp white #FFFFFF)
                draw_circle_ring(buf, canvas_w, canvas_h, scx, scy, 48, 3, [255, 255, 255], off_x, off_y, draw_w, draw_h);
                // 4 scale tick marks at 0, 90, 180, 270 degrees
                draw_filled_circle(buf, canvas_w, canvas_h, scx + 48, scy, 3, [255, 255, 255], [30, 41, 59], off_x, off_y, draw_w, draw_h);
                draw_filled_circle(buf, canvas_w, canvas_h, scx - 48, scy, 3, [255, 255, 255], [30, 41, 59], off_x, off_y, draw_w, draw_h);
                draw_filled_circle(buf, canvas_w, canvas_h, scx, scy + 48, 3, [255, 255, 255], [30, 41, 59], off_x, off_y, draw_w, draw_h);
                draw_filled_circle(buf, canvas_w, canvas_h, scx, scy - 48, 3, [255, 255, 255], [30, 41, 59], off_x, off_y, draw_w, draw_h);

                // 2. Three Rotation Rings (radius 34px, thickness 3):
                // A. Blue Ring: In-plane view roll (#3B82F6)
                draw_circle_ring(buf, canvas_w, canvas_h, scx, scy, 34, 3, [59, 130, 246], off_x, off_y, draw_w, draw_h);

                // B. Red Ring: X-axis 3D perspective ellipse (#EF4444)
                draw_ellipse_ring(buf, canvas_w, canvas_h, scx, scy, 34.0, 14.0, rz, 3, [239, 68, 68], off_x, off_y, draw_w, draw_h);

                // C. Green Ring: Y-axis 3D perspective ellipse (#22C55E)
                draw_ellipse_ring(buf, canvas_w, canvas_h, scx, scy, 14.0, 34.0, rz, 3, [34, 197, 94], off_x, off_y, draw_w, draw_h);

                // 3. White Center for moving on the plane (radius 9px, white with 2px dark border)
                draw_filled_circle(buf, canvas_w, canvas_h, scx, scy, 9, [255, 255, 255], [30, 41, 59], off_x, off_y, draw_w, draw_h);
            } else {
                // Non-selected target: subtle 4px cyan center dot
                draw_filled_circle(buf, canvas_w, canvas_h, scx, scy, 4, [56, 189, 248], [255, 255, 255], off_x, off_y, draw_w, draw_h);
            }
        }
    }

    // 4. Draw crisp 1px stream frame border around the canvas (#334155)
    let border_color = [51u8, 65u8, 85u8];
    for x in 0..draw_w {
        let top_idx = ((off_y * canvas_w + off_x + x) * 4) as usize;
        let bot_y = (off_y + draw_h - 1).min(canvas_h - 1);
        let bot_idx = ((bot_y * canvas_w + off_x + x) * 4) as usize;
        if top_idx + 2 < required_len {
            buf[top_idx..top_idx + 3].copy_from_slice(&border_color);
        }
        if bot_idx + 2 < required_len {
            buf[bot_idx..bot_idx + 3].copy_from_slice(&border_color);
        }
    }
    for y in 0..draw_h {
        let left_idx = (((off_y + y) * canvas_w + off_x) * 4) as usize;
        let right_x = (off_x + draw_w - 1).min(canvas_w - 1);
        let right_idx = (((off_y + y) * canvas_w + right_x) * 4) as usize;
        if left_idx + 2 < required_len {
            buf[left_idx..left_idx + 3].copy_from_slice(&border_color);
        }
        if right_idx + 2 < required_len {
            buf[right_idx..right_idx + 3].copy_from_slice(&border_color);
        }
    }

    1
}

#[unsafe(no_mangle)]
pub extern "C" fn trust_gui_mouse_move(x: f32, y: f32) {
    handle_studio_mouse_move(x, y);
}

#[unsafe(no_mangle)]
pub extern "C" fn trust_gui_mouse_button(down: bool) {
    handle_studio_mouse_button(down);
}

/// Renders `text` into `out_rgba` as a bitmap-font texture — used by
/// `theater_chat.lua` to draw chat message textures (unrelated to egui: a
/// plain embedded 5x7 dot-matrix font, see font.rs).
#[unsafe(no_mangle)]
pub extern "C" fn trust_render_text(
    text: *const c_char,
    scale: u32,
    fg_rgba: u32,
    bg_rgba: u32,
    out_rgba: *mut u8,
    max_buf_len: usize,
    out_w: *mut u32,
    out_h: *mut u32,
) -> u32 {
    if text.is_null() || out_rgba.is_null() || out_w.is_null() || out_h.is_null() {
        return 0;
    }
    let text = unsafe { CStr::from_ptr(text).to_string_lossy().to_string() };
    let buf = unsafe { slice::from_raw_parts_mut(out_rgba, max_buf_len) };
    let (mut w, mut h) = (0u32, 0u32);
    let bytes = crate::font::render_text(&text, scale, fg_rgba, bg_rgba, buf, &mut w, &mut h);
    unsafe {
        *out_w = w;
        *out_h = h;
    }
    bytes as u32
}

/// Hardware text rasterizer with energized electric neon aura.
#[unsafe(no_mangle)]
pub extern "C" fn trust_render_text_electric(
    text: *const c_char,
    scale: u32,
    core_rgba: u32,
    glow_rgba: u32,
    glow_radius: u32,
    out_rgba: *mut u8,
    max_buf_len: usize,
    out_w: *mut u32,
    out_h: *mut u32,
) -> u32 {
    if text.is_null() || out_rgba.is_null() || out_w.is_null() || out_h.is_null() {
        return 0;
    }
    let text = unsafe { CStr::from_ptr(text).to_string_lossy().to_string() };
    let buf = unsafe { slice::from_raw_parts_mut(out_rgba, max_buf_len) };
    let (mut w, mut h) = (0u32, 0u32);
    let bytes = crate::font::render_text_electric(
        &text,
        scale,
        core_rgba,
        glow_rgba,
        glow_radius,
        buf,
        &mut w,
        &mut h,
    );
    unsafe {
        *out_w = w;
        *out_h = h;
    }
    bytes as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::TrackingConfig;

    #[test]
    fn test_update_target_transform_keeps_mask_center_of_mass() {
        let (px, w, h) = crate::tracker::tests::asymmetric_l_source();
        let name = CString::new("MassCenterTarget").unwrap();
        let start = SourceTransform { pos_x: 100.0, pos_y: 50.0, ..SourceTransform::default() };
        trust_process_rgba_transformed(name.as_ptr(), px.as_ptr(), w, h, w * 4, start);

        // Moving the target goes through the no-rescan fast path
        let moved = SourceTransform { pos_x: 700.0, pos_y: 250.0, scale_x: 1.5, scale_y: 1.5, rot_deg: 20.0, ..SourceTransform::default() };
        assert!(trust_update_target_transform(name.as_ptr(), moved));
        let hb = get_state().hitboxes.read()["MassCenterTarget"];

        let config = *get_state().config.read();
        let (fresh, _, _) = scan_rgba_with_transform(&px, w, h, w * 4, &moved, &config);
        assert!((hb.world_center_x - fresh.world_center_x).abs() < 0.01, "center x {} vs rescan {}", hb.world_center_x, fresh.world_center_x);
        assert!((hb.world_center_y - fresh.world_center_y).abs() < 0.01, "center y {} vs rescan {}", hb.world_center_y, fresh.world_center_y);
        assert!((hb.world_x - fresh.world_x).abs() < 0.01 && (hb.world_width - fresh.world_width).abs() < 0.01, "hitbox box drifted from the mask");

        // Previously reset to the middle of the source rect on every move
        let rect_mid_x = moved.pos_x + w as f32 * moved.scale_x / 2.0;
        assert!((hb.world_center_x - rect_mid_x).abs() > 50.0, "center x {} is the source middle {}", hb.world_center_x, rect_mid_x);
    }

    #[test]
    fn test_alpha_transparency_detection() {
        let width = 20;
        let height = 20;
        let stride = width * 4;
        let mut buffer = vec![0u8; (height * stride) as usize];

        // Draw an 8x8 opaque purple square at local (5, 5)
        for y in 5..13 {
            for x in 5..13 {
                let idx = ((y * stride) + (x * 4)) as usize;
                buffer[idx] = 180;    // R
                buffer[idx + 1] = 50; // G
                buffer[idx + 2] = 220;// B
                buffer[idx + 3] = 255;// Alpha = 255 (Opaque)
            }
        }

        let transform = SourceTransform {
            pos_x: 200.0,
            pos_y: 300.0,
            scale_x: 2.0,
            scale_y: 2.0,
            crop_left: 0,
            crop_top: 0,
            crop_right: 0,
            crop_bottom: 0,
            rot_deg: 0.0,
        };

        let config = TrackingConfig {
            mode: 0, // Alpha mode
            alpha_threshold: 10,
            precision: 10,
            ..Default::default()
        };

        let (hitbox, mask, _grid) = scan_rgba_with_transform(&buffer, width, height, stride, &transform, &config);

        assert!(hitbox.active);
        // Local space checks
        assert_eq!(hitbox.local_x, 5.0);
        assert_eq!(hitbox.local_y, 5.0);
        assert_eq!(hitbox.local_width, 8.0);
        assert_eq!(hitbox.local_height, 8.0);

        // World stream-space checks (pos + local * scale)
        // world_x = 200 + 5 * 2 = 210
        // world_y = 300 + 5 * 2 = 310
        // world_w = 8 * 2 = 16
        // world_h = 8 * 2 = 16
        assert_eq!(hitbox.world_x, 210.0);
        assert_eq!(hitbox.world_y, 310.0);
        assert_eq!(hitbox.world_width, 16.0);
        assert_eq!(hitbox.world_height, 16.0);

        // Point checks in world space
        assert!(mask.is_solid_world(212.0, 312.0));
        assert!(!mask.is_solid_world(100.0, 100.0));
    }

    #[test]
    fn test_scan_and_mask_with_3d_transform() {
        use crate::tracker::scan_rgba_with_3d_transform;

        let width = 20u32;
        let height = 20u32;
        let stride = width * 4;
        let mut buffer = vec![0u8; (stride * height) as usize];

        // Fill center 8x8 block with solid pixels
        for y in 6..14 {
            for x in 6..14 {
                let idx = ((y * stride) + (x * 4)) as usize;
                buffer[idx + 3] = 255;
            }
        }

        let transform = SourceTransform {
            pos_x: 500.0,
            pos_y: 400.0,
            scale_x: 1.0,
            scale_y: 1.0,
            crop_left: 0,
            crop_top: 0,
            crop_right: 0,
            crop_bottom: 0,
            rot_deg: 35.0,
        };

        let config = TrackingConfig {
            mode: 0,
            alpha_threshold: 10,
            ..Default::default()
        };

        let (hitbox, mask, grid) = scan_rgba_with_3d_transform(
            &buffer,
            width,
            height,
            stride,
            &transform,
            25.0, // pitch
            15.0, // yaw
            &config,
        );

        assert!(hitbox.active);
        assert_eq!(hitbox.local_x, 6.0);
        assert_eq!(hitbox.local_y, 6.0);
        assert_eq!(hitbox.local_width, 8.0);
        assert_eq!(hitbox.local_height, 8.0);
        assert!(!grid.is_empty());
        assert!(hitbox.world_width > 0.0);
        assert!(hitbox.world_height > 0.0);

        // Center of mass should be solid in 3D world space
        assert!(mask.is_solid_world(hitbox.world_center_x, hitbox.world_center_y));
        // Point far outside should not be solid
        assert!(!mask.is_solid_world(0.0, 0.0));
    }

    #[test]
    fn test_perspective_3d_projection_and_unprojection() {
        use crate::tracker::{project_3d_canvas, PERSPECTIVE_D};

        let cx = 400.0f32;
        let cy = 300.0f32;
        let roll = 25.0f32;
        let pitch = 35.0f32;
        let yaw = -20.0f32;

        let test_points = [
            (0.0f32, 0.0f32),
            (50.0f32, -30.0f32),
            (-40.0f32, 60.0f32),
            (100.0f32, 100.0f32),
        ];

        let cz = roll.to_radians().cos();
        let sz = roll.to_radians().sin();
        let cx_ang = pitch.to_radians().cos();
        let sx_ang = pitch.to_radians().sin();
        let cy_ang = yaw.to_radians().cos();
        let sy_ang = yaw.to_radians().sin();

        let ux = cy_ang * cz - sy_ang * sx_ang * sz;
        let uy = cy_ang * sz + sy_ang * sx_ang * cz;
        let uz = -sy_ang * cx_ang;

        let vx = -cx_ang * sz;
        let vy = cx_ang * cz;
        let vz = sx_ang;

        for &(lx, ly) in &test_points {
            let (wx, wy) = project_3d_canvas(lx, ly, cx, cy, roll, pitch, yaw);

            // Invert back to local coordinates
            let dx = wx - cx;
            let dy = wy - cy;

            let a00 = ux - (dx / PERSPECTIVE_D) * uz;
            let a01 = vx - (dx / PERSPECTIVE_D) * vz;
            let a10 = uy - (dy / PERSPECTIVE_D) * uz;
            let a11 = vy - (dy / PERSPECTIVE_D) * vz;

            let det = a00 * a11 - a01 * a10;
            assert!(det.abs() > 1e-6);

            let unrot_lx = (dx * a11 - dy * a01) / det;
            let unrot_ly = (a00 * dy - a10 * dx) / det;

            assert!((unrot_lx - lx).abs() < 1e-3, "lx mismatch: expected {}, got {}", lx, unrot_lx);
            assert!((unrot_ly - ly).abs() < 1e-3, "ly mismatch: expected {}, got {}", ly, unrot_ly);
        }
    }


    #[test]
    fn test_path_randomness_aims_at_the_targets_solid_pixels() {
        let target = "RandomAimTarget";
        get_state().pixel_grids.write().insert(target.to_string(), vec![crate::types::Point2D { x: 100.0, y: 200.0 }]);
        assert_eq!(crate::physics::random_aim(0.0, 0.0, Some(target), 1.0), (100.0, 200.0));
        assert_eq!(crate::physics::random_aim(0.0, 0.0, Some(target), 0.5), (50.0, 100.0));
        get_state().pixel_grids.write().remove(target);
        // No grid: near the center
        let (x, y) = crate::physics::random_aim(500.0, 500.0, Some(target), 1.0);
        assert!((x - 500.0).abs() <= 40.0 && (y - 500.0).abs() <= 40.0);
    }

    #[test]
    fn test_path_randomness_varies_each_throw() {
        use crate::physics::SpawnPackage;

        let paths = |randomness: Option<f32>| {
            let pkg = SpawnPackage {
                name: Some("PathTest".to_string()),
                action: Some("throwable".to_string()),
                target: Some("NoSuchPathTarget".to_string()),
                speed: Some(8.0),
                gravity: Some(0.8),
                spawn_points: Some(vec!["tc".to_string()]),
                amount: Some(serde_json::json!(6)),
                repeat_delay_ms: Some(10),
                path_randomness: randomness,
                ..Default::default()
            };
            let mut physics = get_state().physics.write();
            let first = physics.spawn(pkg, 1920.0, 1080.0);
            let throws: Vec<(f32, f32, f32)> = physics
                .throwables
                .iter()
                .filter(|t| t.id >= first && t.id < first + 6)
                .map(|t| (t.x, t.vx, t.vy))
                .collect();
            physics.throwables.retain(|t| t.id < first || t.id >= first + 6);
            throws
        };
        let same = paths(None);
        assert_eq!(same.len(), 6);
        assert!(same.windows(2).all(|w| w[0] == w[1]), "0%: one path");
        let varied = paths(Some(1.0));
        let distinct = varied.iter().filter(|p| **p != varied[0]).count();
        assert!(distinct >= 4, "100%: each throw its own path {:?}", varied);
    }

    #[test]
    fn test_physics_simulation_and_instances() {
        use crate::physics::SpawnPackage;

        trust_physics_clear();
        let id = get_state().physics.write().register_redeem("tomato");
        assert_eq!(id, 1);

        let pkg = SpawnPackage {
            name: Some("Tomato".to_string()),
            action: Some("throwable".to_string()),
            collision: Some("bounce".to_string()),
            target: Some("1".to_string()),
            speed: Some(10.0),
            scale: Some(1.0),
            gravity: Some(0.8),
            image_src: None,
            image_src_alt: None,
            start_sound: None,
            start_sound_alt: None,
            collision_sound: None,
            collision_sound_alt: None,
            spawn_points: Some(vec!["ml".to_string()]),
            spawn_points_alt: None,
            shake_on_impact: None,
            shake_on_impact_alt: None,
            redeem_key: Some("tomato".to_string()),
            frame_count: Some(1),
            frame_delay_ms: Some(100),
            ..Default::default()
        };

        let th_id = get_state().physics.write().spawn(pkg, 1920.0, 1080.0);
        assert!(th_id > 0);

        let mut t_count = 0u32;
        let mut p_count = 0u32;
        trust_physics_get_count(&mut t_count, &mut p_count);
        assert_eq!(t_count, 1);
        assert_eq!(p_count, 0);

        // Step physics 10 frames
        for _ in 0..10 {
            trust_physics_step(1.0 / 60.0, 1920.0, 1080.0);
        }

        let mut instances = [crate::physics::RenderInstance {
            x: 0.0, y: 0.0, rotation: 0.0, scale: 0.0, cur_frame: 0,
            redeem_id: 0, is_particle: 0, r: 0.0, g: 0.0, b: 0.0, a: 0.0, size: 0.0
        }; 10];
        let count = trust_physics_get_instances(instances.as_mut_ptr(), 10);
        assert_eq!(count, 1);
        assert_eq!(instances[0].redeem_id, 1);
        assert!(instances[0].x > -50.0); // Moved forward!
    }

    #[test]
    fn test_ballistic_trajectory_aims_at_little_robot() {
        use crate::physics::SpawnPackage;

        // Ensure targets are loaded from disk
        crate::studio_app::load_studio_targets_from_disk();
        trust_physics_clear();

        // Little robot is "Image 3"
        let pkg = SpawnPackage {
            name: Some("TomatoBotTest".to_string()),
            action: Some("throwable".to_string()),
            collision: Some("bounce".to_string()),
            target: Some("Image 3".to_string()),
            speed: Some(8.0),
            scale: Some(1.0),
            gravity: Some(0.8),
            spawn_points: Some(vec!["ml".to_string()]),
            redeem_key: Some("tomato".to_string()),
            ..Default::default()
        };

        let th_id = get_state().physics.write().spawn(pkg, 1920.0, 1080.0);
        assert!(th_id > 0);

        // Step physics until impact
        let mut hit = false;
        for _ in 0..120 {
            trust_physics_step(1.0 / 60.0, 1920.0, 1080.0);
            let phys = get_state().physics.read();
            if let Some(th) = phys.throwables.iter().find(|t| t.id == th_id) {
                if th.has_hit {
                    hit = true;
                    // Verify impact position is near Image 3 bounds
                    let img3_opt = get_state().studio_sources.read().iter().find(|s| s.name == "Image 3").cloned();
                    let (min_x, max_x, min_y, max_y) = if let Some(s) = img3_opt {
                        (s.x - 50.0, s.x + s.width + 50.0, s.y - 50.0, s.y + s.height + 50.0)
                    } else {
                        (200.0, 1200.0, 600.0, 1100.0)
                    };
                    assert!(th.x >= min_x && th.x <= max_x, "Impact x ({}) must be near little robot ({}-{})", th.x, min_x, max_x);
                    assert!(th.y >= min_y && th.y <= max_y, "Impact y ({}) must be near little robot ({}-{})", th.y, min_y, max_y);
                    break;
                }
            }
        }
        assert!(hit, "Throwable aimed at Image 3 must connect with the little robot");

        // Verify "random" target resolution picks an active target
        get_state().target_slots.write().insert(1, "Image 3".to_string());
        get_state().hitboxes.write().insert("Image 3".to_string(), crate::types::Hitbox {
            active: true,
            world_width: 200.0,
            world_height: 200.0,
            ..Default::default()
        });
        let slots = get_state().target_slots.read();
        let hitboxes = get_state().hitboxes.read();
        let resolved = resolve_target_hitbox("random", &slots, &hitboxes);
        assert!(resolved.is_some(), "'random' must resolve to an active target");
    }

    #[test]
    fn test_scan_targets_from_composite() {
        let state = get_state();
        let target_name = "Avatar_Composite_Test".to_string();

        // Register a studio source target
        state.studio_sources.write().push(crate::StudioSource {
            scene: "Scène".to_string(),
            name: target_name.clone(),
            slot: 1,
            x: 100.0,
            y: 100.0,
            width: 50.0,
            height: 50.0,
            roll: 0.0,
            pitch: 0.0,
            yaw: 0.0,
            is_target: true,
            match_obs: true,
            is_visible: true,
            file_path: String::new(),
            default_x: 100.0,
            default_y: 100.0,
            default_width: 50.0,
            default_height: 50.0,
            default_roll: 0.0,
        });

        // Create a 400x300 buffer with transparent layer background [0,0,0,0]
        // and a solid 20x20 box + 5x5 solid black box inside the target area (110..130, 110..130 and 135..140, 135..140)
        let cw = 400u32;
        let ch = 300u32;
        let mut canvas = vec![0u8; (cw * ch * 4) as usize];
        for y in 110..130 {
            for x in 110..130 {
                let idx = ((y * cw + x) * 4) as usize;
                canvas[idx] = 255;     // R
                canvas[idx + 1] = 100; // G
                canvas[idx + 2] = 50;  // B
                canvas[idx + 3] = 255; // A (solid)
            }
        }
        for y in 135..140 {
            for x in 135..140 {
                let idx = ((y * cw + x) * 4) as usize;
                canvas[idx] = 0;     // Pure black R
                canvas[idx + 1] = 0; // Pure black G
                canvas[idx + 2] = 0; // Pure black B
                canvas[idx + 3] = 255; // A (solid black is detected!)
            }
        }

        crate::obs_source::scan_targets_from_composite(&canvas, cw, ch);

        let hitboxes = state.hitboxes.read();
        assert!(hitboxes.contains_key(&target_name));
        let hb = &hitboxes[&target_name];
        assert!(hb.active);
        assert_eq!(hb.mass, 20 * 20 + 5 * 5);

        let masks = state.masks.read();
        assert!(masks.contains_key(&target_name));

        let grids = state.pixel_grids.read();
        assert!(grids.contains_key(&target_name));
        assert_eq!(grids[&target_name].len(), 425);

        let textures = state.target_textures.read();
        assert!(textures.contains_key(&target_name));
        let tex = &textures[&target_name];
        assert_eq!(tex.width, 50);
        assert_eq!(tex.height, 50);
    }

    #[test]
    fn test_scan_targets_from_composite_rotated_3d() {
        let state = get_state();
        let target_name = "Rotated_3D_Target".to_string();

        // 100x100 target at (150, 150) rotated with roll=45.0
        // Center is (200, 200). Unrotated AABB would be (150..250, 150..250).
        // Rotated by 45 degrees, corners extend to 200 +/- 100*sqrt(2)/2 = 200 +/- 70.71 -> (129..271)
        state.studio_sources.write().push(crate::StudioSource {
            scene: "Scène".to_string(),
            name: target_name.clone(),
            slot: 2,
            x: 150.0,
            y: 150.0,
            width: 100.0,
            height: 100.0,
            roll: 45.0,
            pitch: 0.0,
            yaw: 0.0,
            is_target: true,
            match_obs: true,
            is_visible: true,
            file_path: String::new(),
            default_x: 150.0,
            default_y: 150.0,
            default_width: 100.0,
            default_height: 100.0,
            default_roll: 45.0,
        });

        let cw = 400u32;
        let ch = 400u32;
        let mut canvas = vec![0u8; (cw * ch * 4) as usize];

        // Draw a solid 20x20 square around the center (190..210, 190..210) on canvas
        for y in 190..210 {
            for x in 190..210 {
                let idx = ((y * cw + x) * 4) as usize;
                canvas[idx] = 255;
                canvas[idx + 1] = 50;
                canvas[idx + 2] = 50;
                canvas[idx + 3] = 255;
            }
        }

        crate::obs_source::scan_targets_from_composite(&canvas, cw, ch);

        let hitboxes = state.hitboxes.read();
        assert!(hitboxes.contains_key(&target_name));
        let hb = &hitboxes[&target_name];
        assert!(hb.active);
        assert!(hb.mass >= 380 && hb.mass <= 420);
        // Center of mass should be around (199.5, 199.5)
        assert!((hb.world_center_x - 200.0).abs() < 2.0);
        assert!((hb.world_center_y - 200.0).abs() < 2.0);

        let masks = state.masks.read();
        assert!(masks.contains_key(&target_name));
        let mask = &masks[&target_name];
        assert!(mask.is_solid_world(200.0, 200.0));
        assert!(mask.is_solid_world(195.0, 195.0));
        assert!(!mask.is_solid_world(170.0, 170.0));
        assert!(!mask.is_solid_world(230.0, 230.0));
    }

    #[test]
    fn test_scan_targets_from_composite_yaw_over_90_flips_mask() {
        let state = get_state();
        let target_name = "Yaw_Flip_Target".to_string();

        // 100x100 target at center (200, 200) rotated with yaw=180.0 (facing backwards/flipped)
        state.studio_sources.write().push(crate::StudioSource {
            scene: "Scène".to_string(),
            name: target_name.clone(),
            slot: 3,
            x: 150.0,
            y: 150.0,
            width: 100.0,
            height: 100.0,
            roll: 0.0,
            pitch: 0.0,
            yaw: 180.0,
            is_target: true,
            match_obs: true,
            is_visible: true,
            file_path: String::new(),
            default_x: 150.0,
            default_y: 150.0,
            default_width: 100.0,
            default_height: 100.0,
            default_roll: 0.0,
        });

        let cw = 400u32;
        let ch = 400u32;
        let mut canvas = vec![0u8; (cw * ch * 4) as usize];

        // Because yaw=180 degrees, the sprite's local left side (lx in -40..-20) is rendered on the RIGHT side of the canvas (220..240)!
        for y in 190..210 {
            for x in 220..240 {
                let idx = ((y * cw + x) * 4) as usize;
                canvas[idx] = 255;
                canvas[idx + 1] = 100;
                canvas[idx + 2] = 50;
                canvas[idx + 3] = 255;
            }
        }

        crate::obs_source::scan_targets_from_composite(&canvas, cw, ch);

        let masks = state.masks.read();
        assert!(masks.contains_key(&target_name));
        let mask = &masks[&target_name];

        // Mask must be solid on the right side where it was drawn in world space!
        assert!(mask.is_solid_world(230.0, 200.0), "Mask must be solid on world right (230, 200) where pixel was rendered with yaw=180");
        // Mask must NOT be solid on the opposite left side
        assert!(!mask.is_solid_world(170.0, 200.0), "Mask must NOT be solid on world left (170, 200)");

        // Also verify target preview texture in target_textures is stored unrotated (local left)
        let textures = state.target_textures.read();
        assert!(textures.contains_key(&target_name));
        let tex = &textures[&target_name];
        assert_eq!(tex.width, 100);
        assert_eq!(tex.height, 100);
        // In local unrotated space, local x in 10..30 (left side) is non-zero
        let local_left_idx = (50 * 100 + 20) * 4;
        assert_eq!(tex.rgba[local_left_idx], 255, "Local unrotated texture must have pixel on local left");
    }

    #[test]
    fn test_pixel_shatter_uses_image_pixel_colors() {
        use image::{ImageBuffer, Rgba};
        use base64::Engine;

        // 1. Create a 4x4 image of pure blue pixels (R=0, G=0, B=255, A=255)
        let img: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::from_pixel(4, 4, Rgba([0, 0, 255, 255]));
        let mut png_bytes = Vec::new();
        let encoder = image::codecs::png::PngEncoder::new(&mut png_bytes);
        img.write_with_encoder(encoder).unwrap();
        let b64 = format!("data:image/png;base64,{}", base64::prelude::BASE64_STANDARD.encode(&png_bytes));

        let state = get_state();
        state.physics.write().clear();

        // 2. Set up target hitbox at (500, 500)
        let mut hb = crate::types::Hitbox::default();
        hb.active = true;
        hb.world_x = 450.0;
        hb.world_y = 450.0;
        hb.world_width = 100.0;
        hb.world_height = 100.0;
        hb.world_center_x = 500.0;
        hb.world_center_y = 500.0;
        state.hitboxes.write().insert("TestTarget".to_string(), hb);

        // 3. Spawn throwable with pixel-shatter collision
        let pkg = crate::physics::SpawnPackage {
            name: Some("BlueShatter".to_string()),
            action: Some("throwable".to_string()),
            collision: Some("pixel-shatter".to_string()),
            target: Some("TestTarget".to_string()),
            speed: Some(15.0),
            scale: Some(1.0),
            gravity: Some(0.8),
            image_src: Some(b64),
            ..Default::default()
        };
        let th_id = state.physics.write().spawn(pkg, 1920.0, 1080.0);
        assert!(th_id > 0);

        // Move throwable directly to target center to guarantee impact
        {
            let mut p = state.physics.write();
            assert_eq!(p.throwables.len(), 1);
            p.throwables[0].x = 500.0;
            p.throwables[0].y = 500.0;
        }

        // Step physics to trigger impact and shatter
        state.physics.write().step(1.0 / 60.0, 1920.0, 1080.0);

        let p = state.physics.read();
        assert_eq!(p.throwables.len(), 0, "Throwable should be dead after pixel-shatter");
        assert!(p.particles.len() > 0, "Shatter should produce particles from image pixels");

        // Verify that particles have the image's actual blue color (B > 0.9, R < 0.1), NOT hardcoded red (R > 0.8, B < 0.2)
        for particle in &p.particles {
            assert!(particle.b > 0.9, "Particle B should be ~1.0, got {}", particle.b);
            assert!(particle.r < 0.1, "Particle R should be ~0.0, got {}", particle.r);
            assert!(particle.g < 0.1, "Particle G should be ~0.0, got {}", particle.g);
        }
    }

    #[test]
    fn test_splat_collision_sticks_and_slides_and_fades() {
        use crate::physics::{CollisionType, ThrowableState, RenderInstance};

        // 1. Verify "splat" string mapping
        assert_eq!(CollisionType::from("splat"), CollisionType::Squish);
        assert_eq!(CollisionType::from("Splat & Fade (splat)"), CollisionType::Squish);

        let state = get_state();
        state.physics.write().clear();

        // 2. Set up target hitbox at (600, 400), height = 200
        let mut hb = crate::types::Hitbox::default();
        hb.active = true;
        hb.world_x = 550.0;
        hb.world_y = 300.0;
        hb.world_width = 100.0;
        hb.world_height = 200.0;
        hb.world_center_x = 600.0;
        hb.world_center_y = 400.0;
        state.hitboxes.write().insert("SplatTarget".to_string(), hb);

        // 3. Spawn throwable with "splat" collision
        let pkg = crate::physics::SpawnPackage {
            name: Some("TomatoSplat".to_string()),
            action: Some("throwable".to_string()),
            collision: Some("splat".to_string()),
            target: Some("SplatTarget".to_string()),
            speed: Some(10.0),
            scale: Some(1.0),
            gravity: Some(0.8),
            ..Default::default()
        };
        let th_id = state.physics.write().spawn(pkg, 1920.0, 1080.0);
        assert!(th_id > 0);

        {
            let p = state.physics.read();
            assert_eq!(p.throwables.len(), 1);
            assert_eq!(p.throwables[0].collision, CollisionType::Squish);
            assert_eq!(p.throwables[0].alpha, 1.0);
        }

        // Place directly on target (y = 350.0, which is relative y = 0.25 on a 200px tall target)
        {
            let mut p = state.physics.write();
            p.throwables[0].x = 600.0;
            p.throwables[0].y = 350.0;
        }

        // Step physics to trigger impact
        state.physics.write().step(1.0 / 60.0, 1920.0, 1080.0);

        let initial_stuck_rel_y;
        let initial_stuck_y;
        {
            let p = state.physics.read();
            assert_eq!(p.throwables.len(), 1);
            let th = &p.throwables[0];
            assert_eq!(th.state, ThrowableState::Stuck, "Throwable must stick on splat collision");
            assert_eq!(th.vx, 0.0);
            assert_eq!(th.vy, 0.0);
            assert_eq!(th.v_rot, 0.0);
            assert_eq!(th.alpha, 1.0, "Alpha must initially be 1.0 on impact");
            initial_stuck_rel_y = th.stuck_rel_y;
            initial_stuck_y = th.y;
            assert!(initial_stuck_rel_y > 0.0 && initial_stuck_rel_y < 0.9, "Relative y should be within target bounds, got {}", initial_stuck_rel_y);
        }

        // Advance 30 physics steps (0.5s at 60fps)
        for _ in 0..30 {
            state.physics.write().step(1.0 / 60.0, 1920.0, 1080.0);
        }

        {
            let p = state.physics.read();
            assert_eq!(p.throwables.len(), 1);
            let th = &p.throwables[0];
            assert_eq!(th.state, ThrowableState::Stuck);
            assert!(th.stuck_rel_y > initial_stuck_rel_y, "Throwable must slide down slowly: initial={}, current={}", initial_stuck_rel_y, th.stuck_rel_y);
            assert!(th.y > initial_stuck_y, "World Y must also move down with sliding: was {}, now {}", initial_stuck_y, th.y);
        }

        // Simulate time advancing into fade period (e.g. 4.5 seconds after stuck)
        {
            let mut p = state.physics.write();
            p.throwables[0].stuck_time = p.throwables[0].stuck_time.saturating_sub(4_500_000_000);
        }

        state.physics.write().step(1.0 / 60.0, 1920.0, 1080.0);

        {
            let p = state.physics.read();
            assert_eq!(p.throwables.len(), 1);
            let th = &p.throwables[0];
            assert!(th.alpha < 0.95 && th.alpha > 0.0, "Throwable must fade out over time: alpha={}", th.alpha);

            // Test RenderInstance output
            let mut instances = [RenderInstance {
                x: 0.0, y: 0.0, rotation: 0.0, scale: 0.0, cur_frame: 0, redeem_id: 0,
                is_particle: 0, r: 0.0, g: 0.0, b: 0.0, a: 0.0, size: 0.0,
            }; 8];
            let count = p.get_render_instances(&mut instances);
            assert_eq!(count, 1);
            assert_eq!(instances[0].a, th.alpha, "RenderInstance alpha must match throwable alpha");
        }
    }

    #[test]
    fn test_throwable_amount_parsing_and_wildcards() {
        use crate::physics::{parse_amount_with_wildcards, resolve_wildcards};

        // 1. Direct numbers
        let num_val = serde_json::json!(5);
        assert_eq!(parse_amount_with_wildcards(Some(&num_val), &[]), 5);

        // 2. Direct string numbers
        let str_val = serde_json::json!("8");
        assert_eq!(parse_amount_with_wildcards(Some(&str_val), &[]), 8);

        // 3. Wildcard {1} resolution with arguments
        let wildcard_val = serde_json::json!("{1}");
        let args = vec!["3".to_string()];
        assert_eq!(parse_amount_with_wildcards(Some(&wildcard_val), &args), 3);

        // 4. Wildcard {2} resolution
        let wildcard_2 = serde_json::json!("{2}");
        let multi_args = vec!["tomato".to_string(), "7".to_string()];
        assert_eq!(parse_amount_with_wildcards(Some(&wildcard_2), &multi_args), 7);

        // 5. Wildcard {1} when no arguments provided: falls back cleanly to 1
        assert_eq!(parse_amount_with_wildcards(Some(&wildcard_val), &[]), 1);

        // 6. Non-number string: falls back cleanly to 1
        let invalid_val = serde_json::json!("invalid");
        assert_eq!(parse_amount_with_wildcards(Some(&invalid_val), &[]), 1);

        // 7. None: defaults to 1
        assert_eq!(parse_amount_with_wildcards(None, &[]), 1);

        // 8. Amount bounds: minimum 1, no upper limit
        let huge_val = serde_json::json!(9999);
        assert_eq!(parse_amount_with_wildcards(Some(&huge_val), &[]), 9999);

        let zero_val = serde_json::json!(0);
        assert_eq!(parse_amount_with_wildcards(Some(&zero_val), &[]), 1);

        // 9. Template string replacement
        assert_eq!(resolve_wildcards("Target {1}", &["Avatar".to_string()]), "Target Avatar");
        assert_eq!(resolve_wildcards("{amount}", &["10".to_string()]), "10");
    }

    #[test]
    fn test_throwable_multi_spawn_staggered() {
        let state = get_state();
        state.physics.write().clear();

        let pkg = crate::physics::SpawnPackage {
            name: Some("Tennis Ball".to_string()),
            action: Some("throwable".to_string()),
            collision: Some("bounce".to_string()),
            target: Some("1".to_string()),
            amount: Some(serde_json::json!("{1}")),
            repeat_delay_ms: Some(150),
            wildcards: Some(vec!["4".to_string()]),
            ..Default::default()
        };

        let first_id = state.physics.write().spawn(pkg, 1920.0, 1080.0);
        assert!(first_id > 0);

        let p = state.physics.read();
        assert_eq!(p.throwables.len(), 4, "Should spawn exactly 4 throwables for wildcard {{1}} = 4");

        // First item spawns immediately, subsequent items have staggered spawn times
        let t0 = p.throwables[0].spawn_time;
        let t1 = p.throwables[1].spawn_time;
        let t2 = p.throwables[2].spawn_time;
        let t3 = p.throwables[3].spawn_time;

        assert!(t1 > t0, "t1 must be staggered after t0");
        assert!(t2 > t1, "t2 must be staggered after t1");
        assert!(t3 > t2, "t3 must be staggered after t2");

        let diff_ms_1 = (t1 - t0) / 1_000_000;
        let diff_ms_2 = (t2 - t1) / 1_000_000;
        assert_eq!(diff_ms_1, 150, "Stagger gap must match repeat_delay_ms = 150");
        assert_eq!(diff_ms_2, 150, "Stagger gap must match repeat_delay_ms = 150");
    }

    #[test]
    fn test_dispatch_test_redeem_with_wildcard_amount() {
        let state = get_state();
        state.physics.write().clear();

        let redeem_json = r#"{
            "name": "Pie",
            "type": "image",
            "action": "throwable",
            "target": "Avatar",
            "amount": "{1}",
            "wildcards": ["3"],
            "repeatDelay": 120
        }"#;

        assert!(crate::studio_app::dispatch_test_redeem_json(redeem_json));

        let p = state.physics.read();
        assert_eq!(p.throwables.len(), 3, "Wildcard {{1}} with ['3'] must spawn 3 throwables");
        for th in &p.throwables {
            assert_eq!(th.name, "Pie");
        }
    }

    #[test]
    fn test_injected_chat_reaches_rhai_sources() {
        // The chat (a Rhai source) gets an injected message as the relay would send it
        let feed = crate::script_feed::subscribe();
        let test_json = r##"{"type":"chat","source":"Twitch","user":"Alice","color":"#9146FF","badges":"[BROADCASTER] [VIP]","message":"GG [twitch_emote:25] awesome stream"}"##;
        let c_json = std::ffi::CString::new(test_json).unwrap();
        trust_inject_event(c_json.as_ptr());
        let got: Vec<std::sync::Arc<str>> = feed.try_iter().collect();
        assert!(got.iter().any(|m| &**m == test_json), "{:?}", got);
    }

    #[test]
    fn test_snap_image_bottom_to_avatar_keeps_bottom_aimed_at_target_center() {
        let state = get_state();
        state.physics.write().clear();

        // 1. Set up avatar hitbox at center (500, 500)
        let mut hb = crate::types::Hitbox::default();
        hb.active = true;
        hb.world_x = 450.0;
        hb.world_y = 450.0;
        hb.world_width = 100.0;
        hb.world_height = 100.0;
        hb.world_center_x = 500.0;
        hb.world_center_y = 500.0;
        state.hitboxes.write().insert("Avatar".to_string(), hb);

        // 2. Spawn throwable with snapBottom = true
        let pkg = crate::physics::SpawnPackage {
            name: Some("SnapArrow".to_string()),
            action: Some("throwable".to_string()),
            target: Some("Avatar".to_string()),
            snap_bottom: Some(true),
            speed: Some(10.0),
            gravity: Some(0.0),
            ..Default::default()
        };

        let th_id = state.physics.write().spawn(pkg, 1920.0, 1080.0);
        assert!(th_id > 0);

        // 3. Position above avatar (500, 100).
        // Target is at (500, 500), so direction is straight down (+Y).
        // In 2D image coordinates, bottom of image points straight down when rotation = 0 deg.
        {
            let mut p = state.physics.write();
            p.throwables[0].x = 500.0;
            p.throwables[0].y = 100.0;
            p.throwables[0].vx = 0.0;
            p.throwables[0].vy = 0.0;
        }
        state.physics.write().step(1.0 / 60.0, 1920.0, 1080.0);

        {
            let p = state.physics.read();
            assert!(p.throwables[0].snap_bottom);
            let rot = p.throwables[0].rotation;
            assert!(rot.abs() < 1.0, "Expected rotation ~0.0 when above avatar, got {}", rot);
        }

        // 4. Position to the left of avatar (100, 500).
        // Target is at (500, 500), so direction is to the right (+X).
        // Bottom of image pointing to the right means rotation = -90 deg.
        {
            let mut p = state.physics.write();
            p.throwables[0].x = 100.0;
            p.throwables[0].y = 500.0;
            p.throwables[0].vx = 0.0;
            p.throwables[0].vy = 0.0;
        }
        state.physics.write().step(1.0 / 60.0, 1920.0, 1080.0);

        {
            let p = state.physics.read();
            let rot = p.throwables[0].rotation;
            assert!((rot - (-90.0)).abs() < 1.0, "Expected rotation ~ -90.0 when left of avatar, got {}", rot);
        }

        // 5. Position below avatar (500, 900).
        // Target is at (500, 500), so direction is straight up (-Y).
        // Bottom of image pointing straight up means rotation = -180 deg or +180 deg.
        {
            let mut p = state.physics.write();
            p.throwables[0].x = 500.0;
            p.throwables[0].y = 900.0;
            p.throwables[0].vx = 0.0;
            p.throwables[0].vy = 0.0;
        }
        state.physics.write().step(1.0 / 60.0, 1920.0, 1080.0);

        {
            let p = state.physics.read();
            let rot = p.throwables[0].rotation;
            assert!((rot.abs() - 180.0).abs() < 1.0, "Expected rotation ~180.0 when below avatar, got {}", rot);
        }
    }
}


