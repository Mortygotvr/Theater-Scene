//! The "🎭 Puppet" redeem type: a redeem that drives a Theater Puppet avatar
//! (the Theater-Puppet OBS plugin) through its procedure
//! `theater_puppet_command`: play an animation, toggle or ping-pong a shape
//! key, animate a shape key, show or hide a GLB, set a colour slot, show a
//! tile (by name, or next / random in a set; and a shape key with it). Each but the shape-key animation takes a
//! timer: then Puppet puts back what it changed.
//!
//! A GLB prefab ("prefab"): Puppet spawns a GLB that plays one of its
//! animations once (baked in Blender, no physics) and goes. Its contact
//! frames (Blender markers named "contact", which Theater Rig exports) run
//! the redeem's contact section, as a throwable's impact does: its Impact
//! Audio, and its Screen Shake and Impact Reaction set to On Contact.
//!
//! The redeem keeps the command as its `puppet` setting (the procedure's own
//! JSON); its text settings take the redeem's wildcards ({1}, {2}…) when it
//! fires. The Studio's form fills its lists from what the Puppets have.

use serde_json::{Value, json};

const PROC: &str = "theater_puppet_command";

/// Every string in `v` with the event's wildcards put in
fn with_wildcards(v: &Value, wildcards: &[String]) -> Value {
    match v {
        Value::String(s) => Value::String(crate::physics::resolve_wildcards(s, wildcards)),
        Value::Object(m) => Value::Object(m.iter().map(|(k, v)| (k.clone(), with_wildcards(v, wildcards))).collect()),
        Value::Array(a) => Value::Array(a.iter().map(|v| with_wildcards(v, wildcards)).collect()),
        other => other.clone(),
    }
}

/// An impact reaction's Puppet animation (`impactReactions.puppet_animation`):
/// `clip` played once on `target` if it's a Puppet source, else on every
/// Puppet. Off the caller's thread (the physics step calls it).
pub fn play_reaction_animation(clip: &str, target: &str) {
    let (clip, target) = (clip.to_string(), target.to_string());
    let _ = std::thread::Builder::new().name("puppet-reaction-animation".into()).spawn(move || {
        let ask = |source: &str| {
            let args = json!({ "action": "animation", "mode": "once", "clip": clip, "source": source });
            crate::obs_procs::call_proc_json(PROC, &args.to_string()).and_then(|s| serde_json::from_str::<Value>(&s).ok())
        };
        let is_puppet = !target.is_empty() && puppets().iter().any(|p| p.get("source").and_then(|s| s.as_str()) == Some(target.as_str()));
        let out = ask(if is_puppet { &target } else { "" });
        match out {
            None => crate::obs_source::trust_log("[Puppet reaction] ⚠️ Theater Puppet isn't loaded"),
            Some(o) if o.get("ok").and_then(|v| v.as_bool()) == Some(false) => {
                crate::obs_source::trust_log(&format!("[Puppet reaction] ⚠️ \"{clip}\": {}", o.get("error").and_then(|e| e.as_str()).unwrap_or("failed")))
            }
            Some(_) => crate::obs_source::trust_log(&format!("[Puppet reaction] 🎬 \"{clip}\" on {}", if is_puppet { target.as_str() } else { "every Puppet" })),
        }
    });
}

/// Every animation the Puppets have (their own and their Animation folder's), once each
pub fn animation_names() -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for p in puppets() {
        for a in p.get("animations").and_then(|a| a.as_array()).into_iter().flatten() {
            if let Some(n) = a.as_str() {
                if !out.iter().any(|o| o == n) {
                    out.push(n.to_string());
                }
            }
        }
    }
    out
}

/// The command a redeem's `puppet` setting holds, its wildcards put in
pub fn command(puppet: &Value, wildcards: &[String]) -> Value {
    with_wildcards(puppet, wildcards)
}

/// Fires a Puppet redeem (its settings `val`); false when Puppet isn't there
/// or said no
pub fn fire(val: &Value, wildcards: &[String]) -> bool {
    let Some(puppet) = val.get("puppet").filter(|p| p.is_object()) else {
        crate::obs_source::trust_log("[Puppet redeem] ⚠️ No Puppet command set");
        return false;
    };
    let args = command(puppet, wildcards);
    match crate::obs_procs::call_proc_json(PROC, &args.to_string()) {
        None => {
            crate::obs_source::trust_log("[Puppet redeem] ⚠️ Theater Puppet isn't loaded");
            false
        }
        Some(out) => {
            let out: Value = serde_json::from_str(&out).unwrap_or(json!({}));
            if out.get("ok").and_then(|v| v.as_bool()) == Some(false) {
                crate::obs_source::trust_log(&format!("[Puppet redeem] ⚠️ {}", out.get("error").and_then(|v| v.as_str()).unwrap_or("failed")));
                false
            } else {
                crate::obs_source::trust_log(&format!("[Puppet redeem] 🎭 {}", args));
                // A GLB prefab: its contact section, timed by its contact frames
                if is_prefab(Some(&args)) {
                    prefab_effects(val, &out);
                }
                true
            }
        }
    }
}

// ---------------------------------------------------------------------------
// The Studio's form

/// What the Puppets have (Puppet's `list`), asked again every second while
/// the form is shown
fn puppets() -> Vec<Value> {
    static CACHE: parking_lot::Mutex<Option<(std::time::Instant, Vec<Value>)>> = parking_lot::Mutex::new(None);
    let mut cache = CACHE.lock();
    if cache.as_ref().is_none_or(|(t, _)| t.elapsed().as_secs_f32() > 1.0) {
        let list = crate::obs_procs::call_proc_json(PROC, r#"{"action":"list"}"#)
            .and_then(|s| serde_json::from_str::<Value>(&s).ok())
            .and_then(|v| v.get("puppets").and_then(|p| p.as_array()).cloned())
            .unwrap_or_default();
        *cache = Some((std::time::Instant::now(), list));
    }
    cache.as_ref().map(|(_, l)| l.clone()).unwrap_or_default()
}

/// Pictures of a Puppet's tiles (Puppet's `tile_previews`, `ask` its
/// request without the action), as textures, asked again every 5 seconds
/// (name, set, picture)
fn tile_pictures(ctx: &egui::Context, ask: Value) -> Vec<(String, String, egui::TextureHandle)> {
    use base64::Engine;
    type Cache = std::collections::HashMap<String, (std::time::Instant, Vec<(String, String, egui::TextureHandle)>)>;
    static CACHE: parking_lot::Mutex<Option<Cache>> = parking_lot::Mutex::new(None);
    let mut args = ask;
    args["action"] = json!("tile_previews");
    args["size"] = json!(64);
    let key = args.to_string();
    let mut cache = CACHE.lock();
    let cache = cache.get_or_insert_with(Default::default);
    if let Some((t, pics)) = cache.get(&key) {
        if t.elapsed().as_secs_f32() < 5.0 {
            return pics.clone();
        }
    }
    let previews = crate::obs_procs::call_proc_json(PROC, &key)
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .and_then(|v| v.get("previews").and_then(|p| p.as_array()).cloned())
        .unwrap_or_default();
    let pics: Vec<(String, String, egui::TextureHandle)> = previews
        .iter()
        .filter_map(|p| {
            let name = p.get("name")?.as_str()?.to_string();
            let png = base64::prelude::BASE64_STANDARD.decode(p.get("png")?.as_str()?).ok()?;
            let img = image::load_from_memory(&png).ok()?.to_rgba8();
            let color = egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw());
            let set = p.get("set").and_then(|s| s.as_str()).unwrap_or("").to_string();
            Some((name.clone(), set, ctx.load_texture(format!("puppet_tile {key} {name}"), color, egui::TextureOptions::NEAREST)))
        })
        .collect();
    cache.insert(key, (std::time::Instant::now(), pics.clone()));
    pics
}

/// The pictures as buttons, the one named `current` lit; the name clicked
fn picture_picker(ui: &mut egui::Ui, pics: &[(String, egui::TextureHandle)], current: &str) -> Option<String> {
    let mut picked = None;
    ui.horizontal_wrapped(|ui| {
        for (name, tex) in pics {
            ui.vertical(|ui| {
                let size = tex.size_vec2();
                let k = 56.0 / size.y.max(1.0);
                let lit = name.eq_ignore_ascii_case(current.trim());
                if ui.add(egui::ImageButton::new(egui::Image::new(tex).fit_to_exact_size(size * k)).selected(lit)).on_hover_text(name).clicked() {
                    picked = Some(name.clone());
                }
                ui.label(egui::RichText::new(name).small());
            });
        }
    });
    picked
}

const ACTIONS: [(&str, &str); 8] = [
    ("animation", "🎬 Play an animation"),
    ("shape", "🙂 Shape key on / off / ping-pong"),
    ("shape_anim", "📈 Animate a shape key"),
    ("glb", "👁 Show / hide a GLB"),
    ("group", "🔀 Show a GLB of a group"),
    ("tile", "🧩 Tile texture"),
    ("color", "🎨 Set a colour slot"),
    ("prefab", "📦 Spawn a GLB (prefab: plays once, then goes)"),
];

/// Whether a redeem's Puppet command spawns a GLB prefab (its contact section applies)
pub fn is_prefab(puppet: Option<&Value>) -> bool {
    puppet.and_then(|p| p.get("action")).and_then(|a| a.as_str()) == Some("prefab")
}

/// A GLB prefab's contact section (its redeem's settings `val`, Puppet's
/// `plan`): the Screen Shake and Impact Reaction set On Start now; at each
/// contact frame (seconds from now), the Impact Audio, and the shake and the
/// reaction set On Contact. On the avatar the prefab spawned with.
fn prefab_effects(val: &Value, plan: &Value) {
    use crate::physics::{ImpactReactionConfig, ShakeTiming};
    let text = |keys: &[&str]| keys.iter().find_map(|k| val.get(*k).and_then(|v| v.as_str()).map(str::trim).filter(|s| !s.is_empty())).map(str::to_string);
    let sound = text(&["collisionSound", "collision_sound", "collisionSoundPath", "collision_sound_path"]);
    let map: std::collections::HashMap<String, Value> = val.as_object().map(|m| m.clone().into_iter().collect()).unwrap_or_default();
    let shake = crate::physics::parse_shake_config(&map);
    let reaction = ImpactReactionConfig::parse(val.get("impactReactions").or_else(|| val.get("impact_reactions")));
    let target = plan.get("source").and_then(|s| s.as_str()).unwrap_or("").to_string();
    let (w, h) = crate::obs_source::get_obs_canvas_resolution();
    let (cx, cy) = (w.max(1) as f32 * 0.5, h.max(1) as f32 * 0.5);
    let shakes = |when: ShakeTiming| shake.enabled && shake.timing == when;
    if shakes(ShakeTiming::Start) {
        crate::state::get_state().trigger_screen_shake(shake.duration, shake.intensity);
    }
    if let Some(r) = reaction.at(ShakeTiming::Start) {
        crate::physics::fire_impact_reaction(&r, &target, cx, cy);
    }
    let contacts: Vec<f64> = plan.get("contacts").and_then(|c| c.as_array()).map(|a| a.iter().filter_map(|t| t.as_f64()).filter(|t| t.is_finite() && *t >= 0.0).collect()).unwrap_or_default();
    let on_contact = reaction.at(ShakeTiming::Impact);
    let shake_on = shakes(ShakeTiming::Impact);
    if contacts.is_empty() || (sound.is_none() && !shake_on && on_contact.is_none()) {
        return;
    }
    let _ = std::thread::Builder::new().name("puppet-prefab-contacts".into()).spawn(move || {
        let start = std::time::Instant::now();
        for t in contacts {
            let at = start + std::time::Duration::from_secs_f64(t);
            if let Some(wait) = at.checked_duration_since(std::time::Instant::now()) {
                std::thread::sleep(wait);
            }
            if let Some(s) = &sound {
                let _ = crate::play_sound_str(s);
            }
            if shake_on {
                crate::state::get_state().trigger_screen_shake(shake.duration, shake.intensity);
            }
            if let Some(r) = &on_contact {
                crate::physics::fire_impact_reaction(r, &target, cx, cy);
            }
            crate::obs_source::trust_log(&format!("[Puppet redeem] 💥 Prefab contact at {t:.2} s"));
        }
    });
}

/// A GLB's animations (Puppet's `prefab_info`: name, seconds, contacts),
/// asked again every 2 seconds while the form shows it
fn prefab_clips(glb: &str) -> Option<Vec<(String, f64, usize)>> {
    type Cache = std::collections::HashMap<String, (std::time::Instant, Option<Vec<(String, f64, usize)>>)>;
    static CACHE: std::sync::LazyLock<parking_lot::Mutex<Cache>> = std::sync::LazyLock::new(|| parking_lot::Mutex::new(Cache::new()));
    let mut cache = CACHE.lock();
    if let Some((at, clips)) = cache.get(glb) {
        if at.elapsed().as_secs_f32() < 2.0 {
            return clips.clone();
        }
    }
    let clips = crate::obs_procs::call_proc_json(PROC, &json!({ "action": "prefab_info", "glb": glb }).to_string())
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .filter(|v| v.get("ok").and_then(|o| o.as_bool()) == Some(true))
        .and_then(|v| v.get("clips").and_then(|c| c.as_array()).cloned())
        .map(|a| {
            a.iter()
                .map(|c| (c.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string(), c.get("duration").and_then(|d| d.as_f64()).unwrap_or(0.0), c.get("contacts").and_then(|k| k.as_array()).map_or(0, |k| k.len())))
                .collect()
        });
    cache.insert(glb.to_string(), (std::time::Instant::now(), clips.clone()));
    clips
}

fn get_str<'a>(cmd: &'a Value, key: &str, default: &'a str) -> &'a str {
    cmd.get(key).and_then(|v| v.as_str()).unwrap_or(default)
}

fn get_f(cmd: &Value, key: &str, default: f64) -> f64 {
    cmd.get(key).and_then(|v| v.as_f64()).unwrap_or(default)
}

/// A text setting with a ▾ menu of the known values; true when it changed
fn text_with_choices(ui: &mut egui::Ui, label: &str, cmd: &mut Value, key: &str, choices: &[String], hint: &str) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(label);
        let mut s = get_str(cmd, key, "").to_string();
        if ui.add(egui::TextEdit::singleline(&mut s).hint_text(hint).desired_width(180.0)).changed() {
            cmd[key] = json!(s);
            changed = true;
        }
        if !choices.is_empty() {
            ui.menu_button("▾", |ui| {
                egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                    for c in choices {
                        if ui.button(c).clicked() {
                            cmd[key] = json!(c);
                            changed = true;
                            ui.close_menu();
                        }
                    }
                });
            });
        }
    });
    changed
}

/// A choice among fixed (value, label) pairs; true when it changed
fn choice(ui: &mut egui::Ui, label: &str, cmd: &mut Value, key: &str, options: &[(&str, &str)]) -> bool {
    let current = get_str(cmd, key, options[0].0).to_string();
    let shown = options.iter().find(|(v, _)| *v == current).map(|(_, l)| *l).unwrap_or(options[0].1);
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(label);
        egui::ComboBox::from_id_salt(("puppet_redeem", key)).selected_text(shown).show_ui(ui, |ui| {
            for (v, l) in options {
                if ui.selectable_label(current == *v, *l).clicked() && current != *v {
                    cmd[key] = json!(v);
                    changed = true;
                }
            }
        });
    });
    // A new form keeps its first choice
    if cmd.get(key).is_none() {
        cmd[key] = json!(options[0].0);
        changed = true;
    }
    changed
}

/// A number slider; true when it changed
fn slider(ui: &mut egui::Ui, label: &str, cmd: &mut Value, key: &str, range: std::ops::RangeInclusive<f64>, default: f64, suffix: &str) -> bool {
    let mut v = get_f(cmd, key, default);
    let changed = ui.add(egui::Slider::new(&mut v, range).text(label).suffix(suffix)).changed();
    if changed || cmd.get(key).is_none() {
        cmd[key] = json!(v);
    }
    changed
}

/// The Puppet redeem's settings; true when they changed
pub fn editor_ui(ui: &mut egui::Ui, setting: &mut Option<Value>) -> bool {
    let mut changed = false;
    let cmd = setting.get_or_insert_with(|| {
        changed = true;
        json!({ "action": "animation", "source": "", "mode": "once" })
    });
    if !cmd.is_object() {
        *cmd = json!({ "action": "animation" });
        changed = true;
    }
    let puppets = puppets();
    if puppets.is_empty() {
        ui.label(egui::RichText::new("Theater Puppet isn't running, or has no Puppet source: the lists below stay empty (you can still type the names).").italics().color(egui::Color32::from_rgb(148, 163, 184)));
    }
    ui.label(egui::RichText::new("Text fields take the redeem's wildcards: {1}, {2}… (e.g. a colour or a shape key a viewer typed).").italics().color(egui::Color32::from_rgb(148, 163, 184)));
    ui.add_space(4.0);

    // Which Puppet: its lists
    let names: Vec<String> = puppets.iter().filter_map(|p| p.get("source").and_then(|s| s.as_str()).map(String::from)).collect();
    changed |= text_with_choices(ui, "Puppet source:", cmd, "source", &names, "empty: every Puppet");
    let source = get_str(cmd, "source", "").to_string();
    let puppet = puppets.iter().find(|p| p.get("source").and_then(|s| s.as_str()) == Some(source.as_str())).or(puppets.first());
    let list = |key: &str| -> Vec<String> {
        puppet.and_then(|p| p.get(key)).and_then(|a| a.as_array()).map(|a| a.iter().map(|v| v.as_str().map(String::from).unwrap_or_else(|| v.to_string())).collect()).unwrap_or_default()
    };
    let glbs: Vec<Value> = puppet.and_then(|p| p.get("glbs")).and_then(|g| g.as_array()).cloned().unwrap_or_default();
    let glb_choices: Vec<String> = glbs.iter().filter_map(|g| g.get("name").and_then(|n| n.as_str()).map(String::from)).collect();

    changed |= choice(ui, "Action:", cmd, "action", &ACTIONS);
    let action = get_str(cmd, "action", "animation").to_string();
    // The shape keys of the GLB picked (by name or number)
    let keys = |cmd: &Value| -> Vec<String> {
        let glb = get_str(cmd, "glb", "0").trim().to_string();
        let g = glb.parse::<usize>().ok().and_then(|i| glbs.get(i)).or_else(|| glbs.iter().find(|g| g.get("name").and_then(|n| n.as_str()).is_some_and(|n| n.eq_ignore_ascii_case(&glb))));
        g.and_then(|g| g.get("keys")).and_then(|k| k.as_array()).map(|k| k.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_default()
    };

    match action.as_str() {
        "animation" => {
            changed |= choice(ui, "Play:", cmd, "mode", &[("once", "Once, then back"), ("loop", "Loop (in place of the idle motion)"), ("stop", "Stop the one playing once")]);
            changed |= text_with_choices(ui, "Animation:", cmd, "clip", &list("animations"), "its name in the GLB");
        }
        "shape" | "shape_anim" => {
            changed |= text_with_choices(ui, "GLB:", cmd, "glb", &glb_choices, "0 = the avatar, or its name");
            let k = keys(cmd);
            changed |= text_with_choices(ui, "Shape key:", cmd, "key", &k, "its name in Blender");
            if action == "shape" {
                changed |= choice(ui, "State:", cmd, "state", &[("on", "On (held)"), ("off", "Off (back to normal)"), ("pingpong", "Ping-pong (until turned off)")]);
                changed |= slider(ui, "Strength (on, ping-pong's top)", cmd, "value", 0.0..=1.0, 1.0, "");
                if get_str(cmd, "state", "on") == "pingpong" {
                    changed |= slider(ui, "Ping-pong: there and back", cmd, "period", 0.1..=10.0, 1.0, " s");
                }
            } else {
                changed |= slider(ui, "Range: from", cmd, "from", 0.0..=1.0, 0.0, "");
                changed |= slider(ui, "Range: to", cmd, "to", 0.0..=1.0, 1.0, "");
                changed |= slider(ui, "Duration", cmd, "duration", 0.1..=30.0, 1.0, " s");
                changed |= choice(ui, "Type:", cmd, "type", &[("up", "Increasing (from → to)"), ("down", "Decreasing (to → from)"), ("pingpong", "Ping-pong (from → to → from)")]);
                let mut show = cmd.get("show_at_start").and_then(|v| v.as_bool()).unwrap_or(false);
                let mut hide = cmd.get("hide_at_end").and_then(|v| v.as_bool()).unwrap_or(false);
                ui.horizontal(|ui| {
                    if ui.checkbox(&mut show, "Show the GLB at the start").changed() {
                        changed = true;
                    }
                    if ui.checkbox(&mut hide, "Hide it at the end").changed() {
                        changed = true;
                    }
                });
                cmd["show_at_start"] = json!(show);
                cmd["hide_at_end"] = json!(hide);
            }
        }
        "glb" => {
            changed |= text_with_choices(ui, "GLB:", cmd, "glb", &glb_choices, "0 = the avatar, or its name");
            // "visible": true, false or "toggle", as the procedure takes it
            let current = match cmd.get("visible") {
                Some(Value::Bool(true)) => "show",
                Some(Value::Bool(false)) => "hide",
                _ => "toggle",
            };
            let mut pick = json!({ "v": current });
            if choice(ui, "Visibility:", &mut pick, "v", &[("toggle", "Toggle"), ("show", "Show"), ("hide", "Hide")]) {
                changed = true;
            }
            cmd["visible"] = match get_str(&pick, "v", "toggle") {
                "show" => json!(true),
                "hide" => json!(false),
                _ => json!("toggle"),
            };
        }
        "group" => {
            changed |= text_with_choices(ui, "Group:", cmd, "group", &list("groups"), "as in the Puppet's properties");
            // That group's GLBs, then next / none
            let group = get_str(cmd, "group", "").trim().to_string();
            let mut picks: Vec<String> = glbs
                .iter()
                .filter(|g| g.get("group").and_then(|v| v.as_str()).is_some_and(|v| !group.is_empty() && v.trim().eq_ignore_ascii_case(&group)))
                .filter_map(|g| g.get("name").and_then(|n| n.as_str()).map(String::from))
                .collect();
            picks.extend(["next".to_string(), "none".to_string()]);
            if cmd.get("glb").is_none() {
                cmd["glb"] = json!("next");
                changed = true;
            }
            changed |= text_with_choices(ui, "Show:", cmd, "glb", &picks, "a GLB, next or none");
        }
        "tile" => {
            changed |= text_with_choices(ui, "GLB:", cmd, "glb", &glb_choices, "0 = the avatar, or its name");
            // The GLB picked: its tiles and materials
            let glb = get_str(cmd, "glb", "0").trim().to_string();
            let g = glb.parse::<usize>().ok().and_then(|i| glbs.get(i)).or_else(|| glbs.iter().find(|g| g.get("name").and_then(|n| n.as_str()).is_some_and(|n| n.eq_ignore_ascii_case(&glb))));
            // Its tile sets, each tile in order (Theater Rig: Make Tiles, or
            // the VRM's expressions converted: "expressions")
            let sets: Vec<(String, Vec<String>)> = g
                .and_then(|g| g.get("tile_sets"))
                .and_then(|m| m.as_object())
                .map(|m| m.iter().map(|(k, v)| (k.clone(), v.as_array().map(|a| a.iter().filter_map(|n| n.as_str().map(String::from)).collect()).unwrap_or_default())).collect())
                .unwrap_or_default();
            // An old redeem's grid (material, columns, rows, cell): it still works
            let old_grid = cmd.get("material").and_then(|v| v.as_str()).is_some_and(|m| !m.is_empty());
            if old_grid {
                ui.label(egui::RichText::new("This redeem uses the old grid form (material / columns / rows / cell): it still works. Pick a tile to switch it to the standard one.").small().color(egui::Color32::from_rgb(234, 179, 8)));
            }
            if cmd.get("name").is_none() {
                cmd["name"] = json!("next");
                changed = true;
            }
            let mut choices: Vec<String> = sets.iter().flat_map(|(_, n)| n.clone()).collect();
            choices.extend(["next".to_string(), "random".to_string()]);
            let before = get_str(cmd, "name", "").to_string();
            changed |= text_with_choices(ui, "Tile:", cmd, "name", &choices, "its name, next, random or {1}");
            let mut picked: Option<(String, String)> = None;
            let name = get_str(cmd, "name", "").trim().to_ascii_lowercase();
            if name == "next" || name == "random" {
                let set_names: Vec<String> = sets.iter().map(|(s, _)| s.clone()).collect();
                changed |= text_with_choices(ui, "In set:", cmd, "set", &set_names, "empty: all the GLB's tiles");
            }
            // Each set, its tiles seen (a wildcard GLB has none to show)
            if !glb.contains('{') && !sets.is_empty() {
                let pics = tile_pictures(ui.ctx(), json!({ "source": source, "glb": glb }));
                let current = get_str(cmd, "name", "").to_string();
                for (set, _) in &sets {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(if set.is_empty() { "(no set)" } else { set.as_str() }).strong());
                        if ui.small_button("▶ next").on_hover_text("Each time: the next tile of this set").clicked() {
                            picked = Some(("next".into(), set.clone()));
                        }
                        if ui.small_button("🎲 random").on_hover_text("Each time: another tile of this set").clicked() {
                            picked = Some(("random".into(), set.clone()));
                        }
                    });
                    let mine: Vec<(String, egui::TextureHandle)> = pics.iter().filter(|(_, s, _)| s == set).map(|(n, _, t)| (n.clone(), t.clone())).collect();
                    if let Some(name) = picture_picker(ui, &mine, &current) {
                        picked = Some((name, set.clone()));
                    }
                }
            } else if sets.is_empty() && !glb.contains('{') {
                ui.label(egui::RichText::new("This GLB has no tiles: make them in Theater Rig (Tile textures → Make Tiles, or Convert VRM Tiles), then export.").small().color(egui::Color32::from_rgb(148, 163, 184)));
            }
            if let Some((name, set)) = picked {
                cmd["name"] = json!(name);
                cmd["set"] = json!(set);
                changed = true;
            }
            // A tile picked: the old grid's settings go
            if old_grid && get_str(cmd, "name", "") != before {
                if let Some(m) = cmd.as_object_mut() {
                    for k in ["material", "columns", "rows", "cell"] {
                        m.remove(k);
                    }
                }
            }
            changed |= choice(ui, "State:", cmd, "state", &[("on", "On"), ("off", "Off (back to its own)"), ("toggle", "Toggle")]);
            // A shape key held while the tile shows (let go with it)
            let k = keys(cmd);
            changed |= text_with_choices(ui, "Shape key with it:", cmd, "key", &k, "empty: none");
            if !get_str(cmd, "key", "").trim().is_empty() {
                changed |= slider(ui, "Its strength", cmd, "key_value", 0.0..=1.0, 1.0, "");
            }
            // (A strength would slide the texture between tiles, not fade: the crossfade does that)
            if let Some(m) = cmd.as_object_mut() {
                m.remove("value");
            }
        }
        "color" => {
            let slots = list("colour_slots");
            let mut slot = get_f(cmd, "slot", slots.first().and_then(|s| s.parse().ok()).unwrap_or(0.0)) as i64;
            ui.horizontal(|ui| {
                ui.label("Colour slot:");
                if ui.add(egui::DragValue::new(&mut slot).range(0..=99)).changed() {
                    changed = true;
                }
                if !slots.is_empty() {
                    ui.label(egui::RichText::new(format!("(this avatar has: {})", slots.join(", "))).small());
                }
            });
            cmd["slot"] = json!(slot);
            let mut text = get_str(cmd, "color", "#ff4080").to_string();
            ui.horizontal(|ui| {
                ui.label("Colour:");
                // The picker writes #rrggbb; the text can be a wildcard or "reset"
                let parsed = text.trim_start_matches('#');
                let mut rgb = if parsed.len() == 6 {
                    [0, 2, 4].map(|i| u8::from_str_radix(&parsed[i..i + 2], 16).unwrap_or(255))
                } else {
                    [255, 64, 128]
                };
                if ui.color_edit_button_srgb(&mut rgb).changed() {
                    text = format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2]);
                    changed = true;
                }
                if ui.add(egui::TextEdit::singleline(&mut text).hint_text("#rrggbb, {1} or reset").desired_width(120.0)).changed() {
                    changed = true;
                }
                if ui.button("Reset").on_hover_text("Back to the slot's own colour").clicked() {
                    text = "reset".to_string();
                    changed = true;
                }
            });
            cmd["color"] = json!(text);
        }
        "prefab" => {
            ui.horizontal(|ui| {
                ui.label("GLB file:");
                let mut path = get_str(cmd, "glb", "").to_string();
                if ui.add(egui::TextEdit::singleline(&mut path).hint_text("C:\\path\\to\\prefab.glb").desired_width(260.0)).changed() {
                    cmd["glb"] = json!(path);
                    changed = true;
                }
                if ui.button("📂").on_hover_text("Pick the GLB").clicked() {
                    if let Some(p) = crate::studio_app::pick_file_dialog("GLB prefab", "glTF (*.glb, *.gltf) | *.glb *.gltf") {
                        cmd["glb"] = json!(p.to_string_lossy());
                        changed = true;
                    }
                }
            });
            let glb = get_str(cmd, "glb", "").trim().to_string();
            let clips = if glb.is_empty() || glb.contains('{') { None } else { prefab_clips(&glb) };
            let mut names: Vec<String> = clips.iter().flatten().map(|(n, _, _)| n.clone()).collect();
            names.push("random".into());
            changed |= text_with_choices(ui, "Animation:", cmd, "clip", &names, "empty: its first; random; or its name");
            let wanted = get_str(cmd, "clip", "").trim().to_string();
            match &clips {
                None if !glb.is_empty() && !glb.contains('{') => {
                    ui.label(egui::RichText::new("Puppet can't read this GLB (is Theater Puppet running, and the path right?).").small().color(egui::Color32::from_rgb(234, 179, 8)));
                }
                Some(c) if c.is_empty() => {
                    ui.label(egui::RichText::new("This GLB has no animation: it shows for the time below, then goes.").small().color(egui::Color32::from_rgb(148, 163, 184)));
                }
                Some(c) => {
                    let shown = if wanted.is_empty() { c.first() } else { c.iter().find(|(n, _, _)| *n == wanted) };
                    let line = match shown {
                        Some((n, d, k)) if *k > 0 => format!("{n}: {d:.2} s, {k} contact frame(s): the contact section (below) runs at each."),
                        Some((n, d, _)) => format!("{n}: {d:.2} s, no contact frame (in Blender: timeline markers named \"contact\", exported with Theater Rig enabled)."),
                        None if wanted == "random" => format!("One of its {} animations, picked each time.", c.len()),
                        None => "No animation of that name in this GLB.".to_string(),
                    };
                    ui.label(egui::RichText::new(line).small().color(egui::Color32::from_rgb(148, 163, 184)));
                }
                None => {}
            }
            // An armature with the avatar's bones (Hips, Spine…): it can follow the avatar's pose
            let mut follow = cmd.get("follow").and_then(|v| v.as_bool()).unwrap_or(true);
            if ui.checkbox(&mut follow, "🦴 Follow the avatar's armature").on_hover_text("When the GLB's armature has the avatar's bones (Theater Rig's names: Hips, Spine…), each bone moves with the avatar's, its shape keys too, and its own animation plays over that, lined up with the avatar. A GLB without them is placed below.").changed() {
                cmd["follow"] = json!(follow);
                changed = true;
            }
            let where_ = if follow { "Where, when it doesn't follow (no avatar bones): from the avatar's feet (metres), its right as seen, up, toward the camera" } else { "Where, from the avatar's feet (metres): its right as seen, up, toward the camera" };
            ui.label(egui::RichText::new(where_).small());
            changed |= slider(ui, "Right (-) / left (+)", cmd, "x", -3.0..=3.0, 0.0, " m");
            changed |= slider(ui, "Down (-) / up (+)", cmd, "y", -1.0..=3.0, 0.0, " m");
            changed |= slider(ui, "Back (-) / forward (+)", cmd, "z", -3.0..=3.0, 0.0, " m");
            changed |= slider(ui, "Size", cmd, "scale", 0.05..=10.0, 1.0, "×");
            changed |= slider(ui, "Turn", cmd, "turn", -180.0..=180.0, 0.0, "°");
            if clips.as_ref().is_some_and(|c| c.is_empty()) {
                changed |= slider(ui, "Shows for (no animation)", cmd, "hold", 0.5..=60.0, 3.0, " s");
            }
            ui.label(egui::RichText::new("Contact section: the Impact Audio, and the Screen Shake and Impact Reaction set to On Contact (Audio & Screen Shake, Impact Reactions below).").small().italics().color(egui::Color32::from_rgb(148, 163, 184)));
        }
        _ => {}
    }
    // A GLB dissolving in or out, a tile crossfading
    if matches!(action.as_str(), "glb" | "group" | "shape_anim" | "tile") {
        let label = if action == "tile" { "Crossfade" } else { "Fade (GLB in / out)" };
        changed |= slider(ui, label, cmd, "fade", 0.0..=5.0, 0.0, " s");
    }
    // Back as it was after that long (a shape-key animation and a prefab end by themselves)
    if action != "shape_anim" && action != "prefab" {
        changed |= slider(ui, "Timer (then back as it was; 0 = it stays)", cmd, "timer", 0.0..=600.0, 0.0, " s");
        ui.label(egui::RichText::new("A colour, a GLB or group's Show, a tile and its shape key, a shape key, an animation: undone when the timer runs out.").small().color(egui::Color32::from_rgb(148, 163, 184)));
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcards_go_into_every_text() {
        let cmd = json!({ "action": "color", "slot": 0, "color": "#{1}", "source": "Puppet {2}" });
        let out = command(&cmd, &["ff0000".to_string(), "Main".to_string()]);
        assert_eq!(out, json!({ "action": "color", "slot": 0, "color": "#ff0000", "source": "Puppet Main" }));
    }

    #[test]
    fn a_redeem_without_puppet_or_command_says_no() {
        // No OBS in tests: no Puppet to call
        assert!(!fire(&json!({ "type": "puppet" }), &[]));
        assert!(!fire(&json!({ "type": "puppet", "puppet": { "action": "list" } }), &[]));
    }
}
