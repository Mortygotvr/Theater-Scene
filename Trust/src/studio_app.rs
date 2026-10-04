use eframe::egui;
use egui::{Color32, Pos2, Rect, Stroke, Vec2};
use crate::state::get_state;
use crate::types::GizmoDragMode;
use base64::Engine;

mod scripts_tab;
mod settings_tab;

/// Deterministic, distinct color per source name — every target gets its own
/// stable color across frames (and across restarts), making it easy to tell
/// at a glance which mask/hitbox on screen belongs to which source instead
/// of every target showing the same cyan tint.
fn color_for_source_name(name: &str) -> Color32 {
    let mut hash: u32 = 2166136261; // FNV-1a
    for b in name.bytes() {
        hash ^= b as u32;
        hash = hash.wrapping_mul(16777619);
    }
    let hue = (hash % 360) as f32;
    egui::ecolor::Hsva::new(hue / 360.0, 0.65, 0.95, 1.0).into()
}

fn project_3d_point(lx: f32, ly: f32, cx: f32, cy: f32, roll: f32, pitch: f32, yaw: f32, d: f32) -> Pos2 {
    let rx = pitch.to_radians();
    let ry = yaw.to_radians();
    let rz = roll.to_radians();

    let cos_rx = rx.cos();
    let sin_rx = rx.sin();
    let cos_ry = ry.cos();
    let sin_ry = ry.sin();
    let cos_rz = rz.cos();
    let sin_rz = rz.sin();

    // OBS OpenGL matrix stack order:
    // 1. Ry(yaw)
    let x1 = lx * cos_ry;
    let y1 = ly;
    let z1 = -lx * sin_ry;

    // 2. Rx(pitch)
    let x2 = x1;
    let y2 = y1 * cos_rx - z1 * sin_rx;
    let z2 = y1 * sin_rx + z1 * cos_rx;

    // 3. Rz(roll)
    let x3 = x2 * cos_rz - y2 * sin_rz;
    let y3 = x2 * sin_rz + y2 * cos_rz;
    let z3 = z2;

    // 2.5D Per-Target Perspective Projection scaled with viewport
    let k = d / (d + z3).max(10.0);
    Pos2::new(cx + x3 * k, cy + y3 * k)
}

static IS_STUDIO_THREAD_RUNNING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static IS_STUDIO_WINDOW_VISIBLE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static STUDIO_SHOW_REQUEST: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// Started for the egui docks (start_studio_hidden): its first frame hides it
static STUDIO_HIDE_AT_START: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// Still where it was put out of sight at start: centred when first shown
static STUDIO_OFF_SCREEN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static STUDIO_EGUI_CTX: parking_lot::Mutex<Option<egui::Context>> = parking_lot::Mutex::new(None);
#[cfg(target_os = "windows")]
static STUDIO_HWND: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StudioActiveTab {
    SceneStudio,
    RedeemEditor,
    /// Every script, grouped as reactions, redeems and filters (scripts_tab)
    Scripts,
    /// Theater's own settings: target masks, Rhai filter folders (settings_tab)
    Settings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedeemSubTab {
    Physics,
    Fluid,
    Fx,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RedeemConfig {
    #[serde(default)]
    pub name: String,
    #[serde(default = "default_redeem_version")]
    pub version: String,
    #[serde(rename = "type", default = "default_redeem_type")]
    pub redeem_type: String,
    /// Path to the standalone .lua script that implements this redeem's
    /// entire effect, when redeem_type == "custom". Rust runs it when the
    /// redeem fires (custom_lua::run_custom_redeem_script), with the event's
    /// details as parameters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_redeem_script: Option<String>,
    /// Values for the sliders the custom redeem script declares with
    /// `-- @param` lines (script_sliders), by name. The script finds each in
    /// `params.<name>`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub script_params: Option<std::collections::BTreeMap<String, f64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collision: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speed: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gravity: Option<f32>,
    /// How different each throw is, 0–1 (see physics::SpawnPackage)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path_randomness: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spawn_points: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snap_bottom: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub random_mirror: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub triggers: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub amount: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wildcards: Option<Vec<String>>,
    #[serde(rename = "repeatDelay", default, skip_serializing_if = "Option::is_none")]
    pub repeat_delay: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_src: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_sound: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collision_sound: Option<String>,

    // Fluid fields
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fluid_preset: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fluid_count: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fluid_radius: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fluid_viscosity: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fluid_gravity: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fluid_duration: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fluid_width: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cone_spread: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream_mode: Option<String>,
    /// Overall travel speed multiplier for water — separate from gravity
    /// (arc shape only) and viscosity (drag after emission only). 1.0 = normal.
    #[serde(rename = "streamSpeed", default, skip_serializing_if = "Option::is_none")]
    pub stream_speed: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drip_through_percent: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color_r: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color_g: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color_b: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color_a: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color2_r: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color2_g: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color2_b: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color2_a: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bloom_size: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enable_bloom: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enable_metaball: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub material: Option<serde_json::Value>,

    // Lightning fields
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strike_count: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bolt_count: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bolt_width: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub roughness: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub straightness: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branching: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub smoothness: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enable_impact: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub impact_size: Option<f32>,
    /// Lightning spawn origin ("tc"/"tl"/"tr"/"ml"/"mr").
    /// Stored as `lightSpawn` in JSON. On load, also reads legacy `spawnPoints[0]`
    /// via the custom deserializer. On save, written as `lightSpawn` string.
    #[serde(
        rename = "lightSpawn",
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_light_spawn"
    )]
    pub light_spawn: Option<String>,
    /// Flamethrower ("grid" stream mode) origin ("tc"/"tl"/"tr"/"ml"/"mr") —
    /// where the fire shoots FROM. Unused in "top" (campfire) mode.
    #[serde(rename = "fireSpawn", default, skip_serializing_if = "Option::is_none")]
    pub fire_spawn: Option<String>,
    /// Flamethrower mask collision tuning, in pixels: how far a particle can
    /// sink past the target's mask surface before it's considered "hit".
    /// 0 stops right at the visible surface; negative stops a bit early;
    /// positive lets it dig in further for a more visceral look.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mask_penetration: Option<f32>,
    /// Vertical offset in pixels for fire emission / positioning.
    #[serde(rename = "offsetY", alias = "fireOffsetY", default, skip_serializing_if = "Option::is_none")]
    pub offset_y: Option<f32>,
    /// Water beam ("grid" stream mode) origin ("tc"/"tl"/"tr"/"ml"/"mr") —
    /// where the stream shoots FROM. Unused in "top" (fall from above) mode.
    #[serde(rename = "waterSpawn", default, skip_serializing_if = "Option::is_none")]
    pub water_spawn: Option<String>,
    /// When true, settled/dripping water never fades or shrinks away — lets
    /// it actually accumulate (e.g. filling a cup) instead of dissolving a
    /// few seconds after landing.
    #[serde(rename = "noDecay", default, skip_serializing_if = "Option::is_none")]
    pub no_decay: Option<bool>,
    /// Metaball rendering look for this water redeem — see
    /// `fluid::WaterVisualSettings`. Was previously (wrongly) stored in
    /// global state, which meant it never got saved with the redeem.
    #[serde(rename = "waterTexturePath", default, skip_serializing_if = "Option::is_none")]
    pub water_texture_path: Option<String>,
    #[serde(rename = "waterTextureScale", default, skip_serializing_if = "Option::is_none")]
    pub water_texture_scale: Option<f32>,
    #[serde(rename = "waterTextureScrollSpeed", default, skip_serializing_if = "Option::is_none")]
    pub water_texture_scroll_speed: Option<f32>,
    #[serde(rename = "waterRefractionEnabled", default, skip_serializing_if = "Option::is_none")]
    pub water_refraction_enabled: Option<bool>,
    #[serde(rename = "waterRefractionStrength", default, skip_serializing_if = "Option::is_none")]
    pub water_refraction_strength: Option<f32>,
    #[serde(rename = "waterRoundEdge", default, skip_serializing_if = "Option::is_none")]
    pub water_round_edge: Option<bool>,
    #[serde(rename = "waterMetaballSize", default, skip_serializing_if = "Option::is_none")]
    pub water_metaball_size: Option<f32>,

    // Shared screen shake & impact reactions & lua hook
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shake_on_impact: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub impact_reactions: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_impact_lua: Option<String>,

    // Local file path caches
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_file_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_sound_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collision_sound_path: Option<String>,
    #[serde(rename = "contactSound", alias = "contact_sound", default, skip_serializing_if = "Option::is_none")]
    pub contact_sound: Option<String>,
    #[serde(rename = "contactSoundPath", alias = "contact_sound_path", default, skip_serializing_if = "Option::is_none")]
    pub contact_sound_path: Option<String>,
    #[serde(rename = "contactSoundCooldownMs", alias = "contact_sound_cooldown_ms", default, skip_serializing_if = "Option::is_none")]
    pub contact_sound_cooldown_ms: Option<f32>,
    #[serde(rename = "contactSoundFirstOnly", alias = "contact_sound_first_only", default, skip_serializing_if = "Option::is_none")]
    pub contact_sound_first_only: Option<bool>,

    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

fn default_redeem_version() -> String { "1.0".to_string() }
fn default_redeem_type() -> String { "image".to_string() }



/// Deserialize `light_spawn` from either `"tc"` (plain String) or `["tc"]` (Vec) or null.
/// This handles Lua-saved `lightSpawn` strings and old array formats.
fn deserialize_light_spawn<'de, D>(d: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::Deserialize;
    let v = Option::<serde_json::Value>::deserialize(d)?;
    match v {
        None => Ok(None),
        Some(serde_json::Value::Array(arr)) => {
            let first = arr.into_iter().next()
                .and_then(|v| if let serde_json::Value::String(s) = v { Some(s) } else { None })
                .unwrap_or_else(|| "tc".to_string());
            Ok(Some(first))
        }
        Some(serde_json::Value::String(s)) => Ok(Some(s)),
        Some(serde_json::Value::Null) => Ok(None),
        _ => Ok(None),
    }
}

fn guess_mime_type_from_path(path: &std::path::Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase().as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "ogg" => "audio/ogg",
        "wav" => "audio/wav",
        "mp3" => "audio/mp3",
        _ => "application/octet-stream",
    }
}

impl Default for RedeemConfig {
    fn default() -> Self {
        Self {
            name: "New Redeem".to_string(),
            version: "1.0".to_string(),
            redeem_type: "image".to_string(),
            custom_redeem_script: None,
            script_params: None,
            target: Some("1".to_string()),
            action: Some("throwable".to_string()),
            collision: Some("bounce".to_string()),
            speed: Some(8.0),
            scale: Some(1.0),
            gravity: Some(0.8),
            path_randomness: None,
            spawn_points: Some(vec!["tl".to_string(), "tc".to_string(), "tr".to_string(), "ml".to_string(), "mr".to_string()]),
            snap_bottom: Some(false),
            random_mirror: Some(true),
            // None until the user sets the Reader event that fires it
            triggers: Some(Vec::new()),
            amount: Some(serde_json::Value::String("1".to_string())),
            wildcards: None,
            repeat_delay: Some(150),
            image_src: None,
            start_sound: None,
            collision_sound: None,
            fluid_preset: Some("water".to_string()),
            fluid_count: Some(120),
            fluid_radius: Some(18.0),
            fluid_viscosity: Some(0.5),
            fluid_gravity: Some(0.8),
            fluid_duration: Some(6000),
            fluid_width: Some(30.0),
            cone_spread: Some(30.0),
            stream_mode: Some("grid".to_string()),
            stream_speed: Some(1.0),
            drip_through_percent: Some(50.0),
            color_r: Some(38),
            color_g: Some(155),
            color_b: Some(230),
            color_a: Some(0.95),
            color2_r: Some(145),
            color2_g: Some(232),
            color2_b: Some(255),
            color2_a: Some(0.95),
            bloom_size: Some(1.0),
            enable_bloom: Some(true),
            enable_metaball: Some(true),
            material: None,
            strike_count: Some(3),
            bolt_count: Some(2),
            duration: Some(500),
            bolt_width: Some(4.0),
            roughness: Some(0.85),
            straightness: Some(0.6),
            branching: Some(3),
            smoothness: Some(30),
            enable_impact: Some(true),
            impact_size: Some(1.0),
            light_spawn: Some("tc".to_string()),
            fire_spawn: Some("tc".to_string()),
            mask_penetration: Some(0.0),
            offset_y: Some(0.0),
            water_spawn: Some("tc".to_string()),
            no_decay: Some(false),
            water_texture_path: None,
            water_texture_scale: Some(160.0),
            water_texture_scroll_speed: Some(12.0),
            water_refraction_enabled: Some(true),
            water_refraction_strength: Some(350.0),
            water_round_edge: Some(false),
            water_metaball_size: Some(9.5),
            shake_on_impact: Some(serde_json::json!({
                "enabled": true,
                "duration": 200,
                "intensity": 10
            })),
            impact_reactions: Some(serde_json::json!({
                "bounce": { "enabled": true, "squish_amount": 0.4, "duration_ms": 500 },
                "pixel_drip": { "enabled": false, "viscosity": 0.6, "duration_ms": 2500 },
                "jolt": { "enabled": false, "intensity": 0.8, "duration_ms": 400 }
            })),
            on_impact_lua: None,
            image_file_path: None,
            start_sound_path: None,
            collision_sound_path: None,
            contact_sound: None,
            contact_sound_path: None,
            contact_sound_cooldown_ms: Some(120.0),
            contact_sound_first_only: Some(false),
            extra: serde_json::Map::new(),
        }
    }
}

impl RedeemConfig {
    pub fn get_target(&self) -> String {
        self.target.clone().unwrap_or_default()
    }

    pub fn set_target(&mut self, target: impl Into<String>) {
        let t = target.into();
        self.target = if t.is_empty() { None } else { Some(t) };
    }

    pub fn get_target_slot(&self) -> String {
        self.get_target()
    }

    pub fn set_target_slot(&mut self, slot: impl Into<String>) {
        self.set_target(slot);
    }

    pub fn get_shake_enabled(&self) -> bool {
        self.shake_on_impact.as_ref()
            .and_then(|v| v.get("enabled"))
            .and_then(|b| b.as_bool())
            .unwrap_or(false)
    }

    pub fn set_shake_enabled(&mut self, enabled: bool) {
        let mut obj = self.shake_on_impact.clone().unwrap_or_else(|| serde_json::json!({
            "enabled": true,
            "duration": 200,
            "intensity": 10
        }));
        if let Some(map) = obj.as_object_mut() {
            map.insert("enabled".to_string(), serde_json::Value::Bool(enabled));
        }
        self.shake_on_impact = Some(obj);
    }

    pub fn get_shake_duration(&self) -> u64 {
        self.shake_on_impact.as_ref()
            .and_then(|v| v.get("duration"))
            .and_then(|n| n.as_u64())
            .unwrap_or(200)
    }

    pub fn set_shake_duration(&mut self, dur: u64) {
        let mut obj = self.shake_on_impact.clone().unwrap_or_else(|| serde_json::json!({
            "enabled": true,
            "duration": 200,
            "intensity": 10
        }));
        if let Some(map) = obj.as_object_mut() {
            map.insert("duration".to_string(), serde_json::json!(dur));
        }
        self.shake_on_impact = Some(obj);
    }

    pub fn get_shake_intensity(&self) -> u32 {
        self.shake_on_impact.as_ref()
            .and_then(|v| v.get("intensity"))
            .and_then(|n| n.as_u64())
            .map(|n| n as u32)
            .unwrap_or(10)
    }

    pub fn set_shake_intensity(&mut self, inten: u32) {
        let mut obj = self.shake_on_impact.clone().unwrap_or_else(|| serde_json::json!({
            "enabled": true,
            "duration": 200,
            "intensity": 10
        }));
        if let Some(map) = obj.as_object_mut() {
            map.insert("intensity".to_string(), serde_json::json!(inten));
        }
        self.shake_on_impact = Some(obj);
    }

    /// "impact" (throwables only: fires on contact) or "start" (fires
    /// immediately on trigger — the only option that does anything for
    /// water/fire/lightning, which have no discrete contact moment).
    pub fn get_shake_timing(&self) -> String {
        self.shake_on_impact.as_ref()
            .and_then(|v| v.get("timing"))
            .and_then(|t| t.as_str())
            .unwrap_or("impact")
            .to_string()
    }

    pub fn set_shake_timing(&mut self, timing: impl Into<String>) {
        let mut obj = self.shake_on_impact.clone().unwrap_or_else(|| serde_json::json!({
            "enabled": true,
            "duration": 200,
            "intensity": 10
        }));
        if let Some(map) = obj.as_object_mut() {
            map.insert("timing".to_string(), serde_json::Value::String(timing.into()));
        }
        self.shake_on_impact = Some(obj);
    }

    // Impact Reactions: Bounce
    pub fn get_bounce_enabled(&self) -> bool {
        self.impact_reactions.as_ref()
            .and_then(|v| v.get("bounce"))
            .and_then(|b| b.get("enabled"))
            .and_then(|e| e.as_bool())
            .unwrap_or(true)
    }

    pub fn set_bounce_enabled(&mut self, enabled: bool) {
        self.ensure_reactions();
        if let Some(obj) = self.impact_reactions.as_mut().and_then(|v| v.as_object_mut()) {
            let mut bounce = obj.get("bounce").cloned().unwrap_or_else(|| serde_json::json!({
                "enabled": true,
                "squish_amount": 0.4,
                "duration_ms": 500
            }));
            if let Some(bmap) = bounce.as_object_mut() {
                bmap.insert("enabled".to_string(), serde_json::Value::Bool(enabled));
            }
            obj.insert("bounce".to_string(), bounce);
        }
    }

    pub fn get_bounce_squish(&self) -> f32 {
        self.impact_reactions.as_ref()
            .and_then(|v| v.get("bounce"))
            .and_then(|b| b.get("squish_amount").or_else(|| b.get("squishAmount")))
            .and_then(|s| s.as_f64())
            .map(|f| f as f32)
            .unwrap_or(0.4)
    }

    pub fn set_bounce_squish(&mut self, squish: f32) {
        self.ensure_reactions();
        if let Some(obj) = self.impact_reactions.as_mut().and_then(|v| v.as_object_mut()) {
            let mut bounce = obj.get("bounce").cloned().unwrap_or_else(|| serde_json::json!({
                "enabled": true,
                "squish_amount": 0.4,
                "duration_ms": 500
            }));
            if let Some(bmap) = bounce.as_object_mut() {
                bmap.insert("squish_amount".to_string(), serde_json::json!(squish));
            }
            obj.insert("bounce".to_string(), bounce);
        }
    }

    pub fn get_bounce_duration(&self) -> u64 {
        self.impact_reactions.as_ref()
            .and_then(|v| v.get("bounce"))
            .and_then(|b| b.get("duration_ms").or_else(|| b.get("durationMs")))
            .and_then(|d| d.as_u64())
            .unwrap_or(500)
    }

    pub fn set_bounce_duration(&mut self, dur: u64) {
        self.ensure_reactions();
        if let Some(obj) = self.impact_reactions.as_mut().and_then(|v| v.as_object_mut()) {
            let mut bounce = obj.get("bounce").cloned().unwrap_or_else(|| serde_json::json!({
                "enabled": true,
                "squish_amount": 0.4,
                "duration_ms": 500
            }));
            if let Some(bmap) = bounce.as_object_mut() {
                bmap.insert("duration_ms".to_string(), serde_json::json!(dur));
            }
            obj.insert("bounce".to_string(), bounce);
        }
    }

    // Impact Reactions: Drip
    pub fn get_drip_enabled(&self) -> bool {
        self.impact_reactions.as_ref()
            .and_then(|v| v.get("pixel_drip").or_else(|| v.get("pixelDrip")))
            .and_then(|d| d.get("enabled"))
            .and_then(|e| e.as_bool())
            .unwrap_or(self.redeem_type == "webgl-water")
    }

    pub fn set_drip_enabled(&mut self, enabled: bool) {
        self.ensure_reactions();
        if let Some(obj) = self.impact_reactions.as_mut().and_then(|v| v.as_object_mut()) {
            let mut drip = obj.get("pixel_drip").or_else(|| obj.get("pixelDrip")).cloned().unwrap_or_else(|| serde_json::json!({
                "enabled": false,
                "viscosity": 0.6,
                "duration_ms": 2500
            }));
            if let Some(dmap) = drip.as_object_mut() {
                dmap.insert("enabled".to_string(), serde_json::Value::Bool(enabled));
            }
            obj.insert("pixel_drip".to_string(), drip);
        }
    }

    pub fn get_drip_viscosity(&self) -> f32 {
        self.impact_reactions.as_ref()
            .and_then(|v| v.get("pixel_drip").or_else(|| v.get("pixelDrip")))
            .and_then(|d| d.get("viscosity"))
            .and_then(|s| s.as_f64())
            .map(|f| f as f32)
            .unwrap_or(0.6)
    }

    pub fn set_drip_viscosity(&mut self, visc: f32) {
        self.ensure_reactions();
        if let Some(obj) = self.impact_reactions.as_mut().and_then(|v| v.as_object_mut()) {
            let mut drip = obj.get("pixel_drip").or_else(|| obj.get("pixelDrip")).cloned().unwrap_or_else(|| serde_json::json!({
                "enabled": false,
                "viscosity": 0.6,
                "duration_ms": 2500
            }));
            if let Some(dmap) = drip.as_object_mut() {
                dmap.insert("viscosity".to_string(), serde_json::json!(visc));
            }
            obj.insert("pixel_drip".to_string(), drip);
        }
    }

    pub fn get_drip_duration(&self) -> u64 {
        self.impact_reactions.as_ref()
            .and_then(|v| v.get("pixel_drip").or_else(|| v.get("pixelDrip")))
            .and_then(|d| d.get("duration_ms").or_else(|| d.get("durationMs")))
            .and_then(|d| d.as_u64())
            .unwrap_or(2500)
    }

    pub fn set_drip_duration(&mut self, dur: u64) {
        self.ensure_reactions();
        if let Some(obj) = self.impact_reactions.as_mut().and_then(|v| v.as_object_mut()) {
            let mut drip = obj.get("pixel_drip").or_else(|| obj.get("pixelDrip")).cloned().unwrap_or_else(|| serde_json::json!({
                "enabled": false,
                "viscosity": 0.6,
                "duration_ms": 2500
            }));
            if let Some(dmap) = drip.as_object_mut() {
                dmap.insert("duration_ms".to_string(), serde_json::json!(dur));
            }
            obj.insert("pixel_drip".to_string(), drip);
        }
    }

    // Impact Reactions: Jolt
    pub fn get_jolt_enabled(&self) -> bool {
        self.impact_reactions.as_ref()
            .and_then(|v| v.get("jolt"))
            .and_then(|j| j.get("enabled"))
            .and_then(|e| e.as_bool())
            .unwrap_or(self.redeem_type == "webgl-lightning")
    }

    pub fn set_jolt_enabled(&mut self, enabled: bool) {
        self.ensure_reactions();
        if let Some(obj) = self.impact_reactions.as_mut().and_then(|v| v.as_object_mut()) {
            let mut jolt = obj.get("jolt").cloned().unwrap_or_else(|| serde_json::json!({
                "enabled": false,
                "intensity": 0.8,
                "duration_ms": 400
            }));
            if let Some(jmap) = jolt.as_object_mut() {
                jmap.insert("enabled".to_string(), serde_json::Value::Bool(enabled));
            }
            obj.insert("jolt".to_string(), jolt);
        }
    }

    pub fn get_jolt_intensity(&self) -> f32 {
        self.impact_reactions.as_ref()
            .and_then(|v| v.get("jolt"))
            .and_then(|j| j.get("intensity"))
            .and_then(|s| s.as_f64())
            .map(|f| f as f32)
            .unwrap_or(0.8)
    }

    pub fn set_jolt_intensity(&mut self, inten: f32) {
        self.ensure_reactions();
        if let Some(obj) = self.impact_reactions.as_mut().and_then(|v| v.as_object_mut()) {
            let mut jolt = obj.get("jolt").cloned().unwrap_or_else(|| serde_json::json!({
                "enabled": false,
                "intensity": 0.8,
                "duration_ms": 400
            }));
            if let Some(jmap) = jolt.as_object_mut() {
                jmap.insert("intensity".to_string(), serde_json::json!(inten));
            }
            obj.insert("jolt".to_string(), jolt);
        }
    }

    pub fn get_jolt_duration(&self) -> u64 {
        self.impact_reactions.as_ref()
            .and_then(|v| v.get("jolt"))
            .and_then(|j| j.get("duration_ms").or_else(|| j.get("durationMs")))
            .and_then(|d| d.as_u64())
            .unwrap_or(400)
    }

    pub fn set_jolt_duration(&mut self, dur: u64) {
        self.ensure_reactions();
        if let Some(obj) = self.impact_reactions.as_mut().and_then(|v| v.as_object_mut()) {
            let mut jolt = obj.get("jolt").cloned().unwrap_or_else(|| serde_json::json!({
                "enabled": false,
                "intensity": 0.8,
                "duration_ms": 400
            }));
            if let Some(jmap) = jolt.as_object_mut() {
                jmap.insert("duration_ms".to_string(), serde_json::json!(dur));
            }
            obj.insert("jolt".to_string(), jolt);
        }
    }

    /// "impact" (throwables only: fires on contact) or "start" (fires
    /// immediately on trigger) for whichever reaction is currently enabled.
    pub fn get_reaction_timing(&self) -> String {
        let Some(obj) = self.impact_reactions.as_ref() else { return "impact".to_string(); };
        for key in ["bounce", "pixel_drip", "jolt", "custom_script"] {
            if let Some(sub) = obj.get(key) {
                if sub.get("enabled").and_then(|e| e.as_bool()).unwrap_or(false) {
                    return sub.get("timing").and_then(|t| t.as_str()).unwrap_or("impact").to_string();
                }
            }
        }
        "impact".to_string()
    }

    pub fn set_reaction_timing(&mut self, timing: impl Into<String>) {
        let timing = timing.into();
        if let Some(obj) = self.impact_reactions.as_mut().and_then(|v| v.as_object_mut()) {
            for key in ["bounce", "pixel_drip", "jolt", "custom_script"] {
                if let Some(sub) = obj.get_mut(key).and_then(|s| s.as_object_mut()) {
                    if sub.get("enabled").and_then(|e| e.as_bool()).unwrap_or(false) {
                        sub.insert("timing".to_string(), serde_json::Value::String(timing.clone()));
                    }
                }
            }
        }
    }

    /// A loaded Lua Effect script selected as this redeem's reaction, instead
    /// of a built-in (Bounce/Drip/Jolt). Not wired up to actually load/attach
    /// the script yet — Explorer-side selection + persistence only for now.
    pub fn get_custom_script_enabled(&self) -> bool {
        self.impact_reactions.as_ref()
            .and_then(|v| v.get("custom_script"))
            .and_then(|c| c.get("enabled"))
            .and_then(|e| e.as_bool())
            .unwrap_or(false)
    }

    pub fn get_custom_script_path(&self) -> Option<String> {
        self.impact_reactions.as_ref()
            .and_then(|v| v.get("custom_script"))
            .and_then(|c| c.get("path"))
            .and_then(|p| p.as_str())
            .map(|s| s.to_string())
    }

    /// Selects a custom script as the active reaction, disabling the built-in ones.
    pub fn set_custom_script(&mut self, path: &str, name: &str) {
        self.set_bounce_enabled(false);
        self.set_drip_enabled(false);
        self.set_jolt_enabled(false);
        let mut obj = self.impact_reactions.clone().unwrap_or_else(|| serde_json::json!({}));
        if let Some(map) = obj.as_object_mut() {
            map.insert("custom_script".to_string(), serde_json::json!({
                "enabled": true,
                "path": path,
                "name": name,
            }));
        }
        self.impact_reactions = Some(obj);
    }

    /// Disables the custom script reaction (used when switching to a built-in one).
    pub fn clear_custom_script(&mut self) {
        if let Some(obj) = self.impact_reactions.as_mut().and_then(|v| v.as_object_mut()) {
            if let Some(sub) = obj.get_mut("custom_script").and_then(|s| s.as_object_mut()) {
                sub.insert("enabled".to_string(), serde_json::Value::Bool(false));
            }
        }
    }

    /// How long (ms) we keep rendering the avatar through this custom shader
    /// before reverting to the plain draw — i.e. how long the reaction "stays
    /// applied". Defaults to 4000ms since we can't know how long a given
    /// script's own animation needs to finish.
    pub fn get_custom_script_duration(&self) -> u32 {
        self.impact_reactions.as_ref()
            .and_then(|v| v.get("custom_script"))
            .and_then(|c| c.get("duration_ms"))
            .and_then(|d| d.as_u64())
            .map(|d| d as u32)
            .unwrap_or(4000)
    }

    pub fn set_custom_script_duration(&mut self, duration_ms: u32) {
        let mut obj = self.impact_reactions.clone().unwrap_or_else(|| serde_json::json!({}));
        if let Some(map) = obj.as_object_mut() {
            let custom = map.entry("custom_script").or_insert_with(|| serde_json::json!({}));
            if let Some(custom_map) = custom.as_object_mut() {
                custom_map.insert("duration_ms".to_string(), serde_json::json!(duration_ms));
            }
        }
        self.impact_reactions = Some(obj);
    }

    /// A user-overridden value for one of a custom script's own discovered
    /// uniforms (e.g. "squish_amount"), or `default` if never overridden.
    pub fn get_custom_script_param(&self, name: &str, default: f32) -> f32 {
        self.impact_reactions.as_ref()
            .and_then(|v| v.get("custom_script"))
            .and_then(|c| c.get("params"))
            .and_then(|p| p.get(name))
            .and_then(|v| v.as_f64())
            .map(|v| v as f32)
            .unwrap_or(default)
    }

    pub fn set_custom_script_param(&mut self, name: &str, value: f32) {
        let mut obj = self.impact_reactions.clone().unwrap_or_else(|| serde_json::json!({}));
        if let Some(map) = obj.as_object_mut() {
            let custom = map.entry("custom_script").or_insert_with(|| serde_json::json!({}));
            if let Some(custom_map) = custom.as_object_mut() {
                let params = custom_map.entry("params").or_insert_with(|| serde_json::json!({}));
                if let Some(params_map) = params.as_object_mut() {
                    params_map.insert(name.to_string(), serde_json::json!(value));
                }
            }
        }
        self.impact_reactions = Some(obj);
    }

    fn ensure_reactions(&mut self) {
        if self.impact_reactions.is_none() {
            self.impact_reactions = Some(serde_json::json!({
                "bounce": { "enabled": true, "squish_amount": 0.4, "duration_ms": 500 },
                "pixel_drip": { "enabled": self.redeem_type == "webgl-water", "viscosity": 0.6, "duration_ms": 2500 },
                "jolt": { "enabled": self.redeem_type == "webgl-lightning", "intensity": 0.8, "duration_ms": 400 }
            }));
        }
    }

    pub fn apply_fluid_preset(&mut self, preset: &str) {
        self.fluid_preset = Some(preset.to_string());
        match preset {
            "water" => {
                self.color_r = Some(38); self.color_g = Some(155); self.color_b = Some(230); self.color_a = Some(0.95);
                self.color2_r = Some(145); self.color2_g = Some(232); self.color2_b = Some(255); self.color2_a = Some(0.95);
                self.fluid_width = Some(38.0); self.fluid_duration = Some(5000);
                self.fluid_viscosity = Some(0.50); self.fluid_gravity = Some(0.80);
            }
            "chemical" | "acid" => {
                self.color_r = Some(10); self.color_g = Some(230); self.color_b = Some(65); self.color_a = Some(0.92);
                self.color2_r = Some(185); self.color2_g = Some(255); self.color2_b = Some(30); self.color2_a = Some(0.90);
                self.fluid_viscosity = Some(0.40); self.fluid_gravity = Some(0.85);
            }
            "milk" => {
                self.color_r = Some(245); self.color_g = Some(245); self.color_b = Some(240); self.color_a = Some(0.98);
                self.color2_r = Some(255); self.color2_g = Some(255); self.color2_b = Some(255); self.color2_a = Some(0.98);
                self.fluid_viscosity = Some(0.65); self.fluid_gravity = Some(0.85);
            }
            "slime" => {
                self.color_r = Some(30); self.color_g = Some(200); self.color_b = Some(40); self.color_a = Some(0.95);
                self.color2_r = Some(120); self.color2_g = Some(255); self.color2_b = Some(60); self.color2_a = Some(0.90);
                self.fluid_viscosity = Some(0.85); self.fluid_gravity = Some(0.60);
            }
            "lava" => {
                self.color_r = Some(170); self.color_g = Some(25); self.color_b = Some(0); self.color_a = Some(0.98);
                self.color2_r = Some(255); self.color2_g = Some(140); self.color2_b = Some(0); self.color2_a = Some(0.98);
                self.fluid_viscosity = Some(0.90); self.fluid_gravity = Some(0.70);
            }
            "blood" => {
                self.color_r = Some(160); self.color_g = Some(0); self.color_b = Some(15); self.color_a = Some(0.95);
                self.color2_r = Some(230); self.color2_g = Some(20); self.color2_b = Some(30); self.color2_a = Some(0.90);
                self.fluid_viscosity = Some(0.75); self.fluid_gravity = Some(0.80);
            }
            "gold" | "metal" => {
                self.color_r = Some(235); self.color_g = Some(185); self.color_b = Some(45); self.color_a = Some(0.98);
                self.color2_r = Some(255); self.color2_g = Some(245); self.color2_b = Some(165); self.color2_a = Some(0.98);
                self.fluid_viscosity = Some(0.70); self.fluid_gravity = Some(0.90);
            }
            "glass" => {
                let smooth_tex = crate::get_theater_repo_dir().join("Trust/assets/textures/smooth_deformed_glass.png").to_string_lossy().to_string();
                self.water_texture_path = Some(smooth_tex);
                self.water_texture_scale = Some(180.0);
                self.water_texture_scroll_speed = Some(18.0);
                self.water_refraction_enabled = Some(true);
                self.water_refraction_strength = Some(350.0);
            }
            "sand" => {
                self.color_r = Some(190); self.color_g = Some(150); self.color_b = Some(90); self.color_a = Some(0.95);
                self.color2_r = Some(230); self.color2_g = Some(200); self.color2_b = Some(140); self.color2_a = Some(0.90);
                self.fluid_viscosity = Some(0.30); self.fluid_gravity = Some(1.20);
            }
            _ => {}
        }
        self.material = Some(serde_json::json!({
            "preset": preset,
            "baseColor": [self.color_r.unwrap_or(38), self.color_g.unwrap_or(155), self.color_b.unwrap_or(230)],
            "baseAlpha": self.color_a.unwrap_or(0.95),
            "secondaryColor": [self.color2_r.unwrap_or(145), self.color2_g.unwrap_or(232), self.color2_b.unwrap_or(255)],
            "secondaryAlpha": self.color2_a.unwrap_or(0.95)
        }));
    }
}

#[derive(Debug, Clone)]
pub struct RedeemItem {
    pub filename: String,
    pub path: std::path::PathBuf,
    pub config: RedeemConfig,
    pub is_dirty: bool,
    pub triggers_input: String,
}

pub fn normalize_pattern(pattern: &str) -> String {
    let mut s = pattern.to_string();
    let placeholders = [
        "{1}", "{2}", "{3}", "{4}", "{5}", "{6}", "{7}", "{8}", "{9}",
        "{amount}", "{count}", "{target}", "{user}", "{username}",
    ];
    for ph in &placeholders {
        s = s.replace(ph, "*");
    }
    s
}

pub fn match_and_extract_wildcards(pattern: &str, target: &str) -> Option<Vec<String>> {
    let norm_pat = normalize_pattern(pattern.trim());
    let p = norm_pat.trim();
    let t = target.trim();
    if p.is_empty() || t.is_empty() {
        return None;
    }

    let p_lower = p.to_lowercase();
    let t_lower = t.to_lowercase();

    // If no wildcard '*', exact string match (case-insensitive)
    if !p_lower.contains('*') {
        if p_lower == t_lower {
            return Some(Vec::new());
        }
        return None;
    }

    // Split pattern by '*'
    let parts: Vec<&str> = p.split('*').collect();
    let first = parts[0].to_lowercase();
    let last = parts[parts.len() - 1].to_lowercase();

    if !t_lower.starts_with(&first) || !t_lower.ends_with(&last) {
        return None;
    }
    if t.len() < first.len() + last.len() {
        return None;
    }

    let mut wildcards = Vec::new();
    let mut current_idx = first.len();
    let end_idx = t.len() - last.len();

    for &part in &parts[1..parts.len() - 1] {
        let part_lower = part.to_lowercase();
        if part.is_empty() {
            continue;
        }
        if let Some(pos) = t_lower[current_idx..end_idx].find(&part_lower) {
            let match_start = current_idx + pos;
            let captured = t[current_idx..match_start].trim().to_string();
            wildcards.push(captured);
            current_idx = match_start + part.len();
        } else {
            return None;
        }
    }

    if current_idx <= end_idx {
        let last_captured = t[current_idx..end_idx].trim().to_string();
        wildcards.push(last_captured);
    }

    Some(wildcards)
}

pub fn match_trigger_wildcard(pattern: &str, target: &str) -> bool {
    match_and_extract_wildcards(pattern, target).is_some()
}

pub static REQUESTED_STUDIO_TAB: parking_lot::Mutex<Option<StudioActiveTab>> = parking_lot::Mutex::new(None);

pub fn switch_studio_tab(tab: StudioActiveTab) {
    *REQUESTED_STUDIO_TAB.lock() = Some(tab);
    if let Some(ref ctx) = *STUDIO_EGUI_CTX.lock() {
        ctx.request_repaint();
    }
}

pub fn get_redeems_dir() -> std::path::PathBuf {
    // The relative "redeems" candidate only resolves correctly if this runs
    // after OBS's working directory has settled to the user's home directory
    // — true later (e.g. the Studio window's own "New" button, which is why
    // newly created redeems landed in ~/redeems), but not necessarily true
    // yet at the very first call from module load / early startup, which
    // needs an absolute fallback that doesn't depend on cwd — resolved from
    // $HOME rather than a hardcoded username so it isn't tied to this one
    // machine's account name.
    //
    // Windows has no $HOME, and OBS's working directory there is its own
    // Program Files bin folder, so the relative fallback pointed at a folder
    // that doesn't exist and can't be written: redeems never saved or loaded.
    // Keep them in the repo instead, next to theater_targets.json.
    #[cfg(target_os = "windows")]
    {
        return crate::get_theater_repo_dir().join("redeems");
    }
    #[allow(unreachable_code)]
    let home = std::env::var("HOME").ok().map(std::path::PathBuf::from);
    let mut candidates = vec![std::path::PathBuf::from("redeems")];
    if let Some(ref home) = home {
        candidates.push(home.join("redeems"));
        candidates.push(home.join("repo/theater-scene/redeems"));
    }
    for c in &candidates {
        if c.is_dir() {
            return c.clone();
        }
    }
    home.map(|h| h.join("redeems")).unwrap_or_else(|| std::path::PathBuf::from("redeems"))
}

pub fn get_filters_dir() -> std::path::PathBuf {
    let repo = crate::get_theater_repo_dir();
    let candidates = [
        repo.join("filters"),
        std::path::PathBuf::from("filters"),
        std::path::PathBuf::from("/home/mortygotvr/repo/theater-scene/filters"),
    ];
    for c in &candidates {
        if c.is_dir() {
            return c.clone();
        }
    }
    repo.join("filters")
}

/// Finds the Lua script a redeem or reaction points at. A path from another
/// machine (doom.lua's swirl, redeems made on Linux) doesn't exist here, so a
/// missing file falls back to the one of the same name in the filters folder
/// or the repo; a relative path is taken from the repo.
pub fn resolve_script_path(raw: &str) -> String {
    let raw = raw.trim();
    let path = std::path::Path::new(raw);
    if raw.is_empty() || path.is_file() {
        return raw.to_string();
    }
    let repo = crate::get_theater_repo_dir();
    let mut candidates = Vec::new();
    if path.is_relative() {
        candidates.push(repo.join(path));
    }
    // Either separator: a Linux path on Windows and the other way round
    if let Some(name) = raw.rsplit(['/', '\\']).next().filter(|n| !n.is_empty()) {
        candidates.push(get_filters_dir().join(name));
        candidates.push(repo.join(name));
    }
    candidates
        .into_iter()
        .find(|c| c.is_file())
        .map(|c| c.to_string_lossy().into_owned())
        .unwrap_or_else(|| raw.to_string())
}

pub fn encode_file_path_to_data_uri(path: &std::path::Path) -> Option<String> {
    if path.is_file() {
        if let Ok(bytes) = std::fs::read(path) {
            let mime = guess_mime_type_from_path(path);
            let b64 = format!("data:{};base64,{}", mime, base64::prelude::BASE64_STANDARD.encode(&bytes));
            return Some(b64);
        }
    }
    None
}

pub fn resolve_and_encode_asset(raw_path: &str) -> Option<String> {
    let raw = raw_path.trim();
    if raw.is_empty() {
        return None;
    }
    let p = std::path::Path::new(raw);
    if let Some(uri) = encode_file_path_to_data_uri(p) {
        return Some(uri);
    }
    let in_redeems = get_redeems_dir().join(raw);
    if let Some(uri) = encode_file_path_to_data_uri(&in_redeems) {
        return Some(uri);
    }
    let in_repo = crate::get_theater_repo_dir().join(raw);
    if let Some(uri) = encode_file_path_to_data_uri(&in_repo) {
        return Some(uri);
    }
    None
}

/// The file type of redeem packages (redeem_package), for the file dialogs
pub const REDEEM_PACKAGE_FILTER: &str = "Theater Redeem (*.red) | *.red";

/// Asks for a redeem package to import.
pub fn pick_redeem_package_dialog() -> Option<std::path::PathBuf> {
    #[cfg(target_os = "windows")]
    return win_file_dialog("Import Redeem", REDEEM_PACKAGE_FILTER, None, None);
    #[cfg(not(target_os = "windows"))]
    return pick_file_dialog("Import Redeem", REDEEM_PACKAGE_FILTER);
}

/// Asks where to export a redeem package, suggesting `file_name`; asks before
/// replacing a file.
pub fn save_redeem_package_dialog(file_name: &str) -> Option<std::path::PathBuf> {
    save_file_dialog("Export Redeem", REDEEM_PACKAGE_FILTER, file_name)
}

/// The Save As dialog (`file_filter` as zenity says it: "Name | *.a"),
/// suggesting `file_name`; asks before replacing a file. Blocking.
pub fn save_file_dialog(title: &str, file_filter: &str, file_name: &str) -> Option<std::path::PathBuf> {
    #[cfg(target_os = "windows")]
    return win_file_dialog(title, file_filter, Some(file_name), None);
    #[cfg(not(target_os = "windows"))]
    {
        let mut cmd = std::process::Command::new("zenity");
        cmd.args(["--file-selection", "--save", "--confirm-overwrite"])
            .arg(format!("--title={}", title))
            .arg(format!("--filename={}", file_name))
            .arg(format!("--file-filter={}", file_filter));
        let mut kcmd = std::process::Command::new("kdialog");
        kcmd.arg("--getsavefilename").arg(file_name).arg(format!("--title={}", title));
        for mut c in [cmd, kcmd] {
            if let Ok(output) = c.output() {
                let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if output.status.success() && !s.is_empty() {
                    return Some(std::path::PathBuf::from(s));
                }
            }
        }
        None
    }
}

/// Windows: the native common Open dialog (zenity/kdialog don't exist there).
/// `file_filter` uses zenity's "Name | *.a *.b" syntax, same as the Linux
/// path, converted to Win32's "Name\0*.a;*.b\0" form with "All Files"
/// appended like zenity's second --file-filter. Blocking, like zenity — every
/// caller already runs it on its own thread.
#[cfg(target_os = "windows")]
pub fn pick_file_dialog(title: &str, file_filter: &str) -> Option<std::path::PathBuf> {
    win_file_dialog(title, file_filter, None, Some(&crate::get_theater_repo_dir()))
}

/// The Open dialog, or with `save_as` (the file name it starts on) the Save As
/// dialog, which asks before replacing a file. Without `initial_dir`, Windows
/// opens the folder last used.
#[cfg(target_os = "windows")]
fn win_file_dialog(
    title: &str,
    file_filter: &str,
    save_as: Option<&str>,
    initial_dir: Option<&std::path::Path>,
) -> Option<std::path::PathBuf> {
    #[repr(C)]
    struct OpenFileNameW {
        struct_size: u32,
        hwnd_owner: isize,
        instance: isize,
        filter: *const u16,
        custom_filter: *mut u16,
        max_cust_filter: u32,
        filter_index: u32,
        file: *mut u16,
        max_file: u32,
        file_title: *mut u16,
        max_file_title: u32,
        initial_dir: *const u16,
        title: *const u16,
        flags: u32,
        file_offset: u16,
        file_extension: u16,
        def_ext: *const u16,
        cust_data: isize,
        hook: *const std::ffi::c_void,
        template_name: *const u16,
        reserved_ptr: *mut std::ffi::c_void,
        reserved: u32,
        flags_ex: u32,
    }
    const _: () = assert!(std::mem::size_of::<OpenFileNameW>() == 152, "OPENFILENAMEW size mismatch");
    #[link(name = "comdlg32")]
    unsafe extern "system" {
        fn GetOpenFileNameW(ofn: *mut OpenFileNameW) -> i32;
        fn GetSaveFileNameW(ofn: *mut OpenFileNameW) -> i32;
    }
    #[link(name = "ole32")]
    unsafe extern "system" {
        fn CoInitializeEx(reserved: *mut std::ffi::c_void, co_init: u32) -> i32;
        fn CoUninitialize();
    }
    const COINIT_APARTMENTTHREADED: u32 = 0x2;
    const COINIT_DISABLE_OLE1DDE: u32 = 0x4;
    const OFN_OVERWRITEPROMPT: u32 = 0x2;
    const OFN_HIDEREADONLY: u32 = 0x4;
    // Keeps the dialog from changing OBS's process-wide working directory
    const OFN_NOCHANGEDIR: u32 = 0x8;
    const OFN_PATHMUSTEXIST: u32 = 0x800;
    const OFN_FILEMUSTEXIST: u32 = 0x1000;
    const OFN_EXPLORER: u32 = 0x80000;

    let wide = |s: &str| -> Vec<u16> { s.encode_utf16().chain(std::iter::once(0)).collect() };

    let mut filter = String::new();
    if !file_filter.is_empty() {
        let (name, patterns) = file_filter.split_once('|').unwrap_or((file_filter, file_filter));
        filter.push_str(name.trim());
        filter.push('\0');
        filter.push_str(&patterns.split_whitespace().collect::<Vec<_>>().join(";"));
        filter.push('\0');
    }
    filter.push_str("All Files\0*.*\0");
    let filter_w = wide(&filter); // trailing NUL from `wide` terminates the list
    let title_w = wide(title);
    let initial_dir_w = initial_dir.map(|d| wide(&d.to_string_lossy()));
    // Save As adds the filter's extension ("*.red" → "red") to a name typed without one
    let default_ext_w = file_filter
        .split_once('|')
        .and_then(|(_, patterns)| patterns.split_whitespace().next())
        .and_then(|p| p.strip_prefix("*."))
        .map(wide);
    let mut file_buf = vec![0u16; 32768];
    if let Some(name) = save_as {
        for (slot, c) in file_buf.iter_mut().zip(name.encode_utf16().take(32767)) {
            *slot = c;
        }
    }

    let mut ofn: OpenFileNameW = unsafe { std::mem::zeroed() };
    ofn.struct_size = std::mem::size_of::<OpenFileNameW>() as u32;
    // Owned by the Studio window so the dialog opens on top of it
    ofn.hwnd_owner = STUDIO_HWND.load(std::sync::atomic::Ordering::SeqCst);
    ofn.filter = filter_w.as_ptr();
    ofn.filter_index = 1;
    ofn.file = file_buf.as_mut_ptr();
    ofn.max_file = file_buf.len() as u32;
    ofn.initial_dir = initial_dir_w.as_ref().map_or(std::ptr::null(), |d| d.as_ptr());
    ofn.title = title_w.as_ptr();
    let save = save_as.is_some();
    if save {
        ofn.def_ext = default_ext_w.as_ref().map_or(std::ptr::null(), |e| e.as_ptr());
    }
    ofn.flags = OFN_EXPLORER | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR | OFN_HIDEREADONLY
        | if save { OFN_OVERWRITEPROMPT } else { OFN_FILEMUSTEXIST };

    let hr = unsafe { CoInitializeEx(std::ptr::null_mut(), COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) };
    let picked = unsafe { if save { GetSaveFileNameW(&mut ofn) } else { GetOpenFileNameW(&mut ofn) } } != 0;
    if hr >= 0 {
        unsafe { CoUninitialize() };
    }
    if !picked {
        return None;
    }
    let len = file_buf.iter().position(|&c| c == 0).unwrap_or(file_buf.len());
    if len == 0 {
        return None;
    }
    Some(std::path::PathBuf::from(String::from_utf16_lossy(&file_buf[..len])))
}

#[cfg(not(target_os = "windows"))]
pub fn pick_file_dialog(title: &str, file_filter: &str) -> Option<std::path::PathBuf> {
    // 1. Try zenity first (standard on Linux/GNOME/KDE)
    let mut cmd = std::process::Command::new("zenity");
    cmd.arg("--file-selection").arg(format!("--title={}", title));
    if !file_filter.is_empty() {
        cmd.arg(format!("--file-filter={}", file_filter));
        cmd.arg("--file-filter=All Files | *");
    }
    if let Ok(output) = cmd.output() {
        if output.status.success() {
            let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !s.is_empty() {
                return Some(std::path::PathBuf::from(s));
            }
        }
    }

    // 2. Try kdialog as fallback
    let mut kcmd = std::process::Command::new("kdialog");
    kcmd.arg("--getopenfilename").arg(".").arg(format!("--title={}", title));
    if let Ok(output) = kcmd.output() {
        if output.status.success() {
            let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !s.is_empty() {
                return Some(std::path::PathBuf::from(s));
            }
        }
    }

    None
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileDialogTarget {
    Image,
    LaunchSound,
    CollisionSound,
    LuaEffectScript,
    CustomRedeemScript,
    WaterTexture,
}

pub struct FileDialogResult {
    pub target: FileDialogTarget,
    pub path: std::path::PathBuf,
}

/// What the threads behind Export and Import (they wait on a file dialog)
/// report back.
pub enum PackageEvent {
    Exported { file: std::path::PathBuf, result: Result<crate::redeem_package::ExportReport, String> },
    Opened { file: std::path::PathBuf, result: Result<crate::redeem_package::Package, String> },
}

pub fn load_all_redeems_from_disk() -> Vec<RedeemItem> {
    let dir = get_redeems_dir();
    let mut items = Vec::new();
    let mut seen_stems = std::collections::HashSet::new();

    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
                if ext == "json" || ext == "rdm" {
                    let stem = path.file_stem().unwrap_or_default().to_string_lossy().to_string();
                    if seen_stems.insert(stem) {
                        let filename = path.file_name().unwrap().to_string_lossy().to_string();
                        if let Ok(content) = std::fs::read_to_string(&path) {
                            let parsed = serde_json::from_str::<RedeemConfig>(&content);
                            if let Err(ref e) = parsed {
                                crate::obs_source::trust_log(&format!("[Trust Studio] ⚠️ Could not load redeem '{}': {}", path.display(), e));
                            }
                            if let Ok(mut config) = parsed {
                                // Automatically encode assets on loading if path is set but Base64 src is missing
                                if config.image_src.as_ref().map(|s| s.is_empty()).unwrap_or(true) {
                                    if let Some(ref p) = config.image_file_path {
                                        if let Some(b64) = resolve_and_encode_asset(p) {
                                            config.image_src = Some(b64);
                                        }
                                    }
                                }
                                if config.start_sound.as_ref().map(|s| s.is_empty()).unwrap_or(true) {
                                    if let Some(ref p) = config.start_sound_path {
                                        if let Some(b64) = resolve_and_encode_asset(p) {
                                            config.start_sound = Some(b64);
                                        }
                                    }
                                }
                                if config.collision_sound.as_ref().map(|s| s.is_empty()).unwrap_or(true) {
                                    if let Some(ref p) = config.collision_sound_path {
                                        if let Some(b64) = resolve_and_encode_asset(p) {
                                            config.collision_sound = Some(b64);
                                        }
                                    }
                                }
                                let triggers_input = config.triggers.as_ref().map(|t| t.join(", ")).unwrap_or_default();
                                items.push(RedeemItem {
                                    filename,
                                    path,
                                    config,
                                    is_dirty: false,
                                    triggers_input,
                                });
                            }
                        }
                    }
                }
            }
        }
    }
    items.sort_by(|a, b| a.filename.to_lowercase().cmp(&b.filename.to_lowercase()));
    crate::obs_source::trust_log(&format!("[Trust Studio] 🎁 Loaded {} redeem(s) from '{}'", items.len(), dir.display()));
    items
}

/// The triggers typed in the redeem editor ("a, b"), as saved.
fn parse_triggers(input: &str) -> Option<Vec<String>> {
    let parsed: Vec<String> = input
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if parsed.is_empty() { None } else { Some(parsed) }
}

pub fn save_redeem_to_disk(item: &mut RedeemItem) -> Result<(), String> {
    item.config.triggers = parse_triggers(&item.triggers_input);

    // Only throwables (image) may have impact sound
    if item.config.redeem_type != "image" && item.config.redeem_type != "throwable" {
        item.config.collision_sound = None;
        item.config.collision_sound_path = None;
    }

    // Contact sound on fluid (webgl-water)
    if item.config.redeem_type == "webgl-water" {
        if item.config.contact_sound.as_ref().map(|s| s.is_empty()).unwrap_or(true) {
            if let Some(ref p) = item.config.contact_sound_path {
                if let Some(b64) = resolve_and_encode_asset(p) {
                    item.config.contact_sound = Some(b64);
                }
            }
        }
    } else {
        item.config.contact_sound = None;
        item.config.contact_sound_path = None;
    }

    let json_val = serde_json::to_string_pretty(&item.config)
        .map_err(|e| format!("Serialization error: {}", e))?;
    if let Some(dir) = item.path.parent().filter(|d| !d.as_os_str().is_empty()) {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Err(e) = std::fs::write(&item.path, &json_val) {
        crate::obs_source::trust_log(&format!("[Trust Studio] ⚠️ Could not save redeem '{}' to '{}': {}", item.config.name, item.path.display(), e));
        return Err(format!("Disk write error: {}", e));
    }
    crate::obs_source::trust_log(&format!("[Trust Studio] 🎁 Saved redeem '{}' to '{}'", item.config.name, item.path.display()));

    // Also write companion .rdm or .json
    let parent = item.path.parent().unwrap_or(std::path::Path::new("redeems"));
    let stem = item.path.file_stem().unwrap_or_default().to_string_lossy();
    if item.path.extension().map(|e| e == "json").unwrap_or(false) {
        let rdm_path = parent.join(format!("{}.rdm", stem));
        let _ = std::fs::write(rdm_path, &json_val);
    } else if item.path.extension().map(|e| e == "rdm").unwrap_or(false) {
        let json_path = parent.join(format!("{}.json", stem));
        let _ = std::fs::write(json_path, &json_val);
    }

    item.is_dirty = false;
    reload_global_redeems();
    Ok(())
}

/// Changes a saved redeem's file (and its .json/.rdm twin) with `change`,
/// leaving everything else in it as it is — not whatever the editor holds
/// unsaved (a script rename updating the redeems that use it).
pub fn rewrite_redeem_file(path: &std::path::Path, change: impl Fn(&mut RedeemConfig)) -> Result<(), String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {}", path.display(), e))?;
    let mut config: RedeemConfig = serde_json::from_str(&text).map_err(|e| format!("{}: {}", path.display(), e))?;
    change(&mut config);
    let json = serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?;
    std::fs::write(path, &json).map_err(|e| format!("{}: {}", path.display(), e))?;
    let twin = match path.extension().and_then(|e| e.to_str()) {
        Some("json") => Some(path.with_extension("rdm")),
        Some("rdm") => Some(path.with_extension("json")),
        _ => None,
    };
    if let Some(twin) = twin.filter(|t| t.is_file()) {
        std::fs::write(&twin, &json).map_err(|e| format!("{}: {}", twin.display(), e))?;
    }
    Ok(())
}

/// A redeem as the editor's dropdown and the Scripts tab name it: its file,
/// and its name when that says more.
fn redeem_choice_label(item: &RedeemItem) -> String {
    let stem = item.path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let mut label = format!("{} {}", redeem_badge(&item.config.redeem_type), item.filename);
    let name = item.config.name.trim();
    if !name.is_empty() && !name.eq_ignore_ascii_case(&stem) {
        label.push_str(&format!(" — {}", name));
    }
    if item.is_dirty {
        label.push_str(" *");
    }
    label
}

/// A redeem's icon, by its type.
pub fn redeem_badge(redeem_type: &str) -> &'static str {
    match redeem_type {
        "webgl-water" => "🌊",
        "webgl-lightning" => "⚡",
        "webgl-fire" => "🔥",
        _ => "🎯",
    }
}

pub fn delete_redeem_file(path: &std::path::Path) -> Result<(), String> {
    if path.exists() {
        std::fs::remove_file(path).map_err(|e| format!("Failed to delete: {}", e))?;
    }
    let parent = path.parent().unwrap_or(std::path::Path::new("redeems"));
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    let comp_rdm = parent.join(format!("{}.rdm", stem));
    let comp_json = parent.join(format!("{}.json", stem));
    if comp_rdm.exists() && comp_rdm != path { let _ = std::fs::remove_file(comp_rdm); }
    if comp_json.exists() && comp_json != path { let _ = std::fs::remove_file(comp_json); }
    reload_global_redeems();
    Ok(())
}

pub struct StudioApp {
    selected_source_name: String,
    drag_mode: GizmoDragMode,
    drag_start_pointer: Pos2,
    drag_start_x: f32,
    drag_start_y: f32,
    drag_start_w: f32,
    drag_start_h: f32,
    drag_start_roll: f32,
    drag_start_pitch: f32,
    drag_start_yaw: f32,
    preview_texture: Option<egui::TextureHandle>,
    mask_textures: std::collections::HashMap<String, egui::TextureHandle>,
    last_target_frame_ids: std::collections::HashMap<String, u64>,
    show_hitboxes: bool,
    show_black_backdrop: bool,
    status_msg: String,
    target_fps: u32,
    /// When the Studio last drew (its refresh rate paces the next)
    last_frame_at: Option<std::time::Instant>,
    last_preview_frame_id: u64,
    frame_counter: u64,
    last_synced_scene: String,
    last_throwables_check_ms: u64,
    event_rx: Option<crossbeam_channel::Receiver<crate::types::StreamEvent>>,
    pub active_tab: StudioActiveTab,
    pub redeems_list: Vec<RedeemItem>,
    pub selected_redeem_index: Option<usize>,
    pub redeem_search_filter: String,
    pub active_redeem_subtab: RedeemSubTab,
    pub redeem_editor_status: String,
    pub lua_effects_list: Vec<LuaScriptEntry>,
    /// Lua effects removed from that list (see removed_lua_effects_path)
    removed_lua_effects: std::collections::BTreeSet<String>,
    pub selected_lua_effect_index: Option<usize>,
    pub custom_redeem_scripts_list: Vec<LuaScriptEntry>,
    pub selected_custom_redeem_index: Option<usize>,
    file_dialog_tx: crossbeam_channel::Sender<FileDialogResult>,
    file_dialog_rx: crossbeam_channel::Receiver<FileDialogResult>,
    package_tx: crossbeam_channel::Sender<PackageEvent>,
    package_rx: crossbeam_channel::Receiver<PackageEvent>,
    /// A package whose scripts make blocked calls, waiting on the Shield
    /// alert's answer
    pending_import: Option<(std::path::PathBuf, crate::redeem_package::Package)>,
    /// The Scripts tab's own state
    scripts: scripts_tab::ScriptsTab,
    settings: settings_tab::SettingsTab,
    source_filters_cache: std::collections::HashMap<String, Vec<crate::obs_source::SourceFilterInfo>>,
    last_filters_query_source: String,
    last_filters_query_time: u64,
    /// OBS properties of the selected source's filters, by filter name (None
    /// for a filter without any), read on a new source, every few seconds, or
    /// when an edit's own reaction changed them.
    filter_props: std::collections::HashMap<String, Option<Vec<crate::filter_props::Prop>>>,
    filter_props_source: String,
    filter_props_time: u64,
    filter_props_stale: bool,
    /// Selected source's mixer volume (None = source has no audio), re-read from OBS every 500 ms
    source_volume_cache: Option<f32>,
    last_volume_query_source: String,
    last_volume_query_time: u64,
    app_icon_texture: Option<egui::TextureHandle>,
    /// The pose being edited in 📍 Coordinate Sets — one at a time
    editing_pose: Option<PoseEditState>,
    /// Pose picked while the edited one has unsaved changes; the "save
    /// changes?" prompt is open until the user answers
    pending_pose_switch: Option<String>,
}

/// A pose selected for editing: the Studio's sources were moved to it, and
/// whatever changes after that is an unsaved edit of it.
struct PoseEditState {
    id: String,
    /// `last_applied_pose()` count once this pose was applied for editing
    applied_seq: u64,
    /// The sources once the pose finished applying (or were last saved) —
    /// None while they are still moving there
    baseline: Option<Vec<SourceTransformSnapshot>>,
    /// When sources were first seen at rest, before taking `baseline`
    settled_since_ms: Option<u64>,
    /// Sources differ from `baseline`
    dirty: bool,
    last_check_ms: u64,
}

impl PoseEditState {
    fn new(id: String, applied_seq: u64, baseline: Option<Vec<SourceTransformSnapshot>>) -> Self {
        Self { id, applied_seq, baseline, settled_since_ms: None, dirty: false, last_check_ms: 0 }
    }
}

/// Whether two snapshots of a scene's sources are the same pose (same sources
/// at the same place, size, rotation, visibility, filters and volume).
fn poses_match(a: &[SourceTransformSnapshot], b: &[SourceTransformSnapshot]) -> bool {
    let close = |x: f32, y: f32, tol: f32| (x - y).abs() <= tol;
    a.len() == b.len()
        && a.iter().all(|sa| {
            b.iter().find(|sb| sb.source_name == sa.source_name).is_some_and(|sb| {
                close(sa.x, sb.x, 0.5)
                    && close(sa.y, sb.y, 0.5)
                    && close(sa.width, sb.width, 0.5)
                    && close(sa.height, sb.height, 0.5)
                    && close(sa.roll, sb.roll, 0.1)
                    && close(sa.pitch, sb.pitch, 0.1)
                    && close(sa.yaw, sb.yaw, 0.1)
                    && sa.is_visible == sb.is_visible
                    && sa.filters == sb.filters
                    && match (sa.volume, sb.volume) {
                        (Some(va), Some(vb)) => close(va, vb, 1e-3),
                        (va, vb) => va.is_none() && vb.is_none(),
                    }
            })
        })
}

impl StudioApp {
    /// Follows the pose being edited: lets it go when it's gone (deleted,
    /// other scene) or a trigger / the pose dock applied another pose, and
    /// works out whether the sources still match it.
    fn update_editing_pose(&mut self, scene_name: &str, sources: &[crate::StudioSource], now_ms: u64) {
        let Some(edit) = self.editing_pose.as_mut() else { return; };
        let exists = SCENE_COORDINATE_SETS.read().iter()
            .any(|s| s.id == edit.id && (s.scene == scene_name || s.scene.is_empty()));
        let (seq, last_id) = last_applied_pose();
        if !exists || (seq != edit.applied_seq && last_id != edit.id) {
            self.editing_pose = None;
            return;
        }
        if seq != edit.applied_seq {
            // Applied again from outside: back to its saved state
            *edit = PoseEditState::new(edit.id.clone(), seq, None);
        }
        // Nothing to compare while sources are moving (pose move, Lua redeem)
        if ACTIVE_TRANSITION.lock().is_some() || crate::custom_lua::is_custom_script_active() {
            edit.settled_since_ms = None;
            return;
        }
        match &edit.baseline {
            None => {
                // Give OBS a moment to catch up with the last step of the move
                let since = *edit.settled_since_ms.get_or_insert(now_ms);
                if now_ms.saturating_sub(since) >= 300 {
                    edit.baseline = Some(snapshot_all_sources(sources));
                    edit.dirty = false;
                    edit.last_check_ms = now_ms;
                }
            }
            // Snapshots read each source's filters and volume from OBS, so not every frame
            Some(baseline) if now_ms.saturating_sub(edit.last_check_ms) >= 300 => {
                edit.dirty = !poses_match(baseline, &snapshot_all_sources(sources));
                edit.last_check_ms = now_ms;
            }
            Some(_) => {}
        }
    }

    /// Moves every source to pose `id` (with its own move) and edits it.
    fn start_editing_pose(&mut self, id: &str, sources: &[crate::StudioSource]) {
        let Some(set) = SCENE_COORDINATE_SETS.read().iter().find(|s| s.id == id).cloned() else { return; };
        let seq = start_scene_transition(&set, sources);
        self.editing_pose = Some(PoseEditState::new(set.id.clone(), seq, None));
        self.status_msg = format!("Editing '{}'", set.name);
    }

    /// Edits pose `id`, first asking about unsaved changes to the edited one.
    fn request_pose_switch(&mut self, id: String, sources: &[crate::StudioSource]) {
        // Checked now rather than trusting the last (up to 300 ms old) check
        let dirty = match self.editing_pose.as_ref() {
            Some(PoseEditState { baseline: Some(baseline), .. }) => !poses_match(baseline, &snapshot_all_sources(sources)),
            _ => false,
        };
        if dirty {
            self.pending_pose_switch = Some(id);
        } else {
            self.start_editing_pose(&id, sources);
        }
    }

    /// Stores the sources' current state into the pose being edited.
    fn save_editing_pose(&mut self, sources: &[crate::StudioSource]) -> bool {
        let Some(edit) = self.editing_pose.as_mut() else { return false; };
        let snapshot = snapshot_all_sources(sources);
        let mut sets = SCENE_COORDINATE_SETS.write();
        let Some(set) = sets.iter_mut().find(|s| s.id == edit.id) else { return false; };
        set.sources = snapshot.clone();
        crate::obs_source::trust_log(&format!(
            "[Trust Studio] 💾 Re-saved pose '{}' for scene '{}' ({} sources)",
            set.name, set.scene, set.sources.len()
        ));
        self.status_msg = format!("Saved coordinates for all sources to '{}'", set.name);
        edit.baseline = Some(snapshot);
        edit.dirty = false;
        true
    }
}

/// A discovered or user-browsed `.lua` script — either an avatar-reaction
/// "effect" filter (attachable via the Lua Effects section) or a fully custom
/// redeem-type script (the Custom Redeem Scripts section). Loading/attaching
/// these is not wired up yet — this is the Explorer-side bookkeeping only.
#[derive(Clone)]
pub struct LuaScriptEntry {
    pub name: String,
    pub path: std::path::PathBuf,
}

/// Lists every `.lua` file directly inside `dir` (non-recursive), sorted by name.
fn scan_lua_scripts_dir(dir: &std::path::Path) -> Vec<LuaScriptEntry> {
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()).map(|e| e.eq_ignore_ascii_case("lua")).unwrap_or(false) {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    out.push(LuaScriptEntry { name: stem.to_string(), path });
                }
            }
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// Where the Lua effects removed from the Studio's list are remembered, by
/// file name: the list leaves them out, their files stay in filters/.
fn removed_lua_effects_path() -> std::path::PathBuf {
    crate::get_theater_repo_dir().join("theater_removed_effects.json")
}

fn load_removed_lua_effects() -> std::collections::BTreeSet<String> {
    std::fs::read_to_string(removed_lua_effects_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_removed_lua_effects(removed: &std::collections::BTreeSet<String>) {
    let path = removed_lua_effects_path();
    let written = serde_json::to_string_pretty(removed)
        .map_err(std::io::Error::other)
        .and_then(|json| std::fs::write(&path, json));
    if let Err(e) = written {
        crate::obs_source::trust_log(&format!("[Trust Studio] ⚠️ Could not save '{}': {}", path.display(), e));
    }
}

fn effect_file_name(path: &std::path::Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

/// The Lua effects in filters/, except the ones removed from the list.
fn lua_effects_in_filters(removed: &std::collections::BTreeSet<String>) -> Vec<LuaScriptEntry> {
    scan_lua_scripts_dir(&get_filters_dir())
        .into_iter()
        .filter(|e| !removed.contains(&effect_file_name(&e.path)))
        .collect()
}

/// Scans workspace and user scripts directories for custom redeem scripts.
pub fn scan_workspace_custom_scripts() -> Vec<LuaScriptEntry> {
    let mut out = Vec::new();
    let home = std::env::var("HOME").ok().map(std::path::PathBuf::from);
    let repo = crate::get_theater_repo_dir();
    let mut dirs = vec![
        std::path::PathBuf::from("."),
        std::path::PathBuf::from("scripts"),
        repo.clone(),
        repo.join("scripts"),
    ];
    if let Some(ref home) = home {
        dirs.push(home.join("repo/theater-scene"));
        dirs.push(home.join("repo/theater-scene/scripts"));
        dirs.push(home.join("scripts"));
    }
    for dir in dirs {
        if dir.is_dir() {
            for entry in scan_lua_scripts_dir(&dir) {
                if entry.name.eq_ignore_ascii_case("theater_chat") {
                    continue;
                }
                if !out.iter().any(|e: &LuaScriptEntry| e.path == entry.path) {
                    out.push(entry);
                }
            }
        }
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum MovementEasing {
    EaseOut,
    EaseIn,
    EaseInOut,
    Linear,
    Bounce,
    Instant,
}

impl Default for MovementEasing {
    fn default() -> Self {
        MovementEasing::EaseOut
    }
}

impl MovementEasing {
    pub fn name(&self) -> &'static str {
        match self {
            MovementEasing::EaseOut => "Ease Out",
            MovementEasing::EaseIn => "Ease In",
            MovementEasing::EaseInOut => "Ease In-Out",
            MovementEasing::Linear => "Linear",
            MovementEasing::Bounce => "Bounce",
            MovementEasing::Instant => "Instant",
        }
    }

    pub fn evaluate(&self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            MovementEasing::Instant => 1.0,
            MovementEasing::Linear => t,
            MovementEasing::EaseIn => t * t * t,
            MovementEasing::EaseOut => {
                let inv = 1.0 - t;
                1.0 - inv * inv * inv
            }
            MovementEasing::EaseInOut => {
                if t < 0.5 {
                    4.0 * t * t * t
                } else {
                    let f = (2.0 * t) - 2.0;
                    0.5 * f * f * f + 1.0
                }
            }
            MovementEasing::Bounce => {
                if t < 0.36364 {
                    7.5625 * t * t
                } else if t < 0.72727 {
                    let t2 = t - 0.54545;
                    7.5625 * t2 * t2 + 0.75
                } else if t < 0.90909 {
                    let t2 = t - 0.81818;
                    7.5625 * t2 * t2 + 0.9375
                } else {
                    let t2 = t - 0.95455;
                    7.5625 * t2 * t2 + 0.984375
                }
            }
        }
    }
}

/// Pose numbers saved as `null` — serde_json writes NaN/±inf that way — read
/// back as 0 instead of failing, which would drop the whole pose on load.
fn f32_or_zero<'de, D: serde::Deserializer<'de>>(d: D) -> Result<f32, D::Error> {
    Ok(<Option<f32> as serde::Deserialize>::deserialize(d)?.unwrap_or(0.0))
}

/// Never let a NaN/±inf into a saved pose (it would be written as `null`).
fn finite_or(value: f32, fallback: f32) -> f32 {
    if value.is_finite() { value } else { fallback }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct SourceTransformSnapshot {
    pub source_name: String,
    #[serde(deserialize_with = "f32_or_zero")]
    pub x: f32,
    #[serde(deserialize_with = "f32_or_zero")]
    pub y: f32,
    #[serde(deserialize_with = "f32_or_zero")]
    pub width: f32,
    #[serde(deserialize_with = "f32_or_zero")]
    pub height: f32,
    #[serde(deserialize_with = "f32_or_zero")]
    pub roll: f32,
    #[serde(deserialize_with = "f32_or_zero")]
    pub pitch: f32,
    #[serde(deserialize_with = "f32_or_zero")]
    pub yaw: f32,
    pub is_visible: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filters: Option<std::collections::HashMap<String, serde_json::Value>>,
    /// Mixer volume (linear multiplier, 1.0 = 0 dB) of an audio source; None for video-only sources.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volume: Option<f32>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct SceneCoordinateSet {
    pub id: String,
    pub name: String,
    pub trigger: String,
    pub scene: String,
    #[serde(default)]
    pub easing: MovementEasing,
    #[serde(default = "default_duration_ms")]
    pub duration_ms: u32,
    pub sources: Vec<SourceTransformSnapshot>,
}

fn default_duration_ms() -> u32 {
    800
}

#[derive(Clone)]
pub struct ActiveSourceTransition {
    pub start_time_ms: u64,
    pub duration_ms: u32,
    pub easing: MovementEasing,
    pub start_sources: std::collections::HashMap<String, SourceTransformSnapshot>,
    pub target_sources: std::collections::HashMap<String, SourceTransformSnapshot>,
}

pub static SCENE_COORDINATE_SETS: std::sync::LazyLock<parking_lot::RwLock<Vec<SceneCoordinateSet>>> =
    std::sync::LazyLock::new(|| parking_lot::RwLock::new(Vec::new()));

pub static LAST_SENT_POSE: std::sync::LazyLock<parking_lot::RwLock<std::collections::HashMap<String, SourceTransformSnapshot>>> =
    std::sync::LazyLock::new(|| parking_lot::RwLock::new(std::collections::HashMap::new()));

pub fn record_last_sent_pose(set: &SceneCoordinateSet) {
    let mut map = LAST_SENT_POSE.write();
    for s in &set.sources {
        map.insert(s.source_name.clone(), s.clone());
    }
}

pub fn update_last_sent_pose_for_source(
    name: &str,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    roll: f32,
    pitch: f32,
    yaw: f32,
    is_visible: bool,
) {
    let mut map = LAST_SENT_POSE.write();
    map.insert(name.to_string(), SourceTransformSnapshot {
        source_name: name.to_string(),
        x,
        y,
        width,
        height,
        roll,
        pitch,
        yaw,
        is_visible,
        filters: None,
        volume: None,
    });
}

pub fn get_last_active_pose_id() -> Option<String> {
    let path = crate::get_theater_repo_dir().join("theater_last_pose.txt");
    if let Ok(s) = std::fs::read_to_string(path) {
        let trimmed = s.trim().to_string();
        if !trimmed.is_empty() {
            return Some(trimmed);
        }
    }
    None
}

pub fn save_last_active_pose_id(id: &str) {
    let path = crate::get_theater_repo_dir().join("theater_last_pose.txt");
    let _ = std::fs::write(path, id);
}

pub fn load_and_apply_last_pose_on_load(scene_name: &str) {
    let sets_lock = SCENE_COORDINATE_SETS.read();
    let last_pose_id = get_last_active_pose_id();
    let in_scene = |s: &&SceneCoordinateSet| s.scene == scene_name || s.scene.is_empty();
    let matching_set = last_pose_id.as_ref()
        // Only restore the last pose if it belongs to this scene
        .and_then(|id| sets_lock.iter().filter(in_scene).find(|s| &s.id == id || &s.name == id))
        .or_else(|| {
            // Fallback: take the last coordinate set for this scene
            sets_lock.iter().filter(|s| s.scene == scene_name || s.scene.is_empty()).last()
        })
        .cloned();
    drop(sets_lock);

    if let Some(set) = matching_set {
        println!("[Trust Studio] 📍 Applying last pose on load: '{}' (id: '{}')", set.name, set.id);
        record_last_sent_pose(&set);
        save_last_active_pose_id(&set.id);

        let state = get_state();
        let mut sources = state.studio_sources.write();
        let mut updated = Vec::new();
        for target in &set.sources {
            if let Some(s) = sources.iter_mut().find(|s| s.name.eq_ignore_ascii_case(&target.source_name)) {
                s.x = target.x;
                s.y = target.y;
                if target.width > 0.0 { s.width = target.width; }
                if target.height > 0.0 { s.height = target.height; }
                s.roll = target.roll;
                s.pitch = target.pitch;
                s.yaw = target.yaw;
                s.is_visible = target.is_visible;
                updated.push(s.clone());
            }
        }
        drop(sources);

        for src in updated {
            crate::queue_studio_transform_update(&src);
        }
    }
}

pub static ACTIVE_TRANSITION: std::sync::LazyLock<parking_lot::Mutex<Option<ActiveSourceTransition>>> =
    std::sync::LazyLock::new(|| parking_lot::Mutex::new(None));

// Non-blocking gate around actually *applying* an active transition (calling
// into libobs to update scene-item transforms and filter settings). An
// active transition is ticked from two independent threads at once — the
// video-io thread (`tick_active_transition_standalone`, via the source's
// video_tick callback) and the Studio UI's own render loop
// (`tick_active_transition`) — and both can call real libobs functions for
// the same sources concurrently for the whole transition duration. A
// blocking lock here would just force one thread to wait on the other
// inside libobs's own internals (the same mistake as before), so instead
// whichever thread loses the race simply skips that tick; the next tick
// (a few milliseconds later, well within a transition) picks it up.
static TRANSITION_APPLY_LOCK: parking_lot::Mutex<()> = parking_lot::Mutex::new(());

pub static GLOBAL_REDEEMS: std::sync::LazyLock<parking_lot::RwLock<Vec<RedeemItem>>> =
    std::sync::LazyLock::new(|| parking_lot::RwLock::new(Vec::new()));

pub static LAST_TRIGGER_STATUS: std::sync::LazyLock<parking_lot::RwLock<Option<String>>> =
    std::sync::LazyLock::new(|| parking_lot::RwLock::new(None));

/// Shows `status` in the Studio's status bar (if it's open).
pub fn show_trigger_status(status: String) {
    *LAST_TRIGGER_STATUS.write() = Some(status);
    if let Some(ref ctx) = *STUDIO_EGUI_CTX.lock() {
        ctx.request_repaint();
    }
}

pub fn reload_global_redeems() -> usize {
    let mut list = load_all_redeems_from_disk();

    let repo_tj = crate::get_theater_repo_dir().join("theater.json");
    let theater_json_candidates = [
        repo_tj,
        std::path::PathBuf::from("theater.json"),
        std::path::PathBuf::from("./theater.json"),
        std::path::PathBuf::from("/home/mortygotvr/repo/theater-scene/theater.json"),
    ];

    for tj_path in &theater_json_candidates {
        if tj_path.exists() {
            if let Ok(content) = std::fs::read_to_string(tj_path) {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(trigs) = val.get("triggers").and_then(|t| t.as_array()) {
                        let objects_map: std::collections::HashMap<String, serde_json::Value> = val.get("objects")
                            .and_then(|o| o.as_array())
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(|obj| {
                                        obj.get("id")
                                            .and_then(|id| id.as_str())
                                            .map(|id_str| (id_str.to_string(), obj.clone()))
                                    })
                                    .collect()
                            })
                            .unwrap_or_default();

                        for trig in trigs {
                            let events_str = trig.get("events").and_then(|e| e.as_str()).unwrap_or("");
                            if events_str.is_empty() {
                                continue;
                            }
                            let parsed_events: Vec<String> = events_str
                                .split(',')
                                .map(|s| s.trim().to_string())
                                .filter(|s| !s.is_empty())
                                .collect();
                            if parsed_events.is_empty() {
                                continue;
                            }

                            let obj_id = trig.get("objectId").and_then(|o| o.as_str()).unwrap_or("");
                            let linked_obj = objects_map.get(obj_id);
                            let obj_name = linked_obj
                                .and_then(|o| o.get("name"))
                                .and_then(|n| n.as_str())
                                .unwrap_or("");

                            let mut merged = false;
                            for existing in &mut list {
                                let name_match = !obj_name.is_empty()
                                    && existing.config.name.eq_ignore_ascii_case(obj_name);
                                let stem_match = !obj_name.is_empty()
                                    && existing.filename.to_lowercase().starts_with(&obj_name.to_lowercase());
                                if name_match || stem_match {
                                    let mut current_trigs = existing.config.triggers.clone().unwrap_or_default();
                                    for ev in &parsed_events {
                                        if !current_trigs.iter().any(|t| t.eq_ignore_ascii_case(ev)) {
                                            current_trigs.push(ev.clone());
                                        }
                                    }
                                    existing.config.triggers = Some(current_trigs);
                                    merged = true;
                                    break;
                                }
                            }

                            if !merged {
                                if let Some(obj) = linked_obj {
                                    if let Ok(mut cfg) = serde_json::from_value::<RedeemConfig>(obj.clone()) {
                                        cfg.triggers = Some(parsed_events);
                                        if let Some(amt) = trig.get("amount") {
                                            cfg.amount = Some(amt.clone());
                                        }
                                        if let Some(tgt) = trig.get("target").and_then(|t| t.as_str()) {
                                            cfg.target = Some(tgt.to_string());
                                        }
                                        if let Some(rep) = trig.get("repeatDelay").and_then(|r| r.as_u64()) {
                                            cfg.repeat_delay = Some(rep);
                                        }
                                        list.push(RedeemItem {
                                            filename: format!("{}.json", obj_name.to_lowercase()),
                                            path: std::path::PathBuf::from(format!("theater_{}.json", obj_name)),
                                            config: cfg,
                                            is_dirty: false,
                                            triggers_input: events_str.to_string(),
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }
            break;
        }
    }

    let count = list.len();
    *GLOBAL_REDEEMS.write() = list;
    println!("[Trust Core] 📦 Loaded {} redeems into global trigger cache.", count);
    crate::obs_procs::notify_list_changed();
    count
}

/// Theater Reader's chat event names (see Theater-reader/main.py). Super Chats
/// are paid events, not chat: they aren't listed.
const READER_CHAT_TRIGGERS: [&str; 4] = ["Twitch chat", "Kick chat", "chat_message", "Twitch whisper"];

/// A chat or whisper message rather than an event.
pub fn is_chat_event(event: &crate::types::StreamEvent) -> bool {
    let event_type = event.event_type.trim();
    event_type.eq_ignore_ascii_case("chat")
        || event_type.eq_ignore_ascii_case("whisper")
        || READER_CHAT_TRIGGERS.iter().any(|t| event.trigger.trim().eq_ignore_ascii_case(t))
}

/// Fires the poses (Scene Coordinate Sets) and redeems whose trigger pattern
/// matches a Theater Reader event.
///
/// THERE ARE NO CHAT TRIGGERS — by design, and don't add any. Triggers are
/// Theater Reader *events* (channel point redeems, subs, bits, follows, raids,
/// Super Chats…), matched against the event's trigger string, e.g.
/// "Twitch Mortygotvr redeem Throw Tomato". Chat and whisper messages never
/// fire anything (they only feed the chat overlay), and an event's message text
/// (a redeem's typed input, a cheer message) is never matched either:
/// otherwise any viewer could fire a pose or redeem by typing its pattern.
pub fn process_trigger_event(event: &crate::types::StreamEvent) {
    if is_chat_event(event) {
        return;
    }
    let ev_trig = event.trigger.trim();
    if ev_trig.is_empty() {
        return;
    }

    let state = get_state();

    // 1. Scene Coordinate Sets (Camera / Layout transitions)
    let cur_scene = state.studio_scene_name.read().clone();
    let mut sources = state.studio_sources.read().clone();
    if sources.is_empty() {
        drop(sources);
        load_studio_targets_from_disk();
        sources = state.studio_sources.read().clone();
    }
    let sets_lock = SCENE_COORDINATE_SETS.read();
    let matching_set = sets_lock.iter().find(|set| {
        if set.scene != cur_scene && !set.scene.is_empty() {
            return false;
        }
        let t = set.trigger.trim();
        if t.is_empty() {
            return false;
        }
        match_trigger_wildcard(t, ev_trig)
    }).cloned();
    drop(sets_lock);

    if let Some(set) = matching_set {
        println!("[Trust Trigger Engine] 🎯 Matched coordinate set '{}' ('{}')", set.name, set.trigger);
        start_scene_transition(&set, &sources);
        *LAST_TRIGGER_STATUS.write() = Some(format!("Triggered: {}", set.name));
        if let Some(ref ctx) = *STUDIO_EGUI_CTX.lock() {
            ctx.request_repaint();
        }
    }

    // 2. Redeems (Throwable, Fluid, Fire, Lightning, Audio)
    let redeems_guard = GLOBAL_REDEEMS.read();
    let redeems_snapshot: Vec<RedeemItem> = if redeems_guard.is_empty() {
        drop(redeems_guard);
        reload_global_redeems();
        GLOBAL_REDEEMS.read().clone()
    } else {
        redeems_guard.clone()
    };

    for r_item in &redeems_snapshot {
        if let Some(trigs) = &r_item.config.triggers {
            for pat in trigs {
                if let Some(wcards) = match_and_extract_wildcards(pat, ev_trig) {
                    println!(
                        "[Trust Trigger Engine] 🎯 Matched redeem '{}' (pattern '{}') with wildcards: {:?}",
                        r_item.config.name, pat, wcards
                    );
                    let mut cfg = r_item.config.clone();
                    cfg.wildcards = Some(wcards);
                    if let Some(json_str) = build_redeem_dispatch_json(&r_item.filename, &cfg) {
                        crate::studio_app::dispatch_test_redeem_json(&json_str);
                    }
                    *LAST_TRIGGER_STATUS.write() = Some(format!("🚀 Redeem: {}", r_item.config.name));
                    if let Some(ref ctx) = *STUDIO_EGUI_CTX.lock() {
                        ctx.request_repaint();
                    }
                    break;
                }
            }
        }
    }
}

/// Runtime state a filter script keeps in its own settings, rewritten every
/// frame — Theatre 360's Master broadcasts its camera to the Layers there as
/// `_live_pan`, `_live_tilt`, `_live_fov`, `_has_live`. It is OBS's live
/// output, not a setting: the Studio never shows it, stores it in a pose or
/// writes it back (a stale copy of it made every pose look edited and was
/// written over the Master's live values by every pose move).
fn is_runtime_filter_key(key: &str) -> bool {
    key.starts_with('_')
}

/// A filter's settings without its runtime keys.
fn without_runtime_keys(settings: serde_json::Value) -> serde_json::Value {
    match settings {
        serde_json::Value::Object(mut map) => {
            map.retain(|k, _| !is_runtime_filter_key(k));
            serde_json::Value::Object(map)
        }
        other => other,
    }
}

/// A source's OBS filters as a pose stores them (id, enabled, settings), or
/// None when it has none.
fn pose_filters(source_name: &str) -> Option<std::collections::HashMap<String, serde_json::Value>> {
    let flts = crate::obs_source::query_source_filters(source_name);
    if flts.is_empty() {
        return None;
    }
    Some(flts.into_iter().map(|f| (f.name, serde_json::json!({
        "id": f.id,
        "enabled": f.enabled,
        "settings": without_runtime_keys(f.settings),
    }))).collect())
}

/// Where a Theatre 360 Master's camera really is, as a pose move's starting
/// point. `settings` are its raw settings (runtime keys included);
/// `target_rotating`: whether the pose leaves it rotating. Its pan, tilt and
/// FOV settings lag behind the view (smoothing) and leave the rotation's angle
/// out: a move started from them jumped — instantly to the pose when the pan
/// setting already matched the pose's (intro → Game). Unless it rotates both
/// before and after, the move starts from the live pan the Master publishes
/// (when the rotation stops, the Master keeps that view, and the move carries
/// on from it). While it keeps rotating, the rotation stays on top of the pan
/// setting, so that setting is where the pan starts.
fn live_360_start(settings: &mut serde_json::Value, target_rotating: Option<bool>) {
    let Some(map) = settings.as_object_mut() else { return };
    if map.get("_has_live").and_then(|v| v.as_bool()) != Some(true) {
        return;
    }
    let rotating = map.get("enable_rotation").and_then(|v| v.as_bool()).unwrap_or(false);
    let keeps_rotating = rotating && target_rotating.unwrap_or(rotating);
    for (key, live) in [("tilt", "_live_tilt"), ("fov", "_live_fov"), ("pan", "_live_pan")] {
        if key == "pan" && keeps_rotating {
            continue;
        }
        if let Some(value) = map.get(live).cloned() {
            map.insert(key.to_string(), value);
        }
    }
}

/// A source's filters as a pose move starts from them: as pose_filters, with a
/// 360 Master's camera where it really is (live_360_start).
fn transition_start_filters(
    source_name: &str,
    target: Option<&SourceTransformSnapshot>,
) -> Option<std::collections::HashMap<String, serde_json::Value>> {
    let flts = crate::obs_source::query_source_filters(source_name);
    if flts.is_empty() {
        return None;
    }
    Some(flts.into_iter().map(|f| {
        let mut settings = f.settings;
        if f.id == THEATRE_360_MASTER_FILTER {
            let target_rotating = target
                .and_then(|t| t.filters.as_ref())
                .and_then(|m| m.get(&f.name))
                .and_then(|t| t.pointer("/settings/enable_rotation"))
                .and_then(|v| v.as_bool());
            live_360_start(&mut settings, target_rotating);
        }
        (f.name, serde_json::json!({
            "id": f.id,
            "enabled": f.enabled,
            "settings": without_runtime_keys(settings),
        }))
    }).collect())
}

/// Theatre 360 settings that are a direction around the sphere (-180..180°),
/// eased the short way round like the Master's own smoothing: 170° → -170° is
/// a 20° turn, not a 340° spin back through 0°.
fn is_360_yaw(filter_id: &str, key: &str) -> bool {
    filter_id.contains("360") && matches!(key, "pan" | "rot_pan" | "m_pan" | "x_offset")
}

/// This tick's writes to one filter of a pose move: numeric settings eased
/// from where they were when the move began, other settings set right away
/// (a filter that wasn't there when it began gets the pose's settings at the
/// end). Settings the pose leaves as they were aren't written at all, so a
/// move never undoes a change made meanwhile in OBS (or by a filter script)
/// to a setting the pose doesn't change; runtime keys are never written.
fn filter_settings_step(
    filter_id: &str,
    start: Option<&serde_json::Map<String, serde_json::Value>>,
    target: &serde_json::Map<String, serde_json::Value>,
    progress: f32,
) -> serde_json::Map<String, serde_json::Value> {
    let p = progress as f64;
    let mut writes = serde_json::Map::new();
    if start.is_none() && progress < 1.0 {
        return writes;
    }
    for (key, t_v) in target {
        if is_runtime_filter_key(key) {
            continue;
        }
        let s_v = start.and_then(|m| m.get(key));
        if s_v == Some(t_v) {
            continue;
        }
        let (Some(s), Some(t)) = (s_v.and_then(|v| v.as_f64()), t_v.as_f64()) else {
            writes.insert(key.clone(), t_v.clone());
            continue;
        };
        let value = if progress >= 1.0 {
            t_v.clone()
        } else if is_360_yaw(filter_id, key) {
            let turn = (t - s + 180.0).rem_euclid(360.0) - 180.0;
            serde_json::json!((s + turn * p + 180.0).rem_euclid(360.0) - 180.0)
        } else if s_v.is_some_and(|v| v.is_i64() || v.is_u64()) && (t_v.is_i64() || t_v.is_u64()) {
            // Whole-number settings (crop edges, sizes) stay whole numbers
            serde_json::json!((s + (t - s) * p).round() as i64)
        } else {
            serde_json::json!(s + (t - s) * p)
        };
        writes.insert(key.clone(), value);
    }
    writes
}

/// How often the Studio's filter list re-reads the selected source's filters from OBS.
const FILTER_LIST_REFRESH_MS: u64 = 500;

/// How often it re-reads their OBS properties (dropdown choices can change:
/// a source added in OBS shows up in a 360 Layer's "Select Master Viewer").
const FILTER_PROPS_REFRESH_MS: u64 = 5000;

/// Whether a filter setting's value is this dropdown choice (numbers compared as numbers).
fn list_value_is(value: Option<&serde_json::Value>, choice: &serde_json::Value) -> bool {
    match (value, choice.as_f64()) {
        (Some(v), Some(c)) => v.as_f64().is_some_and(|v| (v - c).abs() < 1e-9),
        (Some(v), None) => v == choice,
        (None, _) => false,
    }
}

/// The Studio's controls for a filter's OBS properties, laid out as its OBS
/// Filters window lays them out; (setting, new value) of the one the user
/// changed. As with the raw settings, showing a value never alters it: no
/// clamping to a range or snapping to a step until the user edits it.
fn filter_props_ui(
    ui: &mut egui::Ui,
    filter_name: &str,
    props: &[crate::filter_props::Prop],
    settings: &serde_json::Value,
) -> Option<(String, serde_json::Value)> {
    use crate::filter_props::Kind;
    let mut edited = None;
    for prop in props {
        let value = settings.get(&prop.name);
        let changed = ui.add_enabled_ui(prop.enabled, |ui| -> Option<serde_json::Value> {
            match &prop.kind {
                Kind::Bool => {
                    let mut b = value.and_then(|v| v.as_bool()).unwrap_or(false);
                    ui.checkbox(&mut b, &prop.label).changed().then(|| serde_json::json!(b))
                }
                Kind::Int { min, max, step, slider } => ui.horizontal(|ui| {
                    ui.label(&prop.label);
                    let mut v = value
                        .and_then(|v| v.as_i64().or_else(|| v.as_f64().map(|f| f.round() as i64)))
                        .unwrap_or(*min);
                    let changed = if *slider {
                        ui.add(egui::Slider::new(&mut v, *min..=*max).step_by(*step as f64).clamping(egui::SliderClamping::Edits)).changed()
                    } else {
                        ui.add(egui::DragValue::new(&mut v).range(*min..=*max).clamp_existing_to_range(false).speed(*step as f64)).changed()
                    };
                    changed.then(|| serde_json::json!(v))
                }).inner,
                Kind::Float { min, max, step, slider } => ui.horizontal(|ui| {
                    ui.label(&prop.label);
                    let mut v = value.and_then(|v| v.as_f64()).unwrap_or(*min);
                    let step = if *step > 0.0 { *step } else { 0.01 };
                    let changed = if *slider {
                        ui.add(egui::Slider::new(&mut v, *min..=*max).step_by(step).clamping(egui::SliderClamping::Edits)).changed()
                    } else {
                        ui.add(egui::DragValue::new(&mut v).range(*min..=*max).clamp_existing_to_range(false).speed(step)).changed()
                    };
                    changed.then(|| serde_json::json!(v))
                }).inner,
                Kind::Text { info: true, .. } => {
                    let text = value.and_then(|v| v.as_str()).unwrap_or("");
                    ui.label(if text.is_empty() { prop.label.clone() } else { format!("{}: {}", prop.label, text) });
                    None
                }
                Kind::Text { password, multiline, .. } => ui.horizontal(|ui| {
                    ui.label(&prop.label);
                    let mut s = value.and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let edit = if *multiline {
                        egui::TextEdit::multiline(&mut s)
                    } else {
                        egui::TextEdit::singleline(&mut s).password(*password)
                    };
                    ui.add(edit).changed().then(|| serde_json::json!(s))
                }).inner,
                Kind::Path => ui.horizontal(|ui| {
                    ui.label(&prop.label);
                    let mut s = value.and_then(|v| v.as_str()).unwrap_or("").to_string();
                    ui.text_edit_singleline(&mut s).changed().then(|| serde_json::json!(s))
                }).inner,
                Kind::List { items, .. } => ui.horizontal(|ui| {
                    ui.label(&prop.label);
                    let shown = items
                        .iter()
                        .find(|i| list_value_is(value, &i.value))
                        .map(|i| i.label.clone())
                        .unwrap_or_else(|| match value {
                            Some(serde_json::Value::String(s)) => s.clone(),
                            Some(v) => v.to_string(),
                            None => String::new(),
                        });
                    let mut picked = None;
                    egui::ComboBox::from_id_salt(("filter_prop", filter_name, &prop.name))
                        .selected_text(shown)
                        .show_ui(ui, |ui| {
                            for item in items {
                                let selected = list_value_is(value, &item.value);
                                if ui.add_enabled(!item.disabled, egui::SelectableLabel::new(selected, &item.label)).clicked() && !selected {
                                    picked = Some(item.value.clone());
                                }
                            }
                        });
                    picked
                }).inner,
                Kind::Color { alpha } => ui.horizontal(|ui| {
                    ui.label(&prop.label);
                    let raw = value
                        .and_then(|v| v.as_u64().or_else(|| v.as_i64().map(|i| i as u32 as u64)))
                        .unwrap_or(0xFFFF_FFFF);
                    let mut color = if *alpha {
                        crate::filter_props::color_from_obs(raw)
                    } else {
                        let v = raw as u32;
                        Color32::from_rgb(v as u8, (v >> 8) as u8, (v >> 16) as u8)
                    };
                    let mode = if *alpha { egui::color_picker::Alpha::OnlyBlend } else { egui::color_picker::Alpha::Opaque };
                    egui::color_picker::color_edit_button_srgba(ui, &mut color, mode)
                        .changed()
                        .then(|| serde_json::json!(crate::filter_props::color_to_obs(color, *alpha)))
                }).inner,
                Kind::Group { checkable, children } => {
                    let mut group_edit = None;
                    ui.group(|ui| {
                        if *checkable {
                            let mut b = value.and_then(|v| v.as_bool()).unwrap_or(false);
                            if ui.checkbox(&mut b, egui::RichText::new(&prop.label).strong()).changed() {
                                group_edit = Some((prop.name.clone(), serde_json::json!(b)));
                            }
                        } else {
                            ui.label(egui::RichText::new(&prop.label).strong());
                        }
                        if let Some(child_edit) = filter_props_ui(ui, filter_name, children, settings) {
                            group_edit = Some(child_edit);
                        }
                    });
                    if group_edit.is_some() {
                        edited = group_edit;
                    }
                    None
                }
                Kind::Other => {
                    ui.label(
                        egui::RichText::new(format!("{} — set in OBS's Filters window", prop.label))
                            .small()
                            .color(Color32::from_rgb(148, 163, 184)),
                    );
                    None
                }
            }
        }).inner;
        if let Some(v) = changed {
            edited = Some((prop.name.clone(), v));
        }
    }
    edited
}

const THEATRE_360_MASTER_FILTER: &str = "theatre_360_master_filter";
const THEATRE_360_LAYER_FILTER: &str = "theatre_360_layer_filter";

/// The Master setting a Theatre 360 Layer's "Remote Master Control" setting
/// drives (OBS's Layer properties forward them to the Master's filter).
fn layer_remote_master_key(filter_id: &str, key: &str) -> Option<&'static str> {
    if filter_id != THEATRE_360_LAYER_FILTER {
        return None;
    }
    Some(match key {
        "m_pan" => "pan",
        "m_tilt" => "tilt",
        "m_fov" => "fov",
        "enable_rotation" => "enable_rotation",
        "rot_pan" => "rot_pan",
        "rot_speed" => "rot_speed",
        _ => return None,
    })
}

/// The Theatre 360 Master filter on `source_name`, if it has one.
fn theatre_360_master(source_name: &str) -> Option<crate::obs_source::SourceFilterInfo> {
    crate::obs_source::query_source_filters(source_name)
        .into_iter()
        .find(|f| f.id == THEATRE_360_MASTER_FILTER)
}

/// The selected source's filters for the Studio's filter list, runtime keys
/// left out. A Theatre 360 Layer's "Remote Master Control" settings show its
/// Master's current values (the Layer's own copies only catch up when its
/// Master is picked again in OBS).
fn studio_filter_list(source_name: &str) -> Vec<crate::obs_source::SourceFilterInfo> {
    let mut flts = crate::obs_source::query_source_filters(source_name);
    let mut masters: std::collections::HashMap<String, Option<serde_json::Value>> = std::collections::HashMap::new();
    for flt in &mut flts {
        flt.settings = without_runtime_keys(std::mem::take(&mut flt.settings));
        if flt.id != THEATRE_360_LAYER_FILTER {
            continue;
        }
        let Some(master_source) = flt.settings.get("master_source").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) else {
            continue;
        };
        let master = masters
            .entry(master_source.to_string())
            .or_insert_with_key(|name| theatre_360_master(name).map(|m| m.settings));
        let (Some(master), Some(settings)) = (master.as_ref(), flt.settings.as_object_mut()) else { continue };
        for (key, value) in settings.iter_mut() {
            if let Some(master_value) = layer_remote_master_key(THEATRE_360_LAYER_FILTER, key).and_then(|k| master.get(k)) {
                *value = master_value.clone();
            }
        }
    }
    flts
}

fn filter_setting_label(key: &str) -> &str {
    match key {
        "pan" => "🔄 Pan (Yaw):",
        "tilt" => "↕️ Tilt (Pitch):",
        "fov" => "🔍 FOV (Zoom):",
        "smoothing" => "🌊 Smoothing:",
        "gamma" => "Gamma:",
        "contrast" => "Contrast:",
        "brightness" => "Brightness:",
        "saturation" => "Saturation:",
        "opacity" => "Opacity:",
        _ => key,
    }
}

/// Slider range of a filter setting the Studio knows (Theatre 360's, as its
/// filter properties give them); others get a plain number field.
fn filter_setting_range(key: &str) -> Option<std::ops::RangeInclusive<f64>> {
    Some(match key {
        "pan" | "m_pan" | "rot_pan" | "x_offset" => -180.0..=180.0,
        "tilt" | "m_tilt" | "y_offset" => -90.0..=90.0,
        "fov" | "m_fov" => 10.0..=170.0,
        "smoothing" => 0.0..=0.99,
        "rot_speed" => -360.0..=360.0,
        "scale_x" | "scale_y" => 0.01..=10.0,
        "distance" => 0.1..=20.0,
        _ => return None,
    })
}

/// The Studio's control for one filter setting: Some(new value) only when
/// the user changed it. Showing a value never alters it — nothing is clamped
/// to the slider's range or snapped to a step until the user edits it
/// (sliders with egui's default clamping did both every frame and reported it
/// as a change, so merely opening a filter here rewrote it in OBS: a Master
/// pan of 12.5° became 13°, a Layer scale of 0.35 became 0.4, colors above
/// 5000 were cut to 5000).
fn filter_setting_widget(ui: &mut egui::Ui, key: &str, value: &serde_json::Value) -> Option<serde_json::Value> {
    match value {
        serde_json::Value::Number(num) => {
            let whole = num.is_i64() || num.is_u64();
            let mut f = num.as_f64()?;
            let changed = match filter_setting_range(key) {
                Some(range) => {
                    let slider = egui::Slider::new(&mut f, range).clamping(egui::SliderClamping::Edits);
                    ui.add(if whole { slider.integer() } else { slider }).changed()
                }
                None => ui.add(egui::DragValue::new(&mut f).speed(if whole { 1.0 } else { 0.01 })).changed(),
            };
            changed.then(|| if whole { serde_json::json!(f.round() as i64) } else { serde_json::json!(f) })
        }
        serde_json::Value::Bool(b) => {
            let mut b = *b;
            ui.checkbox(&mut b, "").changed().then(|| serde_json::json!(b))
        }
        serde_json::Value::String(s) => {
            let mut s = s.clone();
            ui.text_edit_singleline(&mut s).changed().then(|| serde_json::json!(s))
        }
        _ => None,
    }
}

/// Writes one setting the user changed in the Studio to the OBS filter —
/// only that setting, so the rest isn't overwritten with the Studio's older
/// copy of it. A Theatre 360 Layer's "Remote Master Control" settings also
/// move its Master, as they do in the Layer's OBS properties.
fn write_filter_setting(
    source_name: &str,
    flt: &crate::obs_source::SourceFilterInfo,
    key: &str,
    value: serde_json::Value,
) {
    let one = |k: &str, v: serde_json::Value| serde_json::json!({ k: v }).to_string();
    if let Some(master_key) = layer_remote_master_key(&flt.id, key) {
        let master_source = flt.settings.get("master_source").and_then(|v| v.as_str()).unwrap_or("");
        if let Some(master) = theatre_360_master(master_source) {
            crate::obs_source::update_source_filter_settings(master_source, &master.name, &one(master_key, value.clone()));
        }
    }
    crate::obs_source::update_source_filter_settings(source_name, &flt.name, &one(key, value));
}

pub fn snapshot_all_sources(sources: &[crate::StudioSource]) -> Vec<SourceTransformSnapshot> {
    sources.iter().map(|src| {
        let filters_opt = pose_filters(&src.name);

        SourceTransformSnapshot {
            source_name: src.name.clone(),
            x: finite_or(src.x, 0.0),
            y: finite_or(src.y, 0.0),
            width: finite_or(src.width, 0.0),
            height: finite_or(src.height, 0.0),
            roll: finite_or(src.roll, 0.0),
            pitch: finite_or(src.pitch, 0.0),
            yaw: finite_or(src.yaw, 0.0),
            is_visible: src.is_visible,
            filters: filters_opt,
            volume: crate::obs_source::query_source_volume(&src.name),
        }
    }).collect()
}

/// (times a pose was applied, id of the last one) — lets the Studio's pose
/// editor notice a pose applied from outside it (Reader event trigger, pose dock).
static LAST_APPLIED_POSE: parking_lot::Mutex<(u64, String)> = parking_lot::Mutex::new((0, String::new()));

fn last_applied_pose() -> (u64, String) {
    LAST_APPLIED_POSE.lock().clone()
}

/// Starts moving every source to `set`. Returns its `last_applied_pose()` count.
pub fn start_scene_transition(
    set: &SceneCoordinateSet,
    current_sources: &[crate::StudioSource],
) -> u64 {
    let seq = {
        let mut last = LAST_APPLIED_POSE.lock();
        last.0 += 1;
        last.1 = set.id.clone();
        last.0
    };
    record_last_sent_pose(set);
    save_last_active_pose_id(&set.id);
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    let mut start_sources = std::collections::HashMap::new();
    let mut target_sources = std::collections::HashMap::new();

    for src in current_sources {
        let target = set.sources.iter().find(|t| t.source_name == src.name);
        let filter_map = transition_start_filters(&src.name, target);

        start_sources.insert(src.name.clone(), SourceTransformSnapshot {
            source_name: src.name.clone(),
            x: src.x,
            y: src.y,
            width: src.width,
            height: src.height,
            roll: src.roll,
            pitch: src.pitch,
            yaw: src.yaw,
            is_visible: src.is_visible,
            filters: filter_map,
            volume: crate::obs_source::query_source_volume(&src.name),
        });
    }

    for target in &set.sources {
        target_sources.insert(target.source_name.clone(), target.clone());
    }

    let dur = if set.easing == MovementEasing::Instant { 0 } else { set.duration_ms.max(1) };
    *ACTIVE_TRANSITION.lock() = Some(ActiveSourceTransition {
        start_time_ms: now_ms,
        duration_ms: dur,
        easing: set.easing,
        start_sources,
        target_sources,
    });
    seq
}

/// Silent end of the Studio's audio volume slider (shown as -∞ dB, sets volume 0).
const VOLUME_SLIDER_MIN_DB: f32 = -60.0;

/// The Studio's volume slider over -60..=max_db dB. Deliberately no step_by and
/// edits-only clamping: with egui's default (always clamp), the live value is
/// re-snapped to the step grid every frame and reported as changed, and the
/// dB↔volume round trip never lands exactly on that grid — so the Studio kept
/// re-applying its stale volume every frame, undoing moves in OBS's own mixer.
fn volume_slider(db: &mut f32, max_db: f32) -> egui::Slider<'_> {
    egui::Slider::new(db, VOLUME_SLIDER_MIN_DB..=max_db)
        .clamping(egui::SliderClamping::Edits)
        .custom_formatter(|v, _| {
            if v <= VOLUME_SLIDER_MIN_DB as f64 { "-∞ dB".to_string() } else { format!("{:.1} dB", v) }
        })
}

/// OBS mixer volume (linear multiplier, 1.0 = 0 dB) → dB for the Studio's slider.
fn volume_to_db(volume: f32) -> f32 {
    if volume <= 0.0 {
        VOLUME_SLIDER_MIN_DB
    } else {
        (20.0 * volume.log10()).max(VOLUME_SLIDER_MIN_DB)
    }
}

/// dB from the Studio's slider → OBS mixer volume (linear multiplier).
fn db_to_volume(db: f32) -> f32 {
    if db <= VOLUME_SLIDER_MIN_DB {
        0.0
    } else {
        10f32.powf(db / 20.0)
    }
}

/// Horizontal position (0..1) of a dBFS level on the Studio's level meter,
/// which spans the same -60..0 dB as the volume slider.
fn meter_fraction(db: f32) -> f32 {
    if db.is_nan() {
        return 0.0;
    }
    // -inf (silence) clamps to 0, clipping above 0 dB to 1
    ((db - VOLUME_SLIDER_MIN_DB) / -VOLUME_SLIDER_MIN_DB).clamp(0.0, 1.0)
}

/// OBS-mixer-style level meter: one bar per channel over -60..0 dBFS, lit
/// green, then yellow from -20 dB and red from -9 dB (OBS's own zones and
/// colors), with a tick at the peak.
fn draw_audio_meter(ui: &mut egui::Ui, levels: &crate::obs_source::AudioLevels) {
    const BAR_H: f32 = 5.0;
    const GAP: f32 = 2.0;
    // (from dB, to dB, dim unlit color, lit color)
    const ZONES: [(f32, f32, Color32, Color32); 3] = [
        (VOLUME_SLIDER_MIN_DB, -20.0, Color32::from_rgb(38, 127, 38), Color32::from_rgb(76, 255, 76)),
        (-20.0, -9.0, Color32::from_rgb(127, 127, 38), Color32::from_rgb(255, 255, 76)),
        (-9.0, 0.0, Color32::from_rgb(127, 38, 38), Color32::from_rgb(255, 76, 76)),
    ];

    let channels = levels.channels.clamp(1, crate::obs_source::MAX_AUDIO_CHANNELS);
    let size = Vec2::new(ui.available_width(), channels as f32 * (BAR_H + GAP) - GAP);
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let painter = ui.painter_at(rect);
    let x_at = |db: f32| rect.left() + meter_fraction(db) * rect.width();

    for ch in 0..channels {
        let top = rect.top() + ch as f32 * (BAR_H + GAP);
        let bottom = top + BAR_H;
        let magnitude = levels.magnitude[ch];
        for (from_db, to_db, dim, lit) in ZONES {
            painter.rect_filled(Rect::from_x_y_ranges(x_at(from_db)..=x_at(to_db), top..=bottom), 0.0, dim.gamma_multiply(0.45));
            if magnitude > from_db {
                painter.rect_filled(Rect::from_x_y_ranges(x_at(from_db)..=x_at(magnitude.min(to_db)), top..=bottom), 0.0, lit);
            }
        }
        let peak = levels.peak[ch];
        if peak > VOLUME_SLIDER_MIN_DB {
            let lit = ZONES.iter().rev().find(|z| peak > z.0).map(|z| z.3).unwrap_or(ZONES[0].3);
            let x = x_at(peak);
            painter.line_segment([Pos2::new(x, top), Pos2::new(x, bottom)], Stroke::new(1.5, lit));
        }
    }
}

/// Volume to set this tick of a pose transition, or None to leave it alone:
/// eased from the current volume (like numeric filter settings), landing
/// exactly on the saved volume at the end (also the only step taken when the
/// current volume couldn't be read). Never negative, since easings can push
/// progress outside 0..1.
fn eased_volume(start: Option<f32>, target: Option<f32>, progress: f32) -> Option<f32> {
    let target = target?;
    if progress >= 1.0 {
        return Some(target);
    }
    start.map(|start| (start + (target - start) * progress).max(0.0))
}

pub fn apply_transition_to_sources(
    trans: &ActiveSourceTransition,
    now_ms: u64,
    sources: &mut [crate::StudioSource],
) -> (f32, bool) {
    let elapsed = now_ms.saturating_sub(trans.start_time_ms);
    let dur = trans.duration_ms as f32;
    let progress = if trans.easing == MovementEasing::Instant || dur <= 0.0 {
        1.0
    } else {
        let t = (elapsed as f32 / dur).clamp(0.0, 1.0);
        trans.easing.evaluate(t)
    };

    for src in sources.iter_mut() {
        if let (Some(start), Some(target)) = (trans.start_sources.get(&src.name), trans.target_sources.get(&src.name)) {
            src.x = start.x + (target.x - start.x) * progress;
            src.y = start.y + (target.y - start.y) * progress;
            src.width = start.width + (target.width - start.width) * progress;
            src.height = start.height + (target.height - start.height) * progress;
            src.roll = start.roll + (target.roll - start.roll) * progress;
            src.pitch = start.pitch + (target.pitch - start.pitch) * progress;
            src.yaw = start.yaw + (target.yaw - start.yaw) * progress;
            if target.is_visible {
                src.is_visible = true;
            } else if progress >= 1.0 {
                src.is_visible = false;
            }
            crate::queue_studio_transform_update(src);

            // Ease the filters toward the pose's — only what the pose changes
            if let Some(target_flts) = target.filters.as_ref() {
                for (flt_name, target_f_val) in target_flts {
                    let start_f_val = start.filters.as_ref().and_then(|m| m.get(flt_name));
                    if let Some(serde_json::Value::Object(t_map)) = target_f_val.get("settings") {
                        let filter_id = target_f_val.get("id").and_then(|v| v.as_str()).unwrap_or("");
                        let s_map = start_f_val.and_then(|f| f.get("settings")).and_then(|v| v.as_object());
                        let writes = filter_settings_step(filter_id, s_map, t_map, progress);
                        if !writes.is_empty() {
                            let json_str = serde_json::Value::Object(writes).to_string();
                            crate::obs_source::update_source_filter_settings(&src.name, flt_name, &json_str);
                        }
                    }

                    if progress >= 1.0 {
                        let was_enabled = start_f_val.and_then(|f| f.get("enabled")).and_then(|v| v.as_bool());
                        if let Some(enabled) = target_f_val.get("enabled").and_then(|v| v.as_bool()) {
                            if was_enabled != Some(enabled) {
                                crate::obs_source::set_source_filter_enabled(&src.name, flt_name, enabled);
                            }
                        }
                    }
                }
            }

            // Ease the audio source's mixer volume toward the pose's, like numeric filter settings
            if let Some(vol) = eased_volume(start.volume, target.volume, progress) {
                crate::obs_source::set_source_volume(&src.name, vol);
            }
        }
    }

    let is_finished = trans.duration_ms == 0 || elapsed >= trans.duration_ms as u64;
    (progress, is_finished)
}

pub fn tick_active_transition_standalone() {
    let _apply_guard = match TRANSITION_APPLY_LOCK.try_lock() {
        Some(g) => g,
        None => return,
    };
    let mut trans_guard = ACTIVE_TRANSITION.lock();
    let trans = match trans_guard.as_mut() {
        Some(t) => t,
        None => return,
    };

    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    // Operate on an owned snapshot rather than holding studio_sources.write()
    // here: apply_transition_to_sources calls queue_studio_transform_update
    // -> process_pending_studio_transforms, which acquires studio_sources.write()
    // itself to sync results back in. Holding our own write guard across that
    // call self-deadlocks this thread against its own lock (confirmed live via
    // gdb: this exact frame stuck in RawRwLock::lock_exclusive_slow).
    let state = get_state();
    let mut sources_snapshot = state.studio_sources.read().clone();
    let (_, is_finished) = apply_transition_to_sources(trans, now_ms, &mut sources_snapshot);

    if let Some(ref ctx) = *STUDIO_EGUI_CTX.lock() {
        ctx.request_repaint();
    }

    if is_finished {
        *trans_guard = None;
    }
}

pub fn tick_active_transition(
    ctx: &egui::Context,
    sources: &mut [crate::StudioSource],
    sources_modified: &mut bool,
) {
    let _apply_guard = match TRANSITION_APPLY_LOCK.try_lock() {
        Some(g) => g,
        None => return,
    };
    let mut trans_guard = ACTIVE_TRANSITION.lock();
    let trans = match trans_guard.as_mut() {
        Some(t) => t,
        None => return,
    };

    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    // Note: apply_transition_to_sources already syncs each updated source back
    // into the shared state.studio_sources (via queue_studio_transform_update
    // -> process_pending_studio_transforms), so there's no need to separately
    // re-apply it while holding state.studio_sources.write() here — doing so
    // used to self-deadlock this thread against its own write lock the moment
    // process_pending_studio_transforms tried to acquire it again.
    let (_, is_finished) = apply_transition_to_sources(trans, now_ms, sources);

    *sources_modified = true;
    ctx.request_repaint();

    if is_finished {
        *trans_guard = None;
    }
}

pub fn read_disk_targets() -> Vec<crate::StudioSource> {
    let repo_targets = crate::get_theater_repo_dir().join("theater_targets.json");
    let paths = [
        repo_targets,
        std::path::PathBuf::from("/home/mortygotvr/repo/theater-scene/theater_targets.json"),
        std::path::PathBuf::from("./theater_targets.json"),
        std::path::PathBuf::from("theater_targets.json"),
    ];

    let mut loaded_sources = Vec::new();
    let mut loaded_sets = Vec::new();
    for path in &paths {
        if let Ok(file) = std::fs::File::open(path) {
            if let Ok(serde_json::Value::Array(arr)) = serde_json::from_reader(file) {
                for item in arr {
                    // Studio settings: only ever held the render delay, which is
                    // gone (the combined render is always shown the same frame)
                    if item.get("type").and_then(|v| v.as_str()) == Some("studio_settings") {
                        continue;
                    }

                    // Check if coordinate set
                    let is_coord_set = item.get("type").and_then(|v| v.as_str()) == Some("scene_coordinate_set");
                    if is_coord_set {
                        match serde_json::from_value::<SceneCoordinateSet>(item.clone()) {
                            Ok(set) => loaded_sets.push(set),
                            Err(e) => crate::obs_source::trust_log(&format!(
                                "[Trust Studio] ⚠️ Could not load pose '{}' (scene '{}') from '{}': {}",
                                item.get("name").and_then(|v| v.as_str()).unwrap_or("?"),
                                item.get("scene").and_then(|v| v.as_str()).unwrap_or("?"),
                                path.display(),
                                e
                            )),
                        }
                        continue;
                    }

                    let name = item.get("name")
                        .or_else(|| item.get("source_name"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    if name.is_empty() { continue; }
                    let scene = item.get("scene")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Scène")
                        .to_string();
                    let slot = item.get("slot").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                    let is_target = item.get("target").and_then(|v| v.as_bool()).unwrap_or(false);
                    let match_obs = item.get("match_obs").and_then(|v| v.as_bool()).unwrap_or(true);
                    let is_visible = item.get("visible").and_then(|v| v.as_bool()).unwrap_or(true);
                    let file_path = item.get("file_path").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let custom = item.get("custom");
                    let cur = item.get("current_xform");
                    let def = item.get("default_xform");
                    let xf = if match_obs {
                        cur.or(custom)
                    } else {
                        custom.or(cur)
                    };
                    let x = xf.and_then(|c| c.get("x")).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                    let y = xf.and_then(|c| c.get("y")).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                    let width = xf.and_then(|c| c.get("width")).and_then(|v| v.as_f64()).unwrap_or(200.0) as f32;
                    let height = xf.and_then(|c| c.get("height")).and_then(|v| v.as_f64()).unwrap_or(200.0) as f32;
                    let roll = xf.and_then(|c| c.get("roll")).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                    let pitch = xf.and_then(|c| c.get("pitch")).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                    let yaw = xf.and_then(|c| c.get("yaw")).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;

                    let def_xf = def.or(cur).or(custom);
                    let default_x = def_xf.and_then(|c| c.get("x")).and_then(|v| v.as_f64()).unwrap_or(x as f64) as f32;
                    let default_y = def_xf.and_then(|c| c.get("y")).and_then(|v| v.as_f64()).unwrap_or(y as f64) as f32;
                    let default_width = def_xf.and_then(|c| c.get("width")).and_then(|v| v.as_f64()).unwrap_or(width as f64) as f32;
                    let default_height = def_xf.and_then(|c| c.get("height")).and_then(|v| v.as_f64()).unwrap_or(height as f64) as f32;
                    let default_roll = def_xf.and_then(|c| c.get("roll")).and_then(|v| v.as_f64()).unwrap_or(roll as f64) as f32;

                    loaded_sources.push(crate::StudioSource {
                        scene,
                        name: name.clone(),
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
                        file_path,
                        default_x,
                        default_y,
                        default_width,
                        default_height,
                        default_roll,
                    });
                }
                println!("[Trust Studio] 📥 Loaded {} target sources and {} coordinate sets from '{}'", loaded_sources.len(), loaded_sets.len(), path.display());
                *SCENE_COORDINATE_SETS.write() = loaded_sets;
                {
                    let mut map = LAST_SENT_POSE.write();
                    for src in &loaded_sources {
                        map.entry(src.name.clone()).or_insert_with(|| SourceTransformSnapshot {
                            source_name: src.name.clone(),
                            x: src.x,
                            y: src.y,
                            width: src.width,
                            height: src.height,
                            roll: src.roll,
                            pitch: src.pitch,
                            yaw: src.yaw,
                            is_visible: src.is_visible,
                            filters: None,
                            volume: None,
                        });
                    }
                }
                break;
            }
        }
    }
    loaded_sources
}

static STUDIO_TARGETS_LOADING: parking_lot::Mutex<()> = parking_lot::Mutex::new(());

pub fn load_studio_targets_from_disk() -> usize {
    // Non-blocking: this function is reachable from multiple threads at once
    // (Lua's script_load on the scripting thread, and source_video_tick's
    // "sources list still empty" check on the video-io thread both call it
    // during startup). If a load is already in flight, skip instead of
    // waiting — this function drives real libobs sceneitem calls, and two
    // threads running those concurrently for the same scene can deadlock
    // against libobs's own internal locking. A blocking lock here would just
    // trade the double-invocation race for a different stall, so callers
    // that lose the race simply back off; the in-flight load will populate
    // studio_sources for everyone.
    let _guard = match STUDIO_TARGETS_LOADING.try_lock() {
        Some(g) => g,
        None => return 0,
    };
    let state = get_state();
    let mut loaded_sources = read_disk_targets();

    // Sort in good order by slot (1, 2, 3...)
    loaded_sources.sort_by(|a, b| {
        let a_slot = if a.slot > 0 { a.slot } else { u32::MAX };
        let b_slot = if b.slot > 0 { b.slot } else { u32::MAX };
        a_slot.cmp(&b_slot)
    });

    let count = loaded_sources.len();
    if !loaded_sources.is_empty() {
        if state.studio_selected_source.read().is_empty() {
            if let Some(first_target) = loaded_sources.iter().find(|s| s.is_target) {
                *state.studio_selected_source.write() = first_target.name.clone();
            } else if let Some(first) = loaded_sources.first() {
                *state.studio_selected_source.write() = first.name.clone();
            }
        }

        // Sync target source names into target_slots
        {
            let mut slot_map = state.target_slots.write();
            let mut auto_slot = 1;
            for src in loaded_sources.iter().filter(|s| s.is_target) {
                let s_num = if src.slot > 0 { src.slot as usize } else { auto_slot };
                slot_map.insert(s_num, src.name.clone());
                auto_slot = s_num + 1;
            }
        }

        for src in &loaded_sources {
            // Pre-load texture from file_path if available so background/target textures exist immediately
            if !src.file_path.is_empty() {
                let p = std::path::Path::new(&src.file_path);
                if p.exists() {
                    if let Ok(img) = image::open(p) {
                        let rgba = img.to_rgba8();
                        let w = rgba.width();
                        let h = rgba.height();
                        let raw = rgba.into_raw();
                        let config = *state.config.read();
                        let transform = crate::types::SourceTransform {
                            pos_x: src.x,
                            pos_y: src.y,
                            scale_x: if w > 0 { src.width / w as f32 } else { 1.0 },
                            scale_y: if h > 0 { src.height / h as f32 } else { 1.0 },
                            crop_left: 0,
                            crop_top: 0,
                            crop_right: 0,
                            crop_bottom: 0,
                            rot_deg: src.roll,
                        };
                        let (hitbox, mask, pixel_grid) = crate::tracker::scan_rgba_with_3d_transform(
                            &raw,
                            w,
                            h,
                            w * 4,
                            &transform,
                            src.pitch,
                            src.yaw,
                            &config,
                        );
                        state.hitboxes.write().insert(src.name.clone(), hitbox);
                        state.masks.write().insert(src.name.clone(), mask);
                        state.pixel_grids.write().insert(src.name.clone(), pixel_grid);
                        state.target_textures.write().insert(src.name.clone(), crate::state::TargetPreviewTexture {
                            width: w,
                            height: h,
                            rgba: raw,
                            frame_id: 1,
                            base_width: w,
                            base_height: h,
                        });
                        println!("[Trust Studio] 🖼️ Loaded initial texture for '{}' from '{}' ({}x{})", src.name, src.file_path, w, h);
                    }
                }
            }
        }

        *state.studio_sources.write() = loaded_sources;
        let cur_scene = state.studio_scene_name.read().clone();
        let scene_to_use = if cur_scene.is_empty() { "Scène".to_string() } else { cur_scene };
        load_and_apply_last_pose_on_load(&scene_to_use);
    }
    count
}

const APP_ICON_PNG_BYTES: &[u8] = include_bytes!("../assets/icon_256.png");

pub fn get_app_icon_data() -> Option<egui::IconData> {
    if let Ok(img) = image::load_from_memory(APP_ICON_PNG_BYTES) {
        let rgba = img.to_rgba8();
        let (width, height) = rgba.dimensions();
        Some(egui::IconData {
            rgba: rgba.into_raw(),
            width,
            height,
        })
    } else {
        None
    }
}

impl StudioApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        crate::obs_source::refresh_obs_scenes();
        let state = get_state();
        let cur_scene = state.studio_scene_name.read().clone();
        let scene_to_use = if cur_scene.is_empty() { "Scène".to_string() } else { cur_scene };
        *state.studio_scene_name.write() = scene_to_use.clone();

        // Opening the Studio shows the scene as it is. Targets are read from
        // disk only if Theater has none yet (that first load puts the last
        // pose back, as when OBS starts); the sync with OBS moves nothing.
        if state.studio_sources.read().is_empty() {
            load_studio_targets_from_disk();
        }
        crate::obs_source::sync_studio_sources_for_scene(&scene_to_use);

        let app_icon_texture = if let Ok(img) = image::load_from_memory(APP_ICON_PNG_BYTES) {
            let rgba = img.to_rgba8();
            let (w, h) = rgba.dimensions();
            let color_img = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &rgba.into_raw());
            Some(_cc.egui_ctx.load_texture("theater_app_icon", color_img, egui::TextureOptions::LINEAR))
        } else {
            None
        };

        let mut initial_selected = state.studio_selected_source.read().clone();
        if initial_selected.is_empty() {
            let sources = state.studio_sources.read();
            if let Some(first_target) = sources.iter().find(|s| s.is_target) {
                initial_selected = first_target.name.clone();
            } else if let Some(first) = sources.first() {
                initial_selected = first.name.clone();
            }
            drop(sources);
            if !initial_selected.is_empty() {
                *state.studio_selected_source.write() = initial_selected.clone();
            }
        }

        let initial_show_hitbox = crate::state::get_state().studio_show_hitbox.load(std::sync::atomic::Ordering::Relaxed);
        let initial_black_backdrop = crate::state::get_state().studio_black_backdrop_enabled.load(std::sync::atomic::Ordering::Relaxed);

        let (file_dialog_tx, file_dialog_rx) = crossbeam_channel::unbounded();
        let (package_tx, package_rx) = crossbeam_channel::unbounded();
        let removed_lua_effects = load_removed_lua_effects();
        let redeems = load_all_redeems_from_disk();
        let sel_idx = if redeems.is_empty() { None } else { Some(0) };

        let mut custom_redeem_scripts = scan_workspace_custom_scripts();
        for item in &redeems {
            if let Some(ref p) = item.config.custom_redeem_script {
                let path = std::path::PathBuf::from(p);
                if path.is_file() && !custom_redeem_scripts.iter().any(|e| e.path == path) {
                    let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("Custom Script").to_string();
                    custom_redeem_scripts.push(LuaScriptEntry { name, path });
                }
            }
        }

        Self {
            selected_source_name: initial_selected,
            drag_mode: GizmoDragMode::None,
            drag_start_pointer: Pos2::ZERO,
            drag_start_x: 0.0,
            drag_start_y: 0.0,
            drag_start_w: 0.0,
            drag_start_h: 0.0,
            drag_start_roll: 0.0,
            drag_start_pitch: 0.0,
            drag_start_yaw: 0.0,
            preview_texture: None,
            mask_textures: std::collections::HashMap::new(),
            last_target_frame_ids: std::collections::HashMap::new(),
            show_hitboxes: initial_show_hitbox,
            show_black_backdrop: initial_black_backdrop,
            status_msg: "Ready".to_string(),
            target_fps: crate::obs_source::saved_studio_fps().unwrap_or(60),
            last_frame_at: None,
            last_preview_frame_id: 0,
            frame_counter: 0,
            last_synced_scene: scene_to_use,
            last_throwables_check_ms: 0,
            event_rx: Some(get_state().get_or_create_client_receiver(3)),
            active_tab: StudioActiveTab::SceneStudio,
            redeems_list: redeems,
            selected_redeem_index: sel_idx,
            redeem_search_filter: String::new(),
            active_redeem_subtab: RedeemSubTab::Physics,
            redeem_editor_status: "Ready".to_string(),
            lua_effects_list: lua_effects_in_filters(&removed_lua_effects),
            removed_lua_effects,
            selected_lua_effect_index: None,
            custom_redeem_scripts_list: custom_redeem_scripts,
            selected_custom_redeem_index: None,
            file_dialog_tx,
            file_dialog_rx,
            package_tx,
            package_rx,
            pending_import: None,
            scripts: scripts_tab::ScriptsTab::default(),
            settings: settings_tab::SettingsTab::default(),
            source_filters_cache: std::collections::HashMap::new(),
            last_filters_query_source: String::new(),
            last_filters_query_time: 0,
            filter_props: std::collections::HashMap::new(),
            filter_props_source: String::new(),
            filter_props_time: 0,
            filter_props_stale: false,
            source_volume_cache: None,
            last_volume_query_source: String::new(),
            last_volume_query_time: 0,
            app_icon_texture,
            editing_pose: None,
            pending_pose_switch: None,
        }
    }
}


impl StudioApp {
    /// Export's and Import's results, and the Shield alert for a package
    /// whose scripts make blocked calls.
    fn handle_redeem_packages(&mut self, ctx: &egui::Context) {
        while let Ok(event) = self.package_rx.try_recv() {
            match event {
                PackageEvent::Exported { file, result } => {
                    self.redeem_editor_status = match result {
                        Ok(report) if report.missing.is_empty() => {
                            format!("Exported {} ({} files)", file.display(), report.files)
                        }
                        Ok(report) => format!(
                            "Exported {} ({} files) — not found, so not in it: {}",
                            file.display(),
                            report.files,
                            report.missing.join(", ")
                        ),
                        Err(e) => format!("Export failed: {}", e),
                    };
                }
                PackageEvent::Opened { file, result } => match result {
                    Ok(package) if package.threats.is_empty() => self.install_redeem_package(&file, &package),
                    Ok(package) => self.pending_import = Some((file, package)),
                    Err(e) => self.redeem_editor_status = format!("Import failed: {}", e),
                },
            }
        }

        let mut answer = None;
        if let Some((file, package)) = &self.pending_import {
            let red = Color32::from_rgb(239, 68, 68);
            egui::Window::new("🛡 Theater Shield")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.set_max_width(520.0);
                    let file_name = file.file_name().unwrap_or_default().to_string_lossy();
                    ui.label(
                        egui::RichText::new(format!(
                            "⚠ {} risky call{} found in {}",
                            package.threats.len(),
                            if package.threats.len() == 1 { "" } else { "s" },
                            file_name
                        ))
                        .strong()
                        .size(15.0)
                        .color(red),
                    );
                    ui.add_space(4.0);
                    ui.label(
                        "Redeem scripts are filters and animations: they have no business with files, \
                         programs or other code. Theater blocks these calls, so they won't run even if \
                         you import this redeem:",
                    );
                    ui.add_space(4.0);
                    egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                        for (script, threat) in &package.threats {
                            ui.label(
                                egui::RichText::new(format!(
                                    "⛔ {} line {}: {} — it {}",
                                    script, threat.line, threat.call, threat.why
                                ))
                                .color(Color32::from_rgb(251, 146, 60)),
                            );
                        }
                    });
                    ui.add_space(4.0);
                    ui.label(egui::RichText::new("Import it only if you trust whoever made it.").italics());
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if ui.button(egui::RichText::new("Don't import").strong()).clicked() {
                            answer = Some(false);
                        }
                        if ui.button("Import anyway (calls stay blocked)").clicked() {
                            answer = Some(true);
                        }
                    });
                });
        }
        match answer {
            Some(true) => {
                if let Some((file, package)) = self.pending_import.take() {
                    self.install_redeem_package(&file, &package);
                }
            }
            Some(false) => {
                self.pending_import = None;
                self.redeem_editor_status = "Import cancelled".to_string();
            }
            None => {}
        }
    }

    /// Adds an opened package to the redeems and selects it.
    fn install_redeem_package(&mut self, file: &std::path::Path, package: &crate::redeem_package::Package) {
        let name = file.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        match crate::redeem_package::install(package, &name) {
            Ok(item) => {
                if let Some(script) = item.config.custom_redeem_script.as_ref().map(std::path::PathBuf::from) {
                    if script.is_file() && !self.custom_redeem_scripts_list.iter().any(|e| e.path == script) {
                        let name = script.file_stem().and_then(|s| s.to_str()).unwrap_or("Custom Script").to_string();
                        self.custom_redeem_scripts_list.push(LuaScriptEntry { name, path: script });
                    }
                }
                let filename = item.filename.clone();
                self.redeem_editor_status = format!(
                    "Imported {} as {}",
                    file.file_name().unwrap_or_default().to_string_lossy(),
                    filename
                );
                self.redeems_list.push(item);
                self.redeems_list.sort_by(|a, b| a.filename.to_lowercase().cmp(&b.filename.to_lowercase()));
                self.selected_redeem_index = self.redeems_list.iter().position(|r| r.filename == filename);
            }
            Err(e) => self.redeem_editor_status = format!("Import failed: {}", e),
        }
    }
}

impl eframe::App for StudioApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // The egui docks' windows are this thread's too
        crate::egui_docks::show(ctx);
        if STUDIO_HIDE_AT_START.swap(false, std::sync::atomic::Ordering::SeqCst) {
            // eframe shows its window after the first frame: back out of sight
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }

        // Handle incoming results from native system file dialogs
        while let Ok(res) = self.file_dialog_rx.try_recv() {
            let p_str = res.path.to_string_lossy().to_string();

            match res.target {
                FileDialogTarget::LuaEffectScript => {
                    let name = res.path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or(p_str.clone());
                    // Browsing for a removed one brings it back
                    if self.removed_lua_effects.remove(&effect_file_name(&res.path)) {
                        save_removed_lua_effects(&self.removed_lua_effects);
                    }
                    match self.lua_effects_list.iter().position(|e| e.path == res.path) {
                        Some(existing) => self.selected_lua_effect_index = Some(existing),
                        None => {
                            self.lua_effects_list.push(LuaScriptEntry { name, path: res.path.clone() });
                            self.selected_lua_effect_index = Some(self.lua_effects_list.len() - 1);
                        }
                    }
                    self.redeem_editor_status = format!("Added Lua effect: {}", p_str);
                    continue;
                }
                FileDialogTarget::CustomRedeemScript => {
                    let name = res.path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or(p_str.clone());
                    if !self.custom_redeem_scripts_list.iter().any(|e| e.path == res.path) {
                        self.custom_redeem_scripts_list.push(LuaScriptEntry { name, path: res.path.clone() });
                    }
                    self.selected_custom_redeem_index = Some(self.custom_redeem_scripts_list.len() - 1);
                    self.redeem_editor_status = format!("Selected custom script: {}", p_str);
                    if let Some(idx) = self.selected_redeem_index {
                        if let Some(item) = self.redeems_list.get_mut(idx) {
                            item.config.custom_redeem_script = Some(p_str.clone());
                            item.is_dirty = true;
                        }
                    }
                    continue;
                }
                _ => {}
            }

            if let Some(idx) = self.selected_redeem_index {
                if let Some(item) = self.redeems_list.get_mut(idx) {
                    match res.target {
                        FileDialogTarget::Image => {
                            item.config.image_file_path = Some(p_str.clone());
                            if let Some(b64) = resolve_and_encode_asset(&p_str) {
                                let len_kb = b64.len() / 1024;
                                item.config.image_src = Some(b64);
                                self.redeem_editor_status = format!("Loaded & encoded image: {} ({} KB)", res.path.file_name().unwrap_or_default().to_string_lossy(), len_kb);
                            }
                            item.is_dirty = true;
                        }
                        FileDialogTarget::LaunchSound => {
                            item.config.start_sound_path = Some(p_str.clone());
                            if let Some(b64) = resolve_and_encode_asset(&p_str) {
                                let len_kb = b64.len() / 1024;
                                item.config.start_sound = Some(b64);
                                self.redeem_editor_status = format!("Loaded & encoded launch audio: {} ({} KB)", res.path.file_name().unwrap_or_default().to_string_lossy(), len_kb);
                            }
                            item.is_dirty = true;
                        }
                        FileDialogTarget::CollisionSound => {
                            item.config.collision_sound_path = Some(p_str.clone());
                            if let Some(b64) = resolve_and_encode_asset(&p_str) {
                                let len_kb = b64.len() / 1024;
                                item.config.collision_sound = Some(b64);
                                self.redeem_editor_status = format!("Loaded & encoded collision audio: {} ({} KB)", res.path.file_name().unwrap_or_default().to_string_lossy(), len_kb);
                            }
                            item.is_dirty = true;
                        }
                        FileDialogTarget::WaterTexture => {
                            item.config.water_texture_path = Some(p_str.clone());
                            item.is_dirty = true;
                            self.redeem_editor_status = format!("Set water texture: {}", p_str);
                        }
                        FileDialogTarget::LuaEffectScript | FileDialogTarget::CustomRedeemScript => {}
                    }
                }
            }
        }

        self.handle_redeem_packages(ctx);

        // 0. Intercept close request from window manager (X button) to keep EventLoop alive
        if ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            IS_STUDIO_WINDOW_VISIBLE.store(false, std::sync::atomic::Ordering::SeqCst);
            // No one to show the level meter to while hidden
            crate::obs_source::stop_audio_levels();
            println!("[Trust Studio] 👁️ Studio window closed/hidden (event loop kept alive).");
        }

        // Handle show / focus request triggered from OBS top menu
        if STUDIO_SHOW_REQUEST.swap(false, std::sync::atomic::Ordering::SeqCst) {
            IS_STUDIO_WINDOW_VISIBLE.store(true, std::sync::atomic::Ordering::SeqCst);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            if STUDIO_OFF_SCREEN.swap(false, std::sync::atomic::Ordering::SeqCst) {
                let centred = egui::ViewportCommand::center_on_screen(ctx);
                ctx.send_viewport_cmd(centred.unwrap_or(egui::ViewportCommand::OuterPosition(egui::pos2(80.0, 80.0))));
            }
            println!("[Trust Studio] 🎬 Studio window restored and focused.");
        }

        // If window is hidden, throttle repaint to sleep and avoid GPU/CPU overhead
        if !IS_STUDIO_WINDOW_VISIBLE.load(std::sync::atomic::Ordering::SeqCst) {
            ctx.request_repaint_after(std::time::Duration::from_millis(300));
            return;
        }

        if let Some(tab) = REQUESTED_STUDIO_TAB.lock().take() {
            self.active_tab = tab;
        }

        // The chosen refresh rate is the most frames the Studio draws a
        // second: a frame asked for sooner (moving throwables, a new preview
        // picture, an effect all ask for one at once) waits for its turn. Only
        // asking for the next frame "no later than" that let them all through.
        // The Studio has its own thread, which nothing in OBS waits on.
        let interval = std::time::Duration::from_micros(1_000_000 / self.target_fps.max(15) as u64);
        if let Some(last) = self.last_frame_at {
            let since = last.elapsed();
            if since < interval {
                std::thread::sleep(interval - since);
            }
        }
        self.last_frame_at = Some(std::time::Instant::now());
        ctx.request_repaint_after(interval);

        let state = get_state();
        let (cw, ch) = crate::obs_source::get_obs_canvas_resolution();
        crate::obs_source::notify_preview_active();

        // No physics or effects stepping here: OBS steps them every frame (in
        // the Throwables Layer's render, or its tick when that's hidden), and
        // the Studio shows OBS's own picture. Stepping them here as well, once
        // per Studio frame, made throws move faster while the Studio was open —
        // in OBS's output too, and more so the faster the Studio redrew.

        // Continuous repaint if active simulation particles / throwables exist
        let has_active_sim = {
            let p = state.physics.read();
            let has_p = !p.throwables.is_empty() || !p.particles.is_empty();
            let e = state.effects.read();
            let has_e = e.particle_count() > 0 || e.segment_count() > 0;
            has_p || has_e
        };
        if has_active_sim {
            ctx.request_repaint();
        }

        // Update composite frame texture ONLY if a new frame arrived from OBS
        // (Hoisted here so both SceneStudio and RedeemEditor share the fresh live composite frame)
        {
            let comp_guard = state.studio_composite_frame.read();
            if let Some(ref comp) = *comp_guard {
                if comp.width > 0 && comp.height > 0 && !comp.rgba.is_empty() && (comp.frame_id != self.last_preview_frame_id || self.preview_texture.is_none()) {
                    self.last_preview_frame_id = comp.frame_id;

                    let color_image = egui::ColorImage::from_rgba_unmultiplied(
                        [comp.width as usize, comp.height as usize],
                        &comp.rgba,
                    );
                    if let Some(ref mut tex) = self.preview_texture {
                        tex.set(color_image, egui::TextureOptions::LINEAR);
                    } else {
                        self.preview_texture = Some(ctx.load_texture(
                            "composite_frame",
                            color_image,
                            egui::TextureOptions::LINEAR,
                        ));
                    }
                }
            }
        }

        self.frame_counter = self.frame_counter.wrapping_add(1);
        if self.frame_counter % 60 == 0 {
            crate::obs_source::refresh_obs_scenes();
        }
        // Never sit on a scene this OBS doesn't have (the "Scène" default, or
        // another machine's scene kept in theater_targets.json): a pose saved
        // there is filed under a scene no dock or list here ever shows.
        {
            let available = state.studio_available_scenes.read().clone();
            let cur = state.studio_scene_name.read().clone();
            if !available.is_empty() && !available.contains(&cur) {
                crate::obs_source::refresh_obs_scenes();
                let obs_current = state.studio_scene_name.read().clone();
                let fixed = if available.contains(&obs_current) { obs_current } else { available[0].clone() };
                *state.studio_scene_name.write() = fixed;
            }
        }
        // Follow scene changes from anywhere (OBS, the dock, the render thread)
        // with that scene's source list too, not just its name — as picking a
        // scene in the dropdown already does.
        let cur_scene = state.studio_scene_name.read().clone();
        if !cur_scene.is_empty() && cur_scene != self.last_synced_scene {
            crate::obs_source::sync_studio_sources_for_scene(&cur_scene);
            self.last_synced_scene = cur_scene.clone();
            self.selected_source_name = state.studio_selected_source.read().clone();
        }

        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        if now_ms.saturating_sub(self.last_throwables_check_ms) >= 300 {
            self.last_throwables_check_ms = now_ms;
            crate::obs_source::get_throwables_layer_status(&cur_scene);
        }

        let is_throwables_vis = state.studio_throwables_visible.load(std::sync::atomic::Ordering::Relaxed);
        let last_render_ms = state.studio_throwables_last_render_ms.load(std::sync::atomic::Ordering::Relaxed);
        let render_stale = last_render_ms > 0 && now_ms.saturating_sub(last_render_ms) > 1200;
        let is_throwable_inactive = !is_throwables_vis || render_stale || (last_render_ms == 0 && self.preview_texture.is_none());

        if is_throwable_inactive {
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }

        let mut sources = state.studio_sources.read().clone();
        let mut sources_modified = false;
        let mut save_requested = false;

        // Tick active smooth transitions for scene coordinate sets
        tick_active_transition(ctx, &mut sources, &mut sources_modified);

        // Drain any unconsumed channel events to prevent backlog
        if let Some(rx) = &self.event_rx {
            while let Ok(_) = rx.try_recv() {}
        }
        if let Some(status) = LAST_TRIGGER_STATUS.write().take() {
            self.status_msg = status;
            ctx.request_repaint();
        }

        // 1. Top Control Bar
        egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if let Some(ref icon) = self.app_icon_texture {
                    ui.image((icon.id(), egui::vec2(22.0, 22.0)));
                }
                if ui.selectable_label(
                    self.active_tab == StudioActiveTab::SceneStudio,
                    egui::RichText::new("🎬 Scene Studio").strong().size(13.5),
                ).clicked() {
                    self.active_tab = StudioActiveTab::SceneStudio;
                }

                if ui.selectable_label(
                    self.active_tab == StudioActiveTab::RedeemEditor,
                    egui::RichText::new("🎨 Redeem & FX Editor").strong().size(13.5),
                ).clicked() {
                    self.active_tab = StudioActiveTab::RedeemEditor;
                }

                if ui.selectable_label(
                    self.active_tab == StudioActiveTab::Scripts,
                    egui::RichText::new("📜 Scripts").strong().size(13.5),
                ).on_hover_text("Reactions, redeem scripts and filters (Lua and Rhai)").clicked() {
                    self.active_tab = StudioActiveTab::Scripts;
                }

                if ui.selectable_label(
                    self.active_tab == StudioActiveTab::Settings,
                    egui::RichText::new("⚙ Settings").strong().size(13.5),
                ).on_hover_text("Theater's own settings: target masks, Rhai filter folders").clicked() {
                    self.active_tab = StudioActiveTab::Settings;
                }
                ui.separator();

                let mut current_scene = state.studio_scene_name.read().clone();
                let mut available_scenes = state.studio_available_scenes.read().clone();
                if available_scenes.is_empty() {
                    drop(available_scenes);
                    crate::obs_source::refresh_obs_scenes();
                    available_scenes = state.studio_available_scenes.read().clone();
                }
                let scenes_list = if available_scenes.is_empty() {
                    vec![if current_scene.is_empty() { "Scène".to_string() } else { current_scene.clone() }]
                } else {
                    available_scenes
                };

                if current_scene.is_empty() {
                    current_scene = scenes_list[0].clone();
                    *state.studio_scene_name.write() = current_scene.clone();
                }

                let mut scene_changed = false;
                ui.label("🎬 Scene:");
                egui::ComboBox::from_id_salt("scene_select_combo")
                    .selected_text(if current_scene.is_empty() { "Select Scene" } else { &current_scene })
                    .show_ui(ui, |ui| {
                        for sc in &scenes_list {
                            if ui.selectable_value(&mut current_scene, sc.clone(), sc).clicked() {
                                scene_changed = true;
                            }
                        }
                    });

                if scene_changed {
                    *state.studio_scene_name.write() = current_scene.clone();
                    crate::queue_studio_scene_change(&current_scene);
                    crate::obs_source::switch_obs_current_scene(&current_scene);
                    crate::obs_source::sync_studio_sources_for_scene(&current_scene);
                    // Picked here: that scene comes back in its last pose
                    load_and_apply_last_pose_on_load(&current_scene);
                    sources = state.studio_sources.read().clone();
                    self.selected_source_name = state.studio_selected_source.read().clone();
                }

                if ui.button("🔄").on_hover_text("Refresh OBS scenes and source order").clicked() {
                    crate::obs_source::refresh_obs_scenes();
                    crate::obs_source::sync_studio_sources_for_scene(&current_scene);
                    sources = state.studio_sources.read().clone();
                    self.selected_source_name = state.studio_selected_source.read().clone();
                }
                ui.separator();

                ui.label(format!("Resolution: {}x{}", cw, ch));
                ui.separator();

                // Refresh Rate Selector: the most frames the Studio draws a second (saved)
                ui.label("⚡ Refresh Rate:");
                let before = self.target_fps;
                egui::ComboBox::from_id_salt("fps_select")
                    .selected_text(format!("{} FPS", self.target_fps))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.target_fps, 30, "30 FPS");
                        ui.selectable_value(&mut self.target_fps, 60, "60 FPS");
                        ui.selectable_value(&mut self.target_fps, 120, "120 FPS");
                        ui.selectable_value(&mut self.target_fps, 144, "144 FPS");
                    })
                    .response
                    .on_hover_text("How often the Studio window redraws, at most. Lower is lighter on the PC; OBS's own output isn't affected");
                if self.target_fps != before {
                    crate::obs_source::save_studio_fps(self.target_fps);
                }


                if self.active_tab == StudioActiveTab::SceneStudio {
                    if ui.checkbox(&mut self.show_hitboxes, "🎯 Show Hitboxes").changed() {
                        get_state().studio_show_hitbox.store(self.show_hitboxes, std::sync::atomic::Ordering::Relaxed);
                    }
                    if ui.checkbox(&mut self.show_black_backdrop, "⬛ Black Backdrop").on_hover_text("Real black source rendered behind everything else — disable to see the composite without it").changed() {
                        get_state().studio_black_backdrop_enabled.store(self.show_black_backdrop, std::sync::atomic::Ordering::Relaxed);
                    }
                    ui.separator();

                    if ui.button("💾 Save Targets").clicked() {
                        save_requested = true;
                    }
                }
                ui.separator();

                if ui.button("❌ Close Window").clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                    IS_STUDIO_WINDOW_VISIBLE.store(false, std::sync::atomic::Ordering::SeqCst);
                    println!("[Trust Studio] 👁️ Studio window hidden via Close Window button.");
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let msg = match self.active_tab {
                        StudioActiveTab::SceneStudio => &self.status_msg,
                        StudioActiveTab::RedeemEditor => &self.redeem_editor_status,
                        StudioActiveTab::Scripts => &self.scripts.status,
                        StudioActiveTab::Settings => &self.settings.status,
                    };
                    ui.label(msg);
                });
            });
        });

        match self.active_tab {
            StudioActiveTab::SceneStudio => {
        // 2. Main Central Layout: Left Preview Viewport, Right Target Sources Table
        egui::SidePanel::right("targets_panel")
            .min_width(320.0)
            .default_width(380.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if let Some(ref icon) = self.app_icon_texture {
                        ui.image((icon.id(), egui::vec2(18.0, 18.0)));
                    }
                    ui.heading("📊 Scene Sources & Targets");
                });
                ui.separator();

                let mut source_to_select = None;

                // Limit visible sources table to 8 sources with an always-visible scroll bar
                let row_height = 25.0;
                let header_height = 24.0;
                let max_sources_height = header_height + (8.0 * row_height); // Limit to 8 sources visible (~224px)

                egui::ScrollArea::vertical()
                    .id_salt("sources_scroll_area")
                    .max_height(max_sources_height)
                    .auto_shrink([false, false])
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
                    .show(ui, |ui| {
                    egui::Grid::new("sources_grid")
                        .striped(true)
                        .min_col_width(40.0)
                        .show(ui, |ui| {
                            ui.label(egui::RichText::new("Target").strong());
                            ui.label(egui::RichText::new("Visible").strong());
                            ui.label(egui::RichText::new("Name").strong());
                            ui.label(egui::RichText::new("Pos").strong());
                            ui.end_row();

                            for src in sources.iter_mut() {
                                let is_selected = src.name == self.selected_source_name;
                                
                                // Target Checkbox
                                if ui.checkbox(&mut src.is_target, "").changed() {
                                    sources_modified = true;
                                    save_requested = true;
                                }

                                // Visible Checkbox
                                let vis_icon = if src.is_visible { "👁" } else { "🚫" };
                                if ui.checkbox(&mut src.is_visible, vis_icon).changed() {
                                    sources_modified = true;
                                    save_requested = true;
                                    crate::queue_studio_transform_update(&src);
                                }

                                // Name / Selection Button
                                let label_text = if is_selected {
                                    egui::RichText::new(&src.name).color(Color32::from_rgb(245, 158, 11)).strong()
                                } else if !src.is_visible {
                                    egui::RichText::new(&src.name).color(Color32::from_rgb(100, 116, 139))
                                } else {
                                    egui::RichText::new(&src.name)
                                } ;
                                if ui.button(label_text).clicked() {
                                    source_to_select = Some(src.name.clone());
                                }

                                // Position summary
                                ui.label(format!("{:.0},{:.0}", src.x, src.y));
                                ui.end_row();
                            }
                        });
                });

                if let Some(name) = source_to_select {
                    self.selected_source_name = name.clone();
                    *state.studio_selected_source.write() = name;
                }

                // Selected Target Transform Inspector
                if !self.selected_source_name.is_empty() {
                    ui.separator();
                    
                    if let Some(src) = sources.iter_mut().find(|s| s.name == self.selected_source_name) {
                        let current_scene = state.studio_scene_name.read().clone();
                        ui.horizontal(|ui| {
                            ui.heading(format!("⚙️ {}", self.selected_source_name));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.button(
                                    egui::RichText::new("🔄 Reset")
                                        .color(Color32::from_rgb(251, 191, 36))
                                        .strong()
                                ).on_hover_text("Reset all Studio transformations back to original OBS source position and scale").clicked() {
                                    crate::obs_source::reset_studio_source_to_obs(&current_scene, src);
                                    sources_modified = true;
                                    save_requested = true;
                                    self.status_msg = format!("Reset '{}' to OBS transform", src.name);
                                    ctx.request_repaint();
                                }
                            });
                        });
                        
                        ui.horizontal(|ui| {
                            let vis_text = if src.is_visible { "👁 Visible" } else { "🚫 Hidden" };
                            if ui.checkbox(&mut src.is_visible, vis_text).changed() {
                                sources_modified = true;
                                save_requested = true;
                                crate::queue_studio_transform_update(&src);
                            }
                            if ui.checkbox(&mut src.is_target, "🎯 Target").changed() {
                                sources_modified = true;
                                save_requested = true;
                            }
                            if ui.checkbox(&mut src.match_obs, "🔗 Sync OBS").on_hover_text("When checked, transformations in Studio immediately move the item in OBS").changed() {
                                sources_modified = true;
                                save_requested = true;
                                crate::queue_studio_transform_update(&src);
                            }
                        });
                        if src.is_target {
                            // How targets' outlines are scanned: in the Settings tab (all targets)
                            ui.horizontal(|ui| {
                                if ui.button("🎭 Mask settings…").on_hover_text("How finely, how often and by what targets' outlines are found (Settings tab)").clicked() {
                                    self.active_tab = StudioActiveTab::Settings;
                                }
                                let scanned = state.target_textures.try_read().and_then(|t| t.get(&src.name).map(|t| (t.width, t.height)));
                                if let Some((w, h)) = scanned {
                                    ui.label(egui::RichText::new(format!("scanned at {}×{}", w, h)).small().color(Color32::from_rgb(148, 163, 184)));
                                }
                            });
                        }
                        ui.horizontal(|ui| {
                            ui.label("X:");
                            if ui.add(egui::DragValue::new(&mut src.x).speed(1.0)).changed() { sources_modified = true; }
                            ui.label("Y:");
                            if ui.add(egui::DragValue::new(&mut src.y).speed(1.0)).changed() { sources_modified = true; }
                        });
                        ui.horizontal(|ui| {
                            ui.label("W:");
                            if ui.add(egui::DragValue::new(&mut src.width).speed(1.0).range(10.0..=5000.0)).changed() { sources_modified = true; }
                            ui.label("H:");
                            if ui.add(egui::DragValue::new(&mut src.height).speed(1.0).range(10.0..=5000.0)).changed() { sources_modified = true; }
                        });
                        ui.horizontal(|ui| {
                            ui.colored_label(Color32::from_rgb(59, 130, 246), "Roll:");
                            if ui.add(egui::DragValue::new(&mut src.roll).speed(1.0).suffix("°")).changed() { sources_modified = true; }
                            ui.colored_label(Color32::from_rgb(34, 197, 94), "Pitch:");
                            if ui.add(egui::DragValue::new(&mut src.pitch).speed(1.0).suffix("°")).changed() { sources_modified = true; }
                            ui.colored_label(Color32::from_rgb(239, 68, 68), "Yaw:");
                            if ui.add(egui::DragValue::new(&mut src.yaw).speed(1.0).suffix("°")).changed() { sources_modified = true; }
                        });

                        ui.add_space(6.0);
                        let full_reset_btn = ui.add_sized(
                            [ui.available_width(), 28.0],
                            egui::Button::new(
                                egui::RichText::new("🔄 Reset to OBS Position & Scale")
                                    .color(Color32::from_rgb(251, 191, 36))
                                    .strong()
                            )
                            .fill(Color32::from_rgb(30, 41, 59))
                            .stroke(Stroke::new(1.0, Color32::from_rgb(217, 119, 6)))
                        );
                        if full_reset_btn.on_hover_text(format!(
                            "Reset position to ({:.0}, {:.0}), size to {:.0}x{:.0}, roll to {:.1}°, and 3D pitch/yaw to 0°",
                            src.default_x, src.default_y, src.default_width, src.default_height, src.default_roll
                        )).clicked() {
                            crate::obs_source::reset_studio_source_to_obs(&current_scene, src);
                            sources_modified = true;
                            save_requested = true;
                            self.status_msg = format!("Reset '{}' to OBS transform", src.name);
                            ctx.request_repaint();
                        }
                    }
                }

                // 2.5 🎨 Source Filters & Effects
                if !self.selected_source_name.is_empty() {
                    ui.add_space(8.0);
                    ui.separator();

                    let now_ms = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_millis() as u64)
                        .unwrap_or(0);

                    // 🔊 Audio: the selected source's mixer volume — what a pose's
                    // 💾 Save / ➕ Add Set captures. Re-read so OBS mixer changes show up.
                    if self.selected_source_name != self.last_volume_query_source
                        || now_ms.saturating_sub(self.last_volume_query_time) > 500 {
                        self.last_volume_query_source = self.selected_source_name.clone();
                        self.last_volume_query_time = now_ms;
                        self.source_volume_cache = crate::obs_source::query_source_volume(&self.selected_source_name);
                    }
                    if let Some(volume) = self.source_volume_cache {
                        egui::CollapsingHeader::new(
                            egui::RichText::new("🔊 Audio")
                                .color(Color32::from_rgb(134, 239, 172))
                                .strong()
                        )
                        .default_open(true)
                        .show(ui, |ui| {
                            let mut db = volume_to_db(volume);
                            // Boosted sources (above 0 dB) keep their level instead of being clamped down
                            let max_db = db.max(0.0);
                            ui.horizontal(|ui| {
                                ui.label("Volume:");
                                let shown_db = db;
                                let response = ui.add(volume_slider(&mut db, max_db));
                                // Only a real edit goes to OBS — never the level just read back from it
                                if response.changed() && (db - shown_db).abs() > 0.005 {
                                    let new_volume = db_to_volume(db);
                                    crate::obs_source::set_source_volume(&self.selected_source_name, new_volume);
                                    self.source_volume_cache = Some(new_volume);
                                    self.last_volume_query_time = now_ms;
                                }
                                ui.label(
                                    egui::RichText::new(format!("{:.0}%", db_to_volume(db) * 100.0))
                                        .color(Color32::from_rgb(148, 163, 184)),
                                );
                            });
                            crate::obs_source::watch_audio_levels(&self.selected_source_name);
                            draw_audio_meter(ui, &crate::obs_source::audio_levels());
                            ui.label(
                                egui::RichText::new("Saved into a pose by its 💾 Save, or by ➕ Add Set")
                                    .small()
                                    .color(Color32::from_rgb(100, 116, 139)),
                            );
                        });
                        ui.add_space(4.0);
                    } else {
                        crate::obs_source::stop_audio_levels();
                    }

                    // Keep the list in step with OBS (its own filter properties,
                    // filter scripts, pose moves) — but not mid-drag, which would
                    // pull the slider out from under the pointer
                    let dragging = ui.input(|i| i.pointer.any_down());
                    if self.selected_source_name != self.last_filters_query_source
                        || (!dragging && now_ms.saturating_sub(self.last_filters_query_time) > FILTER_LIST_REFRESH_MS) {
                        self.last_filters_query_source = self.selected_source_name.clone();
                        self.last_filters_query_time = now_ms;
                        let flts = studio_filter_list(&self.selected_source_name);
                        if !flts.is_empty() || !self.source_filters_cache.contains_key(&self.selected_source_name) {
                            self.source_filters_cache.insert(self.selected_source_name.clone(), flts);
                        }
                    }

                    // The filters' OBS properties (controls, labels, dropdowns):
                    // read less often than their values, as reading them runs
                    // the filters' own property code, as opening OBS's Filters
                    // window does
                    let filter_names: Vec<String> = self.source_filters_cache
                        .get(&self.selected_source_name)
                        .map(|flts| flts.iter().map(|f| f.name.clone()).collect())
                        .unwrap_or_default();
                    let props_listed = filter_names.len() == self.filter_props.len()
                        && filter_names.iter().all(|n| self.filter_props.contains_key(n));
                    if self.selected_source_name != self.filter_props_source
                        || !props_listed
                        || self.filter_props_stale
                        || (!dragging && now_ms.saturating_sub(self.filter_props_time) > FILTER_PROPS_REFRESH_MS) {
                        self.filter_props_source = self.selected_source_name.clone();
                        self.filter_props_time = now_ms;
                        self.filter_props_stale = false;
                        self.filter_props = filter_names
                            .iter()
                            .map(|n| (n.clone(), crate::filter_props::query(&self.selected_source_name, n)))
                            .collect();
                    }

                    let filters_opt = self.source_filters_cache.get_mut(&self.selected_source_name);
                    let has_filters = filters_opt.as_ref().map(|f| !f.is_empty()).unwrap_or(false);

                    egui::CollapsingHeader::new(
                        egui::RichText::new("🎨 Source Filters & Effects")
                            .color(Color32::from_rgb(147, 197, 253))
                            .strong()
                    )
                    .default_open(true)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            if ui.button("🔄 Refresh Filters").clicked() {
                                let flts = studio_filter_list(&self.selected_source_name);
                                self.source_filters_cache.insert(self.selected_source_name.clone(), flts);
                                self.filter_props_stale = true;
                            }
                            if !has_filters {
                                ui.label(egui::RichText::new("No active filters detected on this source").italics().color(Color32::from_rgb(148, 163, 184)));
                            }
                        });

                        if let Some(flts) = self.source_filters_cache.get_mut(&self.selected_source_name) {
                            for flt in flts.iter_mut() {
                                ui.add_space(2.0);
                                let is_360 = flt.id.contains("360") || flt.name.contains("360");
                                let header_icon = if is_360 { "🌐" } else { "🔹" };

                                egui::CollapsingHeader::new(format!("{} {}", header_icon, flt.name))
                                    .default_open(is_360)
                                    .show(ui, |ui| {
                                        ui.horizontal(|ui| {
                                            let vis_lbl = if flt.enabled { "👁 Enabled" } else { "🚫 Disabled" };
                                            if ui.checkbox(&mut flt.enabled, vis_lbl).changed() {
                                                crate::obs_source::set_source_filter_enabled(
                                                    &self.selected_source_name,
                                                    &flt.name,
                                                    flt.enabled,
                                                );
                                                save_requested = true;
                                            }
                                        });

                                        let mut edited: Option<(String, serde_json::Value)> = None;
                                        match self.filter_props.get(&flt.name) {
                                            // As OBS's Filters window shows it
                                            Some(Some(props)) => {
                                                edited = filter_props_ui(ui, &flt.name, props, &flt.settings);
                                            }
                                            // A filter with no properties: its raw settings
                                            _ => {
                                                if let serde_json::Value::Object(ref map) = flt.settings {
                                                    for (k, v) in map.iter() {
                                                        if k == "debug" { continue; }
                                                        ui.horizontal(|ui| {
                                                            ui.label(filter_setting_label(k));
                                                            if let Some(new_v) = filter_setting_widget(ui, k, v) {
                                                                edited = Some((k.clone(), new_v));
                                                            }
                                                        });
                                                    }
                                                }
                                            }
                                        }

                                        if let Some((key, value)) = edited {
                                            if let Some(map) = flt.settings.as_object_mut() {
                                                map.insert(key.clone(), value.clone());
                                            }
                                            match crate::filter_props::set(&self.selected_source_name, &flt.name, &key, &value) {
                                                Some(refresh) => self.filter_props_stale |= refresh,
                                                None => write_filter_setting(&self.selected_source_name, flt, &key, value),
                                            }
                                            save_requested = true;
                                        }
                                    });
                            }
                        }
                    });
                }

                // 3. Coordinate Sets (All Sources in Scene) — one pose is edited
                // at a time: ✏ Edit moves every source to it, and picking another
                // pose first asks whether to save its unsaved changes.
                ui.add_space(8.0);
                ui.separator();
                let current_scene = state.studio_scene_name.read().clone();
                let scene_name = if current_scene.is_empty() { "Scène".to_string() } else { current_scene };
                let now_ms = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or(0);
                self.update_editing_pose(&scene_name, &sources, now_ms);

                ui.horizontal(|ui| {
                    ui.heading("📍 Coordinate Sets");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(
                            egui::RichText::new("➕ Add Set")
                                .color(Color32::from_rgb(34, 197, 94))
                                .strong()
                        ).on_hover_text("Snapshot coordinates of ALL sources in this scene into a new coordinate set, and edit it").clicked() {
                            let count = SCENE_COORDINATE_SETS.read().iter().filter(|s| s.scene == scene_name).count();
                            let new_set = SceneCoordinateSet {
                                id: format!("set_{}_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis(), count + 1),
                                name: format!("Pose {}", count + 1),
                                trigger: String::new(),
                                scene: scene_name.clone(),
                                easing: MovementEasing::EaseOut,
                                duration_ms: 800,
                                sources: snapshot_all_sources(&sources),
                            };
                            crate::obs_source::trust_log(&format!(
                                "[Trust Studio] ➕ Added pose '{}' for scene '{}' ({} sources)",
                                new_set.name, new_set.scene, new_set.sources.len()
                            ));
                            // The new pose is the current layout, so it's edited from here
                            self.editing_pose = Some(PoseEditState::new(new_set.id.clone(), last_applied_pose().0, Some(new_set.sources.clone())));
                            SCENE_COORDINATE_SETS.write().push(new_set);
                            save_requested = true;
                            self.status_msg = "Added new coordinate set for all sources".to_string();
                        }
                    });
                });

                let mut sets = SCENE_COORDINATE_SETS.read().clone();
                let mut sets_changed = false;
                let mut set_to_delete: Option<usize> = None;
                let mut pose_to_edit: Option<String> = None;
                let mut pose_to_revert: Option<String> = None;
                let mut editing_saved: Option<Vec<SourceTransformSnapshot>> = None;
                let editing_id = self.editing_pose.as_ref().map(|e| e.id.clone());
                // (reached the pose, has unsaved changes)
                let editing_status = self.editing_pose.as_ref().map(|e| (e.baseline.is_some(), e.dirty));

                egui::ScrollArea::vertical()
                    .id_salt("coord_sets_scroll_area")
                    .max_height(280.0)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let has_sets = sets.iter().any(|s| s.scene == scene_name);
                        if !has_sets {
                            ui.vertical_centered(|ui| {
                                ui.add_space(8.0);
                                ui.label(egui::RichText::new("No coordinate sets for this scene yet.").italics().color(Color32::from_rgb(148, 163, 184)));
                                ui.label(egui::RichText::new("Click '+ Add Set' to snapshot all sources.").small().color(Color32::from_rgb(100, 116, 139)));
                                ui.add_space(8.0);
                            });
                        } else if editing_id.is_none() {
                            ui.label(egui::RichText::new("Click ✏ Edit on a pose to change it.").small().color(Color32::from_rgb(100, 116, 139)));
                            ui.add_space(4.0);
                        }

                        for (idx, set) in sets.iter_mut().enumerate() {
                            if set.scene != scene_name && !set.scene.is_empty() {
                                continue;
                            }

                            // A pose that isn't being edited: one compact row
                            if editing_id.as_deref() != Some(set.id.as_str()) {
                                egui::Frame::NONE
                                    .fill(Color32::from_rgb(24, 30, 42))
                                    .stroke(Stroke::new(1.0, Color32::from_rgb(51, 65, 85)))
                                    .inner_margin(egui::Margin::same(8))
                                    .corner_radius(6.0)
                                    .show(ui, |ui| {
                                        ui.horizontal(|ui| {
                                            ui.label(egui::RichText::new(&set.name).strong());
                                            if !set.trigger.trim().is_empty() {
                                                ui.label(
                                                    egui::RichText::new(format!("⚡ {}", set.trigger.trim()))
                                                        .small()
                                                        .color(Color32::from_rgb(148, 163, 184))
                                                );
                                            }
                                            ui.label(
                                                egui::RichText::new(format!("📸 {}", set.sources.len()))
                                                    .small()
                                                    .color(Color32::from_rgb(100, 116, 139))
                                            );
                                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                                if ui.button(
                                                    egui::RichText::new("✏ Edit")
                                                        .color(Color32::from_rgb(59, 130, 246))
                                                        .strong()
                                                ).on_hover_text(format!("Move all sources to this pose ({} ms, {}) and edit it", set.duration_ms, set.easing.name())).clicked() {
                                                    pose_to_edit = Some(set.id.clone());
                                                }
                                            });
                                        });
                                    });
                                ui.add_space(4.0);
                                continue;
                            }

                            // The pose being edited
                            egui::Frame::NONE
                                .fill(Color32::from_rgb(30, 38, 54))
                                .stroke(Stroke::new(1.5, Color32::from_rgb(59, 130, 246)))
                                .inner_margin(egui::Margin::same(8))
                                .corner_radius(6.0)
                                .show(ui, |ui| {
                                    // Row 1: Name and Delete button
                                    ui.horizontal(|ui| {
                                        ui.label("🏷 Name:");
                                        if ui.add(egui::TextEdit::singleline(&mut set.name).desired_width(140.0)).changed() {
                                            sets_changed = true;
                                            save_requested = true;
                                        }
                                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                            if ui.button(
                                                egui::RichText::new("🗑 Delete")
                                                    .color(Color32::from_rgb(239, 68, 68))
                                            ).on_hover_text("Delete this coordinate set").clicked() {
                                                set_to_delete = Some(idx);
                                            }
                                        });
                                    });

                                    // Row 2: its trigger, from lists
                                    ui.label("⚡ Trigger:");
                                    let mut trigger: Vec<String> = Some(set.trigger.trim().to_string()).filter(|t| !t.is_empty()).into_iter().collect();
                                    if crate::trigger_builder::triggers_ui(ui, ("pose_trigger", set.id.clone()), &mut trigger, 1) {
                                        set.trigger = trigger.into_iter().next().unwrap_or_default();
                                        sets_changed = true;
                                        save_requested = true;
                                    }

                                    // Row 3: Movement Easing dropdown (English names) & Speed slider
                                    ui.horizontal(|ui| {
                                        ui.label("🔄 Move:");
                                        let prev_easing = set.easing;
                                        egui::ComboBox::from_id_salt(format!("easing_combo_{}", set.id))
                                            .selected_text(set.easing.name())
                                            .width(110.0)
                                            .show_ui(ui, |ui| {
                                                for easing in [
                                                    MovementEasing::EaseOut,
                                                    MovementEasing::EaseIn,
                                                    MovementEasing::EaseInOut,
                                                    MovementEasing::Linear,
                                                    MovementEasing::Bounce,
                                                    MovementEasing::Instant,
                                                ] {
                                                    ui.selectable_value(&mut set.easing, easing, easing.name());
                                                }
                                            });
                                        if set.easing != prev_easing {
                                            sets_changed = true;
                                            save_requested = true;
                                        }

                                        ui.add_space(4.0);
                                        ui.label("⏱");
                                        if ui.add(
                                            egui::Slider::new(&mut set.duration_ms, 100..=5000)
                                                .suffix("ms")
                                                .show_value(true)
                                        ).changed() {
                                            sets_changed = true;
                                            save_requested = true;
                                        }
                                    });

                                    // Row 4: Edit state, sources count, ↩ Revert and 💾 Save buttons
                                    ui.horizontal(|ui| {
                                        let (state_text, state_color) = match editing_status {
                                            Some((false, _)) => ("⏳ Moving to pose…", Color32::from_rgb(148, 163, 184)),
                                            Some((true, true)) => ("● Unsaved changes", Color32::from_rgb(251, 191, 36)),
                                            _ => ("✔ Saved", Color32::from_rgb(34, 197, 94)),
                                        };
                                        ui.label(egui::RichText::new(state_text).small().strong().color(state_color));
                                        ui.label(
                                            egui::RichText::new(format!("📸 {} sources", set.sources.len()))
                                                .small()
                                                .color(Color32::from_rgb(148, 163, 184))
                                        );

                                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                            if ui.button(
                                                egui::RichText::new("💾 Save")
                                                    .color(Color32::from_rgb(251, 191, 36))
                                                    .strong()
                                            ).on_hover_text("Overwrite this set with current coordinates of all sources in this scene").clicked() {
                                                set.sources = snapshot_all_sources(&sources);
                                                crate::obs_source::trust_log(&format!(
                                                    "[Trust Studio] 💾 Re-saved pose '{}' for scene '{}' ({} sources)",
                                                    set.name, set.scene, set.sources.len()
                                                ));
                                                editing_saved = Some(set.sources.clone());
                                                sets_changed = true;
                                                save_requested = true;
                                                self.status_msg = format!("Saved coordinates for all sources to '{}'", set.name);
                                            }

                                            if matches!(editing_status, Some((true, true)))
                                                && ui.button(
                                                    egui::RichText::new("↩ Revert")
                                                        .color(Color32::from_rgb(203, 213, 225))
                                                ).on_hover_text("Drop the unsaved changes: move all sources back to the saved pose").clicked()
                                            {
                                                pose_to_revert = Some(set.id.clone());
                                            }
                                        });
                                    });
                                });
                            ui.add_space(4.0);
                        }
                    });

                if let Some(idx) = set_to_delete {
                    sets.remove(idx);
                    sets_changed = true;
                    save_requested = true;
                    self.status_msg = "Deleted coordinate set".to_string();
                }

                if sets_changed {
                    *SCENE_COORDINATE_SETS.write() = sets;
                }

                if let Some(snapshot) = editing_saved {
                    if let Some(edit) = self.editing_pose.as_mut() {
                        edit.baseline = Some(snapshot);
                        edit.dirty = false;
                    }
                }
                if let Some(id) = pose_to_revert {
                    self.start_editing_pose(&id, &sources);
                    self.status_msg = "Reverted the unsaved changes".to_string();
                    ctx.request_repaint();
                }
                if let Some(id) = pose_to_edit {
                    self.request_pose_switch(id, &sources);
                    ctx.request_repaint();
                }

                // Leaving a pose with unsaved changes: save them or not?
                if let Some(target_id) = self.pending_pose_switch.clone() {
                    let name_of = |id: &str| {
                        SCENE_COORDINATE_SETS.read().iter().find(|s| s.id == id).map(|s| s.name.clone()).unwrap_or_default()
                    };
                    let edited_name = self.editing_pose.as_ref().map(|e| name_of(&e.id)).unwrap_or_default();
                    let target_name = name_of(&target_id);
                    let mut save_answer: Option<bool> = None;
                    let mut cancelled = false;
                    let modal = egui::Modal::new(egui::Id::new("pose_switch_prompt")).show(ctx, |ui| {
                        ui.set_width(340.0);
                        ui.heading("Unsaved changes");
                        ui.add_space(6.0);
                        ui.label(format!(
                            "'{}' has changes that aren't saved. Save them before switching to '{}'?",
                            edited_name, target_name
                        ));
                        ui.add_space(12.0);
                        ui.horizontal(|ui| {
                            if ui.button(
                                egui::RichText::new("💾 Save")
                                    .color(Color32::from_rgb(251, 191, 36))
                                    .strong()
                            ).clicked() {
                                save_answer = Some(true);
                            }
                            if ui.button("Don't save").clicked() {
                                save_answer = Some(false);
                            }
                            if ui.button("Cancel").clicked() {
                                cancelled = true;
                            }
                        });
                    });
                    if let Some(save) = save_answer {
                        if save && self.save_editing_pose(&sources) {
                            save_requested = true;
                        }
                        self.pending_pose_switch = None;
                        self.start_editing_pose(&target_id, &sources);
                        ctx.request_repaint();
                    } else if cancelled || modal.should_close() {
                        self.pending_pose_switch = None;
                    }
                }
            });

        // 3. Bottom Status & Modifier Keys Bar (Under Render)
        let is_ctrl_active = ctx.input(|i| i.modifiers.ctrl);
        let is_shift_active = ctx.input(|i| i.modifiers.shift);

        egui::TopBottomPanel::bottom("studio_bottom_bar")
            .exact_height(34.0)
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;

                    // 1. Ctrl Snap Badge
                    let (ctrl_text, ctrl_bg, ctrl_border, ctrl_fg) = if is_ctrl_active {
                        (
                            "⚡ Ctrl: Snap ON (10px / 5°)",
                            Color32::from_rgb(14, 116, 144),        // Cyan-700
                            Color32::from_rgb(34, 211, 238),        // Cyan-400
                            Color32::WHITE,
                        )
                    } else {
                        (
                            "Ctrl: Snap (10px / 5°)",
                            Color32::from_rgb(30, 41, 59),          // Slate-800
                            Color32::from_rgb(51, 65, 85),          // Slate-700
                            Color32::from_rgb(148, 163, 184),       // Slate-400
                        )
                    };

                    egui::Frame::NONE
                        .fill(ctrl_bg)
                        .stroke(Stroke::new(1.0, ctrl_border))
                        .corner_radius(4.0)
                        .inner_margin(egui::Margin::symmetric(8, 4))
                        .show(ui, |ui| {
                            ui.label(egui::RichText::new(ctrl_text).color(ctrl_fg).size(11.5).strong());
                        });

                    // 2. Shift Precision Badge
                    let (shift_text, shift_bg, shift_border, shift_fg) = if is_shift_active {
                        (
                            "🎯 Shift: Precision ON (0.1x)",
                            Color32::from_rgb(180, 83, 9),          // Amber-700
                            Color32::from_rgb(251, 191, 36),        // Amber-400
                            Color32::WHITE,
                        )
                    } else {
                        (
                            "Shift: Precision (0.1x)",
                            Color32::from_rgb(30, 41, 59),          // Slate-800
                            Color32::from_rgb(51, 65, 85),          // Slate-700
                            Color32::from_rgb(148, 163, 184),       // Slate-400
                        )
                    };

                    egui::Frame::NONE
                        .fill(shift_bg)
                        .stroke(Stroke::new(1.0, shift_border))
                        .corner_radius(4.0)
                        .inner_margin(egui::Margin::symmetric(8, 4))
                        .show(ui, |ui| {
                            ui.label(egui::RichText::new(shift_text).color(shift_fg).size(11.5).strong());
                        });

                    ui.separator();

                    ui.label(
                        egui::RichText::new("Gizmo: ⚪ Move  ⭕ Scale  🔵 Roll  🔴 Yaw  🟢 Pitch")
                            .size(11.0)
                            .color(Color32::from_rgb(100, 116, 139)),
                    );
                });
            });

        // 4. Central Live Scene Preview Viewport + 3D Gizmo
        egui::CentralPanel::default().show(ctx, |ui| {
            let available_size = ui.available_size();
            let (rect, response) = ui.allocate_exact_size(available_size, egui::Sense::click_and_drag());
            let painter = ui.painter_at(rect);

            // Calculate 16:9 or aspect-fit inner viewport inside available space
            let canvas_w = cw.max(1) as f32;
            let canvas_h = ch.max(1) as f32;
            let aspect = canvas_w / canvas_h;

            let draw_w = (rect.width()).min(rect.height() * aspect);
            let draw_h = draw_w / aspect;
            let off_x = rect.min.x + (rect.width() - draw_w) / 2.0;
            let off_y = rect.min.y + (rect.height() - draw_h) / 2.0;
            let vp_rect = Rect::from_min_size(Pos2::new(off_x, off_y), Vec2::new(draw_w, draw_h));

            // Viewport background
            painter.rect_filled(rect, 0.0, Color32::from_rgb(15, 23, 42)); // Dark slate panel surround
            painter.rect_filled(vp_rect, 0.0, Color32::BLACK); // Solid black render background
            painter.rect_stroke(vp_rect, 0.0, Stroke::new(1.0, Color32::from_rgb(51, 65, 85)), egui::StrokeKind::Outside);
            // Draw composite texture or offline background sources fallback
            if let Some(ref tex) = self.preview_texture {
                painter.image(
                    tex.id(),
                    vp_rect,
                    Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(1.0, 1.0)),
                    Color32::WHITE,
                );
            } else {
                // Subtle grid
                for x in (0..(draw_w as i32)).step_by(40) {
                    painter.line_segment(
                        [Pos2::new(off_x + x as f32, off_y), Pos2::new(off_x + x as f32, off_y + draw_h)],
                        Stroke::new(1.0, Color32::from_rgb(30, 41, 59)),
                    );
                }
                for y in (0..(draw_h as i32)).step_by(40) {
                    painter.line_segment(
                        [Pos2::new(off_x, off_y + y as f32), Pos2::new(off_x + draw_w, off_y + y as f32)],
                        Stroke::new(1.0, Color32::from_rgb(30, 41, 59)),
                    );
                }
            }

            if is_throwable_inactive {
                painter.rect_filled(vp_rect, 0.0, Color32::from_rgba_unmultiplied(10, 15, 26, 175));
            }

            // Draw targets & Interactive 3D Gizmo
            let scale_x = draw_w / canvas_w;
            let scale_y = draw_h / canvas_h;
            let preview_d = (1000.0 * scale_y).max(50.0);

            let pointer_pos = response.hover_pos();
            let is_drag = response.dragged() && !is_throwable_inactive;
            let is_ctrl = ui.input(|i| i.modifiers.ctrl);
            let is_shift = ui.input(|i| i.modifiers.shift);
            let speed_mult = if is_shift { 0.1 } else { 1.0 };

            let selected_name = self.selected_source_name.clone();

            // Render all sources (targets and non-targets alike)
            for src in sources.iter() {
                let is_sel = !selected_name.is_empty() && src.name == selected_name;
                if !src.is_visible { continue; }

                let sx = off_x + src.x * scale_x;
                let sy = off_y + src.y * scale_y;
                let sw = (src.width * scale_x).max(10.0);
                let sh = (src.height * scale_y).max(10.0);
                let scx = sx + sw / 2.0;
                let scy = sy + sh / 2.0;

                // --- 🎯 Render Collision Mask & Hitbox Overlay ---
                if self.show_hitboxes && src.is_visible && src.is_target {
                    let textures = state.target_textures.read();
                    if let Some(tex) = textures.get(&src.name) {
                        let last_id = self.last_target_frame_ids.get(&src.name).copied().unwrap_or(0);
                        let need_load = !self.mask_textures.contains_key(&src.name) || (tex.frame_id > 0 && tex.frame_id != last_id);
                        if need_load && tex.width > 0 && tex.height > 0 && !tex.rgba.is_empty() {
                            let src_color = color_for_source_name(&src.name);
                            let mut mask_rgba = vec![0u8; (tex.width * tex.height * 4) as usize];
                            for (idx, chunk) in tex.rgba.chunks_exact(4).enumerate() {
                                let a = chunk[3];
                                let dst_idx = idx * 4;
                                if a > 15 {
                                    mask_rgba[dst_idx] = src_color.r();
                                    mask_rgba[dst_idx + 1] = src_color.g();
                                    mask_rgba[dst_idx + 2] = src_color.b();
                                    mask_rgba[dst_idx + 3] = 160;  // Semi-transparent
                                }
                            }
                            let color_img = egui::ColorImage::from_rgba_unmultiplied(
                                [tex.width as usize, tex.height as usize],
                                &mask_rgba,
                            );
                            let handle = ctx.load_texture(
                                format!("mask_{}", src.name),
                                color_img,
                                egui::TextureOptions::LINEAR,
                            );
                            self.mask_textures.insert(src.name.clone(), handle);
                            self.last_target_frame_ids.insert(src.name.clone(), tex.frame_id);
                        }
                    }
                    drop(textures);

                    let (pad_ratio_x, pad_ratio_y, is_reacted) = {
                        let textures = state.target_textures.read();
                        textures.get(&src.name).map(|t| (t.pad_ratio_x(), t.pad_ratio_y(), t.is_padded_reaction())).unwrap_or((1.0, 1.0, false))
                    };

                    let mesh_w = sw * pad_ratio_x;
                    let mesh_h = sh * pad_ratio_y;
                    let (mesh_roll, mesh_pitch, mesh_yaw) = if is_reacted {
                        (0.0, 0.0, 0.0)
                    } else {
                        (src.roll, src.pitch, src.yaw)
                    };

                    // Draw silhouette mask in 3D perspective quad mesh if available, or tinted hitbox fill
                    let n = 8;
                    if let Some(mask_tex) = self.mask_textures.get(&src.name) {
                        let tint = if is_sel { Color32::from_rgb(254, 240, 138) } else { Color32::WHITE };
                        let mut mesh = egui::Mesh::with_texture(mask_tex.id());
                        for iy in 0..=n {
                            let v = iy as f32 / n as f32;
                            let ly = (v - 0.5) * mesh_h;
                            for ix in 0..=n {
                                let u = ix as f32 / n as f32;
                                let lx = (u - 0.5) * mesh_w;
                                let pos = project_3d_point(lx, ly, scx, scy, mesh_roll, mesh_pitch, mesh_yaw, preview_d);
                                mesh.vertices.push(egui::epaint::Vertex {
                                    pos,
                                    uv: Pos2::new(u, v),
                                    color: tint,
                                });
                            }
                        }
                        for iy in 0..n {
                            for ix in 0..n {
                                let i0 = (iy * (n + 1) + ix) as u32;
                                let i1 = i0 + 1;
                                let i2 = ((iy + 1) * (n + 1) + ix) as u32;
                                let i3 = i2 + 1;
                                mesh.indices.extend_from_slice(&[i0, i1, i2, i1, i3, i2]);
                            }
                        }
                        painter.add(egui::Shape::mesh(mesh));
                    } else {
                        let fill_col = if is_sel {
                            Color32::from_rgba_unmultiplied(245, 158, 11, 60)
                        } else {
                            let c = color_for_source_name(&src.name);
                            Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), 45)
                        };
                        let mut mesh = egui::Mesh::default();
                        for iy in 0..=n {
                            let v = iy as f32 / n as f32;
                            let ly = (v - 0.5) * sh;
                            for ix in 0..=n {
                                let u = ix as f32 / n as f32;
                                let lx = (u - 0.5) * sw;
                                let pos = project_3d_point(lx, ly, scx, scy, src.roll, src.pitch, src.yaw, preview_d);
                                mesh.vertices.push(egui::epaint::Vertex {
                                    pos,
                                    uv: Pos2::ZERO,
                                    color: fill_col,
                                });
                            }
                        }
                        for iy in 0..n {
                            for ix in 0..n {
                                let i0 = (iy * (n + 1) + ix) as u32;
                                let i1 = i0 + 1;
                                let i2 = ((iy + 1) * (n + 1) + ix) as u32;
                                let i3 = i2 + 1;
                                mesh.indices.extend_from_slice(&[i0, i1, i2, i1, i3, i2]);
                            }
                        }
                        painter.add(egui::Shape::mesh(mesh));
                    }

                    // Compute calculated hitbox 3D corners and center of mass reticle
                    let hitboxes = state.hitboxes.read();
                    let hb_opt = hitboxes.get(&src.name).copied();
                    drop(hitboxes);
                    // The reticle marks the mask's center of mass — the point effects aim at
                    let mask_local_center = state.masks.read().get(&src.name).and_then(|m| m.local_center);

                    let (h_lx0, h_lx1, h_ly0, h_ly1, reticle_lx, reticle_ly) = if let Some(ref hb) = hb_opt {
                        if hb.active && hb.local_width > 0.0 && hb.local_height > 0.0 {
                            let (tex_w, tex_h) = {
                                let textures = state.target_textures.read();
                                textures.get(&src.name).map(|t| (t.width as f32, t.height as f32)).unwrap_or((src.width, src.height))
                            };
                            let tw = tex_w.max(1.0);
                            let th = tex_h.max(1.0);
                            let u0 = hb.local_x / tw;
                            let u1 = (hb.local_x + hb.local_width) / tw;
                            let v0 = hb.local_y / th;
                            let v1 = (hb.local_y + hb.local_height) / th;
                            let (uc, vc) = mask_local_center
                                .map(|(cx, cy)| (cx / tw, cy / th))
                                .unwrap_or(((u0 + u1) * 0.5, (v0 + v1) * 0.5));
                            (
                                (u0 - 0.5) * mesh_w,
                                (u1 - 0.5) * mesh_w,
                                (v0 - 0.5) * mesh_h,
                                (v1 - 0.5) * mesh_h,
                                (uc - 0.5) * mesh_w,
                                (vc - 0.5) * mesh_h,
                            )
                        } else {
                            (-mesh_w / 2.0, mesh_w / 2.0, -mesh_h / 2.0, mesh_h / 2.0, 0.0, 0.0)
                        }
                    } else {
                        (-mesh_w / 2.0, mesh_w / 2.0, -mesh_h / 2.0, mesh_h / 2.0, 0.0, 0.0)
                    };

                    let hc0 = project_3d_point(h_lx0, h_ly0, scx, scy, mesh_roll, mesh_pitch, mesh_yaw, preview_d);
                    let hc1 = project_3d_point(h_lx1, h_ly0, scx, scy, mesh_roll, mesh_pitch, mesh_yaw, preview_d);
                    let hc2 = project_3d_point(h_lx1, h_ly1, scx, scy, mesh_roll, mesh_pitch, mesh_yaw, preview_d);
                    let hc3 = project_3d_point(h_lx0, h_ly1, scx, scy, mesh_roll, mesh_pitch, mesh_yaw, preview_d);
                    let reticle_pos = project_3d_point(reticle_lx, reticle_ly, scx, scy, mesh_roll, mesh_pitch, mesh_yaw, preview_d);

                    // Draw calculated hitbox bounds
                    let hb_stroke = Stroke::new(
                        1.2,
                        if is_sel {
                            Color32::from_rgb(251, 191, 36)
                        } else {
                            let c = color_for_source_name(&src.name);
                            Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), 200)
                        },
                    );
                    painter.line_segment([hc0, hc1], hb_stroke);
                    painter.line_segment([hc1, hc2], hb_stroke);
                    painter.line_segment([hc2, hc3], hb_stroke);
                    painter.line_segment([hc3, hc0], hb_stroke);

                    // High-visibility 3D corner brackets around calculated hitbox
                    let bracket_col = if is_sel { Color32::from_rgb(251, 191, 36) } else { Color32::from_rgb(125, 211, 252) };
                    let bw = 2.5;
                    let clen = 16.0f32;

                    let seg_01 = hc1 - hc0;
                    let seg_03 = hc3 - hc0;
                    let len_01 = seg_01.length().max(1e-4);
                    let len_03 = seg_03.length().max(1e-4);
                    let arm_01 = (seg_01 / len_01) * clen.min(len_01 * 0.4);
                    let arm_03 = (seg_03 / len_03) * clen.min(len_03 * 0.4);
                    painter.line_segment([hc0, hc0 + arm_01], Stroke::new(bw, bracket_col));
                    painter.line_segment([hc0, hc0 + arm_03], Stroke::new(bw, bracket_col));

                    let seg_10 = hc0 - hc1;
                    let seg_12 = hc2 - hc1;
                    let len_10 = seg_10.length().max(1e-4);
                    let len_12 = seg_12.length().max(1e-4);
                    let arm_10 = (seg_10 / len_10) * clen.min(len_10 * 0.4);
                    let arm_12 = (seg_12 / len_12) * clen.min(len_12 * 0.4);
                    painter.line_segment([hc1, hc1 + arm_10], Stroke::new(bw, bracket_col));
                    painter.line_segment([hc1, hc1 + arm_12], Stroke::new(bw, bracket_col));

                    let seg_21 = hc1 - hc2;
                    let seg_23 = hc3 - hc2;
                    let len_21 = seg_21.length().max(1e-4);
                    let len_23 = seg_23.length().max(1e-4);
                    let arm_21 = (seg_21 / len_21) * clen.min(len_21 * 0.4);
                    let arm_23 = (seg_23 / len_23) * clen.min(len_23 * 0.4);
                    painter.line_segment([hc2, hc2 + arm_21], Stroke::new(bw, bracket_col));
                    painter.line_segment([hc2, hc2 + arm_23], Stroke::new(bw, bracket_col));

                    let seg_32 = hc2 - hc3;
                    let seg_30 = hc0 - hc3;
                    let len_32 = seg_32.length().max(1e-4);
                    let len_30 = seg_30.length().max(1e-4);
                    let arm_32 = (seg_32 / len_32) * clen.min(len_32 * 0.4);
                    let arm_30 = (seg_30 / len_30) * clen.min(len_30 * 0.4);
                    painter.line_segment([hc3, hc3 + arm_32], Stroke::new(bw, bracket_col));
                    painter.line_segment([hc3, hc3 + arm_30], Stroke::new(bw, bracket_col));

                    // Center reticle
                    painter.circle_filled(reticle_pos, 3.0, bracket_col);
                    painter.line_segment([Pos2::new(reticle_pos.x - 7.0, reticle_pos.y), Pos2::new(reticle_pos.x + 7.0, reticle_pos.y)], Stroke::new(1.0, bracket_col));
                    painter.line_segment([Pos2::new(reticle_pos.x, reticle_pos.y - 7.0), Pos2::new(reticle_pos.x, reticle_pos.y + 7.0)], Stroke::new(1.0, bracket_col));
                }

                let stroke_color = if !src.is_visible {
                    Color32::from_rgba_unmultiplied(148, 163, 184, 120) // Dimmed slate for hidden
                } else if is_sel {
                    Color32::from_rgb(245, 158, 11) // Amber
                } else {
                    color_for_source_name(&src.name)
                };

                let label = if !src.is_visible {
                    format!("{} (Hidden)", src.name)
                } else {
                    src.name.clone()
                };

                let frame_p0 = project_3d_point(-sw / 2.0, -sh / 2.0, scx, scy, src.roll, src.pitch, src.yaw, preview_d);
                let frame_p1 = project_3d_point( sw / 2.0, -sh / 2.0, scx, scy, src.roll, src.pitch, src.yaw, preview_d);
                let frame_p2 = project_3d_point( sw / 2.0,  sh / 2.0, scx, scy, src.roll, src.pitch, src.yaw, preview_d);
                let frame_p3 = project_3d_point(-sw / 2.0,  sh / 2.0, scx, scy, src.roll, src.pitch, src.yaw, preview_d);

                let frame_stroke = Stroke::new(if is_sel { 2.5 } else { 1.5 }, stroke_color);
                painter.line_segment([frame_p0, frame_p1], frame_stroke);
                painter.line_segment([frame_p1, frame_p2], frame_stroke);
                painter.line_segment([frame_p2, frame_p3], frame_stroke);
                painter.line_segment([frame_p3, frame_p0], frame_stroke);

                painter.text(frame_p0 + Vec2::new(4.0, 4.0), egui::Align2::LEFT_TOP, &label, egui::FontId::proportional(12.0), stroke_color);
            }


            // Render 3D Gizmo on Selected Target
            if let Some(src) = sources.iter_mut().find(|s| s.name == selected_name) {
                let sx = off_x + src.x * scale_x;
                let sy = off_y + src.y * scale_y;
                let sw = (src.width * scale_x).max(10.0);
                let sh = (src.height * scale_y).max(10.0);
                let scx = sx + sw / 2.0;
                let scy = sy + sh / 2.0;
                let center_pos = Pos2::new(scx, scy);

                // Gizmo Dimensions
                let r_blue = 34.0;
                let r_scale = 48.0;

                // Handle Gizmo Mouse Interaction
                if response.drag_started() {
                    if let Some(p) = pointer_pos {
                        let dx = p.x - scx;
                        let dy = p.y - scy;
                        let dist = (dx * dx + dy * dy).sqrt();

                        let rad = (-src.roll).to_radians();
                        let cos_a = rad.cos();
                        let sin_a = rad.sin();
                        let lx = dx * cos_a - dy * sin_a;
                        let ly = dx * sin_a + dy * cos_a;

                        // 1. White Center Spot (r <= 12) -> PanFree
                        if dist <= 12.0 {
                            self.drag_mode = GizmoDragMode::PanFree;
                        // 2. White Scale Ring (r = 48) -> Scale (generous zone directly on/around white ring)
                        } else if dist >= 41.0 && dist <= 58.0 {
                            self.drag_mode = GizmoDragMode::Scale;
                        // 3. Rotation: Blue Roll Ring & Thick '+' Axis Bars (Red = Yaw, Green = Pitch)
                        } else {
                            let d_blue = (dist - r_blue).abs();

                            // Distance to Red horizontal bar (Yaw: local y = 0, x in [-r_blue, r_blue])
                            let d_red = if lx.abs() <= r_blue {
                                ly.abs()
                            } else {
                                ((lx.abs() - r_blue).powi(2) + ly.powi(2)).sqrt()
                            };

                            // Distance to Green vertical bar (Pitch: local x = 0, y in [-r_blue, r_blue])
                            let d_green = if ly.abs() <= r_blue {
                                lx.abs()
                            } else {
                                (lx.powi(2) + (ly.abs() - r_blue).powi(2)).sqrt()
                            };

                            let min_d = d_blue.min(d_red).min(d_green);
                            if min_d <= 7.5 {
                                if (d_red - min_d).abs() < 1e-3 {
                                    self.drag_mode = GizmoDragMode::RotateYaw;
                                } else if (d_green - min_d).abs() < 1e-3 {
                                    self.drag_mode = GizmoDragMode::RotatePitch;
                                } else {
                                    self.drag_mode = GizmoDragMode::RotateRoll;
                                }
                            } else if is_ctrl && lx.abs() <= sw / 2.0 && ly.abs() <= sh / 2.0 {
                                // Blender Ctrl + Click: Move anywhere inside target
                                self.drag_mode = GizmoDragMode::PanFree;
                            } else {
                                self.drag_mode = GizmoDragMode::None;
                            }
                        }

                        if self.drag_mode != GizmoDragMode::None {
                            self.drag_start_pointer = p;
                            self.drag_start_x = src.x;
                            self.drag_start_y = src.y;
                            self.drag_start_w = src.width;
                            self.drag_start_h = src.height;
                            self.drag_start_roll = src.roll;
                            self.drag_start_pitch = src.pitch;
                            self.drag_start_yaw = src.yaw;
                        }
                    }
                }

                // Dragging in Progress
                if is_drag && self.drag_mode != GizmoDragMode::None {
                    if let Some(p) = pointer_pos {
                        let delta_screen_x = (p.x - self.drag_start_pointer.x) * speed_mult;
                        let delta_screen_y = (p.y - self.drag_start_pointer.y) * speed_mult;
                        let delta_world_x = delta_screen_x / scale_x;
                        let delta_world_y = delta_screen_y / scale_y;

                        match self.drag_mode {
                            GizmoDragMode::PanFree | GizmoDragMode::Move => {
                                let raw_x = self.drag_start_x + delta_world_x;
                                let raw_y = self.drag_start_y + delta_world_y;
                                if is_ctrl {
                                    let grid = if is_shift { 1.0 } else { 10.0 };
                                    src.x = (raw_x / grid).round() * grid;
                                    src.y = (raw_y / grid).round() * grid;
                                } else {
                                    src.x = raw_x;
                                    src.y = raw_y;
                                }
                                sources_modified = true;
                            }
                            GizmoDragMode::Scale => {
                                let d0 = (self.drag_start_pointer.x - scx).hypot(self.drag_start_pointer.y - scy).max(10.0);
                                let d1 = (p.x - scx).hypot(p.y - scy);
                                let raw_factor = (d1 / d0).clamp(0.05, 20.0);
                                let factor = 1.0 + (raw_factor - 1.0) * speed_mult;
                                let eff_factor = if is_ctrl {
                                    let step = if is_shift { 0.01 } else { 0.05 };
                                    (factor / step).round() * step
                                } else {
                                    factor
                                };
                                let new_w = (self.drag_start_w * eff_factor).max(10.0);
                                let new_h = (self.drag_start_h * eff_factor).max(10.0);
                                let center_x = self.drag_start_x + self.drag_start_w / 2.0;
                                let center_y = self.drag_start_y + self.drag_start_h / 2.0;
                                src.x = center_x - new_w / 2.0;
                                src.y = center_y - new_h / 2.0;
                                src.width = new_w;
                                src.height = new_h;
                                sources_modified = true;
                            }
                            GizmoDragMode::RotateRoll | GizmoDragMode::Rotate => {
                                let a0 = (self.drag_start_pointer.y - scy).atan2(self.drag_start_pointer.x - scx).to_degrees();
                                let a1 = (p.y - scy).atan2(p.x - scx).to_degrees();
                                let delta_angle = (a1 - a0) * speed_mult;
                                let raw_roll = ((self.drag_start_roll + delta_angle) % 360.0 + 360.0) % 360.0;
                                src.roll = if is_ctrl {
                                    let step = if is_shift { 1.0 } else { 5.0 };
                                    ((raw_roll / step).round() * step) % 360.0
                                } else {
                                    raw_roll
                                };
                                sources_modified = true;
                            }
                            GizmoDragMode::RotatePitch => {
                                let rad = (-self.drag_start_roll).to_radians();
                                let cos_a = rad.cos();
                                let sin_a = rad.sin();
                                let s_ly = (self.drag_start_pointer.x - scx) * sin_a + (self.drag_start_pointer.y - scy) * cos_a;
                                let c_ly = (p.x - scx) * sin_a + (p.y - scy) * cos_a;

                                // Vertical Green bar -> Pitch tilt (dragging up tilts forward, down tilts backward)
                                let delta_deg = -(c_ly - s_ly) * speed_mult * 1.5;
                                let raw_pitch = ((self.drag_start_pitch + delta_deg) % 360.0 + 360.0) % 360.0;
                                src.pitch = if is_ctrl {
                                    let step = if is_shift { 1.0 } else { 5.0 };
                                    ((raw_pitch / step).round() * step) % 360.0
                                } else {
                                    raw_pitch
                                };
                                sources_modified = true;
                            }
                            GizmoDragMode::RotateYaw => {
                                let rad = (-self.drag_start_roll).to_radians();
                                let cos_a = rad.cos();
                                let sin_a = rad.sin();
                                let s_lx = (self.drag_start_pointer.x - scx) * cos_a - (self.drag_start_pointer.y - scy) * sin_a;
                                let c_lx = (p.x - scx) * cos_a - (p.y - scy) * sin_a;

                                // Horizontal Red bar -> Yaw turntable (dragging right turns right, left turns left)
                                let delta_deg = (c_lx - s_lx) * speed_mult * 1.5;
                                let raw_yaw = ((self.drag_start_yaw + delta_deg) % 360.0 + 360.0) % 360.0;
                                src.yaw = if is_ctrl {
                                    let step = if is_shift { 1.0 } else { 5.0 };
                                    ((raw_yaw / step).round() * step) % 360.0
                                } else {
                                    raw_yaw
                                };
                                sources_modified = true;
                            }
                            GizmoDragMode::None => {}
                        }

                        if sources_modified {
                            // Update active state and queue transform update to OBS
                            let mut hitboxes = state.hitboxes.write();
                            if let Some(hb) = hitboxes.get_mut(&src.name) {
                                hb.active = src.is_target && src.is_visible;
                            }
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
                                // Follow the drag with the mask's center of mass
                                if let Some(hb) = hitboxes.get_mut(&src.name) {
                                    crate::tracker::reproject_hitbox(hb, mask);
                                }
                            }
                            drop(masks);
                            drop(hitboxes);
                            crate::queue_studio_transform_update(&src);
                            ctx.request_repaint();
                        }
                    }
                }

                if response.drag_stopped() {
                    self.drag_mode = GizmoDragMode::None;
                    save_requested = true;
                    crate::queue_studio_transform_update(&src);
                    ctx.request_repaint();
                }


                // Draw Gizmo Rings & Axis Controls
                let line_w = 4.0;
                let stroke_white = Stroke::new(line_w, Color32::WHITE);
                let stroke_blue = Stroke::new(line_w, Color32::from_rgb(59, 130, 246));   // Blue = Roll
                let stroke_red = Stroke::new(6.0, Color32::from_rgb(239, 68, 68));       // Thick Red = Yaw (horizontal bar)
                let stroke_green = Stroke::new(6.0, Color32::from_rgb(34, 197, 94));     // Thick Green = Pitch (vertical bar)

                // 1. White Outer Scale Ring (r = 48)
                painter.circle_stroke(center_pos, r_scale, stroke_white);

                // 2. Blue Roll Ring (r = 34)
                painter.circle_stroke(center_pos, r_blue, stroke_blue);

                // 3. Thick Red and Green '+' Sign (rotated with roll)
                let rad = src.roll.to_radians();
                let cos_t = rad.cos();
                let sin_t = rad.sin();

                // Red Horizontal Axis Bar (Yaw: from -r_blue to +r_blue)
                let p_red_left = Pos2::new(scx - r_blue * cos_t, scy - r_blue * sin_t);
                let p_red_right = Pos2::new(scx + r_blue * cos_t, scy + r_blue * sin_t);
                painter.line_segment([p_red_left, p_red_right], stroke_red);

                // Green Vertical Axis Bar (Pitch: perpendicular across blue ring)
                let p_green_top = Pos2::new(scx + r_blue * sin_t, scy - r_blue * cos_t);
                let p_green_bot = Pos2::new(scx - r_blue * sin_t, scy + r_blue * cos_t);
                painter.line_segment([p_green_top, p_green_bot], stroke_green);

                // 5. White Center Spot (r = 11) for PanFree
                painter.circle_filled(center_pos, 11.0, Color32::WHITE);
                painter.circle_stroke(center_pos, 11.0, Stroke::new(2.0, Color32::from_rgb(15, 23, 42)));
            }

            if is_throwable_inactive {
                let card_w = (vp_rect.width() - 40.0).clamp(340.0, 520.0);
                let card_h = 165.0;
                let card_pos = Pos2::new(
                    vp_rect.center().x - card_w / 2.0,
                    vp_rect.center().y - card_h / 2.0,
                );

                egui::Area::new(egui::Id::new("throwables_invisible_modal"))
                    .fixed_pos(card_pos)
                    .order(egui::Order::Foreground)
                    .show(ctx, |ui| {
                        egui::Frame::NONE
                            .fill(Color32::from_rgba_unmultiplied(15, 23, 42, 250))
                            .stroke(Stroke::new(2.0, Color32::from_rgb(239, 68, 68)))
                            .corner_radius(12.0)
                            .inner_margin(egui::Margin::symmetric(24, 18))
                            .show(ui, |ui| {
                                ui.set_width(card_w - 48.0);
                                ui.vertical_centered(|ui| {
                                    ui.label(
                                        egui::RichText::new("⚠️ THROWABLE LAYER IS INVISIBLE IN OBS")
                                            .size(15.5)
                                            .strong()
                                            .color(Color32::from_rgb(248, 113, 113)),
                                    );
                                    ui.add_space(8.0);
                                    ui.label(
                                        egui::RichText::new(
                                            "OBS pauses rendering when a source's eye icon is turned off.\nMake the Throwables Layer visible in OBS to resume the live render.",
                                        )
                                        .size(12.5)
                                        .color(Color32::from_rgb(226, 232, 240)),
                                    );
                                    ui.add_space(12.0);
                                    let btn = ui.add(
                                        egui::Button::new(
                                            egui::RichText::new("👁 Make Visible in OBS")
                                                .size(13.5)
                                                .strong()
                                                .color(Color32::WHITE),
                                        )
                                        .fill(Color32::from_rgb(220, 38, 38))
                                        .corner_radius(6.0)
                                        .min_size(Vec2::new(180.0, 32.0)),
                                    );
                                    if btn.on_hover_text("Toggles the eye icon for Throwables Layer in OBS to visible").clicked() {
                                        crate::obs_source::set_throwables_layer_visible_in_obs(&cur_scene, true);
                                        ctx.request_repaint();
                                    }
                                });
                            });
                    });
            }

        });

        if sources_modified {
            *state.studio_sources.write() = sources.clone();
            if let Some(src) = sources.iter().find(|s| s.name == self.selected_source_name) {
                crate::queue_studio_transform_update(src);
                let textures = state.target_textures.read();
                if let Some(tex) = textures.get(&src.name) {
                    if tex.width > 0 && tex.height > 0 && !tex.rgba.is_empty() {
                        let config = *state.config.read();
                        let transform = crate::types::SourceTransform {
                            pos_x: src.x,
                            pos_y: src.y,
                            scale_x: src.width / tex.width as f32,
                            scale_y: src.height / tex.height as f32,
                            crop_left: 0,
                            crop_top: 0,
                            crop_right: 0,
                            crop_bottom: 0,
                            rot_deg: src.roll,
                        };
                        let (hitbox, mask, pixel_grid) = crate::tracker::scan_rgba_with_3d_transform(
                            &tex.rgba,
                            tex.width,
                            tex.height,
                            tex.width * 4,
                            &transform,
                            src.pitch,
                            src.yaw,
                            &config,
                        );
                        state.hitboxes.write().insert(src.name.clone(), hitbox);
                        state.masks.write().insert(src.name.clone(), mask);
                        state.pixel_grids.write().insert(src.name.clone(), pixel_grid);
                    }
                } else {
                    let mut hitboxes = state.hitboxes.write();
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
                        // Keeps the target point on the mask's center of mass, not the middle of the source
                        if let Some(hb) = hitboxes.get_mut(&src.name) {
                            crate::tracker::reproject_hitbox(hb, mask);
                        }
                    }
                }
            }
        }
            }
            StudioActiveTab::RedeemEditor => {
                self.render_redeem_editor(ctx, cw, ch);
            }
            StudioActiveTab::Scripts => {
                self.render_scripts_tab(ctx);
            }
            StudioActiveTab::Settings => {
                self.render_settings_tab(ctx);
            }
        }
        if save_requested {
            self.save_targets_file(&sources);
        }

        // Process any queued studio transforms directly into OBS
        crate::obs_source::process_pending_studio_transforms();
    }
}

pub fn save_targets_to_disk(sources: &[crate::StudioSource]) -> usize {
    let state = get_state();

    // Update physics hitboxes active state
    {
        let mut hitboxes = state.hitboxes.write();
        for src in sources {
            if src.is_target {
                if let Some(hb) = hitboxes.get_mut(&src.name) {
                    hb.active = src.is_target && src.is_visible;
                }
            }
        }
    }

    // Update target_slots mapping
    {
        let mut slot_map = state.target_slots.write();
        let mut auto_slot = 1;
        for src in sources.iter().filter(|s| s.is_target) {
            let s_num = if src.slot > 0 { src.slot as usize } else { auto_slot };
            slot_map.insert(s_num, src.name.clone());
            auto_slot = s_num + 1;
        }
    }

    let path_buf = crate::get_theater_repo_dir().join("theater_targets.json");
    let path = path_buf.to_str().unwrap_or("theater_targets.json");
    let mut existing_data = Vec::new();
    if let Ok(file) = std::fs::File::open(path) {
        if let Ok(serde_json::Value::Array(arr)) = serde_json::from_reader(file) {
            existing_data = arr;
        }
    }

    let current_scene = {
        let sc = state.studio_scene_name.read().clone();
        if sc.is_empty() { "Scène".to_string() } else { sc }
    };

    // Retain targets from other scenes, and exclude coordinate sets & studio settings (re-serialized below)
    let mut list: Vec<serde_json::Value> = existing_data.iter().filter(|item| {
        let is_special = item.get("type").is_some();
        if is_special {
            return false;
        }
        let item_scene = item.get("scene").and_then(|v| v.as_str()).unwrap_or("Scène");
        item_scene != current_scene
    }).cloned().collect();

    for src in sources {
        let existing_item = existing_data.iter().find(|item| {
            let item_scene = item.get("scene").and_then(|v| v.as_str()).unwrap_or("Scène");
            let item_name = item.get("name").or_else(|| item.get("source_name")).and_then(|v| v.as_str()).unwrap_or("");
            item_scene == current_scene && item_name == src.name
        });

        let mut obj = match existing_item {
            Some(serde_json::Value::Object(map)) => map.clone(),
            _ => serde_json::Map::new(),
        };

        let src_scene = if src.scene.is_empty() { current_scene.clone() } else { src.scene.clone() };
        obj.insert("scene".to_string(), serde_json::Value::String(src_scene));
        obj.insert("name".to_string(), serde_json::Value::String(src.name.clone()));
        obj.insert("source_name".to_string(), serde_json::Value::String(src.name.clone()));
        obj.insert("slot".to_string(), serde_json::json!(src.slot));
        obj.insert("target".to_string(), serde_json::Value::Bool(src.is_target));
        obj.insert("match_obs".to_string(), serde_json::Value::Bool(src.match_obs));
        obj.insert("visible".to_string(), serde_json::Value::Bool(src.is_visible));
        if !src.file_path.is_empty() {
            obj.insert("file_path".to_string(), serde_json::Value::String(src.file_path.clone()));
        }
        obj.insert("default_xform".to_string(), serde_json::json!({
            "x": src.default_x,
            "y": src.default_y,
            "width": src.default_width,
            "height": src.default_height,
            "roll": src.default_roll,
            "pitch": 0.0,
            "yaw": 0.0,
        }));
        if src.match_obs {
            obj.insert("current_xform".to_string(), serde_json::json!({
                "x": src.x,
                "y": src.y,
                "width": src.width,
                "height": src.height,
                "roll": src.roll,
                "pitch": src.pitch,
                "yaw": src.yaw,
            }));
        }
        obj.insert("custom".to_string(), serde_json::json!({
            "x": src.x,
            "y": src.y,
            "width": src.width,
            "height": src.height,
            "roll": src.roll,
            "pitch": src.pitch,
            "yaw": src.yaw,
        }));

        if let Some(filters) = pose_filters(&src.name) {
            obj.insert("filters".to_string(), serde_json::Value::Object(filters.into_iter().collect()));
        }

        list.push(serde_json::Value::Object(obj));
    }

    // Append all coordinate sets across all scenes
    {
        let sets_lock = SCENE_COORDINATE_SETS.read();
        for set in sets_lock.iter() {
            if let Ok(val) = serde_json::to_value(set) {
                let mut obj = match val {
                    serde_json::Value::Object(m) => m,
                    _ => serde_json::Map::new(),
                };
                obj.insert("type".to_string(), serde_json::Value::String("scene_coordinate_set".to_string()));
                list.push(serde_json::Value::Object(obj));
            }
        }
    }

    let count = list.len();
    let written = std::fs::File::create(path)
        .map_err(|e| e.to_string())
        .and_then(|file| serde_json::to_writer_pretty(file, &list).map_err(|e| e.to_string()));
    {
        let sets = SCENE_COORDINATE_SETS.read();
        let mut per_scene: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
        for s in sets.iter() {
            *per_scene.entry(s.scene.as_str()).or_default() += 1;
        }
        match &written {
            Ok(()) => crate::obs_source::trust_log(&format!(
                "[Trust Studio] 💾 Saved {} entries to '{}' (current scene '{}', poses per scene: {:?})",
                count, path, current_scene, per_scene
            )),
            Err(e) => crate::obs_source::trust_log(&format!("[Trust Studio] ⚠️ Could not save '{}': {}", path, e)),
        }
    }
    // Poses may have been added/renamed/deleted — keep the Scene Switching dock
    // and the Control Panel's Theater Scene command in step. Only then: the
    // Studio saves on every frame of a drag, and each notice makes OBS's UI
    // thread rebuild the dock and the Control Panel ask for the whole list.
    if pose_list_changed() {
        crate::obs_source::request_scene_dock_refresh();
        crate::obs_procs::notify_list_changed();
    }
    count
}

/// Whether the poses (id, name, scene) differ from the last time this was asked.
fn pose_list_changed() -> bool {
    static LISTED: parking_lot::Mutex<Option<Vec<(String, String, String)>>> = parking_lot::Mutex::new(None);
    let poses: Vec<(String, String, String)> = SCENE_COORDINATE_SETS
        .read()
        .iter()
        .map(|s| (s.id.clone(), s.name.clone(), s.scene.clone()))
        .collect();
    let mut listed = LISTED.lock();
    if listed.as_ref() == Some(&poses) {
        return false;
    }
    *listed = Some(poses);
    true
}

impl StudioApp {
    fn save_targets_file(&mut self, sources: &[crate::StudioSource]) {
        let count = save_targets_to_disk(sources);
        self.status_msg = format!("Saved {} targets", count);
    }

    fn render_redeem_editor(&mut self, ctx: &egui::Context, cw: u32, ch: u32) {
        let state = get_state();
        let cur_target = self.selected_source_name.clone();

        // 1. The redeem being edited, picked from a dropdown, and what can be
        // done with the list. The redeems' files, scripts and reactions are
        // on the Scripts page.
        let mut custom_redeem_browse_clicked = false;
        let redeem_choices: Vec<String> = self.redeems_list.iter().map(redeem_choice_label).collect();
        let mut picked_redeem: Option<usize> = None;

        egui::TopBottomPanel::top("redeem_editor_bar").show(ctx, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("🎁 Redeem:").strong().size(14.0));
                let current = self
                    .selected_redeem_index
                    .and_then(|i| redeem_choices.get(i))
                    .cloned()
                    .unwrap_or_else(|| "(none: pick one, or ➕ New)".to_string());
                egui::ComboBox::from_id_salt("redeem_editor_pick")
                    .selected_text(egui::RichText::new(current).size(14.0))
                    .width(380.0)
                    .height(480.0)
                    // Stays open while its search is typed in
                    .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                    .show_ui(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label("🔍");
                            ui.text_edit_singleline(&mut self.redeem_search_filter);
                            if !self.redeem_search_filter.is_empty() && ui.small_button("✖").clicked() {
                                self.redeem_search_filter.clear();
                            }
                        });
                        ui.separator();
                        let filter = self.redeem_search_filter.trim().to_lowercase();
                        for (idx, label) in redeem_choices.iter().enumerate() {
                            if !filter.is_empty() && !label.to_lowercase().contains(&filter) {
                                continue;
                            }
                            if ui.selectable_label(self.selected_redeem_index == Some(idx), label).clicked() {
                                picked_redeem = Some(idx);
                                ui.memory_mut(|m| m.close_popup());
                            }
                        }
                    });
                ui.separator();

                if ui.button("➕ New").on_hover_text("Create a new redeem file").clicked() {
                    let new_id = self.redeems_list.len() + 1;
                    let new_name = format!("new_redeem_{}.json", new_id);
                    let new_path = get_redeems_dir().join(&new_name);
                    let default_cfg = RedeemConfig::default();
                    let triggers_input = default_cfg.triggers.as_ref().map(|t| t.join(", ")).unwrap_or_default();
                    self.redeems_list.push(RedeemItem {
                        filename: new_name,
                        path: new_path,
                        config: default_cfg,
                        is_dirty: true,
                        triggers_input,
                    });
                    self.selected_redeem_index = Some(self.redeems_list.len() - 1);
                    self.redeem_editor_status = "Created new redeem template".to_string();
                }

                if ui.button("📥 Import").on_hover_text("Add a redeem from a .red file (the Theater Shield checks its scripts first)").clicked() {
                    let tx = self.package_tx.clone();
                    let ctx = ui.ctx().clone();
                    std::thread::spawn(move || {
                        let Some(file) = pick_redeem_package_dialog() else { return };
                        let result = crate::redeem_package::open(&file);
                        let _ = tx.send(PackageEvent::Opened { file, result });
                        ctx.request_repaint();
                    });
                }

                if ui.button("🔄 Reload").on_hover_text("Read every redeem from disk again (unsaved changes are lost)").clicked() {
                    let selected = self.selected_redeem_index.and_then(|i| self.redeems_list.get(i)).map(|r| r.path.clone());
                    self.redeems_list = load_all_redeems_from_disk();
                    // The same redeem stays picked when it's still there
                    self.selected_redeem_index = selected
                        .and_then(|p| self.redeems_list.iter().position(|r| r.path == p))
                        .or(if self.redeems_list.is_empty() { None } else { Some(0) });
                    self.redeem_editor_status = "Reloaded redeems from disk".to_string();
                }

                ui.separator();
                if ui.button("📜 Scripts…").on_hover_text("The redeems' files, their scripts and reactions (Scripts page)").clicked() {
                    self.active_tab = StudioActiveTab::Scripts;
                }
            });
            ui.add_space(4.0);
        });

        if let Some(idx) = picked_redeem {
            self.selected_redeem_index = Some(idx);
        }

        if custom_redeem_browse_clicked {
            let tx = self.file_dialog_tx.clone();
            let ctx_clone = ctx.clone();
            std::thread::spawn(move || {
                if let Some(path) = pick_file_dialog("Select Custom Redeem Script", "Lua Script | *.lua") {
                    let _ = tx.send(FileDialogResult { target: FileDialogTarget::CustomRedeemScript, path });
                    ctx_clone.request_repaint();
                }
            });
        }

        // 2. Right Redeem Configuration Panel
        let mut save_clicked = false;
        let mut export_clicked = false;
        let mut copy_json_clicked = false;
        let mut test_launch_clicked = false;
        let mut delete_clicked = false;

        let mut available_lua_effects = self.lua_effects_list.clone();
        // A redeem keeps a reaction script removed from the Lua Effects list
        // (its file is still there, and it still runs): offer it here anyway
        let selected_script = self
            .selected_redeem_index
            .and_then(|i| self.redeems_list.get(i))
            .and_then(|item| item.config.get_custom_script_path())
            .map(std::path::PathBuf::from);
        if let Some(path) = selected_script {
            if path.is_file() && !available_lua_effects.iter().any(|e| e.path == path) {
                let name = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                available_lua_effects.push(LuaScriptEntry { name, path });
            }
        }

        egui::SidePanel::right("redeem_config_panel")
            .min_width(420.0)
            .default_width(480.0)
            .show(ctx, |ui| {
                if let Some(sel_idx) = self.selected_redeem_index {
                    if let Some(item) = self.redeems_list.get_mut(sel_idx) {
                        // Header
                        ui.horizontal(|ui| {
                            ui.heading(format!("⚙ {}", item.filename));
                            if item.is_dirty {
                                ui.label(egui::RichText::new("● Modified").color(Color32::from_rgb(251, 191, 36)).strong());
                            } else {
                                ui.label(egui::RichText::new("✓ Saved").color(Color32::from_rgb(34, 197, 94)));
                            }

                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.button(egui::RichText::new("💾 Save").strong()).clicked() {
                                    save_clicked = true;
                                }
                                if ui.button(egui::RichText::new("📤 Export").strong()).on_hover_text("Save this redeem with everything it needs (script, images, sounds) as a .red file, to share or keep").clicked() {
                                    export_clicked = true;
                                }
                                if ui.small_button("📋").on_hover_text("Copy this redeem's settings (JSON) to the clipboard").clicked() {
                                    copy_json_clicked = true;
                                }
                            });
                        });

                        ui.separator();

                        egui::ScrollArea::vertical()
                            .id_salt("redeem_editor_unified_scroll")
                            .auto_shrink([false, false])
                            .max_height(ui.available_height() - 44.0)
                            .show(ui, |ui| {
                                // 1. Metadata & General Settings
                                egui::CollapsingHeader::new("🏷️ METADATA & GENERAL SETTINGS")
                                    .default_open(true)
                                    .show(ui, |ui| {
                                        ui.horizontal(|ui| {
                                            ui.label("🏷️ Redeem Name:");
                                            if ui.text_edit_singleline(&mut item.config.name).changed() {
                                                item.is_dirty = true;
                                            }
                                        });

                                        // Built from lists (platform, channel, event, title)
                                        ui.label("⚡ Event Triggers:");
                                        let mut triggers = item.config.triggers.clone().unwrap_or_default();
                                        if crate::trigger_builder::triggers_ui(ui, ("redeem_triggers", item.filename.clone()), &mut triggers, usize::MAX) {
                                            item.triggers_input = triggers.join(", ");
                                            item.config.triggers = if triggers.is_empty() { None } else { Some(triggers) };
                                            item.is_dirty = true;
                                            let c_str = std::ffi::CString::new(item.triggers_input.clone()).unwrap_or_default();
                                            crate::trust_set_editor_triggers(c_str.as_ptr());
                                        }
                                        ui.add_space(4.0);

                                        // Target Source selector
                                        ui.horizontal(|ui| {
                                            ui.label("👤 Target:");
                                            let mut current_target = item.config.get_target();

                                            // Gather all candidate sources from studio_sources
                                            let sources = state.studio_sources.read();
                                            let mut target_sources: Vec<String> = Vec::new();
                                            let mut other_sources: Vec<String> = Vec::new();

                                            for src in sources.iter() {
                                                if src.is_target {
                                                    if !target_sources.contains(&src.name) {
                                                        target_sources.push(src.name.clone());
                                                    }
                                                } else if !src.name.is_empty() && !other_sources.contains(&src.name) {
                                                    other_sources.push(src.name.clone());
                                                }
                                            }

                                            // If legacy slot number (e.g. "1") was saved, migrate it to the source name
                                            if let Ok(s_num) = current_target.parse::<usize>() {
                                                if s_num > 0 && s_num <= target_sources.len() {
                                                    current_target = target_sources[s_num - 1].clone();
                                                    item.config.set_target(&current_target);
                                                    item.is_dirty = true;
                                                } else if let Some(name) = state.target_slots.read().get(&s_num) {
                                                    current_target = name.clone();
                                                    item.config.set_target(&current_target);
                                                    item.is_dirty = true;
                                                }
                                            }

                                            // Fallback if empty: default to first target source if available
                                            if current_target.is_empty() && !target_sources.is_empty() {
                                                current_target = target_sources[0].clone();
                                                item.config.set_target(&current_target);
                                            }

                                            // Determine display text for current selection
                                            let selected_text = if current_target == "random" {
                                                "🎲 Random Target".to_string()
                                            } else if target_sources.contains(&current_target) {
                                                format!("🎯 {}", current_target)
                                            } else if other_sources.contains(&current_target) {
                                                format!("🖼 {}", current_target)
                                            } else if !current_target.is_empty() {
                                                format!("🎯 {}", current_target)
                                            } else {
                                                "-- Select Target --".to_string()
                                            };

                                            egui::ComboBox::from_id_salt("target_source_combo")
                                                .selected_text(selected_text)
                                                .show_ui(ui, |ui| {
                                                    // Designated target sources
                                                    for tgt_name in &target_sources {
                                                        let label = format!("🎯 {}", tgt_name);
                                                        let is_sel = current_target == *tgt_name;
                                                        if ui.selectable_label(is_sel, label).clicked() {
                                                            item.config.set_target(tgt_name.clone());
                                                            item.is_dirty = true;
                                                        }
                                                    }

                                                    // Other scene sources
                                                    if !other_sources.is_empty() {
                                                        ui.separator();
                                                        for src_name in &other_sources {
                                                            let label = format!("🖼 {}", src_name);
                                                            let is_sel = current_target == *src_name;
                                                            if ui.selectable_label(is_sel, label).clicked() {
                                                                item.config.set_target(src_name.clone());
                                                                item.is_dirty = true;
                                                            }
                                                        }
                                                    }

                                                    ui.separator();
                                                    if ui.selectable_label(current_target == "random", "🎲 Random Target").clicked() {
                                                        item.config.set_target("random");
                                                        item.is_dirty = true;
                                                    }
                                                });
                                        });

                                        // Redeem Type selector
                                        ui.horizontal(|ui| {
                                            ui.label("🎭 Redeem Type:");
                                            let type_label = match item.config.redeem_type.as_str() {
                                                "webgl-water" => "🌊 Anime Water Stream & Splash (SPH Metaball)",
                                                "webgl-fire" => "🔥 Fire / Flame / Smoke",
                                                "webgl-lightning" => "⚡ Lightning / Electricity",
                                                "reaction" => "💥 Reaction Only (No Throwable)",
                                                "custom" => "📜 Custom Lua Script",
                                                _ => "🖼️ Image / Throwable",
                                            };

                                            egui::ComboBox::from_id_salt("redeem_type_combo")
                                                .selected_text(type_label)
                                                .show_ui(ui, |ui| {
                                                    // Fire and water share the same underlying fields
                                                    // (fluidGravity, streamMode, ...) with type-specific
                                                    // (and sometimes opposite-signed — fire rises, water
                                                    // falls) meanings, so switching types must clear them
                                                    // back to None instead of leaving stale values behind
                                                    // (e.g. a fire redeem's negative "rising" gravity
                                                    // leaking into water, making it rise like fire too).
                                                    // Each type's own slider already falls back to its
                                                    // own sensible default via unwrap_or() once these are
                                                    // None, so clearing them is enough to "reset".
                                                    if ui.selectable_label(item.config.redeem_type == "image", "🖼️ Image / Throwable").clicked() {
                                                        item.config.redeem_type = "image".to_string();
                                                        item.config.fluid_gravity = None;
                                                        item.config.stream_mode = None;
                                                        item.is_dirty = true;
                                                    }
                                                    if ui.selectable_label(item.config.redeem_type == "webgl-water", "🌊 Anime Water Stream & Splash (SPH Metaball)").clicked() {
                                                        item.config.redeem_type = "webgl-water".to_string();
                                                        item.config.collision_sound = None;
                                                        item.config.collision_sound_path = None;
                                                        item.config.fluid_gravity = None;
                                                        item.config.stream_mode = None;
                                                        // Water is the only fluid type — always reset back to
                                                        // its correct, positive gravity/viscosity/color, not
                                                        // just on first pick but every time this redeem
                                                        // returns to water after visiting another type.
                                                        item.config.apply_fluid_preset("water");
                                                        item.is_dirty = true;
                                                    }
                                                    if ui.selectable_label(item.config.redeem_type == "webgl-fire", "🔥 Fire / Flame / Smoke").clicked() {
                                                        item.config.redeem_type = "webgl-fire".to_string();
                                                        item.config.collision_sound = None;
                                                        item.config.collision_sound_path = None;
                                                        item.config.fluid_gravity = None;
                                                        item.config.stream_mode = None;
                                                        item.config.mask_penetration = None;
                                                        item.is_dirty = true;
                                                    }
                                                    if ui.selectable_label(item.config.redeem_type == "webgl-lightning", "⚡ Lightning / Electricity").clicked() {
                                                        item.config.redeem_type = "webgl-lightning".to_string();
                                                        item.config.collision_sound = None;
                                                        item.config.collision_sound_path = None;
                                                        item.config.fluid_gravity = None;
                                                        item.config.stream_mode = None;
                                                        item.is_dirty = true;
                                                    }
                                                    if ui.selectable_label(item.config.redeem_type == "reaction", "💥 Reaction Only (No Throwable)")
                                                        .on_hover_text("No throwable, no particle effect — just an avatar reaction plus a launch sound.")
                                                        .clicked()
                                                    {
                                                        item.config.redeem_type = "reaction".to_string();
                                                        item.config.collision_sound = None;
                                                        item.config.collision_sound_path = None;
                                                        item.config.fluid_gravity = None;
                                                        item.config.stream_mode = None;
                                                        item.is_dirty = true;
                                                    }
                                                    if ui.selectable_label(item.config.redeem_type == "custom", "📜 Custom Lua Script")
                                                        .on_hover_text("Executes a custom .lua script with access to fluids, throwables, fire, lightning, reactions, and delays.")
                                                        .clicked()
                                                    {
                                                        item.config.redeem_type = "custom".to_string();
                                                        item.is_dirty = true;
                                                    }
                                                });
                                        });
                                    });

                                ui.add_space(8.0);

                                // 2. Dynamic Type Parameters
                                match item.config.redeem_type.as_str() {
                                    "webgl-water" => {
                                        egui::CollapsingHeader::new("🌊 ANIME WATER STREAM & SPLASH (SPH METABALL)")
                                            .default_open(true)
                                            .show(ui, |ui| {
                                                let mut cnt = item.config.fluid_count.unwrap_or(120);
                                                if ui.add(egui::Slider::new(&mut cnt, 1..=20000).step_by(10.0).text("💧 Particle Count")).changed() {
                                                    item.config.fluid_count = Some(cnt); item.is_dirty = true;
                                                }

                                                let mut rad = item.config.fluid_radius.unwrap_or(18.0);
                                                if ui.add(egui::Slider::new(&mut rad, 1.0..=20.0).step_by(1.0).suffix(" px").text("🔵 Particle Radius")).changed() {
                                                    item.config.fluid_radius = Some(rad); item.is_dirty = true;
                                                }

                                                let mut visc = item.config.fluid_viscosity.unwrap_or(0.5);
                                                if ui.add(egui::Slider::new(&mut visc, 0.0..=2.0).step_by(0.01).text("🍯 Viscosity")).changed() {
                                                    item.config.fluid_viscosity = Some(visc); item.is_dirty = true;
                                                }

                                                let mut grv = item.config.fluid_gravity.unwrap_or(0.8);
                                                if ui.add(egui::Slider::new(&mut grv, 0.0..=10.0).step_by(0.05).text("🌍 Gravity")).changed() {
                                                    item.config.fluid_gravity = Some(grv); item.is_dirty = true;
                                                }

                                                let mut speed = item.config.stream_speed.unwrap_or(1.0);
                                                let speed_resp = ui.add(egui::Slider::new(&mut speed, 0.1..=3.0).step_by(0.05).text("💨 Speed"));
                                                if speed_resp.changed() {
                                                    item.config.stream_speed = Some(speed); item.is_dirty = true;
                                                }
                                                speed_resp.on_hover_text("How fast the stream actually travels \u{2014} separate from Gravity (which only shapes the arc's curve) and Viscosity (which only affects drag after landing).");

                                                let mut dur = item.config.fluid_duration.unwrap_or(6000);
                                                if ui.add(egui::Slider::new(&mut dur, 100..=30000).step_by(100.0).suffix(" ms").text("⏱️ Duration")).changed() {
                                                    item.config.fluid_duration = Some(dur); item.is_dirty = true;
                                                }

                                                let mut w = item.config.fluid_width.unwrap_or(30.0);
                                                if ui.add(egui::Slider::new(&mut w, 0.0..=2000.0).step_by(5.0).suffix(" px").text("↔️ Nozzle Width")).changed() {
                                                    item.config.fluid_width = Some(w); item.is_dirty = true;
                                                }

                                                let mut cone = item.config.cone_spread.unwrap_or(30.0);
                                                if ui.add(egui::Slider::new(&mut cone, 0.0..=2000.0).step_by(5.0).suffix(" px").text("📐 Beam Cone Spread")).changed() {
                                                    item.config.cone_spread = Some(cone); item.is_dirty = true;
                                                }

                                                let cur_mode = item.config.stream_mode.clone().unwrap_or_else(|| "grid".to_string());
                                                ui.horizontal(|ui| {
                                                    ui.label("🌊 Stream Mode:");
                                                    let mode_lbl = if cur_mode == "top" { "Fall From Above (top)" } else { "Arc Toward Target (grid)" };
                                                    egui::ComboBox::from_id_salt("stream_mode_combo")
                                                        .selected_text(mode_lbl)
                                                        .show_ui(ui, |ui| {
                                                            if ui.selectable_label(cur_mode == "grid", "Arc Toward Target (grid)").clicked() {
                                                                item.config.stream_mode = Some("grid".to_string()); item.is_dirty = true;
                                                            }
                                                            if ui.selectable_label(cur_mode == "top", "Fall From Above (top)").clicked() {
                                                                item.config.stream_mode = Some("top".to_string()); item.is_dirty = true;
                                                            }
                                                        });
                                                });

                                                if cur_mode == "grid" {
                                                    ui.horizontal(|ui| {
                                                        ui.label("🎯 Stream Origin:");
                                                        let cur_sp = item.config.water_spawn.clone().unwrap_or_else(|| "tc".to_string());
                                                        let sp_label = match cur_sp.as_str() {
                                                            "tl" => "Top Left",
                                                            "tr" => "Top Right",
                                                            "ml" => "Left Side",
                                                            "mr" => "Right Side",
                                                            _ => "Top Center",
                                                        };
                                                        egui::ComboBox::from_id_salt("water_spawn_combo")
                                                            .selected_text(sp_label)
                                                            .show_ui(ui, |ui| {
                                                                for (val, lbl) in [
                                                                    ("tc", "Top Center"),
                                                                    ("tl", "Top Left"),
                                                                    ("tr", "Top Right"),
                                                                    ("ml", "Left Side"),
                                                                    ("mr", "Right Side"),
                                                                ] {
                                                                    if ui.selectable_label(cur_sp == val, lbl).clicked() {
                                                                        item.config.water_spawn = Some(val.to_string());
                                                                        item.is_dirty = true;
                                                                    }
                                                                }
                                                            });
                                                    });
                                                }

                                                let mut drip = item.config.drip_through_percent.unwrap_or(50.0);
                                                if ui.add(egui::Slider::new(&mut drip, 0.0..=100.0).step_by(5.0).suffix(" %").text("💦 Drip-Through %")).changed() {
                                                    item.config.drip_through_percent = Some(drip); item.is_dirty = true;
                                                }

                                                let mut no_decay = item.config.no_decay.unwrap_or(false);
                                                if ui.checkbox(&mut no_decay, "🥛 Disable Decay (fill a container)")
                                                    .on_hover_text("Settled/dripping water stays put instead of fading away a few seconds after landing — use this to visually fill a cup or other recipient.")
                                                    .changed()
                                                {
                                                    item.config.no_decay = Some(no_decay); item.is_dirty = true;
                                                }

                                                ui.horizontal(|ui| {
                                                    let r = item.config.color_r.unwrap_or(38);
                                                    let g = item.config.color_g.unwrap_or(155);
                                                    let b = item.config.color_b.unwrap_or(230);
                                                    let mut rgb = [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0];
                                                    ui.label("🎨 Color:");
                                                    if ui.color_edit_button_rgb(&mut rgb).changed() {
                                                        item.config.color_r = Some((rgb[0] * 255.0).round() as u8);
                                                        item.config.color_g = Some((rgb[1] * 255.0).round() as u8);
                                                        item.config.color_b = Some((rgb[2] * 255.0).round() as u8);
                                                        item.is_dirty = true;
                                                    }
                                                    let mut a = item.config.color_a.unwrap_or(0.95);
                                                    if ui.add(egui::Slider::new(&mut a, 0.0..=1.0).step_by(0.05).text("Alpha")).changed() {
                                                        item.config.color_a = Some(a); item.is_dirty = true;
                                                    }
                                                });

                                                ui.horizontal(|ui| {
                                                    ui.label("🖼️ Water Texture:");
                                                    let cur_path = item.config.water_texture_path.clone();
                                                    let label = cur_path.as_ref()
                                                        .and_then(|p| std::path::Path::new(p).file_name())
                                                        .map(|n| n.to_string_lossy().to_string())
                                                        .unwrap_or_else(|| "(procedural, no image)".to_string());
                                                    ui.label(label);
                                                    if ui.button("📂 Browse...").on_hover_text(
                                                        "Drives the metaball's surface bump pattern instead of the built-in procedural noise, for this redeem."
                                                    ).clicked() {
                                                        let tx = self.file_dialog_tx.clone();
                                                        let ctx_clone = ui.ctx().clone();
                                                        std::thread::spawn(move || {
                                                            if let Some(path) = pick_file_dialog("Select Water Texture Image", "Images (*.png, *.jpg, *.webp) | *.png *.jpg *.jpeg *.webp") {
                                                                let _ = tx.send(FileDialogResult { target: FileDialogTarget::WaterTexture, path });
                                                                ctx_clone.request_repaint();
                                                            }
                                                        });
                                                    }
                                                    if cur_path.is_some() && ui.button("🗑 Clear").clicked() {
                                                        item.config.water_texture_path = None;
                                                        item.is_dirty = true;
                                                        self.redeem_editor_status = "Cleared water texture (back to procedural)".to_string();
                                                    }
                                                });

                                                ui.horizontal(|ui| {
                                                    let mut tex_scale = item.config.water_texture_scale.unwrap_or(160.0);
                                                    if ui.add(egui::Slider::new(&mut tex_scale, 20.0..=600.0).suffix(" px").text("🔍 Texture Size")).changed() {
                                                        item.config.water_texture_scale = Some(tex_scale); item.is_dirty = true;
                                                    }
                                                });

                                                ui.horizontal(|ui| {
                                                    let mut scroll_speed = item.config.water_texture_scroll_speed.unwrap_or(12.0);
                                                    if ui.add(egui::Slider::new(&mut scroll_speed, 0.0..=100.0).suffix(" px/s").text("⬇️ Texture Scroll Speed")).changed() {
                                                        item.config.water_texture_scroll_speed = Some(scroll_speed); item.is_dirty = true;
                                                    }
                                                }).response.on_hover_text("How fast the water texture drifts downward. 0 = static.");

                                                let mut refraction = item.config.water_refraction_enabled.unwrap_or(true);
                                                if ui.checkbox(&mut refraction, "🌊 Enable Refraction (liquid color tinting)")
                                                    .on_hover_text("Turn off for pure clear optical deformation without colored liquid tinting.")
                                                    .changed()
                                                {
                                                    item.config.water_refraction_enabled = Some(refraction); item.is_dirty = true;
                                                }

                                                let is_smooth_glass = item.config.water_texture_path.as_deref()
                                                    .map(|p| p.ends_with("smooth_deformed_glass.png"))
                                                    .unwrap_or(false);
                                                let is_rippled_glass = item.config.water_texture_path.as_deref()
                                                    .map(|p| p.ends_with("deformed_glass.png") && !p.ends_with("smooth_deformed_glass.png"))
                                                    .unwrap_or(false);
                                                let is_deformed_glass = is_smooth_glass || is_rippled_glass;
                                                let mut deformed_glass = is_deformed_glass;
                                                if ui.checkbox(&mut deformed_glass, "🌊 Deformed Glass Texture (undulating optics)")
                                                    .on_hover_text("Applies seamless deformed glass with dynamic dual-layer morphing and living optical wave distortion.")
                                                    .changed()
                                                {
                                                    if deformed_glass {
                                                        let smooth_tex = crate::get_theater_repo_dir().join("Trust/assets/textures/smooth_deformed_glass.png").to_string_lossy().to_string();
                                                        item.config.water_texture_path = Some(smooth_tex);
                                                        if item.config.water_texture_scale.is_none() {
                                                            item.config.water_texture_scale = Some(180.0);
                                                        }
                                                        if item.config.water_texture_scroll_speed.is_none() {
                                                            item.config.water_texture_scroll_speed = Some(18.0);
                                                        }
                                                        if item.config.water_refraction_strength.is_none() {
                                                            item.config.water_refraction_strength = Some(350.0);
                                                        }
                                                        self.redeem_editor_status = "Enabled Deformed Glass (ultra-smooth finish)".to_string();
                                                    } else {
                                                        if is_deformed_glass {
                                                            item.config.water_texture_path = None;
                                                        }
                                                        self.redeem_editor_status = "Disabled Deformed Glass (back to procedural)".to_string();
                                                    }
                                                    item.is_dirty = true;
                                                }

                                                if deformed_glass {
                                                    ui.horizontal(|ui| {
                                                        ui.label("  Surface Finish:");
                                                        let is_smooth = is_smooth_glass || !is_rippled_glass;
                                                        if ui.selectable_label(is_smooth, "✨ Ultra-Smooth Swells").clicked() {
                                                            let smooth_tex = crate::get_theater_repo_dir().join("Trust/assets/textures/smooth_deformed_glass.png").to_string_lossy().to_string();
                                                            item.config.water_texture_path = Some(smooth_tex);
                                                            item.is_dirty = true;
                                                            self.redeem_editor_status = "Selected Ultra-Smooth Swell surface".to_string();
                                                        }
                                                        if ui.selectable_label(!is_smooth, "🌊 Rippled Glass").clicked() {
                                                            let rippled_tex = crate::get_theater_repo_dir().join("Trust/assets/textures/deformed_glass.png").to_string_lossy().to_string();
                                                            item.config.water_texture_path = Some(rippled_tex);
                                                            item.is_dirty = true;
                                                            self.redeem_editor_status = "Selected Rippled Glass surface".to_string();
                                                        }
                                                    });
                                                }

                                                if refraction || deformed_glass {
                                                    ui.horizontal(|ui| {
                                                        let mut r_str = item.config.water_refraction_strength.unwrap_or(350.0);
                                                        if ui.add(egui::Slider::new(&mut r_str, 50.0..=1000.0).suffix(" px").text("🔍 Optical Distortion")).changed() {
                                                            item.config.water_refraction_strength = Some(r_str); item.is_dirty = true;
                                                        }
                                                    }).response.on_hover_text("Optical bending strength through the glass/water.");
                                                }

                                                let mut round_edge = item.config.water_round_edge.unwrap_or(false);
                                                if ui.checkbox(&mut round_edge, "⭕ Round Particles (smooth edge)")
                                                    .on_hover_text("Smooth, round silhouette instead of the default jagged/pointed splash-spike look.")
                                                    .changed()
                                                {
                                                    item.config.water_round_edge = Some(round_edge); item.is_dirty = true;
                                                }

                                                ui.horizontal(|ui| {
                                                    let mut mb_size = item.config.water_metaball_size.unwrap_or(9.5);
                                                    if ui.add(egui::Slider::new(&mut mb_size, 2.0..=50.0).text("🔵 Metaball Size")).changed() {
                                                        item.config.water_metaball_size = Some(mb_size); item.is_dirty = true;
                                                    }
                                                }).response.on_hover_text(
                                                    "How much bigger than each particle's own radius the fused blob renders \u{2014} higher also helps nearby particles fuse into one shape instead of showing as separate beads."
                                                );
                                            });
                                    }
                                    "webgl-fire" => {
                                        egui::CollapsingHeader::new("🔥 FIRE / FLAME / SMOKE SETTINGS")
                                            .default_open(true)
                                            .show(ui, |ui| {
                                                let mut cnt = item.config.fluid_count.unwrap_or(400);
                                                if ui.add(egui::Slider::new(&mut cnt, 1..=3000).step_by(10.0).text("🔥 Particle Count")).changed() {
                                                    item.config.fluid_count = Some(cnt); item.is_dirty = true;
                                                }

                                                let mut rad = item.config.fluid_radius.unwrap_or(16.0).min(20.0);
                                                if ui.add(egui::Slider::new(&mut rad, 1.0..=20.0).step_by(1.0).suffix(" px").text("🔵 Particle Size")).changed() {
                                                    item.config.fluid_radius = Some(rad.min(20.0)); item.is_dirty = true;
                                                }

                                                let mut visc = item.config.fluid_viscosity.unwrap_or(0.6);
                                                if ui.add(egui::Slider::new(&mut visc, 0.01..=2.0).step_by(0.01).text("💨 Spread / Drag")).changed() {
                                                    item.config.fluid_viscosity = Some(visc); item.is_dirty = true;
                                                }

                                                let mut grv = item.config.fluid_gravity.unwrap_or(-1.0).clamp(-1.0, 0.0);
                                                if ui.add(egui::Slider::new(&mut grv, -1.0..=0.0).step_by(0.01).text("🌬️ Rise Force (neg = up)")).changed() {
                                                    item.config.fluid_gravity = Some(grv.clamp(-1.0, 0.0)); item.is_dirty = true;
                                                }

                                                let mut dur = item.config.fluid_duration.unwrap_or(3000);
                                                if ui.add(egui::Slider::new(&mut dur, 100..=30000).step_by(100.0).suffix(" ms").text("⏱️ Duration")).changed() {
                                                    item.config.fluid_duration = Some(dur); item.is_dirty = true;
                                                }

                                                let mut w = item.config.fluid_width.unwrap_or(200.0);
                                                if ui.add(egui::Slider::new(&mut w, 1.0..=2000.0).step_by(10.0).suffix(" px").text("↔️ Spread Width")).changed() {
                                                    item.config.fluid_width = Some(w); item.is_dirty = true;
                                                }

                                                let mut off_y = item.config.offset_y.unwrap_or(0.0);
                                                if ui.add(egui::Slider::new(&mut off_y, -1000.0..=1000.0).step_by(5.0).suffix(" px").text("↕️ Y Offset")).changed() {
                                                    item.config.offset_y = Some(off_y); item.is_dirty = true;
                                                }

                                                let cur_mode = item.config.stream_mode.clone().unwrap_or_else(|| "top".to_string());
                                                ui.horizontal(|ui| {
                                                    ui.label("🔥 Emission Mode:");
                                                    let mode_lbl = if cur_mode == "grid" { "Flamethrower (stream to target)" } else { "Campfire (rise from target)" };
                                                    egui::ComboBox::from_id_salt("fire_mode_combo")
                                                        .selected_text(mode_lbl)
                                                        .show_ui(ui, |ui| {
                                                            if ui.selectable_label(cur_mode == "top", "Campfire (rise from target)").clicked() {
                                                                item.config.stream_mode = Some("top".to_string()); item.is_dirty = true;
                                                            }
                                                            if ui.selectable_label(cur_mode == "grid", "Flamethrower (stream to target)").clicked() {
                                                                item.config.stream_mode = Some("grid".to_string()); item.is_dirty = true;
                                                            }
                                                        });
                                                });

                                                if cur_mode == "grid" {
                                                    ui.horizontal(|ui| {
                                                        ui.label("🎯 Stream Origin:");
                                                        let cur_sp = item.config.fire_spawn.clone().unwrap_or_else(|| "tc".to_string());
                                                        let sp_label = match cur_sp.as_str() {
                                                            "tl" => "Top Left",
                                                            "tr" => "Top Right",
                                                            "ml" => "Left Side",
                                                            "mr" => "Right Side",
                                                            _ => "Top Center",
                                                        };
                                                        egui::ComboBox::from_id_salt("fire_spawn_combo")
                                                            .selected_text(sp_label)
                                                            .show_ui(ui, |ui| {
                                                                for (val, lbl) in [
                                                                    ("tc", "Top Center"),
                                                                    ("tl", "Top Left"),
                                                                    ("tr", "Top Right"),
                                                                    ("ml", "Left Side"),
                                                                    ("mr", "Right Side"),
                                                                ] {
                                                                    if ui.selectable_label(cur_sp == val, lbl).clicked() {
                                                                        item.config.fire_spawn = Some(val.to_string());
                                                                        item.is_dirty = true;
                                                                    }
                                                                }
                                                            });
                                                    });

                                                    ui.horizontal(|ui| {
                                                        ui.label("🎯 Mask Penetration:");
                                                        let mut pen = item.config.mask_penetration.unwrap_or(0.0);
                                                        // Range covers the worst case: Particle Radius goes up
                                                        // to 1000px, and the visual lead this offsets against
                                                        // is up to size*1.3 where size can be radius*1.3 —
                                                        // ~1690px — so a small fixed cap left large particles
                                                        // unable to reach a real "hit" at all.
                                                        if ui.add(egui::Slider::new(&mut pen, -100.0..=2000.0).suffix(" px")).changed() {
                                                            item.config.mask_penetration = Some(pen); item.is_dirty = true;
                                                        }
                                                    }).response.on_hover_text(
                                                        "How far the stream sinks past the target's mask surface before it stops. \
                                                        0 stops right at the visible edge, negative stops a bit early, positive digs in further."
                                                    );
                                                }

                                                ui.horizontal(|ui| {
                                                    let r = item.config.color_r.unwrap_or(255);
                                                    let g = item.config.color_g.unwrap_or(80);
                                                    let b = item.config.color_b.unwrap_or(0);
                                                    let mut rgb = [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0];
                                                    ui.label("🔴 Flame Color:");
                                                    if ui.color_edit_button_rgb(&mut rgb).changed() {
                                                        item.config.color_r = Some((rgb[0] * 255.0).round() as u8);
                                                        item.config.color_g = Some((rgb[1] * 255.0).round() as u8);
                                                        item.config.color_b = Some((rgb[2] * 255.0).round() as u8);
                                                        item.is_dirty = true;
                                                    }
                                                    let mut a = item.config.color_a.unwrap_or(0.95);
                                                    if ui.add(egui::Slider::new(&mut a, 0.0..=1.0).step_by(0.05).text("Alpha")).changed() {
                                                        item.config.color_a = Some(a); item.is_dirty = true;
                                                    }
                                                });

                                                // In percent of the default glow; saved as bloomSize (1 = 100%)
                                                let max_glow = crate::fire::MAX_GLOW_SIZE * 100.0;
                                                let mut glow = item.config.bloom_size.unwrap_or(1.0) * 100.0;
                                                if ui.add(egui::Slider::new(&mut glow, 0.0..=max_glow).step_by(5.0).suffix("%").text("✨ Glow Size"))
                                                    .on_hover_text("How far each flame's glow reaches: 100% is the default, 0% leaves only the hot core")
                                                    .changed()
                                                {
                                                    item.config.bloom_size = Some(glow / 100.0); item.is_dirty = true;
                                                }

                                                let mut eb = item.config.enable_bloom.unwrap_or(true);
                                                if ui.checkbox(&mut eb, "✨ Enable Glow Effect").changed() {
                                                    item.config.enable_bloom = Some(eb); item.is_dirty = true;
                                                }
                                            });
                                    }
                                    "webgl-lightning" => {
                                        egui::CollapsingHeader::new("⚡ LIGHTNING / ELECTRICITY SETTINGS")
                                            .default_open(true)
                                            .show(ui, |ui| {
                                                let mut strikes = item.config.strike_count.unwrap_or(3);
                                                if ui.add(egui::Slider::new(&mut strikes, 1..=10).text("⚡ Number of Strikes")).changed() {
                                                    item.config.strike_count = Some(strikes); item.is_dirty = true;
                                                }

                                                let mut bolts = item.config.bolt_count.unwrap_or(2);
                                                if ui.add(egui::Slider::new(&mut bolts, 1..=6).text("🌩️ Parallel Bolts")).changed() {
                                                    item.config.bolt_count = Some(bolts); item.is_dirty = true;
                                                }

                                                let mut dur = item.config.duration.unwrap_or(500);
                                                if ui.add(egui::Slider::new(&mut dur, 100..=3000).step_by(50.0).suffix(" ms").text("⏱️ Strike Duration")).changed() {
                                                    item.config.duration = Some(dur); item.is_dirty = true;
                                                }

                                                let mut w = item.config.bolt_width.unwrap_or(4.0);
                                                if ui.add(egui::Slider::new(&mut w, 1.0..=20.0).step_by(0.5).suffix(" px").text("📏 Bolt Width")).changed() {
                                                    item.config.bolt_width = Some(w); item.is_dirty = true;
                                                }

                                                let mut rough = item.config.roughness.unwrap_or(0.85);
                                                if ui.add(egui::Slider::new(&mut rough, 0.0..=1.0).step_by(0.05).text("⚡ Roughness (jaggedness)")).changed() {
                                                    item.config.roughness = Some(rough); item.is_dirty = true;
                                                }

                                                let mut straight = item.config.straightness.unwrap_or(0.6);
                                                if ui.add(egui::Slider::new(&mut straight, 0.0..=1.0).step_by(0.05).text("📏 Straightness")).changed() {
                                                    item.config.straightness = Some(straight); item.is_dirty = true;
                                                }

                                                let mut branch = item.config.branching.unwrap_or(3);
                                                if ui.add(egui::Slider::new(&mut branch, 0..=8).text("🌿 Branch Count")).changed() {
                                                    item.config.branching = Some(branch); item.is_dirty = true;
                                                }

                                                ui.horizontal(|ui| {
                                                    let r = item.config.color_r.unwrap_or(180);
                                                    let g = item.config.color_g.unwrap_or(220);
                                                    let b = item.config.color_b.unwrap_or(255);
                                                    let mut rgb = [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0];
                                                    ui.label("🔴 Bolt Color:");
                                                    if ui.color_edit_button_rgb(&mut rgb).changed() {
                                                        item.config.color_r = Some((rgb[0] * 255.0).round() as u8);
                                                        item.config.color_g = Some((rgb[1] * 255.0).round() as u8);
                                                        item.config.color_b = Some((rgb[2] * 255.0).round() as u8);
                                                        item.is_dirty = true;
                                                    }
                                                });

                                                let mut bloom = item.config.bloom_size.unwrap_or(1.0);
                                                if ui.add(egui::Slider::new(&mut bloom, 0.0..=5.0).step_by(0.1).text("✨ Glow Size")).changed() {
                                                    item.config.bloom_size = Some(bloom); item.is_dirty = true;
                                                }

                                                let mut eb = item.config.enable_bloom.unwrap_or(true);
                                                if ui.checkbox(&mut eb, "✨ Enable Glow Effect").changed() {
                                                    item.config.enable_bloom = Some(eb); item.is_dirty = true;
                                                }

                                                let mut fps = item.config.smoothness.unwrap_or(30);
                                                if ui.add(egui::Slider::new(&mut fps, 5..=60).step_by(1.0).suffix(" FPS").text("⚡ Animation Smoothness")).changed() {
                                                    item.config.smoothness = Some(fps); item.is_dirty = true;
                                                }

                                                let mut flash = item.config.enable_impact.unwrap_or(true);
                                                if ui.checkbox(&mut flash, "💥 Enable Impact Flash Bloom").changed() {
                                                    item.config.enable_impact = Some(flash); item.is_dirty = true;
                                                }

                                                let mut fsize = item.config.impact_size.unwrap_or(1.0);
                                                if ui.add(egui::Slider::new(&mut fsize, 0.2..=30.0).step_by(0.5).text("💥 Impact Flash Size")).changed() {
                                                    item.config.impact_size = Some(fsize); item.is_dirty = true;
                                                }

                                                ui.horizontal(|ui| {
                                                    ui.label("🎯 Strike Origin:");
                                                    let cur_sp = item.config.light_spawn.clone().unwrap_or_else(|| "tc".to_string());
                                                    let sp_label = match cur_sp.as_str() {
                                                        "tl" => "Top Left",
                                                        "tr" => "Top Right",
                                                        "ml" => "Left Side",
                                                        "mr" => "Right Side",
                                                        _ => "Top Center",
                                                    };
                                                    egui::ComboBox::from_id_salt("light_spawn_combo")
                                                        .selected_text(sp_label)
                                                        .show_ui(ui, |ui| {
                                                            for (val, lbl) in [
                                                                ("tc", "Top Center"),
                                                                ("tl", "Top Left"),
                                                                ("tr", "Top Right"),
                                                                ("ml", "Left Side"),
                                                                ("mr", "Right Side"),
                                                            ] {
                                                                if ui.selectable_label(cur_sp == val, lbl).clicked() {
                                                                    item.config.light_spawn = Some(val.to_string());
                                                                    item.is_dirty = true;
                                                                }
                                                            }
                                                        });
                                                });
                                            });
                                    }
                                    "reaction" => {
                                        egui::CollapsingHeader::new("💥 REACTION ONLY SETTINGS")
                                            .default_open(true)
                                            .show(ui, |ui| {
                                                ui.label(egui::RichText::new(
                                                    "No throwable, no particle effect. Configure the Launch Audio (below) \
                                                     and the Impact Reaction (further down) — that's the whole redeem."
                                                ).italics().color(Color32::from_rgb(148, 163, 184)));
                                            });
                                    }
                                    "custom" => {
                                        egui::CollapsingHeader::new("📜 CUSTOM LUA SCRIPT SETTINGS")
                                            .default_open(true)
                                            .show(ui, |ui| {
                                                ui.label(egui::RichText::new(
                                                    "Runs this Lua script when the redeem fires (from its Reader event trigger or the Test button). \
                                                     The script receives redeem parameters (`params.target`, `params.trigger`, `params.user`, `params.wildcards`) \
                                                     and can call `trust.trigger(...)`, `trust.spawn_water(...)` (metaballs/SPH), `trust.spawn_throwable(...)`, \
                                                     `trust.spawn_fire(...)`, `trust.spawn_lightning(...)`, `trust.sleep(...)` to compose custom redeems."
                                                ).italics().color(Color32::from_rgb(148, 163, 184)));
                                                ui.add_space(6.0);

                                                if item.config.custom_redeem_script.is_none() {
                                                    if let Some(entry) = self.custom_redeem_scripts_list.iter().find(|e| e.name.eq_ignore_ascii_case("snow_storm")).or_else(|| self.custom_redeem_scripts_list.first()) {
                                                        item.config.custom_redeem_script = Some(entry.path.to_string_lossy().to_string());
                                                        item.is_dirty = true;
                                                    }
                                                }

                                                let cur_label = item.config.custom_redeem_script.as_deref()
                                                    .and_then(|p| std::path::Path::new(p).file_name())
                                                    .map(|n| n.to_string_lossy().to_string())
                                                    .unwrap_or_else(|| "(none selected)".to_string());

                                                ui.horizontal(|ui| {
                                                    ui.label("Script:");
                                                    egui::ComboBox::from_id_salt("custom_redeem_script_combo")
                                                        .selected_text(cur_label)
                                                        .show_ui(ui, |ui| {
                                                            if self.custom_redeem_scripts_list.is_empty() {
                                                                ui.label(egui::RichText::new("No custom redeem scripts found on disk.").italics());
                                                            }
                                                            for entry in &self.custom_redeem_scripts_list {
                                                                let path_str = entry.path.to_string_lossy().to_string();
                                                                let is_sel = item.config.custom_redeem_script.as_deref() == Some(path_str.as_str());
                                                                if ui.selectable_label(is_sel, format!("📜 {}", entry.name)).clicked() {
                                                                    item.config.custom_redeem_script = Some(path_str);
                                                                    item.is_dirty = true;
                                                                }
                                                            }
                                                        });

                                                    if ui.button("➕ Browse...").on_hover_text("Select a .lua script file from disk").clicked() {
                                                        custom_redeem_browse_clicked = true;
                                                    }
                                                });

                                                ui.add_space(4.0);
                                                ui.horizontal(|ui| {
                                                    ui.label("⚡ Speed:");
                                                    let mut spd = item.config.speed.unwrap_or(8.0);
                                                    if ui.add(egui::Slider::new(&mut spd, 0.5..=30.0).step_by(0.5).suffix("x")).changed() {
                                                        item.config.speed = Some(spd);
                                                        item.is_dirty = true;
                                                    }
                                                });

                                                ui.horizontal(|ui| {
                                                    ui.label("⏱ Duration:");
                                                    let mut dur = item.config.duration.unwrap_or(5000);
                                                    if ui.add(egui::Slider::new(&mut dur, 500..=30000).step_by(250.0).suffix(" ms")).changed() {
                                                        item.config.duration = Some(dur);
                                                        item.is_dirty = true;
                                                    }
                                                });

                                                // The script's own sliders (its `-- @param` lines)
                                                let script_path = item.config.custom_redeem_script.clone();
                                                let info = script_path
                                                    .as_deref()
                                                    .map(|p| script_info(std::path::Path::new(&resolve_script_path(p))))
                                                    .unwrap_or_default();
                                                for slider in &info.sliders {
                                                    ui.horizontal(|ui| {
                                                        ui.label(format!("{}:", slider.label));
                                                        let mut value = item.config.script_params.as_ref()
                                                            .and_then(|m| m.get(&slider.name))
                                                            .copied()
                                                            .unwrap_or(slider.default);
                                                        let changed = ui.add(
                                                            egui::Slider::new(&mut value, slider.min..=slider.max)
                                                                .clamping(egui::SliderClamping::Edits),
                                                        ).changed();
                                                        if changed {
                                                            item.config.script_params
                                                                .get_or_insert_with(Default::default)
                                                                .insert(slider.name.clone(), value);
                                                            item.is_dirty = true;
                                                        }
                                                        if ui.small_button("↺").on_hover_text("Back to the script's default").clicked() {
                                                            if let Some(m) = item.config.script_params.as_mut() {
                                                                m.remove(&slider.name);
                                                            }
                                                            item.is_dirty = true;
                                                        }
                                                    });
                                                }

                                                // What the Theater Shield blocks in it
                                                if !info.threats.is_empty() {
                                                    let calls: Vec<String> = info.threats.iter().map(|t| format!("{} (line {})", t.call, t.line)).collect();
                                                    let why: Vec<String> = info.threats.iter().map(|t| format!("{}: it {}", t.call, t.why)).collect();
                                                    ui.label(
                                                        egui::RichText::new(format!("🛡 Blocked when it runs: {}", calls.join(", ")))
                                                            .color(Color32::from_rgb(251, 146, 60)),
                                                    )
                                                    .on_hover_text(why.join("\n"));
                                                }

                                                if item.config.custom_redeem_script.is_none() {
                                                    ui.label(egui::RichText::new("⚠️ No script selected! Choose or browse for a script above.").color(Color32::from_rgb(251, 146, 60)));
                                                }
                                            });
                                    }
                                    _ => {
                                        // Default "image" (throwable)
                                        egui::CollapsingHeader::new("🚀 IMAGE / THROWABLE SETTINGS")
                                            .default_open(true)
                                            .show(ui, |ui| {
                                                // Throw Action
                                                ui.horizontal(|ui| {
                                                    ui.label("🚀 Throw Action:");
                                                    let cur_action = item.config.action.clone().unwrap_or_else(|| "throwable".to_string());
                                                    let action_label = match cur_action.as_str() {
                                                        "throw-front" => "3D Forward Zoom (throw-front)",
                                                        "swirl" => "Vortex Spiral (swirl)",
                                                        "drop" => "Straight Drop (drop)",
                                                        "rain" => "Shower Rain (rain)",
                                                        _ => "Arc Throw (throwable)",
                                                    };
                                                    egui::ComboBox::from_id_salt("throw_action_combo")
                                                        .selected_text(action_label)
                                                        .show_ui(ui, |ui| {
                                                            for (val, lbl) in [
                                                                ("throwable", "Arc Throw (throwable)"),
                                                                ("throw-front", "3D Forward Zoom (throw-front)"),
                                                                ("swirl", "Vortex Spiral (swirl)"),
                                                                ("drop", "Straight Drop (drop)"),
                                                                ("rain", "Shower Rain (rain)"),
                                                            ] {
                                                                if ui.selectable_label(cur_action == val, lbl).clicked() {
                                                                    item.config.action = Some(val.to_string());
                                                                    item.is_dirty = true;
                                                                }
                                                            }
                                                        });
                                                });

                                                // Collision Behavior
                                                ui.horizontal(|ui| {
                                                    ui.label("💥 Collision:");
                                                    let cur_col = item.config.collision.clone().unwrap_or_else(|| "bounce".to_string());
                                                    let col_label = match cur_col.as_str() {
                                                        "stick" => "Stick to Avatar (stick)",
                                                        "splat" => "Splat & Fade (splat)",
                                                        "shatter" | "pixel-shatter" => "Spin & Shatter (shatter)",
                                                        _ => "Physics Bounce (bounce)",
                                                    };
                                                    egui::ComboBox::from_id_salt("collision_combo")
                                                        .selected_text(col_label)
                                                        .show_ui(ui, |ui| {
                                                            for (val, lbl) in [
                                                                ("bounce", "Physics Bounce (bounce)"),
                                                                ("stick", "Stick to Avatar (stick)"),
                                                                ("splat", "Splat & Fade (splat)"),
                                                                ("shatter", "Spin & Shatter (shatter)"),
                                                            ] {
                                                                if ui.selectable_label(cur_col == val || (val == "shatter" && cur_col == "pixel-shatter"), lbl).clicked() {
                                                                    item.config.collision = Some(val.to_string());
                                                                    item.is_dirty = true;
                                                                }
                                                            }
                                                        });
                                                });

                                                // Speed: 1.0 to 50.0 (step 0.5)
                                                let mut spd = item.config.speed.unwrap_or(8.0);
                                                if ui.add(egui::Slider::new(&mut spd, 1.0..=50.0).step_by(0.5).suffix(" m/s").text("⚡ Flight Speed")).changed() {
                                                    item.config.speed = Some(spd);
                                                    item.is_dirty = true;
                                                }

                                                // Scale: 0.01 to 20.0 (step 0.05)
                                                let mut sc = item.config.scale.unwrap_or(1.0);
                                                if ui.add(egui::Slider::new(&mut sc, 0.01..=20.0).step_by(0.05).suffix("x").text("📏 Object Scale")).changed() {
                                                    item.config.scale = Some(sc);
                                                    item.is_dirty = true;
                                                }

                                                // Gravity: 0.0 to 10.0 (step 0.1)
                                                let mut grv = item.config.gravity.unwrap_or(0.8);
                                                if ui.add(egui::Slider::new(&mut grv, 0.0..=10.0).step_by(0.1).suffix("x").text("🌍 Gravity Arc")).changed() {
                                                    item.config.gravity = Some(grv);
                                                    item.is_dirty = true;
                                                }

                                                // Path Randomness: 0% to 100%, saved as 0–1
                                                let mut rnd = item.config.path_randomness.unwrap_or(0.0) * 100.0;
                                                if ui.add(egui::Slider::new(&mut rnd, 0.0..=100.0).step_by(5.0).suffix("%").text("🎲 Path Randomness"))
                                                    .on_hover_text("How different each throw is: where along its edge it starts, where on the target it lands, how high it arcs. 0% throws every one from a spawn point on the same path")
                                                    .changed()
                                                {
                                                    item.config.path_randomness = Some(rnd / 100.0);
                                                    item.is_dirty = true;
                                                }


                                                // Amount of objects thrown: supports number or wildcard {1}
                                                ui.horizontal(|ui| {
                                                    ui.label("📦 Amount:");
                                                    let mut amt_str = match &item.config.amount {
                                                        Some(serde_json::Value::String(s)) => s.clone(),
                                                        Some(serde_json::Value::Number(n)) => n.to_string(),
                                                        _ => "1".to_string(),
                                                    };
                                                    if ui.add(
                                                        egui::TextEdit::singleline(&mut amt_str)
                                                            .hint_text("1 or {1}")
                                                            .desired_width(65.0)
                                                    ).on_hover_text("Number of objects thrown (e.g. 1, 5), or {1} to use the value the trigger's first * matched in the event").changed() {
                                                        item.config.amount = Some(serde_json::Value::String(amt_str));
                                                        item.is_dirty = true;
                                                    }

                                                    ui.label("Delay:");
                                                    let mut delay = item.config.repeat_delay.unwrap_or(150);
                                                    if ui.add(egui::DragValue::new(&mut delay).range(10..=2000).suffix(" ms")).changed() {
                                                        item.config.repeat_delay = Some(delay);
                                                        item.is_dirty = true;
                                                    }
                                                });

                                                // Spawn Locations preset
                                                ui.horizontal(|ui| {
                                                    ui.label("🎯 Spawn Locations:");
                                                    let pts = item.config.spawn_points.get_or_insert_with(|| vec!["tl".into(), "tc".into(), "tr".into(), "ml".into(), "mr".into()]);
                                                    let current_preset = if pts.len() == 1 && pts[0] == "tc" {
                                                        "Center Drop"
                                                    } else if pts.len() == 3 && pts.contains(&"tl".into()) && pts.contains(&"tc".into()) && pts.contains(&"tr".into()) {
                                                        "Top Edge Only"
                                                    } else if pts.len() == 2 && pts.contains(&"ml".into()) && pts.contains(&"mr".into()) {
                                                        "Left & Right Sides Only"
                                                    } else if pts.len() >= 5 {
                                                        "All Borders (Top+Sides)"
                                                    } else {
                                                        "Custom Selection"
                                                    };

                                                    egui::ComboBox::from_id_salt("spawn_preset_combo")
                                                        .selected_text(current_preset)
                                                        .show_ui(ui, |ui| {
                                                            if ui.selectable_label(current_preset == "All Borders (Top+Sides)", "All Borders (Top+Sides)").clicked() {
                                                                *pts = vec!["tl".into(), "tc".into(), "tr".into(), "ml".into(), "mr".into()];
                                                                item.is_dirty = true;
                                                            }
                                                            if ui.selectable_label(current_preset == "Top Edge Only", "Top Edge Only").clicked() {
                                                                *pts = vec!["tl".into(), "tc".into(), "tr".into()];
                                                                item.is_dirty = true;
                                                            }
                                                            if ui.selectable_label(current_preset == "Left & Right Sides Only", "Left & Right Sides Only").clicked() {
                                                                *pts = vec!["ml".into(), "mr".into()];
                                                                item.is_dirty = true;
                                                            }
                                                            if ui.selectable_label(current_preset == "Center Drop", "Center Drop").clicked() {
                                                                *pts = vec!["tc".into()];
                                                                item.is_dirty = true;
                                                            }
                                                        });
                                                });

                                                // Spawn points fine-tuning badges
                                                let pts = item.config.spawn_points.get_or_insert_with(Vec::new);
                                                ui.horizontal_wrapped(|ui| {
                                                    ui.label("Points:");
                                                    for p in &["tl", "tc", "tr", "ml", "mr", "bl", "bc", "br"] {
                                                        let mut active = pts.iter().any(|x| x == *p);
                                                        if ui.checkbox(&mut active, *p).changed() {
                                                            if active { pts.push(p.to_string()); } else { pts.retain(|x| x != *p); }
                                                            item.is_dirty = true;
                                                        }
                                                    }
                                                });

                                                // Booleans
                                                let mut mirror = item.config.random_mirror.unwrap_or(true);
                                                if ui.checkbox(&mut mirror, "🪞 Random Horizontal Mirror").changed() {
                                                    item.config.random_mirror = Some(mirror);
                                                    item.is_dirty = true;
                                                }

                                                let mut snap = item.config.snap_bottom.unwrap_or(false);
                                                if ui.checkbox(&mut snap, "⚓ Snap Image's Bottom to Avatar")
                                                    .on_hover_text("Keeps the bottom of the image continuously aimed at the avatar / target center during flight")
                                                    .changed()
                                                {
                                                    item.config.snap_bottom = Some(snap);
                                                    item.is_dirty = true;
                                                }

                                                // Local Image / GIF File
                                                ui.horizontal(|ui| {
                                                    ui.label("🖼️ Image Asset:");
                                                    let mut path_str = item.config.image_file_path.clone().unwrap_or_default();
                                                    if ui.add(
                                                        egui::TextEdit::singleline(&mut path_str)
                                                            .hint_text("Path e.g. images/tomato.png")
                                                            .desired_width(170.0)
                                                    ).changed() {
                                                        item.config.image_file_path = Some(path_str.clone());
                                                        item.is_dirty = true;
                                                        if let Some(b64) = resolve_and_encode_asset(&path_str) {
                                                            item.config.image_src = Some(b64);
                                                            self.redeem_editor_status = format!("Loaded image: {}", path_str);
                                                        }
                                                    }

                                                    if ui.button("📂 Load Object...").on_hover_text("Open system file dialog to choose an image or GIF asset").clicked() {
                                                        let tx = self.file_dialog_tx.clone();
                                                        let ctx_clone = ui.ctx().clone();
                                                        std::thread::spawn(move || {
                                                            if let Some(path) = pick_file_dialog("Select Image or GIF Object", "Images (*.png, *.jpg, *.webp, *.gif) | *.png *.jpg *.jpeg *.webp *.gif") {
                                                                let _ = tx.send(FileDialogResult {
                                                                    target: FileDialogTarget::Image,
                                                                    path,
                                                                });
                                                                ctx_clone.request_repaint();
                                                            }
                                                        });
                                                    }
                                                });

                                                if let Some(ref b64) = item.config.image_src {
                                                    ui.label(egui::RichText::new(format!("✓ Embedded Asset ({} KB)", b64.len() / 1024)).color(Color32::from_rgb(34, 197, 94)).size(11.0));
                                                }
                                            });
                                    }
                                }

                                ui.add_space(8.0);

                                // 3. Screen Shake & Audio
                                egui::CollapsingHeader::new("🔊 AUDIO & SCREEN SHAKE")
                                    .default_open(true)
                                    .show(ui, |ui| {
                                        // Universal Launch Audio (Available for ALL redeem types)
                                        ui.horizontal(|ui| {
                                            ui.label("🔊 Launch Audio:");
                                            let mut path_str = item.config.start_sound_path.clone().unwrap_or_default();
                                            if ui.add(
                                                egui::TextEdit::singleline(&mut path_str)
                                                    .hint_text("Path e.g. sounds/throw.ogg")
                                                    .desired_width(170.0)
                                            ).changed() {
                                                item.config.start_sound_path = Some(path_str.clone());
                                                item.is_dirty = true;
                                                if let Some(b64) = resolve_and_encode_asset(&path_str) {
                                                    item.config.start_sound = Some(b64);
                                                    self.redeem_editor_status = format!("Loaded launch audio: {}", path_str);
                                                }
                                            }

                                            if ui.button("📂 Load Object...").on_hover_text("Open system file dialog to choose launch audio for this redeem").clicked() {
                                                let tx = self.file_dialog_tx.clone();
                                                let ctx_clone = ui.ctx().clone();
                                                std::thread::spawn(move || {
                                                    if let Some(path) = pick_file_dialog("Select Launch Audio Object", "Audio (*.ogg, *.wav, *.mp3, *.aac, *.flac) | *.ogg *.wav *.mp3 *.aac *.flac") {
                                                        let _ = tx.send(FileDialogResult {
                                                            target: FileDialogTarget::LaunchSound,
                                                            path,
                                                        });
                                                        ctx_clone.request_repaint();
                                                    }
                                                });
                                            }

                                            let snd_candidate = item.config.start_sound_path.as_deref().or(item.config.start_sound.as_deref());
                                            if let Some(s) = snd_candidate {
                                                if !s.is_empty() && ui.button("▶ Preview").on_hover_text("Preview launch audio playback").clicked() {
                                                    crate::play_sound_str(s);
                                                }
                                            }
                                        });

                                        let is_throwable = item.config.redeem_type == "image" || item.config.redeem_type == "throwable";
                                        if is_throwable {
                                            ui.horizontal(|ui| {
                                                ui.label("💥 Impact Audio:");
                                                let mut path_str = item.config.collision_sound_path.clone().unwrap_or_default();
                                                if ui.add(
                                                    egui::TextEdit::singleline(&mut path_str)
                                                        .hint_text("Path e.g. sounds/splat.ogg")
                                                        .desired_width(170.0)
                                                ).changed() {
                                                    item.config.collision_sound_path = Some(path_str.clone());
                                                    item.is_dirty = true;
                                                    if let Some(b64) = resolve_and_encode_asset(&path_str) {
                                                        item.config.collision_sound = Some(b64);
                                                        self.redeem_editor_status = format!("Loaded impact audio: {}", path_str);
                                                    }
                                                }

                                                if ui.button("📂 Load Object...").on_hover_text("Open system file dialog to choose impact audio (throwables only)").clicked() {
                                                    let tx = self.file_dialog_tx.clone();
                                                    let ctx_clone = ui.ctx().clone();
                                                    std::thread::spawn(move || {
                                                        if let Some(path) = pick_file_dialog("Select Impact Audio Object", "Audio (*.ogg, *.wav, *.mp3, *.aac, *.flac) | *.ogg *.wav *.mp3 *.aac *.flac") {
                                                            let _ = tx.send(FileDialogResult {
                                                                target: FileDialogTarget::CollisionSound,
                                                                path,
                                                            });
                                                            ctx_clone.request_repaint();
                                                        }
                                                    });
                                                }

                                                let col_candidate = item.config.collision_sound_path.as_deref().or(item.config.collision_sound.as_deref());
                                                if let Some(s) = col_candidate {
                                                    if !s.is_empty() && ui.button("▶ Preview").on_hover_text("Preview impact audio playback").clicked() {
                                                        crate::play_sound_str(s);
                                                    }
                                                }
                                            });
                                        }

                                        ui.add_space(4.0);
                                        let mut shake = item.config.get_shake_enabled();
                                        if ui.checkbox(&mut shake, "📳 Screen Shake").changed() {
                                            item.config.set_shake_enabled(shake); item.is_dirty = true;
                                        }

                                        if shake {
                                            // Flamethrower mode now does real AABB collision against the
                                            // target's hitbox (fire.rs step_internal), so it has a genuine
                                            // "contact" moment too — unlike campfire fire, water, or lightning.
                                            let is_flamethrower = item.config.redeem_type == "webgl-fire"
                                                && item.config.stream_mode.as_deref() == Some("grid");
                                            let supports_impact = is_throwable || is_flamethrower;
                                            let mut timing = item.config.get_shake_timing();
                                            if !supports_impact && timing == "impact" {
                                                timing = "start".to_string();
                                                item.config.set_shake_timing(&timing);
                                                item.is_dirty = true;
                                            }
                                            ui.horizontal(|ui| {
                                                ui.label("⏰ Shake Timing:");
                                                egui::ComboBox::from_id_salt("shake_timing_combo")
                                                    .selected_text(if timing == "start" { "On Start" } else { "On Contact" })
                                                    .show_ui(ui, |ui| {
                                                        if supports_impact {
                                                            let label = if is_flamethrower {
                                                                "On Contact (stream reaches target)"
                                                            } else {
                                                                "On Contact (throwable hits target)"
                                                            };
                                                            if ui.selectable_label(timing == "impact", label).clicked() {
                                                                item.config.set_shake_timing("impact"); item.is_dirty = true;
                                                            }
                                                        }
                                                        if ui.selectable_label(timing == "start", "On Start (fires immediately)").clicked() {
                                                            item.config.set_shake_timing("start"); item.is_dirty = true;
                                                        }
                                                    });
                                            });

                                            let mut dur = item.config.get_shake_duration();
                                            if ui.add(egui::Slider::new(&mut dur, 50..=1000).step_by(25.0).suffix(" ms").text("⏱️ Shake Duration")).changed() {
                                                item.config.set_shake_duration(dur); item.is_dirty = true;
                                            }

                                            let mut inten = item.config.get_shake_intensity();
                                            if ui.add(egui::Slider::new(&mut inten, 1..=50).text("💥 Shake Intensity")).changed() {
                                                item.config.set_shake_intensity(inten); item.is_dirty = true;
                                            }
                                        }
                                    });

                                ui.add_space(8.0);

                                // 4. Avatar Impact Reactions
                                egui::CollapsingHeader::new("💥 IMPACT REACTIONS")
                                    .default_open(true)
                                    .show(ui, |ui| {
                                        // These are mutually exclusive reaction *styles*, not independent
                                        // toggles — stacking bounce+drip+jolt on the same impact never made sense.
                                        #[derive(PartialEq, Clone, Copy)]
                                        enum ReactionKind { None, Bounce, Drip, Jolt, Custom(usize) }
                                        let current = if item.config.get_bounce_enabled() {
                                            ReactionKind::Bounce
                                        } else if item.config.get_drip_enabled() {
                                            ReactionKind::Drip
                                        } else if item.config.get_jolt_enabled() {
                                            ReactionKind::Jolt
                                        } else if item.config.get_custom_script_enabled() {
                                            let saved_path = item.config.get_custom_script_path();
                                            match saved_path.and_then(|p| available_lua_effects.iter().position(|e| e.path.to_string_lossy() == p)) {
                                                Some(idx) => ReactionKind::Custom(idx),
                                                None => ReactionKind::None, // script was removed from the Lua Effects list
                                            }
                                        } else {
                                            ReactionKind::None
                                        };
                                        let label = |k: &ReactionKind| match k {
                                            ReactionKind::None => "None".to_string(),
                                            ReactionKind::Bounce => "🏀 Bounce & Squish".to_string(),
                                            ReactionKind::Drip => "💧 Fluid Pixel Drip".to_string(),
                                            ReactionKind::Jolt => "⚡ Voltage Jolt & Flash".to_string(),
                                            ReactionKind::Custom(idx) => available_lua_effects.get(*idx)
                                                .map(|e| format!("🎨 {}", e.name))
                                                .unwrap_or_else(|| "🎨 (missing script)".to_string()),
                                        };
                                        let mut selected = current;
                                        egui::ComboBox::from_label("Impact Reaction")
                                            .selected_text(label(&selected))
                                            .show_ui(ui, |ui| {
                                                for k in [ReactionKind::None, ReactionKind::Bounce, ReactionKind::Drip, ReactionKind::Jolt] {
                                                    let text = label(&k);
                                                    if ui.selectable_label(selected == k, text).clicked() {
                                                        selected = k;
                                                    }
                                                }
                                                if !available_lua_effects.is_empty() {
                                                    ui.separator();
                                                    for idx in 0..available_lua_effects.len() {
                                                        let k = ReactionKind::Custom(idx);
                                                        let text = label(&k);
                                                        if ui.selectable_label(selected == k, text).clicked() {
                                                            selected = k;
                                                        }
                                                    }
                                                }
                                            });
                                        if selected != current {
                                            item.config.set_bounce_enabled(selected == ReactionKind::Bounce);
                                            item.config.set_drip_enabled(selected == ReactionKind::Drip);
                                            item.config.set_jolt_enabled(selected == ReactionKind::Jolt);
                                            if let ReactionKind::Custom(idx) = selected {
                                                if let Some(entry) = available_lua_effects.get(idx) {
                                                    item.config.set_custom_script(&entry.path.to_string_lossy(), &entry.name);
                                                }
                                            } else {
                                                item.config.clear_custom_script();
                                            }
                                            item.is_dirty = true;
                                        }

                                        if selected != ReactionKind::None {
                                            let is_throwable = item.config.redeem_type == "image" || item.config.redeem_type == "throwable";
                                            // Flamethrower mode now does real AABB collision against the
                                            // target's hitbox (fire.rs step_internal), so it has a genuine
                                            // "contact" moment too — unlike campfire fire, water, or lightning.
                                            let is_flamethrower = item.config.redeem_type == "webgl-fire"
                                                && item.config.stream_mode.as_deref() == Some("grid");
                                            let supports_impact = is_throwable || is_flamethrower;
                                            let mut timing = item.config.get_reaction_timing();
                                            if !supports_impact && timing == "impact" {
                                                timing = "start".to_string();
                                                item.config.set_reaction_timing(&timing);
                                                item.is_dirty = true;
                                            }
                                            ui.horizontal(|ui| {
                                                ui.label("⏰ Reaction Timing:");
                                                egui::ComboBox::from_id_salt("reaction_timing_combo")
                                                    .selected_text(if timing == "start" { "On Start" } else { "On Contact" })
                                                    .show_ui(ui, |ui| {
                                                        if supports_impact {
                                                            let label = if is_flamethrower {
                                                                "On Contact (stream reaches target)"
                                                            } else {
                                                                "On Contact (throwable hits target)"
                                                            };
                                                            if ui.selectable_label(timing == "impact", label).clicked() {
                                                                item.config.set_reaction_timing("impact"); item.is_dirty = true;
                                                            }
                                                        }
                                                        if ui.selectable_label(timing == "start", "On Start (fires immediately)").clicked() {
                                                            item.config.set_reaction_timing("start"); item.is_dirty = true;
                                                        }
                                                    });
                                            });
                                        }

                                        match selected {
                                            ReactionKind::Bounce => {
                                                let mut squish = item.config.get_bounce_squish();
                                                if ui.add(egui::Slider::new(&mut squish, 0.0..=1.0).step_by(0.05).text("🏀 Squish Intensity")).changed() {
                                                    item.config.set_bounce_squish(squish); item.is_dirty = true;
                                                }
                                                let mut bdur = item.config.get_bounce_duration();
                                                if ui.add(egui::Slider::new(&mut bdur, 100..=2000).step_by(50.0).suffix(" ms").text("⏱️ Bounce Duration")).changed() {
                                                    item.config.set_bounce_duration(bdur); item.is_dirty = true;
                                                }
                                            }
                                            ReactionKind::Drip => {
                                                let mut dvisc = item.config.get_drip_viscosity();
                                                if ui.add(egui::Slider::new(&mut dvisc, 0.1..=2.0).step_by(0.05).text("💧 Drip Viscosity")).changed() {
                                                    item.config.set_drip_viscosity(dvisc); item.is_dirty = true;
                                                }
                                                let mut ddur = item.config.get_drip_duration();
                                                if ui.add(egui::Slider::new(&mut ddur, 500..=10000).step_by(100.0).suffix(" ms").text("⏱️ Drip Duration")).changed() {
                                                    item.config.set_drip_duration(ddur); item.is_dirty = true;
                                                }
                                            }
                                            ReactionKind::Jolt => {
                                                let mut jinten = item.config.get_jolt_intensity();
                                                if ui.add(egui::Slider::new(&mut jinten, 0.1..=2.0).step_by(0.05).text("⚡ Voltage Jolt Intensity")).changed() {
                                                    item.config.set_jolt_intensity(jinten); item.is_dirty = true;
                                                }
                                                let mut jdur = item.config.get_jolt_duration();
                                                if ui.add(egui::Slider::new(&mut jdur, 100..=2000).step_by(50.0).suffix(" ms").text("⏱️ Jolt Duration")).changed() {
                                                    item.config.set_jolt_duration(jdur); item.is_dirty = true;
                                                }
                                            }
                                            ReactionKind::Custom(idx) => {
                                                let mut cdur = item.config.get_custom_script_duration();
                                                // Up to a minute: bite_vanish heals its bites over it
                                                if ui.add(egui::Slider::new(&mut cdur, 200..=60000).step_by(100.0).suffix(" ms").text("⏱️ Reaction Duration")).changed() {
                                                    item.config.set_custom_script_duration(cdur); item.is_dirty = true;
                                                }
                                                if let Some(entry) = available_lua_effects.get(idx) {
                                                    match std::fs::read_to_string(&entry.path) {
                                                        Ok(lua_source) => {
                                                            let defaults = crate::obs_source::extract_default_floats_from_lua(&lua_source);
                                                            if defaults.is_empty() {
                                                                ui.label(egui::RichText::new("This script declared no tunable settings (only \"time\" drives it).").italics().color(Color32::from_rgb(148, 163, 184)));
                                                            }
                                                            for (name, default_val) in &defaults {
                                                                if name == "time" {
                                                                    continue;
                                                                }
                                                                let mut val = item.config.get_custom_script_param(name, *default_val);
                                                                let range = if *default_val > 0.0 { 0.0..=(*default_val * 4.0).max(1.0) } else { -2.0..=2.0 };
                                                                if ui.add(egui::Slider::new(&mut val, range).text(name)).changed() {
                                                                    item.config.set_custom_script_param(name, val);
                                                                    item.is_dirty = true;
                                                                }
                                                            }
                                                        }
                                                        Err(e) => {
                                                            ui.colored_label(Color32::from_rgb(239, 68, 68), format!("⚠ Could not read script: {}", e));
                                                        }
                                                    }
                                                }
                                            }
                                            ReactionKind::None => {}
                                        }
                                    });

                                ui.add_space(8.0);

                                // 5. Action Buttons
                                egui::CollapsingHeader::new("🚀 ACTIONS & LIVE TESTING")
                                    .default_open(true)
                                    .show(ui, |ui| {
                                        if ui.button(
                                            egui::RichText::new("Test")
                                                .color(Color32::WHITE)
                                                .strong()
                                                .size(14.0)
                                        ).on_hover_text("Execute full live test of this redeem package in the studio viewport").clicked() {
                                            test_launch_clicked = true;
                                        }

                                        ui.add_space(6.0);
                                        ui.horizontal(|ui| {
                                            if ui.button(
                                                egui::RichText::new("💾 Save / Export Redeem Package")
                                                    .color(Color32::from_rgb(34, 197, 94))
                                                    .strong()
                                            ).clicked() {
                                                save_clicked = true;
                                            }

                                            if ui.button(
                                                egui::RichText::new("🗑️ Delete Redeem File")
                                                    .color(Color32::from_rgb(239, 68, 68))
                                            ).on_hover_text("Permanently delete this redeem file from disk").clicked() {
                                                delete_clicked = true;
                                            }
                                        });
                                    });
                            });
                    }
                } else {
                    ui.heading("⚙ Redeem Configuration");
                    ui.separator();
                    ui.label("No redeem selected. Pick one in the 🎁 Redeem dropdown above, or click ➕ New.");
                }

                ui.add_space(8.0);
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(&self.redeem_editor_status).size(11.5).color(Color32::from_rgb(148, 163, 184)));
                });
            });

        // Handle Save
        if save_clicked {
            if let Some(idx) = self.selected_redeem_index {
                if let Some(item) = self.redeems_list.get_mut(idx) {
                    match save_redeem_to_disk(item) {
                        Ok(_) => {
                            self.redeem_editor_status = format!("Saved {}", item.filename);
                        }
                        Err(e) => {
                            self.redeem_editor_status = format!("Save error: {}", e);
                        }
                    }
                }
            }
        }

        // Handle Export: a .red package (redeem_package), written by its own
        // thread once the Save As dialog has an answer
        if export_clicked {
            if let Some(item) = self.selected_redeem_index.and_then(|i| self.redeems_list.get(i)) {
                let mut config = item.config.clone();
                config.triggers = parse_triggers(&item.triggers_input);
                // Named like the redeem, which is what an import names it
                let stem = std::path::Path::new(&item.filename)
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let name = if config.name.trim().is_empty() || config.name == RedeemConfig::default().name {
                    stem
                } else {
                    crate::redeem_package::file_stem_from(&config.name)
                };
                let tx = self.package_tx.clone();
                let ctx = ctx.clone();
                std::thread::spawn(move || {
                    let Some(mut file) = save_redeem_package_dialog(&format!("{}.red", name)) else { return };
                    if file.extension().is_none() {
                        file.set_extension("red");
                    }
                    let result = crate::redeem_package::export(&config, &file);
                    let _ = tx.send(PackageEvent::Exported { file, result });
                    ctx.request_repaint();
                });
            }
        }
        if copy_json_clicked {
            if let Some(item) = self.selected_redeem_index.and_then(|i| self.redeems_list.get(i)) {
                if let Ok(json_str) = serde_json::to_string_pretty(&item.config) {
                    ctx.copy_text(json_str);
                    self.redeem_editor_status = format!("Copied {}'s settings to the clipboard", item.filename);
                }
            }
        }

        // Handle Delete
        if delete_clicked {
            if let Some(idx) = self.selected_redeem_index {
                if idx < self.redeems_list.len() {
                    let removed = self.redeems_list.remove(idx);
                    let _ = delete_redeem_file(&removed.path);
                    self.redeem_editor_status = format!("Deleted {}", removed.filename);
                    if self.redeems_list.is_empty() {
                        self.selected_redeem_index = None;
                    } else {
                        self.selected_redeem_index = Some(idx.min(self.redeems_list.len() - 1));
                    }
                }
            }
        }

        // Handle Live Test Launch
        if test_launch_clicked {
            if let Some(idx) = self.selected_redeem_index {
                if let Some(item) = self.redeems_list.get(idx) {
                    if let Some(json_str) = build_redeem_dispatch_json(&item.filename, &item.config) {
                        // Dispatch directly into native Rust physics / effects
                        if dispatch_test_redeem_json(&json_str) {
                            println!("[Trust Studio] 🚀 Launched test redeem '{}' ({})", item.config.name, item.config.redeem_type);
                            self.redeem_editor_status = format!("🚀 Launched test: {} ({})", item.config.name, item.config.redeem_type);
                            ctx.request_repaint();
                        } else {
                            self.redeem_editor_status = format!("⚠️ Test launch failed: {}", item.config.name);
                        }
                    }
                }
            }
        }

        // 3. Central Clean Live OBS Render Viewport with Real-time Simulation Overlay
        egui::CentralPanel::default().show(ctx, |ui| {
            let available_size = ui.available_size();
            let (rect, _response) = ui.allocate_exact_size(available_size, egui::Sense::hover());
            let painter = ui.painter_at(rect);

            let canvas_w = cw.max(1) as f32;
            let canvas_h = ch.max(1) as f32;
            let aspect = canvas_w / canvas_h;

            let draw_w = (rect.width()).min(rect.height() * aspect);
            let draw_h = draw_w / aspect;
            let off_x = rect.min.x + (rect.width() - draw_w) / 2.0;
            let off_y = rect.min.y + (rect.height() - draw_h) / 2.0;
            let vp_rect = Rect::from_min_size(Pos2::new(off_x, off_y), Vec2::new(draw_w, draw_h));

            // Viewport background
            painter.rect_filled(rect, 0.0, Color32::from_rgb(15, 23, 42)); // Dark slate surround
            painter.rect_filled(vp_rect, 0.0, Color32::BLACK); // Solid black render background
            painter.rect_stroke(vp_rect, 0.0, Stroke::new(1.0, Color32::from_rgb(51, 65, 85)), egui::StrokeKind::Outside);

            // Clean live OBS composite frame render
            if let Some(ref tex) = self.preview_texture {
                painter.image(
                    tex.id(),
                    vp_rect,
                    Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(1.0, 1.0)),
                    Color32::WHITE,
                );
            } else {
                painter.text(
                    vp_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "Awaiting OBS live composite frame...",
                    egui::FontId::proportional(16.0),
                    Color32::from_rgb(148, 163, 184),
                );
            }

            // Fluids, fire, lightning, and throwables are now drawn natively into
            // the OBS "Throwables Layer" source itself (see obs_source.rs::
            // render_throwable_layer), so they already arrive baked into
            // `preview_texture` above via the live OBS composite capture —
            // drawing them again here would double them up.
        });
    }
}

pub fn is_studio_native_open() -> bool {
    IS_STUDIO_WINDOW_VISIBLE.load(std::sync::atomic::Ordering::Relaxed)
}

/// The Studio open on screen, not minimized: OBS may show the "Studio in
/// use" screen then.
pub fn is_studio_in_use() -> bool {
    if !IS_STUDIO_WINDOW_VISIBLE.load(std::sync::atomic::Ordering::Relaxed) {
        return false;
    }
    #[cfg(target_os = "windows")]
    {
        unsafe extern "system" {
            fn IsIconic(hWnd: isize) -> i32;
        }
        let hwnd = STUDIO_HWND.load(std::sync::atomic::Ordering::Relaxed);
        if hwnd != 0 && unsafe { IsIconic(hwnd) } != 0 {
            return false;
        }
    }
    true
}

/// On Windows a hidden window never receives WM_PAINT, and winit only emits
/// RedrawRequested from WM_PAINT — so while the Studio is hidden eframe never
/// runs `update()`, and STUDIO_SHOW_REQUEST would sit there unprocessed.
/// Showing the window natively gets painting going again; `update()` then
/// handles STUDIO_SHOW_REQUEST (Visible + Focus) exactly as on Linux.
#[cfg(target_os = "windows")]
fn show_studio_window_native() {
    unsafe extern "system" {
        fn ShowWindowAsync(hWnd: isize, nCmdShow: i32) -> i32;
        fn IsIconic(hWnd: isize) -> i32;
    }
    const SW_SHOW: i32 = 5;
    const SW_RESTORE: i32 = 9;
    let hwnd = STUDIO_HWND.load(std::sync::atomic::Ordering::SeqCst);
    if hwnd == 0 {
        return;
    }
    // Async: the window belongs to the Studio thread, so don't block the caller (OBS UI thread) on it
    unsafe {
        ShowWindowAsync(hwnd, if IsIconic(hwnd) != 0 { SW_RESTORE } else { SW_SHOW });
    }
}

pub fn launch_studio_native() {
    // If the window thread is already active, restore visibility and bring to front
    if IS_STUDIO_THREAD_RUNNING.load(std::sync::atomic::Ordering::SeqCst) {
        println!("[Trust Studio] 🎬 Scene Studio window thread active, restoring visibility & focusing...");
        IS_STUDIO_WINDOW_VISIBLE.store(true, std::sync::atomic::Ordering::SeqCst);
        STUDIO_SHOW_REQUEST.store(true, std::sync::atomic::Ordering::SeqCst);
        #[cfg(target_os = "windows")]
        show_studio_window_native();
        if let Some(ctx) = STUDIO_EGUI_CTX.lock().as_ref() {
            ctx.request_repaint();
        }
        return;
    }

    IS_STUDIO_THREAD_RUNNING.store(true, std::sync::atomic::Ordering::SeqCst);
    IS_STUDIO_WINDOW_VISIBLE.store(true, std::sync::atomic::Ordering::SeqCst);
    STUDIO_SHOW_REQUEST.store(true, std::sync::atomic::Ordering::SeqCst);
    spawn_studio_thread(false);
}

/// The Studio's thread runs (its window shown or not).
pub fn is_studio_thread_running() -> bool {
    IS_STUDIO_THREAD_RUNNING.load(std::sync::atomic::Ordering::SeqCst)
}

/// Starts the Studio's thread without showing its window, for the egui docks
/// (their windows are its). Opening the Studio later shows it as usual.
pub fn start_studio_hidden() {
    if IS_STUDIO_THREAD_RUNNING.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    STUDIO_HIDE_AT_START.store(true, std::sync::atomic::Ordering::SeqCst);
    STUDIO_OFF_SCREEN.store(true, std::sync::atomic::Ordering::SeqCst);
    spawn_studio_thread(true);
}

fn spawn_studio_thread(hidden: bool) {
    // Named so a panic in it is identifiable in trust_debug.log (see install_panic_logger)
    let _ = std::thread::Builder::new().name("trust-studio".into()).spawn(move || {
        crate::obs_source::trust_log("[Trust Studio] 🚀 Launching persistent native egui window...");
        let mut viewport = egui::ViewportBuilder::default()
            .with_inner_size([1240.0, 720.0])
            .with_min_inner_size([800.0, 500.0])
            .with_title("🎬 Theater Scene — Scene Studio");
        if hidden {
            // Shown for its first frame (eframe's doing), where no one sees it
            viewport = viewport.with_position([-20000.0, -20000.0]);
        }

        if let Some(icon) = get_app_icon_data() {
            viewport = viewport.with_icon(icon);
        }

        let native_options = eframe::NativeOptions {
            viewport,
            event_loop_builder: Some(Box::new(|_builder| {
                #[cfg(target_os = "linux")]
                {
                    use winit::platform::x11::EventLoopBuilderExtX11;
                    use winit::platform::wayland::EventLoopBuilderExtWayland;
                    // Prefer X11 via XWayland when DISPLAY is present for dependable window mapping / unmapping
                    if std::env::var("DISPLAY").map(|s| !s.is_empty()).unwrap_or(false) {
                        EventLoopBuilderExtX11::with_x11(_builder);
                        EventLoopBuilderExtX11::with_any_thread(_builder, true);
                    } else {
                        EventLoopBuilderExtWayland::with_any_thread(_builder, true);
                    }
                }
                #[cfg(target_os = "windows")]
                {
                    use winit::platform::windows::EventLoopBuilderExtWindows;
                    EventLoopBuilderExtWindows::with_any_thread(_builder, true);
                }
            })),
            ..Default::default()
        };
        let res = eframe::run_native(
            "Theater Scene Studio",
            native_options,
            Box::new(|cc| {
                *STUDIO_EGUI_CTX.lock() = Some(cc.egui_ctx.clone());
                #[cfg(target_os = "windows")]
                {
                    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
                    if let Ok(handle) = cc.window_handle() {
                        if let RawWindowHandle::Win32(win32) = handle.as_raw() {
                            STUDIO_HWND.store(win32.hwnd.get(), std::sync::atomic::Ordering::SeqCst);
                        }
                    }
                }
                Ok(Box::new(StudioApp::new(cc)))
            }),
        );
        if let Err(e) = res {
            eprintln!("[Trust Studio] ❌ eframe error: {:?}", e);
        }
        #[cfg(target_os = "windows")]
        STUDIO_HWND.store(0, std::sync::atomic::Ordering::SeqCst);
        *STUDIO_EGUI_CTX.lock() = None;
        IS_STUDIO_WINDOW_VISIBLE.store(false, std::sync::atomic::Ordering::SeqCst);
        IS_STUDIO_THREAD_RUNNING.store(false, std::sync::atomic::Ordering::SeqCst);
        crate::obs_source::trust_log("[Trust Studio] Native egui window thread terminated.");
    });
}

/// A slider a custom redeem script declares for the Studio's redeem editor,
/// with a comment line: `-- @param <name> <default> <min> <max> <label…>`
/// (e.g. `-- @param swirl_intensity 8 0 20 Swirl strength`). Read from the
/// file's text: the script is never run to find them.
#[derive(Debug, Clone, PartialEq)]
pub struct ScriptSlider {
    pub name: String,
    pub default: f64,
    pub min: f64,
    pub max: f64,
    pub label: String,
}

pub fn script_sliders(source: &str) -> Vec<ScriptSlider> {
    source
        .lines()
        .filter_map(|line| {
            let rest = line.trim().strip_prefix("--")?.trim().strip_prefix("@param")?;
            if !rest.starts_with(char::is_whitespace) {
                return None;
            }
            let mut parts = rest.split_whitespace();
            let name = parts.next()?.to_string();
            let default: f64 = parts.next()?.parse().ok()?;
            let min: f64 = parts.next()?.parse().ok()?;
            let max: f64 = parts.next()?.parse().ok()?;
            let label = parts.collect::<Vec<_>>().join(" ");
            (min < max).then(|| ScriptSlider {
                label: if label.is_empty() { name.clone() } else { label },
                name,
                default: default.clamp(min, max),
                min,
                max,
            })
        })
        .collect()
}

/// What the redeem editor reads from a custom redeem script's text.
#[derive(Clone, Default)]
struct ScriptInfo {
    sliders: Vec<ScriptSlider>,
    /// Calls the script makes that its sandbox blocks (lua_shield)
    threats: Vec<crate::lua_shield::LuaThreat>,
}

/// The script file at `path`, read again when the file changes.
fn script_info(path: &std::path::Path) -> ScriptInfo {
    thread_local! {
        static CACHE: std::cell::RefCell<std::collections::HashMap<std::path::PathBuf, (Option<std::time::SystemTime>, ScriptInfo)>> =
            std::cell::RefCell::new(std::collections::HashMap::new());
    }
    let modified = std::fs::metadata(path).and_then(|m| m.modified()).ok();
    CACHE.with_borrow_mut(|cache| {
        if let Some((when, info)) = cache.get(path) {
            if *when == modified {
                return info.clone();
            }
        }
        let info = std::fs::read_to_string(path)
            .map(|s| ScriptInfo { sliders: script_sliders(&s), threats: crate::lua_shield::scan(&s) })
            .unwrap_or_default();
        cache.insert(path.to_path_buf(), (modified, info.clone()));
        info
    })
}

/// Hands a custom redeem script the rest of its dispatch JSON as `params`
/// (fields already in `params` win), and the values of its own sliders
/// (`scriptParams`) as `params.<name>`.
fn add_dispatch_params(params: &mut serde_json::Value, dispatch: &serde_json::Value) {
    let (serde_json::Value::Object(p_map), serde_json::Value::Object(v_map)) = (params, dispatch) else {
        return;
    };
    let sliders = match v_map.get("scriptParams") {
        Some(serde_json::Value::Object(sliders)) => Some(sliders),
        _ => None,
    };
    for (k, v) in v_map.iter().chain(sliders.into_iter().flatten()) {
        if !p_map.contains_key(k) {
            p_map.insert(k.clone(), v.clone());
        }
    }
}

/// Serializes a redeem's config for dispatch, stamping in its filename as
/// `redeem_key` — the identity `physics::PhysicsEngine::spawn` actually uses
/// to cache/select each redeem's own image (see `redeem_key_to_id` /
/// `redeem_images`). Every redeem defaults to the same `name` ("New Redeem")
/// until a user renames it, so dispatching on `name` alone collapses
/// differently-imaged redeems onto one shared cache slot — whichever was
/// thrown most recently then overwrites what all of them display. The
/// filename (unique per file on disk) is a reliable identity `name` isn't.
pub fn build_redeem_dispatch_json(filename: &str, config: &RedeemConfig) -> Option<String> {
    let mut val = serde_json::to_value(config).ok()?;
    if let serde_json::Value::Object(ref mut map) = val {
        let stem = std::path::Path::new(filename)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| filename.to_string());
        map.insert("redeem_key".to_string(), serde_json::Value::String(stem));
        if let Some(ref s) = config.custom_redeem_script {
            map.insert("custom_redeem_script".to_string(), serde_json::Value::String(s.clone()));
        }
    }
    serde_json::to_string(&val).ok()
}

pub fn dispatch_test_redeem_json(json_str: &str) -> bool {
    let Ok(val) = serde_json::from_str::<serde_json::Value>(json_str) else {
        return false;
    };
    let state = get_state();
    let (cw, ch) = crate::obs_source::get_obs_canvas_resolution();
    let cw_f = cw.max(1) as f32;
    let ch_f = ch.max(1) as f32;

    let wildcards: Vec<String> = val.get("wildcards")
        .and_then(|w| w.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
        .unwrap_or_default();

    let rtype = val.get("type").and_then(|v| v.as_str()).unwrap_or("image");
    let target_raw = val.get("target").and_then(|v| v.as_str()).unwrap_or("");
    let target_resolved = crate::physics::resolve_wildcards(target_raw, &wildcards);
    let target = target_resolved.trim();
    let target_clean = target.trim_matches(|c| c == '{' || c == '}');

    let cur_target = state.studio_selected_source.read().clone();
    let target_source_name = if target.is_empty() || target == "random" {
        let sources = state.studio_sources.read();
        let tgts: Vec<String> = sources.iter().filter(|s| s.is_target).map(|s| s.name.clone()).collect();
        if !tgts.is_empty() {
            let idx = (std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as usize) % tgts.len();
            tgts[idx].clone()
        } else {
            state.hitboxes.read().keys().next().cloned().unwrap_or_else(|| cur_target.clone())
        }
    } else if let Ok(s_num) = target.parse::<usize>().or_else(|_| target_clean.parse::<usize>()) {
        state.target_slots.read().get(&s_num).cloned().unwrap_or_else(|| cur_target.clone())
    } else {
        target.to_string()
    };

    let (center_x, center_y) = if let (Some(tx), Some(ty)) = (
        val.get("target_x").or_else(|| val.get("targetX")).and_then(|v| v.as_f64()),
        val.get("target_y").or_else(|| val.get("targetY")).and_then(|v| v.as_f64()),
    ) {
        (tx as f32, ty as f32)
    } else {
        let hitboxes = state.hitboxes.read();
        let slots = state.target_slots.read();
        if let Some((_, hb)) = crate::resolve_target_hitbox(&target_source_name, &slots, &hitboxes) {
            if hb.active && hb.world_center_x > 0.0 && hb.world_center_y > 0.0 {
                (hb.world_center_x, hb.world_center_y)
            } else if hb.active && hb.world_width > 0.0 {
                (hb.world_x + hb.world_width * 0.5, hb.world_y + hb.world_height * 0.5)
            } else {
                (cw_f * 0.5, ch_f * 0.5)
            }
        } else {
            let sources = state.studio_sources.read();
            if let Some(src) = sources.iter().find(|s| s.name.eq_ignore_ascii_case(&target_source_name)) {
                (src.x + src.width * 0.5, src.y + src.height * 0.5)
            } else if let Ok(s_num) = target_source_name.parse::<u32>() {
                if let Some(src) = sources.iter().find(|s| s.slot == s_num) {
                    (src.x + src.width * 0.5, src.y + src.height * 0.5)
                } else {
                    (cw_f * 0.5, ch_f * 0.5)
                }
            } else {
                (cw_f * 0.5, ch_f * 0.5)
            }
        }
    };

    let start_snd = val.get("start_sound")
        .or_else(|| val.get("startSound"))
        .or_else(|| val.get("start_sound_path"))
        .or_else(|| val.get("startSoundPath"))
        .or_else(|| val.get("sound"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if rtype == "image" || rtype == "throwable" {
        if let Ok(mut pkg) = serde_json::from_value::<crate::physics::SpawnPackage>(val.clone()) {
            pkg.target = Some(target_source_name.clone());
            if pkg.wildcards.is_none() && !wildcards.is_empty() {
                pkg.wildcards = Some(wildcards.clone());
            }
            let id = state.physics.write().spawn(pkg, cw_f, ch_f);
            println!("[Trust] 🚀 Spawned throwable test ID: {} targeted at '{}'", id, target_source_name);
            return true;
        }
    } else {
        // Universal launch audio plays on every redeem type (water, fire, lightning)
        if !start_snd.is_empty() {
            crate::play_sound_str(start_snd);
        }

        if rtype == "webgl-water" {
            let json_map: std::collections::HashMap<String, serde_json::Value> = if let serde_json::Value::Object(map) = val {
                map.into_iter().collect()
            } else {
                std::collections::HashMap::new()
            };
            let cfg = crate::fluid::SphConfig::from_json_map(&json_map);
            let (sx, sy) = if let (Some(x_val), Some(y_val)) = (
                json_map.get("startX").or_else(|| json_map.get("start_x")).and_then(|v| v.as_f64()),
                json_map.get("startY").or_else(|| json_map.get("start_y")).and_then(|v| v.as_f64()),
            ) {
                (x_val as f32, y_val as f32)
            } else if cfg.stream_mode == "top" {
                (center_x, -20.0)
            } else {
                // Beam origin — where the stream shoots FROM, per the "Stream
                // Origin" picker. Same edge keys/positions as fire's flamethrower
                // and lightning's "Strike Origin" for consistency.
                let (cw_r, ch_r) = crate::obs_source::get_obs_canvas_resolution();
                let canvas_w = cw_r.max(1) as f32;
                let canvas_h = ch_r.max(1) as f32;
                let spawn_key = json_map.get("waterSpawn").or_else(|| json_map.get("water_spawn")).and_then(|v| v.as_str()).unwrap_or("tc");
                match spawn_key {
                    "tl" => (canvas_w * 0.05, -50.0),
                    "tr" => (canvas_w * 0.95, -50.0),
                    "ml" => (-50.0, canvas_h * 0.5),
                    "mr" => (canvas_w + 50.0, canvas_h * 0.5),
                    _ => (center_x, -50.0), // "tc" default: top-center
                }
            };
            state.effects.write().spawn_fluid(sx, sy, center_x, center_y, target_source_name.clone(), cfg);
            println!("[Trust] 🌊 Spawned water stream test targeting '{}'", target_source_name);
            return true;
        } else if rtype == "webgl-fire" {
            let json_map: std::collections::HashMap<String, serde_json::Value> = if let serde_json::Value::Object(map) = val {
                map.into_iter().collect()
            } else {
                std::collections::HashMap::new()
            };
            let cfg = crate::fire::FireConfig::from_json_map(&json_map);
            // The mask's center of mass (the shared center_x/center_y above), like every other effect
            let (fire_cx, fire_cy) = (center_x, center_y);
            let (sx, sy) = if cfg.stream_mode == "top" {
                (fire_cx, fire_cy + cfg.offset_y)
            } else {
                // Flamethrower origin — where the stream shoots FROM, per the
                // "Stream Origin" picker. Same edge keys/positions as lightning's
                // "Strike Origin" for consistency.
                let (cw_r, ch_r) = crate::obs_source::get_obs_canvas_resolution();
                let canvas_w = cw_r.max(1) as f32;
                let canvas_h = ch_r.max(1) as f32;
                let spawn_key = json_map.get("fireSpawn").and_then(|v| v.as_str()).unwrap_or("tc");
                match spawn_key {
                    "tl" => (canvas_w * 0.05, -50.0),
                    "tr" => (canvas_w * 0.95, -50.0),
                    "ml" => (-50.0, canvas_h * 0.5),
                    "mr" => (canvas_w + 50.0, canvas_h * 0.5),
                    _ => (fire_cx, -50.0), // "tc" default: top-center
                }
            };
            state.effects.write().spawn_fire(sx, sy, fire_cx, fire_cy, target_source_name.clone(), cfg);
            println!("[Trust] 🔥 Spawned fire burst test targeting '{}'", target_source_name);
            return true;
        } else if rtype == "webgl-lightning" {
            let json_map: std::collections::HashMap<String, serde_json::Value> = if let serde_json::Value::Object(map) = val {
                map.into_iter().collect()
            } else {
                std::collections::HashMap::new()
            };
            let cfg = crate::lightning::LightningConfig::from_json_map(&json_map);

            // Resolve spawn origin from spawnPoints[0] or lightSpawn field.
            // Keys: "tc"=top-center, "tl"=top-left, "tr"=top-right, "ml"=mid-left, "mr"=mid-right.
            let spawn_key = json_map.get("spawnPoints")
                .and_then(|v| v.as_array())
                .and_then(|a| a.first())
                .and_then(|v| v.as_str())
                .or_else(|| json_map.get("lightSpawn").and_then(|v| v.as_str()))
                .unwrap_or("tc");

            let (cw_r, ch_r) = crate::obs_source::get_obs_canvas_resolution();
            let canvas_w = cw_r.max(1) as f32;
            let canvas_h = ch_r.max(1) as f32;
            let (spawn_x, spawn_y) = match spawn_key {
                "tl" => (canvas_w * 0.05,  -50.0),
                "tr" => (canvas_w * 0.95,  -50.0),
                "ml" => (-50.0,            canvas_h * 0.5),
                "mr" => (canvas_w + 50.0,  canvas_h * 0.5),
                _    => (center_x,         -50.0),  // "tc" default: top-center
            };

            state.effects.write().spawn_lightning(spawn_x, spawn_y, center_x, center_y, target_source_name.clone(), cfg);
            println!("[Trust] ⚡ Spawned lightning strike test targeting '{}' from origin '{}'" , target_source_name, spawn_key);
            return true;
        } else if rtype == "reaction" {
            // Lightest-weight redeem type: no throwable, no particle engine —
            // just an avatar reaction (+ the start sound already played above).
            // This is what a "just a reaction and sound" custom redeem uses.
            let cfg = crate::physics::ImpactReactionConfig::parse(val.get("impactReactions"));
            crate::physics::fire_impact_reaction(&cfg, &target_source_name, center_x, center_y);
            println!("[Trust] 💥 Fired reaction-only test targeting '{}'", target_source_name);
            return true;
        } else if rtype == "custom" {
            let mut script = val.get("custom_redeem_script")
                .or_else(|| val.get("customRedeemScript"))
                .or_else(|| val.get("script"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if script.is_empty() || script == "(none selected)" {
                let snow_repo = crate::get_theater_repo_dir().join("snow_storm.lua");
                let candidates = [
                    snow_repo,
                    std::path::PathBuf::from("/home/mortygotvr/repo/theater-scene/snow_storm.lua"),
                    std::path::PathBuf::from("snow_storm.lua"),
                ];
                for c in &candidates {
                    if c.is_file() {
                        script = c.to_string_lossy().to_string();
                        break;
                    }
                }
            }

            let trigger_text = val.get("triggers")
                .and_then(|t| t.as_array())
                .and_then(|arr| arr.first())
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let ev = crate::types::StreamEvent {
                event_type: "custom_redeem".to_string(),
                trigger: trigger_text.to_string(),
                user: val.get("user").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                message: trigger_text.to_string(),
                raw_json: json_str.to_string(),
            };
            state.broadcast_event(ev);

            if !script.is_empty() && script != "(none selected)" {
                let mut params = serde_json::json!({
                    "target": target_source_name,
                    "trigger": trigger_text,
                    "user": val.get("user").and_then(|v| v.as_str()).unwrap_or(""),
                    "wildcards": wildcards,
                    "center_x": center_x,
                    "center_y": center_y,
                    "redeem_key": val.get("redeem_key").and_then(|v| v.as_str()).unwrap_or(""),
                    "name": val.get("name").and_then(|v| v.as_str()).unwrap_or(""),
                });
                add_dispatch_params(&mut params, &val);
                crate::custom_lua::run_custom_redeem_script(&script, params);
                println!("[Trust] 📜 Custom Lua redeem '{}' running script '{}' targeted at '{}' (trigger: '{}')", target_source_name, script, target_source_name, trigger_text);
            } else {
                println!("[Trust] ⚠️ Custom Lua redeem '{}' fired but has no script selected", target_source_name);
            }
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Taken by the tests that apply poses: which pose was applied last
    /// (LAST_APPLIED_POSE) and the active transition are global, so those tests
    /// run one at a time.
    static POSE_TESTS: parking_lot::Mutex<()> = parking_lot::Mutex::new(());

    /// A Theater Reader event (an alert), e.g. a channel point redeem.
    fn reader_event(trigger: &str) -> crate::types::StreamEvent {
        crate::types::StreamEvent {
            event_type: "alert".to_string(),
            trigger: trigger.to_string(),
            user: "Viewer123".to_string(),
            message: String::new(),
            raw_json: "{}".to_string(),
        }
    }

    /// A Theater Reader chat message saying `text`.
    fn chat_message(text: &str) -> crate::types::StreamEvent {
        crate::types::StreamEvent {
            event_type: "chat".to_string(),
            trigger: "Twitch chat".to_string(),
            user: "Viewer123".to_string(),
            message: text.to_string(),
            raw_json: "{}".to_string(),
        }
    }

    /// While no source is loaded, process_trigger_event loads the targets from
    /// disk, which replaces SCENE_COORDINATE_SETS: keep one loaded so a test's
    /// own pose stays.
    fn keep_a_source_loaded() {
        let mut sources = get_state().studio_sources.write();
        if sources.is_empty() {
            sources.push(crate::StudioSource {
                scene: String::new(), name: "Test Placeholder".to_string(), slot: 0,
                x: 0.0, y: 0.0, width: 100.0, height: 100.0, roll: 0.0, pitch: 0.0, yaw: 0.0,
                is_target: false, match_obs: false, is_visible: true, file_path: String::new(),
                default_x: 0.0, default_y: 0.0, default_width: 100.0, default_height: 100.0, default_roll: 0.0,
            });
        }
    }

    /// There are no chat triggers: nothing a viewer types fires a pose, not in a
    /// chat message, a whisper, or an event's message text.
    #[test]
    fn test_chat_never_fires_triggers() {
        let _serial = POSE_TESTS.lock();
        keep_a_source_loaded();
        let id = "set_no_chat_triggers";
        let trigger = "Twitch Mortygotvr redeem No Chat Pose";
        SCENE_COORDINATE_SETS.write().push(SceneCoordinateSet {
            id: id.to_string(),
            name: "No Chat Pose".to_string(),
            scene: String::new(),
            trigger: trigger.to_string(),
            duration_ms: 0,
            easing: MovementEasing::Instant,
            sources: vec![],
        });

        let whisper = crate::types::StreamEvent {
            event_type: "whisper".to_string(),
            trigger: "Twitch whisper".to_string(),
            ..chat_message(trigger)
        };
        let chat_as_alert = crate::types::StreamEvent { event_type: "alert".to_string(), ..chat_message(trigger) };
        let cheer_saying_it = crate::types::StreamEvent {
            message: trigger.to_string(),
            ..reader_event("Twitch Mortygotvr cheer 100")
        };
        for event in [chat_message(trigger), whisper, chat_as_alert, cheer_saying_it] {
            assert!(is_chat_event(&event) || event.trigger != trigger);
            process_trigger_event(&event);
            assert_ne!(last_applied_pose().1, id, "'{}' ({}) must not fire a pose", event.message, event.event_type);
        }

        process_trigger_event(&reader_event(trigger));
        assert_eq!(last_applied_pose().1, id, "the Reader event fires it");

        SCENE_COORDINATE_SETS.write().retain(|s| s.id != id);
    }

    #[test]
    fn test_poses_match_ignores_tiny_drift_but_sees_edits() {
        let snap = |name: &str, x: f32, volume: Option<f32>| SourceTransformSnapshot {
            source_name: name.to_string(),
            x,
            y: 20.0,
            width: 300.0,
            height: 200.0,
            roll: 0.0,
            pitch: 0.0,
            yaw: 0.0,
            is_visible: true,
            filters: None,
            volume,
        };
        let saved = vec![snap("Cam", 100.0, None), snap("Mic", 0.0, Some(0.5))];

        // Same pose, listed in another order and a hair off after a move
        assert!(poses_match(&saved, &[snap("Mic", 0.0, Some(0.5004)), snap("Cam", 100.2, None)]));
        // Moved, volume changed, hidden, or a source added
        assert!(!poses_match(&saved, &[snap("Cam", 140.0, None), snap("Mic", 0.0, Some(0.5))]));
        assert!(!poses_match(&saved, &[snap("Cam", 100.0, None), snap("Mic", 0.0, Some(0.8))]));
        let mut hidden = saved.clone();
        hidden[0].is_visible = false;
        assert!(!poses_match(&saved, &hidden));
        let mut more = saved.clone();
        more.push(snap("Overlay", 0.0, None));
        assert!(!poses_match(&saved, &more));
    }

    #[test]
    fn test_applying_a_pose_is_counted_with_its_id() {
        let _serial = POSE_TESTS.lock();
        let set = SceneCoordinateSet {
            id: "set_count_test".to_string(),
            name: "Count Test".to_string(),
            trigger: String::new(),
            scene: "Count Scene".to_string(),
            easing: MovementEasing::Instant,
            duration_ms: 100,
            sources: Vec::new(),
        };
        let seq = start_scene_transition(&set, &[]);
        let (last_seq, last_id) = last_applied_pose();
        assert!(last_seq >= seq && seq > 0);
        if last_seq == seq {
            assert_eq!(last_id, "set_count_test");
        }
        assert!(start_scene_transition(&set, &[]) > seq);
    }

    #[test]
    fn test_resolve_script_path_finds_scripts_from_other_machines() {
        let swirl = get_filters_dir().join("swirl.lua");
        if !swirl.is_file() {
            return;
        }
        for raw in [
            "/home/mortygotvr/repo/theater-scene/filters/swirl.lua",
            "C:\\Somewhere\\else\\swirl.lua",
            "filters/swirl.lua",
        ] {
            let resolved = resolve_script_path(raw);
            assert!(std::path::Path::new(&resolved).is_file(), "'{}' resolved to missing '{}'", raw, resolved);
            assert!(resolved.ends_with("swirl.lua"));
        }
        // A path that exists is kept; one that can't be found is left as is
        let existing = swirl.to_string_lossy().into_owned();
        assert_eq!(resolve_script_path(&existing), existing);
        assert_eq!(resolve_script_path("/nowhere/not_a_script_xyz.lua"), "/nowhere/not_a_script_xyz.lua");
    }

    #[test]
    fn test_script_sliders_read_param_lines() {
        let src = "\
-- @param swirl_intensity 8 0 20 Swirl strength
  --   @param tumble_speed 180 0 720 Tumble speed (°/s)
-- @param no_label 0.5 0 1
-- @param too_big 50 0 10 Clamped default
-- @param backwards 1 5 0 Min above max
-- @param missing 1 0
-- @parameters 1 0 5 Not a param line
-- @param bad x 0 5 Not a number
local x = 1 -- @param after_code 1 0 5 Not a comment line
";
        let sliders = script_sliders(src);
        let names: Vec<&str> = sliders.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["swirl_intensity", "tumble_speed", "no_label", "too_big"]);
        assert_eq!(
            sliders[0],
            ScriptSlider { name: "swirl_intensity".into(), default: 8.0, min: 0.0, max: 20.0, label: "Swirl strength".into() }
        );
        assert_eq!(sliders[1].label, "Tumble speed (°/s)");
        assert_eq!(sliders[2].label, "no_label");
        assert_eq!(sliders[3].default, 10.0);
    }

    #[test]
    fn test_doom_script_declares_its_sliders() {
        let doom = crate::get_theater_repo_dir().join("doom.lua");
        let Ok(src) = std::fs::read_to_string(&doom) else {
            return;
        };
        let names: Vec<String> = script_sliders(&src).into_iter().map(|s| s.name).collect();
        for name in ["swirl_intensity", "swirl_radius", "swirl_speed", "tumble_speed"] {
            assert!(names.iter().any(|n| n == name), "doom.lua has no '{}' slider", name);
        }
    }

    #[test]
    fn test_script_slider_values_reach_the_script_params() {
        let mut config = RedeemConfig::default();
        config.redeem_type = "custom".into();
        config.custom_redeem_script = Some("doom.lua".into());
        config.script_params = Some([("swirl_intensity".to_string(), 12.5), ("target".to_string(), 3.0)].into());

        let json = build_redeem_dispatch_json("doom_redeem.json", &config).unwrap();
        let dispatch: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(dispatch["scriptParams"]["swirl_intensity"], 12.5);

        let mut params = serde_json::json!({ "target": "Avatar" });
        add_dispatch_params(&mut params, &dispatch);
        assert_eq!(params["swirl_intensity"], 12.5);
        assert_eq!(params["redeem_key"], "doom_redeem");
        // A slider can't override what the dispatch sets itself
        assert_eq!(params["target"], "Avatar");

        // Redeems saved before sliders existed load, and save, without them
        let old: RedeemConfig = serde_json::from_str(r#"{"name":"Doom","type":"custom"}"#).unwrap();
        assert_eq!(old.script_params, None);
        assert!(!serde_json::to_string(&old).unwrap().contains("scriptParams"));
    }

    #[test]
    fn test_removed_lua_effects_leave_the_list_not_the_disk() {
        let all = lua_effects_in_filters(&Default::default());
        let Some(first) = all.first() else { return };
        let removed: std::collections::BTreeSet<String> = [effect_file_name(&first.path)].into();
        let listed = lua_effects_in_filters(&removed);
        assert_eq!(listed.len(), all.len() - 1);
        assert!(!listed.iter().any(|e| e.path == first.path));
        assert!(first.path.is_file(), "the file stays");
    }

    #[test]
    fn test_save_redeem_creates_missing_folder_and_reads_back() {
        let root = std::env::temp_dir().join(format!("trust_redeem_save_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let mut item = RedeemItem {
            filename: "water_test.json".into(),
            path: root.join("redeems").join("water_test.json"),
            config: RedeemConfig { name: "Water Test".into(), redeem_type: "webgl-water".into(), ..RedeemConfig::default() },
            is_dirty: true,
            triggers_input: "Twitch * redeem Water, Kick * redeem Splash".into(),
        };
        save_redeem_to_disk(&mut item).expect("saving must create the missing redeems folder");
        let back: RedeemConfig = serde_json::from_str(&std::fs::read_to_string(&item.path).unwrap()).unwrap();
        assert_eq!(back.name, "Water Test");
        assert_eq!(back.redeem_type, "webgl-water");
        assert_eq!(back.triggers, Some(vec!["Twitch * redeem Water".to_string(), "Kick * redeem Splash".to_string()]));
        assert!(root.join("redeems").join("water_test.rdm").is_file(), "companion .rdm written too");
        assert!(!item.is_dirty);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn test_redeems_folder_is_in_the_repo_on_windows() {
        let dir = get_redeems_dir();
        assert!(dir.is_absolute(), "must not depend on OBS's working directory: {dir:?}");
        assert_eq!(dir, crate::get_theater_repo_dir().join("redeems"));
    }

    #[test]
    fn test_pose_with_null_numbers_still_loads() {
        // What the Studio wrote for audio sources whose size had run off to
        // infinity: serde_json saves non-finite numbers as null
        let json = r#"{
            "id": "set_1", "name": "Pose 1", "trigger": "", "scene": "Streaming",
            "easing": "EaseOut", "duration_ms": 800, "type": "scene_coordinate_set",
            "sources": [
                {"source_name": "Micro", "x": 0.0, "y": 0.0, "width": 100.0, "height": 100.0, "roll": 0.0, "pitch": 0.0, "yaw": 0.0, "is_visible": true, "volume": 0.6},
                {"source_name": "Stream", "x": 0.0, "y": 0.0, "width": null, "height": null, "roll": 0.0, "pitch": 0.0, "yaw": 0.0, "is_visible": true, "volume": 0.37}
            ]
        }"#;
        let set: SceneCoordinateSet = serde_json::from_str(json).expect("one bad source must not drop the whole pose");
        assert_eq!(set.sources.len(), 2);
        assert_eq!((set.sources[1].width, set.sources[1].height), (0.0, 0.0));
        assert_eq!(set.sources[1].volume, Some(0.37));
    }

    #[test]
    fn test_snapshot_of_runaway_size_saves_and_loads_back() {
        let source = |name: &str, width: f32, height: f32| crate::StudioSource {
            scene: "Streaming".to_string(),
            name: name.to_string(),
            slot: 1,
            x: 5.0, y: 6.0, width, height,
            roll: 0.0, pitch: 0.0, yaw: 0.0,
            is_target: false, match_obs: true, is_visible: true,
            file_path: String::new(),
            default_x: 0.0, default_y: 0.0, default_width: 100.0, default_height: 100.0, default_roll: 0.0,
        };
        let snapshots = snapshot_all_sources(&[source("Stream", f32::INFINITY, f32::NAN), source("Tv", 640.0, 360.0)]);
        assert_eq!((snapshots[0].width, snapshots[0].height), (0.0, 0.0), "non-finite sizes are never saved");
        assert_eq!((snapshots[1].width, snapshots[1].height), (640.0, 360.0));

        let set = SceneCoordinateSet {
            id: "set_2".into(), name: "Pose 2".into(), trigger: String::new(), scene: "Streaming".into(),
            easing: MovementEasing::EaseOut, duration_ms: 800, sources: snapshots,
        };
        let json = serde_json::to_string(&set).unwrap();
        assert!(!json.contains("null"), "saved pose must not contain null numbers: {json}");
        let back: SceneCoordinateSet = serde_json::from_str(&json).unwrap();
        assert_eq!(back, set);
    }

    #[test]
    fn test_volume_slider_does_not_rewrite_the_live_volume() {
        // A level read back from OBS's mixer, off any 0.1 dB grid
        let off_grid = volume_to_db(0.4321);
        let ctx = egui::Context::default();

        let mut db = off_grid;
        let mut changed = true;
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                changed = ui.add(volume_slider(&mut db, 0.0)).changed();
            });
        });
        assert!(!changed, "an untouched volume slider must not report a change");
        assert_eq!(db, off_grid);

        // The previous configuration (default clamping + step_by) did — on every frame
        let mut db_old = off_grid;
        let mut changed_old = false;
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                changed_old = ui.add(egui::Slider::new(&mut db_old, VOLUME_SLIDER_MIN_DB..=0.0).step_by(0.1)).changed();
            });
        });
        assert!(changed_old, "sanity: a step_by slider re-snaps and reports untouched values");
    }

    #[test]
    fn test_level_meter_maps_dbfs_across_the_bar() {
        assert_eq!(meter_fraction(f32::NEG_INFINITY), 0.0, "silence");
        assert_eq!(meter_fraction(f32::NAN), 0.0);
        assert_eq!(meter_fraction(-80.0), 0.0);
        assert_eq!(meter_fraction(-60.0), 0.0);
        assert!((meter_fraction(-30.0) - 0.5).abs() < 1e-6);
        assert_eq!(meter_fraction(0.0), 1.0);
        assert_eq!(meter_fraction(3.0), 1.0, "clipping pins to the end");
    }

    #[test]
    fn test_studio_volume_slider_db_conversion() {
        assert_eq!(volume_to_db(1.0), 0.0);
        assert!((volume_to_db(0.5) - (-6.0206)).abs() < 0.001);
        assert_eq!(volume_to_db(0.0), VOLUME_SLIDER_MIN_DB);
        assert_eq!(volume_to_db(0.00001), VOLUME_SLIDER_MIN_DB);
        assert!(volume_to_db(2.0) > 6.0, "boosted volumes keep their level");
        assert_eq!(db_to_volume(0.0), 1.0);
        assert_eq!(db_to_volume(VOLUME_SLIDER_MIN_DB), 0.0);
        for v in [0.01f32, 0.25, 0.5, 1.0, 1.5] {
            assert!((db_to_volume(volume_to_db(v)) - v).abs() < 1e-4, "round trip {v}");
        }
    }

    #[test]
    fn test_pose_volume_eases_from_current_to_saved() {
        assert_eq!(eased_volume(Some(1.0), Some(0.2), 0.0), Some(1.0));
        assert!((eased_volume(Some(1.0), Some(0.2), 0.5).unwrap() - 0.6).abs() < 1e-6);
        assert_eq!(eased_volume(Some(1.0), Some(0.2), 1.0), Some(0.2));
        // Easings that leave 0..1 never push the volume negative or past the saved value
        assert_eq!(eased_volume(Some(0.1), Some(1.0), -0.5), Some(0.0));
        assert_eq!(eased_volume(Some(0.5), Some(0.0), 1.3), Some(0.0));
        // Pose without a saved volume (video-only source, or saved before volume existed): leave it alone
        assert_eq!(eased_volume(Some(1.0), None, 1.0), None);
        // Current volume unreadable: apply the saved one only once the transition ends
        assert_eq!(eased_volume(None, Some(0.3), 0.5), None);
        assert_eq!(eased_volume(None, Some(0.3), 1.0), Some(0.3));
    }

    fn settings(v: serde_json::Value) -> serde_json::Map<String, serde_json::Value> {
        v.as_object().unwrap().clone()
    }

    #[test]
    fn test_pose_move_writes_only_what_the_pose_changes() {
        let start = settings(serde_json::json!({ "fov": 90.0, "tilt": 10.0, "smoothing": 0.5, "enable_rotation": true }));
        let target = settings(serde_json::json!({ "fov": 50.0, "tilt": 10.0, "smoothing": 0.5, "enable_rotation": false }));
        let mid = filter_settings_step("theatre_360_master_filter", Some(&start), &target, 0.5);
        // Unchanged settings are left to OBS (so a change made there meanwhile survives)
        assert_eq!(mid.len(), 2);
        assert_eq!(mid["fov"], serde_json::json!(70.0));
        assert_eq!(mid["enable_rotation"], serde_json::json!(false));
        let end = filter_settings_step("theatre_360_master_filter", Some(&start), &target, 1.0);
        assert_eq!(end["fov"], serde_json::json!(50.0));
        assert!(!end.contains_key("tilt"));
        // Nothing to write at all when the pose changes nothing
        assert!(filter_settings_step("theatre_360_master_filter", Some(&target), &target, 0.3).is_empty());
    }

    #[test]
    fn test_pose_move_never_writes_360_live_values() {
        // Poses saved before the runtime keys were left out still hold them
        let start = settings(serde_json::json!({ "pan": 0.0, "_live_pan": 12.0, "_has_live": true }));
        let target = settings(serde_json::json!({ "pan": 40.0, "_live_pan": 33.0, "_has_live": false }));
        for progress in [0.0, 0.5, 1.0] {
            let writes = filter_settings_step("theatre_360_master_filter", Some(&start), &target, progress);
            assert!(writes.keys().all(|k| !k.starts_with('_')), "{:?}", writes);
        }
        let stored = without_runtime_keys(serde_json::json!({ "pan": 5.0, "_live_pan": 7.0, "_live_tilt": 1.0, "_live_fov": 90.0, "_has_live": true }));
        assert_eq!(stored, serde_json::json!({ "pan": 5.0 }));
    }

    #[test]
    fn test_pose_move_turns_360_pan_the_short_way() {
        let start = settings(serde_json::json!({ "pan": 170.0, "x_offset": -170.0, "scale_x": 170.0 }));
        let target = settings(serde_json::json!({ "pan": -170.0, "x_offset": 170.0, "scale_x": -170.0 }));
        let mid = filter_settings_step("theatre_360_layer_filter", Some(&start), &target, 0.5);
        // 170° → -170° passes through 180°, not 0°
        assert!((mid["pan"].as_f64().unwrap().abs() - 180.0).abs() < 1e-9);
        assert!((mid["x_offset"].as_f64().unwrap().abs() - 180.0).abs() < 1e-9);
        let quarter = filter_settings_step("theatre_360_layer_filter", Some(&start), &target, 0.25);
        assert!((quarter["pan"].as_f64().unwrap() - 175.0).abs() < 1e-9);
        assert!((quarter["x_offset"].as_f64().unwrap() + 175.0).abs() < 1e-9);
        // Not an angle: straight line
        assert_eq!(mid["scale_x"], serde_json::json!(0.0));
        // Other filters' "pan" isn't a 360 direction
        let other = filter_settings_step("some_filter", Some(&start), &target, 0.5);
        assert_eq!(other["pan"], serde_json::json!(0.0));
        let end = filter_settings_step("theatre_360_layer_filter", Some(&start), &target, 1.0);
        assert_eq!(end["pan"], serde_json::json!(-170.0));
    }

    /// The Media Master as it runs in the "intro" pose: rotating, its pan
    /// setting 35 but the view turned to 147.5 by the rotation.
    fn intro_master_live() -> serde_json::Value {
        serde_json::json!({
            "enable_rotation": true, "pan": 35.0, "tilt": 3.0, "fov": 60.0, "rot_pan": -108.0,
            "_has_live": true, "_live_pan": 147.5, "_live_tilt": 3.0, "_live_fov": 60.0,
        })
    }

    #[test]
    fn test_pose_move_stopping_the_360_rotation_turns_from_the_live_view() {
        // intro -> Game: the rotation stops; Game's pan setting is also 35
        let mut start = intro_master_live();
        live_360_start(&mut start, Some(false));
        assert_eq!(start["pan"], serde_json::json!(147.5));
        let start = without_runtime_keys(start);
        let game = settings(serde_json::json!({ "enable_rotation": false, "pan": 35.0, "tilt": -10.0, "fov": 60.0, "rot_pan": -108.0 }));
        let mid = filter_settings_step(THEATRE_360_MASTER_FILTER, start.as_object(), &game, 0.5);
        // The pan is moved (it wasn't: it matched, and the view snapped to 35),
        // from the live view the short way round: 147.5 -> 91.25 -> 35
        assert!((mid["pan"].as_f64().unwrap() - 91.25).abs() < 1e-9, "{mid:?}");
        assert_eq!(mid["enable_rotation"], serde_json::json!(false));
        let end = filter_settings_step(THEATRE_360_MASTER_FILTER, start.as_object(), &game, 1.0);
        assert_eq!(end["pan"], serde_json::json!(35.0));
    }

    #[test]
    fn test_pose_move_keeping_the_360_rotation_leaves_its_angle_alone() {
        // intro -> intro-like pose that keeps rotating: the live pan holds the
        // rotation's angle, which stays on top of the pan setting
        let mut start = intro_master_live();
        live_360_start(&mut start, Some(true));
        assert_eq!(start["pan"], serde_json::json!(35.0));
        // Game -> intro: not rotating yet, so the live pan is the view
        let mut game_live = serde_json::json!({
            "enable_rotation": false, "pan": 35.0, "tilt": -10.0, "fov": 60.0,
            "_has_live": true, "_live_pan": 34.9, "_live_tilt": -9.5, "_live_fov": 60.0,
        });
        live_360_start(&mut game_live, Some(true));
        assert_eq!(game_live["pan"], serde_json::json!(34.9));
        assert_eq!(game_live["tilt"], serde_json::json!(-9.5));
        // Before the Master has run once there's no live view: settings as they are
        let mut fresh = serde_json::json!({ "enable_rotation": true, "pan": 35.0 });
        live_360_start(&mut fresh, Some(false));
        assert_eq!(fresh["pan"], serde_json::json!(35.0));
    }

    #[test]
    fn test_pose_move_keeps_whole_number_settings_whole() {
        let start = settings(serde_json::json!({ "left": 0, "gamma": 0.0 }));
        let target = settings(serde_json::json!({ "left": 25, "gamma": 1.0 }));
        let mid = filter_settings_step("crop_filter", Some(&start), &target, 0.5);
        assert!(mid["left"].is_i64(), "{:?}", mid);
        assert_eq!(mid["left"], serde_json::json!(13));
        assert_eq!(mid["gamma"], serde_json::json!(0.5));
    }

    #[test]
    fn test_pose_move_sets_a_new_filter_only_at_the_end() {
        let target = settings(serde_json::json!({ "left": 25, "relative": true }));
        assert!(filter_settings_step("crop_filter", None, &target, 0.5).is_empty());
        assert_eq!(filter_settings_step("crop_filter", None, &target, 1.0), target);
    }

    #[test]
    fn test_layer_remote_settings_drive_the_master() {
        assert_eq!(layer_remote_master_key("theatre_360_layer_filter", "m_pan"), Some("pan"));
        assert_eq!(layer_remote_master_key("theatre_360_layer_filter", "enable_rotation"), Some("enable_rotation"));
        assert_eq!(layer_remote_master_key("theatre_360_layer_filter", "x_offset"), None);
        assert_eq!(layer_remote_master_key("theatre_360_master_filter", "enable_rotation"), None);
    }

    #[test]
    fn test_showing_a_filter_setting_never_changes_it() {
        // Values off the old sliders' step grid or outside their range
        let values = [
            ("pan", serde_json::json!(12.5)),
            ("scale_x", serde_json::json!(0.35)),
            ("fov", serde_json::json!(175.0)),
            ("color_multiply", serde_json::json!(4294967295u64)),
            ("left", serde_json::json!(7)),
            ("enable_rotation", serde_json::json!(true)),
            ("master_source", serde_json::json!("360 Cam")),
        ];
        let ctx = egui::Context::default();
        for frame in 0..3 {
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    for (key, value) in &values {
                        assert_eq!(filter_setting_widget(ui, key, value), None, "frame {frame}: {key}");
                    }
                });
            });
        }
    }

    #[test]
    fn test_showing_filter_properties_never_changes_them() {
        use crate::filter_props::{Kind, ListFormat, ListItem, Prop};
        let prop = |name: &str, kind: Kind| Prop { name: name.to_string(), label: name.to_string(), enabled: true, kind };
        let props = vec![
            // The 360 Layer's master dropdown, currently on a source the list doesn't have yet
            prop("master_source", Kind::List {
                format: ListFormat::String,
                items: vec![
                    ListItem { label: "(None)".into(), value: serde_json::json!(""), disabled: false },
                    ListItem { label: "360 Cam".into(), value: serde_json::json!("360 Cam"), disabled: false },
                ],
            }),
            prop("sampling", Kind::List {
                format: ListFormat::Int,
                items: vec![ListItem { label: "Bicubic".into(), value: serde_json::json!(1), disabled: false }],
            }),
            // Off the slider's step and outside its range
            prop("pan", Kind::Float { min: -180.0, max: 180.0, step: 1.0, slider: true }),
            prop("fov", Kind::Float { min: 10.0, max: 170.0, step: 0.1, slider: false }),
            prop("left", Kind::Int { min: 0, max: 100, step: 5, slider: true }),
            prop("color", Kind::Color { alpha: false }),
            prop("color_alpha", Kind::Color { alpha: true }),
            prop("enable_rotation", Kind::Bool),
            prop("group", Kind::Group { checkable: true, children: vec![prop("path", Kind::Path)] }),
            prop("font", Kind::Other),
        ];
        let settings = serde_json::json!({
            "master_source": "Old Cam",
            "sampling": 1.0,
            "pan": 12.5,
            "fov": 175.0,
            "left": 7,
            "color": 4278190335u64,
            "color_alpha": 2164195328u64,
            "enable_rotation": true,
            "group": false,
            "path": "C:/x.png",
        });
        let ctx = egui::Context::default();
        for frame in 0..3 {
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    assert_eq!(filter_props_ui(ui, "Theatre 360 (Layer)", &props, &settings), None, "frame {frame}");
                });
            });
        }
        assert!(list_value_is(Some(&serde_json::json!(1.0)), &serde_json::json!(1)));
        assert!(list_value_is(Some(&serde_json::json!("360 Cam")), &serde_json::json!("360 Cam")));
        assert!(!list_value_is(None, &serde_json::json!("")));
    }

    #[test]
    fn test_pose_snapshot_volume_round_trips_and_old_poses_still_load() {
        // Poses saved before volume existed (e.g. the existing theater_targets.json) load with no volume
        let old: SourceTransformSnapshot = serde_json::from_str(
            r#"{"source_name":"Micro","x":0,"y":0,"width":100,"height":100,"roll":0,"pitch":0,"yaw":0,"is_visible":true}"#,
        ).unwrap();
        assert_eq!(old.volume, None);
        let json = serde_json::to_string(&old).unwrap();
        assert!(!json.contains("volume"), "video-only / legacy snapshots shouldn't write a volume field");

        let with_volume = SourceTransformSnapshot { volume: Some(0.35), ..old };
        let back: SourceTransformSnapshot = serde_json::from_str(&serde_json::to_string(&with_volume).unwrap()).unwrap();
        assert_eq!(back.volume, Some(0.35));
    }

    static TEST_STUDIO_MUTEX: parking_lot::Mutex<()> = parking_lot::Mutex::new(());

    #[test]
    fn test_app_icon_loads_valid_rgba_dimensions() {
        let icon_opt = get_app_icon_data();
        assert!(icon_opt.is_some(), "App icon must load from embedded bytes");
        let icon = icon_opt.unwrap();
        assert_eq!(icon.width, 256);
        assert_eq!(icon.height, 256);
        assert_eq!(icon.rgba.len(), (256 * 256 * 4) as usize);
    }

    #[test]
    fn test_studio_window_lifecycle_flags() {
        assert!(!is_studio_native_open());

        // Simulate thread active and hidden
        IS_STUDIO_THREAD_RUNNING.store(true, std::sync::atomic::Ordering::SeqCst);
        IS_STUDIO_WINDOW_VISIBLE.store(false, std::sync::atomic::Ordering::SeqCst);
        assert!(!is_studio_native_open());

        // Call launch_studio_native while thread is running: should request show and set visible
        launch_studio_native();
        assert!(is_studio_native_open());
        assert!(STUDIO_SHOW_REQUEST.load(std::sync::atomic::Ordering::SeqCst));

        // Reset
        IS_STUDIO_THREAD_RUNNING.store(false, std::sync::atomic::Ordering::SeqCst);
        IS_STUDIO_WINDOW_VISIBLE.store(false, std::sync::atomic::Ordering::SeqCst);
        STUDIO_SHOW_REQUEST.store(false, std::sync::atomic::Ordering::SeqCst);
    }

    #[test]
    fn test_load_studio_targets_from_disk_on_startup() {
        let _lock = TEST_STUDIO_MUTEX.lock();
        let state = get_state();
        *state.studio_scene_name.write() = "Scène".to_string();
        let count = load_studio_targets_from_disk();
        println!("Loaded {} targets from disk in test", count);
        let sources = state.studio_sources.read();
        assert_eq!(sources.len(), count);
        if count > 0 {
            // Verify that at least one target has roll or yaw or dimensions loaded
            assert!(sources.iter().any(|s| s.width > 0.0 && s.height > 0.0));
            // Verify sorted by slot ascending (good order)
            let slots: Vec<u32> = sources.iter().filter(|s| s.slot > 0).map(|s| s.slot).collect();
            let mut sorted_slots = slots.clone();
            sorted_slots.sort();
            assert_eq!(slots, sorted_slots, "Sources must be sorted by slot in good order");
        }
    }

    #[test]
    fn test_sources_sorted_in_good_order() {
        let _lock = TEST_STUDIO_MUTEX.lock();
        let state = get_state();
        let test_sources = vec![
            crate::StudioSource {
                scene: "Scène".to_string(),
                name: "Image".to_string(),
                slot: 3,
                x: 0.0, y: 0.0, width: 100.0, height: 100.0,
                roll: 0.0, pitch: 0.0, yaw: 0.0,
                is_target: false, match_obs: true, is_visible: true,
                file_path: String::new(),
                default_x: 0.0, default_y: 0.0, default_width: 100.0, default_height: 100.0, default_roll: 0.0,
            },
            crate::StudioSource {
                scene: "Scène".to_string(),
                name: "Image 3".to_string(),
                slot: 1,
                x: 0.0, y: 0.0, width: 100.0, height: 100.0,
                roll: 0.0, pitch: 0.0, yaw: 0.0,
                is_target: true, match_obs: true, is_visible: true,
                file_path: String::new(),
                default_x: 0.0, default_y: 0.0, default_width: 100.0, default_height: 100.0, default_roll: 0.0,
            },
            crate::StudioSource {
                scene: "Scène".to_string(),
                name: "Image 2".to_string(),
                slot: 2,
                x: 0.0, y: 0.0, width: 100.0, height: 100.0,
                roll: 0.0, pitch: 0.0, yaw: 0.0,
                is_target: false, match_obs: true, is_visible: true,
                file_path: String::new(),
                default_x: 0.0, default_y: 0.0, default_width: 100.0, default_height: 100.0, default_roll: 0.0,
            },
            crate::StudioSource {
                scene: "Scène".to_string(),
                name: "Unassigned Item".to_string(),
                slot: 0,
                x: 0.0, y: 0.0, width: 100.0, height: 100.0,
                roll: 0.0, pitch: 0.0, yaw: 0.0,
                is_target: false, match_obs: true, is_visible: true,
                file_path: String::new(),
                default_x: 0.0, default_y: 0.0, default_width: 100.0, default_height: 100.0, default_roll: 0.0,
            },
        ];

        *state.studio_sources.write() = test_sources;
        crate::obs_source::sort_studio_sources_in_good_order();

        let sources = state.studio_sources.read();
        assert_eq!(sources[0].name, "Image 3", "Slot 1 must be first (top of dock)");
        assert_eq!(sources[1].name, "Image 2", "Slot 2 must be second");
        assert_eq!(sources[2].name, "Image", "Slot 3 must be third");
        assert_eq!(sources[3].name, "Unassigned Item", "Slot 0 must be at the end");
    }

    #[test]
    fn test_obs_scene_reversal_matches_good_order() {
        // obs_scene_enum_items visits bottom-to-top (creation order)
        let mut obs_enum_order = vec!["Image", "Image 2", "Image 3"];
        // Reversing produces the canonical top-to-bottom OBS Sources dock order
        obs_enum_order.reverse();
        assert_eq!(obs_enum_order, vec!["Image 3", "Image 2", "Image"]);
    }

    #[test]
    fn test_disk_targets_loaded_reliably() {
        let _lock = TEST_STUDIO_MUTEX.lock();
        let count = load_studio_targets_from_disk();
        assert!(count > 0, "Targets must always be loaded reliably from disk");
    }

    #[test]
    fn test_save_preserves_other_scene_targets() {
        let _lock = TEST_STUDIO_MUTEX.lock();
        let state = get_state();
        *state.studio_scene_name.write() = "Scène".to_string();
        let original_targets = read_disk_targets();
        assert!(!original_targets.is_empty());

        let scene1_count = original_targets.iter().filter(|s| s.scene == "Scène").count();
        assert!(scene1_count > 0);

        // Simulate saving a new target in "Scène 2"
        let scene2_sources = vec![
            crate::StudioSource {
                scene: "Scène 2".to_string(),
                name: "Image in Scene 2".to_string(),
                slot: 1,
                x: 50.0, y: 50.0, width: 200.0, height: 200.0,
                roll: 0.0, pitch: 0.0, yaw: 0.0,
                is_target: true, match_obs: false, is_visible: true,
                file_path: String::new(),
                default_x: 0.0, default_y: 0.0, default_width: 200.0, default_height: 200.0, default_roll: 0.0,
            }
        ];

        let path_buf = crate::get_theater_repo_dir().join("theater_targets.json");
        let path = path_buf.to_str().unwrap_or("theater_targets.json");
        let existing_data: Vec<serde_json::Value> = std::fs::File::open(path)
            .ok()
            .and_then(|f| {
                let val: serde_json::Value = serde_json::from_reader(f).ok()?;
                if let Some(arr) = val.get("sources").and_then(|s| s.as_array()) {
                    Some(arr.clone())
                } else if let Some(arr) = val.as_array() {
                    Some(arr.clone())
                } else {
                    None
                }
            })
            .unwrap_or_default();

        let current_scene = "Scène 2".to_string();
        let mut list: Vec<serde_json::Value> = existing_data.iter().filter(|item| {
            let is_special = item.get("type").is_some();
            if is_special { return false; }
            let item_scene = item.get("scene").and_then(|v| v.as_str()).unwrap_or("Scène");
            item_scene != current_scene
        }).cloned().collect();

        for src in &scene2_sources {
            list.push(serde_json::json!({
                "scene": src.scene,
                "name": src.name,
                "source_name": src.name,
                "slot": src.slot,
                "target": src.is_target,
                "match_obs": src.match_obs,
                "visible": src.is_visible,
            }));
        }

        let sc1_in_list = list.iter().filter(|item| {
            let is_special = item.get("type").is_some();
            if is_special { return false; }
            let is_sc1 = item.get("scene").and_then(|v| v.as_str()).unwrap_or("Scène") == "Scène";
            is_sc1
        }).count();
        let sc2_in_list = list.iter().filter(|item| item.get("scene").and_then(|v| v.as_str()) == Some("Scène 2")).count();
        assert_eq!(sc1_in_list, scene1_count);
        assert_eq!(sc2_in_list, 1);

        // Reset state
        *state.studio_scene_name.write() = "Scène".to_string();
        load_studio_targets_from_disk();
    }

    #[test]
    fn test_reset_source_to_obs_restores_default_xform_and_clears_3d_rotations() {
        let mut src = crate::StudioSource {
            scene: "Scène".to_string(),
            name: "Image".to_string(),
            slot: 1,
            x: 1282.85,
            y: 70.45,
            width: 513.7,
            height: 513.7,
            roll: 11.93,
            pitch: 15.0,
            yaw: 25.0,
            is_target: true,
            match_obs: true,
            is_visible: true,
            file_path: String::new(),
            default_x: 1227.0,
            default_y: 323.0,
            default_width: 207.0,
            default_height: 207.0,
            default_roll: 0.0,
        };

        crate::obs_source::reset_studio_source_to_obs("Scène", &mut src);

        assert_eq!(src.x, 1227.0);
        assert_eq!(src.y, 323.0);
        assert_eq!(src.width, 207.0);
        assert_eq!(src.height, 207.0);
        assert_eq!(src.roll, 0.0);
        assert_eq!(src.pitch, 0.0);
        assert_eq!(src.yaw, 0.0);
    }

    #[test]
    fn test_sources_table_scroll_limit_and_visibility() {
        let row_height = 25.0;
        let header_height = 24.0;
        let max_sources_height = header_height + (8.0 * row_height);
        assert_eq!(max_sources_height, 224.0);

        // Ensure egui context can execute a frame containing the scroll area with 8-source height limit and always-visible scrollbar
        let ctx = egui::Context::default();
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("sources_scroll_area")
                    .max_height(max_sources_height)
                    .auto_shrink([false, false])
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
                    .show(ui, |ui| {
                        for i in 0..20 {
                            ui.label(format!("Source {}", i));
                        }
                    });
            });
        });
    }

    #[test]
    fn test_movement_easing_evaluation() {
        assert_eq!(MovementEasing::Instant.evaluate(0.0), 1.0);
        assert_eq!(MovementEasing::Instant.evaluate(0.5), 1.0);
        assert_eq!(MovementEasing::Linear.evaluate(0.0), 0.0);
        assert_eq!(MovementEasing::Linear.evaluate(0.5), 0.5);
        assert_eq!(MovementEasing::Linear.evaluate(1.0), 1.0);
        assert_eq!(MovementEasing::EaseIn.evaluate(0.0), 0.0);
        assert_eq!(MovementEasing::EaseIn.evaluate(1.0), 1.0);
        assert_eq!(MovementEasing::EaseOut.evaluate(0.0), 0.0);
        assert_eq!(MovementEasing::EaseOut.evaluate(1.0), 1.0);
        assert_eq!(MovementEasing::EaseInOut.evaluate(0.0), 0.0);
        assert_eq!(MovementEasing::EaseInOut.evaluate(0.5), 0.5);
        assert_eq!(MovementEasing::EaseInOut.evaluate(1.0), 1.0);
        assert_eq!(MovementEasing::Bounce.evaluate(0.0), 0.0);
        assert!((MovementEasing::Bounce.evaluate(1.0) - 1.0).abs() < 0.001);

        // English names check
        assert_eq!(MovementEasing::EaseOut.name(), "Ease Out");
        assert_eq!(MovementEasing::EaseIn.name(), "Ease In");
        assert_eq!(MovementEasing::EaseInOut.name(), "Ease In-Out");
        assert_eq!(MovementEasing::Linear.name(), "Linear");
        assert_eq!(MovementEasing::Bounce.name(), "Bounce");
        assert_eq!(MovementEasing::Instant.name(), "Instant");
    }

    #[test]
    fn test_scene_coordinate_set_serde() {
        let set = SceneCoordinateSet {
            id: "test_set_1".to_string(),
            name: "Gaming Pose".to_string(),
            trigger: "Twitch * redeem Gaming Pose".to_string(),
            scene: "Scène".to_string(),
            easing: MovementEasing::EaseInOut,
            duration_ms: 1200,
            sources: vec![
                SourceTransformSnapshot {
                    source_name: "Camera".to_string(),
                    x: 100.0,
                    y: 200.0,
                    width: 400.0,
                    height: 300.0,
                    roll: 10.0,
                    pitch: 5.0,
                    yaw: -5.0,
                    is_visible: true,
                    filters: None,
                    volume: None,
                }
            ],
        };

        let json_val = serde_json::to_value(&set).unwrap();
        assert_eq!(json_val["name"], "Gaming Pose");
        assert_eq!(json_val["trigger"], "Twitch * redeem Gaming Pose");
        assert_eq!(json_val["easing"], "EaseInOut");
        assert_eq!(json_val["duration_ms"], 1200);

        let deserialized: SceneCoordinateSet = serde_json::from_value(json_val).unwrap();
        assert_eq!(deserialized, set);
    }

    #[test]
    fn test_snapshot_all_sources() {
        let sources = vec![
            crate::StudioSource {
                scene: "Scène".to_string(),
                name: "Src1".to_string(),
                slot: 1,
                x: 10.0, y: 20.0, width: 100.0, height: 100.0,
                roll: 1.0, pitch: 2.0, yaw: 3.0,
                is_target: true, match_obs: true, is_visible: true,
                file_path: String::new(),
                default_x: 0.0, default_y: 0.0, default_width: 100.0, default_height: 100.0, default_roll: 0.0,
            },
            crate::StudioSource {
                scene: "Scène".to_string(),
                name: "Src2".to_string(),
                slot: 2,
                x: 50.0, y: 60.0, width: 200.0, height: 150.0,
                roll: 0.0, pitch: 0.0, yaw: 0.0,
                is_target: false, match_obs: false, is_visible: false,
                file_path: String::new(),
                default_x: 0.0, default_y: 0.0, default_width: 200.0, default_height: 150.0, default_roll: 0.0,
            }
        ];

        let snapshots = snapshot_all_sources(&sources);
        assert_eq!(snapshots.len(), 2);
        assert_eq!(snapshots[0].source_name, "Src1");
        assert_eq!(snapshots[0].x, 10.0);
        assert_eq!(snapshots[0].is_visible, true);
        assert_eq!(snapshots[1].source_name, "Src2");
        assert_eq!(snapshots[1].is_visible, false);
    }

    #[test]
    fn test_smooth_transition_interpolation() {
        let _serial = POSE_TESTS.lock();
        let mut sources = vec![
            crate::StudioSource {
                scene: "Scène".to_string(),
                name: "Src1".to_string(),
                slot: 1,
                x: 0.0, y: 0.0, width: 100.0, height: 100.0,
                roll: 0.0, pitch: 0.0, yaw: 0.0,
                is_target: true, match_obs: true, is_visible: true,
                file_path: String::new(),
                default_x: 0.0, default_y: 0.0, default_width: 100.0, default_height: 100.0, default_roll: 0.0,
            }
        ];

        let target_set = SceneCoordinateSet {
            id: "target1".to_string(),
            name: "Target Pose".to_string(),
            trigger: "Twitch * redeem Pose 1".to_string(),
            scene: "Scène".to_string(),
            easing: MovementEasing::Linear,
            duration_ms: 1000,
            sources: vec![
                SourceTransformSnapshot {
                    source_name: "Src1".to_string(),
                    x: 100.0, y: 200.0, width: 300.0, height: 400.0,
                    roll: 45.0, pitch: 10.0, yaw: 20.0,
                    is_visible: true,
                    filters: None,
                    volume: None,
                }
            ],
        };

        start_scene_transition(&target_set, &sources);
        assert!(ACTIVE_TRANSITION.lock().is_some());

        let ctx = egui::Context::default();
        let mut modified = false;

        // Simulate halfway elapsed
        {
            let mut trans = ACTIVE_TRANSITION.lock();
            if let Some(t) = trans.as_mut() {
                t.start_time_ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64 - 500;
            }
        }
        tick_active_transition(&ctx, &mut sources, &mut modified);
        assert!(modified);
        assert!((sources[0].x - 50.0).abs() < 5.0);
        assert!((sources[0].y - 100.0).abs() < 5.0);

        // Simulate finish
        {
            let mut trans = ACTIVE_TRANSITION.lock();
            if let Some(t) = trans.as_mut() {
                t.start_time_ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64 - 1100;
            }
        }
        tick_active_transition(&ctx, &mut sources, &mut modified);
        assert_eq!(sources[0].x, 100.0);
        assert_eq!(sources[0].y, 200.0);
        assert_eq!(sources[0].width, 300.0);
        assert_eq!(sources[0].height, 400.0);
        assert_eq!(sources[0].roll, 45.0);
        assert_eq!(sources[0].pitch, 10.0);
        assert_eq!(sources[0].yaw, 20.0);
        assert!(ACTIVE_TRANSITION.lock().is_none());
    }

    #[test]
    fn test_trigger_matching_and_instant_transition() {
        let _serial = POSE_TESTS.lock();
        let set = SceneCoordinateSet {
            id: "set_trig_test".to_string(),
            name: "BRB Screen".to_string(),
            trigger: "Twitch * redeem BRB".to_string(),
            scene: "Scène".to_string(),
            easing: MovementEasing::Instant,
            duration_ms: 0,
            sources: vec![
                SourceTransformSnapshot {
                    source_name: "Cam".to_string(),
                    x: 999.0, y: 888.0, width: 500.0, height: 500.0,
                    roll: 0.0, pitch: 0.0, yaw: 0.0,
                    is_visible: false,
                    filters: None,
                    volume: None,
                }
            ],
        };

        // Case-insensitive matching against the Reader event's trigger string
        assert!(match_trigger_wildcard(&set.trigger, "twitch Mortygotvr REDEEM brb"));

        // Test instant transition execution
        let mut sources = vec![
            crate::StudioSource {
                scene: "Scène".to_string(),
                name: "Cam".to_string(),
                slot: 1,
                x: 0.0, y: 0.0, width: 200.0, height: 200.0,
                roll: 0.0, pitch: 0.0, yaw: 0.0,
                is_target: true, match_obs: true, is_visible: true,
                file_path: String::new(),
                default_x: 0.0, default_y: 0.0, default_width: 200.0, default_height: 200.0, default_roll: 0.0,
            }
        ];

        start_scene_transition(&set, &sources);
        let ctx = egui::Context::default();
        let mut modified = false;
        tick_active_transition(&ctx, &mut sources, &mut modified);

        assert!(modified);
        assert_eq!(sources[0].x, 999.0);
        assert_eq!(sources[0].y, 888.0);
        assert_eq!(sources[0].width, 500.0);
        assert_eq!(sources[0].height, 500.0);
        assert_eq!(sources[0].is_visible, false);
        assert!(ACTIVE_TRANSITION.lock().is_none());
    }

    #[test]
    fn test_redeem_config_serde_flatten_preserves_custom_fields() {
        let sample_json = r#"{
            "name": "Custom Milk Splash",
            "version": "1.0",
            "type": "webgl-water",
            "fluidPreset": "milk",
            "fluidCount": 150,
            "customShadingModel": "glass_subsurface",
            "material": {
                "preset": "milk",
                "smoothness": 0.95,
                "metallic": 0.1
            },
            "customNestedArray": [1, 2, 3]
        }"#;

        let config: RedeemConfig = serde_json::from_str(sample_json).expect("Should deserialize");
        assert_eq!(config.name, "Custom Milk Splash");
        assert_eq!(config.redeem_type, "webgl-water");
        assert_eq!(config.fluid_preset.as_deref(), Some("milk"));
        assert_eq!(config.fluid_count, Some(150));

        // Custom fields must be preserved in `extra` map
        assert!(config.extra.contains_key("customShadingModel"));
        assert_eq!(config.extra["customShadingModel"], serde_json::json!("glass_subsurface"));
        assert!(config.extra.contains_key("material") || config.material.is_some());
        assert!(config.extra.contains_key("customNestedArray"));

        // Roundtrip re-serialization must keep extra fields intact
        let reserialized = serde_json::to_string_pretty(&config).expect("Should serialize");
        let roundtrip_val: serde_json::Value = serde_json::from_str(&reserialized).expect("Should parse roundtrip");
        assert_eq!(roundtrip_val["customShadingModel"], serde_json::json!("glass_subsurface"));
        assert_eq!(roundtrip_val["customNestedArray"], serde_json::json!([1, 2, 3]));
        assert_eq!(roundtrip_val["material"]["smoothness"], serde_json::json!(0.95));
    }

    #[test]
    fn test_all_redeem_files_deserialize_cleanly() {
        let redeems = load_all_redeems_from_disk();
        if redeems.is_empty() {
            println!("[Test] Skipping test_all_redeem_files_deserialize_cleanly because redeems/ directory is empty.");
            return;
        }

        for item in &redeems {
            assert!(!item.filename.is_empty());
            assert!(!item.config.name.is_empty(), "File {} should have a name", item.filename);
            assert!(!item.config.redeem_type.is_empty(), "File {} should have a redeem_type", item.filename);

            // Verify serialization succeeds
            let serialized = serde_json::to_string(&item.config);
            assert!(serialized.is_ok(), "Serialization failed for {}", item.filename);
        }
    }

    #[test]
    fn test_redeem_file_lifecycle_save_and_delete() {
        let temp_dir = std::env::temp_dir();
        let test_path = temp_dir.join("test_trust_temp_redeem.json");

        let mut item = RedeemItem {
            filename: "test_trust_temp_redeem.json".to_string(),
            path: test_path.clone(),
            config: RedeemConfig {
                name: "Temp Test Item".to_string(),
                version: "1.0".to_string(),
                redeem_type: "image".to_string(),
                action: Some("throwable".to_string()),
                collision: Some("pixel-shatter".to_string()),
                speed: Some(12.5),
                scale: Some(1.2),
                gravity: Some(0.9),
                spawn_points: Some(vec!["tc".to_string(), "ml".to_string()]),
                snap_bottom: Some(true),
                random_mirror: Some(false),
                triggers: Some(vec!["Twitch * redeem Test Trigger".to_string()]),
                image_src: None,
                start_sound: None,
                collision_sound: None,
                fluid_preset: None,
                fluid_count: None,
                fluid_radius: None,
                fluid_viscosity: None,
                fluid_gravity: None,
                fluid_duration: None,
                fluid_width: None,
                stream_mode: None,
                drip_through_percent: None,
                color_r: None,
                color_g: None,
                color_b: None,
                color_a: None,
                color2_r: None,
                color2_g: None,
                color2_b: None,
                color2_a: None,
                enable_metaball: None,
                material: None,
                shake_on_impact: None,
                ..RedeemConfig::default()
            },
            is_dirty: true,
            triggers_input: "Twitch * redeem Test Trigger".to_string(),
        };

        // 1. Save
        let save_res = save_redeem_to_disk(&mut item);
        assert!(save_res.is_ok());
        assert!(!item.is_dirty);
        assert!(test_path.exists());

        // 2. Read back & verify
        let content = std::fs::read_to_string(&test_path).expect("File must be readable");
        let loaded: RedeemConfig = serde_json::from_str(&content).expect("Must parse");
        assert_eq!(loaded.name, "Temp Test Item");
        assert_eq!(loaded.speed, Some(12.5));
        assert_eq!(loaded.snap_bottom, Some(true));

        // 3. Delete
        let del_res = delete_redeem_file(&test_path);
        assert!(del_res.is_ok());
        assert!(!test_path.exists());
    }

    #[test]
    fn test_redeem_fluid_presets_application() {
        let mut cfg = RedeemConfig {
            name: "Fluid Test".to_string(),
            version: "1.0".to_string(),
            redeem_type: "water".to_string(),
            ..RedeemConfig::default()
        };

        // 1. Water
        cfg.apply_fluid_preset("water");
        assert_eq!(cfg.fluid_preset, Some("water".to_string()));
        assert_eq!(cfg.color_r, Some(38));
        assert_eq!(cfg.color_g, Some(155));
        assert_eq!(cfg.color_b, Some(230));
        assert!((cfg.fluid_viscosity.unwrap() - 0.50).abs() < 0.001);
        assert!((cfg.fluid_gravity.unwrap() - 0.80).abs() < 0.001);

        // 2. Milk
        cfg.apply_fluid_preset("milk");
        assert_eq!(cfg.fluid_preset, Some("milk".to_string()));
        assert_eq!(cfg.color_r, Some(245));
        assert_eq!(cfg.color_g, Some(245));
        assert_eq!(cfg.color_b, Some(240));
        assert!((cfg.fluid_viscosity.unwrap() - 0.65).abs() < 0.001);

        // 3. Lava
        cfg.apply_fluid_preset("lava");
        assert_eq!(cfg.fluid_preset, Some("lava".to_string()));
        assert_eq!(cfg.color_r, Some(170));
        assert_eq!(cfg.color_g, Some(25));
        assert_eq!(cfg.color_b, Some(0));
        assert!((cfg.fluid_viscosity.unwrap() - 0.90).abs() < 0.001);

        // 4. Blood
        cfg.apply_fluid_preset("blood");
        assert_eq!(cfg.fluid_preset, Some("blood".to_string()));
        assert_eq!(cfg.color_r, Some(160));
        assert_eq!(cfg.color_g, Some(0));
        assert_eq!(cfg.color_b, Some(15));
        assert!((cfg.fluid_viscosity.unwrap() - 0.75).abs() < 0.001);
    }

    #[test]
    fn test_redeem_shake_and_reactions_getters_and_setters() {
        let mut cfg = RedeemConfig {
            name: "Reaction Test".to_string(),
            version: "1.0".to_string(),
            redeem_type: "image".to_string(),
            ..RedeemConfig::default()
        };

        // Shake
        assert!(cfg.get_shake_enabled());
        cfg.set_shake_enabled(false);
        assert!(!cfg.get_shake_enabled());
        cfg.set_shake_enabled(true);
        assert!(cfg.get_shake_enabled());
        cfg.set_shake_duration(350);
        assert_eq!(cfg.get_shake_duration(), 350);
        cfg.set_shake_intensity(15);
        assert_eq!(cfg.get_shake_intensity(), 15);

        // Bounce
        assert!(cfg.get_bounce_enabled());
        cfg.set_bounce_enabled(false);
        assert!(!cfg.get_bounce_enabled());
        cfg.set_bounce_enabled(true);
        assert!(cfg.get_bounce_enabled());
        cfg.set_bounce_squish(0.42);
        assert!((cfg.get_bounce_squish() - 0.42).abs() < 0.001);
        cfg.set_bounce_duration(600);
        assert_eq!(cfg.get_bounce_duration(), 600);

        // Fluid pixel drip
        assert!(!cfg.get_drip_enabled());
        cfg.set_drip_enabled(true);
        assert!(cfg.get_drip_enabled());
        cfg.set_drip_viscosity(0.85);
        assert!((cfg.get_drip_viscosity() - 0.85).abs() < 0.001);
        cfg.set_drip_duration(3500);
        assert_eq!(cfg.get_drip_duration(), 3500);

        // Jolt
        assert!(!cfg.get_jolt_enabled());
        cfg.set_jolt_enabled(true);
        assert!(cfg.get_jolt_enabled());
        cfg.set_jolt_intensity(1.25);
        assert!((cfg.get_jolt_intensity() - 1.25).abs() < 0.001);
        cfg.set_jolt_duration(450);
        assert_eq!(cfg.get_jolt_duration(), 450);
    }

    #[test]
    fn test_redeem_target_slot_mapping() {
        let mut cfg = RedeemConfig {
            name: "Target Test".to_string(),
            version: "1.0".to_string(),
            redeem_type: "image".to_string(),
            target: Some("Image".to_string()),
            ..RedeemConfig::default()
        };

        assert_eq!(cfg.get_target(), "Image");
        assert_eq!(cfg.get_target_slot(), "Image");
        cfg.set_target("random");
        assert_eq!(cfg.get_target(), "random");
        assert_eq!(cfg.target, Some("random".to_string()));

        cfg.set_target("Image 2");
        assert_eq!(cfg.get_target(), "Image 2");
        assert_eq!(cfg.get_target_slot(), "Image 2");
    }

    #[test]
    fn test_resolve_and_encode_asset() {
        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join("test_studio_asset.png");
        let dummy_data = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR";
        std::fs::write(&test_file, dummy_data).expect("Must write temp png");

        let res = resolve_and_encode_asset(test_file.to_str().unwrap());
        assert!(res.is_some());
        let uri = res.unwrap();
        assert!(uri.starts_with("data:image/png;base64,"));
        let _ = std::fs::remove_file(test_file);
    }

    #[test]
    fn test_dispatch_test_redeem_json() {
        let water_json = r#"{
            "name": "Test Milk",
            "type": "webgl-water",
            "target": "Image",
            "fluidCount": 10,
            "fluidRadius": 15.0
        }"#;
        assert!(dispatch_test_redeem_json(water_json));

        let fire_json = r#"{
            "name": "Test Fire",
            "type": "webgl-fire",
            "target": "Image",
            "fluidCount": 10
        }"#;
        assert!(dispatch_test_redeem_json(fire_json));

        let lightning_json = r#"{
            "name": "Test Lightning",
            "type": "webgl-lightning",
            "target": "Image",
            "strikeCount": 1
        }"#;
        assert!(dispatch_test_redeem_json(lightning_json));

        let image_json = r#"{
            "name": "Test Throwable",
            "type": "image",
            "target": "Image",
            "speed": 10.0
        }"#;
        assert!(dispatch_test_redeem_json(image_json));
    }

    #[test]
    fn test_launch_audio_all_redeems_and_impact_throwable_only() {
        let temp_dir = std::env::temp_dir();
        let dummy_sound_file = temp_dir.join("test_sound_fx.wav");
        let _ = std::fs::write(&dummy_sound_file, b"RIFF....WAVEfmt ....data....");
        let snd_path = dummy_sound_file.to_str().unwrap().to_string();
        // As a JSON string: a Windows path's backslashes must be escaped
        let snd_json = serde_json::to_string(&snd_path).unwrap();

        // 1. Water redeem with launch sound
        let water_json = format!(
            r#"{{
                "name": "Water With Sound",
                "type": "webgl-water",
                "target": "Image",
                "startSoundPath": {},
                "fluidCount": 10
            }}"#,
            snd_json
        );
        assert!(dispatch_test_redeem_json(&water_json));

        // 2. Fire redeem with launch sound
        let fire_json = format!(
            r#"{{
                "name": "Fire With Sound",
                "type": "webgl-fire",
                "target": "Image",
                "start_sound": "data:audio/wav;base64,UklGRg==",
                "fluidCount": 10
            }}"#
        );
        assert!(dispatch_test_redeem_json(&fire_json));

        // 3. Lightning redeem with launch sound
        let lightning_json = format!(
            r#"{{
                "name": "Lightning With Sound",
                "type": "webgl-lightning",
                "target": "Image",
                "startSound": "data:audio/wav;base64,UklGRg==",
                "strikeCount": 1
            }}"#
        );
        assert!(dispatch_test_redeem_json(&lightning_json));

        // 4. Image throwable with both launch and impact sound
        let image_json = format!(
            r#"{{
                "name": "Throwable With Both Sounds",
                "type": "image",
                "target": "Image",
                "start_sound_path": {},
                "collision_sound_path": {},
                "speed": 10.0
            }}"#,
            snd_json, snd_json
        );
        assert!(dispatch_test_redeem_json(&image_json));

        // 5. Test saving non-throwable strips impact sound
        let mut non_throwable_item = RedeemItem {
            filename: "water_test.json".to_string(),
            path: temp_dir.join("water_test.json"),
            config: RedeemConfig {
                name: "Water Test".to_string(),
                version: "1.0".to_string(),
                redeem_type: "webgl-water".to_string(),
                start_sound_path: Some(snd_path.clone()),
                collision_sound_path: Some(snd_path.clone()),
                collision_sound: Some("dummy_b64".to_string()),
                ..RedeemConfig::default()
            },
            is_dirty: true,
            triggers_input: String::new(),
        };
        assert!(save_redeem_to_disk(&mut non_throwable_item).is_ok());
        assert!(non_throwable_item.config.start_sound_path.is_some());
        assert_eq!(non_throwable_item.config.collision_sound, None);
        assert_eq!(non_throwable_item.config.collision_sound_path, None);
        let _ = std::fs::remove_file(&non_throwable_item.path);

        // 6. Test saving throwable retains impact sound
        let mut throwable_item = RedeemItem {
            filename: "image_test.json".to_string(),
            path: temp_dir.join("image_test.json"),
            config: RedeemConfig {
                name: "Image Test".to_string(),
                version: "1.0".to_string(),
                redeem_type: "image".to_string(),
                start_sound_path: Some(snd_path.clone()),
                collision_sound_path: Some(snd_path.clone()),
                collision_sound: Some("dummy_b64".to_string()),
                ..RedeemConfig::default()
            },
            is_dirty: true,
            triggers_input: String::new(),
        };
        assert!(save_redeem_to_disk(&mut throwable_item).is_ok());
        assert!(throwable_item.config.start_sound_path.is_some());
        assert!(throwable_item.config.collision_sound.is_some());
        assert!(throwable_item.config.collision_sound_path.is_some());
        let _ = std::fs::remove_file(&throwable_item.path);
        let _ = std::fs::remove_file(dummy_sound_file);
    }

    #[test]
    fn test_redeem_image_loading_and_single_spawn() {
        let state = get_state();
        state.physics.write().clear();
        assert_eq!(state.physics.read().throwables.len(), 0);

        let tomato_json = r#"{
            "name": "Tomato",
            "type": "image",
            "action": "throwable",
            "target": "Avatar",
            "imageSrc": "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==",
            "speed": 8.0,
            "scale": 1.0,
            "gravity": 0.8
        }"#;

        assert!(dispatch_test_redeem_json(tomato_json));

        let p = state.physics.read();
        assert_eq!(p.throwables.len(), 1, "Exactly ONE throwable should be spawned per test redeem");
        let th = &p.throwables[0];
        assert_eq!(th.name, "Tomato");
        assert!(p.redeem_images.contains_key(&th.redeem_id), "Image source must be cached in redeem_images");
        let img = p.redeem_images.get(&th.redeem_id).unwrap();
        assert!(img.starts_with("data:image/png;base64,"));
    }

    #[test]
    fn test_match_trigger_wildcard() {
        let incoming_redeem = "Twitch Mortygotvr redeem Throw Tomato";
        let incoming_kick = "Kick Mortygotvr redeem Confetti";

        // Exact matches without wildcards
        assert!(match_trigger_wildcard("Twitch Mortygotvr redeem Throw Tomato", incoming_redeem));
        assert!(match_trigger_wildcard("twitch mortygotvr redeem throw tomato", incoming_redeem));
        assert!(match_trigger_wildcard("Kick Mortygotvr redeem Confetti", incoming_kick));

        // Exact anchored wildcard patterns
        assert!(match_trigger_wildcard("Twitch * redeem Throw Tomato", incoming_redeem));
        assert!(match_trigger_wildcard("Twitch * redeem *", incoming_redeem));
        assert!(match_trigger_wildcard("Twitch * redeem * Tomato", incoming_redeem));
        assert!(match_trigger_wildcard("* redeem Throw Tomato", incoming_redeem));
        assert!(match_trigger_wildcard("Kick Mortygotvr redeem *", incoming_kick));
        assert!(match_trigger_wildcard("*", incoming_redeem));

        // Negative cases (must NOT match substring without wildcards)
        assert!(!match_trigger_wildcard("Throw Tomato", incoming_redeem));
        assert!(!match_trigger_wildcard("redeem Throw Tomato", incoming_redeem));
        assert!(!match_trigger_wildcard("Kick Mortygotvr redeem", incoming_kick));
        assert!(!match_trigger_wildcard("Kick *", incoming_redeem));
        assert!(!match_trigger_wildcard("Banana", incoming_redeem));
        assert!(!match_trigger_wildcard("Twitch Mortygotvr redeem *", incoming_kick));
        assert!(!match_trigger_wildcard("", incoming_redeem));
        assert!(!match_trigger_wildcard("Twitch Mortygotvr redeem Throw Tomato", ""));
    }

    #[test]
    fn test_match_and_extract_wildcards_with_numbers() {
        // 1. Single wildcard matching a number
        let res = match_and_extract_wildcards("Twitch Mortygotvr redeem Throw *", "Twitch Mortygotvr redeem Throw 3");
        assert_eq!(res, Some(vec!["3".to_string()]));

        // 2. Multi-wildcard with positional arguments and numbers
        let res2 = match_and_extract_wildcards(
            "Twitch Mortygotvr redeem Throw * * *",
            "Twitch Mortygotvr redeem Throw tomato avatar 3",
        );
        assert_eq!(res2, Some(vec!["tomato".to_string(), "avatar".to_string(), "3".to_string()]));

        // 3. Braced placeholders in pattern normalized to wildcards
        let res3 = match_and_extract_wildcards(
            "Twitch Mortygotvr redeem Throw {1} {2} {3}",
            "Twitch Mortygotvr redeem Throw tomato avatar 5",
        );
        assert_eq!(res3, Some(vec!["tomato".to_string(), "avatar".to_string(), "5".to_string()]));

        // 4. Pattern with {3} directly
        let res4 = match_and_extract_wildcards("Twitch Mortygotvr redeem Throw {3}", "Twitch Mortygotvr redeem Throw 10");
        assert_eq!(res4, Some(vec!["10".to_string()]));

        // 5. Channel points redeem with number
        let res5 = match_and_extract_wildcards("Twitch * redeem Throw *", "Twitch Mortygotvr redeem Throw 7");
        assert_eq!(res5, Some(vec!["Mortygotvr".to_string(), "7".to_string()]));
    }

    #[test]
    fn test_dispatch_redeem_with_braced_three_amount() {
        let state = get_state();
        state.physics.write().throwables.clear();

        // Redeem with amount set to "{3}" and no wildcards passed (e.g. manual test launch)
        let json_test = r#"{
            "name": "Triple Tomato",
            "type": "image",
            "action": "throwable",
            "target": "Avatar",
            "amount": "{3}",
            "imageSrc": "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==",
            "speed": 8.0
        }"#;

        assert!(dispatch_test_redeem_json(json_test));
        assert_eq!(state.physics.read().throwables.len(), 3, "Amount '{{3}}' without arguments must spawn 3 objects");

        // Now test with wildcards provided: 3rd argument is "5"
        state.physics.write().throwables.clear();
        let json_with_wcards = r#"{
            "name": "Custom Count Tomato",
            "type": "image",
            "action": "throwable",
            "target": "Avatar",
            "amount": "{3}",
            "wildcards": ["tomato", "Avatar", "5"],
            "imageSrc": "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==",
            "speed": 8.0
        }"#;

        assert!(dispatch_test_redeem_json(json_with_wcards));
        assert_eq!(state.physics.read().throwables.len(), 5, "Amount '{{3}}' with 3rd wildcard '5' must spawn 5 objects");
    }

    #[test]
    fn test_trigger_system_works_without_studio_open() {
        let _serial = POSE_TESTS.lock();
        // Studio window is completely closed
        IS_STUDIO_WINDOW_VISIBLE.store(false, std::sync::atomic::Ordering::SeqCst);
        let state = get_state();
        state.physics.write().throwables.clear();

        // Ensure global redeems are loaded — this test needs a specific
        // "tomato" fixture redeem, not just any redeems on disk (a real
        // user's unrelated redeem files must not make this test try to run
        // against data it was never written for).
        reload_global_redeems();
        let has_tomato_fixture = GLOBAL_REDEEMS.read().iter().any(|r| {
            r.config.triggers.as_ref().map(|t| t.iter().any(|p| p.to_lowercase().contains("tomato"))).unwrap_or(false)
        });
        if !has_tomato_fixture {
            println!("[Test] Skipping test_trigger_system_works_without_studio_open because no 'tomato' fixture redeem exists on disk.");
            return;
        }

        // The Reader event the fixture redeem's trigger pattern matches
        let pattern = GLOBAL_REDEEMS.read().iter()
            .filter_map(|r| r.config.triggers.as_ref())
            .flatten()
            .find(|p| p.to_lowercase().contains("tomato"))
            .cloned()
            .unwrap();
        let event_trigger = normalize_pattern(&pattern).replace('*', "Mortygotvr");

        // There are no chat triggers: a chat message saying exactly that must not fire it
        process_trigger_event(&chat_message(&event_trigger));
        assert!(state.physics.read().throwables.is_empty(), "A chat message must never fire a redeem");

        // The Reader event does, without studio open
        process_trigger_event(&reader_event(&event_trigger));

        let p = state.physics.read();
        assert!(!p.throwables.is_empty(), "Tomato throwable must spawn when studio is closed");
        assert_eq!(p.throwables[0].name, "Tomato");
        drop(p);

        // Test coordinate set trigger without studio open
        keep_a_source_loaded();
        SCENE_COORDINATE_SETS.write().push(SceneCoordinateSet {
            id: "set_test_1".to_string(),
            name: "Background Test Set".to_string(),
            scene: "".to_string(),
            trigger: "Twitch * redeem Close-up".to_string(),
            duration_ms: 100,
            easing: MovementEasing::Linear,
            sources: vec![],
        });

        process_trigger_event(&chat_message("Twitch Mortygotvr redeem Close-up"));
        assert_ne!(last_applied_pose().1, "set_test_1", "A chat message must never fire a pose");

        process_trigger_event(&reader_event("Twitch Mortygotvr redeem Close-up"));
        assert_eq!(last_applied_pose().1, "set_test_1", "The Reader event must fire the pose without studio open");
        assert!(ACTIVE_TRANSITION.lock().is_some(), "Active transition must start without studio open");

        // Tick standalone transition without studio open
        tick_active_transition_standalone();
    }

    #[test]
    fn test_load_and_apply_last_pose_on_load() {
        let state = crate::state::get_state();
        let scene_name = "TestScenePose";
        let src_name = "PoseLoadAvatar";

        // Setup a source with initial coords (0, 0)
        let initial_src = crate::StudioSource {
            scene: scene_name.to_string(),
            name: src_name.to_string(),
            slot: 1,
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 100.0,
            roll: 0.0,
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
            sources.push(initial_src);
        }

        // Add a pose set to SCENE_COORDINATE_SETS
        let test_set = SceneCoordinateSet {
            id: "set_test_pose_load".to_string(),
            name: "Pose 3".to_string(),
            scene: scene_name.to_string(),
            trigger: "".to_string(),
            duration_ms: 800,
            easing: MovementEasing::EaseOut,
            sources: vec![SourceTransformSnapshot {
                source_name: src_name.to_string(),
                x: 450.0,
                y: 650.0,
                width: 300.0,
                height: 400.0,
                roll: 25.0,
                pitch: 0.0,
                yaw: 0.0,
                is_visible: true,
                filters: None,
                volume: None,
            }],
        };

        {
            let mut sets = SCENE_COORDINATE_SETS.write();
            sets.retain(|s| s.id != "set_test_pose_load");
            sets.push(test_set);
        }

        // Save this set as the active pose ID
        save_last_active_pose_id("set_test_pose_load");

        // Call load_and_apply_last_pose_on_load
        load_and_apply_last_pose_on_load(scene_name);

        // Verify the source in studio_sources now holds the pose coords (450, 650, 300x400, roll 25)
        {
            let sources = state.studio_sources.read();
            let s = sources.iter().find(|s| s.name == src_name).unwrap();
            assert_eq!(s.x, 450.0);
            assert_eq!(s.y, 650.0);
            assert_eq!(s.width, 300.0);
            assert_eq!(s.height, 400.0);
            assert_eq!(s.roll, 25.0);
        }
    }
}

