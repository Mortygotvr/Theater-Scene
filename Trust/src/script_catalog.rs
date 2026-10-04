//! Every script Theater or OBS uses, for the Studio's Scripts tab, grouped by
//! what it's for:
//!  * reactions: shaders Theater runs on a target when it's hit (a
//!    `shader_code = [[…]]` block; bite_vanish, swirl…),
//!  * redeems: whole redeem types (`function on_redeem`; doom, snow_storm…),
//!  * filters: OBS filters — Lua ones OBS loads itself (Tools → Scripts) and
//!    Rhai ones Theater hosts (script_filters).
//!
//! A file can be more than one (swirl.lua is an OBS filter and a reaction).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::lua_shield::LuaThreat;
use crate::studio_app::ScriptSlider;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Lang {
    Lua,
    Rhai,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    Reaction,
    Redeem,
    Filter,
}

#[derive(Debug, Clone)]
pub struct ScriptEntry {
    pub path: PathBuf,
    /// Its file name without the extension
    pub name: String,
    pub lang: Lang,
    pub kinds: Vec<Kind>,
    /// The OBS filter (or source) id it registers
    pub filter_id: Option<String>,
    /// OBS loads it (Tools → Scripts) in the current scene collection
    pub loaded_by_obs: bool,
    /// Why it doesn't compile (or, for a Rhai filter, isn't running)
    pub problem: Option<String>,
    /// Redeem scripts: calls their sandbox blocks (lua_shield)
    pub threats: Vec<LuaThreat>,
    /// Redeem scripts: their `-- @param` sliders
    pub sliders: Vec<ScriptSlider>,
    /// Reactions: their settings and defaults
    pub reaction_settings: Vec<(String, f32)>,
    pub modified: Option<SystemTime>,
}

impl ScriptEntry {
    pub fn is(&self, kind: Kind) -> bool {
        self.kinds.contains(&kind)
    }
}

/// What a Lua script is for, from its text: (kinds, the OBS id it registers).
pub fn classify_lua(source: &str) -> (Vec<Kind>, Option<String>) {
    let mut kinds = Vec::new();
    if crate::obs_source::extract_shader_code_from_lua(source).is_some() {
        kinds.push(Kind::Reaction);
    }
    if source.contains("function on_redeem") {
        kinds.push(Kind::Redeem);
    }
    let filter_id = source.contains("obs_register_source").then(|| lua_source_id(source)).flatten();
    if source.contains("obs_register_source") {
        kinds.push(Kind::Filter);
    }
    (kinds, filter_id)
}

/// The id in a Lua script's source info (`….id = "…"`).
fn lua_source_id(source: &str) -> Option<String> {
    let start = source.find(".id = \"")? + ".id = \"".len();
    let rest = &source[start..];
    Some(rest[..rest.find('"')?].to_string())
}

/// Reads and sorts out one script file.
pub fn read_entry(path: &Path) -> Option<ScriptEntry> {
    let lang = match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "lua" => Lang::Lua,
        "rhai" => Lang::Rhai,
        _ => return None,
    };
    let source = std::fs::read_to_string(path).ok()?;
    let mut entry = ScriptEntry {
        path: path.to_path_buf(),
        name: path.file_stem()?.to_string_lossy().into_owned(),
        lang,
        kinds: Vec::new(),
        filter_id: None,
        loaded_by_obs: false,
        problem: None,
        threats: Vec::new(),
        sliders: Vec::new(),
        reaction_settings: Vec::new(),
        modified: std::fs::metadata(path).and_then(|m| m.modified()).ok(),
    };
    match lang {
        Lang::Lua => {
            (entry.kinds, entry.filter_id) = classify_lua(&source);
            if let Err(e) = mlua::Lua::new().load(&source).set_name(entry.name.as_str()).into_function() {
                entry.problem = Some(e.to_string().lines().next().unwrap_or_default().to_string());
            }
            if entry.is(Kind::Redeem) {
                entry.threats = crate::lua_shield::scan(&source);
                entry.sliders = crate::studio_app::script_sliders(&source);
            }
            if entry.is(Kind::Reaction) {
                entry.reaction_settings = crate::obs_source::extract_default_floats_from_lua(&source);
            }
        }
        Lang::Rhai => {
            entry.kinds = vec![Kind::Filter];
            match crate::script_filters::compile(&source, path) {
                Ok(compiled) => entry.filter_id = Some(compiled.id),
                Err(e) => entry.problem = Some(e),
            }
        }
    }
    Some(entry)
}

/// The scripts OBS loads (Tools → Scripts) in the scene collection `json`.
pub fn obs_scripts(json: &serde_json::Value) -> Vec<PathBuf> {
    json.pointer("/modules/scripts-tool")
        .and_then(|s| s.as_array())
        .map(|list| {
            list.iter()
                .filter_map(|s| s.get("path")?.as_str().map(PathBuf::from))
                .collect()
        })
        .unwrap_or_default()
}

/// Which sources carry each filter id in the scene collection `json`:
/// id → ["Source (filter name)", …], "(off)" after a disabled one.
pub fn filter_users(json: &serde_json::Value) -> HashMap<String, Vec<String>> {
    let mut users: HashMap<String, Vec<String>> = HashMap::new();
    for list in ["sources", "groups"] {
        for source in json.get(list).and_then(|s| s.as_array()).into_iter().flatten() {
            let owner = source.get("name").and_then(|n| n.as_str()).unwrap_or("?");
            for filter in source.get("filters").and_then(|f| f.as_array()).into_iter().flatten() {
                let Some(id) = filter.get("id").and_then(|i| i.as_str()) else { continue };
                let name = filter.get("name").and_then(|n| n.as_str()).unwrap_or(id);
                let off = filter.get("enabled").and_then(|e| e.as_bool()) == Some(false);
                users
                    .entry(id.to_string())
                    .or_default()
                    .push(format!("{} ({}){}", owner, name, if off { " (off)" } else { "" }));
            }
        }
    }
    users
}

/// OBS's current scene collection file, from its user.ini (global.ini
/// before OBS 31).
pub fn current_scene_collection_file() -> Option<PathBuf> {
    let dir = obs_config_dir()?;
    for ini in ["user.ini", "global.ini"] {
        let Ok(text) = std::fs::read_to_string(dir.join(ini)) else { continue };
        let mut in_basic = false;
        for line in text.lines() {
            let line = line.trim();
            if line.starts_with('[') {
                in_basic = line == "[Basic]";
            } else if in_basic {
                if let Some(file) = line.strip_prefix("SceneCollectionFile=") {
                    let file = if file.ends_with(".json") { file.to_string() } else { format!("{}.json", file) };
                    let path = dir.join("basic").join("scenes").join(file);
                    if path.is_file() {
                        return Some(path);
                    }
                }
            }
        }
    }
    None
}

fn obs_config_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    return std::env::var("APPDATA").ok().map(|a| PathBuf::from(a).join("obs-studio"));
    #[cfg(not(target_os = "windows"))]
    return std::env::var("HOME").ok().map(|h| PathBuf::from(h).join(".config").join("obs-studio"));
}

/// Everything the Scripts tab shows, from one scan.
#[derive(Debug, Clone, Default)]
pub struct Catalog {
    pub entries: Vec<ScriptEntry>,
    /// Filter id → the sources carrying it (see filter_users)
    pub filter_users: HashMap<String, Vec<String>>,
}

/// Scans `dirs` (and `extra` files) for scripts, adding what the current
/// scene collection says about them. Files and folders only: no OBS calls,
/// so it can run on any thread.
pub fn scan(dirs: &[PathBuf], extra: &[PathBuf]) -> Catalog {
    let collection = current_scene_collection_file()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .unwrap_or(serde_json::Value::Null);
    let obs_loaded = obs_scripts(&collection);

    let mut paths: Vec<PathBuf> = Vec::new();
    for dir in dirs {
        let Ok(read) = std::fs::read_dir(dir) else { continue };
        paths.extend(read.flatten().map(|e| e.path()));
    }
    paths.extend(extra.iter().cloned());
    paths.extend(obs_loaded.iter().cloned());

    let same = |a: &Path, b: &Path| a == b || matches!((a.canonicalize(), b.canonicalize()), (Ok(x), Ok(y)) if x == y);
    let mut entries: Vec<ScriptEntry> = Vec::new();
    for path in paths {
        if !path.is_file() || entries.iter().any(|e| same(&e.path, &path)) {
            continue;
        }
        let Some(mut entry) = read_entry(&path) else { continue };
        if entry.kinds.is_empty() {
            continue;
        }
        entry.loaded_by_obs = obs_loaded.iter().any(|p| same(p, &path));
        entries.push(entry);
    }
    entries.sort_by_key(|e| e.name.to_lowercase());
    Catalog { entries, filter_users: filter_users(&collection) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scripts_are_sorted_by_what_they_are_for() {
        let reaction = "obs = obslua\nlocal shader_code = [[ technique Draw {} ]]";
        assert_eq!(classify_lua(reaction), (vec![Kind::Reaction], None));
        let redeem = "-- @param x 1 0 2 X\nfunction on_redeem(params) end";
        assert_eq!(classify_lua(redeem), (vec![Kind::Redeem], None));
        let filter = "local shader_code = [[ x ]]\nfilter_info.id = \"my_filter\"\nobs.obs_register_source(filter_info)";
        assert_eq!(classify_lua(filter), (vec![Kind::Reaction, Kind::Filter], Some("my_filter".to_string())));
        assert_eq!(classify_lua("print('hi')"), (vec![], None));
    }

    #[test]
    fn the_scene_collection_says_what_obs_loads_and_where_filters_are() {
        let json = serde_json::json!({
            "modules": { "scripts-tool": [ { "path": "C:/x/a.lua" }, { "path": "C:/x/b.lua" } ] },
            "sources": [
                { "name": "Cam", "filters": [ { "id": "blur", "name": "Blur" }, { "id": "fix", "name": "Fix", "enabled": false } ] },
                { "name": "Game", "filters": [ { "id": "fix", "name": "Fix 2" } ] }
            ],
            "groups": [ { "name": "G", "filters": [ { "id": "blur", "name": "B" } ] } ]
        });
        assert_eq!(obs_scripts(&json), [PathBuf::from("C:/x/a.lua"), PathBuf::from("C:/x/b.lua")]);
        let users = filter_users(&json);
        assert_eq!(users["fix"], ["Cam (Fix) (off)", "Game (Fix 2)"]);
        assert_eq!(users["blur"], ["Cam (Blur)", "G (B)"]);
    }

    #[test]
    fn theaters_own_scripts_are_found_and_sorted_out() {
        let dirs = [crate::studio_app::get_filters_dir(), crate::get_theater_repo_dir()];
        let catalog = scan(&dirs, &[]);
        let find = |name: &str| catalog.entries.iter().find(|e| e.name == name);
        if let Some(doom) = find("doom") {
            assert!(doom.is(Kind::Redeem) && doom.problem.is_none() && doom.threats.is_empty());
            assert!(doom.sliders.iter().any(|s| s.name == "tumble_speed"));
        }
        if let Some(bite) = find("bite_vanish") {
            assert!(bite.is(Kind::Reaction));
            assert!(bite.reaction_settings.iter().any(|(k, _)| k == "bite_size"));
        }
        let fixes: Vec<&ScriptEntry> = catalog.entries.iter().filter(|e| e.name == "transparency_fix").collect();
        if fixes.len() == 2 {
            assert!(fixes.iter().all(|e| e.is(Kind::Filter) && e.filter_id.as_deref() == Some("theater_transparency_fix_filter")));
            assert!(fixes.iter().any(|e| e.lang == Lang::Rhai) && fixes.iter().any(|e| e.lang == Lang::Lua));
        }
    }
}
