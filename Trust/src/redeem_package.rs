//! Redeem packages (.red): a zip of a redeem's settings (redeem.json) and
//! every file it needs — its custom script and the files that script names,
//! images, sounds, the water texture, reaction scripts — so a redeem made on
//! one PC works on another. Importing unpacks it into its own folder in
//! redeems/, after lua_shield has checked its scripts.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use crate::lua_shield::LuaThreat;
use crate::studio_app::{RedeemConfig, RedeemItem};

/// The redeem's settings, inside a package
const SETTINGS: &str = "redeem.json";
/// The setting holding the custom redeem script's path
const SCRIPT_FIELD: &str = "customRedeemScript";
/// Settings holding a file's path, with the setting embedding that same file
/// as a data URI
const ASSET_FIELDS: &[(&str, Option<&str>)] = &[
    ("imageFilePath", Some("imageSrc")),
    ("startSoundPath", Some("startSound")),
    ("collisionSoundPath", Some("collisionSound")),
    ("contactSoundPath", Some("contactSound")),
    ("waterTexturePath", None),
];
/// The most a package may unpack to (a zip can claim any size)
const MAX_UNPACKED: u64 = 512 * 1024 * 1024;
/// How far scripts naming scripts are followed
const MAX_SCRIPT_DEPTH: usize = 4;

#[derive(Debug)]
pub struct ExportReport {
    /// Files packed besides the settings
    pub files: usize,
    /// Files the redeem names that weren't found, so aren't in the package
    pub missing: Vec<String>,
}

/// An opened package, not unpacked yet.
#[derive(Debug)]
pub struct Package {
    pub settings: serde_json::Value,
    /// Each file's path in the package ("doom.lua", "filters/swirl.lua") and
    /// contents
    pub files: Vec<(String, Vec<u8>)>,
    /// Blocked calls in its scripts, with the script's path in the package
    pub threats: Vec<(String, LuaThreat)>,
}

/// Writes `config` and the files it needs to `out`, as a .red package. The
/// custom script goes at the package's root, and the files it names at the
/// same paths from it as from the Theater folder (see
/// custom_lua::localize_paths); every other file at the root.
pub fn export(config: &RedeemConfig, out: &Path) -> Result<ExportReport, String> {
    let mut settings = serde_json::to_value(config).map_err(|e| e.to_string())?;
    let mut files = Files::default();
    let mut missing = Vec::new();

    if let Some(raw) = text(&settings, &[SCRIPT_FIELD]) {
        match find_script(&raw) {
            Some(disk) => {
                let at = files.add_at_root(&disk);
                pack_named_files(&disk, &mut files, &mut missing, 0);
                set_text(&mut settings, &[SCRIPT_FIELD], at);
            }
            None => missing.push(raw),
        }
    }

    for &(field, embedded) in ASSET_FIELDS {
        let Some(raw) = text(&settings, &[field]) else { continue };
        match find_asset(&raw) {
            Some(disk) => {
                let at = files.add_at_root(&disk);
                set_text(&mut settings, &[field], at);
                // The file travels itself: its copy in the settings needn't
                if let (Some(embedded), Some(map)) = (embedded, settings.as_object_mut()) {
                    map.remove(embedded);
                }
            }
            // Still embedded in the settings, and travels that way
            None if embedded.is_some_and(|e| text(&settings, &[e]).is_some()) => {}
            None => missing.push(raw),
        }
    }

    // Reaction scripts: impactReactions.<reaction>.path
    let reactions: Vec<String> = settings
        .get("impactReactions")
        .and_then(|r| r.as_object())
        .map(|r| r.keys().cloned().collect())
        .unwrap_or_default();
    for reaction in reactions {
        let keys = ["impactReactions", reaction.as_str(), "path"];
        let Some(raw) = text(&settings, &keys) else { continue };
        match find_script(&raw) {
            Some(disk) => {
                let at = files.add_at_root(&disk);
                set_text(&mut settings, &keys, at);
            }
            None => missing.push(raw),
        }
    }

    // Written aside first: a failed export doesn't leave half a package
    let mut part = out.as_os_str().to_owned();
    part.push(".part");
    let part = PathBuf::from(part);
    let written = write_zip(&part, &settings, &files).and_then(|()| {
        std::fs::rename(&part, out).map_err(|e| format!("Couldn't write {}: {}", out.display(), e))
    });
    if written.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    written?;
    Ok(ExportReport { files: files.entries.len(), missing })
}

/// Reads a .red package and checks its scripts (lua_shield::scan).
pub fn open(path: &Path) -> Result<Package, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("Couldn't open {}: {}", path.display(), e))?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|e| format!("{} isn't a redeem package: {}", path.display(), e))?;
    let mut settings = None;
    let mut files = Vec::new();
    let mut unpacked = 0u64;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        if entry.is_dir() {
            continue;
        }
        // A name like "../../x" would unpack outside the redeem's folder
        let at = entry
            .enclosed_name()
            .and_then(|p| package_path(&p.to_string_lossy()))
            .ok_or_else(|| format!("The package has a file that would land outside its folder: {}", entry.name()))?;
        let mut bytes = Vec::new();
        (&mut entry)
            .take(MAX_UNPACKED - unpacked + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| format!("Couldn't read {} in the package: {}", at, e))?;
        unpacked += bytes.len() as u64;
        if unpacked > MAX_UNPACKED {
            return Err(format!("The package unpacks to more than {} MB", MAX_UNPACKED / (1024 * 1024)));
        }
        if at == SETTINGS {
            settings = Some(serde_json::from_slice(&bytes).map_err(|e| format!("Its {} doesn't read: {}", SETTINGS, e))?);
        } else {
            files.push((at, bytes));
        }
    }
    let settings: serde_json::Value =
        settings.ok_or_else(|| format!("The package has no {}: it isn't a redeem", SETTINGS))?;
    serde_json::from_value::<RedeemConfig>(settings.clone())
        .map_err(|e| format!("Its {} isn't a redeem: {}", SETTINGS, e))?;
    let threats = files
        .iter()
        .filter(|(at, _)| at.to_lowercase().ends_with(".lua"))
        .flat_map(|(at, bytes)| {
            crate::lua_shield::scan(&String::from_utf8_lossy(bytes))
                .into_iter()
                .map(move |t| (at.clone(), t))
        })
        .collect();
    Ok(Package { settings, files, threats })
}

/// Adds `package` to the Studio's redeems, named after `name`.
pub fn install(package: &Package, name: &str) -> Result<RedeemItem, String> {
    install_into(package, name, &crate::studio_app::get_redeems_dir())
}

/// Unpacks `package` as a new redeem named after `name` (made unique): its
/// files in `redeems/<name>/`, its settings in `redeems/<name>.json` pointing
/// at them.
fn install_into(package: &Package, name: &str, redeems: &Path) -> Result<RedeemItem, String> {
    std::fs::create_dir_all(redeems).map_err(|e| format!("Couldn't create {}: {}", redeems.display(), e))?;
    let stem = unique_stem(redeems, &file_stem_from(name));
    let folder = redeems.join(&stem);
    for (at, bytes) in &package.files {
        let dest = on_disk(&folder, at);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("Couldn't create {}: {}", parent.display(), e))?;
        }
        std::fs::write(&dest, bytes).map_err(|e| format!("Couldn't write {}: {}", dest.display(), e))?;
    }

    // Paths into the package now name the unpacked files
    let unpacked = |raw: &str| -> Option<String> {
        let at = package_path(raw)?;
        package
            .files
            .iter()
            .any(|(p, _)| *p == at)
            .then(|| on_disk(&folder, &at).to_string_lossy().into_owned())
    };
    let mut settings = package.settings.clone();
    let mut fields: Vec<Vec<String>> = std::iter::once(SCRIPT_FIELD)
        .chain(ASSET_FIELDS.iter().map(|(f, _)| *f))
        .map(|f| vec![f.to_string()])
        .collect();
    if let Some(reactions) = settings.get("impactReactions").and_then(|r| r.as_object()) {
        fields.extend(reactions.keys().map(|r| vec!["impactReactions".to_string(), r.clone(), "path".to_string()]));
    }
    for keys in fields {
        let keys: Vec<&str> = keys.iter().map(String::as_str).collect();
        if let Some(full) = text(&settings, &keys).and_then(|raw| unpacked(&raw)) {
            set_text(&mut settings, &keys, full);
        }
    }

    let mut config: RedeemConfig = serde_json::from_value(settings).map_err(|e| e.to_string())?;
    // Images and sounds are embedded in the settings too, as when redeems load
    fn embed(data: &mut Option<String>, path: &Option<String>) {
        if data.as_deref().unwrap_or("").is_empty() {
            if let Some(uri) = path.as_deref().and_then(crate::studio_app::resolve_and_encode_asset) {
                *data = Some(uri);
            }
        }
    }
    embed(&mut config.image_src, &config.image_file_path);
    embed(&mut config.start_sound, &config.start_sound_path);
    embed(&mut config.collision_sound, &config.collision_sound_path);
    embed(&mut config.contact_sound, &config.contact_sound_path);

    let filename = format!("{}.json", stem);
    let mut item = RedeemItem {
        path: redeems.join(&filename),
        filename,
        triggers_input: config.triggers.as_ref().map(|t| t.join(", ")).unwrap_or_default(),
        config,
        is_dirty: false,
    };
    crate::studio_app::save_redeem_to_disk(&mut item)?;
    Ok(item)
}

/// What goes in a package besides its settings: each file's path in the
/// package and where it is on disk.
#[derive(Default)]
struct Files {
    entries: Vec<(String, PathBuf)>,
}

impl Files {
    fn has(&self, at: &str) -> bool {
        self.entries.iter().any(|(p, _)| p == at)
    }

    /// Packs `disk` at `at`. False if something is there already (this file,
    /// or another one).
    fn add_at(&mut self, at: &str, disk: &Path) -> bool {
        if self.has(at) {
            return false;
        }
        self.entries.push((at.to_string(), disk.to_path_buf()));
        true
    }

    /// Packs `disk` at the package's root under its own name, or 2-name,
    /// 3-name… when another file has it. Where it is in the package (where it
    /// already was, if it's in).
    fn add_at_root(&mut self, disk: &Path) -> String {
        if let Some((at, _)) = self.entries.iter().find(|(_, d)| same_file(d, disk)) {
            return at.clone();
        }
        let name = disk
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "file".to_string());
        let mut at = name.clone();
        let mut n = 2;
        while at == SETTINGS || self.has(&at) {
            at = format!("{}-{}", n, name);
            n += 1;
        }
        self.entries.push((at.clone(), disk.to_path_buf()));
        at
    }
}

fn same_file(a: &Path, b: &Path) -> bool {
    a == b || matches!((a.canonicalize(), b.canonicalize()), (Ok(x), Ok(y)) if x == y)
}

/// Packs the files `script` names (named_files), and the ones the scripts
/// among them name.
fn pack_named_files(script: &Path, files: &mut Files, missing: &mut Vec<String>, depth: usize) {
    let Ok(source) = std::fs::read_to_string(script) else { return };
    let script_dir = script.parent().unwrap_or(Path::new("")).to_path_buf();
    let (literals, declared) = named_files(&source);
    let named = literals.into_iter().map(|n| (n, false)).chain(declared.into_iter().map(|n| (n, true)));
    for (name, is_declared) in named {
        let found = package_path(&name).and_then(|at| {
            [script_dir.join(&at), crate::paths::config_dir().join(&at), crate::paths::data_dir().join(&at)]
                .into_iter()
                .find(|p| p.is_file())
                .map(|disk| (at, disk))
        });
        match found {
            Some((at, disk)) => {
                let is_script = disk.extension().is_some_and(|e| e.eq_ignore_ascii_case("lua"));
                if files.add_at(&at, &disk) && is_script && depth < MAX_SCRIPT_DEPTH {
                    pack_named_files(&disk, files, missing, depth + 1);
                }
            }
            // Only a string that looked like a file: nothing's missing
            None if !is_declared => {}
            None => missing.push(name),
        }
    }
}

/// Files a script names: its string literals that look like a relative path
/// to a file ("filters/swirl.lua"), and its `-- @asset <path>` lines, for a
/// file whose path the script builds instead of writing out.
fn named_files(source: &str) -> (Vec<String>, Vec<String>) {
    let literals = crate::lua_shield::string_literals(source)
        .into_iter()
        .filter(|s| looks_like_file(s))
        .collect();
    let declared = source
        .lines()
        .filter_map(|line| {
            let rest = line.trim().strip_prefix("--")?.trim().strip_prefix("@asset")?;
            rest.starts_with(char::is_whitespace).then(|| rest.trim().to_string())
        })
        .filter(|s| !s.is_empty())
        .collect();
    (literals, declared)
}

fn looks_like_file(s: &str) -> bool {
    let extension_ok = Path::new(s)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| (1..=5).contains(&e.len()) && e.chars().all(|c| c.is_ascii_alphanumeric()));
    s.len() < 260 && !s.contains(['\n', '\r']) && !s.contains("://") && extension_ok && package_path(s).is_some()
}

/// `rel` as a path inside a package ("a/b.png"), or None for a path that
/// isn't relative or leaves ("C:\\x", "/x", "../x").
fn package_path(rel: &str) -> Option<String> {
    let rel = rel.trim().replace('\\', "/");
    if rel.starts_with('/') || rel.contains(':') {
        return None;
    }
    let parts: Vec<&str> = rel.split('/').filter(|p| !p.is_empty() && *p != ".").collect();
    if parts.is_empty() || parts.contains(&"..") {
        return None;
    }
    Some(parts.join("/"))
}

fn on_disk(folder: &Path, at: &str) -> PathBuf {
    at.split('/').fold(folder.to_path_buf(), |path, part| path.join(part))
}

fn find_script(raw: &str) -> Option<PathBuf> {
    let found = PathBuf::from(crate::studio_app::resolve_script_path(raw));
    found.is_file().then_some(found)
}

fn find_asset(raw: &str) -> Option<PathBuf> {
    crate::paths::find_file(raw)
}

/// The non-empty text at `keys` in the settings.
fn text(settings: &serde_json::Value, keys: &[&str]) -> Option<String> {
    let mut value = settings;
    for key in keys {
        value = value.get(key)?;
    }
    value.as_str().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string)
}

fn set_text(settings: &mut serde_json::Value, keys: &[&str], text: String) {
    let mut value = settings;
    for key in keys {
        match value.get_mut(key) {
            Some(v) => value = v,
            None => return,
        }
    }
    *value = serde_json::Value::String(text);
}

/// `name` made a file name: characters Windows refuses become _
pub fn file_stem_from(name: &str) -> String {
    let stem: String = name
        .chars()
        .map(|c| if c.is_control() || r#"<>:"/\|?*"#.contains(c) { '_' } else { c })
        .collect();
    let stem = stem.trim().trim_end_matches('.').trim();
    if stem.is_empty() { "imported_redeem".to_string() } else { stem.to_string() }
}

/// `stem`, or "stem (2)", "stem (3)"…: the first no redeem (or redeem
/// folder) in `redeems` has.
fn unique_stem(redeems: &Path, stem: &str) -> String {
    let taken = |s: &str| {
        redeems.join(s).exists()
            || ["json", "rdm"].iter().any(|ext| redeems.join(format!("{}.{}", s, ext)).exists())
    };
    let mut candidate = stem.to_string();
    let mut n = 2;
    while taken(&candidate) {
        candidate = format!("{} ({})", stem, n);
        n += 1;
    }
    candidate
}

fn write_zip(path: &Path, settings: &serde_json::Value, files: &Files) -> Result<(), String> {
    let file = std::fs::File::create(path).map_err(|e| format!("Couldn't create {}: {}", path.display(), e))?;
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let json = serde_json::to_vec_pretty(settings).map_err(|e| e.to_string())?;
    zip.start_file(SETTINGS, options).map_err(|e| e.to_string())?;
    zip.write_all(&json).map_err(|e| e.to_string())?;
    for (at, disk) in &files.entries {
        let bytes = std::fs::read(disk).map_err(|e| format!("Couldn't read {}: {}", disk.display(), e))?;
        zip.start_file(at.as_str(), options).map_err(|e| e.to_string())?;
        zip.write_all(&bytes).map_err(|e| e.to_string())?;
    }
    zip.finish().map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("trust_red_{}_{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(path: &Path, bytes: &[u8]) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    const PNG_1X1: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0x0D, 0x49, 0x48, 0x44, 0x52, 0, 0, 0, 1, 0, 0, 0, 1,
        8, 6, 0, 0, 0, 0x1F, 0x15, 0xC4, 0x89,
    ];

    #[test]
    fn package_paths_stay_inside() {
        assert_eq!(package_path("filters/swirl.lua").as_deref(), Some("filters/swirl.lua"));
        assert_eq!(package_path(".\\sounds\\boom.mp3").as_deref(), Some("sounds/boom.mp3"));
        for outside in ["../x.lua", "a/../../x.lua", "/etc/passwd", "C:\\Windows\\x.dll", "c:x.lua", "", "./"] {
            assert_eq!(package_path(outside), None, "{}", outside);
        }
    }

    #[test]
    fn export_packs_what_the_redeem_needs_and_import_unpacks_it() {
        let root = temp_dir("roundtrip");
        let src = root.join("src");
        write(
            &src.join("main.lua"),
            br#"-- @asset img/extra.png
function on_redeem(params)
    trust.trigger_reaction({ impactReactions = { custom_script = { enabled = true, path = "fx/glow.lua" } } })
    trust.play_sound("sounds/boom.mp3")
    trust.play_sound("not/here.mp3")
    os.execute("calc")
end
"#,
        );
        write(&src.join("fx/glow.lua"), b"shader_code = [[ ]]");
        write(&src.join("sounds/boom.mp3"), b"ID3");
        write(&src.join("img/extra.png"), PNG_1X1);
        write(&src.join("pics/pic.png"), PNG_1X1);

        let mut config = RedeemConfig::default();
        config.name = "Glow".into();
        config.redeem_type = "custom".into();
        config.custom_redeem_script = Some(src.join("main.lua").to_string_lossy().into_owned());
        config.image_file_path = Some(src.join("pics/pic.png").to_string_lossy().into_owned());
        config.image_src = Some("data:image/png;base64,AAAA".into());
        config.start_sound_path = Some(root.join("gone.ogg").to_string_lossy().into_owned());
        config.impact_reactions = Some(serde_json::json!({
            "custom_script": { "enabled": true, "path": src.join("fx/glow.lua").to_string_lossy() },
            "bounce": { "enabled": false }
        }));

        let out = root.join("Glow.red");
        let report = export(&config, &out).unwrap();
        assert_eq!(report.missing, [root.join("gone.ogg").to_string_lossy().into_owned()]);
        assert!(!root.join("Glow.red.part").exists());

        let package = open(&out).unwrap();
        let mut paths: Vec<&str> = package.files.iter().map(|(p, _)| p.as_str()).collect();
        paths.sort();
        assert_eq!(paths, ["fx/glow.lua", "img/extra.png", "main.lua", "pic.png", "sounds/boom.mp3"]);
        assert_eq!(report.files, 5);
        assert_eq!(package.settings["customRedeemScript"], "main.lua");
        assert_eq!(package.settings["imageFilePath"], "pic.png");
        assert!(package.settings.get("imageSrc").is_none(), "the image travels as a file");
        // The reaction script is the file the script names: packed once
        assert_eq!(package.settings["impactReactions"]["custom_script"]["path"], "fx/glow.lua");
        let threats: Vec<(&str, &str)> = package.threats.iter().map(|(f, t)| (f.as_str(), t.call.as_str())).collect();
        assert_eq!(threats, [("main.lua", "os.execute")]);

        let redeems = root.join("redeems");
        let item = install_into(&package, "Glow", &redeems).unwrap();
        assert_eq!(item.filename, "Glow.json");
        assert!(redeems.join("Glow.json").is_file());
        let folder = redeems.join("Glow");
        let script = folder.join("main.lua");
        assert_eq!(item.config.custom_redeem_script.as_deref(), Some(script.to_string_lossy().as_ref()));
        assert!(folder.join("fx").join("glow.lua").is_file());
        assert!(folder.join("sounds").join("boom.mp3").is_file());
        let reaction = &item.config.impact_reactions.as_ref().unwrap()["custom_script"]["path"];
        assert!(Path::new(reaction.as_str().unwrap()).is_file(), "{}", reaction);
        assert!(item.config.image_src.as_deref().unwrap_or("").starts_with("data:image/png;base64,"));

        // A second import doesn't replace the first
        let again = install_into(&package, "Glow", &redeems).unwrap();
        assert_eq!(again.filename, "Glow (2).json");
        assert!(redeems.join("Glow (2)").join("main.lua").is_file());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_package_reaching_outside_its_folder_is_refused() {
        let root = temp_dir("zipslip");
        let evil = root.join("evil.red");
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&evil).unwrap());
        let options = zip::write::SimpleFileOptions::default();
        zip.start_file(SETTINGS, options).unwrap();
        zip.write_all(br#"{"name":"Evil","type":"custom"}"#).unwrap();
        zip.start_file("../../outside.lua", options).unwrap();
        zip.write_all(b"-- boo").unwrap();
        zip.finish().unwrap();

        let err = open(&evil).unwrap_err();
        assert!(err.contains("outside its folder"), "{}", err);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn not_a_redeem_is_refused() {
        let root = temp_dir("notredeem");
        let file = root.join("x.red");
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&file).unwrap());
        zip.start_file("readme.txt", zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(b"hi").unwrap();
        zip.finish().unwrap();
        assert!(open(&file).unwrap_err().contains("no redeem.json"));
        std::fs::write(&file, b"not a zip").unwrap();
        assert!(open(&file).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }
}
