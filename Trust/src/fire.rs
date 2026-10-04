// ============================================================================
// Trust — Fire Particle Engine
// ============================================================================
// Ports WebGLFireStream.js physics into native Rust.
// Simulates fire, smoke, plasma-flame effects via upward particles
// with turbulence and life-based color gradients.

use std::collections::HashMap;
use crate::types::Hitbox;
use crate::fluid::Rng;

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------
#[derive(Debug, Clone)]
pub struct FireConfig {
    pub particle_count: usize,
    /// Emission spread width in pixels
    pub fluid_width: f32,
    /// Negative = rises (fire), Positive = falls (embers falling)
    pub gravity: f32,
    /// Drag coefficient (0 = frictionless, 1 = heavily damped)
    pub viscosity: f32,
    pub duration_ms: f32,
    /// Base flame color [0–1] (cooler / outer flame)
    pub color: [f32; 3],
    pub color_a: f32,
    /// Core/hot color [0–1] (brighter, inner flame) — derived if not specified
    pub core_color: [f32; 3],
    /// Outline / shadow color [0–1]
    pub outline_color: [f32; 3],
    /// Particle base radius
    pub radius: f32,
    /// "top" = rises from target  |  "grid" = streams toward target
    pub stream_mode: String,
    /// Pre-warm frames before returning so flame is already full
    pub prewarm_frames: u32,
    /// Optional Screen Shake fired at burst start (no discrete "impact" here).
    pub shake: crate::physics::ShakeConfig,
    /// Optional avatar reaction fired at burst start (see `shake` above — same reasoning).
    pub impact_reaction: crate::physics::ImpactReactionConfig,
    /// Flamethrower ("grid" mode) mask collision tuning, in pixels: how far a
    /// particle is allowed to sink past the target's mask surface before
    /// it's considered "hit". 0 stops right at the visible surface (the
    /// default); negative stops a bit before it; positive lets it dig in
    /// further. See arc_gravity()'s sibling `hit` handling in step_internal.
    pub mask_penetration: f32,
    /// Vertical offset in pixels added to the target / emission position
    pub offset_y: f32,
    /// How far each flame's and spark's glow reaches: the redeem's Glow Size
    /// (bloomSize). 1 is the default look, 0 no glow past the hot core (also
    /// when Enable Glow is off); at most MAX_GLOW_SIZE.
    pub glow_size: f32,
}

/// The largest glow: bigger sprites, times every particle, cost OBS's GPU
pub const MAX_GLOW_SIZE: f32 = 5.0;

impl Default for FireConfig {
    fn default() -> Self {
        let base = [1.0f32, 0.314, 0.0]; // orange
        let core = [
            (base[0] + (1.0 - base[0]) * 0.7).min(1.0),
            (base[1] + (1.0 - base[1]) * 0.7).min(1.0),
            (base[2] + (1.0 - base[2]) * 0.4).min(1.0),
        ];
        let outline = [base[0] * 0.9, base[1] * 0.3, base[2] * 0.15];
        Self {
            particle_count: 400,
            fluid_width: 200.0,
            gravity: -1.0,
            viscosity: 0.6,
            duration_ms: 3000.0,
            color: base,
            color_a: 0.95,
            core_color: core,
            outline_color: outline,
            radius: 16.0,
            stream_mode: "top".to_string(),
            prewarm_frames: 72,
            shake: crate::physics::ShakeConfig::default(),
            impact_reaction: crate::physics::ImpactReactionConfig::default(),
            mask_penetration: 0.0,
            offset_y: 0.0,
            glow_size: 1.0,
        }
    }
}

impl FireConfig {
    pub fn from_json_map(m: &HashMap<String, serde_json::Value>) -> Self {
        let mut c = Self::default();
        // TEMP fire-size diagnostics (trust_debug.log) — remove once resolved
        {
            let raw = serde_json::to_string(m).unwrap_or_default();
            crate::obs_source::trust_log(&format!("[Trust Fire] config json: {}", raw.chars().take(700).collect::<String>()));
        }

        let get_f64 = |keys: &[&str]| -> Option<f64> {
            for k in keys {
                if let Some(v) = m.get(*k) {
                    if let Some(num) = v.as_f64() {
                        return Some(num);
                    }
                    if let Some(s) = v.as_str() {
                        if let Ok(num) = s.parse::<f64>() {
                            return Some(num);
                        }
                    }
                }
            }
            None
        };

        if let Some(v) = get_f64(&["fluidCount", "fluid_count", "count"]) { c.particle_count = v as usize; }
        if let Some(v) = get_f64(&["fluidWidth", "fluid_width", "width"]) { c.fluid_width = v as f32; }
        if let Some(v) = get_f64(&["fluidGravity", "fluid_gravity", "gravity"]) { c.gravity = (v as f32).clamp(-1.0, 0.0); }
        if let Some(v) = get_f64(&["fluidViscosity", "fluid_viscosity", "viscosity"]) { c.viscosity = v as f32; }
        if let Some(v) = get_f64(&["fluidDuration", "fluid_duration", "duration"]) { c.duration_ms = v as f32; }
        if let Some(v) = get_f64(&["fluidRadius", "fluid_radius", "radius"]) { c.radius = (v as f32).clamp(1.0, 20.0); }
        if let Some(v) = get_f64(&["offsetY", "offset_y", "fluidOffsetY", "fluid_offset_y", "fireOffsetY", "fire_offset_y"]) { c.offset_y = v as f32; }
        if let Some(v) = m.get("streamMode").or_else(|| m.get("stream_mode")).and_then(|v| v.as_str()) { c.stream_mode = v.to_string(); }
        if let Some(v) = get_f64(&["maskPenetration", "mask_penetration"]) { c.mask_penetration = v as f32; }
        if let Some(v) = get_f64(&["bloomSize", "bloom_size"]) { c.glow_size = (v as f32).clamp(0.0, MAX_GLOW_SIZE); }
        if m.get("enableBloom").or_else(|| m.get("enable_bloom")).and_then(|v| v.as_bool()) == Some(false) {
            c.glow_size = 0.0;
        }

        let mut base = c.color;
        if let Some(v) = get_f64(&["colorR", "color_r"]) { base[0] = v as f32 / 255.0; }
        if let Some(v) = get_f64(&["colorG", "color_g"]) { base[1] = v as f32 / 255.0; }
        if let Some(v) = get_f64(&["colorB", "color_b"]) { base[2] = v as f32 / 255.0; }
        if let Some(v) = get_f64(&["colorA", "color_a"]) { c.color_a = v as f32; }
        c.color = base;

        // Derive core and outline from base color (matching WebGLFireStream.js logic)
        c.core_color = [
            (base[0] + (1.0 - base[0]) * 0.7).min(1.0),
            (base[1] + (1.0 - base[1]) * 0.7).min(1.0),
            (base[2] + (1.0 - base[2]) * 0.4).min(1.0),
        ];
        c.outline_color = [base[0] * 0.9, base[1] * 0.3, base[2] * 0.15];

        // Guard duration: ensure second values (e.g. 1-60s) become milliseconds
        // so pre-warm doesn't immediately expire the burst, and non-positive defaults safely.
        if c.duration_ms > 0.0 && c.duration_ms <= 60.0 {
            c.duration_ms *= 1000.0;
        } else if c.duration_ms <= 0.0 {
            c.duration_ms = 3000.0;
        }

        c.shake = crate::physics::parse_shake_config(m);
        c.impact_reaction = crate::physics::ImpactReactionConfig::parse(m.get("impactReactions"));
        c
    }
}

// ---------------------------------------------------------------------------
// Particle
// ---------------------------------------------------------------------------
#[derive(Debug, Clone)]
pub struct FireParticle {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    /// 0.0 = dead, 1.0 = just born
    pub life: f32,
    pub size: f32,
    /// Unique seed for per-particle turbulence variation
    pub seed: f32,
    pub decay: f32,
    pub is_ember: bool,
    /// Set once an arc-mode (flamethrower) particle touches the target's
    /// hitbox — from then on it just drops under gravity instead of
    /// continuing its thrown arc or being destroyed on contact.
    pub hit: bool,
}

// ---------------------------------------------------------------------------
// Engine
// ---------------------------------------------------------------------------
pub struct FireEngine {
    pub particles: Vec<FireParticle>,
    pub config: FireConfig,

    pub start_x: f32,
    pub start_y: f32,
    pub target_x: f32,
    pub target_y: f32,
    pub target_name: String,

    pub elapsed_ms: f32,
    pub emitted_count: usize,
    pub dead: bool,

    /// Accumulated time for turbulence phase (seconds)
    time_s: f32,

    /// Whether an arc-mode (flamethrower) particle has already made contact
    /// with the target's hitbox — fires the "Impact" timing exactly once per
    /// burst instead of on every particle that lands.
    reaction_fired: bool,

    rng: Rng,
}

impl FireEngine {
    pub fn new(
        start_x: f32,
        start_y: f32,
        target_x: f32,
        target_y: f32,
        target_name: String,
        config: FireConfig,
        seed: u64,
    ) -> Self {
        let mut eng = Self {
            particles: Vec::with_capacity(config.particle_count + 64),
            config,
            start_x,
            start_y,
            target_x,
            target_y,
            target_name,
            elapsed_ms: 0.0,
            emitted_count: 0,
            dead: false,
            time_s: 0.0,
            reaction_fired: false,
            rng: Rng::new(seed),
        };

        // Pre-warm: simulate frames so flame looks full at start
        let prewarm = eng.config.prewarm_frames;
        if prewarm > 0 && eng.config.stream_mode == "top" {
            let fake_hitboxes: HashMap<String, Hitbox> = HashMap::new();
            let fake_slots: HashMap<usize, String> = HashMap::new();
            let fake_masks: HashMap<String, crate::tracker::CollisionMask> = HashMap::new();
            for _ in 0..prewarm {
                eng.step_internal(16.67, 1920.0, 1080.0, &fake_hitboxes, &fake_slots, &fake_masks);
            }
            // Reset elapsed but keep particles — they appear "pre-existing"
            eng.elapsed_ms = prewarm as f32 * 16.67;
            eng.emitted_count = eng.particles.len().min(eng.config.particle_count);
        }

        eng
    }

    /// Real, always-downward gravity used for the flamethrower's thrown arc —
    /// deliberately independent of the raw `gravity` config field, which
    /// defaults negative (a buoyant "rises like fire" force for campfire
    /// mode). Must be called identically here and in step_internal's is_arc
    /// branch so the arc's landing-spot solve stays exact.
    fn arc_gravity(&self) -> f32 {
        self.config.gravity.abs().max(0.15)
    }

    fn emit_particle(&mut self) {
        let r1 = self.rng.next_f32();
        let r2 = self.rng.next_f32();
        let r3 = self.rng.next_f32();
        let r4 = self.rng.next_f32();
        let r5 = self.rng.next_f32();

        let is_ember = r5 < 0.08;

        let (px, py, pvx, pvy) = if self.config.stream_mode == "top" {
            // Rise straight up from target position (like a campfire)
            let spread = self.config.fluid_width * 0.5;
            (
                self.target_x + (r1 - 0.5) * spread,
                self.target_y + self.config.offset_y,
                (r2 - 0.5) * 1.5,
                self.config.gravity * (r3 * 0.5 + 0.5), // upward (gravity is negative)
            )
        } else {
            // Thrown arc toward target (like a flamethrower shooting a
            // fireball) — same ballistic solve used for regular throwables
            // (physics.rs ActionType::Throwable): pick a flight time from
            // distance/speed, then solve the initial velocity that lands
            // exactly on target after that many frames, so gravity actually
            // curves the path instead of the particle just drifting toward
            // the target in a vague, undirected line.
            let eff_target_y = self.target_y + self.config.offset_y;
            let dx = self.target_x - (self.start_x + (r1 - 0.5) * 20.0);
            let dy = eff_target_y - self.start_y;
            let dist = (dx * dx + dy * dy).sqrt().max(1.0);
            let speed = 9.0 + r2 * 5.0;
            let n = (dist / (speed * 3.5)).clamp(10.0, 45.0);
            // Solve for the *exact* discrete landing spot, matching the
            // integration order in step_internal's is_arc branch frame-for-frame,
            // rather than the continuous-physics approximation — that
            // approximation was fine for short hand-thrown arcs, but edge-to-
            // target flamethrower shots now cover much more distance over more
            // frames, so its error compounds enough to visibly miss.
            //
            // vx decays by *0.95 every frame (see the `p.vx *= 0.95` damping
            // below), so a constant vx = dx/n only covers dx * (1-0.95^n)/(1-0.95*n)
            // of the intended distance — for n=45 that's under half. Scale the
            // initial vx up by the inverse of the geometric-decay sum so the
            // *damped* velocity still totals dx over n frames.
            const VX_DAMPING: f32 = 0.95;
            let vx_sum_factor = (1.0 - VX_DAMPING.powf(n)) / (1.0 - VX_DAMPING);
            let vx = dx / vx_sum_factor;
            // vy isn't damped, only accumulates gravity before each frame's
            // move (`p.vy += gravity; p.y += p.vy;`), so the exact discrete
            // total after n frames is n*vy0 + gravity*n*(n+1)/2 — solve that
            // exactly instead of the continuous n^2 term. Uses `arc_gravity()`
            // (a real downward pull), not the raw configured `gravity` field —
            // that field defaults negative ("rises like fire" for campfire
            // mode), and reusing it here made the thrown stream visibly drift
            // upward mid-flight instead of arcing like a thrown object.
            let vy = (dy - 0.5 * self.arc_gravity() * n * (n + 1.0)) / n;
            (
                self.start_x + (r3 - 0.5) * 20.0,
                self.start_y + (r4 - 0.5) * 20.0,
                vx,
                vy,
            )
        };

        let (size, decay) = if is_ember {
            ((self.config.radius * (0.15 + r3 * 0.2)).min(20.0), 0.003 + r4 * 0.008)
        } else {
            ((self.config.radius * (0.6 + r3 * 0.7)).min(20.0), 0.004 + r4 * 0.006)
        };

        self.particles.push(FireParticle {
            x: px,
            y: py,
            vx: pvx,
            vy: pvy,
            life: 1.0,
            size,
            seed: r2 * 100.0,
            decay,
            is_ember,
            hit: false,
        });
    }

    fn step_internal(
        &mut self,
        dt_ms: f32,
        canvas_w: f32,
        canvas_h: f32,
        hitboxes: &HashMap<String, Hitbox>,
        slots: &HashMap<usize, String>,
        masks: &HashMap<String, crate::tracker::CollisionMask>,
    ) {
        self.elapsed_ms += dt_ms;
        self.time_s += dt_ms / 1000.0;

        // Target hitbox bounds (x, y, w, h), captured for the arc-mode
        // collision check below — the AABB a flamethrower particle actually
        // has to enter to be considered "touching" the target.
        let mut target_bounds: Option<(f32, f32, f32, f32)> = None;

        // `hitboxes`/`slots`/`masks` are already locked by the caller
        // (EffectsManager::step) — re-locking `state.target_slots`/`state.masks`
        // here used to be a same-thread recursive read, which could
        // self-deadlock against a writer (e.g. the Studio's own hitbox/mask
        // rescan) queued in between. Use the passed-in references instead.
        let target_mask = crate::resolve_target_mask(&self.target_name, slots, masks).map(|(_, m)| m);
        if let Some((_, hb)) = crate::resolve_target_hitbox(&self.target_name, slots, hitboxes) {
            if hb.active && hb.world_width > 0.0 && hb.world_height > 0.0 {
                target_bounds = Some((hb.world_x, hb.world_y, hb.world_width, hb.world_height));
            }
            // Aim at the mask's center of mass, like every other effect. The
            // "fire leans left" look that once moved this to the box midpoint
            // wasn't the centroid: fluid::Rng capped its output at ~0.5, so
            // every `(r - 0.5) * spread` below came out negative (fixed there).
            if hb.active && hb.world_center_x > 0.0 && hb.world_center_y > 0.0 {
                self.target_x = hb.world_center_x;
                self.target_y = hb.world_center_y;
            } else if hb.active && hb.world_width > 0.0 && hb.world_height > 0.0 {
                self.target_x = hb.world_x + hb.world_width * 0.5;
                self.target_y = hb.world_y + hb.world_height * 0.5;
            }
        }

        // Emission
        if self.emitted_count < self.config.particle_count {
            let target_emitted = ((self.elapsed_ms / self.config.duration_ms)
                * self.config.particle_count as f32)
                .min(self.config.particle_count as f32) as usize;
            while self.emitted_count < target_emitted {
                self.emit_particle();
                self.emitted_count += 1;
            }
        }

        // Physics
        let t = self.time_s;
        let gravity = self.config.gravity;
        let arc_gravity = self.arc_gravity();
        let is_arc = self.config.stream_mode != "top";
        let mut hit_this_frame = false;

        for p in &mut self.particles {
            let prev_x = p.x;
            let prev_y = p.y;

            // Turbulence: sine-based noise on x velocity (matches JS version).
            // A thrown arc gets a much gentler sway — it should read as a
            // directed shot, not a lazily-drifting campfire wisp.
            p.vx += (p.seed + t * 3.0).sin() * (if is_arc { 0.08 } else { 0.3 });

            if p.hit {
                // Already made contact — go back to the same buoyant physics
                // as campfire mode (rises/curls via the raw, negative-by-
                // default `gravity` field) instead of the arc's real downward
                // pull, so it still reads as fire lingering on the target
                // rather than a dead weight dropping off it.
                p.vy += gravity * self.config.viscosity;
                p.x += p.vx * self.config.viscosity;
                p.y += p.vy * self.config.viscosity;
            } else if is_arc {
                // Real downward gravity for the thrown arc (see arc_gravity),
                // matching the solve in emit_particle so it lands where aimed
                // instead of drifting upward or falling short/long.
                p.vy += arc_gravity;
                p.x += p.vx;
                p.y += p.vy;
            } else {
                p.vy += gravity * self.config.viscosity;
                p.x += p.vx * self.config.viscosity;
                p.y += p.vy * self.config.viscosity;
            }
            p.life -= p.decay;

            // Clamp turb x
            p.vx *= 0.95;

            // Flamethrower collision: a landed aim point means nothing if the
            // particle just keeps flying past the target — mark it "hit" the
            // moment it actually enters the target's hitbox (see the `p.hit`
            // branch above), like a throwable making contact, instead of
            // relying purely on decay/lifetime. The bounding box alone is
            // just a rectangle around the sprite, so also require an actual
            // opaque pixel at that point (same alpha-mask check throwables
            // use) — otherwise it registers as a "hit" over transparent
            // corners of the box and never feels like it touched anything.
            //
            // Testing only the post-move position lets a fast particle
            // tunnel a few pixels past the mask's edge before the check ever
            // catches it — sweep the frame's motion in small steps and snap
            // to the FIRST solid pixel found, so a fast particle can't skip
            // over a thin mask feature in one step.
            //
            // That still isn't the whole story though: obs_source.rs draws
            // a wisp as an elongated glow oriented along its velocity,
            // `rad*2.6` long — its visible leading tip sits ~`rad*1.3` px
            // AHEAD of the particle's own (x, y). Testing collision at the
            // bare center let the *visible* flame tip already be well inside
            // the mask for several frames before the invisible center caught
            // up, which is exactly the "passes through" look. Project the
            // test point forward by that same lead distance (along current
            // velocity) so the wall lines up with what's actually on screen,
            // and keep the particle's own position at the trailing point
            // that produced the hit.
            if is_arc && !p.hit {
                if let Some((bx, by, bw, bh)) = target_bounds {
                    let speed = (p.vx * p.vx + p.vy * p.vy).sqrt();
                    let (dir_x, dir_y) = if speed > 0.01 { (p.vx / speed, p.vy / speed) } else { (0.0, 0.0) };
                    // `mask_penetration` shifts the effective stopping depth:
                    // subtracting it from the visual lead delays the trigger,
                    // letting the particle sink further in before it "hits".
                    let lead = p.size * 1.3 - self.config.mask_penetration;

                    let pad = lead.abs();
                    let seg_min_x = prev_x.min(p.x) - pad;
                    let seg_max_x = prev_x.max(p.x) + pad;
                    let seg_min_y = prev_y.min(p.y) - pad;
                    let seg_max_y = prev_y.max(p.y) + pad;
                    if seg_max_x >= bx && seg_min_x <= bx + bw && seg_max_y >= by && seg_min_y <= by + bh {
                        let travel = ((p.x - prev_x).powi(2) + (p.y - prev_y).powi(2)).sqrt();
                        let steps = (travel / 3.0).ceil().clamp(1.0, 24.0) as usize;
                        for i in 1..=steps {
                            let f = i as f32 / steps as f32;
                            let cx = prev_x + (p.x - prev_x) * f;
                            let cy = prev_y + (p.y - prev_y) * f;
                            let sx = cx + dir_x * lead;
                            let sy = cy + dir_y * lead;
                            if sx >= bx && sx <= bx + bw && sy >= by && sy <= by + bh {
                                let is_solid = target_mask.map(|m| m.is_solid_world(sx, sy)).unwrap_or(true);
                                if is_solid {
                                    p.x = cx;
                                    p.y = cy;
                                    p.hit = true;
                                    hit_this_frame = true;
                                    // Kill the thrown-arc momentum on contact — without
                                    // this the particle just kept coasting through the
                                    // character on its old (deliberately amplified, to
                                    // cover long flights) velocity once "hit", switching
                                    // acceleration model but never actually stopping. A
                                    // real wall stops the object, not just its gravity.
                                    p.vx = 0.0;
                                    p.vy = 0.0;
                                    break;
                                }
                            }
                        }
                    }
                }
            }

            // Wall boundary (soft)
            if p.x < -100.0 || p.x > canvas_w + 100.0 || p.y < -canvas_h || p.y > canvas_h + 200.0 {
                p.life = 0.0;
            }
        }

        self.particles.retain(|p| p.life > 0.0 && p.size >= 0.5);

        if hit_this_frame && !self.reaction_fired {
            self.reaction_fired = true;
            if self.config.shake.enabled && self.config.shake.timing == crate::physics::ShakeTiming::Impact {
                crate::state::get_state().trigger_screen_shake(self.config.shake.duration, self.config.shake.intensity);
            }
            if self.config.impact_reaction.kind != crate::physics::ImpactReactionKind::None
                && self.config.impact_reaction.timing == crate::physics::ShakeTiming::Impact
            {
                crate::physics::fire_impact_reaction(&self.config.impact_reaction, &self.target_name, self.target_x, self.target_y + self.config.offset_y);
            }
        }
    }

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
        self.step_internal(dt_ms, canvas_w, canvas_h, hitboxes, slots, masks);

        if self.elapsed_ms >= self.config.duration_ms && self.particles.is_empty() {
            self.dead = true;
        }
    }

    /// Color gradient: 1.0 = core (hot), 0.5 = base, 0.0 = transparent.
    fn particle_color(&self, p: &FireParticle) -> (f32, f32, f32) {
        if p.is_ember {
            (1.0f32, 0.9, 0.6) // bright white-yellow ember
        } else if p.life > 0.6 {
            let t = (p.life - 0.6) / 0.4; // 0–1 within young range
            (
                self.config.core_color[0] * t + self.config.color[0] * (1.0 - t),
                self.config.core_color[1] * t + self.config.color[1] * (1.0 - t),
                self.config.core_color[2] * t + self.config.color[2] * (1.0 - t),
            )
        } else {
            let t = p.life / 0.6; // 0–1 within old range
            (
                self.config.color[0] * t + self.config.outline_color[0] * (1.0 - t),
                self.config.color[1] * t + self.config.outline_color[1] * (1.0 - t),
                self.config.color[2] * t + self.config.outline_color[2] * (1.0 - t),
            )
        }
    }

    /// Export: [x, y, size, r, g, b, a, vx, vy]  (9 f32 per particle)
    /// Color lerps from core_color (young/hot) → base_color (mid) → dark/transparent (old)
    pub fn fill_render_buf(&self, buf: &mut Vec<f32>) {
        for p in &self.particles {
            let (r, g, b) = self.particle_color(p);
            buf.push(p.x);
            buf.push(p.y);
            buf.push(p.size * p.life.max(0.0).sqrt());
            buf.push(r);
            buf.push(g);
            buf.push(b);
            buf.push(p.life.min(1.0) * self.config.color_a);
            buf.push(p.vx);
            buf.push(p.vy);
        }
    }

    /// Like `fill_render_buf`, but splits flame wisps (rendered as an oval
    /// oriented along velocity, mimicking a leaning/stretching flame) from
    /// ember/spark particles (rendered as plain round dots), and adds the
    /// glow size (config.glow_size) — a 10-float stride for both:
    /// [x, y, size, r, g, b, a, vx, vy, glow].
    pub fn fill_render_buf_split(&self, wisp_buf: &mut Vec<f32>, spark_buf: &mut Vec<f32>) {
        for p in &self.particles {
            let (r, g, b) = self.particle_color(p);
            let out = if p.is_ember { &mut *spark_buf } else { &mut *wisp_buf };
            out.push(p.x);
            out.push(p.y);
            out.push(p.size * p.life.max(0.0).sqrt());
            out.push(r);
            out.push(g);
            out.push(b);
            out.push(p.life.min(1.0) * self.config.color_a);
            out.push(p.vx);
            out.push(p.vy);
            out.push(self.config.glow_size);
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
    use std::collections::HashMap;

    fn dummy_hitboxes() -> HashMap<String, crate::types::Hitbox> {
        HashMap::new()
    }

    fn dummy_slots() -> HashMap<usize, String> {
        HashMap::new()
    }

    fn dummy_masks() -> HashMap<String, crate::tracker::CollisionMask> {
        HashMap::new()
    }

    #[test]
    fn test_fire_aims_at_mask_center_of_mass_not_box_middle() {
        let mut hitboxes = HashMap::new();
        hitboxes.insert("avatar".to_string(), crate::types::Hitbox {
            world_x: 400.0,
            world_y: 300.0,
            world_width: 200.0,
            world_height: 400.0,
            world_center_x: 560.0, // box middle is (500, 500)
            world_center_y: 540.0,
            mass: 1000,
            active: true,
            ..crate::types::Hitbox::default()
        });
        let mut config = FireConfig::default();
        config.prewarm_frames = 0;
        let mut eng = FireEngine::new(0.0, 0.0, 0.0, 0.0, "avatar".into(), config, 7);
        eng.step(16.67, 1920.0, 1080.0, &hitboxes, &dummy_slots(), &dummy_masks());
        assert_eq!((eng.target_x, eng.target_y), (560.0, 540.0));
    }

    #[test]
    fn test_fire_spread_is_centered_not_leaning_left() {
        let mut config = FireConfig::default();
        config.prewarm_frames = 0;
        config.stream_mode = "top".to_string();
        let mut eng = FireEngine::new(960.0, 800.0, 960.0, 800.0, "avatar".into(), config, 11);
        for _ in 0..180 {
            eng.step(16.67, 1920.0, 1080.0, &dummy_hitboxes(), &dummy_slots(), &dummy_masks());
        }
        let n = eng.particles.len() as f32;
        assert!(n > 100.0, "expected a full flame, got {n} particles");
        let mean_x = eng.particles.iter().map(|p| p.x).sum::<f32>() / n;
        // The old capped Rng put every particle left of the target (mean ~25px left)
        assert!((mean_x - 960.0).abs() < 8.0, "flame centered at {mean_x}, target 960");
        assert!(eng.particles.iter().any(|p| p.x > 990.0), "no particles right of the target");
        assert!(eng.particles.iter().any(|p| p.x < 930.0), "no particles left of the target");
    }

    #[test]
    fn test_rng_covers_full_unit_range() {
        let mut rng = Rng::new(12345);
        let samples: Vec<f32> = (0..10_000).map(|_| rng.next_f32()).collect();
        let mean = samples.iter().sum::<f32>() / samples.len() as f32;
        assert!((mean - 0.5).abs() < 0.02, "mean {mean}");
        assert!(samples.iter().cloned().fold(0.0, f32::max) > 0.99, "outputs capped below 1");
        assert!(samples.iter().cloned().fold(1.0, f32::min) < 0.01);
    }

    #[test]
    fn test_fire_prewarm_has_particles() {
        let config = FireConfig::default();
        let eng = FireEngine::new(960.0, 800.0, 960.0, 800.0, "avatar".into(), config, 1);
        assert!(eng.particle_count() > 0, "Pre-warmed fire should have particles immediately");
    }

    #[test]
    fn test_fire_color_buf_stride() {
        let config = FireConfig::default();
        let mut eng = FireEngine::new(960.0, 800.0, 960.0, 800.0, "avatar".into(), config, 2);
        eng.step(300.0, 1920.0, 1080.0, &dummy_hitboxes(), &dummy_slots(), &dummy_masks());
        let mut buf = Vec::new();
        eng.fill_render_buf(&mut buf);
        let count = eng.particle_count();
        assert_eq!(buf.len(), count * 9, "Buffer stride must be 9 floats per particle");
    }

    #[test]
    fn test_fire_glow_size_from_the_redeem() {
        let glow = |pairs: &[(&str, serde_json::Value)]| {
            let map: HashMap<String, serde_json::Value> = pairs.iter().map(|(k, v)| (k.to_string(), v.clone())).collect();
            FireConfig::from_json_map(&map).glow_size
        };
        assert_eq!(glow(&[]), 1.0);
        assert_eq!(glow(&[("bloomSize", serde_json::json!(2.5))]), 2.5);
        assert_eq!(glow(&[("bloomSize", serde_json::json!(500.0))]), MAX_GLOW_SIZE);
        assert_eq!(glow(&[("bloomSize", serde_json::json!(3.0)), ("enableBloom", serde_json::json!(false))]), 0.0);

        let mut config = FireConfig::default();
        config.glow_size = 3.5;
        let mut eng = FireEngine::new(960.0, 800.0, 960.0, 800.0, "avatar".into(), config, 4);
        eng.step(300.0, 1920.0, 1080.0, &dummy_hitboxes(), &dummy_slots(), &dummy_masks());
        let (mut wisps, mut sparks) = (Vec::new(), Vec::new());
        eng.fill_render_buf_split(&mut wisps, &mut sparks);
        assert_eq!(wisps.len() + sparks.len(), eng.particle_count() * 10, "10 floats per particle");
        assert!(wisps.chunks_exact(10).chain(sparks.chunks_exact(10)).all(|p| p[9] == 3.5));
    }

    #[test]
    fn test_fire_particles_rise() {
        let mut config = FireConfig::default();
        config.prewarm_frames = 0;
        config.stream_mode = "top".to_string();
        let mut eng = FireEngine::new(960.0, 800.0, 960.0, 800.0, "avatar".into(), config, 3);
        eng.step(50.0, 1920.0, 1080.0, &dummy_hitboxes(), &dummy_slots(), &dummy_masks());
        let y_after_1 = eng.particles.first().map(|p| p.y).unwrap_or(800.0);
        eng.step(50.0, 1920.0, 1080.0, &dummy_hitboxes(), &dummy_slots(), &dummy_masks());
        let y_after_2 = eng.particles.first().map(|p| p.y).unwrap_or(800.0);
        // gravity is negative so y should decrease (rising)
        assert!(y_after_2 < y_after_1, "Fire particles should rise (y decreasing)");
    }

    #[test]
    fn test_fire_config_from_json_map_aliases_and_duration() {
        let mut map = HashMap::new();
        map.insert("fluid_count".to_string(), serde_json::json!(550));
        map.insert("duration".to_string(), serde_json::json!(5.0)); // 5 seconds
        map.insert("gravity".to_string(), serde_json::json!(-2.5)); // should clamp to -1.0
        map.insert("radius".to_string(), serde_json::json!(45.0)); // should clamp to 20.0
        map.insert("offset_y".to_string(), serde_json::json!(120.0));
        map.insert("stream_mode".to_string(), serde_json::json!("grid"));

        let cfg = FireConfig::from_json_map(&map);
        assert_eq!(cfg.particle_count, 550);
        assert_eq!(cfg.duration_ms, 5000.0, "5.0 seconds duration must be converted to 5000.0 ms");
        assert_eq!(cfg.gravity, -1.0, "Gravity must clamp between -1.0 and 0.0");
        assert_eq!(cfg.radius, 20.0, "Radius must not be bigger than 20.0");
        assert_eq!(cfg.offset_y, 120.0, "offset_y must be parsed");
        assert_eq!(cfg.stream_mode, "grid");
    }

    #[test]
    fn test_fire_offset_y_emission_position() {
        let mut config = FireConfig::default();
        config.prewarm_frames = 0;
        config.offset_y = -75.0;
        config.stream_mode = "top".to_string();
        let mut eng = FireEngine::new(960.0, 500.0, 960.0, 500.0, "avatar".into(), config, 42);
        eng.step(16.67, 1920.0, 1080.0, &dummy_hitboxes(), &dummy_slots(), &dummy_masks());
        assert!(!eng.particles.is_empty());
        for p in &eng.particles {
            assert!(p.size <= 20.0, "Particle size must never exceed 20.0");
        }
        let py = eng.particles[0].y;
        // Target was 500.0, offset was -75.0, so initial emit y should be near 425.0 (rising from 425.0)
        assert!(py < 430.0, "Particle should be emitted near target_y + offset_y (425px)");
    }
}
