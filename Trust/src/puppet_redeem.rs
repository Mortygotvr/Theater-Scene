//! The "🎭 Puppet" redeem type: a redeem that drives a Theater Puppet avatar
//! (the Theater-Puppet OBS plugin) through its procedure
//! `theater_puppet_command`: play an animation, toggle or ping-pong a shape
//! key, animate a shape key, show or hide a GLB, set a colour slot.
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

const ACTIONS: [(&str, &str); 7] = [
    ("animation", "🎬 Play an animation"),
    ("shape", "🙂 Shape key on / off / ping-pong"),
    ("shape_anim", "📈 Animate a shape key"),
    ("glb", "👁 Show / hide a GLB"),
    ("group", "🔀 Show a GLB of a group"),
    ("tile", "🧩 Tile texture"),
    ("color", "🎨 Set a colour slot"),
];

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
            let names = |key: &str| -> Vec<String> {
                g.and_then(|g| g.get(key)).and_then(|a| a.as_array()).map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_default()
            };
            let mut by_name = cmd.get("name").and_then(|v| v.as_str()).is_some_and(|n| !n.is_empty()) || cmd.get("material").is_none();
            ui.horizontal(|ui| {
                if ui.radio_value(&mut by_name, true, "A tile of the model (VRM expression)").changed() | ui.radio_value(&mut by_name, false, "A cell of a grid").changed() {
                    changed = true;
                }
            });
            if by_name {
                if let Some(m) = cmd.as_object_mut() {
                    m.remove("material");
                }
                changed |= text_with_choices(ui, "Tile:", cmd, "name", &names("tiles"), "its name");
            } else {
                if cmd.get("material").is_none() {
                    cmd["material"] = json!("");
                    cmd["name"] = json!("");
                    changed = true;
                }
                changed |= text_with_choices(ui, "Material:", cmd, "material", &names("materials"), "the texture's material");
                for (key, label) in [("columns", "Columns"), ("rows", "Rows")] {
                    let mut n = get_f(cmd, key, 2.0) as i64;
                    ui.horizontal(|ui| {
                        ui.label(label);
                        if ui.add(egui::DragValue::new(&mut n).range(1..=64)).changed() {
                            changed = true;
                        }
                    });
                    cmd[key] = json!(n);
                }
                if cmd.get("cell").is_none() {
                    cmd["cell"] = json!("0");
                }
                changed |= text_with_choices(ui, "Cell:", cmd, "cell", &["next".to_string(), "random".to_string()], "0 = top left, across then down; next; random; {1}");
            }
            changed |= choice(ui, "State:", cmd, "state", &[("on", "On"), ("off", "Off (back to its own)"), ("toggle", "Toggle")]);
            changed |= slider(ui, "Strength", cmd, "value", 0.0..=1.0, 1.0, "");
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
        _ => {}
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
