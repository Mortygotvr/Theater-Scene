//! Theater Scene's OBS procedures: how the Control Panel (or any other plugin)
//! lists and fires poses and redeems, directly through OBS's global procedure
//! handler — no WebSocket. Registered by the OBS plugin copy of Theater Scene
//! only (obs_module_load), which runs the Studio, the docks and the Throwables
//! Layer; a copy a Lua script loads never registers them.
//!
//! - `theater_scene_list(out string json)`:
//!   `{"poses":[{"id","name","scene"}],"redeems":[{"id","name"}]}`
//! - `theater_scene_fire(in string id, in string event, out bool ok, out string result)`:
//!   fires the pose or redeem with that id, like the dock buttons do. `event`
//!   is optional JSON with the firing event's details, `{"user","wildcards":[…]}`.
//!   A pose saved for another scene than the one live in OBS is skipped.
//!   `result` says what happened.
//! - signal `theater_scene_list_changed()` on OBS's global signal handler, sent
//!   when poses or redeems change.
//! - `theater_scene_play_sound(in string path, in float volume, out bool ok, out string result)`:
//!   plays a sound file through the Throwables Layer (which only the plugin
//!   copy has). How the Lua-loaded copy plays its redeems' sounds.
//!
//! The chat reader's signal and procedures (`theater_reader_*`) are Theater
//! Reader's, a plugin of its own (repo Theater-Reader). Theater only asks it which
//! chats it reads, for the trigger lists (reader_chats).

use std::ffi::{c_char, c_void, CStr, CString};
use std::sync::atomic::{AtomicBool, Ordering};

use crate::studio_app::{self, GLOBAL_REDEEMS, SCENE_COORDINATE_SETS};

const LIST_DECL: &CStr = c"void theater_scene_list(out string json)";
const FIRE_DECL: &CStr = c"void theater_scene_fire(in string id, in string event, out bool ok, out string result)";
const VISIBILITY_DECL: &CStr = c"void theater_scene_set_visibility(in string scene, in string source, in string action, in string transition, in int duration, in string settings, out bool ok, out string result)";
const PLAY_SOUND_DECL: &CStr = c"void theater_scene_play_sound(in string path, in float volume, out bool ok, out string result)";
const PLAY_SOUND: &CStr = c"theater_scene_play_sound";
const LIST_CHANGED_DECL: &CStr = c"void theater_scene_list_changed()";
const LIST_CHANGED: &CStr = c"theater_scene_list_changed";
/// Each message sent through the Theater plugins' relay (theater_shared::relay)
const THEATER_EVENT_DECL: &CStr = c"void theater_event(string json)";
const THEATER_EVENT: &CStr = c"theater_event";
/// The Theater plugin hosting the relay in OBS registers it
const RELAY_HOST: &CStr = c"theater_relay_host";
/// Theater Reader's (its plugin registers it)
const READER_STATE: &CStr = c"theater_reader_state";

/// libobs's calldata_t.
#[repr(C)]
struct Calldata {
    stack: *mut u8,
    size: usize,
    capacity: usize,
    fixed: bool,
}

type ProcFn = unsafe extern "C" fn(data: *mut c_void, cd: *mut Calldata);

struct Obs {
    proc_handler_add: unsafe extern "C" fn(*mut c_void, *const c_char, ProcFn, *mut c_void),
    proc_handler_call: unsafe extern "C" fn(*mut c_void, *const c_char, *mut Calldata) -> bool,
    signal_handler_add: unsafe extern "C" fn(*mut c_void, *const c_char) -> bool,
    signal_handler_signal: unsafe extern "C" fn(*mut c_void, *const c_char, *mut Calldata),
    signal_handler_connect: unsafe extern "C" fn(*mut c_void, *const c_char, ProcFn, *mut c_void),
    calldata_get_string: unsafe extern "C" fn(*const Calldata, *const c_char, *mut *const c_char) -> bool,
    calldata_get_data: unsafe extern "C" fn(*const Calldata, *const c_char, *mut c_void, usize) -> bool,
    calldata_set_data: unsafe extern "C" fn(*mut Calldata, *const c_char, *const c_void, usize),
    bfree: unsafe extern "C" fn(*mut c_void),
    proc_handler: *mut c_void,
    signal_handler: *mut c_void,
}

// The handlers are OBS's global ones, which live as long as OBS.
unsafe impl Send for Obs {}
unsafe impl Sync for Obs {}

static OBS: std::sync::OnceLock<Obs> = std::sync::OnceLock::new();
/// This copy of Theater Scene registered the procedures (and may signal).
static REGISTERED: AtomicBool = AtomicBool::new(false);

fn load() -> Option<Obs> {
    fn sym<T>(name: &CStr) -> Option<T> {
        let p = crate::obs_source::obs_symbol(name);
        // SAFETY: T is the function pointer type of the libobs export `name`.
        (!p.is_null()).then(|| unsafe { std::mem::transmute_copy(&p) })
    }
    let get_proc_handler: unsafe extern "C" fn() -> *mut c_void = sym(c"obs_get_proc_handler")?;
    let get_signal_handler: unsafe extern "C" fn() -> *mut c_void = sym(c"obs_get_signal_handler")?;
    let proc_handler = unsafe { get_proc_handler() };
    let signal_handler = unsafe { get_signal_handler() };
    if proc_handler.is_null() || signal_handler.is_null() {
        return None;
    }
    Some(Obs {
        proc_handler_add: sym(c"proc_handler_add")?,
        proc_handler_call: sym(c"proc_handler_call")?,
        signal_handler_add: sym(c"signal_handler_add")?,
        signal_handler_signal: sym(c"signal_handler_signal")?,
        signal_handler_connect: sym(c"signal_handler_connect")?,
        calldata_get_string: sym(c"calldata_get_string")?,
        calldata_get_data: sym(c"calldata_get_data")?,
        calldata_set_data: sym(c"calldata_set_data")?,
        bfree: sym(c"bfree")?,
        proc_handler,
        signal_handler,
    })
}

/// Registers the procedures and the signal. Call from obs_module_load.
pub fn register() {
    if OBS.get().is_none() {
        if let Some(obs) = load() {
            let _ = OBS.set(obs);
        }
    }
    let Some(obs) = OBS.get() else {
        crate::obs_source::trust_log("[Trust Core] ⚠️ OBS procedure handler not available; no Control Panel link");
        return;
    };
    unsafe {
        (obs.signal_handler_add)(obs.signal_handler, LIST_CHANGED_DECL.as_ptr());
        (obs.proc_handler_add)(obs.proc_handler, LIST_DECL.as_ptr(), list_proc, std::ptr::null_mut());
        (obs.proc_handler_add)(obs.proc_handler, FIRE_DECL.as_ptr(), fire_proc, std::ptr::null_mut());
        (obs.proc_handler_add)(obs.proc_handler, VISIBILITY_DECL.as_ptr(), visibility_proc, std::ptr::null_mut());
        (obs.proc_handler_add)(obs.proc_handler, PLAY_SOUND_DECL.as_ptr(), play_sound_proc, std::ptr::null_mut());
        // The Theater API plugins' messages when no Theater plugin hosts the relay
        (obs.signal_handler_add)(obs.signal_handler, THEATER_EVENT_DECL.as_ptr());
        (obs.signal_handler_connect)(obs.signal_handler, THEATER_EVENT.as_ptr(), theater_event_signal, std::ptr::null_mut());
    }
    REGISTERED.store(true, Ordering::SeqCst);
    crate::obs_source::trust_log(
        "[Trust Core] 🔗 Registered theater_scene_list / theater_scene_fire / theater_scene_set_visibility / theater_scene_play_sound OBS procedures.",
    );
}

/// OBS's handlers, loaded on first use in a copy that didn't register.
fn obs() -> Option<&'static Obs> {
    if OBS.get().is_none() {
        if let Some(obs) = load() {
            let _ = OBS.set(obs);
        }
    }
    OBS.get()
}

/// Plays a sound through the plugin copy's Throwables Layer, with
/// `theater_scene_play_sound`. None when that procedure isn't there (the
/// plugin copy isn't loaded). Decodes on the calling thread.
pub fn call_play_sound(path: &std::path::Path, volume: f32) -> Option<Result<(), String>> {
    let obs = obs()?;
    let mut cd = Calldata { stack: std::ptr::null_mut(), size: 0, capacity: 0, fixed: false };
    set_string(obs, &mut cd, c"path", &path.to_string_lossy());
    let volume = volume as f64; // an OBS procedure's "float" is a double
    unsafe { (obs.calldata_set_data)(&mut cd, c"volume".as_ptr(), &volume as *const f64 as *const c_void, size_of::<f64>()) };
    let called = unsafe { (obs.proc_handler_call)(obs.proc_handler, PLAY_SOUND.as_ptr(), &mut cd) };
    let result = called.then(|| {
        let mut ok = false;
        unsafe { (obs.calldata_get_data)(&cd, c"ok".as_ptr(), &mut ok as *mut bool as *mut c_void, size_of::<bool>()) };
        if ok { Ok(()) } else { Err(get_string(obs, &mut cd, c"result")) }
    });
    if !cd.stack.is_null() {
        unsafe { (obs.bfree)(cd.stack as *mut c_void) };
    }
    result
}

/// Tells listeners (the Control Panel) that poses or redeems changed. Any thread.
pub fn notify_list_changed() {
    if !REGISTERED.load(Ordering::SeqCst) {
        return;
    }
    let Some(obs) = OBS.get() else { return };
    let mut cd = Calldata { stack: std::ptr::null_mut(), size: 0, capacity: 0, fixed: false };
    unsafe {
        (obs.signal_handler_signal)(obs.signal_handler, LIST_CHANGED.as_ptr(), &mut cd);
        if !cd.stack.is_null() {
            (obs.bfree)(cd.stack as *mut c_void);
        }
    }
}

/// The chats Theater Reader reads, as (platform, channel), from its
/// `theater_reader_state` procedure. Empty when that plugin isn't installed
/// (or reads none). Any thread.
pub fn reader_chats() -> Vec<(theater_shared::Platform, String)> {
    let Some(obs) = obs() else { return Vec::new() };
    let mut cd = Calldata { stack: std::ptr::null_mut(), size: 0, capacity: 0, fixed: false };
    let called = unsafe { (obs.proc_handler_call)(obs.proc_handler, READER_STATE.as_ptr(), &mut cd) };
    let json = if called { get_string(obs, &mut cd, c"json") } else { String::new() };
    if !cd.stack.is_null() {
        unsafe { (obs.bfree)(cd.stack as *mut c_void) };
    }
    parse_reader_chats(&json)
}

/// Whether a Theater plugin (Theater Reader's) hosts the relay in OBS: then
/// every message, the Theater API plugins' too, comes by the relay port.
/// Any thread.
pub fn relay_host_in_obs() -> bool {
    let Some(obs) = obs() else { return false };
    let mut cd = Calldata { stack: std::ptr::null_mut(), size: 0, capacity: 0, fixed: false };
    let called = unsafe { (obs.proc_handler_call)(obs.proc_handler, RELAY_HOST.as_ptr(), &mut cd) };
    if !cd.stack.is_null() {
        unsafe { (obs.bfree)(cd.stack as *mut c_void) };
    }
    called
}

/// A Theater plugin's message inside OBS. Without a Theater plugin hosting
/// the relay, the Theater API plugins send only this way (the port is the
/// Theater Reader app's, or nobody's): it goes where a relay message goes,
/// to the triggers and the Rhai sources. With one, the same message comes by
/// the port, so this one is left out.
unsafe extern "C" fn theater_event_signal(_data: *mut c_void, cd: *mut Calldata) {
    let Some(obs) = OBS.get() else { return };
    let json = get_string(obs, cd, c"json");
    if json.is_empty() || relay_host_in_obs() {
        return;
    }
    let _ = std::panic::catch_unwind(|| {
        crate::reader_client::on_relay_message(&json);
        crate::script_feed::publish(&json);
    });
}

/// Whether a relay message is a Twitch alert that Theater API · Twitch sends
/// itself (for a channel it's connected to), so the one from the port would
/// come twice. Only without a Theater plugin hosting the relay: the Theater
/// Reader plugin already leaves those out, the Theater Reader app doesn't.
pub fn api_sends_this_twitch_alert(text: &str) -> bool {
    if !text.contains("\"alert\"") || relay_host_in_obs() {
        return false;
    }
    let Ok(v) = serde_json::from_str::<serde_json::Value>(text) else { return false };
    let Some(channel) = twitch_alert_channel(&v) else { return false };
    let Some(channels) = call_json(c"theater_api_twitch_channels") else { return false };
    serde_json::from_str::<Vec<String>>(&channels).unwrap_or_default().iter().any(|c| c.eq_ignore_ascii_case(&channel))
}

/// A Twitch alert's channel, from its trigger ("Twitch <channel> redeem …"):
/// redeems' IRC copies too
fn twitch_alert_channel(v: &serde_json::Value) -> Option<String> {
    if v.get("type")?.as_str()? != "alert" {
        return None;
    }
    let mut words = v.get("trigger")?.as_str()?.split_whitespace();
    if !words.next()?.eq_ignore_ascii_case("twitch") {
        return None;
    }
    Some(words.next()?.trim_start_matches('#').to_string())
}

/// Another plugin's procedure that hands back `out string json` (Theater
/// API's lists…). None when that procedure isn't there (its plugin isn't
/// installed). Any thread.
pub fn call_json(name: &CStr) -> Option<String> {
    let obs = obs()?;
    let mut cd = Calldata { stack: std::ptr::null_mut(), size: 0, capacity: 0, fixed: false };
    let called = unsafe { (obs.proc_handler_call)(obs.proc_handler, name.as_ptr(), &mut cd) };
    let json = called.then(|| get_string(obs, &mut cd, c"json"));
    if !cd.stack.is_null() {
        unsafe { (obs.bfree)(cd.stack as *mut c_void) };
    }
    json
}

/// theater_reader_state's `chats`: `[{"platform":"Twitch","channel":"…"}]`
fn parse_reader_chats(json: &str) -> Vec<(theater_shared::Platform, String)> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(json) else { return Vec::new() };
    v.get("chats")
        .and_then(|c| c.as_array())
        .map(|chats| {
            chats
                .iter()
                .filter_map(|c| {
                    let platform = theater_shared::Platform::named(c.get("platform")?.as_str()?)?;
                    let channel = c.get("channel")?.as_str()?.trim();
                    (!channel.is_empty()).then(|| (platform, channel.to_string()))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn get_string(obs: &Obs, cd: *mut Calldata, name: &CStr) -> String {
    let mut s: *const c_char = std::ptr::null();
    if unsafe { (obs.calldata_get_string)(cd, name.as_ptr(), &mut s) } && !s.is_null() {
        unsafe { CStr::from_ptr(s) }.to_string_lossy().into_owned()
    } else {
        String::new()
    }
}

fn set_string(obs: &Obs, cd: *mut Calldata, name: &CStr, value: &str) {
    let value = CString::new(value.replace('\0', "")).unwrap_or_default();
    let bytes = value.as_bytes_with_nul();
    unsafe { (obs.calldata_set_data)(cd, name.as_ptr(), bytes.as_ptr() as *const c_void, bytes.len()) };
}

fn set_bool(obs: &Obs, cd: *mut Calldata, name: &CStr, value: bool) {
    unsafe { (obs.calldata_set_data)(cd, name.as_ptr(), &value as *const bool as *const c_void, size_of::<bool>()) };
}

unsafe extern "C" fn list_proc(_data: *mut c_void, cd: *mut Calldata) {
    let Some(obs) = OBS.get() else { return };
    let json = std::panic::catch_unwind(list_json).unwrap_or_else(|_| "{}".to_string());
    set_string(obs, cd, c"json", &json);
}

unsafe extern "C" fn fire_proc(_data: *mut c_void, cd: *mut Calldata) {
    let Some(obs) = OBS.get() else { return };
    let id = get_string(obs, cd, c"id");
    let event = get_string(obs, cd, c"event");
    let result = std::panic::catch_unwind(|| fire(&id, &event))
        .unwrap_or_else(|_| Err("Theater Scene failed (panic)".to_string()));
    let (ok, message) = match result {
        Ok(m) => (true, m),
        Err(m) => (false, m),
    };
    crate::obs_source::trust_log(&format!("[Trust Core] 🔗 Control Panel fire '{}': {}", id, message));
    set_bool(obs, cd, c"ok", ok);
    set_string(obs, cd, c"result", &message);
}

fn get_int(obs: &Obs, cd: *mut Calldata, name: &CStr) -> i64 {
    let mut value: i64 = 0;
    let found = unsafe { (obs.calldata_get_data)(cd, name.as_ptr(), &mut value as *mut i64 as *mut c_void, size_of::<i64>()) };
    if found { value } else { 0 }
}

unsafe extern "C" fn visibility_proc(_data: *mut c_void, cd: *mut Calldata) {
    let Some(obs) = OBS.get() else { return };
    let scene = get_string(obs, cd, c"scene");
    let source = get_string(obs, cd, c"source");
    let action = get_string(obs, cd, c"action");
    let transition = get_string(obs, cd, c"transition");
    let duration = get_int(obs, cd, c"duration").clamp(0, 600_000) as u32;
    let settings = get_string(obs, cd, c"settings");
    let result = std::panic::catch_unwind(|| {
        crate::item_fx::set_visibility(&scene, &source, &action, &transition, duration, &settings)
    })
    .unwrap_or_else(|_| Err("Theater Scene failed (panic)".to_string()));
    let (ok, message) = match result {
        Ok(m) => (true, m),
        Err(m) => (false, m),
    };
    crate::obs_source::trust_log(&format!("[Trust Core] 👁 Control Panel visibility: {message}"));
    set_bool(obs, cd, c"ok", ok);
    set_string(obs, cd, c"result", &message);
}

unsafe extern "C" fn play_sound_proc(_data: *mut c_void, cd: *mut Calldata) {
    let Some(obs) = OBS.get() else { return };
    let path = get_string(obs, cd, c"path");
    let mut volume: f64 = 1.0;
    unsafe { (obs.calldata_get_data)(cd, c"volume".as_ptr(), &mut volume as *mut f64 as *mut c_void, size_of::<f64>()) };
    let result = std::panic::catch_unwind(|| crate::sound::queue_file(std::path::Path::new(&path), volume as f32))
        .unwrap_or_else(|_| Err("Theater Scene failed (panic)".to_string()));
    set_bool(obs, cd, c"ok", result.is_ok());
    set_string(obs, cd, c"result", result.as_ref().err().map(String::as_str).unwrap_or("playing"));
}

/// A redeem's id: its file name without extension (its .json and .rdm share it).
fn redeem_id(item: &studio_app::RedeemItem) -> String {
    std::path::Path::new(&item.filename)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| item.filename.clone())
}

/// The redeems, as loaded at startup and after each save or delete. Not
/// reloaded here: a reload signals theater_scene_list_changed, which makes
/// listeners ask for the list again.
fn redeems() -> Vec<studio_app::RedeemItem> {
    GLOBAL_REDEEMS.read().clone()
}

fn list_json() -> String {
    let poses: Vec<serde_json::Value> = SCENE_COORDINATE_SETS
        .read()
        .iter()
        .map(|s| serde_json::json!({ "id": s.id, "name": s.name, "scene": s.scene }))
        .collect();
    let redeems: Vec<serde_json::Value> = redeems()
        .iter()
        .map(|r| serde_json::json!({ "id": redeem_id(r), "name": r.config.name }))
        .collect();
    serde_json::json!({
        "poses": poses,
        "redeems": redeems,
        // For the Source Visibility command
        "transitions": crate::item_fx::transitions_json(),
        "luma_wipes": crate::item_fx::luma_wipes_json(),
    })
    .to_string()
}

/// Fires the pose or redeem `id`, as the dock buttons do. `event` is optional
/// JSON: {"user", "wildcards": [...]}.
fn fire(id: &str, event: &str) -> Result<String, String> {
    if id.is_empty() {
        return Err("no pose or redeem selected".to_string());
    }
    let event: serde_json::Value = serde_json::from_str(event).unwrap_or(serde_json::Value::Null);

    let pose = SCENE_COORDINATE_SETS.read().iter().find(|s| s.id == id).cloned();
    if let Some(set) = pose {
        if !set.scene.is_empty() {
            let live = crate::obs_source::current_obs_scene_name().unwrap_or_default();
            if live != set.scene {
                return Err(format!("skipped pose '{}': it's for scene '{}', and '{}' is live", set.name, set.scene, live));
            }
        }
        let state = crate::state::get_state();
        if state.studio_sources.read().is_empty() {
            studio_app::load_studio_targets_from_disk();
        }
        let sources = state.studio_sources.read().clone();
        studio_app::start_scene_transition(&set, &sources);
        studio_app::show_trigger_status(format!("Triggered: {}", set.name));
        return Ok(format!("pose '{}'", set.name));
    }

    let Some(item) = redeems().into_iter().find(|r| redeem_id(r) == id) else {
        return Err(format!("no pose or redeem with id '{}' (deleted?)", id));
    };
    let mut config = item.config.clone();
    if let Some(wildcards) = event.get("wildcards").and_then(|w| w.as_array()) {
        config.wildcards = Some(wildcards.iter().map(|v| v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string())).collect());
    }
    let json = studio_app::build_redeem_dispatch_json(&item.filename, &config)
        .ok_or_else(|| format!("couldn't prepare redeem '{}'", config.name))?;
    let json = match (serde_json::from_str::<serde_json::Value>(&json), event.get("user")) {
        (Ok(serde_json::Value::Object(mut map)), Some(user)) => {
            map.insert("user".to_string(), user.clone());
            serde_json::Value::Object(map).to_string()
        }
        _ => json,
    };
    if !studio_app::dispatch_test_redeem_json(&json) {
        return Err(format!("redeem '{}' didn't fire", config.name));
    }
    studio_app::show_trigger_status(format!("🚀 Redeem: {}", config.name));
    Ok(format!("redeem '{}'", config.name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::studio_app::{MovementEasing, SceneCoordinateSet};

    fn pose(id: &str, scene: &str) -> SceneCoordinateSet {
        SceneCoordinateSet {
            id: id.to_string(),
            name: format!("Pose {id}"),
            scene: scene.to_string(),
            trigger: String::new(),
            duration_ms: 0,
            easing: MovementEasing::Instant,
            sources: vec![],
        }
    }

    #[test]
    fn test_list_has_poses_with_their_scene() {
        SCENE_COORDINATE_SETS.write().push(pose("procs_list_pose", "Streaming"));
        let list: serde_json::Value = serde_json::from_str(&list_json()).unwrap();
        let found = list["poses"].as_array().unwrap().iter().find(|p| p["id"] == "procs_list_pose").cloned();
        SCENE_COORDINATE_SETS.write().retain(|s| s.id != "procs_list_pose");
        let found = found.expect("the pose is listed");
        assert_eq!(found["name"], "Pose procs_list_pose");
        assert_eq!(found["scene"], "Streaming");
        assert!(list["redeems"].is_array());
    }

    #[test]
    fn test_fire_unknown_id_says_so() {
        let err = fire("procs_no_such_id", "").unwrap_err();
        assert!(err.contains("procs_no_such_id"), "{err}");
        assert!(fire("", "").is_err());
    }

    #[test]
    fn test_fire_skips_a_pose_for_another_scene() {
        SCENE_COORDINATE_SETS.write().push(pose("procs_other_scene_pose", "Procs Not Live Scene"));
        let result = fire("procs_other_scene_pose", "{}");
        SCENE_COORDINATE_SETS.write().retain(|s| s.id != "procs_other_scene_pose");
        let err = result.unwrap_err();
        assert!(err.starts_with("skipped pose"), "{err}");
    }

    #[test]
    fn theater_readers_chats_come_from_its_state() {
        use theater_shared::Platform;
        // What Theater Reader's theater_reader_state says
        let json = r#"{"chats":[{"channel":"mortygotvr","platform":"Twitch"},{"channel":" mortygotvr ","platform":"Kick"},
            {"channel":"","platform":"YouTube"},{"channel":"x","platform":"Nowhere"}],"enabled":false,"relay":{"apps":0,"port":41837,"up":false}}"#;
        assert_eq!(
            parse_reader_chats(json),
            vec![(Platform::Twitch, "mortygotvr".to_string()), (Platform::Kick, "mortygotvr".to_string())]
        );
        // Theater Reader not installed: no answer
        assert!(parse_reader_chats("").is_empty());
    }

    #[test]
    fn a_twitch_alerts_channel_comes_from_its_trigger() {
        let channel = |json: &str| twitch_alert_channel(&serde_json::from_str(json).unwrap());
        // The Theater Reader app's alert, and a redeem's IRC copy
        assert_eq!(channel(r#"{"type":"alert","source":"twitch","event":"sub","trigger":"Twitch MyChannel sub","data":{}}"#).as_deref(), Some("MyChannel"));
        assert_eq!(channel(r#"{"type":"alert","trigger":"twitch #mychannel redeem 1234-abcd"}"#).as_deref(), Some("mychannel"));
        assert_eq!(channel(r#"{"type":"alert","trigger":"YouTube MyChannel superchat"}"#), None);
        assert_eq!(channel(r#"{"type":"chat","trigger":"Twitch MyChannel"}"#), None, "chat stays");
    }
}
