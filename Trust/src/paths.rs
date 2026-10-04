//! Where Theater Scene's files are.
//!
//! - **Your data** (`config_dir`): poses, redeems, settings and the scripts
//!   you make. OBS's folder for the plugin's settings:
//!   `%APPDATA%\obs-studio\plugin_config\theater-scene` on Windows,
//!   `~/.config/obs-studio/plugin_config/theater-scene` on Linux. Your files
//!   from before, when they were kept next to Theater's code, are copied in
//!   once (`import_old_data`).
//! - **What ships with Theater** (`data_dir`): scripts, shaders, textures and
//!   fonts, in the plugin's `data` folder (`…\plugins\theater-scene\data`).
//!   With `THEATER_SCENE_DIR` set to a checkout of the repo, its files are used
//!   instead, so they can be edited live.

use std::ffi::{CStr, c_char, c_void};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicPtr, Ordering};

/// The plugin's folder name, in plugins\ and plugin_config\
const PLUGIN: &str = "theater-scene";

/// OBS's handle of this module (obs_module_set_pointer): null in a copy a Lua
/// script loads, which then uses OBS's usual folders.
static MODULE: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());

pub fn set_module(module: *mut c_void) {
    MODULE.store(module, Ordering::SeqCst);
}

/// A string libobs allocated, as a path (freed)
fn take_obs_path(p: *mut c_char) -> Option<PathBuf> {
    if p.is_null() {
        return None;
    }
    let s = unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned();
    let bfree = crate::obs_source::obs_symbol(c"bfree");
    if !bfree.is_null() {
        let bfree: unsafe extern "C" fn(*mut c_void) = unsafe { std::mem::transmute(bfree) };
        unsafe { bfree(p as *mut c_void) };
    }
    // ("…\theater-scene/": without the separator)
    Some(PathBuf::from(s.trim_end_matches(['/', '\\'])))
}

// ─── Your data ──────────────────────────────────────────────────────────────

/// Your data's folder (made when first asked for).
pub fn config_dir() -> PathBuf {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        let dir = if cfg!(test) {
            // Tests never touch your real data: a folder of their own, with
            // the made-up poses and redeem of tests\fixtures\config
            let dir = std::env::temp_dir().join(format!("theater_scene_test_config_{}", std::process::id()));
            copy_missing_files(&Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("fixtures").join("config"), &dir);
            dir
        } else {
            module_config_dir().unwrap_or_else(default_config_dir)
        };
        let _ = std::fs::create_dir_all(&dir);
        dir
    })
    .clone()
}

/// OBS's config folder for this module
fn module_config_dir() -> Option<PathBuf> {
    let module = MODULE.load(Ordering::SeqCst);
    if module.is_null() {
        return None;
    }
    let get = crate::obs_source::obs_symbol(c"obs_module_get_config_path");
    if get.is_null() {
        return None;
    }
    let get: unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut c_char = unsafe { std::mem::transmute(get) };
    take_obs_path(unsafe { get(module, c"".as_ptr()) })
}

/// Where OBS keeps plugin settings when it isn't portable
fn default_config_dir() -> PathBuf {
    obs_user_dir().join("plugin_config").join(PLUGIN)
}

/// OBS's folder of the user's: %APPDATA%\obs-studio, ~/.config/obs-studio
fn obs_user_dir() -> PathBuf {
    if cfg!(target_os = "windows") {
        PathBuf::from(std::env::var_os("APPDATA").unwrap_or_default()).join("obs-studio")
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
            .unwrap_or_default()
            .join("obs-studio")
    }
}

/// One of your data files (theater_targets.json, theater_settings.json…).
pub fn file(name: &str) -> PathBuf {
    config_dir().join(name)
}

/// Your redeems' folder.
pub fn redeems_dir() -> PathBuf {
    config_dir().join("redeems")
}

/// Where the custom redeem scripts you make in the Studio go, next to the
/// shipped ones' scripts_dir (made when first asked for).
pub fn user_scripts_dir() -> PathBuf {
    let dir = config_dir().join("scripts");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Where the effects and Rhai filters you make in the Studio go, next to the
/// shipped ones' filters_dir (made when first asked for).
pub fn user_filters_dir() -> PathBuf {
    let dir = config_dir().join("filters");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// A redeem's own file (redeems\<key>.json), by its key as saved (lower
/// case, no spaces) or as given.
pub fn redeem_file(key: &str) -> Option<PathBuf> {
    let saved = key.to_lowercase().replace(' ', "");
    [saved, key.to_string()].into_iter().map(|k| redeems_dir().join(format!("{k}.json"))).find(|p| p.is_file())
}

// ─── What ships with Theater ────────────────────────────────────────────────

/// The folder of the files Theater ships with.
pub fn data_dir() -> PathBuf {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        if let Some(dir) = std::env::var_os("THEATER_SCENE_DIR").map(PathBuf::from).filter(|d| d.join("filters").is_dir()) {
            return dir;
        }
        if cfg!(test) {
            // The repo the tests are built from
            return Path::new(env!("CARGO_MANIFEST_DIR")).parent().map(Path::to_path_buf).unwrap_or_default();
        }
        module_data_dir().or_else(dll_data_dir).unwrap_or_else(|| obs_user_dir().join("plugins").join(PLUGIN).join("data"))
    })
    .clone()
}

/// OBS's data folder for this module
fn module_data_dir() -> Option<PathBuf> {
    let module = MODULE.load(Ordering::SeqCst);
    if module.is_null() {
        return None;
    }
    let get = crate::obs_source::obs_symbol(c"obs_get_module_data_path");
    if get.is_null() {
        return None;
    }
    let get: unsafe extern "C" fn(*mut c_void) -> *const c_char = unsafe { std::mem::transmute(get) };
    let p = unsafe { get(module) };
    if p.is_null() {
        return None;
    }
    // (OBS's own string: not freed)
    let dir = PathBuf::from(unsafe { CStr::from_ptr(p) }.to_string_lossy().trim_end_matches(['/', '\\']));
    dir.is_dir().then_some(dir)
}

/// …\theater-scene\data, from this DLL in …\theater-scene\bin\64bit
fn dll_data_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        unsafe extern "system" {
            fn GetModuleHandleExW(flags: u32, name: *const u16, module: *mut *mut c_void) -> i32;
            fn GetModuleFileNameW(module: *mut c_void, file: *mut u16, size: u32) -> u32;
        }
        const FROM_ADDRESS: u32 = 0x4;
        const UNCHANGED_REFCOUNT: u32 = 0x2;
        let mut module: *mut c_void = std::ptr::null_mut();
        if unsafe { GetModuleHandleExW(FROM_ADDRESS | UNCHANGED_REFCOUNT, dll_data_dir as *const u16, &mut module) } == 0 {
            return None;
        }
        let mut buf = [0u16; 1024];
        let len = unsafe { GetModuleFileNameW(module, buf.as_mut_ptr(), buf.len() as u32) } as usize;
        if len == 0 || len >= buf.len() {
            return None;
        }
        let dll = PathBuf::from(String::from_utf16_lossy(&buf[..len]));
        let dir = dll.parent()?.parent()?.parent()?.join("data");
        dir.is_dir().then_some(dir)
    }
    #[cfg(not(target_os = "windows"))]
    None
}

/// Shipped scripts and filters: Rhai sources and filters, Lua effects,
/// reaction shaders.
pub fn filters_dir() -> PathBuf {
    data_dir().join("filters")
}

/// Shipped custom redeem scripts (doom.lua, snow_storm.lua).
pub fn scripts_dir() -> PathBuf {
    data_dir().join("scripts")
}

/// Icons, fonts and textures: data\assets, or Trust\assets in a checkout of the repo.
pub fn assets_dir() -> PathBuf {
    let data = data_dir();
    let in_repo = data.join("Trust").join("assets");
    if in_repo.is_dir() { in_repo } else { data.join("assets") }
}

/// A shipped texture (smooth_deformed_glass.png…), as a path string.
pub fn texture(name: &str) -> String {
    assets_dir().join("textures").join(name).to_string_lossy().into_owned()
}

// ─── Finding a file something names ─────────────────────────────────────────

/// A file a redeem or script names (a picture, a sound, a script): as given
/// when it exists, else relative to your redeems, your data, then what ships
/// with Theater.
pub fn find_file(raw: &str) -> Option<PathBuf> {
    let raw = raw.trim();
    if raw.is_empty() || raw.starts_with("data:") {
        return None;
    }
    let path = Path::new(raw);
    if path.is_file() {
        return Some(path.to_path_buf());
    }
    if !path.is_relative() {
        return None;
    }
    [redeems_dir(), config_dir(), data_dir(), assets_dir()].into_iter().map(|base| base.join(path)).find(|p| p.is_file())
}

// ─── Your files from before ─────────────────────────────────────────────────

/// What Theater kept next to its code before it used config_dir.
const OLD_DATA: [&str; 7] = [
    "theater_targets.json",
    "theater_last_pose.txt",
    "theater_settings.json",
    "theater_script_folders.json",
    "theater_removed_effects.json",
    "theater_reader_seen.json",
    "theater.json",
];

/// Says the old files were looked at (and from where).
const IMPORTED_MARK: &str = "imported_from.txt";

/// Folders Theater's files may have been kept in before
fn old_data_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(dir) = std::env::var_os("THEATER_SCENE_DIR") {
        dirs.push(PathBuf::from(dir));
    }
    if let Some(home) = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")) {
        let repo = PathBuf::from(home).join("repo");
        for name in ["theater scene(rust)", "Theater-Scene", "theater-scene"] {
            dirs.push(repo.join(name));
        }
    }
    dirs
}

/// Copies your poses, redeems and settings from where Theater kept them
/// before into config_dir, once. Nothing is moved or overwritten: the old
/// files stay where they were. Call before anything reads them.
pub fn import_old_data() {
    if cfg!(test) {
        return;
    }
    let config = config_dir();
    if config.join(IMPORTED_MARK).exists() {
        return;
    }
    // Data of its own already: nothing to import
    if config.join("theater_targets.json").exists() {
        let _ = std::fs::write(config.join(IMPORTED_MARK), "(nothing: theater_targets.json was already here)\n");
        return;
    }
    let Some(old) = old_data_dirs().into_iter().find(|d| d.join("theater_targets.json").is_file()) else {
        return;
    };
    let mut copied = 0;
    for name in OLD_DATA {
        let (from, to) = (old.join(name), config.join(name));
        if from.is_file() && !to.exists() && std::fs::copy(&from, &to).is_ok() {
            copied += 1;
        }
    }
    copied += copy_missing_files(&old.join("redeems"), &redeems_dir());
    let _ = std::fs::write(config.join(IMPORTED_MARK), format!("{}\n", old.display()));
    crate::obs_source::trust_log(&format!(
        "[Trust Core] 📥 Copied {} file(s) of your poses, redeems and settings from {} to {}",
        copied,
        old.display(),
        config.display()
    ));
}

/// Copies the files of `from` (and its folders) that `to` doesn't have yet.
fn copy_missing_files(from: &Path, to: &Path) -> usize {
    let Ok(entries) = std::fs::read_dir(from) else { return 0 };
    let _ = std::fs::create_dir_all(to);
    let mut copied = 0;
    for entry in entries.flatten() {
        let (src, dst) = (entry.path(), to.join(entry.file_name()));
        if src.is_dir() {
            copied += copy_missing_files(&src, &dst);
        } else if !dst.exists() && std::fs::copy(&src, &dst).is_ok() {
            copied += 1;
        }
    }
    copied
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tests_use_a_folder_of_their_own_and_the_repos_files() {
        assert!(config_dir().starts_with(std::env::temp_dir()), "never your real data");
        assert!(filters_dir().join("swirl.lua").is_file());
        assert!(scripts_dir().join("doom.lua").is_file());
        assert!(Path::new(&texture("smooth_deformed_glass.png")).is_file());
        assert!(assets_dir().join("fonts").is_dir());
    }

    #[test]
    fn named_files_are_found_where_they_are() {
        assert_eq!(find_file("filters/swirl.lua"), Some(data_dir().join("filters/swirl.lua")));
        assert_eq!(find_file("textures/deformed_glass.png"), Some(assets_dir().join("textures/deformed_glass.png")));
        assert_eq!(find_file(""), None);
        assert_eq!(find_file("data:image/png;base64,AAAA"), None);
        assert_eq!(find_file("no_such_file_anywhere.png"), None);

        // The fixture's redeem, by the key as typed
        assert_eq!(redeem_file("Throw Tomato"), Some(redeems_dir().join("throwtomato.json")));
        assert_eq!(redeem_file("Nothing Like It"), None);
    }

    #[test]
    fn missing_files_are_copied_and_nothing_is_overwritten() {
        let root = std::env::temp_dir().join(format!("theater_paths_copy_{}", std::process::id()));
        let (from, to) = (root.join("from"), root.join("to"));
        std::fs::create_dir_all(from.join("sub")).unwrap();
        std::fs::create_dir_all(&to).unwrap();
        std::fs::write(from.join("a.json"), "new").unwrap();
        std::fs::write(from.join("sub/b.json"), "b").unwrap();
        std::fs::write(to.join("a.json"), "mine").unwrap();
        assert_eq!(copy_missing_files(&from, &to), 1);
        assert_eq!(std::fs::read_to_string(to.join("a.json")).unwrap(), "mine", "kept");
        assert_eq!(std::fs::read_to_string(to.join("sub/b.json")).unwrap(), "b");
        let _ = std::fs::remove_dir_all(&root);
    }
}
