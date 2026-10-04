// ============================================================================
// Trust — SPH Fluid Engine (2D Smoothed Particle Hydrodynamics)
// ============================================================================
// Ports WebGLWaterStream.js physics into native Rust.
// Supports presets: water, slime, lava, sand, goo via config tuning.

use std::collections::HashMap;
use crate::types::Hitbox;

// ---------------------------------------------------------------------------
// Per-engine LCG RNG (avoids the time-based xorshift collision issue)
// ---------------------------------------------------------------------------
pub struct Rng {
    state: u64,
}

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self { state: seed.max(1) }
    }
    pub fn next_f32(&mut self) -> f32 {
        // LCG: Knuth's constants
        self.state = self.state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        // Top 32 bits as the output word. (Previously shifted by 33, keeping
        // only 31 bits while still dividing by u32::MAX — that silently
        // capped every output at ~0.5, so every "(r - 0.5) * spread" idiom
        // anywhere in fluid/fire was always <= 0: a hard bias toward
        // left/up instead of a true centered spread.)
        ((self.state >> 32) as u32 as f32) / (u32::MAX as f32)
    }
}

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------
#[derive(Debug, Clone)]
pub struct SphConfig {
    pub particle_count: usize,
    /// Smoothing radius in pixels
    pub h: f32,
    pub rest_density: f32,
    /// Pressure stiffness
    pub pressure_k: f32,
    /// Viscosity coefficient
    pub viscosity: f32,
    /// Pixels/frame² downward (use negative for upward/fire-like)
    pub gravity: f32,
    pub duration_ms: f32,
    /// Base color [0–1]
    pub color: [f32; 3],
    /// Highlight / inner color [0–1]
    pub color2: [f32; 3],
    pub color_a: f32,
    pub color2_a: f32,
    /// % of particles that pass through hitbox vs slide around
    pub drip_through_percent: f32,
    /// Spawn spread width in pixels
    pub fluid_width: f32,
    /// Expanding beam cone spread width in pixels at target
    pub cone_spread: f32,
    /// "grid" = arc toward target  |  "top" = falls from above
    pub stream_mode: String,
    /// Overall travel speed multiplier — separate from gravity (arc shape
    /// only) and viscosity (drag after emission only). 1.0 = normal.
    pub stream_speed: f32,
    /// Optional Screen Shake fired at stream start (no discrete "impact" here).
    pub shake: crate::physics::ShakeConfig,
    /// Optional avatar reaction fired at stream start (see `shake` above — same reasoning).
    pub impact_reaction: crate::physics::ImpactReactionConfig,
    /// When true, settled/dripping particles never fade or shrink away — lets
    /// water actually accumulate and stay put (e.g. filling a cup/recipient)
    /// instead of every particle dissolving a few seconds after landing.
    pub no_decay: bool,
    /// Per-redeem metaball rendering look (texture/scale/refraction/edge
    /// style/blob size) — see `WaterVisualSettings`.
    pub water_visual: WaterVisualSettings,
    /// Optional redeem key or name associating this fluid stream with a specific redeem
    pub redeem_key: Option<String>,
    /// Optional sound to play on contact with target or ground (e.g. splash, splat, sizzle)
    pub contact_sound: Option<String>,
    /// Cooldown between contact sound triggers in milliseconds (debounces high-frequency particle hits)
    pub contact_sound_cooldown_ms: f32,
    /// When true, only plays contact sound on the very first contact of the stream
    pub contact_sound_first_only: bool,
    /// Whether contact sound triggers when fluid hits canvas ground
    pub contact_sound_on_ground: bool,
}

impl Default for SphConfig {
    fn default() -> Self {
        Self {
            particle_count: 120,
            h: 18.0,
            rest_density: 1.0,
            pressure_k: 200.0,
            viscosity: 0.5,
            gravity: 0.8,
            duration_ms: 6000.0,
            color: [0.0, 0.667, 1.0],
            color2: [0.392, 0.824, 1.0],
            color_a: 0.95,
            color2_a: 0.85,
            drip_through_percent: 50.0,
            fluid_width: 30.0,
            cone_spread: 30.0,
            stream_mode: "grid".to_string(),
            stream_speed: 1.0,
            shake: crate::physics::ShakeConfig::default(),
            impact_reaction: crate::physics::ImpactReactionConfig::default(),
            no_decay: false,
            water_visual: WaterVisualSettings::default(),
            redeem_key: None,
            contact_sound: None,
            contact_sound_cooldown_ms: 120.0,
            contact_sound_first_only: false,
            contact_sound_on_ground: true,
        }
    }
}

/// Per-redeem metaball rendering look. Carried from `SphConfig` (parsed from
/// the redeem's own saved JSON — see `RedeemConfig` in studio_app.rs) into
/// the render layer via a small process-wide registry: the flat particle
/// export buffer only has room for plain floats per particle, so each engine
/// registers its settings once at spawn time and bakes the resulting index
/// into every one of its particles (see `fill_render_buf`) — the renderer
/// (obs_source.rs) resolves that index back into these settings, and groups
/// particles by (color, index) instead of just color, so two simultaneous
/// streams with different textures/settings never bleed into each other.
#[derive(Debug, Clone, PartialEq)]
pub struct WaterVisualSettings {
    pub texture_path: Option<String>,
    pub texture_scale: f32,
    /// Pixels/second the water texture drifts downward — see the
    /// hardcoded-then-slider fix in filters/metaball.effect.
    pub texture_scroll_speed: f32,
    pub refraction_enabled: bool,
    pub refraction_strength: f32,
    pub round_edge: bool,
    pub metaball_size: f32,
}

impl Default for WaterVisualSettings {
    fn default() -> Self {
        Self {
            texture_path: None,
            texture_scale: 160.0,
            texture_scroll_speed: 12.0,
            refraction_enabled: true,
            refraction_strength: 350.0,
            round_edge: false,
            metaball_size: 9.5,
        }
    }
}

static WATER_VISUAL_REGISTRY: std::sync::LazyLock<parking_lot::Mutex<Vec<WaterVisualSettings>>> =
    std::sync::LazyLock::new(|| parking_lot::Mutex::new(Vec::new()));

/// Registers a copy of `settings`, returning an index that can round-trip
/// through the particle export buffer as a plain `f32` (see `water_visual_settings`).
pub fn register_water_visual_settings(settings: WaterVisualSettings) -> u32 {
    let mut reg = WATER_VISUAL_REGISTRY.lock();
    // Reuse an existing entry if one already matches exactly, so a long
    // session re-triggering the same redeem over and over doesn't grow this
    // registry unboundedly.
    if let Some(idx) = reg.iter().position(|s| *s == settings) {
        return idx as u32;
    }
    reg.push(settings);
    (reg.len() - 1) as u32
}

pub fn water_visual_settings(idx: u32) -> Option<WaterVisualSettings> {
    WATER_VISUAL_REGISTRY.lock().get(idx as usize).cloned()
}

fn parse_hex_color(s: &str) -> Option<([f32; 3], Option<f32>)> {
    let clean = s.trim().trim_start_matches('#');
    if clean.len() == 6 {
        let r = u8::from_str_radix(&clean[0..2], 16).ok()? as f32 / 255.0;
        let g = u8::from_str_radix(&clean[2..4], 16).ok()? as f32 / 255.0;
        let b = u8::from_str_radix(&clean[4..6], 16).ok()? as f32 / 255.0;
        Some(([r, g, b], None))
    } else if clean.len() == 8 {
        let r = u8::from_str_radix(&clean[0..2], 16).ok()? as f32 / 255.0;
        let g = u8::from_str_radix(&clean[2..4], 16).ok()? as f32 / 255.0;
        let b = u8::from_str_radix(&clean[4..6], 16).ok()? as f32 / 255.0;
        let a = u8::from_str_radix(&clean[6..8], 16).ok()? as f32 / 255.0;
        Some(([r, g, b], Some(a)))
    } else {
        None
    }
}

fn parse_color_value(val: &serde_json::Value) -> Option<([f32; 3], Option<f32>)> {
    if let Some(s) = val.as_str() {
        parse_hex_color(s)
    } else if let Some(arr) = val.as_array() {
        if arr.len() >= 3 {
            let r_val = arr[0].as_f64()? as f32;
            let g_val = arr[1].as_f64()? as f32;
            let b_val = arr[2].as_f64()? as f32;
            let max_val = r_val.max(g_val).max(b_val);
            let scale = if max_val > 1.0 { 255.0 } else { 1.0 };
            let r = (r_val / scale).clamp(0.0, 1.0);
            let g = (g_val / scale).clamp(0.0, 1.0);
            let b = (b_val / scale).clamp(0.0, 1.0);
            let a = if arr.len() >= 4 {
                let a_val = arr[3].as_f64()? as f32;
                Some(if a_val > 1.0 { (a_val / 255.0).clamp(0.0, 1.0) } else { a_val.clamp(0.0, 1.0) })
            } else {
                None
            };
            Some(([r, g, b], a))
        } else {
            None
        }
    } else {
        None
    }
}

impl SphConfig {
    /// Parse from a flat key=value map produced by JSON decoding on the Lua side
    pub fn from_json_map(m: &HashMap<String, serde_json::Value>) -> Self {
        // 1. Check for nested material object if present
        let mat_obj = m.get("material").and_then(|v| v.as_object());

        let preset_str = mat_obj
            .and_then(|mat| mat.get("preset").and_then(|v| v.as_str()))
            .or_else(|| m.get("preset").or_else(|| m.get("fluidPreset")).and_then(|v| v.as_str()));

        let mut c = if let Some(p_str) = preset_str {
            Self::preset(p_str)
        } else {
            Self::default()
        };

        // 2. Parse general fluid properties (supporting camelCase & snake_case Lua aliases)
        if let Some(v) = m.get("fluidCount")
            .or_else(|| m.get("particle_count"))
            .or_else(|| m.get("particleCount"))
            .or_else(|| m.get("fluid_count"))
            .or_else(|| m.get("count"))
            .or_else(|| m.get("particles"))
            .and_then(|v| v.as_f64())
        {
            c.particle_count = (v as usize).max(1);
        }

        // Capped at 20 — matches the Studio slider's max, and keeps old saved
        // redeems (from before that cap existed) from loading a much larger
        // particle size than is now allowed.
        if let Some(v) = m.get("fluidRadius")
            .or_else(|| m.get("fluid_radius"))
            .or_else(|| m.get("particleRadius"))
            .or_else(|| m.get("particle_radius"))
            .or_else(|| m.get("radius"))
            .or_else(|| m.get("h"))
            .and_then(|v| v.as_f64())
        {
            c.h = (v as f32).clamp(1.0, 20.0);
        }

        if let Some(v) = m.get("rest_density")
            .or_else(|| m.get("restDensity"))
            .or_else(|| m.get("density"))
            .and_then(|v| v.as_f64())
        {
            c.rest_density = v as f32;
        }

        if let Some(v) = m.get("pressure_k")
            .or_else(|| m.get("pressureK"))
            .or_else(|| m.get("stiffness"))
            .or_else(|| m.get("pressure"))
            .and_then(|v| v.as_f64())
        {
            c.pressure_k = v as f32;
        }

        if let Some(v) = m.get("fluidViscosity")
            .or_else(|| m.get("fluid_viscosity"))
            .or_else(|| m.get("viscosity"))
            .and_then(|v| v.as_f64())
        {
            c.viscosity = (v as f32).clamp(0.01, 3.0);
        }

        if let Some(v) = m.get("fluidGravity")
            .or_else(|| m.get("fluid_gravity"))
            .or_else(|| m.get("gravity"))
            .and_then(|v| v.as_f64())
        {
            c.gravity = v as f32;
        }

        if let Some(v) = m.get("fluidDuration")
            .or_else(|| m.get("fluid_duration"))
            .or_else(|| m.get("duration_ms"))
            .or_else(|| m.get("durationMs"))
            .or_else(|| m.get("duration"))
            .and_then(|v| v.as_f64())
        {
            // If specified in seconds (e.g. 3.5), convert to milliseconds
            c.duration_ms = if v > 0.0 && v < 50.0 { (v * 1000.0) as f32 } else { v as f32 };
        }

        if let Some(v) = m.get("dripThroughPercent")
            .or_else(|| m.get("drip_through_percent"))
            .or_else(|| m.get("dripThrough"))
            .or_else(|| m.get("drip_through"))
            .or_else(|| m.get("drip"))
            .and_then(|v| v.as_f64())
        {
            c.drip_through_percent = (v as f32).clamp(0.0, 100.0);
        }

        if let Some(v) = m.get("fluidWidth")
            .or_else(|| m.get("fluid_width"))
            .or_else(|| m.get("fluidSpread"))
            .or_else(|| m.get("fluid_spread"))
            .or_else(|| m.get("spread"))
            .or_else(|| m.get("generatorSpread"))
            .and_then(|v| v.as_f64())
        {
            c.fluid_width = v as f32;
        }

        if let Some(v) = m.get("coneSpread")
            .or_else(|| m.get("cone_spread"))
            .or_else(|| m.get("fluidConeSpread"))
            .or_else(|| m.get("fluid_cone_spread"))
            .or_else(|| m.get("beamSpread"))
            .and_then(|v| v.as_f64())
        {
            c.cone_spread = v as f32;
        }

        if let Some(v) = m.get("streamMode")
            .or_else(|| m.get("stream_mode"))
            .or_else(|| m.get("mode"))
            .and_then(|v| v.as_str())
        {
            c.stream_mode = v.to_string();
        }

        if let Some(v) = m.get("streamSpeed")
            .or_else(|| m.get("stream_speed"))
            .or_else(|| m.get("speed"))
            .and_then(|v| v.as_f64())
        {
            c.stream_speed = v as f32;
        }

        if let Some(v) = m.get("noDecay")
            .or_else(|| m.get("no_decay"))
            .and_then(|v| v.as_bool())
        {
            c.no_decay = v;
        }

        // 3. Metaball visual look properties
        if let Some(v) = m.get("waterTexturePath")
            .or_else(|| m.get("water_texture_path"))
            .or_else(|| m.get("texturePath"))
            .or_else(|| m.get("texture_path"))
            .or_else(|| m.get("texture"))
            .and_then(|v| v.as_str())
        {
            c.water_visual.texture_path = Some(v.to_string());
        }

        if let Some(v) = m.get("waterTextureScale")
            .or_else(|| m.get("water_texture_scale"))
            .or_else(|| m.get("textureScale"))
            .or_else(|| m.get("texture_scale"))
            .and_then(|v| v.as_f64())
        {
            c.water_visual.texture_scale = v as f32;
        }

        if let Some(v) = m.get("waterTextureScrollSpeed")
            .or_else(|| m.get("water_texture_scroll_speed"))
            .or_else(|| m.get("textureScrollSpeed"))
            .or_else(|| m.get("texture_scroll_speed"))
            .or_else(|| m.get("scrollSpeed"))
            .or_else(|| m.get("scroll_speed"))
            .and_then(|v| v.as_f64())
        {
            c.water_visual.texture_scroll_speed = v as f32;
        }

        if let Some(v) = m.get("waterRefractionEnabled")
            .or_else(|| m.get("water_refraction_enabled"))
            .or_else(|| m.get("refractionEnabled"))
            .or_else(|| m.get("refraction_enabled"))
            .or_else(|| m.get("refraction"))
            .and_then(|v| v.as_bool())
        {
            c.water_visual.refraction_enabled = v;
        }

        if let Some(v) = m.get("waterRefractionStrength")
            .or_else(|| m.get("water_refraction_strength"))
            .or_else(|| m.get("refractionStrength"))
            .or_else(|| m.get("refraction_strength"))
            .or_else(|| m.get("refractStrength"))
            .or_else(|| m.get("refract_strength"))
            .and_then(|v| v.as_f64())
        {
            c.water_visual.refraction_strength = v as f32;
        }

        if let Some(v) = m.get("waterRoundEdge")
            .or_else(|| m.get("water_round_edge"))
            .or_else(|| m.get("roundEdge"))
            .or_else(|| m.get("round_edge"))
            .and_then(|v| v.as_bool())
        {
            c.water_visual.round_edge = v;
        }

        if let Some(v) = m.get("waterMetaballSize")
            .or_else(|| m.get("water_metaball_size"))
            .or_else(|| m.get("metaballSize"))
            .or_else(|| m.get("metaball_size"))
            .or_else(|| m.get("blobSize"))
            .or_else(|| m.get("blob_size"))
            .and_then(|v| v.as_f64())
        {
            c.water_visual.metaball_size = v as f32;
        }

        // 4. Contact Sound properties
        if let Some(v) = m.get("contactSound")
            .or_else(|| m.get("contact_sound"))
            .or_else(|| m.get("contactSoundPath"))
            .or_else(|| m.get("contact_sound_path"))
            .or_else(|| m.get("collisionSound"))
            .or_else(|| m.get("collision_sound"))
            .or_else(|| m.get("collisionSoundPath"))
            .or_else(|| m.get("collision_sound_path"))
            .or_else(|| m.get("onContactSound"))
            .or_else(|| m.get("on_contact_sound"))
            .or_else(|| m.get("impactSound"))
            .or_else(|| m.get("impact_sound"))
            .and_then(|v| v.as_str())
        {
            if !v.is_empty() {
                c.contact_sound = Some(v.to_string());
            }
        }

        if let Some(v) = m.get("contactSoundCooldownMs")
            .or_else(|| m.get("contact_sound_cooldown_ms"))
            .or_else(|| m.get("contactSoundCooldown"))
            .or_else(|| m.get("contact_sound_cooldown"))
            .or_else(|| m.get("collisionSoundCooldown"))
            .or_else(|| m.get("collision_sound_cooldown"))
            .or_else(|| m.get("soundCooldown"))
            .or_else(|| m.get("sound_cooldown"))
            .or_else(|| m.get("cooldown"))
            .and_then(|v| v.as_f64())
        {
            c.contact_sound_cooldown_ms = (v as f32).max(20.0);
        }

        if let Some(v) = m.get("contactSoundFirstOnly")
            .or_else(|| m.get("contact_sound_first_only"))
            .or_else(|| m.get("firstContactOnly"))
            .or_else(|| m.get("first_contact_only"))
            .or_else(|| m.get("firstOnly"))
            .or_else(|| m.get("first_only"))
            .and_then(|v| v.as_bool())
        {
            c.contact_sound_first_only = v;
        }

        if let Some(v) = m.get("contactSoundOnGround")
            .or_else(|| m.get("contact_sound_on_ground"))
            .or_else(|| m.get("groundContactSound"))
            .or_else(|| m.get("ground_contact_sound"))
            .or_else(|| m.get("soundOnGround"))
            .or_else(|| m.get("sound_on_ground"))
            .and_then(|v| v.as_bool())
        {
            c.contact_sound_on_ground = v;
        }

        // 5. Encapsulated Material Block — a coarse-grained bundle snapshotted
        // whenever a preset was last applied (see RedeemConfig::apply_fluid_preset).
        // Now that there's just one fluid type with one live color picker, this
        // is stale historical data, not the user's current intent — it must be
        // applied BEFORE (so it's overridden by) the flat color fields below.
        if let Some(mat) = mat_obj {
            if let Some(bc) = mat.get("baseColor").and_then(|v| v.as_array()) {
                if bc.len() >= 3 {
                    c.color[0] = bc[0].as_f64().unwrap_or(0.0) as f32 / 255.0;
                    c.color[1] = bc[1].as_f64().unwrap_or(0.0) as f32 / 255.0;
                    c.color[2] = bc[2].as_f64().unwrap_or(0.0) as f32 / 255.0;
                }
                if bc.len() >= 4 {
                    c.color_a = bc[3].as_f64().unwrap_or(0.95) as f32;
                }
            }
            if let Some(ba) = mat.get("baseAlpha").and_then(|v| v.as_f64()) {
                c.color_a = ba as f32;
            }
            if let Some(sc) = mat.get("secondaryColor").and_then(|v| v.as_array()) {
                if sc.len() >= 3 {
                    c.color2[0] = sc[0].as_f64().unwrap_or(0.0) as f32 / 255.0;
                    c.color2[1] = sc[1].as_f64().unwrap_or(0.0) as f32 / 255.0;
                    c.color2[2] = sc[2].as_f64().unwrap_or(0.0) as f32 / 255.0;
                }
                if sc.len() >= 4 {
                    c.color2_a = sc[3].as_f64().unwrap_or(0.90) as f32;
                }
            }
            if let Some(sa) = mat.get("secondaryAlpha").and_then(|v| v.as_f64()) {
                c.color2_a = sa as f32;
            }
        }

        // 6. Direct color array or hex parsing (Lua tables: color = {0, 0.5, 1.0} or color = "#0088ff")
        if let Some(val) = m.get("color").or_else(|| m.get("baseColor")).or_else(|| m.get("base_color")) {
            if let Some((rgb, alpha_opt)) = parse_color_value(val) {
                c.color = rgb;
                if let Some(a) = alpha_opt {
                    c.color_a = a;
                }
            }
        }
        if let Some(val) = m.get("color2").or_else(|| m.get("secondaryColor")).or_else(|| m.get("secondary_color")).or_else(|| m.get("highlightColor")) {
            if let Some((rgb, alpha_opt)) = parse_color_value(val) {
                c.color2 = rgb;
                if let Some(a) = alpha_opt {
                    c.color2_a = a;
                }
            }
        }

        // 7. Flat color / channel overrides — supports 0..1 floats and 0..255 ints
        if let Some(v) = m.get("colorR").or_else(|| m.get("color_r")).and_then(|v| v.as_f64()) {
            c.color[0] = if v > 1.0 { (v as f32 / 255.0).clamp(0.0, 1.0) } else { v as f32 };
        }
        if let Some(v) = m.get("colorG").or_else(|| m.get("color_g")).and_then(|v| v.as_f64()) {
            c.color[1] = if v > 1.0 { (v as f32 / 255.0).clamp(0.0, 1.0) } else { v as f32 };
        }
        if let Some(v) = m.get("colorB").or_else(|| m.get("color_b")).and_then(|v| v.as_f64()) {
            c.color[2] = if v > 1.0 { (v as f32 / 255.0).clamp(0.0, 1.0) } else { v as f32 };
        }
        if let Some(v) = m.get("colorA")
            .or_else(|| m.get("color_a"))
            .or_else(|| m.get("alpha"))
            .or_else(|| m.get("baseAlpha"))
            .or_else(|| m.get("base_alpha"))
            .and_then(|v| v.as_f64())
        {
            c.color_a = if v > 1.0 { (v as f32 / 255.0).clamp(0.0, 1.0) } else { v as f32 };
        }
        if let Some(v) = m.get("color2R").or_else(|| m.get("color2_r")).and_then(|v| v.as_f64()) {
            c.color2[0] = if v > 1.0 { (v as f32 / 255.0).clamp(0.0, 1.0) } else { v as f32 };
        }
        if let Some(v) = m.get("color2G").or_else(|| m.get("color2_g")).and_then(|v| v.as_f64()) {
            c.color2[1] = if v > 1.0 { (v as f32 / 255.0).clamp(0.0, 1.0) } else { v as f32 };
        }
        if let Some(v) = m.get("color2B").or_else(|| m.get("color2_b")).and_then(|v| v.as_f64()) {
            c.color2[2] = if v > 1.0 { (v as f32 / 255.0).clamp(0.0, 1.0) } else { v as f32 };
        }
        if let Some(v) = m.get("color2A")
            .or_else(|| m.get("color2_a"))
            .or_else(|| m.get("alpha2"))
            .or_else(|| m.get("secondaryAlpha"))
            .or_else(|| m.get("secondary_alpha"))
            .and_then(|v| v.as_f64())
        {
            c.color2_a = if v > 1.0 { (v as f32 / 255.0).clamp(0.0, 1.0) } else { v as f32 };
        }

        c.shake = crate::physics::parse_shake_config(m);
        c.impact_reaction = crate::physics::ImpactReactionConfig::parse(m.get("impactReactions"));
        c.redeem_key = m.get("redeem_key")
            .or_else(|| m.get("redeemKey"))
            .or_else(|| m.get("name"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        c
    }

    /// Named presets
    pub fn preset(name: &str) -> Self {
        let mut c = Self::default();
        match name {
            "gold" | "metal" => {
                c.viscosity = 0.70;
                c.gravity = 0.90;
                c.color = [0.92, 0.72, 0.18];
                c.color_a = 0.98;
                c.color2 = [1.0, 0.95, 0.65];
                c.color2_a = 0.98;
            }
            "glass" => {
                c.viscosity = 0.50;
                c.gravity = 0.80;
                c.color = [0.25, 0.78, 0.95];
                c.color_a = 0.75;
                c.color2 = [0.60, 0.92, 1.0];
                c.color2_a = 0.85;
                c.water_visual.texture_path = Some(crate::get_theater_repo_dir().join("Trust/assets/textures/smooth_deformed_glass.png").to_string_lossy().to_string());
                c.water_visual.texture_scale = 180.0;
                c.water_visual.texture_scroll_speed = 18.0;
                c.water_visual.refraction_enabled = true;
                c.water_visual.refraction_strength = 350.0;
            }
            "chemical" | "acid" => {
                c.viscosity = 0.40;
                c.gravity = 0.85;
                c.color = [0.04, 0.90, 0.25];
                c.color_a = 0.92;
                c.color2 = [0.72, 1.0, 0.12];
                c.color2_a = 0.90;
            }
            "milk" => {
                c.viscosity = 0.65;
                c.gravity = 0.85;
                c.color = [0.96, 0.96, 0.94];
                c.color_a = 0.98;
                c.color2 = [1.0, 1.0, 1.0];
                c.color2_a = 0.98;
            }
            "slime" => {
                c.viscosity = 0.85;
                c.pressure_k = 80.0;
                c.gravity = 0.6;
                c.color = [0.12, 0.78, 0.15];
                c.color_a = 0.95;
                c.color2 = [0.47, 1.0, 0.24];
                c.color2_a = 0.90;
            }
            "lava" => {
                c.viscosity = 0.90;
                c.pressure_k = 150.0;
                c.gravity = 0.7;
                c.color = [1.0, 0.24, 0.0];
                c.color_a = 0.98;
                c.color2 = [1.0, 0.78, 0.08];
                c.color2_a = 0.95;
            }
            "sand" => {
                c.viscosity = 0.3;
                c.pressure_k = 400.0;
                c.gravity = 1.2;
                c.color = [0.75, 0.59, 0.35];
                c.color_a = 0.95;
                c.color2 = [0.90, 0.78, 0.55];
                c.color2_a = 0.90;
                c.fluid_width = 50.0;
            }
            "blood" => {
                c.viscosity = 0.75;
                c.pressure_k = 120.0;
                c.gravity = 0.8;
                c.color = [0.63, 0.0, 0.06];
                c.color_a = 0.95;
                c.color2 = [0.90, 0.08, 0.12];
                c.color2_a = 0.90;
            }
            _ => { // water defaults
                c.viscosity = 0.5;
                c.gravity = 0.8;
                c.color = [0.12, 0.55, 0.95];
                c.color_a = 0.90;
                c.color2 = [0.39, 0.82, 1.0];
                c.color2_a = 0.85;
            }
        }
        c
    }
}

// ---------------------------------------------------------------------------
// Particle
// ---------------------------------------------------------------------------
#[derive(Debug, Clone)]
pub struct SphParticle {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub size: f32,
    pub is_foam: bool,
    pub is_dripping: bool,
    pub has_collided: bool,
    /// 0 = unassigned, 1 = through, 2 = around
    pub drip_behavior: u8,
    pub wobble_speed: f32,
    pub decay: f32,
    pub life: f32,
}

// ---------------------------------------------------------------------------
// Fluid Impact Event (For triggering dynamic splash crowns and ripples)
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, Copy)]
pub struct FluidImpact {
    pub x: f32,
    pub y: f32,
    pub kind: u8, // 1 = Head/body impact crown, 2 = Ground settling ripple
    pub intensity: f32,
}

// ---------------------------------------------------------------------------
// Engine (Faithful native Rust port of WebGLWaterStream.js)
// ---------------------------------------------------------------------------
pub struct SphEngine {
    /// Unique instance ID across all active fluid engines — exported in the
    /// particle buffer so the renderer can isolate each redeem/stream into
    /// its own metaball field and draw pass without cross-contamination.
    pub id: u32,
    pub redeem_key: Option<String>,
    pub particles: Vec<SphParticle>,
    pub config: SphConfig,

    /// Canvas-space start position
    pub start_x: f32,
    pub start_y: f32,

    /// Canvas-space target center
    pub target_x: f32,
    pub target_y: f32,

    pub target_name: String,

    pub elapsed_ms: f32,
    pub emitted_count: usize,
    pub dead: bool,

    pub recent_impacts: Vec<FluidImpact>,

    /// Index into WATER_VISUAL_REGISTRY for this engine's own `water_visual`
    /// settings — baked into every exported particle (see `fill_render_buf`)
    /// so the renderer can resolve them back per particle group.
    visual_settings_idx: u32,

    rng: Rng,

    /// Debounce timer in milliseconds since contact sound was last played on this stream
    pub contact_sound_timer: f32,
    /// Whether contact sound has triggered at least once (for first-only mode)
    pub has_played_contact_sound: bool,
    /// Total particle impacts on target for this stream
    pub target_contacts: u32,
    /// Total particle impacts on canvas ground for this stream
    pub ground_contacts: u32,
}

impl SphEngine {
    pub fn new(
        id: u32,
        start_x: f32,
        start_y: f32,
        target_x: f32,
        target_y: f32,
        target_name: String,
        config: SphConfig,
        seed: u64,
    ) -> Self {
        let visual_settings_idx = register_water_visual_settings(config.water_visual.clone());
        let redeem_key = config.redeem_key.clone();
        Self {
            id,
            redeem_key,
            particles: Vec::with_capacity(config.particle_count + 32),
            config,
            start_x,
            start_y,
            target_x,
            target_y,
            target_name,
            elapsed_ms: 0.0,
            emitted_count: 0,
            dead: false,
            recent_impacts: Vec::with_capacity(32),
            visual_settings_idx,
            rng: Rng::new(seed),
            contact_sound_timer: 0.0,
            has_played_contact_sound: false,
            target_contacts: 0,
            ground_contacts: 0,
        }
    }

    pub fn drain_impacts(&mut self) -> Vec<FluidImpact> {
        std::mem::take(&mut self.recent_impacts)
    }

    // -----------------------------------------------------------------------
    // Emission (Exact WebGLWaterStream.js trajectory & gravity compensation)
    // -----------------------------------------------------------------------
    fn emit_particle(&mut self, frame_fraction: f32) {
        let mut px: f32;
        let mut py: f32;
        let pvx: f32;
        let mut pvy: f32;

        let gravity_val = self.config.gravity;
        let visc = self.config.viscosity.clamp(0.05, 2.0);

        let r = self.rng.next_f32();
        let r2 = self.rng.next_f32();
        let r3 = self.rng.next_f32();
        let r4 = self.rng.next_f32();
        let r5 = self.rng.next_f32();

        let nozzle_spread = self.config.fluid_width.max(0.0);
        let cone_spread = self.config.cone_spread.max(0.0);

        // Overall travel speed — separate from gravity (which only shapes the
        // arc's curve, not how long the trip takes) and viscosity (which only
        // drags free-flight/settled motion after emission). Without this,
        // "how fast the water actually moves" had no real control at all —
        // the grid-mode travel time was a hardcoded `dist / 22.0`, entirely
        // independent of every other slider, so lowering gravity expecting
        // slower water did nothing.
        let speed = self.config.stream_speed.max(0.05);

        if self.config.stream_mode == "top" {
            // Drop straight down directly from the avatar's X coordinate with user-configured spread
            px = self.target_x + (r - 0.5) * nozzle_spread;
            py = -30.0;
            pvx = (r3 - 0.5) * (cone_spread * 0.03);
            pvy = (8.0 + r4 * 2.0) * speed;
        } else {
            // Exact kinematic ballistic beam trajectory with independent nozzle spread and target cone spread
            let sx = self.start_x + (r3 - 0.5) * nozzle_spread;
            let sy = self.start_y + (r4 - 0.5) * (nozzle_spread * 0.5);
            let tx = self.target_x + (r - 0.5) * cone_spread;
            let ty = self.target_y + (r2 - 0.5) * (cone_spread * 0.4);
            let dx = tx - sx;
            let dy = ty - sy;
            let dist = (dx * dx + dy * dy).sqrt().max(1.0);
            // `time_frames` is the arc's SHAPE reference — distance only, not
            // speed — solved once for a fixed gravity `g` exactly like
            // before speed existed. Squeezing this by `speed` directly (the
            // earlier approach) shrank the flight duration but kept gravity's
            // influence sized for a *shorter* window, which mathematically
            // flattens the arc as speed increases (less time in the air needs
            // less upward launch to counter the same gravity). Instead, scale
            // the resulting velocity by `speed` (below) and scale gravity
            // itself by `speed²` for the actual simulation (see
            // `effective_gravity` and its use in `step`) — that's the
            // physically exact way to trace the *same shaped* path faster or
            // slower, the way scrubbing playback speed on a video doesn't
            // change what's in frame, just how fast you get there.
            let time_frames = (dist / 22.0).clamp(10.0, 60.0).round();
            let g = gravity_val;

            pvx = (dx / time_frames) * speed;
            pvy = ((dy - 0.5 * g * time_frames * (time_frames + 1.0)) / time_frames) * speed;
            px = sx;
            py = sy;
        }

        // Sub-frame physics interpolation — uses the same speed²-scaled
        // gravity as the main step loop (see `effective_gravity` there) so
        // this first partial frame is consistent with every frame after it.
        let dt = frame_fraction;
        let g = gravity_val * speed * speed;
        px = px + pvx * dt;
        py = py + pvy * dt + 0.5 * g * dt * dt;
        pvy = pvy + g * dt;

        let is_foam = r4 < 0.2;
        let (size, decay) = if is_foam {
            (self.config.h * (0.35 + r5 * 0.35), 0.008 + r3 * 0.012)
        } else {
            (self.config.h * (0.75 + r5 * 0.35), 0.005 + r3 * 0.006)
        };

        let particle_life = (1.0 - (decay * dt)).max(0.01);

        self.particles.push(SphParticle {
            x: px,
            y: py,
            vx: pvx,
            vy: pvy,
            size,
            is_foam,
            is_dripping: false,
            has_collided: false,
            drip_behavior: 0,
            wobble_speed: self.rng.next_f32(),
            decay,
            life: particle_life,
        });
    }

    // -----------------------------------------------------------------------
    // Step (Exact WebGLWaterStream.js update loop)
    // -----------------------------------------------------------------------
    pub fn step(
        &mut self,
        dt_ms: f32,
        canvas_w: f32,
        canvas_h: f32,
        hitboxes: &HashMap<String, Hitbox>,
        slots: &HashMap<usize, String>,
        masks: &HashMap<String, crate::tracker::CollisionMask>,
    ) {
        if self.dead {
            return;
        }

        self.elapsed_ms += dt_ms;
        self.contact_sound_timer = (self.contact_sound_timer - dt_ms).max(0.0);

        let visc: f32 = self.config.viscosity.clamp(0.05, 2.0);
        let gravity_val: f32 = self.config.gravity;
        let speed: f32 = self.config.stream_speed.max(0.05);
        // See emit_particle's comment: to trace the SAME shaped arc faster or
        // slower (not a flatter/steeper one), gravity during actual free-air
        // flight must scale by speed² to match the velocity scaled by speed
        // at emission — exact same reasoning as playback speed on a video.
        let effective_gravity: f32 = gravity_val * speed * speed;

        // `hitboxes`/`slots`/`masks` are already locked by the caller
        // (EffectsManager::step, itself called while the OBS tick/Studio
        // frame already holds these read guards) — re-locking them here via
        // `crate::state::get_state()` used to be a same-thread recursive
        // read. parking_lot's RwLock is writer-fair, so if a writer (e.g.
        // the Studio's own hitbox/mask rescan) queued in the gap between the
        // outer and inner lock, the inner read would block behind it while
        // the writer blocked behind the still-held outer read — a
        // self-deadlock that froze both OBS and the Studio window, and got
        // more likely the more simultaneous fluid streams (heavier per-frame
        // SPH work = wider window) were running. Use the passed-in
        // references instead of ever re-locking.
        let hb = crate::resolve_target_hitbox(&self.target_name, slots, hitboxes).map(|(_, v)| v);
        let target_mask = crate::resolve_target_mask(&self.target_name, slots, masks).map(|(_, v)| v);

        // 1. Sync real-time geometric target center from hitbox bounds BEFORE emission
        if let Some(h) = hb {
            if h.active && h.world_center_x > 0.0 && h.world_center_y > 0.0 {
                self.target_x = h.world_center_x;
                self.target_y = h.world_center_y;
            } else if h.active && h.world_width > 0.0 {
                self.target_x = h.world_x + h.world_width * 0.5;
                self.target_y = h.world_y + h.world_height * 0.5;
            }
        }

        // 2. Emission matching JS emitRateMs = fluidCount / fluidDuration
        if self.elapsed_ms < self.config.duration_ms {
            let emit_rate = self.config.particle_count as f32 / self.config.duration_ms.max(1.0);
            let target_emitted = (self.elapsed_ms * emit_rate).min(self.config.particle_count as f32) as usize;
            let to_emit = target_emitted.saturating_sub(self.emitted_count);
            for i in 0..to_emit {
                let frac = (i as f32 + 0.5) / (to_emit as f32).max(1.0);
                self.emit_particle(frac);
                self.emitted_count += 1;
            }
        }

        // 3. SPH Inter-Particle Hydrodynamics (Particle-Particle Pressure Repulsion & Viscous Drag)
        let n_particles = self.particles.len();
        let mut sph_fx = vec![0.0f32; n_particles];
        let mut sph_fy = vec![0.0f32; n_particles];

        if n_particles > 1 {
            let h_radius = (self.config.h * 1.8).clamp(12.0, 60.0);
            let h2 = h_radius * h_radius;
            let cell_size = h_radius;
            let inv_cell = 1.0 / cell_size;

            let mut grid: HashMap<(i32, i32), Vec<usize>> = HashMap::with_capacity(n_particles);
            for (i, p) in self.particles.iter().enumerate() {
                let cx = (p.x * inv_cell).floor() as i32;
                let cy = (p.y * inv_cell).floor() as i32;
                grid.entry((cx, cy)).or_default().push(i);
            }

            let mut densities = vec![1.0f32; n_particles];
            let mut pressures = vec![0.0f32; n_particles];

            // SPH Density Pass
            for (i, p) in self.particles.iter().enumerate() {
                let cx = (p.x * inv_cell).floor() as i32;
                let cy = (p.y * inv_cell).floor() as i32;

                for dx in -1..=1 {
                    for dy in -1..=1 {
                        if let Some(neighbors) = grid.get(&(cx + dx, cy + dy)) {
                            for &j in neighbors {
                                if i >= j { continue; }
                                let other = &self.particles[j];
                                let rx = p.x - other.x;
                                let ry = p.y - other.y;
                                let r2 = rx * rx + ry * ry;
                                if r2 < h2 && r2 > 0.001 {
                                    let r = r2.sqrt();
                                    let q = 1.0 - (r / h_radius);
                                    let w = q * q * q;
                                    densities[i] += w;
                                    densities[j] += w;
                                }
                            }
                        }
                    }
                }
            }

            // SPH Pressure Pass (Tait's Equation of State for Water)
            let rest_density = self.config.rest_density.max(0.5);
            let k_press = (self.config.pressure_k * 0.0015).clamp(0.01, 0.5);
            for i in 0..n_particles {
                let d = densities[i];
                if d > rest_density {
                    pressures[i] = (d - rest_density) * k_press;
                }
            }

            // SPH Pressure Gradient & Viscous Momentum Sharing Pass
            for (i, p) in self.particles.iter().enumerate() {
                let cx = (p.x * inv_cell).floor() as i32;
                let cy = (p.y * inv_cell).floor() as i32;

                for dx in -1..=1 {
                    for dy in -1..=1 {
                        if let Some(neighbors) = grid.get(&(cx + dx, cy + dy)) {
                            for &j in neighbors {
                                if i >= j { continue; }
                                let other = &self.particles[j];
                                let rx = p.x - other.x;
                                let ry = p.y - other.y;
                                let r2 = rx * rx + ry * ry;
                                if r2 < h2 && r2 > 0.001 {
                                    let r = r2.sqrt();
                                    let q = 1.0 - (r / h_radius);
                                    // SPH Repulsion ONLY between particles that have BOTH hit the target/surface
                                    let both_collided = p.has_collided && other.has_collided;
                                    let p_term = if both_collided {
                                        (pressures[i] + pressures[j]) * 0.5 * q
                                    } else {
                                        0.0 // No mid-air explosion or premature stream scattering
                                    };
                                    let nx = rx / r;
                                    let ny = ry / r;

                                    // Hydrodynamic repulsion force (pushes clustered water particles apart subtly)
                                    let fx = nx * p_term;
                                    let fy = ny * p_term;

                                    // Viscous smoothing (fluid cohesive shear drag)
                                    let dvx = (other.vx - p.vx) * (visc * 0.10 * q);
                                    let dvy = (other.vy - p.vy) * (visc * 0.10 * q);

                                    sph_fx[i] += fx + dvx;
                                    sph_fy[i] += fy + dvy;
                                    sph_fx[j] -= fx - dvx;
                                    sph_fy[j] -= fy - dvy;
                                }
                            }
                        }
                    }
                }
            }
        }

        for (i, p) in self.particles.iter_mut().enumerate() {
            // Apply controlled SPH inter-particle spreading forces ONLY to collided surface particles
            if n_particles > 1 && p.has_collided {
                p.vx += sph_fx[i].clamp(-0.25, 0.25);
                p.vy += sph_fy[i].clamp(-0.25, 0.25);
            }

            let mut on_target = false;
            if let Some(h) = hb {
                if h.active {
                    if let Some(m) = target_mask {
                        if p.x >= h.world_x && p.x <= h.world_x + h.world_width
                            && p.y >= h.world_y && p.y <= h.world_y + h.world_height
                        {
                            on_target = m.is_solid_world(p.x, p.y);
                        }
                    } else {
                        // Without alpha mask: hit when entering vicinity of target center
                        let c_x = if h.world_center_x > 0.0 { h.world_center_x } else { h.world_x + h.world_width * 0.5 };
                        let c_y = if h.world_center_y > 0.0 { h.world_center_y } else { h.world_y + h.world_height * 0.5 };
                        let half_w = (h.world_width * 0.35).max(20.0);
                        let half_h = (h.world_height * 0.35).max(20.0);
                        if (p.x - c_x).abs() <= half_w && (p.y - c_y).abs() <= half_h {
                            on_target = true;
                        }
                    }
                }
            }

            if on_target {
                p.has_collided = true;
                if !p.is_dripping {
                    p.is_dripping = true;
                    self.target_contacts += 1;

                    // Play contact sound if configured and debounce timer elapsed
                    if let Some(ref snd) = self.config.contact_sound {
                        if !snd.is_empty()
                            && (!self.config.contact_sound_first_only || !self.has_played_contact_sound)
                            && self.contact_sound_timer <= 0.0
                        {
                            crate::play_sound_str(snd);
                            self.has_played_contact_sound = true;
                            self.contact_sound_timer = self.config.contact_sound_cooldown_ms.max(20.0);
                        }
                    }

                    let r = self.rng.next_f32();
                    // 50% slide around contours (behavior 2), 50% slow drip through (behavior 1)
                    p.drip_behavior = if r * 100.0 < self.config.drip_through_percent { 1 } else { 2 };

                    // Subtle, natural lateral splash on impact
                    let impact_speed = (p.vx * p.vx + p.vy * p.vy).sqrt();
                    let center_x = if let Some(h) = hb {
                        if h.world_center_x > 0.0 { h.world_center_x } else { h.world_x + h.world_width * 0.5 }
                    } else {
                        self.target_x
                    };
                    let splash_dir = if p.x < center_x { -1.0 } else { 1.0 };
                    let splash_force = (self.rng.next_f32() * 0.30 + 0.15) * (impact_speed * 0.08).clamp(0.2, 1.0);
                    p.vx = splash_dir * splash_force;
                    p.vy *= 0.20;

                    // Register impact event for anime splash burst
                    let intensity = (impact_speed / 12.0).clamp(0.6, 2.0);
                    if self.recent_impacts.len() < 32 {
                        self.recent_impacts.push(FluidImpact {
                            x: p.x,
                            y: p.y,
                            kind: 1, // Impact crown burst
                            intensity,
                        });
                    }
                }
            }

            if p.is_dripping {
                let center_x = if let Some(h) = hb {
                    if h.world_center_x > 0.0 { h.world_center_x } else { h.world_x + h.world_width * 0.5 }
                } else {
                    self.target_x
                };

                let side_dir = if p.x < center_x { -1.0 } else { 1.0 };

                // Surface adhesion damping: keeps water hugging and dripping along target contours
                p.vx *= 0.82;

                if p.drip_behavior == 2 {
                    // Flow running around the outer hitbox contours (hug perimeter, gentle slide)
                    p.vy = (0.8 + p.wobble_speed * 0.5 + 0.3 * gravity_val) * visc;
                    p.vx += side_dir * (0.40 * visc);
                    p.x += p.vx;
                    p.y += p.vy;
                } else {
                    // Surface trickle flowing down the front of the body
                    p.vy = (0.5 + p.wobble_speed * 0.4 + 0.2 * gravity_val) * visc;
                    p.vx += (p.wobble_speed - 0.5) * 0.15 * visc;
                    p.x += p.vx;
                    p.y += p.vy;
                }

                if !self.config.no_decay {
                    // Controlled decay so drips trail down cleanly without hanging indefinitely
                    p.life -= p.decay * 0.60;

                    // Gradually shrink to simulate leaving water behind
                    p.size *= 0.994;
                }

                // Check if still within contact of the solid target
                let still_on = if let Some(h) = hb {
                    if h.active && p.y >= h.world_y && p.y <= h.world_y + h.world_height {
                        if let Some(m) = target_mask {
                            m.is_solid_world(p.x, p.y)
                                || m.is_solid_world(p.x - 4.0, p.y)
                                || m.is_solid_world(p.x + 4.0, p.y)
                        } else {
                            p.x >= h.world_x - 5.0 && p.x <= h.world_x + h.world_width + 5.0
                        }
                    } else {
                        false
                    }
                } else {
                    false
                };

                if !still_on {
                    p.is_dripping = false;
                    // Regain full downward falling velocity when detaching into free air
                    p.vy = p.vy.max(7.0) + effective_gravity;
                }
            } else {
                // Free air flight physics: accelerate under full gravity along exact ballistic arc.
                p.vy += effective_gravity;
                p.x += p.vx;
                p.y += p.vy;

                // "Fill a container" only reaches the settled/dripping decay
                // gate above if a particle actually lands on the tracked
                // target's *solid mask* — for a small or oddly-shaped
                // recipient, particles that fall near but not exactly onto
                // it never leave this free-air branch, so it decayed away
                // (even faster than the settled rate) regardless of the
                // checkbox. Gate this one too.
                if !self.config.no_decay {
                    p.life -= p.decay * 1.4;
                }
            }
        }

        // Check for ground impact ripples & ground contact sound
        if canvas_h > 100.0 {
            let mut ground_hit = false;
            for p in &self.particles {
                if p.y >= canvas_h - 50.0 && p.vy > 3.0 {
                    ground_hit = true;
                    if self.recent_impacts.len() < 48 {
                        let r = self.rng.next_f32();
                        if r < 0.20 {
                            self.recent_impacts.push(FluidImpact {
                                x: p.x,
                                y: canvas_h - 15.0,
                                kind: 2, // Settling ripple
                                intensity: (p.size / self.config.h.max(1.0)).clamp(0.5, 1.5),
                            });
                        }
                    }
                }
            }
            if ground_hit {
                self.ground_contacts += 1;
                if self.config.contact_sound_on_ground {
                    if let Some(ref snd) = self.config.contact_sound {
                        if !snd.is_empty()
                            && (!self.config.contact_sound_first_only || !self.has_played_contact_sound)
                            && self.contact_sound_timer <= 0.0
                        {
                            crate::play_sound_str(snd);
                            self.has_played_contact_sound = true;
                            self.contact_sound_timer = self.config.contact_sound_cooldown_ms.max(20.0);
                        }
                    }
                }
            }
        }

        // 3. Boundary clean
        self.particles.retain(|p| {
            p.life > 0.0 && p.size >= 1.0 && p.x > -150.0 && p.x < canvas_w + 150.0 && p.y < canvas_h + 150.0
        });

        if self.elapsed_ms >= self.config.duration_ms && self.particles.is_empty() {
            self.dead = true;
        }
    }

    // -----------------------------------------------------------------------
    // Export: [x, y, size, r, g, b, a, vx, vy, visual_settings_idx, engine_id] (11 f32/particle)
    // -----------------------------------------------------------------------
    pub fn fill_render_buf(&self, buf: &mut Vec<f32>) {
        for p in &self.particles {
            buf.push(p.x);
            buf.push(p.y);
            buf.push(p.size * p.life.sqrt().max(0.0));
            buf.push(self.config.color[0]);
            buf.push(self.config.color[1]);
            buf.push(self.config.color[2]);
            buf.push(p.life.min(1.0) * self.config.color_a);
            buf.push(p.vx);
            buf.push(p.vy);
            buf.push(self.visual_settings_idx as f32);
            buf.push(self.id as f32);
        }
    }

    pub fn particle_count(&self) -> usize {
        self.particles.len()
    }
}

// ---------------------------------------------------------------------------
// Unit Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_hitboxes() -> HashMap<String, Hitbox> {
        HashMap::new()
    }

    fn dummy_slots() -> HashMap<usize, String> {
        HashMap::new()
    }

    fn dummy_masks() -> HashMap<String, crate::tracker::CollisionMask> {
        HashMap::new()
    }

    #[test]
    fn test_sph_spawns_particles() {
        let config = SphConfig::default();
        let mut eng = SphEngine::new(1, 100.0, 100.0, 500.0, 500.0, "avatar".into(), config, 42);
        eng.step(200.0, 1920.0, 1080.0, &dummy_hitboxes(), &dummy_slots(), &dummy_masks());
        assert!(eng.particle_count() > 0, "Should have emitted particles");
    }

    #[test]
    fn test_sph_particles_move() {
        let config = SphConfig::default();
        let mut eng = SphEngine::new(2, 100.0, 50.0, 500.0, 500.0, "avatar".into(), config, 99);
        eng.step(100.0, 1920.0, 1080.0, &dummy_hitboxes(), &dummy_slots(), &dummy_masks());
        let y0 = eng.particles.first().map(|p| p.y).unwrap_or(0.0);
        eng.step(100.0, 1920.0, 1080.0, &dummy_hitboxes(), &dummy_slots(), &dummy_masks());
        let y1 = eng.particles.first().map(|p| p.y).unwrap_or(0.0);
        // Particles should have moved
        assert!((y1 - y0).abs() > 0.0, "Particles should move");
    }

    #[test]
    fn test_sph_dies_when_done() {
        let mut config = SphConfig::default();
        config.duration_ms = 50.0;
        config.particle_count = 2;
        let mut eng = SphEngine::new(3, 100.0, 100.0, 500.0, 500.0, "avatar".into(), config, 7);
        for _ in 0..500 {
            if eng.dead { break; }
            eng.step(16.0, 1920.0, 1080.0, &dummy_hitboxes(), &dummy_slots(), &dummy_masks());
        }
        assert!(eng.dead, "Engine should be dead after duration expires");
    }

    #[test]
    fn test_fill_render_buf_correct_stride() {
        let config = SphConfig::default();
        let mut eng = SphEngine::new(4, 100.0, 100.0, 500.0, 500.0, "avatar".into(), config, 1);
        eng.step(200.0, 1920.0, 1080.0, &dummy_hitboxes(), &dummy_slots(), &dummy_masks());
        let mut buf = Vec::new();
        eng.fill_render_buf(&mut buf);
        let count = eng.particle_count();
        assert_eq!(buf.len(), count * 11, "Buffer stride must be 11 floats per particle");
    }

    #[test]
    fn test_sph_pressure_particles_spread_from_each_other() {
        let mut config = SphConfig::default();
        config.pressure_k = 300.0;
        let mut eng = SphEngine::new(5, 100.0, 100.0, 500.0, 500.0, "avatar".into(), config, 1);
        // Spawn two collided particles very close together at identical velocities
        eng.particles.push(SphParticle {
            x: 200.0,
            y: 200.0,
            vx: 0.0,
            vy: 0.0,
            size: 20.0,
            is_foam: false,
            is_dripping: true,
            has_collided: true,
            drip_behavior: 1,
            wobble_speed: 0.5,
            decay: 0.001,
            life: 1.0,
        });
        eng.particles.push(SphParticle {
            x: 204.0,
            y: 200.0,
            vx: 0.0,
            vy: 0.0,
            size: 20.0,
            is_foam: false,
            is_dripping: true,
            has_collided: true,
            drip_behavior: 1,
            wobble_speed: 0.5,
            decay: 0.001,
            life: 1.0,
        });

        let dist_initial = (eng.particles[0].x - eng.particles[1].x).abs();
        eng.step(16.0, 1920.0, 1080.0, &dummy_hitboxes(), &dummy_slots(), &dummy_masks());
        let dist_after = (eng.particles[0].x - eng.particles[1].x).abs();
        assert!(dist_after > dist_initial, "SPH pressure repulsion should push collided neighboring particles apart! Initial: {}, After: {}", dist_initial, dist_after);
    }

    #[test]
    fn test_glass_preset_has_deformed_glass_texture_and_valid_asset() {
        let cfg = SphConfig::preset("glass");
        assert_eq!(cfg.color, [0.25, 0.78, 0.95]);
        assert_eq!(cfg.water_visual.texture_scale, 180.0);
        assert_eq!(cfg.water_visual.texture_scroll_speed, 18.0);
        assert!(cfg.water_visual.refraction_enabled);
        assert_eq!(cfg.water_visual.refraction_strength, 350.0);
        let path_str = cfg.water_visual.texture_path.expect("Glass preset should configure texture path");
        assert!(path_str.ends_with("deformed_glass.png"));
        assert!(std::path::Path::new(&path_str).exists(), "Deformed glass texture must exist at configured path");
    }

    #[test]
    fn test_deformed_glass_texture_assets_and_refraction_independence() {
        let smooth_path = "/home/mortygotvr/repo/theater-scene/Trust/assets/textures/smooth_deformed_glass.png";
        let rippled_path = "/home/mortygotvr/repo/theater-scene/Trust/assets/textures/deformed_glass.png";
        assert!(std::path::Path::new(smooth_path).exists(), "Smooth deformed glass texture must exist");
        assert!(std::path::Path::new(rippled_path).exists(), "Rippled deformed glass texture must exist");

        let img_smooth = image::open(smooth_path).expect("Smooth deformed glass must be valid image");
        let img_rippled = image::open(rippled_path).expect("Rippled deformed glass must be valid image");
        assert_eq!(img_smooth.width(), 1024);
        assert_eq!(img_smooth.height(), 1024);
        assert_eq!(img_rippled.width(), 1024);
        assert_eq!(img_rippled.height(), 1024);

        let mut visual = WaterVisualSettings::default();
        visual.refraction_enabled = false;
        visual.texture_path = Some(smooth_path.to_string());
        assert!(!visual.refraction_enabled);
        assert!(visual.texture_path.is_some());
    }

    #[test]
    fn test_lua_sph_config_from_json_map_aliases() {
        let mut m = HashMap::new();
        m.insert("count".to_string(), serde_json::json!(85));
        m.insert("radius".to_string(), serde_json::json!(14.5));
        m.insert("viscosity".to_string(), serde_json::json!(0.75));
        m.insert("gravity".to_string(), serde_json::json!(-0.2));
        m.insert("duration".to_string(), serde_json::json!(4.5)); // seconds -> 4500 ms
        m.insert("spread".to_string(), serde_json::json!(45.0));
        m.insert("cone_spread".to_string(), serde_json::json!(50.0));
        m.insert("speed".to_string(), serde_json::json!(1.4));
        m.insert("no_decay".to_string(), serde_json::json!(true));
        m.insert("metaball_size".to_string(), serde_json::json!(11.2));
        m.insert("round_edge".to_string(), serde_json::json!(true));
        m.insert("refraction".to_string(), serde_json::json!(false));
        m.insert("refraction_strength".to_string(), serde_json::json!(250.0));
        m.insert("texture_path".to_string(), serde_json::json!("textures/bubbles.png"));
        m.insert("texture_scale".to_string(), serde_json::json!(140.0));
        m.insert("texture_scroll_speed".to_string(), serde_json::json!(25.0));
        m.insert("contact_sound".to_string(), serde_json::json!("sounds/splash.wav"));
        m.insert("contact_sound_cooldown_ms".to_string(), serde_json::json!(160.0));
        m.insert("contact_sound_first_only".to_string(), serde_json::json!(true));
        m.insert("color".to_string(), serde_json::json!("#1188ff"));
        m.insert("color2".to_string(), serde_json::json!([200, 240, 255, 230]));
        m.insert("redeem_key".to_string(), serde_json::json!("custom_water_spell"));

        let cfg = SphConfig::from_json_map(&m);
        assert_eq!(cfg.particle_count, 85);
        assert!((cfg.h - 14.5).abs() < 1e-3);
        assert!((cfg.viscosity - 0.75).abs() < 1e-3);
        assert!((cfg.gravity - (-0.2)).abs() < 1e-3);
        assert!((cfg.duration_ms - 4500.0).abs() < 1e-3);
        assert!((cfg.fluid_width - 45.0).abs() < 1e-3);
        assert!((cfg.cone_spread - 50.0).abs() < 1e-3);
        assert!((cfg.stream_speed - 1.4).abs() < 1e-3);
        assert!(cfg.no_decay);
        assert!((cfg.water_visual.metaball_size - 11.2).abs() < 1e-3);
        assert!(cfg.water_visual.round_edge);
        assert!(!cfg.water_visual.refraction_enabled);
        assert!((cfg.water_visual.refraction_strength - 250.0).abs() < 1e-3);
        assert_eq!(cfg.water_visual.texture_path.as_deref(), Some("textures/bubbles.png"));
        assert!((cfg.water_visual.texture_scale - 140.0).abs() < 1e-3);
        assert!((cfg.water_visual.texture_scroll_speed - 25.0).abs() < 1e-3);
        assert_eq!(cfg.contact_sound.as_deref(), Some("sounds/splash.wav"));
        assert!((cfg.contact_sound_cooldown_ms - 160.0).abs() < 1e-3);
        assert!(cfg.contact_sound_first_only);
        assert!((cfg.color[0] - 0x11 as f32 / 255.0).abs() < 1e-3);
        assert!((cfg.color[1] - 0x88 as f32 / 255.0).abs() < 1e-3);
        assert!((cfg.color[2] - 1.0).abs() < 1e-3);
        assert!((cfg.color2[0] - 200.0 / 255.0).abs() < 1e-3);
        assert_eq!(cfg.redeem_key.as_deref(), Some("custom_water_spell"));
    }

    #[test]
    fn test_sph_engine_contact_sound_and_cooldown_isolation() {
        let mut cfg1 = SphConfig::default();
        cfg1.contact_sound = Some("sound1.wav".to_string());
        cfg1.contact_sound_cooldown_ms = 100.0;

        let mut cfg2 = SphConfig::default();
        cfg2.contact_sound = Some("sound2.wav".to_string());
        cfg2.contact_sound_cooldown_ms = 150.0;

        let mut eng1 = SphEngine::new(1, 0.0, 0.0, 500.0, 500.0, "avatar".into(), cfg1, 123);
        let mut eng2 = SphEngine::new(2, 0.0, 0.0, 500.0, 500.0, "avatar".into(), cfg2, 456);

        // Add a particle that immediately collides with target
        let mut hb_map = HashMap::new();
        hb_map.insert("avatar".to_string(), Hitbox {
            world_x: 400.0,
            world_y: 400.0,
            world_width: 200.0,
            world_height: 200.0,
            world_center_x: 500.0,
            world_center_y: 500.0,
            active: true,
            ..Hitbox::default()
        });

        eng1.particles.push(SphParticle {
            x: 500.0, y: 500.0, vx: 0.0, vy: 5.0,
            size: 10.0, is_foam: false, is_dripping: false, has_collided: false,
            drip_behavior: 1, wobble_speed: 0.5, decay: 0.01, life: 1.0,
        });

        // Step 1 frame (16ms)
        eng1.step(16.0, 1920.0, 1080.0, &hb_map, &dummy_slots(), &dummy_masks());
        assert_eq!(eng1.target_contacts, 1);
        assert!(eng1.has_played_contact_sound);
        assert!(eng1.contact_sound_timer > 80.0);

        // eng2 must remain untouched and isolated!
        assert_eq!(eng2.target_contacts, 0);
        assert!(!eng2.has_played_contact_sound);
        assert_eq!(eng2.contact_sound_timer, 0.0);
    }
}
