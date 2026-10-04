use std::collections::HashMap;
use std::f32::consts::PI;
use std::time::{SystemTime, UNIX_EPOCH};
use serde::{Deserialize, Serialize};
use crate::state::get_state;

fn get_time_ns() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64
}

fn rand_f32() -> f32 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    // simple fast xorshift on nanos
    let mut x = nanos ^ 0x9E3779B9;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    (x % 10000) as f32 / 10000.0
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionType {
    Throwable = 0,
    ThrowFront = 1,
    Swirl = 2,
    Drop = 3,
    Rain = 4,
}

impl From<&str> for ActionType {
    fn from(s: &str) -> Self {
        let lower = s.to_lowercase();
        if lower.contains("front") {
            ActionType::ThrowFront
        } else if lower.contains("swirl") || lower.contains("spiral") {
            ActionType::Swirl
        } else if lower.contains("drop") {
            ActionType::Drop
        } else if lower.contains("rain") || lower.contains("shower") {
            ActionType::Rain
        } else {
            ActionType::Throwable
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollisionType {
    Bounce = 0,
    Stick = 1,
    Squish = 2,
    PixelShatter = 3,
    Pass = 4,
}

impl From<&str> for CollisionType {
    fn from(s: &str) -> Self {
        let lower = s.to_lowercase();
        if lower.contains("squish") || lower.contains("splat") {
            CollisionType::Squish
        } else if lower.contains("stick") {
            CollisionType::Stick
        } else if lower.contains("shatter") {
            CollisionType::PixelShatter
        } else if lower.contains("pass") {
            CollisionType::Pass
        } else {
            CollisionType::Bounce
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThrowableState {
    Flying = 0,
    Stuck = 1,
    Dead = 2,
}

/// When a redeem's Screen Shake should actually fire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ShakeTiming {
    /// Throwables: shake when the object physically hits its target.
    Impact,
    /// Any redeem: shake immediately when the redeem is triggered/spawned —
    /// the only option that makes sense for effects with no discrete "contact"
    /// moment (fluid streams, fire bursts, lightning strikes).
    Start,
}

impl Default for ShakeTiming {
    fn default() -> Self {
        ShakeTiming::Impact
    }
}

fn default_shake_timing() -> ShakeTiming {
    ShakeTiming::Impact
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShakeConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_shake_duration")]
    pub duration: u32,
    #[serde(default = "default_shake_intensity")]
    pub intensity: f32,
    #[serde(default = "default_shake_timing")]
    pub timing: ShakeTiming,
}

fn default_shake_duration() -> u32 { 200 }
fn default_shake_intensity() -> f32 { 10.0 }

impl Default for ShakeConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            duration: 200,
            intensity: 10.0,
            timing: ShakeTiming::Impact,
        }
    }
}

/// Shared helper: pull a `shakeOnImpact` block out of any redeem's raw JSON
/// map (used by fluid/fire/lightning configs alongside the throwable's own
/// serde-derived parsing on `SpawnPackage`).
pub fn parse_shake_config(m: &std::collections::HashMap<String, serde_json::Value>) -> ShakeConfig {
    m.get("shakeOnImpact")
        .and_then(|v| serde_json::from_value::<ShakeConfig>(v.clone()).ok())
        .unwrap_or_default()
}

/// Which single avatar reaction (if any) fires when a throwable connects.
/// The redeem editor only ever enables one of bounce/pixel_drip/jolt at a
/// time (see `RedeemConfig` in studio_app.rs), so this resolves to a single
/// kind + generic amount rather than tracking all three independently.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ImpactReactionKind {
    #[default]
    None,
    /// Squash-and-stretch ripple on the avatar itself.
    Bounce,
    /// Droplets falling off the avatar where it was hit.
    PixelDrip,
    /// Brief positional glitch + white flash on the avatar.
    Jolt,
    /// A user-loaded Lua Effect filter script (see `filters/*.lua`) — the same
    /// kind of per-pixel reaction shader as Bounce/PixelDrip, just with its
    /// shader source and tunable uniforms read from that script instead of a
    /// hardcoded `.effect` file. The u64 is a hash of the script's path,
    /// resolved back to a real path via `custom_reaction_script_path`.
    Custom(u64),
}

#[derive(Debug, Clone, Default)]
pub struct ImpactReactionConfig {
    pub kind: ImpactReactionKind,
    pub duration_ms: u32,
    /// Squish amount (Bounce), droplet viscosity (PixelDrip), or intensity (Jolt).
    pub amount: f32,
    /// "impact" (throwables only: fires on contact) or "start" (fires immediately
    /// on trigger — the only option that does anything for water/fire/lightning,
    /// which never reach `trigger_impact_internal`). Same semantics as `ShakeTiming`.
    pub timing: ShakeTiming,
    /// User-configured overrides for a `Custom` script's own discovered
    /// uniforms (e.g. `squish_amount`) — unused for the built-in kinds.
    pub custom_params: Vec<(String, f32)>,
}

impl ImpactReactionConfig {
    /// Parse the `impactReactions` object:
    /// `{ bounce: {enabled, squish_amount, duration_ms},
    ///    pixel_drip: {enabled, viscosity, duration_ms},
    ///    jolt: {enabled, intensity, duration_ms} }`.
    /// Only the first enabled entry (bounce, then pixel_drip, then jolt) applies.
    pub fn parse(v: Option<&serde_json::Value>) -> Self {
        let Some(v) = v else { return Self::default(); };
        let is_enabled = |obj: &serde_json::Value| obj.get("enabled").and_then(|e| e.as_bool()).unwrap_or(false);
        let parse_timing = |obj: &serde_json::Value| {
            obj.get("timing")
                .and_then(|t| serde_json::from_value::<ShakeTiming>(t.clone()).ok())
                .unwrap_or_default()
        };

        if let Some(c) = v.get("custom_script") {
            if is_enabled(c) {
                if let Some(path) = c.get("path").and_then(|p| p.as_str()) {
                    let custom_params = c.get("params")
                        .and_then(|p| p.as_object())
                        .map(|obj| obj.iter()
                            .filter_map(|(k, v)| v.as_f64().map(|f| (k.clone(), f as f32)))
                            .collect())
                        .unwrap_or_default();
                    // A path saved on another machine still finds the script here
                    let path = crate::studio_app::resolve_script_path(path);
                    return Self {
                        kind: ImpactReactionKind::Custom(register_custom_reaction_script_path(&path)),
                        // User-configurable "how long we keep the shader applied" —
                        // defaults to a generous window since we can't know how long
                        // a given script's own animation needs to finish.
                        duration_ms: c.get("duration_ms").and_then(|d| d.as_u64()).unwrap_or(4000) as u32,
                        amount: 0.0,
                        timing: parse_timing(c),
                        custom_params,
                    };
                }
            }
        }
        if let Some(b) = v.get("bounce") {
            if is_enabled(b) {
                return Self {
                    kind: ImpactReactionKind::Bounce,
                    duration_ms: b.get("duration_ms").and_then(|d| d.as_u64()).unwrap_or(500) as u32,
                    amount: b.get("squish_amount").and_then(|a| a.as_f64()).unwrap_or(0.4) as f32,
                    timing: parse_timing(b),
                    custom_params: Vec::new(),
                };
            }
        }
        if let Some(d) = v.get("pixel_drip").or_else(|| v.get("pixelDrip")) {
            if is_enabled(d) {
                return Self {
                    kind: ImpactReactionKind::PixelDrip,
                    duration_ms: d.get("duration_ms").and_then(|x| x.as_u64()).unwrap_or(2500) as u32,
                    amount: d.get("viscosity").and_then(|x| x.as_f64()).unwrap_or(0.6) as f32,
                    timing: parse_timing(d),
                    custom_params: Vec::new(),
                };
            }
        }
        if let Some(j) = v.get("jolt") {
            if is_enabled(j) {
                return Self {
                    kind: ImpactReactionKind::Jolt,
                    duration_ms: j.get("duration_ms").and_then(|x| x.as_u64()).unwrap_or(400) as u32,
                    amount: j.get("intensity").and_then(|x| x.as_f64()).unwrap_or(0.8) as f32,
                    timing: parse_timing(j),
                    custom_params: Vec::new(),
                };
            }
        }
        Self::default()
    }
}

/// Maps a `Custom(u64)` reaction's hash back to the real script path it was
/// registered from. Populated by `register_custom_reaction_script_path`,
/// read by the render side (obs_source.rs) when it needs the actual file.
static CUSTOM_REACTION_SCRIPT_PATHS: std::sync::LazyLock<parking_lot::Mutex<std::collections::HashMap<u64, String>>> =
    std::sync::LazyLock::new(|| parking_lot::Mutex::new(std::collections::HashMap::new()));

/// The Custom reactions made from the script file at `path`, however their
/// path was written.
pub fn custom_reactions_for_script(path: &std::path::Path) -> Vec<ImpactReactionKind> {
    let same = |p: &std::path::Path| p == path || matches!((p.canonicalize(), path.canonicalize()), (Ok(a), Ok(b)) if a == b);
    CUSTOM_REACTION_SCRIPT_PATHS
        .lock()
        .iter()
        .filter(|(_, p)| same(std::path::Path::new(p.as_str())))
        .map(|(id, _)| ImpactReactionKind::Custom(*id))
        .collect()
}

fn hash_str(s: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut hasher);
    hasher.finish()
}

/// Registers `path` under a stable hash of itself and returns that hash —
/// the same path always yields the same hash, so re-selecting the same
/// script across app restarts still hits the same effect/uniform cache key.
pub fn register_custom_reaction_script_path(path: &str) -> u64 {
    let id = hash_str(path);
    CUSTOM_REACTION_SCRIPT_PATHS.lock().insert(id, path.to_string());
    id
}

/// Resolves a `Custom(u64)` reaction's hash back to its script path.
pub fn custom_reaction_script_path(id: u64) -> Option<String> {
    CUSTOM_REACTION_SCRIPT_PATHS.lock().get(&id).cloned()
}

/// Fires a single avatar-impact-reaction instance for `target_name`, spawning
/// PixelDrip particles straight into the global physics state. For use by
/// **external** callers only (fluid/fire/lightning's "on start" firing in
/// effects.rs) — calling this from inside `PhysicsEngine` itself (spawn /
/// trigger_impact_internal) would deadlock re-locking `state.physics`, which
/// is why `fire_impact_reaction_local` exists for that case.
pub fn fire_impact_reaction(reaction: &ImpactReactionConfig, target_name: &str, fallback_x: f32, fallback_y: f32) {
    if reaction.kind == ImpactReactionKind::PixelDrip {
        let state = crate::state::get_state();
        let mut physics = state.physics.write();
        fire_impact_reaction_local(reaction, target_name, fallback_x, fallback_y, None, &mut physics.particles);
    } else {
        fire_impact_reaction_local(reaction, target_name, fallback_x, fallback_y, None, &mut Vec::new());
    }
}

/// Where a throw aims with Path Randomness (0–1): between the target's center
/// (tx, ty) and a random point of its solid pixels (its pixel grid, from the
/// target scan), `randomness` of the way — at 1 anywhere on the target, never
/// on its transparent parts, so the throw still hits. Without a grid, near
/// the center.
pub fn random_aim(tx: f32, ty: f32, target: Option<&str>, randomness: f32) -> (f32, f32) {
    let point = target.and_then(|name| {
        let grids = get_state().pixel_grids.read();
        let grid = grids.get(name).filter(|g| !g.is_empty())?;
        let p = grid[(rand_f32() * grid.len() as f32) as usize % grid.len()];
        Some((p.x, p.y))
    });
    match point {
        Some((px, py)) => (tx + (px - tx) * randomness, ty + (py - ty) * randomness),
        None => (
            tx + (rand_f32() - 0.5) * 80.0 * randomness,
            ty + (rand_f32() - 0.5) * 80.0 * randomness,
        ),
    }
}

/// A target's own frame on the canvas: its Studio source's box and rotation.
/// Things that belong on the target (bites, stuck objects) are placed in it,
/// not on its scanned outline (the hitbox): a reaction (a bite, a squish) or
/// the avatar's own movement reshapes the outline, and stuck objects riding
/// it slid across the target, left where it had been once it changed back.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TargetFrame {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub roll: f32,
    pub pitch: f32,
    pub yaw: f32,
}

impl TargetFrame {
    /// The frame of `target` (a source's name, or a slot number), found as
    /// `tracked` (its name among the tracked targets) first. None if it isn't
    /// a Studio source with a size, or the source list is busy — this runs
    /// inside the physics step, hence try_read (see the PixelDrip note below).
    pub fn find(target: &str, tracked: Option<&str>) -> Option<Self> {
        let sources = crate::state::get_state().studio_sources.try_read()?;
        let src = tracked
            .and_then(|name| sources.iter().find(|s| s.name == name))
            .or_else(|| sources.iter().find(|s| s.name == target))
            .or_else(|| sources.iter().find(|s| s.name.eq_ignore_ascii_case(target)))
            .or_else(|| target.trim().parse::<u32>().ok().and_then(|n| sources.iter().find(|s| s.slot == n)))?;
        (src.width > 0.0 && src.height > 0.0).then_some(Self {
            x: src.x,
            y: src.y,
            w: src.width,
            h: src.height,
            roll: src.roll,
            pitch: src.pitch,
            yaw: src.yaw,
        })
    }

    fn center(&self) -> (f32, f32) {
        (self.x + self.w * 0.5, self.y + self.h * 0.5)
    }

    /// Canvas point (x, y) in the frame: 0..1 across its width and height,
    /// before its rotation.
    pub fn to_local(&self, x: f32, y: f32) -> (f32, f32) {
        let (cx, cy) = self.center();
        let (lx, ly) = crate::tracker::unproject_3d_canvas(x, y, cx, cy, self.roll, self.pitch, self.yaw);
        (0.5 + lx / self.w, 0.5 + ly / self.h)
    }

    /// The frame's point (x, y) (see to_local) on the canvas.
    pub fn to_canvas(&self, x: f32, y: f32) -> (f32, f32) {
        let (cx, cy) = self.center();
        crate::tracker::project_3d_canvas((x - 0.5) * self.w, (y - 0.5) * self.h, cx, cy, self.roll, self.pitch, self.yaw)
    }
}

/// Remembers where `th` stuck on its target: in the target's own frame (see
/// TargetFrame), or across its hitbox when it has no frame.
fn stick_to_target(
    th: &mut Throwable,
    target_hb: Option<&crate::types::Hitbox>,
    hitboxes: &HashMap<String, crate::types::Hitbox>,
    slots: &HashMap<usize, String>,
) {
    // Reuse the read guards already held for this whole step() call (see the
    // top of step) — re-acquiring them here would be a recursive read on the
    // same thread, which parking_lot's RwLock can deadlock on if a writer
    // queues in between.
    let resolved = crate::resolve_target_hitbox(&th.target_name, slots, hitboxes);
    if let Some(frame) = TargetFrame::find(&th.target_name, resolved.map(|(name, _)| name)) {
        (th.stuck_rel_x, th.stuck_rel_y) = frame.to_local(th.x, th.y);
        th.stuck_in_frame = true;
        return;
    }
    let hb = target_hb.copied().or_else(|| resolved.map(|(_, hb)| *hb));
    if let Some(hb) = hb {
        if hb.active && hb.world_width > 0.0 && hb.world_height > 0.0 {
            th.stuck_rel_x = (th.x - hb.world_x) / hb.world_width;
            th.stuck_rel_y = (th.y - hb.world_y) / hb.world_height;
        }
    }
}

/// Canvas point (x, y) in `target`'s own frame (see TargetFrame and
/// state::ReactionHit), a little past its edges at most.
pub fn hit_in_target_frame(target: &str, x: f32, y: f32) -> Option<(f32, f32)> {
    let (lx, ly) = TargetFrame::find(target, None)?.to_local(x, y);
    Some((lx.clamp(-0.5, 1.5), ly.clamp(-0.5, 1.5)))
}

/// Internal version for use within `PhysicsEngine` itself (`spawn` /
/// `trigger_impact_internal`), where `self.particles` is already reachable
/// directly and `state.physics` may already be locked by the caller.
fn fire_impact_reaction_local(
    reaction: &ImpactReactionConfig,
    target_name: &str,
    fallback_x: f32,
    fallback_y: f32,
    target_hb: Option<&crate::types::Hitbox>,
    new_particles: &mut Vec<Particle>,
) {
    match reaction.kind {
        ImpactReactionKind::Bounce | ImpactReactionKind::Jolt | ImpactReactionKind::PixelDrip | ImpactReactionKind::Custom(_) => {
            // Persistent per-frame reaction: the render pass reads this every
            // frame to distort the avatar (via a real per-pixel shader for
            // Bounce/PixelDrip/Custom, or jitter+flash for Jolt) for `duration_ms`.
            // It's told where the target was hit (bite_vanish bites there).
            let hit = hit_in_target_frame(target_name, fallback_x, fallback_y);
            crate::state::get_state().trigger_avatar_reaction(target_name, reaction.kind, reaction.duration_ms, reaction.amount, reaction.custom_params.clone(), hit);
        }
        _ => {}
    }
    match reaction.kind {
        ImpactReactionKind::PixelDrip => {
            // Also spawn a one-shot droplet burst off the bottom edge of the
            // target, complementing the per-pixel melt shader on the avatar itself.
            let (spawn_x, spawn_y, spread) = {
                // try_read, not read: this fires from inside PhysicsEngine::step()
                // (via the *_local call sites), which already holds hitboxes/
                // target_slots reads for the whole step — a blocking read here
                // would be a recursive acquisition on the same thread, which
                // parking_lot's RwLock can deadlock on if a writer queues in
                // between. Skipping the refined spawn position on the rare
                // occasion this lock is contended is harmless; hanging isn't.
                let resolved_hb = target_hb.copied().or_else(|| {
                    let state = crate::state::get_state();
                    let hitboxes = state.hitboxes.try_read()?;
                    let slots = state.target_slots.try_read()?;
                    crate::resolve_target_hitbox(target_name, &slots, &hitboxes).map(|(_, hb)| *hb)
                });
                match resolved_hb {
                    Some(hb) if hb.active && hb.world_width > 0.0 => {
                        (hb.world_x + hb.world_width * 0.5, hb.world_y + hb.world_height * 0.85, hb.world_width * 0.4)
                    }
                    _ => (fallback_x, fallback_y, 60.0),
                }
            };
            let visc = reaction.amount.clamp(0.1, 2.0);
            let duration_ms = reaction.duration_ms.max(200);
            let count = 10 + (duration_ms / 200).min(20);
            for _ in 0..count {
                new_particles.push(Particle {
                    x: spawn_x + (rand_f32() - 0.5) * spread,
                    y: spawn_y,
                    vx: (rand_f32() - 0.5) * 1.5,
                    vy: (1.0 + rand_f32() * 2.0) / visc.max(0.2),
                    alpha: 0.85,
                    fade: 16.0 / duration_ms as f32,
                    size: 3.0 + rand_f32() * 4.0,
                    r: 0.75,
                    g: 0.85,
                    b: 0.95,
                });
            }
        }
        _ => {}
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SpawnPackage {
    pub name: Option<String>,
    pub action: Option<String>,
    pub collision: Option<String>,
    pub target: Option<String>,
    pub speed: Option<f32>,
    pub scale: Option<f32>,
    pub gravity: Option<f32>,
    pub image_src: Option<String>,
    #[serde(rename = "imageSrc")]
    pub image_src_alt: Option<String>,
    pub start_sound: Option<String>,
    #[serde(rename = "startSound")]
    pub start_sound_alt: Option<String>,
    #[serde(default)]
    pub start_sound_path: Option<String>,
    #[serde(rename = "startSoundPath", default)]
    pub start_sound_path_alt: Option<String>,
    pub collision_sound: Option<String>,
    #[serde(rename = "collisionSound")]
    pub collision_sound_alt: Option<String>,
    #[serde(default)]
    pub collision_sound_path: Option<String>,
    #[serde(rename = "collisionSoundPath", default)]
    pub collision_sound_path_alt: Option<String>,
    pub spawn_points: Option<Vec<String>>,
    #[serde(rename = "spawnPoints")]
    pub spawn_points_alt: Option<Vec<String>>,
    pub shake_on_impact: Option<ShakeConfig>,
    #[serde(rename = "shakeOnImpact")]
    pub shake_on_impact_alt: Option<ShakeConfig>,
    #[serde(default, rename = "impactReactions", alias = "impact_reactions")]
    pub impact_reactions: Option<serde_json::Value>,
    #[serde(alias = "redeem_key")]
    pub redeem_key: Option<String>,
    #[serde(alias = "frame_count")]
    pub frame_count: Option<u32>,
    #[serde(alias = "frame_delay_ms")]
    pub frame_delay_ms: Option<u32>,
    #[serde(default)]
    pub amount: Option<serde_json::Value>,
    #[serde(rename = "repeatDelay", default)]
    pub repeat_delay: Option<u64>,
    #[serde(rename = "repeat_delay", default)]
    pub repeat_delay_alt: Option<u64>,
    #[serde(rename = "repeat_delay_ms", default)]
    pub repeat_delay_ms: Option<u64>,
    #[serde(default)]
    pub wildcards: Option<Vec<String>>,
    #[serde(rename = "snapBottom", default)]
    pub snap_bottom: Option<bool>,
    #[serde(rename = "snap_bottom", default)]
    pub snap_bottom_alt: Option<bool>,
    /// How different each throw is, 0–1 (the redeem's Path Randomness): where
    /// on its edge it starts, where on the target it aims, how high it arcs.
    /// 0 (the default) flies every throw from a spawn point on one path.
    #[serde(default, alias = "pathRandomness")]
    pub path_randomness: Option<f32>,
}

/// A number in a wildcard: "500", "500 bits", "$5.00", "CA$2.00", "1,000"
/// (the digits, currency and separators left out)
fn number_in(wildcard: &str) -> Option<String> {
    let t = wildcard.trim();
    let start = t.find(|c: char| c.is_ascii_digit())?;
    let (currency, rest) = t.split_at(start);
    if currency.chars().any(|c| c.is_whitespace() || c.is_ascii_digit()) || currency.chars().count() > 3 {
        return None;
    }
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit() || *c == '.' || *c == ',').collect();
    let after = rest[digits.len()..].trim();
    let unit = after.is_empty() || after.eq_ignore_ascii_case("bits") || after.eq_ignore_ascii_case("bit") || after.chars().count() <= 2 && !after.chars().any(|c| c.is_alphanumeric());
    unit.then(|| digits.replace(',', "").trim_end_matches('.').to_string())
}

/// What {amount} is: the event's amount, the last wildcard that's a number
/// ("Twitch * cheer * bits" catches the channel first), else the first one
pub fn amount_of(wildcards: &[String]) -> Option<String> {
    wildcards.iter().rev().find_map(|w| number_in(w)).or_else(|| wildcards.first().cloned())
}

pub fn resolve_wildcards(template: &str, wildcards: &[String]) -> String {
    let mut result = template.to_string();
    for (idx, val) in wildcards.iter().enumerate() {
        let needle = format!("{{{}}}", idx + 1);
        result = result.replace(&needle, val);
    }
    if let Some(amount) = amount_of(wildcards) {
        result = result.replace("{amount}", &amount);
        result = result.replace("{count}", &amount);
    }
    if wildcards.len() == 1 {
        let tokens: Vec<&str> = wildcards[0].split_whitespace().collect();
        for (idx, tok) in tokens.iter().enumerate() {
            let needle = format!("{{{}}}", idx + 1);
            result = result.replace(&needle, tok);
        }
    }
    result
}

pub fn parse_amount_with_wildcards(
    raw_amount: Option<&serde_json::Value>,
    wildcards: &[String],
) -> usize {
    let raw_str = match raw_amount {
        Some(serde_json::Value::Number(n)) => {
            if let Some(u) = n.as_u64() {
                return (u as usize).max(1);
            } else if let Some(f) = n.as_f64() {
                return (f.round() as usize).max(1);
            } else {
                return 1;
            }
        }
        Some(serde_json::Value::String(s)) => s.as_str(),
        _ => return 1,
    };

    let resolved = resolve_wildcards(raw_str, wildcards);
    let trimmed = resolved.trim();
    if let Ok(parsed) = trimmed.parse::<usize>() {
        parsed.max(1)
    } else if let Some(parsed) = trimmed.parse::<f64>().ok().filter(|f| f.is_finite() && *f >= 0.0) {
        // An amount of money: "5.00"
        (parsed.round() as usize).max(1)
    } else {
        // Strip braces if someone wrote "{3}" or "{5}" directly as a number wildcard
        let clean = trimmed.trim_matches(|c| c == '{' || c == '}').trim();
        if let Ok(parsed) = clean.parse::<usize>() {
            parsed.max(1)
        } else {
            1
        }
    }
}

#[derive(Debug, Clone)]
pub struct Throwable {
    pub id: u64,
    pub redeem_id: u32,
    pub name: String,
    pub target_name: String,
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub gravity: f32,
    pub rotation: f32,
    pub v_rot: f32,
    pub scale: f32,
    pub base_scale: f32,
    pub speed: f32,
    pub action: ActionType,
    pub collision: CollisionType,
    pub state: ThrowableState,
    /// Where it's stuck: in the target's own frame when `stuck_in_frame`
    /// (see TargetFrame), else across the target's hitbox (0..1 each way)
    pub stuck_rel_x: f32,
    pub stuck_rel_y: f32,
    pub stuck_in_frame: bool,
    pub stuck_time: u64,
    pub stick_duration: u64,
    pub swirl_angle: f32,
    pub swirl_radius: f32,
    pub has_hit: bool,
    pub spawn_time: u64,
    pub frame_count: u32,
    pub frame_delay_ms: u32,
    pub collision_sound: Option<String>,
    pub shake: ShakeConfig,
    pub impact_reaction: ImpactReactionConfig,
    pub alpha: f32,
    pub snap_bottom: bool,
}

#[derive(Debug, Clone)]
pub struct Particle {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub alpha: f32,
    pub fade: f32,
    pub size: f32,
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct RenderInstance {
    pub x: f32,
    pub y: f32,
    pub rotation: f32,
    pub scale: f32,
    pub cur_frame: u32,
    pub redeem_id: u32,
    pub is_particle: u8,
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
    pub size: f32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ThrowableImpactEvent {
    pub hit_x: f32,
    pub hit_y: f32,
    pub norm_x: f32,
    pub norm_y: f32,
    pub force: f32,
    pub redeem_id: u32,
}

pub struct PhysicsEngine {
    pub throwables: Vec<Throwable>,
    pub particles: Vec<Particle>,
    pub recent_impacts: Vec<ThrowableImpactEvent>,
    pub redeem_key_to_id: HashMap<String, u32>,
    pub id_to_redeem_key: HashMap<u32, String>,
    pub redeem_images: HashMap<u32, String>,
    pub next_redeem_id: u32,
    pub next_throwable_id: u64,
}

#[derive(Debug, Clone)]
pub struct CachedImagePixels {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

static REDEEM_PIXEL_CACHE: parking_lot::Mutex<Option<std::collections::HashMap<u32, std::sync::Arc<CachedImagePixels>>>> = parking_lot::Mutex::new(None);

/// The picture a redeem's own file (redeems\<key>.json) names, if any.
pub(crate) fn redeem_image_src(key: &str) -> Option<String> {
    let file = std::fs::File::open(crate::paths::redeem_file(key)?).ok()?;
    let val: serde_json::Value = serde_json::from_reader(file).ok()?;
    let src = val.get("imageSrc").or_else(|| val.get("image_src"))?.as_str()?;
    (!src.is_empty()).then(|| src.to_string())
}

pub fn get_or_load_redeem_pixels(redeem_id: u32) -> Option<std::sync::Arc<CachedImagePixels>> {
    get_or_load_redeem_pixels_internal(redeem_id, None, None)
}

pub fn get_or_load_redeem_pixels_internal(
    redeem_id: u32,
    provided_images: Option<&HashMap<u32, String>>,
    provided_keys: Option<&HashMap<u32, String>>,
) -> Option<std::sync::Arc<CachedImagePixels>> {
    if redeem_id == 0 {
        return None;
    }

    {
        let cache = REDEEM_PIXEL_CACHE.lock();
        if let Some(map) = cache.as_ref() {
            if let Some(entry) = map.get(&redeem_id) {
                return Some(entry.clone());
            }
        }
    }

    let img_src_opt = if let Some(imgs) = provided_images {
        imgs.get(&redeem_id).cloned()
    } else {
        let state = crate::state::get_state();
        state.physics.try_read().and_then(|p| p.redeem_images.get(&redeem_id).cloned())
    };

    let img_src = match img_src_opt {
        Some(src) if !src.is_empty() => Some(src),
        _ => {
            let key_opt = if let Some(keys) = provided_keys {
                keys.get(&redeem_id).cloned()
            } else {
                let state = crate::state::get_state();
                state.physics.try_read().and_then(|p| p.id_to_redeem_key.get(&redeem_id).cloned())
            };
            if let Some(key) = key_opt {
                redeem_image_src(&key)
            } else {
                None
            }
        }
    }?;

    use base64::Engine;
    let raw_bytes: Vec<u8> = if let Some(idx) = img_src.find(";base64,") {
        let b64 = &img_src[idx + 8..];
        base64::prelude::BASE64_STANDARD.decode(b64.trim()).ok()?
    } else if std::path::Path::new(&img_src).is_file() {
        std::fs::read(&img_src).ok()?
    } else if let Ok(b) = base64::prelude::BASE64_STANDARD.decode(img_src.trim()) {
        b
    } else {
        // Named relative to your redeems, your data or what ships with Theater
        std::fs::read(crate::paths::find_file(&img_src)?).ok()?
    };

    let rgba = match image::load_from_memory(&raw_bytes) {
        Ok(img) => img.to_rgba8(),
        Err(_) => {
            use image::codecs::gif::GifDecoder;
            use image::AnimationDecoder;
            if let Ok(decoder) = GifDecoder::new(std::io::Cursor::new(&raw_bytes)) {
                if let Ok(mut frames) = decoder.into_frames().collect_frames() {
                    if !frames.is_empty() {
                        frames.remove(0).into_buffer()
                    } else {
                        return None;
                    }
                } else {
                    return None;
                }
            } else {
                return None;
            }
        }
    };

    let (width, height) = rgba.dimensions();
    if width == 0 || height == 0 {
        return None;
    }

    let entry = std::sync::Arc::new(CachedImagePixels {
        width,
        height,
        rgba: rgba.into_raw(),
    });

    let mut cache = REDEEM_PIXEL_CACHE.lock();
    cache.get_or_insert_with(std::collections::HashMap::new).insert(redeem_id, entry.clone());
    Some(entry)
}

pub fn store_redeem_pixels(redeem_id: u32, img_src: &str) {
    if redeem_id == 0 || img_src.is_empty() {
        return;
    }
    use base64::Engine;
    let raw_bytes: Option<Vec<u8>> = if let Some(idx) = img_src.find(";base64,") {
        let b64 = &img_src[idx + 8..];
        base64::prelude::BASE64_STANDARD.decode(b64.trim()).ok()
    } else if std::path::Path::new(img_src).is_file() {
        std::fs::read(img_src).ok()
    } else {
        base64::prelude::BASE64_STANDARD.decode(img_src.trim()).ok()
    };
    if let Some(bytes) = raw_bytes {
        if let Ok(img) = image::load_from_memory(&bytes) {
            let rgba = img.to_rgba8();
            let (width, height) = rgba.dimensions();
            if width > 0 && height > 0 {
                let entry = std::sync::Arc::new(CachedImagePixels {
                    width,
                    height,
                    rgba: rgba.into_raw(),
                });
                let mut cache = REDEEM_PIXEL_CACHE.lock();
                cache.get_or_insert_with(std::collections::HashMap::new).insert(redeem_id, entry);
            }
        }
    }
}

impl PhysicsEngine {
    pub fn new() -> Self {
        Self {
            throwables: Vec::with_capacity(512),
            particles: Vec::with_capacity(1024),
            recent_impacts: Vec::with_capacity(128),
            redeem_key_to_id: HashMap::new(),
            id_to_redeem_key: HashMap::new(),
            redeem_images: HashMap::new(),
            next_redeem_id: 1,
            next_throwable_id: 1,
        }
    }

    pub fn drain_impacts(&mut self, out: &mut [ThrowableImpactEvent]) -> usize {
        let count = self.recent_impacts.len().min(out.len());
        for (i, evt) in self.recent_impacts.drain(..count).enumerate() {
            out[i] = evt;
        }
        count
    }

    pub fn register_redeem(&mut self, key: &str) -> u32 {
        if let Some(&id) = self.redeem_key_to_id.get(key) {
            return id;
        }
        let id = self.next_redeem_id;
        self.next_redeem_id += 1;
        self.redeem_key_to_id.insert(key.to_string(), id);
        self.id_to_redeem_key.insert(id, key.to_string());
        id
    }

    pub fn spawn(&mut self, pkg: SpawnPackage, canvas_w: f32, canvas_h: f32) -> u64 {
        let cw = if canvas_w > 0.0 { canvas_w } else { 1920.0 };
        let ch = if canvas_h > 0.0 { canvas_h } else { 1080.0 };

        let key = pkg.redeem_key.clone()
            .or_else(|| pkg.name.clone())
            .unwrap_or_else(|| "default".to_string());
        let redeem_id = self.register_redeem(&key);

        let img_src = pkg.image_src.clone().or_else(|| pkg.image_src_alt.clone());
        if let Some(src) = img_src {
            if !src.is_empty() {
                self.redeem_images.insert(redeem_id, src.clone());
                store_redeem_pixels(redeem_id, &src);
            }
        }
        if !self.redeem_images.contains_key(&redeem_id) && key != "default" {
            if let Some(src) = redeem_image_src(&key) {
                self.redeem_images.insert(redeem_id, src.clone());
                store_redeem_pixels(redeem_id, &src);
            }
        }

        let action_str = pkg.action.as_deref().unwrap_or("throwable");
        let action = ActionType::from(action_str);

        let col_str = pkg.collision.as_deref().unwrap_or("bounce");
        let collision = CollisionType::from(col_str);

        let wildcards = pkg.wildcards.clone().unwrap_or_default();
        let raw_target = pkg.target.as_deref().unwrap_or("1");
        let target_name = resolve_wildcards(raw_target, &wildcards);
        let amount = parse_amount_with_wildcards(pkg.amount.as_ref(), &wildcards);
        let repeat_delay_ms = pkg.repeat_delay_ms.or(pkg.repeat_delay).or(pkg.repeat_delay_alt).unwrap_or(150).clamp(10, 2000);
        let delay_ns = (repeat_delay_ms as u64) * 1_000_000;
        let base_spawn_time = get_time_ns();

        // Target center, and the tracked target's name (Path Randomness aims
        // at its pixel grid) — from one lookup: "random" picks anew each time
        let mut tracked_target: Option<String> = None;
        let (tx, ty) = {
            let slots = get_state().target_slots.read();
            let hitboxes = get_state().hitboxes.read();
            if let Some((name, hb)) = crate::resolve_target_hitbox(&target_name, &slots, &hitboxes) {
                tracked_target = Some(name.to_string());
                if hb.active && hb.world_center_x > 0.0 && hb.world_center_y > 0.0 {
                    (hb.world_center_x, hb.world_center_y)
                } else if hb.active && hb.world_width > 0.0 {
                    (hb.world_x + hb.world_width * 0.5, hb.world_y + hb.world_height * 0.5)
                } else {
                    (cw * 0.5, ch * 0.5)
                }
            } else {
                let sources = get_state().studio_sources.read();
                if let Some(src) = sources.iter().find(|s| s.name.eq_ignore_ascii_case(&target_name)) {
                    (src.x + src.width * 0.5, src.y + src.height * 0.5)
                } else if let Ok(s_num) = target_name.parse::<u32>() {
                    if let Some(src) = sources.iter().find(|s| s.slot == s_num) {
                        (src.x + src.width * 0.5, src.y + src.height * 0.5)
                    } else {
                        (cw * 0.5, ch * 0.5)
                    }
                } else {
                    (cw * 0.5, ch * 0.5)
                }
            }
        };

        // Start sound
        let start_sound = pkg.start_sound
            .or(pkg.start_sound_alt)
            .or(pkg.start_sound_path)
            .or(pkg.start_sound_path_alt);
        if let Some(ref sound) = start_sound {
            if !sound.is_empty() {
                let _ = crate::play_sound_str(sound);
            }
        }

        let scale = pkg.scale.unwrap_or(1.0).max(0.1);
        let speed = pkg.speed.unwrap_or(8.0).max(1.0);
        let gravity = pkg.gravity.unwrap_or(0.8);
        let randomness = pkg.path_randomness.unwrap_or(0.0).clamp(0.0, 1.0);

        // Spawn coordinates
        let spawn_pts = pkg.spawn_points.or(pkg.spawn_points_alt).unwrap_or_else(|| {
            vec!["tl".to_string(), "tc".to_string(), "tr".to_string(), "ml".to_string(), "mr".to_string()]
        });

        let shake = pkg.shake_on_impact.or(pkg.shake_on_impact_alt).unwrap_or_default();
        let impact_reaction = ImpactReactionConfig::parse(pkg.impact_reactions.as_ref());
        let collision_sound = pkg.collision_sound
            .or(pkg.collision_sound_alt)
            .or(pkg.collision_sound_path)
            .or(pkg.collision_sound_path_alt);
        let snap_bottom = pkg.snap_bottom.or(pkg.snap_bottom_alt).unwrap_or(false);

        let mut first_id = self.next_throwable_id;

        for i in 0..amount {
            let chosen_pt = if !spawn_pts.is_empty() {
                let idx = (rand_f32() * spawn_pts.len() as f32) as usize % spawn_pts.len();
                spawn_pts[idx].as_str()
            } else {
                "ml"
            };

            let (mut sx, mut sy) = match chosen_pt {
                "tl" => (-50.0, -50.0),
                "tc" => (cw * 0.5, -50.0),
                "tr" => (cw + 50.0, -50.0),
                "ml" => (-50.0, ch * 0.4 + (rand_f32() - 0.5) * 200.0),
                "mr" => (cw + 50.0, ch * 0.4 + (rand_f32() - 0.5) * 200.0),
                "bl" => (-50.0, ch + 50.0),
                "bc" => (cw * 0.5, ch + 50.0),
                "br" => (cw + 50.0, ch + 50.0),
                _ => (-50.0, ch * 0.5),
            };

            // Path Randomness: this throw starts somewhere along its spawn
            // point's edge (up to a third of it) and aims anywhere on the target
            let (aim_x, aim_y) = if randomness > 0.0 {
                match chosen_pt {
                    "ml" | "mr" => sy += (rand_f32() - 0.5) * randomness * ch / 3.0,
                    _ => sx += (rand_f32() - 0.5) * randomness * cw / 3.0,
                }
                random_aim(tx, ty, tracked_target.as_deref(), randomness)
            } else {
                (tx, ty)
            };

            let (vx, vy, flight_scale) = match action {
                ActionType::Drop => {
                    let slots = get_state().target_slots.read();
                    let hitboxes = get_state().hitboxes.read();
                    let target_hb = crate::resolve_target_hitbox(&target_name, &slots, &hitboxes).map(|(_, v)| v);
                    if let Some(hb) = target_hb {
                        if hb.active && hb.world_width > 0.0 {
                            sx = hb.world_center_x + (rand_f32() - 0.5) * (hb.world_width * 0.4).min(40.0);
                        } else {
                            sx = tx;
                        }
                    } else {
                        sx = tx;
                    }
                    sy = -60.0;
                    (0.0, speed.max(6.0) * 1.5, scale)
                }
                ActionType::Rain => {
                    sx = tx + (rand_f32() - 0.5) * 400.0;
                    sy = -60.0;
                    ((rand_f32() - 0.5) * 3.0, speed * 1.8, scale)
                }
                ActionType::ThrowFront => {
                    sx = cw * 0.5 + (rand_f32() - 0.5) * 200.0;
                    sy = ch * 0.5 + (rand_f32() - 0.5) * 200.0;
                    let dx = aim_x - sx;
                    let dy = aim_y - sy;
                    let f_scale = scale * 2.8;
                    let time_f = (25.0 / (speed / 8.0)).max(5.0);
                    let vx = dx / time_f;
                    let vy = (dy - 0.5 * gravity * time_f * (time_f - 1.0)) / time_f;
                    (vx, vy, f_scale)
                }
                ActionType::Swirl => {
                    let angle = rand_f32() * PI * 2.0;
                    sx = tx + angle.cos() * 500.0;
                    sy = ty + angle.sin() * 500.0;
                    (0.0, 0.0, scale)
                }
                ActionType::Throwable => {
                    let dx = aim_x - sx;
                    let dy = aim_y - sy;
                    let dist = (dx * dx + dy * dy).sqrt();
                    let mut time_frames = (dist / (speed * 3.5)).clamp(12.0, 75.0);
                    // Path Randomness: a higher, slower lob or a flatter, faster
                    // throw (up to 40% either way) — still landing on its aim
                    if randomness > 0.0 {
                        time_frames = (time_frames * (1.0 + (rand_f32() - 0.5) * 0.8 * randomness)).clamp(8.0, 100.0);
                    }
                    let vx = dx / time_frames;
                    // Discrete Euler correction: sum_{k=0}^{N-1} (vy_0 + k*g) = N*vy_0 + 0.5*g*N*(N-1) = dy
                    let vy = (dy - 0.5 * gravity * time_frames * (time_frames - 1.0)) / time_frames;
                    (vx, vy, scale)
                }
            };

            let id = self.next_throwable_id;
            self.next_throwable_id += 1;
            if i == 0 {
                first_id = id;
                if shake.enabled && shake.timing == ShakeTiming::Start {
                    crate::state::get_state().trigger_screen_shake(shake.duration, shake.intensity);
                }
                if impact_reaction.kind != ImpactReactionKind::None && impact_reaction.timing == ShakeTiming::Start {
                    let target_hb = {
                        let slots = get_state().target_slots.read();
                        let hitboxes = get_state().hitboxes.read();
                        crate::resolve_target_hitbox(&target_name, &slots, &hitboxes).map(|(_, hb)| *hb)
                    };
                    fire_impact_reaction_local(&impact_reaction, &target_name, tx, ty, target_hb.as_ref(), &mut self.particles);
                }
            }

            let spawn_time = base_spawn_time + (i as u64 * delay_ns);

            let init_rotation = if snap_bottom {
                (ty - sy).atan2(tx - sx).to_degrees() - 90.0
            } else {
                rand_f32() * 360.0
            };

            let throwable = Throwable {
                id,
                redeem_id,
                name: pkg.name.clone().unwrap_or_else(|| "Throwable".to_string()),
                target_name: target_name.clone(),
                x: sx,
                y: sy,
                vx,
                vy,
                gravity,
                rotation: init_rotation,
                v_rot: if snap_bottom { 0.0 } else { (rand_f32() - 0.5) * 16.0 },
                scale: flight_scale,
                base_scale: scale,
                speed,
                action,
                collision,
                state: ThrowableState::Flying,
                stuck_rel_x: 0.0,
                stuck_rel_y: 0.0,
                stuck_in_frame: false,
                stuck_time: 0,
                stick_duration: if collision == CollisionType::Squish {
                    6_000_000_000
                } else {
                    8_000_000_000
                },
                swirl_angle: rand_f32() * PI * 2.0,
                swirl_radius: 500.0,
                has_hit: false,
                spawn_time,
                frame_count: pkg.frame_count.unwrap_or(1).max(1),
                frame_delay_ms: pkg.frame_delay_ms.unwrap_or(100).max(10),
                collision_sound: collision_sound.clone(),
                shake: shake.clone(),
                impact_reaction: impact_reaction.clone(),
                alpha: 1.0,
                snap_bottom,
            };

            self.throwables.push(throwable);
        }

        first_id
    }

    pub fn step(&mut self, dt_seconds: f32, canvas_w: f32, canvas_h: f32) {
        let dt = (dt_seconds * 60.0).clamp(0.1, 5.0);
        let cur_ns = get_time_ns();
        let cw = if canvas_w > 0.0 { canvas_w } else { 1920.0 };
        let ch = if canvas_h > 0.0 { canvas_h } else { 1080.0 };

        let state = get_state();
        let hitboxes = state.hitboxes.read();
        let masks = state.masks.read();
        let slots = state.target_slots.read();

        let mut new_particles = Vec::new();
        let mut new_impacts = Vec::new();

        for th in self.throwables.iter_mut() {
            if cur_ns < th.spawn_time {
                continue;
            }
            match th.state {
                ThrowableState::Flying => {
                    let fallback_hb;
                    let target_hb = if let Some((_, v)) = crate::resolve_target_hitbox(&th.target_name, &slots, &hitboxes) {
                        Some(v)
                    } else {
                        let sources = state.studio_sources.read();
                        if let Some(src) = sources.iter().find(|s| s.name.eq_ignore_ascii_case(&th.target_name)) {
                            fallback_hb = crate::types::Hitbox {
                                local_x: 0.0,
                                local_y: 0.0,
                                local_width: src.width,
                                local_height: src.height,
                                world_x: src.x,
                                world_y: src.y,
                                world_width: src.width,
                                world_height: src.height,
                                world_center_x: src.x + src.width * 0.5,
                                world_center_y: src.y + src.height * 0.5,
                                mass: 100,
                                active: true,
                            };
                            Some(&fallback_hb)
                        } else {
                            None
                        }
                    };

                    // Swirl motion only applies before contact — once has_hit
                    // fires (radius decays under the 30px threshold below),
                    // this must stop re-pinning th.x/th.y every frame so the
                    // normal post-impact branch (gravity, collision-type
                    // behavior, off-screen culling) can take over. Without
                    // this, a Bounce-collision swirl (the default collision
                    // type) never changes th.state away from Flying, so it
                    // would keep re-entering this branch and sit rotating in
                    // place at the target forever, never dying.
                    if th.action == ActionType::Swirl && !th.has_hit {
                        let (tx, ty) = if let Some(hb) = target_hb {
                            // Swirl around the mask's center of mass, like the throw itself aims at
                            if hb.active && hb.world_center_x > 0.0 {
                                (hb.world_center_x, hb.world_center_y)
                            } else if hb.active && hb.world_width > 0.0 {
                                (hb.world_x + hb.world_width * 0.5, hb.world_y + hb.world_height * 0.5)
                            } else {
                                (cw * 0.5, ch * 0.5)
                            }
                        } else {
                            (cw * 0.5, ch * 0.5)
                        };

                        th.swirl_angle += 0.08 * (th.speed / 8.0) * dt;
                        th.swirl_radius = (th.swirl_radius - 6.0 * (th.speed / 8.0) * dt).max(0.0);
                        th.x = tx + th.swirl_angle.cos() * th.swirl_radius;
                        th.y = ty + th.swirl_angle.sin() * th.swirl_radius;
                        if th.snap_bottom {
                            let angle = (ty - th.y).atan2(tx - th.x);
                            th.rotation = angle.to_degrees() - 90.0;
                        } else {
                            th.rotation += 15.0 * dt;
                        }

                        if !th.has_hit && th.swirl_radius < 30.0 {
                            Self::trigger_impact_internal(th, target_hb, &hitboxes, &slots, &mut new_particles, &mut new_impacts, &self.redeem_images, &self.id_to_redeem_key);
                        }
                    } else {
                        let prev_y = th.y;

                        th.x += th.vx * dt;
                        th.y += th.vy * dt;
                        th.vy += th.gravity * dt;

                        if th.snap_bottom {
                            let (tx, ty) = if let Some(hb) = target_hb {
                                // Point the bottom at the mask's center of mass, like the throw itself aims at
                                if hb.active && hb.world_center_x > 0.0 {
                                    (hb.world_center_x, hb.world_center_y)
                                } else if hb.active && hb.world_width > 0.0 {
                                    (hb.world_x + hb.world_width * 0.5, hb.world_y + hb.world_height * 0.5)
                                } else {
                                    (cw * 0.5, ch * 0.5)
                                }
                            } else {
                                (cw * 0.5, ch * 0.5)
                            };
                            let angle = (ty - th.y).atan2(tx - th.x);
                            th.rotation = angle.to_degrees() - 90.0;
                        } else {
                            th.rotation += th.v_rot * dt;
                        }

                        if th.action == ActionType::ThrowFront {
                            th.scale = (th.scale - 0.06 * dt).max(th.base_scale);
                        }

                        if !th.has_hit {
                            if let Some(hb) = target_hb {
                                if hb.active && hb.world_width > 0.0 && hb.world_height > 0.0 {
                                    let mut hit = false;
                                    let mut hit_x = th.x;
                                    let mut hit_y = th.y;

                                    let radius = (th.scale * 16.0).clamp(4.0, 32.0);
                                    let (check_x, check_y) = if th.snap_bottom {
                                        let t_cx = hb.world_x + hb.world_width * 0.5;
                                        let t_cy = hb.world_y + hb.world_height * 0.5;
                                        let angle = (t_cy - th.y).atan2(t_cx - th.x);
                                        (th.x + angle.cos() * radius, th.y + angle.sin() * radius)
                                    } else {
                                        (th.x, th.y)
                                    };

                                    if check_x >= hb.world_x && check_x <= hb.world_x + hb.world_width
                                        && check_y >= hb.world_y && check_y <= hb.world_y + hb.world_height
                                    {
                                        hit = true;
                                        hit_x = check_x;
                                        hit_y = check_y;
                                    } else if th.x >= hb.world_x - radius && th.x <= hb.world_x + hb.world_width + radius
                                        && th.y >= hb.world_y - radius && th.y <= hb.world_y + hb.world_height + radius
                                    {
                                        hit = true;
                                        hit_x = th.x.clamp(hb.world_x, hb.world_x + hb.world_width);
                                        hit_y = th.y.clamp(hb.world_y, hb.world_y + hb.world_height);
                                    } else if th.x >= hb.world_x - radius && th.x <= hb.world_x + hb.world_width + radius {
                                        // Continuous sweep for high-velocity drops
                                        if prev_y <= hb.world_y && th.y >= hb.world_y {
                                            hit = true;
                                            hit_y = hb.world_y + 8.0;
                                        }
                                    }

                                    if hit {
                                        let target_mask = crate::resolve_target_mask(&th.target_name, &slots, &masks).map(|(_, v)| v);
                                        let is_solid = if let Some(mask) = target_mask {
                                            mask.is_solid_world(hit_x, hit_y)
                                        } else {
                                            true
                                        };

                                        if is_solid {
                                            th.x = hit_x;
                                            th.y = hit_y;
                                            Self::trigger_impact_internal(th, Some(hb), &hitboxes, &slots, &mut new_particles, &mut new_impacts, &self.redeem_images, &self.id_to_redeem_key);
                                        }
                                    }
                                }
                            }
                        }
                    }

                    if th.y > ch + 600.0 || th.x < -600.0 || th.x > cw + 600.0 {
                        th.state = ThrowableState::Dead;
                    }
                }
                ThrowableState::Stuck => {
                    let elapsed_ns = cur_ns.saturating_sub(th.stuck_time);
                    let duration_ns = th.stick_duration.max(1_000_000_000);
                    let progress = (elapsed_ns as f32 / duration_ns as f32).clamp(0.0, 1.0);

                    let fallback_hb;
                    let target_hb = if let Some((_, v)) = crate::resolve_target_hitbox(&th.target_name, &slots, &hitboxes) {
                        Some(v)
                    } else {
                        let sources = state.studio_sources.read();
                        if let Some(src) = sources.iter().find(|s| s.name.eq_ignore_ascii_case(&th.target_name)) {
                            fallback_hb = crate::types::Hitbox {
                                local_x: 0.0,
                                local_y: 0.0,
                                local_width: src.width,
                                local_height: src.height,
                                world_x: src.x,
                                world_y: src.y,
                                world_width: src.width,
                                world_height: src.height,
                                world_center_x: src.x + src.width * 0.5,
                                world_center_y: src.y + src.height * 0.5,
                                mass: 100,
                                active: true,
                            };
                            Some(&fallback_hb)
                        } else if let Ok(s_num) = th.target_name.parse::<u32>() {
                            if let Some(src) = sources.iter().find(|s| s.slot == s_num) {
                                fallback_hb = crate::types::Hitbox {
                                    local_x: 0.0,
                                    local_y: 0.0,
                                    local_width: src.width,
                                    local_height: src.height,
                                    world_x: src.x,
                                    world_y: src.y,
                                    world_width: src.width,
                                    world_height: src.height,
                                    world_center_x: src.x + src.width * 0.5,
                                    world_center_y: src.y + src.height * 0.5,
                                    mass: 100,
                                    active: true,
                                };
                                Some(&fallback_hb)
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    };

                    if th.collision == CollisionType::Squish {
                        // Smoothly slide / drip down along target
                        th.stuck_rel_y += 0.0018 * dt;

                        // Fade out: stays solid for first 30%, then smoothly fades to 0.0
                        if progress < 0.3 {
                            th.alpha = 1.0;
                        } else {
                            th.alpha = ((1.0 - progress) / 0.7).clamp(0.0, 1.0);
                        }

                        if th.stuck_rel_y > 1.15 || th.alpha <= 0.01 {
                            th.state = ThrowableState::Dead;
                        }
                    } else if th.collision == CollisionType::Stick {
                        if progress > 0.85 {
                            th.alpha = ((1.0 - progress) / 0.15).clamp(0.0, 1.0);
                        } else {
                            th.alpha = 1.0;
                        }
                    }

                    if th.stuck_in_frame {
                        // Moves, scales and turns with the target, not with its outline.
                        // (Without its frame this frame, it stays where it is.)
                        let tracked = crate::resolve_target_hitbox(&th.target_name, &slots, &hitboxes).map(|(name, _)| name);
                        if let Some(frame) = TargetFrame::find(&th.target_name, tracked) {
                            (th.x, th.y) = frame.to_canvas(th.stuck_rel_x, th.stuck_rel_y);
                        }
                    } else if let Some(hb) = target_hb {
                        if hb.active && hb.world_width > 0.0 && hb.world_height > 0.0 {
                            th.x = hb.world_x + th.stuck_rel_x * hb.world_width;
                            th.y = hb.world_y + th.stuck_rel_y * hb.world_height;
                        }
                    } else {
                        // Target lost: slide down slowly in world space
                        if th.collision == CollisionType::Squish {
                            th.y += 0.8 * dt;
                        }
                    }

                    if elapsed_ns > th.stick_duration {
                        th.state = ThrowableState::Dead;
                    }
                }
                ThrowableState::Dead => {}
            }
        }

        // Retain active throwables
        self.throwables.retain(|th| th.state != ThrowableState::Dead);

        // Append new particles and impact events
        self.particles.extend(new_particles);
        self.recent_impacts.extend(new_impacts);

        // Update particles
        for p in self.particles.iter_mut() {
            p.x += p.vx * dt;
            p.y += p.vy * dt;
            p.vy += 0.12 * dt;
            p.vx *= (1.0 - 0.02 * dt).max(0.0);
            p.alpha -= p.fade * dt;
        }

        self.particles.retain(|p| p.alpha > 0.0);
    }

    fn trigger_impact_internal(
        th: &mut Throwable,
        target_hb: Option<&crate::types::Hitbox>,
        hitboxes: &HashMap<String, crate::types::Hitbox>,
        slots: &HashMap<usize, String>,
        new_particles: &mut Vec<Particle>,
        new_impacts: &mut Vec<ThrowableImpactEvent>,
        redeem_images: &HashMap<u32, String>,
        id_to_redeem_key: &HashMap<u32, String>,
    ) {
        th.has_hit = true;

        let speed = (th.vx * th.vx + th.vy * th.vy).sqrt().max(0.1);
        let nx = if speed > 0.001 { -th.vx / speed } else { 0.0 };
        let ny = if speed > 0.001 { -th.vy / speed } else { -1.0 };
        new_impacts.push(ThrowableImpactEvent {
            hit_x: th.x,
            hit_y: th.y,
            norm_x: nx,
            norm_y: ny,
            force: speed.clamp(1.0, 50.0),
            redeem_id: th.redeem_id,
        });

        if let Some(ref sound) = th.collision_sound {
            if !sound.is_empty() {
                let _ = crate::play_sound_str(sound);
            }
        }

        if th.shake.enabled && th.shake.timing == ShakeTiming::Impact {
            crate::state::get_state().trigger_screen_shake(th.shake.duration, th.shake.intensity);
        }

        if th.impact_reaction.kind != ImpactReactionKind::None && th.impact_reaction.timing == ShakeTiming::Impact {
            fire_impact_reaction_local(&th.impact_reaction, &th.target_name, th.x, th.y, target_hb, new_particles);
        }

        match th.collision {
            CollisionType::Bounce => {
                th.vx = -th.vx * 0.55 + (rand_f32() - 0.5) * 4.0;
                th.vy = -th.vy * 0.45 - (3.0 + rand_f32() * 4.0);
                th.v_rot = (rand_f32() - 0.5) * 25.0;
                th.y -= 12.0;
            }
            CollisionType::Stick => {
                th.state = ThrowableState::Stuck;
                th.stuck_time = get_time_ns();
                th.vx = 0.0;
                th.vy = 0.0;
                th.v_rot = 0.0;
                stick_to_target(th, target_hb, hitboxes, slots);
            }
            CollisionType::Squish => {
                th.state = ThrowableState::Stuck;
                th.stuck_time = get_time_ns();
                th.vx = 0.0;
                th.vy = 0.0;
                th.v_rot = 0.0;
                th.scale *= 1.3;
                stick_to_target(th, target_hb, hitboxes, slots);
            }
            CollisionType::PixelShatter => {
                th.state = ThrowableState::Dead;
                let mut shattered = false;

                if let Some(pixels_entry) = get_or_load_redeem_pixels_internal(th.redeem_id, Some(redeem_images), Some(id_to_redeem_key)) {
                    let w = pixels_entry.width;
                    let h = pixels_entry.height;
                    let raw = &pixels_entry.rgba;

                    let base_size = 64.0 * th.scale;
                    let max_dim = (w.max(h) as f32).max(1.0);
                    let draw_w = (w as f32 / max_dim) * base_size;
                    let draw_h = (h as f32 / max_dim) * base_size;

                    let rad = th.rotation.to_radians();
                    let cos = rad.cos();
                    let sin = rad.sin();

                    // Step through grid to extract ~60-120 particles matching actual sprite pixels
                    let step = ((w.max(h) as usize) / 7).clamp(3, 16);
                    let p_size = ((draw_w / w as f32) * (step as f32) * 1.3).clamp(3.0, 14.0);

                    for y in (0..h as usize).step_by(step) {
                        for x in (0..w as usize).step_by(step) {
                            let idx = (y * w as usize + x) * 4;
                            if idx + 3 < raw.len() {
                                let a = raw[idx + 3];
                                if a > 30 {
                                    let r = raw[idx] as f32 / 255.0;
                                    let g = raw[idx + 1] as f32 / 255.0;
                                    let b = raw[idx + 2] as f32 / 255.0;
                                    let alpha = (a as f32 / 255.0) * 0.95;

                                    // Local offset relative to sprite center
                                    let lx = (x as f32 / w as f32 - 0.5) * draw_w;
                                    let ly = (y as f32 / h as f32 - 0.5) * draw_h;

                                    // Rotated into world offset
                                    let rx = lx * cos - ly * sin;
                                    let ry = lx * sin + ly * cos;

                                    let px = th.x + rx;
                                    let py = th.y + ry;

                                    // Scatter outward from sprite center
                                    let dist = (rx * rx + ry * ry).sqrt().max(1.0);
                                    let dir_x = rx / dist;
                                    let dir_y = ry / dist;

                                    let jitter = (rand_f32() - 0.5) * 0.6;
                                    let j_sin = jitter.sin();
                                    let j_cos = jitter.cos();
                                    let out_x = dir_x * j_cos - dir_y * j_sin;
                                    let out_y = dir_x * j_sin + dir_y * j_cos;

                                    let burst_spd = 2.0 + rand_f32() * 4.5;
                                    let vx = out_x * burst_spd + th.vx * 0.1;
                                    let vy = out_y * burst_spd + th.vy * 0.1 - 1.2;

                                    let fade = 0.014 + rand_f32() * 0.016;

                                    new_particles.push(Particle {
                                        x: px,
                                        y: py,
                                        vx,
                                        vy,
                                        alpha,
                                        fade,
                                        size: p_size,
                                        r,
                                        g,
                                        b,
                                    });
                                    shattered = true;
                                }
                            }
                        }
                    }
                }

                if !shattered {
                    for _ in 0..24 {
                        let angle = rand_f32() * std::f32::consts::TAU;
                        let spd = 2.0 + rand_f32() * 3.5;
                        new_particles.push(Particle {
                            x: th.x,
                            y: th.y,
                            vx: angle.cos() * spd + th.vx * 0.1,
                            vy: angle.sin() * spd + th.vy * 0.1 - 1.0,
                            alpha: 1.0,
                            fade: 0.015 + rand_f32() * 0.015,
                            size: 4.0 + rand_f32() * 4.0,
                            r: 0.85,
                            g: 0.85,
                            b: 0.85,
                        });
                    }
                }
            }
            CollisionType::Pass => {}
        }
    }

    pub fn get_render_instances(&self, out_buf: &mut [RenderInstance]) -> usize {
        let cur_ns = get_time_ns();
        let mut idx = 0;
        let max_len = out_buf.len();

        // 1. Throwables
        for th in &self.throwables {
            if cur_ns < th.spawn_time {
                continue;
            }
            if idx >= max_len { break; }
            let cur_frame = if th.frame_count > 1 {
                let elapsed_ms = (cur_ns.saturating_sub(th.spawn_time) / 1_000_000) as u32;
                (elapsed_ms / th.frame_delay_ms) % th.frame_count
            } else {
                0
            };

            out_buf[idx] = RenderInstance {
                x: th.x,
                y: th.y,
                rotation: th.rotation,
                scale: th.scale,
                cur_frame,
                redeem_id: th.redeem_id,
                is_particle: 0,
                r: 1.0,
                g: 1.0,
                b: 1.0,
                a: th.alpha,
                size: 0.0,
            };
            idx += 1;
        }

        // 2. Particles
        for p in &self.particles {
            if idx >= max_len { break; }
            out_buf[idx] = RenderInstance {
                x: p.x,
                y: p.y,
                rotation: 0.0,
                scale: 1.0,
                cur_frame: 0,
                redeem_id: 0,
                is_particle: 1,
                r: p.r,
                g: p.g,
                b: p.b,
                a: p.alpha,
                size: p.size,
            };
            idx += 1;
        }

        idx
    }

    pub fn clear(&mut self) {
        self.throwables.clear();
        self.particles.clear();
        self.recent_impacts.clear();
        self.redeem_key_to_id.clear();
        self.id_to_redeem_key.clear();
        self.redeem_images.clear();
        if let Some(cache) = REDEEM_PIXEL_CACHE.lock().as_mut() {
            cache.clear();
        }
        self.next_redeem_id = 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_amount_is_the_events_number() {
        let w = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        // A builder pattern: "Twitch * cheer * bits" on "Twitch mortygotvr cheer 500 bits"
        assert_eq!(resolve_wildcards("{amount}", &w(&["mortygotvr", "500"])), "500");
        assert_eq!(resolve_wildcards("{amount}", &w(&["@me", "$5.00"])), "5.00");
        assert_eq!(resolve_wildcards("{amount}", &w(&["YouTube", "@me", "CA$1,000.50"])), "1000.50");
        assert_eq!(resolve_wildcards("{amount}", &w(&["m", "500 bits"])), "500");
        assert_eq!(resolve_wildcards("{amount}", &w(&["m", "5,00 €"])), "500", "(a comma is taken as thousands)");
        // As before: one wildcard, or none a number
        assert_eq!(resolve_wildcards("{amount}", &w(&["3"])), "3");
        assert_eq!(resolve_wildcards("{amount}", &w(&["tomato", "Avatar"])), "tomato");
        assert_eq!(resolve_wildcards("{1} x{count}", &w(&["m", "7"])), "m x7");
        // Money counts as its rounded number
        assert_eq!(parse_amount_with_wildcards(Some(&serde_json::json!("{amount}")), &w(&["@me", "$5.00"])), 5);
        assert_eq!(parse_amount_with_wildcards(Some(&serde_json::json!("{amount}")), &w(&["m", "2.6"])), 3);
    }

    #[test]
    fn stuck_objects_follow_the_target_not_its_outline() {
        let name = "StickFrameTarget";
        let state = get_state();
        state.studio_sources.write().push(crate::StudioSource {
            scene: String::new(),
            name: name.to_string(),
            slot: 77,
            x: 100.0,
            y: 100.0,
            width: 400.0,
            height: 300.0,
            roll: 0.0,
            pitch: 0.0,
            yaw: 0.0,
            is_target: true,
            match_obs: false,
            is_visible: true,
            file_path: String::new(),
            default_x: 100.0,
            default_y: 100.0,
            default_width: 400.0,
            default_height: 300.0,
            default_roll: 0.0,
        });
        let outline = |x: f32, w: f32| crate::types::Hitbox {
            world_x: x,
            world_y: 120.0,
            world_width: w,
            world_height: 260.0,
            active: true,
            ..Default::default()
        };
        state.hitboxes.write().insert(name.to_string(), outline(150.0, 300.0));

        let pkg = SpawnPackage {
            name: Some("StickTest".to_string()),
            target: Some(name.to_string()),
            collision: Some("stick".to_string()),
            ..Default::default()
        };
        let id = state.physics.write().spawn(pkg, 1920.0, 1080.0);
        {
            let hitboxes = state.hitboxes.read();
            let slots = state.target_slots.read();
            let mut physics = state.physics.write();
            let th = physics.throwables.iter_mut().find(|t| t.id == id).unwrap();
            (th.x, th.y) = (200.0, 180.0);
            th.state = ThrowableState::Stuck;
            th.stuck_time = get_time_ns();
            stick_to_target(th, None, &hitboxes, &slots);
            assert!(th.stuck_in_frame);
        }
        let position = || {
            state.physics.write().step(1.0 / 60.0, 1920.0, 1080.0);
            let physics = state.physics.read();
            let th = physics.throwables.iter().find(|t| t.id == id).unwrap();
            (th.x, th.y)
        };
        let close = |(x, y): (f32, f32), (ex, ey): (f32, f32)| (x - ex).abs() < 0.01 && (y - ey).abs() < 0.01;

        assert!(close(position(), (200.0, 180.0)));
        // A reaction reshapes the outline (a bite eats its left side): it stays put
        state.hitboxes.write().insert(name.to_string(), outline(260.0, 190.0));
        assert!(close(position(), (200.0, 180.0)), "moved with the outline");
        // The target itself moves: it goes along
        state.studio_sources.write().iter_mut().find(|s| s.name == name).unwrap().x += 50.0;
        assert!(close(position(), (250.0, 180.0)), "didn't follow the target");

        state.physics.write().throwables.retain(|t| t.id != id);
        state.hitboxes.write().remove(name);
        state.studio_sources.write().retain(|s| s.name != name);
    }
}
