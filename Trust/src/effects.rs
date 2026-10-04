// ============================================================================
// Trust — Effects Manager
// ============================================================================
// Central coordinator for effect engines:
//   SphEngine       (fluid.rs)     — SPH liquid & metaball simulation (water, slime, milk, etc.)
//   FireEngine      (fire.rs)      — fire / smoke / plasma
//   LightningEngine (lightning.rs) — lightning / electricity
//
// Provides unified step() and export (fill_particle_buf / fill_segment_buf).

use std::collections::HashMap;
use crate::types::Hitbox;
use crate::fluid::{SphEngine, SphConfig};
use crate::fire::{FireEngine, FireConfig};
use crate::lightning::{LightningEngine, LightningConfig};

/// Fluid/fire/lightning have no discrete "impact" moment the way a thrown
/// object does, so their Screen Shake only ever fires "on start" — "on impact"
/// timing is simply a no-op for these until a per-strike hook exists.
fn maybe_trigger_start_shake(shake: &crate::physics::ShakeConfig) {
    if shake.enabled && shake.timing == crate::physics::ShakeTiming::Start {
        crate::state::get_state().trigger_screen_shake(shake.duration, shake.intensity);
    }
}

/// Same reasoning as `maybe_trigger_start_shake`: fluid/fire/lightning have no
/// discrete "impact" moment, so their avatar reaction only ever fires "on start".
fn maybe_trigger_start_reaction(reaction: &crate::physics::ImpactReactionConfig, target_name: &str, target_x: f32, target_y: f32) {
    if reaction.kind != crate::physics::ImpactReactionKind::None && reaction.timing == crate::physics::ShakeTiming::Start {
        crate::physics::fire_impact_reaction(reaction, target_name, target_x, target_y);
    }
}

// ---------------------------------------------------------------------------
// Effect type tag
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectKind {
    Fluid,
    Fire,
    Lightning,
}

impl EffectKind {
    pub fn from_str(s: &str) -> Self {
        let lower = s.to_lowercase();
        if lower.contains("fire") || lower.contains("flame") || lower.contains("smoke") {
            EffectKind::Fire
        } else if lower.contains("lightning") || lower.contains("electric") || lower.contains("thunder") {
            EffectKind::Lightning
        } else {
            // water, webgl-water, fluid, slime, lava, goo, milk -> SPH FluidEngine!
            EffectKind::Fluid
        }
    }
}

// ---------------------------------------------------------------------------
// EffectsManager
// ---------------------------------------------------------------------------
pub struct EffectsManager {
    pub fluid_engines:     Vec<SphEngine>,
    pub fire_engines:      Vec<FireEngine>,
    pub lightning_engines: Vec<LightningEngine>,
    next_seed: u64,
    next_fluid_id: u32,
}

impl EffectsManager {
    pub fn new() -> Self {
        Self {
            fluid_engines:     Vec::new(),
            fire_engines:      Vec::new(),
            lightning_engines: Vec::new(),
            next_seed: 12345,
            next_fluid_id: 1,
        }
    }

    fn alloc_seed(&mut self) -> u64 {
        self.next_seed = self.next_seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.next_seed
    }

    fn alloc_fluid_id(&mut self) -> u32 {
        let id = self.next_fluid_id;
        self.next_fluid_id = self.next_fluid_id.wrapping_add(1).max(1);
        id
    }

    // -----------------------------------------------------------------------
    // Spawn
    // -----------------------------------------------------------------------

    /// Spawn a fluid (SPH) effect. Returns internal index.
    pub fn spawn_fluid(
        &mut self,
        start_x: f32, start_y: f32,
        target_x: f32, target_y: f32,
        target_name: String,
        config: SphConfig,
    ) -> u32 {
        maybe_trigger_start_shake(&config.shake);
        maybe_trigger_start_reaction(&config.impact_reaction, &target_name, target_x, target_y);
        let id = self.alloc_fluid_id();
        let seed = self.alloc_seed();
        self.fluid_engines.push(SphEngine::new(
            id, start_x, start_y, target_x, target_y, target_name, config, seed,
        ));
        (self.fluid_engines.len() - 1) as u32
    }

    /// Spawn a fire effect.
    pub fn spawn_fire(
        &mut self,
        start_x: f32, start_y: f32,
        target_x: f32, target_y: f32,
        target_name: String,
        config: FireConfig,
    ) -> u32 {
        maybe_trigger_start_shake(&config.shake);
        maybe_trigger_start_reaction(&config.impact_reaction, &target_name, target_x, target_y + config.offset_y);
        // TEMP fire-size diagnostics (trust_debug.log) — remove once resolved
        crate::obs_source::trust_log(&format!(
            "[Trust Fire] spawn target='{}' aim=({:.0},{:.0}) from=({:.0},{:.0}) mode={} count={} radius={} width={} gravity={} viscosity={} duration_ms={} offset_y={} prewarm={}",
            target_name, target_x, target_y, start_x, start_y, config.stream_mode, config.particle_count, config.radius,
            config.fluid_width, config.gravity, config.viscosity, config.duration_ms, config.offset_y, config.prewarm_frames
        ));
        let seed = self.alloc_seed();
        self.fire_engines.push(FireEngine::new(
            start_x, start_y, target_x, target_y, target_name, config, seed,
        ));
        (self.fire_engines.len() - 1) as u32
    }

    /// Spawn a lightning effect.
    pub fn spawn_lightning(
        &mut self,
        start_x: f32, start_y: f32,
        target_x: f32, target_y: f32,
        target_name: String,
        config: LightningConfig,
    ) -> u32 {
        maybe_trigger_start_shake(&config.shake);
        maybe_trigger_start_reaction(&config.impact_reaction, &target_name, target_x, target_y);
        let seed = self.alloc_seed();
        self.lightning_engines.push(LightningEngine::new(
            start_x, start_y, target_x, target_y, target_name, config, seed,
        ));
        (self.lightning_engines.len() - 1) as u32
    }

    // -----------------------------------------------------------------------
    // Step all active engines every frame
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
        for eng in &mut self.fluid_engines {
            eng.step(dt_ms, canvas_w, canvas_h, hitboxes, slots, masks);
        }
        for eng in &mut self.fire_engines {
            eng.step(dt_ms, canvas_w, canvas_h, hitboxes, slots, masks);
        }
        for eng in &mut self.lightning_engines {
            let hb_opt = crate::resolve_target_hitbox(&eng.target_name, slots, hitboxes).map(|(_, hb)| hb);
            let mask_opt = crate::resolve_target_mask(&eng.target_name, slots, masks).map(|(_, m)| m);
            let (center_x, center_y) = if let Some(hb) = &hb_opt {
                if hb.active && hb.world_center_x > 0.0 && hb.world_center_y > 0.0 {
                    (hb.world_center_x, hb.world_center_y)
                } else if hb.active && hb.world_width > 0.0 {
                    (hb.world_x + hb.world_width * 0.5, hb.world_y + hb.world_height * 0.5)
                } else {
                    (eng.target_x, eng.target_y)
                }
            } else {
                (eng.target_x, eng.target_y)
            };

            let (tx, ty) = crate::find_ray_mask_intersection(
                eng.start_x, eng.start_y, center_x, center_y, mask_opt, hb_opt
            );

            eng.step(dt_ms, canvas_w, canvas_h, Some((tx, ty)));
        }

        // Remove dead engines to free memory
        self.fluid_engines.retain(|e| !e.dead);
        self.fire_engines.retain(|e| !e.dead);
        self.lightning_engines.retain(|e| !e.dead);
    }

    // -----------------------------------------------------------------------
    // Counts (for Lua buffer pre-sizing)
    // -----------------------------------------------------------------------
    pub fn particle_count(&self) -> usize {
        let fluid: usize = self.fluid_engines.iter().map(|e| e.particle_count()).sum();
        let fire: usize  = self.fire_engines.iter().map(|e| e.particle_count()).sum();
        fluid + fire
    }

    pub fn segment_count(&self) -> usize {
        self.lightning_engines.iter().map(|e| e.segment_count()).sum()
    }

    pub fn lightning_impact_count(&self) -> usize {
        self.lightning_engines.iter()
            .filter(|e| !e.dead && e.config.enable_impact && e.fade_alpha > 0.01 && e.segment_count() > 0)
            .count()
    }

    // -----------------------------------------------------------------------
    // Export buffers
    // -----------------------------------------------------------------------

    /// Fill flat buffer: fluid particles at 11 floats each (see
    /// `SphEngine::fill_render_buf`), fire particles at 9 (no per-redeem
    /// visual-settings index or engine ID to carry) — NOT a uniform stride, so `out` must
    /// be sized for the worst case (`max_particles * 11`) by the caller.
    /// Returns number of particles written.
    ///
    /// Fluid and fire each get a *guaranteed* share of `max_particles` (at
    /// least half each, more if the other type isn't using its half) rather
    /// than fluid simply filling the buffer first — a heavy water stream
    /// could otherwise silently crowd out every fire particle just by being
    /// simulated first, which is exactly the kind of cross-effect
    /// interference these two are supposed to be free of.
    pub fn fill_particle_buf(&self, out: &mut [f32], max_particles: usize) -> usize {
        const FLUID_STRIDE: usize = 11;
        const FIRE_STRIDE: usize = 9;

        let mut fluid_raw = Vec::new();
        for eng in &self.fluid_engines {
            eng.fill_render_buf(&mut fluid_raw);
        }
        let mut fire_raw = Vec::new();
        for eng in &self.fire_engines {
            eng.fill_render_buf(&mut fire_raw);
        }
        let fluid_total = fluid_raw.len() / FLUID_STRIDE;
        let fire_total = fire_raw.len() / FIRE_STRIDE;

        let (fluid_count, fire_count) = if fluid_total + fire_total <= max_particles {
            // Room for everyone — no truncation, so no fairness question either.
            (fluid_total, fire_total)
        } else {
            let half = max_particles / 2;
            if fluid_total <= half {
                (fluid_total, (max_particles - fluid_total).min(fire_total))
            } else if fire_total <= half {
                ((max_particles - fire_total).min(fluid_total), fire_total)
            } else {
                (half, max_particles - half)
            }
        };

        let fluid_floats = fluid_count * FLUID_STRIDE;
        let fire_floats = fire_count * FIRE_STRIDE;
        out[..fluid_floats].copy_from_slice(&fluid_raw[..fluid_floats]);
        out[fluid_floats..fluid_floats + fire_floats].copy_from_slice(&fire_raw[..fire_floats]);
        fluid_count + fire_count
    }

    /// Like `fill_particle_buf`, but split by kind so a renderer can give
    /// fire a different (additive/glowing) blend mode than fluid's normal
    /// alpha-blended translucent look — same engines, deliberately different
    /// treatment everywhere else, per-kind buffers included.
    pub fn fill_particle_buf_split(&self, fluid_out: &mut Vec<f32>, fire_out: &mut Vec<f32>) {
        for eng in &self.fluid_engines {
            eng.fill_render_buf(fluid_out);
        }
        for eng in &self.fire_engines {
            eng.fill_render_buf(fire_out);
        }
    }

    /// Fluid, plus fire further split into flame wisps vs ember/spark dots —
    /// each rendered with different shapes (see `render_throwable_layer`).
    pub fn fill_particle_buf_split3(&self, fluid_out: &mut Vec<f32>, fire_wisp_out: &mut Vec<f32>, fire_spark_out: &mut Vec<f32>) {
        for eng in &self.fluid_engines {
            eng.fill_render_buf(fluid_out);
        }
        for eng in &self.fire_engines {
            eng.fill_render_buf_split(fire_wisp_out, fire_spark_out);
        }
    }

    /// Fill flat buffer: [x1, y1, x2, y2, r, g, b, alpha, width] × segment_count (9 f32 each).
    pub fn fill_segment_buf(&self, buf: &mut Vec<f32>) {
        for eng in &self.lightning_engines {
            eng.fill_segment_buf(buf);
        }
    }

    /// Like `fill_segment_buf`, split into core bolt segments and glow-pass
    /// duplicates so the glow can be composited additively (real bloom).
    pub fn fill_segment_buf_split(&self, core_buf: &mut Vec<f32>, glow_buf: &mut Vec<f32>) {
        for eng in &self.lightning_engines {
            eng.fill_segment_buf_split(core_buf, glow_buf);
        }
    }

    /// Fill flat buffer for active lightning strike impact flashes:
    /// [tx, ty, r, g, b, alpha, radius, has_mask, xf_pos_x, xf_pos_y, xf_scale_x, xf_scale_y, xf_rot_deg, mask_w, mask_h] (15 f32 each).
    pub fn fill_lightning_impact_buf(
        &self,
        buf: &mut Vec<f32>,
        slots: &HashMap<usize, String>,
        masks: &HashMap<String, crate::tracker::CollisionMask>,
    ) {
        for eng in &self.lightning_engines {
            if !eng.dead && eng.config.enable_impact && eng.fade_alpha > 0.01 && eng.segment_count() > 0 {
                let [r, g, b] = eng.config.color;
                let a = eng.fade_alpha * eng.config.color_a;
                // Impact Size is meant to be independent of the bolt's Bloom Size slider.
                let radius = (eng.config.bolt_width * 10.0 * eng.config.impact_size).clamp(4.0, 1500.0);

                let mask_opt = crate::resolve_target_mask(&eng.target_name, slots, masks).map(|(_, m)| m);

                buf.push(eng.target_x);
                buf.push(eng.target_y);
                buf.push(r);
                buf.push(g);
                buf.push(b);
                buf.push(a);
                buf.push(radius);

                if let Some(mask) = mask_opt {
                    buf.push(1.0); // has_mask = 1.0
                    buf.push(mask.transform.pos_x);
                    buf.push(mask.transform.pos_y);
                    buf.push(mask.transform.scale_x);
                    buf.push(mask.transform.scale_y);
                    buf.push(mask.transform.rot_deg);
                    buf.push(mask.width as f32);
                    buf.push(mask.height as f32);
                    buf.push(mask.transform.crop_left as f32);
                    buf.push(mask.transform.crop_top as f32);
                } else {
                    buf.push(0.0); // has_mask = 0.0
                    buf.push(0.0);
                    buf.push(0.0);
                    buf.push(1.0);
                    buf.push(1.0);
                    buf.push(0.0);
                    buf.push(1.0);
                    buf.push(1.0);
                    buf.push(0.0);
                    buf.push(0.0);
                }
            }
        }
    }

    /// Drain all recent fluid impact events (head splash bursts & ground ripples)
    pub fn drain_impacts(&mut self) -> Vec<crate::fluid::FluidImpact> {
        let mut all = Vec::new();
        for eng in &mut self.fluid_engines {
            all.extend(eng.drain_impacts());
        }
        all
    }

    /// Clear all active effect engines immediately.
    pub fn clear(&mut self) {
        self.fluid_engines.clear();
        self.fire_engines.clear();
        self.lightning_engines.clear();
    }

    /// True if no active effects
    pub fn is_empty(&self) -> bool {
        self.fluid_engines.is_empty()
            && self.fire_engines.is_empty()
            && self.lightning_engines.is_empty()
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

    #[test]
    fn test_manager_spawns_fluid() {
        let mut mgr = EffectsManager::new();
        mgr.spawn_fluid(0.0, 0.0, 500.0, 500.0, "avatar".into(), SphConfig::default());
        assert_eq!(mgr.fluid_engines.len(), 1);
    }

    #[test]
    fn test_manager_spawns_fire() {
        let mut mgr = EffectsManager::new();
        mgr.spawn_fire(0.0, 0.0, 500.0, 500.0, "avatar".into(), FireConfig::default());
        assert_eq!(mgr.fire_engines.len(), 1);
    }

    #[test]
    fn test_manager_spawns_lightning() {
        let mut mgr = EffectsManager::new();
        mgr.spawn_lightning(0.0, 0.0, 500.0, 500.0, "avatar".into(), LightningConfig::default());
        assert_eq!(mgr.lightning_engines.len(), 1);
        assert!(mgr.segment_count() > 0);
    }

    #[test]
    fn test_manager_step_fills_buffers() {
        let mut mgr = EffectsManager::new();
        mgr.spawn_fluid(100.0, 100.0, 500.0, 500.0, "avatar".into(), SphConfig::default());
        mgr.spawn_fire(200.0, 200.0, 500.0, 500.0, "avatar".into(), {
            let mut c = FireConfig::default();
            c.prewarm_frames = 0;
            c
        });
        mgr.spawn_lightning(0.0, 0.0, 500.0, 500.0, "avatar".into(), LightningConfig::default());

        mgr.step(100.0, 1920.0, 1080.0, &dummy_hitboxes(), &HashMap::new(), &HashMap::new());

        let count = mgr.particle_count();
        // Worst case (all fluid) is 11 floats/particle now, not a uniform 9.
        let mut pbuf = vec![0.0f32; count * 11];
        let written = mgr.fill_particle_buf(&mut pbuf, count);
        assert_eq!(written, count, "Particle buffer must fill all particles");

        let mut sbuf = Vec::new();
        mgr.fill_segment_buf(&mut sbuf);
        assert_eq!(sbuf.len(), mgr.segment_count() * 9, "Segment buffer must be count × 9");
    }

    #[test]
    fn test_manager_removes_dead_engines() {
        let mut mgr = EffectsManager::new();
        let mut cfg = SphConfig::default();
        cfg.duration_ms = 20.0;
        cfg.particle_count = 1;
        mgr.spawn_fluid(100.0, 100.0, 500.0, 500.0, "avatar".into(), cfg);

        for _ in 0..500 {
            if mgr.is_empty() { break; }
            mgr.step(16.0, 1920.0, 1080.0, &dummy_hitboxes(), &HashMap::new(), &HashMap::new());
        }
        assert!(mgr.fluid_engines.is_empty(), "Dead engines should be removed");
    }

    #[test]
    fn test_sph_engines_have_unique_ids_and_isolated_render_buffers() {
        let mut mgr = EffectsManager::new();
        let mut cfg1 = SphConfig::default();
        cfg1.color = [0.1, 0.5, 0.9];
        let mut cfg2 = SphConfig::default();
        cfg2.color = [0.1, 0.5, 0.9]; // Identical color to test that ID separates them

        let id1 = mgr.spawn_fluid(100.0, 100.0, 500.0, 500.0, "avatar".into(), cfg1);
        let id2 = mgr.spawn_fluid(200.0, 200.0, 600.0, 600.0, "avatar".into(), cfg2);

        assert_eq!(mgr.fluid_engines.len(), 2);
        let eng1_id = mgr.fluid_engines[id1 as usize].id;
        let eng2_id = mgr.fluid_engines[id2 as usize].id;
        assert_ne!(eng1_id, eng2_id, "Each fluid engine must have a unique ID");

        mgr.step(100.0, 1920.0, 1080.0, &dummy_hitboxes(), &HashMap::new(), &HashMap::new());

        let mut fluid_pbuf = Vec::new();
        for eng in &mgr.fluid_engines {
            eng.fill_render_buf(&mut fluid_pbuf);
        }

        // Verify that 11th float of each chunk is the engine ID and that both IDs are present
        let mut seen_ids = std::collections::HashSet::new();
        for chunk in fluid_pbuf.chunks_exact(11) {
            let engine_id = chunk[10] as u32;
            seen_ids.insert(engine_id);
        }
        assert!(seen_ids.contains(&eng1_id), "Must contain particles with engine 1 ID");
        assert!(seen_ids.contains(&eng2_id), "Must contain particles with engine 2 ID");
        assert_eq!(seen_ids.len(), 2, "Must separate into exactly 2 isolated engine IDs");
    }
}
