//! Drawing for Rhai sources (script_filters, `const KIND = "source"`): the
//! list a source's script returns each frame — rounded boxes, text and
//! pictures — drawn with OBS's graphics API, no browser involved.
//!
//! A command is a map; the first key that's there says what it is:
//!
//! ```text
//! #{ rect: [x, y, w, h], fill: 0xAARRGGBB, radius: 8.0, border: 1.0, border_color: 0xAARRGGBB }
//! #{ text: "hi", x: 10.0, y: 20.0, font: "Poppins", size: 16.0, weight: 700, color: 0xFFFFFFFF, shadow: true }
//! #{ image: "https://…" or a file, x: 0.0, y: 0.0, w: 28.0, h: 28.0, radius: 14.0 }
//! #{ group: [ …commands… ], x: 0.0, y: 40.0, scale: 0.9, origin: [100.0, 12.0], alpha: 0.5,
//!    clip: [x, y, w, h], blend: "add" }
//! #{ line: [x0, y0, x1, y1, x2, y2, …], width: 3.0, color: 0xAARRGGBB, soft: 6.0 }
//! ```
//!
//! A rect's `border` is one width, or [top, right, bottom, left] as in CSS;
//! its `soft` (pixels) makes its edge fade out over that much, outside the
//! box: a glow.
//! A group draws its commands moved by (x, y), scaled around `origin` (its
//! own coordinates) and faded together: a message animating as one, its text
//! scaled on the GPU rather than set again at every size. Its `clip` (in its
//! own coordinates) shows only what's inside that box: a message revealed
//! bit by bit. `blend: "add"` adds its light to what's under it (glows,
//! lightning) instead of covering it.
//! A line goes through its points, rounded at each end; `soft` fades its
//! edge out like a rect's. Lines aren't clipped.
//!
//! Each may also have `alpha` (0–1). Text is set with ab_glyph from the fonts
//! Theater ships (assets/fonts: Poppins, Inter, Outfit, Roboto Mono, Bebas
//! Neue, Montserrat) or installed ones; `measure_text` gives a script its
//! size first. Pictures load in the background — from a file, or the web
//! (Twitch, Kick, YouTube and the emote sites only) — and `image_size`
//! says when one is ready.

use std::collections::HashMap;
use std::ffi::{CStr, c_char, c_void};
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, OnceLock};

use ab_glyph::{Font, FontVec, PxScale, ScaleFont, VariableFont, point};
use parking_lot::Mutex;
use rhai::{Array, Dynamic, Map};

use crate::obs_source::trust_log;

type P = *mut c_void;

// ─── Fonts ──────────────────────────────────────────────────────────────────

/// One font file Theater can set text with.
#[derive(Debug, Clone)]
struct FontFile {
    path: PathBuf,
    /// Its index in a collection (.ttc)
    index: u32,
    /// Lowercase family name ("poppins", "segoe ui")
    family: String,
    /// The family as it's written, for font lists
    display: String,
    weight: u16,
    italic: bool,
    /// Has a weight axis: any weight from the one file
    variable: bool,
}

#[derive(Default)]
struct FontBook {
    files: Vec<FontFile>,
    /// The installed fonts were looked through
    system_scanned: bool,
    /// Loaded fonts, by (file, weight for a variable one)
    loaded: HashMap<(usize, u16), Arc<FontVec>>,
}

static BOOK: LazyLock<Mutex<FontBook>> = LazyLock::new(|| {
    let mut book = FontBook::default();
    for dir in bundled_font_dirs() {
        scan_fonts(&dir, 0, &mut book.files);
    }
    Mutex::new(book)
});

/// Where Theater's own fonts are: its repo, or OBS's plugin data folder.
fn bundled_font_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![crate::get_theater_repo_dir().join("Trust").join("assets").join("fonts")];
    for base in ["APPDATA", "PROGRAMDATA"].iter().filter_map(|v| std::env::var(v).ok()) {
        dirs.push(PathBuf::from(base).join("obs-studio").join("plugins").join("theater-scene").join("data").join("assets").join("fonts"));
    }
    if let Ok(home) = std::env::var("HOME") {
        dirs.push(PathBuf::from(home).join(".config/obs-studio/plugins/theater-scene/data/assets/fonts"));
    }
    dirs.retain(|d| d.is_dir());
    dirs.dedup();
    dirs
}

/// Where the installed fonts are.
fn system_font_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(windir) = std::env::var("WINDIR") {
        dirs.push(PathBuf::from(windir).join("Fonts"));
    }
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        dirs.push(PathBuf::from(local).join("Microsoft").join("Windows").join("Fonts"));
    }
    for d in ["/usr/share/fonts", "/usr/local/share/fonts"] {
        dirs.push(PathBuf::from(d));
    }
    if let Ok(home) = std::env::var("HOME") {
        dirs.push(PathBuf::from(&home).join(".local/share/fonts"));
        dirs.push(PathBuf::from(&home).join(".fonts"));
    }
    dirs.retain(|d| d.is_dir());
    dirs
}

fn scan_fonts(dir: &Path, depth: u32, out: &mut Vec<FontFile>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if depth < 3 {
                scan_fonts(&path, depth + 1, out);
            }
            continue;
        }
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
        if matches!(ext.as_str(), "ttf" | "otf" | "ttc") {
            out.extend(describe_font(&path));
        }
    }
}

/// A font file's families, weights and styles — from its table directory,
/// `name`, `OS/2` and `fvar` tables only, not the whole file (the installed
/// fonts can be hundreds of MB).
fn describe_font(path: &Path) -> Vec<FontFile> {
    use std::io::{Read, Seek, SeekFrom};
    let Ok(mut file) = std::fs::File::open(path) else { return Vec::new() };
    let mut read_at = |offset: u64, len: usize| -> Option<Vec<u8>> {
        if len > 4 << 20 {
            return None;
        }
        let mut buf = vec![0u8; len];
        file.seek(SeekFrom::Start(offset)).ok()?;
        file.read_exact(&mut buf).ok()?;
        Some(buf)
    };
    let u16_at = |b: &[u8], i: usize| -> Option<u16> { Some(u16::from_be_bytes([*b.get(i)?, *b.get(i + 1)?])) };
    let u32_at = |b: &[u8], i: usize| -> Option<u32> { Some(u32::from_be_bytes([*b.get(i)?, *b.get(i + 1)?, *b.get(i + 2)?, *b.get(i + 3)?])) };

    let Some(head) = read_at(0, 12) else { return Vec::new() };
    let offsets: Vec<u64> = if &head[0..4] == b"ttcf" {
        let count = u32_at(&head, 8).unwrap_or(0).min(64) as usize;
        let Some(dir) = read_at(12, count * 4) else { return Vec::new() };
        (0..count).filter_map(|i| u32_at(&dir, i * 4).map(|o| o as u64)).collect()
    } else {
        vec![0]
    };

    let mut fonts = Vec::new();
    for (index, base) in offsets.into_iter().enumerate() {
        let Some(offset_table) = read_at(base, 12) else { continue };
        let tables = u16_at(&offset_table, 4).unwrap_or(0) as usize;
        let Some(records) = read_at(base + 12, tables * 16) else { continue };
        let find = |tag: &[u8; 4]| -> Option<(u64, usize)> {
            (0..tables).find_map(|t| {
                let r = &records[t * 16..t * 16 + 16];
                (&r[0..4] == tag).then(|| (u32_at(r, 8).unwrap_or(0) as u64, u32_at(r, 12).unwrap_or(0) as usize))
            })
        };
        let (name_at, os2_at, fvar) = (find(b"name"), find(b"OS/2"), find(b"fvar"));
        let Some((name_off, name_len)) = name_at else { continue };
        let Some(name) = read_at(name_off, name_len) else { continue };
        let family = font_family_name(&name, &u16_at);
        let Some(family) = family.filter(|f| !f.is_empty()) else { continue };
        let (weight, italic) = os2_at
            .and_then(|(o, l)| read_at(o, l.min(64)))
            .map(|os2| (u16_at(&os2, 4).unwrap_or(400), u16_at(&os2, 62).is_some_and(|sel| sel & 1 != 0)))
            .unwrap_or((400, false));
        fonts.push(FontFile {
            path: path.to_path_buf(),
            index: index as u32,
            family: family.to_lowercase(),
            display: family,
            weight,
            italic,
            variable: fvar.is_some(),
        });
    }
    fonts
}

/// The family in a `name` table: its typographic family (16), else its
/// family (1), in Windows Unicode (3, 1) or Unicode (0) encoding.
fn font_family_name(name: &[u8], u16_at: &dyn Fn(&[u8], usize) -> Option<u16>) -> Option<String> {
    let count = u16_at(name, 2)? as usize;
    let strings = u16_at(name, 4)? as usize;
    let mut best: Option<(u16, String)> = None;
    for r in 0..count {
        let at = 6 + r * 12;
        let (platform, name_id) = (u16_at(name, at)?, u16_at(name, at + 6)?);
        if !matches!(name_id, 1 | 16) || !matches!(platform, 0 | 3) {
            continue;
        }
        let (len, offset) = (u16_at(name, at + 8)? as usize, u16_at(name, at + 10)? as usize);
        let bytes = name.get(strings + offset..strings + offset + len)?;
        let units: Vec<u16> = bytes.chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
        let text = String::from_utf16_lossy(&units);
        // Typographic family first: "Segoe UI" rather than "Segoe UI Semibold"
        let rank = if name_id == 16 { 0 } else { 1 };
        if best.as_ref().is_none_or(|(r, _)| rank < *r) {
            best = Some((rank, text));
        }
    }
    best.map(|(_, t)| t)
}

/// Every family Theater can set text with, its own first, for a font list.
/// Looks through the installed fonts first (OBS's UI thread: properties).
pub fn font_families() -> Vec<String> {
    scan_system_fonts();
    let book = BOOK.lock();
    let bundled = bundled_font_dirs();
    let is_own = |f: &FontFile| bundled.iter().any(|d| f.path.starts_with(d));
    let mut families: Vec<(bool, String)> = Vec::new();
    for f in book.files.iter().filter(|f| !f.italic) {
        if !families.iter().any(|(_, name)| name.eq_ignore_ascii_case(&f.display)) {
            families.push((!is_own(f), f.display.clone()));
        }
    }
    families.sort_by(|a, b| (a.0, a.1.to_lowercase()).cmp(&(b.0, b.1.to_lowercase())));
    families.into_iter().map(|(_, name)| name).collect()
}

/// Looks through the installed fonts, once (they can take a moment).
fn scan_system_fonts() {
    if BOOK.lock().system_scanned {
        return;
    }
    let mut found = Vec::new();
    for dir in system_font_dirs() {
        scan_fonts(&dir, 0, &mut found);
    }
    let mut book = BOOK.lock();
    if !book.system_scanned {
        book.files.extend(found);
        book.system_scanned = true;
    }
}

/// scan_system_fonts in the background: text asked for meanwhile falls back
/// to Theater's own fonts rather than holding up OBS's render thread.
fn scan_system_fonts_soon() {
    static STARTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if !STARTED.swap(true, std::sync::atomic::Ordering::SeqCst) {
        let _ = std::thread::Builder::new().name("theater-fonts".into()).spawn(scan_system_fonts);
    }
}

/// Families text falls back to, in turn, for a character its font lacks.
const FALLBACK_FAMILIES: [&str; 8] = [
    "inter",
    "segoe ui",
    "arial",
    "dejavu sans",
    "noto sans",
    "segoe ui symbol",
    "segoe ui emoji",
    "noto emoji",
];

impl FontBook {
    /// The best file of `family` for `weight`: a variable one, or the
    /// closest static weight (upright before italic).
    fn pick(&self, family: &str, weight: u16) -> Option<usize> {
        let family = family.trim().trim_matches(|c| c == '\'' || c == '"').to_lowercase();
        self.files
            .iter()
            .enumerate()
            .filter(|(_, f)| f.family == family)
            .min_by_key(|(_, f)| {
                let distance = if f.variable { 0 } else { (f.weight as i32 - weight as i32).unsigned_abs() };
                (f.italic as u32, distance)
            })
            .map(|(i, _)| i)
    }

    fn load(&mut self, file: usize, weight: u16) -> Option<Arc<FontVec>> {
        let key_weight = if self.files[file].variable { weight } else { 0 };
        if let Some(font) = self.loaded.get(&(file, key_weight)) {
            return Some(font.clone());
        }
        let info = &self.files[file];
        let data = std::fs::read(&info.path).ok()?;
        let mut font = FontVec::try_from_vec_and_index(data, info.index).ok()?;
        if info.variable {
            font.set_variation(b"wght", weight as f32);
        }
        let font = Arc::new(font);
        self.loaded.insert((file, key_weight), font.clone());
        Some(font)
    }
}

/// The fonts a run of text is set with: its own, then the fallbacks.
fn font_chain(family: &str, weight: u16) -> Vec<Arc<FontVec>> {
    let mut book = BOOK.lock();
    // A family that isn't Theater's own may be installed (and the fallbacks
    // for symbols and emoji are): looked for in the background
    if !book.system_scanned {
        scan_system_fonts_soon();
    }
    let mut chain = Vec::new();
    for fam in std::iter::once(family).chain(FALLBACK_FAMILIES) {
        if let Some(file) = book.pick(fam, weight) {
            if let Some(font) = book.load(file, weight) {
                if !chain.iter().any(|f| Arc::ptr_eq(f, &font)) {
                    chain.push(font);
                }
            }
        }
    }
    chain
}

/// A run of text laid out on one line: each glyph's font, id and pen position.
struct Line {
    glyphs: Vec<(usize, ab_glyph::GlyphId, f32)>,
    width: f32,
    ascent: f32,
    descent: f32,
}

fn layout(text: &str, chain: &[Arc<FontVec>], size: f32) -> Line {
    let scale = PxScale::from(size);
    let (ascent, descent) = chain.first().map(|f| {
        let s = f.as_scaled(scale);
        (s.ascent(), s.descent())
    }).unwrap_or((size * 0.8, -size * 0.2));
    let mut glyphs = Vec::new();
    let mut pen = 0.0f32;
    let mut previous: Option<(usize, ab_glyph::GlyphId)> = None;
    for c in text.chars() {
        if c == '\n' || c == '\r' {
            continue;
        }
        let pick = chain
            .iter()
            .enumerate()
            .find(|(_, f)| f.glyph_id(c).0 != 0 || c.is_whitespace())
            .or_else(|| chain.first().map(|f| (0, f)));
        let Some((fi, font)) = pick else { continue };
        let scaled = font.as_scaled(scale);
        let id = font.glyph_id(c);
        if let Some((pf, pid)) = previous {
            if pf == fi {
                pen += scaled.kern(pid, id);
            }
        }
        glyphs.push((fi, id, pen));
        pen += scaled.h_advance(id);
        previous = Some((fi, id));
    }
    Line { glyphs, width: pen, ascent, descent }
}

/// Text's size set in `font` at `size` px and `weight`: (width, height,
/// ascent), the height being the font's ascent to descent.
pub fn measure(text: &str, font: &str, size: f32, weight: u16) -> (f32, f32, f32) {
    let chain = font_chain(font, weight);
    let line = layout(text, &chain, size.max(1.0));
    (line.width, line.ascent - line.descent, line.ascent)
}

/// What a text command draws, and so its picture's cache key.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TextKey {
    pub text: String,
    pub font: String,
    /// Size in 1/64 px
    pub size: u32,
    pub weight: u16,
    pub color: u32,
    pub shadow: bool,
}

/// A picture, premultiplied RGBA.
pub struct Picture {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    /// Space around the text for its shadow: it's drawn this much up and left
    pub pad: u32,
}

/// Room around text for its shadow (1 px down and right, 2 px blur).
const SHADOW_PAD: u32 = 3;

/// Sets `key`'s text: its glyphs' coverage coloured, over a soft shadow
/// (CSS text-shadow: 1px 1px 2px rgba(0,0,0,0.8)) when asked.
pub fn rasterize(key: &TextKey) -> Picture {
    let size = key.size as f32 / 64.0;
    let chain = font_chain(&key.font, key.weight);
    let line = layout(&key.text, &chain, size.max(1.0));
    let pad = if key.shadow { SHADOW_PAD } else { 1 };
    let w = (line.width.ceil() as u32 + pad * 2).max(1);
    let h = ((line.ascent - line.descent).ceil() as u32 + pad * 2).max(1);
    let mut coverage = vec![0f32; (w * h) as usize];
    let scale = PxScale::from(size.max(1.0));
    for (fi, id, x) in &line.glyphs {
        let glyph = id.with_scale_and_position(scale, point(pad as f32 + x, pad as f32 + line.ascent));
        if let Some(outlined) = chain[*fi].outline_glyph(glyph) {
            let bounds = outlined.px_bounds();
            outlined.draw(|gx, gy, c| {
                let px = bounds.min.x as i32 + gx as i32;
                let py = bounds.min.y as i32 + gy as i32;
                if px >= 0 && py >= 0 && (px as u32) < w && (py as u32) < h {
                    let i = (py as u32 * w + px as u32) as usize;
                    coverage[i] = (coverage[i] + c).min(1.0);
                }
            });
        }
    }
    let shadow = if key.shadow { Some(soft_shadow(&coverage, w, h)) } else { None };
    let [ca, cr, cg, cb] = key.color.to_be_bytes().map(|v| v as f32 / 255.0);
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    for i in 0..(w * h) as usize {
        // The text over its shadow, premultiplied
        let ta = coverage[i] * ca;
        let sa = shadow.as_ref().map_or(0.0, |s| s[i]) * 0.8 * ca;
        let a = ta + sa * (1.0 - ta);
        let (r, g, b) = (cr * ta, cg * ta, cb * ta);
        rgba[i * 4] = (r * 255.0).round() as u8;
        rgba[i * 4 + 1] = (g * 255.0).round() as u8;
        rgba[i * 4 + 2] = (b * 255.0).round() as u8;
        rgba[i * 4 + 3] = (a * 255.0).round() as u8;
    }
    Picture { width: w, height: h, rgba, pad }
}

/// The coverage moved 1 px down and right and blurred (two 3×3 box blurs,
/// about a 2 px gaussian).
fn soft_shadow(coverage: &[f32], w: u32, h: u32) -> Vec<f32> {
    let (w, h) = (w as i32, h as i32);
    let at = |buf: &[f32], x: i32, y: i32| if x >= 0 && y >= 0 && x < w && y < h { buf[(y * w + x) as usize] } else { 0.0 };
    let mut moved = vec![0f32; (w * h) as usize];
    for y in 0..h {
        for x in 0..w {
            moved[(y * w + x) as usize] = at(coverage, x - 1, y - 1);
        }
    }
    let blur = |src: &[f32]| {
        let mut out = vec![0f32; (w * h) as usize];
        for y in 0..h {
            for x in 0..w {
                let mut sum = 0.0;
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        sum += at(src, x + dx, y + dy);
                    }
                }
                out[(y * w + x) as usize] = sum / 9.0;
            }
        }
        out
    };
    blur(&blur(&moved))
}

// ─── Pictures ───────────────────────────────────────────────────────────────

/// Where a picture is at.
#[derive(Clone)]
enum Loading {
    Pending,
    Ready(Arc<Picture>),
    Failed,
}

static IMAGES: LazyLock<Mutex<HashMap<String, Loading>>> = LazyLock::new(|| Mutex::new(HashMap::new()));
static FETCH: OnceLock<crossbeam_channel::Sender<(String, Option<PathBuf>)>> = OnceLock::new();

/// The web hosts pictures may come from: the chat platforms' image servers
/// and the emote sites. A script can't have Theater fetch anything else.
const IMAGE_HOSTS: [&str; 11] = [
    "static-cdn.jtvnw.net",
    "files.kick.com",
    "images.kick.com",
    "yt3.ggpht.com",
    "yt4.ggpht.com",
    "yt3.googleusercontent.com",
    "lh3.googleusercontent.com",
    // YouTube chat's emojis
    "fonts.gstatic.com",
    "cdn.7tv.app",
    "cdn.betterttv.net",
    "cdn.frankerfacez.com",
];

/// Whether Theater may fetch `url` for a script.
pub fn allowed_url(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else { return false };
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = host.rsplit('@').next().unwrap_or(host);
    !host.contains(':') && IMAGE_HOSTS.contains(&host.to_ascii_lowercase().as_str())
}

/// Where fetched pictures are kept: OBS's folder for plugins' own files
/// (obs-studio/plugin_config/theater-scene), else the temp folder.
pub fn cache_dir() -> PathBuf {
    let config = std::env::var("APPDATA")
        .map(|a| PathBuf::from(a).join("obs-studio"))
        .or_else(|_| std::env::var("HOME").map(|h| PathBuf::from(h).join(".config").join("obs-studio")));
    let dir = match config {
        Ok(obs) if obs.is_dir() => obs.join("plugin_config").join("theater-scene").join("image_cache"),
        _ => std::env::temp_dir().join("theater-scene-image-cache"),
    };
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// A picture's size, when it's loaded; asking starts loading it. `src`: an
/// https URL on IMAGE_HOSTS, or a file (relative to `base`, the script's
/// folder). None while it loads; 0 × 0 when it can't be.
pub fn image_size(src: &str, base: Option<&Path>) -> Option<(u32, u32)> {
    let key = src.trim().to_string();
    if key.is_empty() {
        return None;
    }
    {
        let images = IMAGES.lock();
        match images.get(&key) {
            Some(Loading::Ready(p)) => return Some((p.width, p.height)),
            // It won't come: 0 × 0, so nobody waits for it
            Some(Loading::Failed) => return Some((0, 0)),
            Some(Loading::Pending) => return None,
            None => {}
        }
    }
    request(key, base.map(Path::to_path_buf));
    None
}

/// The loaded picture `src`, if it is.
pub fn picture(src: &str) -> Option<Arc<Picture>> {
    match IMAGES.lock().get(src.trim()) {
        Some(Loading::Ready(p)) => Some(p.clone()),
        _ => None,
    }
}

fn request(key: String, base: Option<PathBuf>) {
    IMAGES.lock().insert(key.clone(), Loading::Pending);
    let sender = FETCH.get_or_init(|| {
        let (tx, rx) = crossbeam_channel::unbounded::<(String, Option<PathBuf>)>();
        // One loader, off OBS's threads
        let _ = std::thread::Builder::new().name("theater-script-images".into()).spawn(move || {
            for (key, base) in rx {
                let loaded = load_picture(&key, base.as_deref());
                if let Err(e) = &loaded {
                    trust_log(&format!("[Rhai] picture {}: {}", key, e));
                }
                IMAGES.lock().insert(key, loaded.map(|p| Loading::Ready(Arc::new(p))).unwrap_or(Loading::Failed));
            }
        });
        tx
    });
    let _ = sender.send((key, base));
}

fn load_picture(src: &str, base: Option<&Path>) -> Result<Picture, String> {
    let bytes = if src.starts_with("https://") || src.starts_with("http://") {
        if !allowed_url(src) {
            return Err("not an image server Theater fetches from".to_string());
        }
        // The tests don't go online: a picture stays loading there
        if cfg!(test) {
            return Err("no downloads in tests".to_string());
        }
        let file = cache_dir().join(format!("{:016x}", fnv(src)));
        if !file.is_file() {
            let mut curl = std::process::Command::new("curl");
            curl.args(["-s", "-f", "-L", "--max-time", "10", "--max-filesize", "8000000", "--proto", "=https", "-o"])
                .arg(&file)
                .arg(src);
            // OBS has no console, so Windows would open one for curl on
            // every download
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                const CREATE_NO_WINDOW: u32 = 0x0800_0000;
                curl.creation_flags(CREATE_NO_WINDOW);
            }
            let status = curl.status().map_err(|e| format!("curl: {}", e))?;
            if !status.success() {
                let _ = std::fs::remove_file(&file);
                return Err(format!("download failed ({})", status));
            }
        }
        std::fs::read(&file).map_err(|e| e.to_string())?
    } else {
        let path = Path::new(src);
        let path = if path.is_absolute() { path.to_path_buf() } else { base.unwrap_or(Path::new(".")).join(path) };
        std::fs::read(&path).map_err(|e| format!("{}: {}", path.display(), e))?
    };
    // The first frame of an animated one
    let img = image::load_from_memory(&bytes).map_err(|e| e.to_string())?.to_rgba8();
    let (width, height) = img.dimensions();
    let mut rgba = img.into_raw();
    for px in rgba.chunks_exact_mut(4) {
        let a = px[3] as u32;
        for c in &mut px[..3] {
            *c = ((*c as u32 * a + 127) / 255) as u8;
        }
    }
    Ok(Picture { width, height, rgba, pad: 0 })
}

/// FNV-1a: a stable file name for a URL.
fn fnv(s: &str) -> u64 {
    s.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ b as u64).wrapping_mul(0x0100_0000_01b3))
}

// ─── The display list ───────────────────────────────────────────────────────

/// One thing to draw.
#[derive(Debug, Clone, PartialEq)]
pub enum Cmd {
    /// `border`: top, right, bottom, left; `soft`: the edge's fade, outside
    Rect { x: f32, y: f32, w: f32, h: f32, fill: u32, radius: f32, border: [f32; 4], border_color: u32, soft: f32, alpha: f32 },
    Text { x: f32, y: f32, key: TextKey, alpha: f32 },
    Image { x: f32, y: f32, w: f32, h: f32, src: String, radius: f32, alpha: f32 },
    /// `clip`: x, y, w, h in its own coordinates; `add`: additive blending
    Group { x: f32, y: f32, scale: f32, origin: (f32, f32), alpha: f32, clip: Option<[f32; 4]>, add: bool, children: Vec<Cmd> },
    /// Through `points`, `width` wide, rounded ends
    Line { points: Vec<(f32, f32)>, width: f32, color: u32, soft: f32, alpha: f32 },
}

/// Groups inside groups, at most.
const MAX_DEPTH: u32 = 8;

fn numbers(d: &Dynamic) -> Vec<f32> {
    d.read_lock::<Array>()
        .map(|a| a.iter().filter_map(|v| v.as_float().ok().map(|f| f as f32).or_else(|| v.as_int().ok().map(|i| i as f32))).collect())
        .unwrap_or_default()
}

fn num(m: &Map, k: &str, default: f32) -> f32 {
    m.get(k).and_then(|v| v.as_float().ok().map(|f| f as f32).or_else(|| v.as_int().ok().map(|i| i as f32))).unwrap_or(default)
}

fn color(m: &Map, k: &str, default: u32) -> u32 {
    m.get(k).and_then(|v| v.as_int().ok()).map(|c| c as u32).unwrap_or(default)
}

/// A script's display list, as commands; what isn't one is left out.
pub fn parse(list: &Array) -> Vec<Cmd> {
    parse_at(list, 0)
}

fn parse_at(list: &Array, depth: u32) -> Vec<Cmd> {
    let mut cmds = Vec::with_capacity(list.len());
    for item in list {
        let Some(m) = item.read_lock::<Map>() else { continue };
        let alpha = num(&m, "alpha", 1.0).clamp(0.0, 1.0);
        if alpha <= 0.0 {
            continue;
        }
        if let Some(r) = m.get("rect") {
            let v = numbers(r);
            if v.len() == 4 && v[2] > 0.0 && v[3] > 0.0 {
                let border = match m.get("border") {
                    Some(b) if b.is_array() => {
                        let s = numbers(b);
                        if s.len() == 4 { [s[0].max(0.0), s[1].max(0.0), s[2].max(0.0), s[3].max(0.0)] } else { [0.0; 4] }
                    }
                    _ => [num(&m, "border", 0.0).max(0.0); 4],
                };
                cmds.push(Cmd::Rect {
                    x: v[0],
                    y: v[1],
                    w: v[2],
                    h: v[3],
                    fill: color(&m, "fill", 0),
                    radius: num(&m, "radius", 0.0).max(0.0),
                    border,
                    border_color: color(&m, "border_color", 0),
                    soft: num(&m, "soft", 0.0).clamp(0.0, 500.0),
                    alpha,
                });
            }
        } else if let Some(group) = m.get("group") {
            if depth >= MAX_DEPTH {
                continue;
            }
            let Some(list) = group.read_lock::<Array>() else { continue };
            let children = parse_at(&list, depth + 1);
            if !children.is_empty() {
                let o = m.get("origin").map(numbers).unwrap_or_default();
                // A clip with nothing in it hides the group
                let clip = m.get("clip").map(numbers).filter(|c| c.len() == 4).map(|c| [c[0], c[1], c[2].max(0.0), c[3].max(0.0)]);
                if clip.is_some_and(|c| c[2] <= 0.0 || c[3] <= 0.0) {
                    continue;
                }
                cmds.push(Cmd::Group {
                    x: num(&m, "x", 0.0),
                    y: num(&m, "y", 0.0),
                    scale: num(&m, "scale", 1.0).max(0.0),
                    origin: (o.first().copied().unwrap_or(0.0), o.get(1).copied().unwrap_or(0.0)),
                    alpha,
                    clip,
                    add: m.get("blend").is_some_and(|b| b.to_string() == "add"),
                    children,
                });
            }
        } else if let Some(text) = m.get("text").map(|t| t.to_string()) {
            if text.trim().is_empty() {
                continue;
            }
            cmds.push(Cmd::Text {
                x: num(&m, "x", 0.0),
                y: num(&m, "y", 0.0),
                key: TextKey {
                    text,
                    font: m.get("font").map(|f| f.to_string()).unwrap_or_else(|| "Inter".to_string()),
                    size: (num(&m, "size", 16.0).clamp(1.0, 400.0) * 64.0) as u32,
                    weight: num(&m, "weight", 400.0).clamp(100.0, 1000.0) as u16,
                    color: color(&m, "color", 0xFFFF_FFFF),
                    shadow: m.get("shadow").and_then(|s| s.as_bool().ok()).unwrap_or(false),
                },
                alpha,
            });
        } else if let Some(src) = m.get("image").map(|s| s.to_string()) {
            let (w, h) = (num(&m, "w", 0.0), num(&m, "h", 0.0));
            if w > 0.0 && h > 0.0 {
                cmds.push(Cmd::Image { x: num(&m, "x", 0.0), y: num(&m, "y", 0.0), w, h, src, radius: num(&m, "radius", 0.0).max(0.0), alpha });
            }
        } else if let Some(line) = m.get("line") {
            let v = numbers(line);
            let points: Vec<(f32, f32)> = v.chunks_exact(2).map(|p| (p[0], p[1])).collect();
            let width = num(&m, "width", 2.0);
            if points.len() >= 2 && width > 0.0 {
                cmds.push(Cmd::Line {
                    points,
                    width: width.min(500.0),
                    color: color(&m, "color", 0xFFFF_FFFF),
                    soft: num(&m, "soft", 0.0).clamp(0.0, 500.0),
                    alpha,
                });
            }
        }
    }
    cmds
}

// ─── Drawing it (OBS's graphics thread) ─────────────────────────────────────

/// One effect for all of it: rounded boxes with a border, and pictures
/// (premultiplied) with rounded corners.
const EFFECT: &str = r#"
uniform float4x4 ViewProj;
uniform texture2d image;
uniform float2 size;
uniform float radius;
uniform float4 border;
uniform float4 fill;
uniform float4 border_color;
uniform float opacity;
// A rect's edge fading out over this many pixels, outside the box
uniform float soft;
// The part of the whole quad drawn (a clipped one): offset and size, in uv
uniform float4 uv_rect;

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

// Distance from p to a box of `box` (at 0,0) with corners of radius r
float rounded_box(float2 p, float2 box, float r)
{
    r = min(r, min(box.x, box.y) * 0.5);
    float2 half_size = box * 0.5;
    float2 q = abs(p - half_size) - (half_size - r);
    return length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - r;
}

float4 PSRect(VertInOut vert_in) : TARGET
{
    // Where this pixel is in the whole quad, then in the box (the quad is
    // the box grown by `soft` on each side)
    float2 uv = uv_rect.xy + vert_in.uv * uv_rect.zw;
    float2 p = uv * (size + 2.0 * soft) - soft;
    float d = rounded_box(p, size, radius);
    float fade = saturate(1.0 - max(d, 0.0) / max(soft, 0.0001));
    float inside = soft > 0.0 ? fade * fade : saturate(0.5 - d);
    // The border, as CSS draws it: between the box and the box inset by
    // top, right, bottom, left (border.x, .y, .z, .w)
    float widest = max(max(border.x, border.y), max(border.z, border.w));
    float2 inner_size = size - float2(border.y + border.w, border.x + border.z);
    float d_in = (inner_size.x > 0.0 && inner_size.y > 0.0)
        ? rounded_box(p - float2(border.w, border.x), inner_size, max(radius - widest, 0.0))
        : 1.0;
    float ring = widest > 0.0 ? saturate(d_in + 0.5) : 0.0;
    float4 c = lerp(fill, border_color, ring);
    float a = c.a * inside * opacity;
    return float4(c.rgb * a, a);
}

float4 PSImage(VertInOut vert_in) : TARGET
{
    float2 uv = uv_rect.xy + vert_in.uv * uv_rect.zw;
    float d = rounded_box(uv * size, size, radius);
    float4 c = image.Sample(def_sampler, uv);
    return c * (saturate(0.5 - d) * opacity);
}

technique Rect
{
    pass
    {
        vertex_shader = VSDefault(vert_in);
        pixel_shader  = PSRect(vert_in);
    }
}

technique Image
{
    pass
    {
        vertex_shader = VSDefault(vert_in);
        pixel_shader  = PSImage(vert_in);
    }
}
"#;

/// The libobs graphics calls drawing needs.
struct DrawFns {
    effect_create: unsafe extern "C" fn(*const c_char, *const c_char, *mut *mut c_char) -> P,
    effect_get_param_by_name: unsafe extern "C" fn(P, *const c_char) -> P,
    effect_set_val: unsafe extern "C" fn(P, *const c_void, usize),
    effect_set_texture: unsafe extern "C" fn(P, P),
    effect_loop: unsafe extern "C" fn(P, *const c_char) -> bool,
    get_base_effect: unsafe extern "C" fn(u32) -> P,
    texture_create: unsafe extern "C" fn(u32, u32, u32, u32, *const *const u8, u32) -> P,
    texture_destroy: unsafe extern "C" fn(P),
    texrender_create: unsafe extern "C" fn(u32, u32) -> P,
    texrender_reset: unsafe extern "C" fn(P),
    texrender_begin: unsafe extern "C" fn(P, u32, u32) -> bool,
    texrender_end: unsafe extern "C" fn(P),
    texrender_get_texture: unsafe extern "C" fn(P) -> P,
    clear: unsafe extern "C" fn(u32, *const f32, f32, u8),
    ortho: unsafe extern "C" fn(f32, f32, f32, f32, f32, f32),
    projection_push: unsafe extern "C" fn(),
    projection_pop: unsafe extern "C" fn(),
    matrix_push: unsafe extern "C" fn(),
    matrix_pop: unsafe extern "C" fn(),
    matrix_identity: unsafe extern "C" fn(),
    matrix_translate3f: unsafe extern "C" fn(f32, f32, f32),
    matrix_scale3f: unsafe extern "C" fn(f32, f32, f32),
    matrix_rotaa4f: unsafe extern "C" fn(f32, f32, f32, f32),
    blend_state_push: unsafe extern "C" fn(),
    blend_state_pop: unsafe extern "C" fn(),
    blend_function_separate: unsafe extern "C" fn(u32, u32, u32, u32),
    enable_blending: unsafe extern "C" fn(bool),
    draw_sprite: unsafe extern "C" fn(P, u32, u32, u32),
}

unsafe impl Send for DrawFns {}
unsafe impl Sync for DrawFns {}

fn draw_fns() -> Option<&'static DrawFns> {
    static FNS: OnceLock<Option<DrawFns>> = OnceLock::new();
    FNS.get_or_init(|| {
        use crate::script_filters::sym;
        Some(DrawFns {
            effect_create: sym(c"gs_effect_create")?,
            effect_get_param_by_name: sym(c"gs_effect_get_param_by_name")?,
            effect_set_val: sym(c"gs_effect_set_val")?,
            effect_set_texture: sym(c"gs_effect_set_texture")?,
            effect_loop: sym(c"gs_effect_loop")?,
            get_base_effect: sym(c"obs_get_base_effect")?,
            texture_create: sym(c"gs_texture_create")?,
            texture_destroy: sym(c"gs_texture_destroy")?,
            texrender_create: sym(c"gs_texrender_create")?,
            texrender_reset: sym(c"gs_texrender_reset")?,
            texrender_begin: sym(c"gs_texrender_begin")?,
            texrender_end: sym(c"gs_texrender_end")?,
            texrender_get_texture: sym(c"gs_texrender_get_texture")?,
            clear: sym(c"gs_clear")?,
            ortho: sym(c"gs_ortho")?,
            projection_push: sym(c"gs_projection_push")?,
            projection_pop: sym(c"gs_projection_pop")?,
            matrix_push: sym(c"gs_matrix_push")?,
            matrix_pop: sym(c"gs_matrix_pop")?,
            matrix_identity: sym(c"gs_matrix_identity")?,
            matrix_translate3f: sym(c"gs_matrix_translate3f")?,
            matrix_scale3f: sym(c"gs_matrix_scale3f")?,
            matrix_rotaa4f: sym(c"gs_matrix_rotaa4f")?,
            blend_state_push: sym(c"gs_blend_state_push")?,
            blend_state_pop: sym(c"gs_blend_state_pop")?,
            blend_function_separate: sym(c"gs_blend_function_separate")?,
            enable_blending: sym(c"gs_enable_blending")?,
            draw_sprite: sym(c"gs_draw_sprite")?,
        })
    })
    .as_ref()
}

const GS_RGBA: u32 = 3;
const GS_BLEND_ONE: u32 = 1;
const GS_BLEND_INVSRCALPHA: u32 = 5;
const GS_CLEAR_COLOR: u32 = 1;

/// A texture on the GPU and when it was last drawn (frames).
struct Gpu {
    texture: usize,
    width: u32,
    height: u32,
    pad: u32,
    used: u64,
}

/// The graphics thread's own: the effect, and textures of text and pictures
/// (dropped after going unused a while).
#[derive(Default)]
struct GpuCache {
    effect: usize,
    effect_failed: bool,
    text: HashMap<TextKey, Gpu>,
    images: HashMap<String, Gpu>,
    frame: u64,
}

static GPU: LazyLock<Mutex<GpuCache>> = LazyLock::new(|| Mutex::new(GpuCache::default()));

/// Frames a texture may go unused before it's freed (a minute at 60 fps).
const KEEP_FRAMES: u64 = 3600;

unsafe fn texture_of(fns: &DrawFns, pic: &Picture) -> Gpu {
    let data = [pic.rgba.as_ptr()];
    let texture = unsafe { (fns.texture_create)(pic.width, pic.height, GS_RGBA, 1, data.as_ptr(), 0) };
    Gpu { texture: texture as usize, width: pic.width, height: pic.height, pad: pic.pad, used: 0 }
}

unsafe fn set_param(fns: &DrawFns, effect: P, name: &CStr, bytes: &[u8]) {
    let p = unsafe { (fns.effect_get_param_by_name)(effect, name.as_ptr()) };
    if !p.is_null() {
        unsafe { (fns.effect_set_val)(p, bytes.as_ptr() as *const c_void, bytes.len()) };
    }
}

fn floats(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|f| f.to_ne_bytes()).collect()
}

fn rgba_f(c: u32) -> [f32; 4] {
    let [a, r, g, b] = c.to_be_bytes().map(|v| v as f32 / 255.0);
    [r, g, b, a]
}

/// A texrender per source, clipping what it draws to its size.
pub struct Canvas {
    texrender: usize,
}

impl Canvas {
    pub const fn new() -> Self {
        Canvas { texrender: 0 }
    }

    /// Nothing made on the GPU yet (nothing to free).
    pub fn is_empty(&self) -> bool {
        self.texrender == 0
    }

    /// Frees it (in the graphics context).
    pub unsafe fn free(&mut self) {
        if self.texrender != 0 {
            if let Some(destroy) = crate::script_filters::sym::<unsafe extern "C" fn(P)>(c"gs_texrender_destroy") {
                unsafe { destroy(self.texrender as P) };
            }
            self.texrender = 0;
        }
    }
}

/// Draws `cmds` into a `size` canvas, then the canvas where the source is
/// drawn. On OBS's graphics thread (video_render).
pub unsafe fn render(canvas: &mut Canvas, cmds: &[Cmd], size: (u32, u32)) {
    let Some(fns) = draw_fns() else { return };
    let (w, h) = (size.0.max(1), size.1.max(1));
    let mut gpu = GPU.lock();
    gpu.frame += 1;
    if gpu.effect == 0 && !gpu.effect_failed {
        let code = std::ffi::CString::new(EFFECT).unwrap_or_default();
        let mut error: *mut c_char = std::ptr::null_mut();
        let effect = unsafe { (fns.effect_create)(code.as_ptr(), c"theater_script_draw".as_ptr(), &mut error) };
        if effect.is_null() {
            gpu.effect_failed = true;
            let why = if error.is_null() { String::new() } else { unsafe { CStr::from_ptr(error) }.to_string_lossy().into_owned() };
            trust_log(&format!("[Rhai] ⚠️ the drawing effect didn't compile: {}", why.trim()));
            return;
        }
        // Every value set once: Direct3D won't draw while any is unset
        unsafe {
            set_param(fns, effect, c"size", &floats(&[1.0, 1.0]));
            for name in [c"radius", c"opacity", c"soft"] {
                set_param(fns, effect, name, &floats(&[0.0]));
            }
            set_param(fns, effect, c"uv_rect", &floats(&[0.0, 0.0, 1.0, 1.0]));
            set_param(fns, effect, c"border", &floats(&[0.0; 4]));
            set_param(fns, effect, c"fill", &floats(&[0.0; 4]));
            set_param(fns, effect, c"border_color", &floats(&[0.0; 4]));
        }
        gpu.effect = effect as usize;
    }
    let effect = gpu.effect as P;
    if effect.is_null() {
        return;
    }
    if canvas.texrender == 0 {
        canvas.texrender = unsafe { (fns.texrender_create)(GS_RGBA, 0) } as usize;
    }
    let tr = canvas.texrender as P;
    if tr.is_null() {
        return;
    }

    unsafe {
        (fns.texrender_reset)(tr);
        if !(fns.texrender_begin)(tr, w, h) {
            return;
        }
        let clear = [0.0f32; 4];
        (fns.clear)(GS_CLEAR_COLOR, clear.as_ptr(), 0.0, 0);
        (fns.projection_push)();
        (fns.ortho)(0.0, w as f32, 0.0, h as f32, -100.0, 100.0);
        (fns.matrix_push)();
        (fns.matrix_identity)();
        (fns.blend_state_push)();
        (fns.enable_blending)(true);
        (fns.blend_function_separate)(GS_BLEND_ONE, GS_BLEND_INVSRCALPHA, GS_BLEND_ONE, GS_BLEND_INVSRCALPHA);

        let frame = gpu.frame;
        draw_list(fns, effect, &mut gpu, cmds, 1.0, None);

        (fns.blend_state_pop)();
        (fns.matrix_pop)();
        (fns.projection_pop)();
        (fns.texrender_end)(tr);

        // The canvas where OBS draws the source: premultiplied, as drawn
        let tex = (fns.texrender_get_texture)(tr);
        let base = (fns.get_base_effect)(0); // OBS_EFFECT_DEFAULT
        if !tex.is_null() && !base.is_null() {
            (fns.blend_state_push)();
            (fns.blend_function_separate)(GS_BLEND_ONE, GS_BLEND_INVSRCALPHA, GS_BLEND_ONE, GS_BLEND_INVSRCALPHA);
            let img = (fns.effect_get_param_by_name)(base, c"image".as_ptr());
            if !img.is_null() {
                (fns.effect_set_texture)(img, tex);
            }
            while (fns.effect_loop)(base, c"Draw".as_ptr()) {
                (fns.draw_sprite)(tex, 0, w, h);
            }
            (fns.blend_state_pop)();
        }

        // Textures unused for a while go
        if frame % 600 == 0 {
            let old = |g: &Gpu| frame.saturating_sub(g.used) > KEEP_FRAMES;
            let destroy = fns.texture_destroy;
            gpu.text.retain(|_, g| {
                let keep = !old(g);
                if !keep && g.texture != 0 {
                    destroy(g.texture as P);
                }
                keep
            });
            gpu.images.retain(|_, g| {
                let keep = !old(g);
                if !keep && g.texture != 0 {
                    destroy(g.texture as P);
                }
                keep
            });
        }
    }
}

/// A clip box as [left, top, right, bottom].
type Clip = [f32; 4];

fn intersect(a: Clip, b: Clip) -> Clip {
    [a[0].max(b[0]), a[1].max(b[1]), a[2].min(b[2]), a[3].min(b[3])]
}

/// Draws `cmds` (the list, or a group's) with their alpha times `fade`,
/// only inside `clip` (in their own coordinates) when there's one.
unsafe fn draw_list(fns: &DrawFns, effect: P, gpu: &mut GpuCache, cmds: &[Cmd], fade: f32, clip: Option<Clip>) {
    let frame = gpu.frame;
    for cmd in cmds {
        unsafe {
            match cmd {
                Cmd::Rect { x, y, w, h, fill, radius, border, border_color, soft, alpha } => {
                    set_param(fns, effect, c"size", &floats(&[*w, *h]));
                    set_param(fns, effect, c"radius", &floats(&[*radius]));
                    set_param(fns, effect, c"border", &floats(border));
                    set_param(fns, effect, c"fill", &floats(&rgba_f(*fill)));
                    set_param(fns, effect, c"border_color", &floats(&rgba_f(*border_color)));
                    set_param(fns, effect, c"opacity", &floats(&[alpha * fade]));
                    draw_quad(fns, effect, c"Rect", [*x, *y, *w, *h], *soft, clip, std::ptr::null_mut());
                }
                Cmd::Text { x, y, key, alpha } => {
                    if !gpu.text.contains_key(key) {
                        let pic = rasterize(key);
                        let tex = texture_of(fns, &pic);
                        gpu.text.insert(key.clone(), tex);
                    }
                    let Some(t) = gpu.text.get_mut(key) else { continue };
                    t.used = frame;
                    if t.texture == 0 {
                        continue;
                    }
                    let (tw, th, texture, pad) = (t.width as f32, t.height as f32, t.texture, t.pad as f32);
                    set_param(fns, effect, c"size", &floats(&[tw, th]));
                    set_param(fns, effect, c"radius", &floats(&[0.0]));
                    set_param(fns, effect, c"opacity", &floats(&[alpha * fade]));
                    // Whole pixels: text stays crisp
                    let (px, py) = ((x - pad).round(), (y - pad).round());
                    draw_quad(fns, effect, c"Image", [px, py, tw, th], 0.0, clip, texture as P);
                }
                Cmd::Image { x, y, w, h, src, radius, alpha } => {
                    if !gpu.images.contains_key(src) {
                        let Some(pic) = picture(src) else {
                            // Not loaded yet: ask (again) and skip it this frame
                            let _ = image_size(src, None);
                            continue;
                        };
                        let tex = texture_of(fns, &pic);
                        gpu.images.insert(src.clone(), tex);
                    }
                    let Some(t) = gpu.images.get_mut(src) else { continue };
                    t.used = frame;
                    if t.texture == 0 {
                        continue;
                    }
                    let texture = t.texture;
                    set_param(fns, effect, c"size", &floats(&[*w, *h]));
                    set_param(fns, effect, c"radius", &floats(&[*radius]));
                    set_param(fns, effect, c"opacity", &floats(&[alpha * fade]));
                    draw_quad(fns, effect, c"Image", [*x, *y, *w, *h], 0.0, clip, texture as P);
                }
                Cmd::Group { x, y, scale, origin, alpha, clip: own_clip, add, children } => {
                    let scaled = (*scale - 1.0).abs() > 1e-4;
                    if *scale <= 1e-4 {
                        continue;
                    }
                    // The clip so far, in the children's coordinates (a child at c
                    // is drawn at (x, y) + origin + scale·(c − origin)), and the
                    // group's own on top
                    let to_child = |v: f32, offset: f32, o: f32| if scaled { o + (v - offset - o) / scale } else { v - offset };
                    let mut child_clip = clip.map(|c| {
                        [to_child(c[0], *x, origin.0), to_child(c[1], *y, origin.1), to_child(c[2], *x, origin.0), to_child(c[3], *y, origin.1)]
                    });
                    if let Some(c) = own_clip {
                        let own = [c[0], c[1], c[0] + c[2], c[1] + c[3]];
                        child_clip = Some(child_clip.map_or(own, |inherited| intersect(inherited, own)));
                    }
                    if child_clip.is_some_and(|c| c[2] <= c[0] || c[3] <= c[1]) {
                        continue;
                    }
                    (fns.matrix_push)();
                    (fns.matrix_translate3f)(*x, *y, 0.0);
                    if scaled {
                        (fns.matrix_translate3f)(origin.0, origin.1, 0.0);
                        (fns.matrix_scale3f)(*scale, *scale, 1.0);
                        (fns.matrix_translate3f)(-origin.0, -origin.1, 0.0);
                    }
                    if *add {
                        // Light added to what's under it (premultiplied)
                        (fns.blend_state_push)();
                        (fns.blend_function_separate)(GS_BLEND_ONE, GS_BLEND_ONE, GS_BLEND_ONE, GS_BLEND_ONE);
                    }
                    draw_list(fns, effect, gpu, children, fade * alpha, child_clip);
                    if *add {
                        (fns.blend_state_pop)();
                    }
                    (fns.matrix_pop)();
                }
                Cmd::Line { points, width, color, soft, alpha } => {
                    // Each segment a capsule (a rect as round as it's thick),
                    // turned along it; overlapping ends make the joints round
                    set_param(fns, effect, c"border", &floats(&[0.0; 4]));
                    set_param(fns, effect, c"fill", &floats(&rgba_f(*color)));
                    set_param(fns, effect, c"opacity", &floats(&[alpha * fade]));
                    for seg in points.windows(2) {
                        let ((x0, y0), (x1, y1)) = (seg[0], seg[1]);
                        let len = ((x1 - x0).powi(2) + (y1 - y0).powi(2)).sqrt();
                        let (w, h) = (len + width, *width);
                        set_param(fns, effect, c"size", &floats(&[w, h]));
                        set_param(fns, effect, c"radius", &floats(&[width * 0.5]));
                        (fns.matrix_push)();
                        (fns.matrix_translate3f)(x0, y0, 0.0);
                        (fns.matrix_rotaa4f)(0.0, 0.0, 1.0, (y1 - y0).atan2(x1 - x0));
                        draw_quad(fns, effect, c"Rect", [-width * 0.5, -width * 0.5, w, h], *soft, None, std::ptr::null_mut());
                        (fns.matrix_pop)();
                    }
                }
            }
        }
    }
}

/// The box `b` (x, y, w, h) through `technique`, grown by `soft` on each
/// side, with `texture` as its image: only the part inside `clip`.
unsafe fn draw_quad(fns: &DrawFns, effect: P, technique: &CStr, b: [f32; 4], soft: f32, clip: Option<Clip>, texture: P) {
    let quad = [b[0] - soft, b[1] - soft, b[0] + b[2] + soft, b[1] + b[3] + soft];
    let (qw, qh) = (quad[2] - quad[0], quad[3] - quad[1]);
    if qw <= 0.0 || qh <= 0.0 {
        return;
    }
    let shown = clip.map_or(quad, |c| intersect(quad, c));
    let (sw, sh) = (shown[2] - shown[0], shown[3] - shown[1]);
    if sw <= 0.0 || sh <= 0.0 {
        return;
    }
    unsafe {
        set_param(fns, effect, c"soft", &floats(&[soft]));
        set_param(fns, effect, c"uv_rect", &floats(&[(shown[0] - quad[0]) / qw, (shown[1] - quad[1]) / qh, sw / qw, sh / qh]));
        let img = (fns.effect_get_param_by_name)(effect, c"image".as_ptr());
        if !img.is_null() && !texture.is_null() {
            (fns.effect_set_texture)(img, texture);
        }
        (fns.matrix_push)();
        (fns.matrix_translate3f)(shown[0], shown[1], 0.0);
        (fns.matrix_scale3f)(sw, sh, 1.0);
        while (fns.effect_loop)(effect, technique.as_ptr()) {
            (fns.draw_sprite)(std::ptr::null_mut(), 0, 1, 1);
        }
        (fns.matrix_pop)();
    }
}

/// The Rhai functions a source script calls: measure_text(text, font, size,
/// weight) → [w, h, ascent]; image_size(src) → [w, h], or () while it loads.
pub fn register(engine: &mut rhai::Engine) {
    engine.register_fn("measure_text", |text: &str, font: &str, size: f64, weight: i64| -> Array {
        let (w, h, a) = measure(text, font, size as f32, weight.clamp(100, 1000) as u16);
        vec![Dynamic::from_float(w as f64), Dynamic::from_float(h as f64), Dynamic::from_float(a as f64)]
    });
    engine.register_fn("measure_text", |text: &str, font: &str, size: i64, weight: i64| -> Array {
        let (w, h, a) = measure(text, font, size as f32, weight.clamp(100, 1000) as u16);
        vec![Dynamic::from_float(w as f64), Dynamic::from_float(h as f64), Dynamic::from_float(a as f64)]
    });
    engine.register_fn("image_size", |src: &str| -> Dynamic {
        let base = crate::script_filters::current_script_dir();
        match image_size(src, base.as_deref()) {
            Some((w, h)) => Dynamic::from_array(vec![Dynamic::from_int(w as i64), Dynamic::from_int(h as i64)]),
            None => Dynamic::UNIT,
        }
    });
    engine.register_fn("font_families", || -> Array { font_families().into_iter().map(Dynamic::from).collect() });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theaters_own_fonts_are_found_and_set_text() {
        let dirs = bundled_font_dirs();
        if dirs.is_empty() {
            return;
        }
        // (The installed fonts in the background don't matter here)
        scan_system_fonts();
        let book = BOOK.lock();
        for family in ["poppins", "inter", "outfit", "roboto mono", "bebas neue", "montserrat"] {
            assert!(book.files.iter().any(|f| f.family == family), "{} is found", family);
        }
        // Poppins has static weights; Inter one variable file
        assert!(book.files.iter().any(|f| f.family == "poppins" && f.weight == 700 && !f.variable));
        assert!(book.files.iter().any(|f| f.family == "inter" && f.variable));
        drop(book);

        let (w400, h, ascent) = measure("Hello chat", "Poppins", 16.0, 400);
        let (w700, _, _) = measure("Hello chat", "Poppins", 16.0, 700);
        assert!(w400 > 40.0 && w400 < 160.0 && h > 14.0 && h < 30.0 && ascent > 0.0, "{} {} {}", w400, h, ascent);
        assert!(w700 > w400, "bold is wider");
        let (wi400, _, _) = measure("Hello chat", "Inter", 16.0, 400);
        let (wi700, _, _) = measure("Hello chat", "Inter", 16.0, 700);
        assert!(wi700 > wi400, "the variable font's weight axis is used");

        let key = TextKey { text: "Hi".into(), font: "Poppins".into(), size: 16 * 64, weight: 700, color: 0xFFFF_0000, shadow: true };
        let pic = rasterize(&key);
        assert_eq!((pic.width as usize * pic.height as usize * 4), pic.rgba.len());
        // Red text: some pixels red, premultiplied (red ≤ alpha)
        assert!(pic.rgba.chunks_exact(4).any(|p| p[0] > 200 && p[1] < 40 && p[3] > 200));
        assert!(pic.rgba.chunks_exact(4).all(|p| p[0] <= p[3] && p[1] <= p[3]));
        // The shadow: dark, partly transparent pixels
        assert!(pic.rgba.chunks_exact(4).any(|p| p[3] > 20 && p[3] < 220 && p[0] < 30));
    }

    #[test]
    fn only_the_chat_image_servers_are_fetched() {
        assert!(allowed_url("https://static-cdn.jtvnw.net/emoticons/v2/25/default/dark/2.0"));
        assert!(allowed_url("https://files.kick.com/emotes/123/fullsize"));
        assert!(allowed_url("https://yt3.ggpht.com/abc=s64"));
        assert!(allowed_url("https://fonts.gstatic.com/s/e/notoemoji/15.1/1f928/72.png"));
        assert!(!allowed_url("http://static-cdn.jtvnw.net/x.png"), "https only");
        assert!(!allowed_url("https://evil.example/x.png"));
        assert!(!allowed_url("https://static-cdn.jtvnw.net.evil.example/x.png"));
        assert!(!allowed_url("https://user@evil.example/static-cdn.jtvnw.net"));
        assert!(!allowed_url("https://static-cdn.jtvnw.net:8080/x.png"), "no other ports");
        assert!(!allowed_url("file:///C:/secret.png"));
    }

    #[test]
    fn a_display_list_becomes_commands() {
        let list: Array = vec![
            Dynamic::from_map([("rect".into(), Dynamic::from_array(vec![Dynamic::from_float(1.0), Dynamic::from_int(2), Dynamic::from_float(30.0), Dynamic::from_float(10.0)])), ("fill".into(), Dynamic::from_int(0x8000_00FF)), ("radius".into(), Dynamic::from_float(4.0))].into_iter().collect()),
            Dynamic::from_map([("text".into(), Dynamic::from("hi")), ("x".into(), Dynamic::from_int(5)), ("weight".into(), Dynamic::from_int(700)), ("alpha".into(), Dynamic::from_float(0.5))].into_iter().collect()),
            Dynamic::from_map([("image".into(), Dynamic::from("a.png")), ("w".into(), Dynamic::from_float(0.0)), ("h".into(), Dynamic::from_float(10.0))].into_iter().collect()),
            Dynamic::from_map([("text".into(), Dynamic::from("gone")), ("alpha".into(), Dynamic::from_float(0.0))].into_iter().collect()),
            Dynamic::from_int(3),
        ];
        let cmds = parse(&list);
        assert_eq!(cmds.len(), 2, "a zero-size picture, an invisible text and a number are left out");
        assert!(matches!(cmds[0], Cmd::Rect { x: 1.0, y: 2.0, w: 30.0, h: 10.0, fill: 0x8000_00FF, radius: 4.0, .. }));
        assert!(matches!(&cmds[1], Cmd::Text { x: 5.0, key, alpha: 0.5, .. } if key.weight == 700 && key.font == "Inter"));
    }
}
