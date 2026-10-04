// ============================================================================
// Trust — Lightning Bolt Engine
// ============================================================================
// Ports WebGLLightning.js generate2DLightning() into native Rust.
// Uses recursive midpoint sine-wave displacement with branching.
// Outputs flat segment buffer: [x1,y1,x2,y2,r,g,b,alpha,width] per segment.

use crate::fluid::Rng;

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------
#[derive(Debug, Clone)]
pub struct LightningConfig {
    /// How many times the bolt strikes during its lifetime
    pub strike_count: u32,
    /// Parallel main bolt strands per strike
    pub bolt_count: u32,
    /// Total lifetime in ms (strikes evenly spread within)
    pub duration_ms: f32,
    /// Main bolt width in pixels
    pub bolt_width: f32,
    /// 0.0 = very jagged, 1.0 = very rough/detailed
    pub roughness: f32,
    /// 0.0 = wild zigzag, 1.0 = perfectly straight
    pub straightness: f32,
    /// Number of branch sub-bolts
    pub ramification: u32,
    /// Iterations of recursive subdivision (more = more detailed)
    pub max_iterations: u32,
    /// Number of points per segment
    pub segments_per_bolt: u32,
    /// Main bolt color [0–1]
    pub color: [f32; 3],
    pub color_a: f32,
    /// Outer glow color [0–1]
    pub glow_color: [f32; 3],
    pub glow_size: f32,
    /// Enable additive glow pass
    pub enable_glow: bool,
    /// Smoothness / FPS for lightning animation updates (default: 30.0, range: 5.0 to 120.0)
    pub smoothness: f32,
    /// Enable impact flash bloom circle at contact point
    pub enable_impact: bool,
    /// Relative size multiplier for impact flash
    pub impact_size: f32,
    /// Optional Screen Shake fired when the strike is triggered (no per-flash
    /// impact wiring yet — see `ShakeTiming`).
    pub shake: crate::physics::ShakeConfig,
    /// Optional avatar reaction fired when the strike is triggered (see `shake` above).
    pub impact_reaction: crate::physics::ImpactReactionConfig,
}

impl Default for LightningConfig {
    fn default() -> Self {
        Self {
            strike_count: 3,
            bolt_count: 2,
            duration_ms: 500.0,
            bolt_width: 4.0,
            roughness: 0.85,
            straightness: 0.6,
            ramification: 3,
            max_iterations: 5,
            segments_per_bolt: 16,
            color: [0.706, 0.863, 1.0],  // pale blue-white
            color_a: 1.0,
            glow_color: [0.706, 0.863, 1.0],
            glow_size: 1.0,
            enable_glow: true,
            smoothness: 30.0,
            enable_impact: true,
            impact_size: 1.0,
            shake: crate::physics::ShakeConfig::default(),
            impact_reaction: crate::physics::ImpactReactionConfig::default(),
        }
    }
}

impl LightningConfig {
    pub fn from_json_map(m: &std::collections::HashMap<String, serde_json::Value>) -> Self {
        let mut c = Self::default();
        if let Some(v) = m.get("strikeCount").and_then(|v| v.as_f64()) { c.strike_count = v as u32; }
        if let Some(v) = m.get("boltCount").and_then(|v| v.as_f64()) { c.bolt_count = v as u32; }
        if let Some(v) = m.get("duration").and_then(|v| v.as_f64()) { c.duration_ms = v as f32; }
        if let Some(v) = m.get("boltWidth").and_then(|v| v.as_f64()) { c.bolt_width = v as f32; }
        if let Some(v) = m.get("roughness").and_then(|v| v.as_f64()) { c.roughness = v as f32; }
        if let Some(v) = m.get("straightness").and_then(|v| v.as_f64()) { c.straightness = v as f32; }
        if let Some(v) = m.get("branching").and_then(|v| v.as_f64()) { c.ramification = v as u32; }
        if let Some(v) = m.get("maxIterations").and_then(|v| v.as_f64()) { c.max_iterations = v as u32; }
        if let Some(v) = m.get("colorR").and_then(|v| v.as_f64()) { c.color[0] = v as f32 / 255.0; }
        if let Some(v) = m.get("colorG").and_then(|v| v.as_f64()) { c.color[1] = v as f32 / 255.0; }
        if let Some(v) = m.get("colorB").and_then(|v| v.as_f64()) { c.color[2] = v as f32 / 255.0; }
        if let Some(v) = m.get("colorA").and_then(|v| v.as_f64()) { c.color_a = v as f32; }
        if let Some(v) = m.get("bloomSize").and_then(|v| v.as_f64()) { c.glow_size = v as f32; }
        if let Some(v) = m.get("enableBloom").and_then(|v| v.as_bool()) { c.enable_glow = v; }
        if let Some(v) = m.get("smoothness").and_then(|v| v.as_f64()) { c.smoothness = v as f32; }
        else if let Some(v) = m.get("fps").and_then(|v| v.as_f64()) { c.smoothness = v as f32; }
        if let Some(v) = m.get("enableImpact").and_then(|v| v.as_bool()) { c.enable_impact = v; }
        else if let Some(v) = m.get("enableImpactFlash").and_then(|v| v.as_bool()) { c.enable_impact = v; }
        if let Some(v) = m.get("impactSize").and_then(|v| v.as_f64()) { c.impact_size = v as f32; }
        // Derive glow color from main color
        c.glow_color = c.color;
        c.shake = crate::physics::parse_shake_config(m);
        c.impact_reaction = crate::physics::ImpactReactionConfig::parse(m.get("impactReactions"));
        c
    }
}

// ---------------------------------------------------------------------------
// A generated bolt = list of (x1, y1, x2, y2, width_factor) segments
// ---------------------------------------------------------------------------
#[derive(Debug, Clone)]
pub struct BoltSegment {
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
    /// Relative to config.bolt_width  (main bolt = 1.0, branches < 1.0)
    pub width_factor: f32,
    /// 0.0–1.0 fade for glow passes
    pub alpha: f32,
}

// ---------------------------------------------------------------------------
// Engine
// ---------------------------------------------------------------------------
pub struct LightningEngine {
    pub config: LightningConfig,

    pub start_x: f32,
    pub start_y: f32,
    pub target_x: f32,
    pub target_y: f32,
    pub target_name: String,

    pub elapsed_ms: f32,
    pub dead: bool,

    /// Currently active bolt segments (regenerated each flicker). Core bolt/branch
    /// segments come first, followed by the wider low-alpha glow duplicates.
    segments: Vec<BoltSegment>,
    /// Number of entries at the front of `segments` that are core (non-glow) segments.
    core_segment_count: usize,
    /// Time within current strike cycle
    strike_elapsed_ms: f32,
    /// Index of current strike (0..strike_count)
    strike_index: u32,
    /// How long each individual strike lasts before regenerating
    strike_duration_ms: f32,
    /// Accumulated time for bolt regeneration flicker (~80ms intervals)
    flicker_elapsed_ms: f32,
    /// Current overall fade alpha
    pub fade_alpha: f32,

    rng: Rng,
    noise_seed: f32,
}

impl LightningEngine {
    pub fn new(
        start_x: f32,
        start_y: f32,
        target_x: f32,
        target_y: f32,
        target_name: String,
        config: LightningConfig,
        seed: u64,
    ) -> Self {
        let strike_duration_ms = config.duration_ms / config.strike_count.max(1) as f32;
        let noise_seed = (seed % 1000) as f32 / 1000.0;
        let mut eng = Self {
            config,
            start_x,
            start_y,
            target_x,
            target_y,
            target_name,
            elapsed_ms: 0.0,
            dead: false,
            segments: Vec::new(),
            core_segment_count: 0,
            strike_elapsed_ms: 0.0,
            strike_index: 0,
            strike_duration_ms,
            flicker_elapsed_ms: 0.0,
            fade_alpha: 1.0,
            rng: Rng::new(seed),
            noise_seed,
        };
        // Generate initial bolt
        eng.regenerate();
        eng
    }

    // -----------------------------------------------------------------------
    // Core bolt generation (port of WebGLLightning.js generate2DLightning)
    // -----------------------------------------------------------------------
    fn generate_path(
        &self,
        x1: f32, y1: f32,
        x2: f32, y2: f32,
        num_segments: u32,
        iteration: u32,
        width_factor: f32,
        time: f32,
        fade_alpha: f32,
        seed: f32,
        out: &mut Vec<BoltSegment>,
        rng: &mut Rng,
    ) {
        let dx = x2 - x1;
        let dy = y2 - y1;
        let dist = (dx * dx + dy * dy).sqrt().max(1.0);

        let mut points: Vec<(f32, f32)> = Vec::with_capacity(num_segments as usize + 1);

        for k in 0..=num_segments {
            let t = k as f32 / num_segments as f32;
            let mut px = x1 + dx * t;
            let mut py = y1 + dy * t;

            if k > 0 && k < num_segments {
                let envelope = (t * std::f32::consts::PI).sin();
                let max_stray = dist * (1.0 - self.config.straightness) * 0.25;

                // Low-frequency main wave
                let main_noise = (t * std::f32::consts::PI * 2.0
                    + time * 15.0
                    + seed * 100.0
                    + iteration as f32 * 5.0).sin();
                let main_disp = main_noise * max_stray * envelope;

                // High-frequency detail (3 octaves)
                let mut detail_disp = 0.0f32;
                let mut detail_amp = self.config.roughness * max_stray * 0.45;
                let mut freq = 6.0 * std::f32::consts::PI;
                for octave in 0..3u32 {
                    let noise_val = (t * freq
                        + time * (30.0 + octave as f32 * 10.0)
                        + seed * (200.0 + octave as f32 * 70.0)).sin();
                    detail_disp += noise_val * detail_amp * envelope;
                    detail_amp *= self.config.roughness * 0.5;
                    freq *= 2.0;
                }

                // Perpendicular displacement
                let nx = -dy / dist;
                let ny = dx / dist;
                let total_disp = main_disp + detail_disp;
                px += nx * total_disp;
                py += ny * total_disp;
            }

            points.push((px, py));
        }

        // Convert point list → segments
        for i in 0..points.len().saturating_sub(1) {
            let (ax, ay) = points[i];
            let (bx, by) = points[i + 1];
            out.push(BoltSegment {
                x1: ax, y1: ay,
                x2: bx, y2: by,
                width_factor,
                alpha: fade_alpha,
            });
        }

        // Branching (max 2 recursive levels, matching JS)
        if self.config.ramification > 0 && iteration < 2 {
            let step = (points.len() / (self.config.ramification as usize + 1)).max(2);
            let mut j = step;
            while j < points.len().saturating_sub(step) {
                let branch_noise = (j as f32 * 3.1 + time * 5.0 + seed * 50.0).sin();
                if branch_noise > 0.1 {
                    let (bsx, bsy) = points[j];
                    let next_j = (j + 1).min(points.len() - 1);
                    let (nx2, ny2) = points[next_j];

                    let bdx = nx2 - bsx;
                    let bdy = ny2 - bsy;
                    let blen = (bdx * bdx + bdy * bdy).sqrt().max(1.0);

                    let angle = if (j as f32 + time).sin() > 0.0 { 1.0f32 } else { -1.0 }
                        * (0.4 + rng.next_f32() * 0.4);
                    let cos_a = angle.cos();
                    let sin_a = angle.sin();
                    let rot_dx = (bdx * cos_a - bdy * sin_a) / blen;
                    let rot_dy = (bdx * sin_a + bdy * cos_a) / blen;

                    let rem_dist = ((x2 - bsx) * (x2 - bsx) + (y2 - bsy) * (y2 - bsy)).sqrt();
                    let branch_len = rem_dist * (0.3 + rng.next_f32() * 0.3);

                    let end_x = bsx + rot_dx * branch_len;
                    let end_y = bsy + rot_dy * branch_len;

                    self.generate_path(
                        bsx, bsy, end_x, end_y,
                        (num_segments / 2).max(4),
                        iteration + 1,
                        width_factor * 0.5,
                        time,
                        fade_alpha,
                        seed,
                        out,
                        rng,
                    );
                }
                j += step;
            }
        }
    }


    fn regenerate(&mut self) {
        let time = self.elapsed_ms / 1000.0;
        let fade_alpha = self.fade_alpha;
        let glow_size = self.config.glow_size;
        let enable_glow = self.config.enable_glow;
        let bolt_count = self.config.bolt_count;
        let segments_per_bolt = self.config.segments_per_bolt;
        let start_x = self.start_x;
        let start_y = self.start_y;
        let target_x = self.target_x;
        let target_y = self.target_y;

        let mut new_segments: Vec<BoltSegment> = Vec::new();

        for bolt_i in 0..bolt_count {
            let seed = self.noise_seed + bolt_i as f32 * 0.37;
            let mut rng_copy = Rng::new(
                (self.rng.next_f32() * u32::MAX as f32) as u64 + bolt_i as u64 + 1,
            );
            let spread_x = (bolt_i as f32 - bolt_count as f32 * 0.5) * 15.0;

            self.generate_path(
                start_x + spread_x,
                start_y,
                target_x + spread_x * 0.3,
                target_y,
                segments_per_bolt,
                0,
                1.0,
                time,
                fade_alpha,
                seed,
                &mut new_segments,
                &mut rng_copy,
            );
        }

        // Glow pass: duplicate with larger width, lower alpha. Rendered additively
        // by the caller (see obs_source.rs::render_throwable_layer) to actually
        // bloom/brighten instead of just washing out as a translucent overlay.
        self.core_segment_count = new_segments.len();
        if enable_glow {
            let glow_segs: Vec<BoltSegment> = new_segments.iter().map(|s| BoltSegment {
                x1: s.x1, y1: s.y1,
                x2: s.x2, y2: s.y2,
                width_factor: s.width_factor * glow_size * 3.0,
                alpha: s.alpha * 0.2,
            }).collect();
            new_segments.extend(glow_segs);
        }

        self.segments = new_segments;
    }



    // -----------------------------------------------------------------------
    pub fn step(
        &mut self,
        dt_ms: f32,
        _canvas_w: f32,
        _canvas_h: f32,
        updated_target: Option<(f32, f32)>,
    ) {
        if self.dead {
            return;
        }

        if let Some((tx, ty)) = updated_target {
            self.target_x = tx;
            self.target_y = ty;
        }

        self.elapsed_ms += dt_ms;
        self.strike_elapsed_ms += dt_ms;
        self.flicker_elapsed_ms += dt_ms;

        // Fade out near end of each strike
        let strike_frac = (self.strike_elapsed_ms / self.strike_duration_ms).clamp(0.0, 1.0);
        self.fade_alpha = if strike_frac < 0.1 {
            strike_frac / 0.1 // fade in
        } else if strike_frac > 0.7 {
            1.0 - (strike_frac - 0.7) / 0.3 // fade out
        } else {
            1.0
        };

        // Smoothness-controlled flicker interval (e.g. 60 FPS -> 16.6ms, 30 FPS -> 33.3ms, 12 FPS -> 83.3ms)
        let flicker_interval_ms = 1000.0 / self.config.smoothness.clamp(5.0, 120.0);
        if self.flicker_elapsed_ms >= flicker_interval_ms {
            self.flicker_elapsed_ms = 0.0;
            self.regenerate();
        } else {
            // Update alpha in existing segments
            for s in &mut self.segments {
                s.alpha = self.fade_alpha;
            }
        }

        // Advance to next strike when current one ends
        if self.strike_elapsed_ms >= self.strike_duration_ms {
            self.strike_elapsed_ms = 0.0;
            self.strike_index += 1;
            if self.strike_index >= self.config.strike_count {
                self.dead = true;
                self.segments.clear();
            } else {
                self.regenerate();
            }
        }
    }

    /// Export: [x1, y1, x2, y2, r, g, b, alpha, width]  (9 f32 per segment)
    pub fn fill_segment_buf(&self, buf: &mut Vec<f32>) {
        let [r, g, b] = self.config.color;
        let base_width = self.config.bolt_width;
        for s in &self.segments {
            buf.push(s.x1);
            buf.push(s.y1);
            buf.push(s.x2);
            buf.push(s.y2);
            buf.push(r);
            buf.push(g);
            buf.push(b);
            buf.push(s.alpha * self.config.color_a);
            buf.push(base_width * s.width_factor);
        }
    }

    pub fn segment_count(&self) -> usize {
        self.segments.len()
    }

    /// Like `fill_segment_buf`, but splits core bolt/branch segments from the
    /// wider glow duplicates so the caller can render the glow pass additively.
    pub fn fill_segment_buf_split(&self, core_buf: &mut Vec<f32>, glow_buf: &mut Vec<f32>) {
        let [r, g, b] = self.config.color;
        let base_width = self.config.bolt_width;
        for (i, s) in self.segments.iter().enumerate() {
            let buf = if i < self.core_segment_count { &mut *core_buf } else { &mut *glow_buf };
            buf.push(s.x1);
            buf.push(s.y1);
            buf.push(s.x2);
            buf.push(s.y2);
            buf.push(r);
            buf.push(g);
            buf.push(b);
            buf.push(s.alpha * self.config.color_a);
            buf.push(base_width * s.width_factor);
        }
    }
}

// ---------------------------------------------------------------------------
// Unit Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lightning_generates_segments() {
        let config = LightningConfig::default();
        let eng = LightningEngine::new(100.0, 100.0, 500.0, 500.0, "avatar".into(), config, 42);
        assert!(eng.segment_count() > 0, "Should generate segments immediately");
    }

    #[test]
    fn test_lightning_segment_buf_stride() {
        let config = LightningConfig::default();
        let eng = LightningEngine::new(100.0, 100.0, 500.0, 500.0, "avatar".into(), config, 1);
        let mut buf = Vec::new();
        eng.fill_segment_buf(&mut buf);
        assert_eq!(buf.len(), eng.segment_count() * 9, "Segment buffer stride must be 9 floats");
    }

    #[test]
    fn test_lightning_dies_after_strikes() {
        let mut config = LightningConfig::default();
        config.strike_count = 2;
        config.duration_ms = 100.0;
        let mut eng = LightningEngine::new(100.0, 100.0, 500.0, 500.0, "avatar".into(), config, 5);
        for _ in 0..200 {
            if eng.dead { break; }
            eng.step(16.0, 1920.0, 1080.0, None);
        }
        assert!(eng.dead, "Lightning should be dead after all strikes complete");
    }

    #[test]
    fn test_lightning_flickering_regenerates() {
        let config = LightningConfig::default();
        let mut eng = LightningEngine::new(100.0, 100.0, 500.0, 500.0, "avatar".into(), config, 9);
        let segs_before = eng.segment_count();
        // Advance past flicker interval (80ms)
        eng.step(90.0, 1920.0, 1080.0, None);
        let segs_after = eng.segment_count();
        // After regeneration there should still be segments
        assert!(segs_after > 0, "After flicker, bolt should still exist: before={segs_before} after={segs_after}");
    }
}
