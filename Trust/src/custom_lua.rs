//! Custom Lua Redeem execution engine.
//!
//! Allows streamers and users to write custom Lua scripts (e.g. `snow_storm.lua`,
//! metaball liquid showers, multi-stage spells) that execute when a redeem triggers.
//! Scripts run in an asynchronous worker thread so that `trust.sleep()` or loops
//! do not block the UI or chat processing.

use mlua::{Lua, LuaSerdeExt, Table, Value};

pub static PRE_REDEEM_POSES: std::sync::LazyLock<parking_lot::Mutex<std::collections::HashMap<String, crate::studio_app::SourceTransformSnapshot>>> =
    std::sync::LazyLock::new(|| parking_lot::Mutex::new(std::collections::HashMap::new()));

static ACTIVE_CUSTOM_SCRIPTS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

#[inline]
pub fn is_custom_script_active() -> bool {
    ACTIVE_CUSTOM_SCRIPTS.load(std::sync::atomic::Ordering::Relaxed) > 0
}

#[inline]
pub fn is_source_under_script(name: &str) -> bool {
    let pre = PRE_REDEEM_POSES.lock();
    pre.contains_key(name)
}

pub fn restore_source_to_last_pose(s: &mut crate::StudioSource) {
    let last_pose = {
        let pose_map = crate::studio_app::LAST_SENT_POSE.read();
        pose_map.get(&s.name).cloned().or_else(|| {
            let pre = PRE_REDEEM_POSES.lock();
            pre.get(&s.name).cloned()
        })
    };

    if let Some(pose) = last_pose {
        s.x = pose.x;
        s.y = pose.y;
        if pose.width > 0.0 { s.width = pose.width; }
        if pose.height > 0.0 { s.height = pose.height; }
        s.roll = pose.roll;
        s.pitch = pose.pitch;
        s.yaw = pose.yaw;
        s.is_visible = pose.is_visible;
    } else {
        s.x = s.default_x;
        s.y = s.default_y;
        if s.default_width > 0.0 { s.width = s.default_width; }
        if s.default_height > 0.0 { s.height = s.default_height; }
        s.roll = s.default_roll;
        s.pitch = 0.0;
        s.yaw = 0.0;
        s.is_visible = true;
    }
}

/// The script a Lua state runs (execute_custom_script_sync sets it).
pub(crate) struct RunningScript(pub std::path::PathBuf);

/// Paths a script hands to trust.* that are relative and name a file next to
/// the script become that file's full path: an imported redeem (.red) brings
/// its own files, like doom.lua's "filters/swirl.lua". Anything else is left
/// to the usual lookup (the Theater folder, filters).
fn localize_paths(lua: &Lua, val: &mut serde_json::Value) {
    fn localize(dir: &std::path::Path, val: &mut serde_json::Value) {
        match val {
            serde_json::Value::String(s) => {
                let path = std::path::Path::new(s.as_str());
                if !s.starts_with("data:") && s.len() < 1024 && path.is_relative() && path.extension().is_some() {
                    let full = dir.join(path);
                    if full.is_file() {
                        *s = full.to_string_lossy().into_owned();
                    }
                }
            }
            serde_json::Value::Array(items) => items.iter_mut().for_each(|v| localize(dir, v)),
            serde_json::Value::Object(map) => map.values_mut().for_each(|v| localize(dir, v)),
            _ => {}
        }
    }
    let dir = lua
        .app_data_ref::<RunningScript>()
        .and_then(|s| s.0.parent().map(|d| d.to_path_buf()));
    if let Some(dir) = dir {
        localize(&dir, val);
    }
}

/// Fires what a script built, as a redeem dispatch.
fn dispatch_from_lua(lua: &Lua, mut val: serde_json::Value) -> mlua::Result<bool> {
    localize_paths(lua, &mut val);
    let json_str = serde_json::to_string(&val).map_err(|e| mlua::Error::runtime(e.to_string()))?;
    Ok(crate::studio_app::dispatch_test_redeem_json(&json_str))
}

/// Sets up the global `trust` table in the Lua state.
pub fn setup_trust_env(lua: &Lua) -> mlua::Result<()> {
    let trust = lua.create_table()?;

    // 1. trust.trigger(payload_or_json) -> bool
    // Direct dispatch into the core redeem engine. Supports all redeem types:
    // "webgl-water" (metaball fluid), "image"/"throwable", "webgl-fire",
    // "webgl-lightning", "reaction".
    trust.set(
        "trigger",
        lua.create_function(|lua, val: Value| {
            match val {
                Value::String(s) => {
                    let json_str = s.to_str()?.to_string();
                    match serde_json::from_str::<serde_json::Value>(&json_str) {
                        Ok(serde_val) => dispatch_from_lua(lua, serde_val),
                        Err(_) => Ok(crate::studio_app::dispatch_test_redeem_json(&json_str)),
                    }
                }
                Value::Table(t) => {
                    let serde_val: serde_json::Value = lua.from_value(Value::Table(t))?;
                    dispatch_from_lua(lua, serde_val)
                }
                _ => Err(mlua::Error::runtime("trust.trigger expects a table or JSON string")),
            }
        })?,
    )?;

    // 2. trust.spawn_water(opts_table) / trust.spawn_fluid(opts_table)
    // SPH fluid simulation with metaball surface rendering.
    let spawn_water = lua.create_function(|lua, opts: Table| {
        let mut val: serde_json::Value = lua.from_value(Value::Table(opts))?;
        if let serde_json::Value::Object(ref mut map) = val {
            map.insert("type".to_string(), serde_json::json!("webgl-water"));
            if !map.contains_key("redeem_key") {
                if let Ok(params_tbl) = lua.globals().get::<Table>("params") {
                    if let Ok(rk) = params_tbl.get::<String>("redeem_key") {
                        if !rk.is_empty() {
                            map.insert("redeem_key".to_string(), serde_json::json!(rk));
                        }
                    }
                }
            }
        }
        dispatch_from_lua(lua, val)
    })?;
    trust.set("spawn_water", spawn_water.clone())?;
    trust.set("spawn_fluid", spawn_water)?;

    // 3. trust.spawn_throwable(opts_table) / trust.spawn_shape(opts_table)
    // Physical throwables: shapes, images, rain, swirl, 3D throw-front, drops.
    let spawn_throwable = lua.create_function(|lua, opts: Table| {
        let mut val: serde_json::Value = lua.from_value(Value::Table(opts))?;
        if let serde_json::Value::Object(ref mut map) = val {
            if !map.contains_key("type") {
                map.insert("type".to_string(), serde_json::json!("image"));
            }
            if !map.contains_key("redeem_key") {
                if let Ok(params_tbl) = lua.globals().get::<Table>("params") {
                    if let Ok(rk) = params_tbl.get::<String>("redeem_key") {
                        if !rk.is_empty() {
                            map.insert("redeem_key".to_string(), serde_json::json!(rk));
                        }
                    }
                }
            }
        }
        dispatch_from_lua(lua, val)
    })?;
    trust.set("spawn_throwable", spawn_throwable.clone())?;
    trust.set("spawn_shape", spawn_throwable)?;

    // 4. trust.spawn_fire(opts_table)
    // Flamethrower fire bursts.
    trust.set(
        "spawn_fire",
        lua.create_function(|lua, opts: Table| {
            let mut val: serde_json::Value = lua.from_value(Value::Table(opts))?;
            if let serde_json::Value::Object(ref mut map) = val {
                map.insert("type".to_string(), serde_json::json!("webgl-fire"));
                if !map.contains_key("redeem_key") {
                    if let Ok(params_tbl) = lua.globals().get::<Table>("params") {
                        if let Ok(rk) = params_tbl.get::<String>("redeem_key") {
                            if !rk.is_empty() {
                                map.insert("redeem_key".to_string(), serde_json::json!(rk));
                            }
                        }
                    }
                }
            }
            dispatch_from_lua(lua, val)
        })?,
    )?;

    // 5. trust.spawn_lightning(opts_table)
    // Lightning strikes from origin edges to target.
    trust.set(
        "spawn_lightning",
        lua.create_function(|lua, opts: Table| {
            let mut val: serde_json::Value = lua.from_value(Value::Table(opts))?;
            if let serde_json::Value::Object(ref mut map) = val {
                map.insert("type".to_string(), serde_json::json!("webgl-lightning"));
                if !map.contains_key("redeem_key") {
                    if let Ok(params_tbl) = lua.globals().get::<Table>("params") {
                        if let Ok(rk) = params_tbl.get::<String>("redeem_key") {
                            if !rk.is_empty() {
                                map.insert("redeem_key".to_string(), serde_json::json!(rk));
                            }
                        }
                    }
                }
            }
            dispatch_from_lua(lua, val)
        })?,
    )?;

    // 6. trust.trigger_reaction(opts_table)
    // Impact reactions on the target source.
    trust.set(
        "trigger_reaction",
        lua.create_function(|lua, opts: Table| {
            let mut val: serde_json::Value = lua.from_value(Value::Table(opts))?;
            if let serde_json::Value::Object(ref mut map) = val {
                map.insert("type".to_string(), serde_json::json!("reaction"));
            }
            dispatch_from_lua(lua, val)
        })?,
    )?;

    // 7. trust.play_sound(path_or_name)
    trust.set(
        "play_sound",
        lua.create_function(|lua, sound: String| {
            let mut sound = serde_json::Value::String(sound);
            localize_paths(lua, &mut sound);
            crate::play_sound_str(sound.as_str().unwrap_or_default());
            Ok(())
        })?,
    )?;

    // 8. trust.sleep(seconds)
    // Allows scripts to pause between effects/spawns without blocking OBS or the UI.
    trust.set(
        "sleep",
        lua.create_function(|_lua, seconds: f64| {
            if seconds > 0.0 {
                std::thread::sleep(std::time::Duration::from_secs_f64(seconds.min(60.0)));
            }
            Ok(())
        })?,
    )?;

    // 9. trust.get_canvas_size() -> (w, h)
    trust.set(
        "get_canvas_size",
        lua.create_function(|_lua, ()| {
            let (w, h) = crate::obs_source::get_obs_canvas_resolution();
            Ok((w, h))
        })?,
    )?;

    // 10. trust.get_target_pos(target_name) -> (center_x, center_y, width, height, active)
    trust.set(
        "get_target_pos",
        lua.create_function(|_lua, target_name: String| {
            let state = crate::state::get_state();
            let hitboxes = state.hitboxes.read();
            let slots = state.target_slots.read();
            if let Some((_, hb)) = crate::resolve_target_hitbox(&target_name, &slots, &hitboxes) {
                let cx = if hb.world_center_x > 0.0 { hb.world_center_x } else { hb.world_x + hb.world_width * 0.5 };
                let cy = if hb.world_center_y > 0.0 { hb.world_center_y } else { hb.world_y + hb.world_height * 0.5 };
                Ok((cx, cy, hb.world_width, hb.world_height, hb.active))
            } else {
                let (cw, ch) = crate::obs_source::get_obs_canvas_resolution();
                Ok(((cw as f32) * 0.5, (ch as f32) * 0.5, 0.0f32, 0.0f32, false))
            }
        })?,
    )?;

    // 11. trust.get_target_info(target_name) -> table
    trust.set(
        "get_target_info",
        lua.create_function(|lua, target_name: String| {
            let state = crate::state::get_state();
            let hitboxes = state.hitboxes.read();
            let slots = state.target_slots.read();
            let res = crate::resolve_target_hitbox(&target_name, &slots, &hitboxes);
            let tbl = lua.create_table()?;
            if let Some((name, hb)) = res {
                tbl.set("name", name)?;
                tbl.set("x", hb.world_x)?;
                tbl.set("y", hb.world_y)?;
                tbl.set("width", hb.world_width)?;
                tbl.set("height", hb.world_height)?;
                let cx = if hb.world_center_x > 0.0 { hb.world_center_x } else { hb.world_x + hb.world_width * 0.5 };
                let cy = if hb.world_center_y > 0.0 { hb.world_center_y } else { hb.world_y + hb.world_height * 0.5 };
                tbl.set("center_x", cx)?;
                tbl.set("center_y", cy)?;
                tbl.set("active", hb.active)?;
            } else {
                tbl.set("name", target_name)?;
                let (cw, ch) = crate::obs_source::get_obs_canvas_resolution();
                tbl.set("x", 0.0)?;
                tbl.set("y", 0.0)?;
                tbl.set("width", cw as f32)?;
                tbl.set("height", ch as f32)?;
                tbl.set("center_x", (cw as f32) * 0.5)?;
                tbl.set("center_y", (ch as f32) * 0.5)?;
                tbl.set("active", false)?;
            }
            Ok(tbl)
        })?,
    )?;

    // 12. trust.get_targets() -> array of strings
    trust.set(
        "get_targets",
        lua.create_function(|lua, ()| {
            let state = crate::state::get_state();
            let sources = state.studio_sources.read();
            let tgts: Vec<String> = sources.iter().filter(|s| s.is_target).map(|s| s.name.clone()).collect();
            let arr = lua.create_table()?;
            for (idx, name) in tgts.into_iter().enumerate() {
                arr.set(idx + 1, name)?;
            }
            Ok(arr)
        })?,
    )?;

    // 13. trust.get_sources() -> array of studio source tables
    trust.set(
        "get_sources",
        lua.create_function(|lua, ()| {
            let state = crate::state::get_state();
            let sources = state.studio_sources.read();
            let arr = lua.create_table()?;
            for (idx, s) in sources.iter().enumerate() {
                let tbl = lua.create_table()?;
                tbl.set("name", s.name.clone())?;
                tbl.set("scene", s.scene.clone())?;
                tbl.set("slot", s.slot)?;
                tbl.set("x", s.x)?;
                tbl.set("y", s.y)?;
                tbl.set("width", s.width)?;
                tbl.set("height", s.height)?;
                tbl.set("roll", s.roll)?;
                tbl.set("pitch", s.pitch)?;
                tbl.set("yaw", s.yaw)?;
                tbl.set("is_target", s.is_target)?;
                tbl.set("match_obs", s.match_obs)?;
                tbl.set("is_visible", s.is_visible)?;
                tbl.set("default_x", s.default_x)?;
                tbl.set("default_y", s.default_y)?;
                tbl.set("default_width", s.default_width)?;
                tbl.set("default_height", s.default_height)?;
                tbl.set("default_roll", s.default_roll)?;

                let last_pose_opt = crate::studio_app::LAST_SENT_POSE.read().get(&s.name).cloned()
                    .or_else(|| PRE_REDEEM_POSES.lock().get(&s.name).cloned());
                if let Some(ref lp) = last_pose_opt {
                    tbl.set("last_pose_x", lp.x)?;
                    tbl.set("last_pose_y", lp.y)?;
                    tbl.set("last_pose_width", lp.width)?;
                    tbl.set("last_pose_height", lp.height)?;
                    tbl.set("last_pose_roll", lp.roll)?;
                    tbl.set("last_pose_pitch", lp.pitch)?;
                    tbl.set("last_pose_yaw", lp.yaw)?;
                    tbl.set("last_pose_visible", lp.is_visible)?;
                } else {
                    tbl.set("last_pose_x", s.x)?;
                    tbl.set("last_pose_y", s.y)?;
                    tbl.set("last_pose_width", s.width)?;
                    tbl.set("last_pose_height", s.height)?;
                    tbl.set("last_pose_roll", s.roll)?;
                    tbl.set("last_pose_pitch", s.pitch)?;
                    tbl.set("last_pose_yaw", s.yaw)?;
                    tbl.set("last_pose_visible", s.is_visible)?;
                }

                arr.set(idx + 1, tbl)?;
            }
            Ok(arr)
        })?,
    )?;

    // 14. trust.set_source_transform(source_name, opts_table) -> bool
    trust.set(
        "set_source_transform",
        lua.create_function(|_, (source_name, opts): (String, Table)| {
            let state = crate::state::get_state();
            let mut sources = state.studio_sources.write();
            if let Some(s) = sources.iter_mut().find(|s| s.name.eq_ignore_ascii_case(&source_name)) {
                // Ensure pre-redeem snapshot is preserved before mutation
                {
                    let mut pre = PRE_REDEEM_POSES.lock();
                    pre.entry(s.name.clone()).or_insert_with(|| crate::studio_app::SourceTransformSnapshot {
                        source_name: s.name.clone(),
                        x: s.x,
                        y: s.y,
                        width: s.width,
                        height: s.height,
                        roll: s.roll,
                        pitch: s.pitch,
                        yaw: s.yaw,
                        is_visible: s.is_visible,
                        filters: None,
                        volume: None,
                    });
                }
                if let Ok(v) = opts.get::<f32>("x") { s.x = v; }
                if let Ok(v) = opts.get::<f32>("y") { s.y = v; }
                if let Ok(v) = opts.get::<f32>("width") { s.width = v; }
                if let Ok(v) = opts.get::<f32>("height") { s.height = v; }
                if let Ok(v) = opts.get::<f32>("roll") { s.roll = v; }
                if let Ok(v) = opts.get::<f32>("pitch") { s.pitch = v; }
                if let Ok(v) = opts.get::<f32>("yaw") { s.yaw = v; }
                if let Ok(v) = opts.get::<bool>("is_visible") { s.is_visible = v; }
                Ok(true)
            } else {
                Ok(false)
            }
        })?,
    )?;

    // 15. trust.move_source(source_name, x, y) -> bool
    trust.set(
        "move_source",
        lua.create_function(|_, (source_name, x, y): (String, f32, f32)| {
            let state = crate::state::get_state();
            let mut sources = state.studio_sources.write();
            if let Some(s) = sources.iter_mut().find(|s| s.name.eq_ignore_ascii_case(&source_name)) {
                {
                    let mut pre = PRE_REDEEM_POSES.lock();
                    pre.entry(s.name.clone()).or_insert_with(|| crate::studio_app::SourceTransformSnapshot {
                        source_name: s.name.clone(),
                        x: s.x,
                        y: s.y,
                        width: s.width,
                        height: s.height,
                        roll: s.roll,
                        pitch: s.pitch,
                        yaw: s.yaw,
                        is_visible: s.is_visible,
                        filters: None,
                        volume: None,
                    });
                }
                s.x = x;
                s.y = y;
                Ok(true)
            } else {
                Ok(false)
            }
        })?,
    )?;

    // 16. trust.reset_source(source_name) -> bool (Restores to last pose sent)
    trust.set(
        "reset_source",
        lua.create_function(|_, source_name: String| {
            let state = crate::state::get_state();
            let mut sources = state.studio_sources.write();
            let mut updated_src: Option<crate::StudioSource> = None;
            if let Some(s) = sources.iter_mut().find(|s| s.name.eq_ignore_ascii_case(&source_name)) {
                restore_source_to_last_pose(s);
                updated_src = Some(s.clone());
            }
            drop(sources);
            if let Some(src) = updated_src {
                crate::queue_studio_transform_update(&src);
                Ok(true)
            } else {
                Ok(false)
            }
        })?,
    )?;

    // 17. trust.reset_sources() -> usize (Restores all sources to last pose sent)
    trust.set(
        "reset_sources",
        lua.create_function(|_, ()| {
            let state = crate::state::get_state();
            let mut sources = state.studio_sources.write();
            let mut count = 0;
            let mut updated_sources = Vec::new();
            for s in sources.iter_mut() {
                restore_source_to_last_pose(s);
                updated_sources.push(s.clone());
                count += 1;
            }
            drop(sources);
            for src in updated_sources {
                crate::queue_studio_transform_update(&src);
            }
            Ok(count)
        })?,
    )?;

    // 18. trust.save_source_poses() -> usize (Explicitly snapshots current poses)
    trust.set(
        "save_source_poses",
        lua.create_function(|_, ()| {
            let state = crate::state::get_state();
            let sources = state.studio_sources.read();
            let mut pre = PRE_REDEEM_POSES.lock();
            let mut last_pose = crate::studio_app::LAST_SENT_POSE.write();
            for s in sources.iter() {
                let snap = crate::studio_app::SourceTransformSnapshot {
                    source_name: s.name.clone(),
                    x: s.x,
                    y: s.y,
                    width: s.width,
                    height: s.height,
                    roll: s.roll,
                    pitch: s.pitch,
                    yaw: s.yaw,
                    is_visible: s.is_visible,
                    filters: None,
                    volume: None,
                };
                pre.insert(s.name.clone(), snap.clone());
                last_pose.insert(s.name.clone(), snap);
            }
            Ok(sources.len())
        })?,
    )?;

    lua.globals().set("trust", trust)?;
    Ok(())
}

static SCRIPT_CACHE: std::sync::LazyLock<parking_lot::RwLock<std::collections::HashMap<String, (std::time::SystemTime, String)>>> =
    std::sync::LazyLock::new(|| parking_lot::RwLock::new(std::collections::HashMap::new()));

fn get_or_load_script_code(script_path: &str) -> Result<String, String> {
    let path = std::path::Path::new(script_path);
    let mtime = std::fs::metadata(path)
        .and_then(|m| m.modified())
        .unwrap_or(std::time::SystemTime::UNIX_EPOCH);

    {
        let cache = SCRIPT_CACHE.read();
        if let Some((cached_mtime, code)) = cache.get(script_path) {
            if *cached_mtime == mtime {
                return Ok(code.clone());
            }
        }
    }

    let code = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            let err = format!("[Trust Lua] ❌ Failed to read custom script '{}': {}", script_path, e);
            eprintln!("{}", err);
            return Err(err);
        }
    };

    let mut cache = SCRIPT_CACHE.write();
    cache.insert(script_path.to_string(), (mtime, code.clone()));
    Ok(code)
}

/// Executes a custom Lua redeem script synchronously in the current thread.
/// (Used by worker threads and unit tests).
pub fn execute_custom_script_sync(script_path: &str, params: serde_json::Value) -> Result<(), String> {
    let script_path = &crate::studio_app::resolve_script_path(script_path);
    let code = get_or_load_script_code(script_path)?;

    // Pre-snapshot current source poses so reset_source(s) can restore accurately
    {
        let state = crate::state::get_state();
        let sources = state.studio_sources.read();
        let mut pre = PRE_REDEEM_POSES.lock();
        pre.clear();
        for s in sources.iter() {
            pre.insert(s.name.clone(), crate::studio_app::SourceTransformSnapshot {
                source_name: s.name.clone(),
                x: s.x,
                y: s.y,
                width: s.width,
                height: s.height,
                roll: s.roll,
                pitch: s.pitch,
                yaw: s.yaw,
                is_visible: s.is_visible,
                filters: None,
                volume: None,
            });
        }
    }

    // No files, programs or loading other code (lua_shield)
    let lua = match crate::lua_shield::sandboxed_lua() {
        Ok(lua) => lua,
        Err(e) => {
            let err = format!("[Trust Lua] ❌ Failed to create the script's Lua: {}", e);
            eprintln!("{}", err);
            return Err(err);
        }
    };
    lua.set_app_data(RunningScript(std::path::PathBuf::from(script_path)));

    if let Err(e) = setup_trust_env(&lua) {
        let err = format!("[Trust Lua] ❌ Failed to setup Lua environment: {}", e);
        eprintln!("{}", err);
        return Err(err);
    }

    let lua_params = match lua.to_value(&params) {
        Ok(v) => v,
        Err(e) => {
            let err = format!("[Trust Lua] ❌ Failed to convert params to Lua: {}", e);
            eprintln!("{}", err);
            return Err(err);
        }
    };

    // Inject params into globals so procedural scripts can access `params.target` directly
    if let Err(e) = lua.globals().set("params", lua_params.clone()) {
        eprintln!("[Trust Lua] ⚠️ Failed to set global params: {}", e);
    }

    // Execute script chunk
    if let Err(e) = lua.load(&code).set_name(script_path).exec() {
        let err = format!("[Trust Lua] ❌ Error executing script '{}': {}", script_path, e);
        eprintln!("{}", err);
        return Err(err);
    }

    // If the script defines `on_redeem(params)`, invoke it:
    let globals = lua.globals();
    if let Ok(on_redeem) = globals.get::<mlua::Function>("on_redeem") {
        if let Err(e) = on_redeem.call::<()>(lua_params) {
            let err = format!("[Trust Lua] ❌ Error in on_redeem() for '{}': {}", script_path, e);
            eprintln!("{}", err);
            return Err(err);
        }
    }

    Ok(())
}

pub(crate) struct ScriptGuard;

impl ScriptGuard {
    pub fn new() -> Self {
        ACTIVE_CUSTOM_SCRIPTS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Self
    }
}

impl Drop for ScriptGuard {
    fn drop(&mut self) {
        ACTIVE_CUSTOM_SCRIPTS.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
        PRE_REDEEM_POSES.lock().clear();
    }
}

/// Executes a custom Lua redeem script on a dedicated worker thread so that
/// any sleep calls or long animations do not block the UI or OBS frame loops.
pub fn run_custom_redeem_script(script_path: &str, params: serde_json::Value) {
    let path = script_path.to_string();
    std::thread::spawn(move || {
        let _guard = ScriptGuard::new();
        // stderr isn't captured by OBS on Windows
        if let Err(e) = execute_custom_script_sync(&path, params) {
            crate::obs_source::trust_log(&e);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_custom_lua_env_and_helpers() {
        let lua = Lua::new();
        setup_trust_env(&lua).unwrap();

        // Test canvas size query
        let result: (u32, u32) = lua.load("return trust.get_canvas_size()").eval().unwrap();
        assert!(result.0 > 0 || result.0 == 0);

        // Test sleep
        lua.load("trust.sleep(0.001)").exec().unwrap();
    }

    #[test]
    fn test_custom_lua_on_redeem_invoked() {
        let lua = Lua::new();
        setup_trust_env(&lua).unwrap();

        let script = r#"
            triggered_target = ""
            function on_redeem(params)
                triggered_target = params.target
            end
        "#;
        lua.load(script).exec().unwrap();

        let params = serde_json::json!({
            "target": "TestAvatar",
            "trigger": "Twitch Mortygotvr redeem Test"
        });
        let lua_params = lua.to_value(&params).unwrap();
        let on_redeem: mlua::Function = lua.globals().get("on_redeem").unwrap();
        on_redeem.call::<()>(lua_params).unwrap();

        let target: String = lua.globals().get("triggered_target").unwrap();
        assert_eq!(target, "TestAvatar");
    }

    #[test]
    fn test_custom_lua_dispatch_helpers() {
        let lua = Lua::new();
        setup_trust_env(&lua).unwrap();

        // Calling trust.trigger with a table should run dispatch_test_redeem_json
        let script = r#"
            local ok1 = trust.trigger({
                type = "reaction",
                target = "Image 3",
                impactReactions = { bounce = { enabled = true } }
            })
            local ok2 = trust.trigger_reaction({
                target = "Image 3",
                impactReactions = { bounce = { enabled = true } }
            })
            return ok1, ok2
        "#;
        let (ok1, ok2): (bool, bool) = lua.load(script).eval().unwrap();
        assert!(ok1);
        assert!(ok2);
    }

    #[test]
    fn test_snow_storm_script_syntax_and_execution() {
        let p_buf = crate::paths::scripts_dir().join("snow_storm.lua");
        let path = p_buf.to_str().unwrap();
        let code = std::fs::read_to_string(path).expect("snow_storm.lua must exist");
        let lua = Lua::new();
        setup_trust_env(&lua).unwrap();
        lua.load(&code).exec().expect("snow_storm.lua syntax must be valid");
        let _on_redeem: mlua::Function = lua.globals().get("on_redeem").expect("snow_storm.lua must define on_redeem");

        // Verify executing snow_storm.lua terminates cleanly and does not loop infinitely
        let params = serde_json::json!({
            "target": "Image",
            "trigger": "Twitch Mortygotvr redeem Snow",
            "duration": 0.2, // 200ms -> 2 bursts
            "speed": 5.0,
            "direction": 90.0,
            "scale": 0.6,
            "gravity": 0.5,
            "collision": "bounce"
        });
        execute_custom_script_sync(path, params).expect("snow_storm.lua execution must succeed");
    }

    #[test]
    fn test_snow_storm_no_unwanted_water_and_no_bouncing() {
        let p_buf = crate::paths::scripts_dir().join("snow_storm.lua");
        let path = p_buf.to_str().unwrap();
        let code = std::fs::read_to_string(path).expect("snow_storm.lua must exist");
        let lua = Lua::new();
        setup_trust_env(&lua).unwrap();

        let water_count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let wc = water_count.clone();
        let trust: mlua::Table = lua.globals().get("trust").unwrap();
        trust.set("spawn_water", lua.create_function(move |_, _opts: mlua::Table| {
            wc.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(true)
        }).unwrap()).unwrap();

        let bounced_count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let bc = bounced_count.clone();
        trust.set("spawn_throwable", lua.create_function(move |_, opts: mlua::Table| {
            if let Ok(col) = opts.get::<String>("collision") {
                if col == "bounce" {
                    bc.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                }
            }
            Ok(true)
        }).unwrap()).unwrap();

        lua.load(&code).exec().expect("snow_storm.lua syntax must be valid");
        let on_redeem: mlua::Function = lua.globals().get("on_redeem").unwrap();

        let params = lua.create_table().unwrap();
        params.set("target", "Image").unwrap();
        params.set("duration", 0.2).unwrap();
        params.set("collision", "bounce").unwrap(); // Default from redeem config!

        on_redeem.call::<()>(params).unwrap();

        // 0 water streams must be spawned by default
        assert_eq!(water_count.load(std::sync::atomic::Ordering::SeqCst), 0, "Snow storm must not spawn unwanted water simulations!");
        // 0 snowflakes must bounce
        assert_eq!(bounced_count.load(std::sync::atomic::Ordering::SeqCst), 0, "Snowflakes must adhere/melt, not bounce!");
    }

    #[test]
    fn test_custom_redeem_fallback_dispatch() {
        let json = r#"{
            "type": "custom",
            "target": "Image",
            "triggers": ["Twitch * redeem Snow"]
        }"#;
        assert!(crate::studio_app::dispatch_test_redeem_json(json));
    }

    #[test]
    fn test_custom_lua_spawn_water_with_metaball_and_contact_sound() {
        let lua = Lua::new();
        setup_trust_env(&lua).unwrap();

        let script = r#"
            local ok = trust.spawn_water({
                target = "Image",
                count = 60,
                radius = 12.0,
                viscosity = 0.8,
                gravity = 0.5,
                duration = 3.0,
                metaball_size = 10.5,
                round_edge = true,
                refraction = false,
                contact_sound = "splash.wav",
                contact_sound_cooldown = 150,
                contact_sound_first_only = true,
                color = { 0.1, 0.7, 0.9 },
                start_x = 100,
                start_y = 50,
                target_x = 500,
                target_y = 500
            })
            return ok
        "#;
        let ok: bool = lua.load(script).eval().unwrap();
        assert!(ok);

        let state = crate::state::get_state();
        let effects = state.effects.read();
        assert!(!effects.fluid_engines.is_empty());
        let last_eng = effects.fluid_engines.last().unwrap();
        assert_eq!(last_eng.config.particle_count, 60);
        assert!((last_eng.config.h - 12.0).abs() < 1e-3);
        assert!((last_eng.config.viscosity - 0.8).abs() < 1e-3);
        assert!((last_eng.config.water_visual.metaball_size - 10.5).abs() < 1e-3);
        assert!(last_eng.config.water_visual.round_edge);
        assert!(!last_eng.config.water_visual.refraction_enabled);
        assert_eq!(last_eng.config.contact_sound.as_deref(), Some("splash.wav"));
        assert!((last_eng.config.contact_sound_cooldown_ms - 150.0).abs() < 1e-3);
        assert!(last_eng.config.contact_sound_first_only);
    }

    #[test]
    fn test_script_paths_look_next_to_the_script_first() {
        let dir = std::env::temp_dir().join(format!("trust_localize_{}", std::process::id()));
        std::fs::create_dir_all(dir.join("filters")).unwrap();
        std::fs::write(dir.join("filters").join("mine.lua"), "-- x").unwrap();
        let lua = Lua::new();
        lua.set_app_data(RunningScript(dir.join("main.lua")));
        let mut val = serde_json::json!({
            "impactReactions": { "custom_script": { "path": "filters/mine.lua" } },
            "target": "Avatar",
            "sound": "missing.mp3",
            "list": ["filters/mine.lua"],
        });
        localize_paths(&lua, &mut val);
        let mine = dir.join("filters/mine.lua").to_string_lossy().into_owned();
        assert_eq!(val["impactReactions"]["custom_script"]["path"], mine.as_str());
        assert_eq!(val["list"][0], mine.as_str());
        assert_eq!(val["target"], "Avatar");
        assert_eq!(val["sound"], "missing.mp3");
        // Not from a script: left to the usual lookup
        let mut untouched = serde_json::json!({ "path": "filters/mine.lua" });
        localize_paths(&Lua::new(), &mut untouched);
        assert_eq!(untouched["path"], "filters/mine.lua");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_doom_script_swirl_and_pull() {
        let p_buf = crate::paths::scripts_dir().join("doom.lua");
        let path = p_buf.to_str().unwrap();
        if !std::path::Path::new(path).is_file() {
            return;
        }

        let state = crate::state::get_state();
        {
            let mut sources = state.studio_sources.write();
            sources.clear();
            sources.push(crate::StudioSource {
                scene: "Scène".to_string(),
                name: "TargetAvatar".to_string(),
                slot: 1,
                x: 800.0,
                y: 400.0,
                width: 300.0,
                height: 400.0,
                roll: 0.0,
                pitch: 0.0,
                yaw: 0.0,
                is_target: true,
                match_obs: false,
                is_visible: true,
                file_path: String::new(),
                default_x: 800.0,
                default_y: 400.0,
                default_width: 300.0,
                default_height: 400.0,
                default_roll: 0.0,
            });
            sources.push(crate::StudioSource {
                scene: "Scène".to_string(),
                name: "VictimSource".to_string(),
                slot: 2,
                x: 100.0,
                y: 100.0,
                width: 200.0,
                height: 200.0,
                roll: 0.0,
                pitch: 0.0,
                yaw: 0.0,
                is_target: false,
                match_obs: false,
                is_visible: true,
                file_path: String::new(),
                default_x: 100.0,
                default_y: 100.0,
                default_width: 200.0,
                default_height: 200.0,
                default_roll: 0.0,
            });
        }

        let params = serde_json::json!({
            "target": "TargetAvatar",
            "trigger": "Twitch Mortygotvr redeem Doom",
            "duration": 0.2, // 200ms quick test duration
            "speed": 8.0,
        });
        execute_custom_script_sync(path, params).expect("doom.lua execution must succeed");

        // Verify that after execution, sources are properly restored to their default state
        let sources = state.studio_sources.read();
        let victim = sources.iter().find(|s| s.name == "VictimSource").expect("VictimSource must exist");
        assert_eq!(victim.x, 100.0);
        assert_eq!(victim.y, 100.0);
        assert_eq!(victim.width, 200.0);
        assert_eq!(victim.height, 200.0);
        // Tumbled on the way in, upright again afterwards
        assert_eq!(victim.pitch, 0.0);
        assert_eq!(victim.yaw, 0.0);
        assert!(victim.is_visible);
    }

    #[test]
    fn test_custom_lua_sources_query_and_transform() {
        let state = crate::state::get_state();
        let test_source = crate::StudioSource {
            scene: "Scène".to_string(),
            name: "TestAvatar2".to_string(),
            slot: 2,
            x: 200.0,
            y: 300.0,
            width: 400.0,
            height: 500.0,
            roll: 0.0,
            pitch: 0.0,
            yaw: 0.0,
            is_target: false,
            match_obs: false,
            is_visible: true,
            file_path: String::new(),
            default_x: 200.0,
            default_y: 300.0,
            default_width: 400.0,
            default_height: 500.0,
            default_roll: 0.0,
        };
        state.studio_sources.write().push(test_source);

        let lua = Lua::new();
        setup_trust_env(&lua).unwrap();

        let script = r#"
            local sources = trust.get_sources()
            assert(#sources > 0, "must return at least one source")
            local found = false
            for _, s in ipairs(sources) do
                if s.name == "TestAvatar2" then
                    found = true
                    assert(s.x == 200.0, "initial x must be 200")
                end
            end
            assert(found, "TestAvatar2 must be in sources list")

            -- Update transform
            local ok = trust.set_source_transform("TestAvatar2", {
                x = 650.0,
                y = 800.0,
                width = 200.0,
                height = 250.0,
                roll = 45.0,
                is_visible = false
            })
            assert(ok, "set_source_transform must return true")

            return true
        "#;
        let res: bool = lua.load(script).eval().unwrap();
        assert!(res);

        {
            let sources = state.studio_sources.read();
            let s = sources.iter().find(|s| s.name == "TestAvatar2").unwrap();
            assert_eq!(s.x, 650.0);
            assert_eq!(s.y, 800.0);
            assert_eq!(s.width, 200.0);
            assert_eq!(s.height, 250.0);
            assert_eq!(s.roll, 45.0);
            assert!(!s.is_visible);
        }

        // Test reset
        let reset_script = r#"
            local count = trust.reset_sources()
            assert(count > 0, "reset_sources must reset at least one source")
            return count
        "#;
        let count: usize = lua.load(reset_script).eval().unwrap();
        assert!(count > 0);

        {
            let sources = state.studio_sources.read();
            let s = sources.iter().find(|s| s.name == "TestAvatar2").unwrap();
            assert_eq!(s.x, 200.0);
            assert_eq!(s.y, 300.0);
            assert_eq!(s.width, 400.0);
            assert_eq!(s.height, 500.0);
            assert_eq!(s.roll, 0.0);
            assert!(s.is_visible);
        }
    }

    #[test]
    fn test_reset_sources_restores_last_pose_sent_not_default() {
        let state = crate::state::get_state();
        let src_name = "PosedAvatar";

        // Initial default coords in OBS are (0, 0)
        let posed_source = crate::StudioSource {
            scene: "Scène".to_string(),
            name: src_name.to_string(),
            slot: 3,
            x: 777.0,
            y: 888.0,
            width: 333.0,
            height: 444.0,
            roll: 15.0,
            pitch: 0.0,
            yaw: 0.0,
            is_target: false,
            match_obs: false,
            is_visible: true,
            file_path: String::new(),
            default_x: 0.0,
            default_y: 0.0,
            default_width: 100.0,
            default_height: 100.0,
            default_roll: 0.0,
        };

        {
            let mut sources = state.studio_sources.write();
            sources.retain(|s| s.name != src_name);
            sources.push(posed_source);
        }

        // Simulate sending a pose (e.g. Pose 1 / coordinate set)
        crate::studio_app::update_last_sent_pose_for_source(src_name, 777.0, 888.0, 333.0, 444.0, 15.0, 0.0, 0.0, true);

        let lua = Lua::new();
        setup_trust_env(&lua).unwrap();

        // Mutate the source during redeem (pull into black hole)
        let mutate_script = r#"
            trust.set_source_transform("PosedAvatar", {
                x = 0.0,
                y = 0.0,
                width = 1.0,
                height = 1.0,
                roll = 720.0,
                is_visible = false
            })
            return true
        "#;
        let ok: bool = lua.load(mutate_script).eval().unwrap();
        assert!(ok);

        {
            let sources = state.studio_sources.read();
            let s = sources.iter().find(|s| s.name == src_name).unwrap();
            assert_eq!(s.x, 0.0);
            assert_eq!(s.width, 1.0);
            assert!(!s.is_visible);
        }

        // Now call reset_sources
        let reset_script = r#"
            return trust.reset_sources()
        "#;
        let count: usize = lua.load(reset_script).eval().unwrap();
        assert!(count > 0);

        // Verify it restored to the LAST SENT POSE (777, 888, 333x444, roll 15), NOT default_x (0, 0)!
        {
            let sources = state.studio_sources.read();
            let s = sources.iter().find(|s| s.name == src_name).unwrap();
            assert_eq!(s.x, 777.0);
            assert_eq!(s.y, 888.0);
            assert_eq!(s.width, 333.0);
            assert_eq!(s.height, 444.0);
            assert_eq!(s.roll, 15.0);
            assert!(s.is_visible);
        }
    }

    #[test]
    fn test_custom_script_active_flag_and_guard() {
        let initial = ACTIVE_CUSTOM_SCRIPTS.load(std::sync::atomic::Ordering::SeqCst);

        {
            let _guard = ScriptGuard::new();
            assert_eq!(ACTIVE_CUSTOM_SCRIPTS.load(std::sync::atomic::Ordering::SeqCst), initial + 1);
            assert!(is_custom_script_active());

            // Simulate saving a source pose during script
            PRE_REDEEM_POSES.lock().insert("VictimSource".to_string(), crate::studio_app::SourceTransformSnapshot {
                source_name: "VictimSource".to_string(),
                x: 10.0,
                y: 20.0,
                width: 100.0,
                height: 100.0,
                roll: 0.0,
                pitch: 0.0,
                yaw: 0.0,
                is_visible: true,
                filters: None,
                volume: None,
            });
            assert!(is_source_under_script("VictimSource"));
            assert!(!is_source_under_script("OtherSource"));
        }

        // Guard dropped: active count returns to initial and pre-redeem poses cleared
        assert_eq!(ACTIVE_CUSTOM_SCRIPTS.load(std::sync::atomic::Ordering::SeqCst), initial);
        assert!(!is_source_under_script("VictimSource"));
    }
}

