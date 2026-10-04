use crate::types::{Hitbox, Point2D, SourceTransform, TrackingConfig};

/// Standard focal distance for 2.5D per-target perspective projection matching OBS.
pub const PERSPECTIVE_D: f32 = 1000.0;

/// In-memory binary mask for granular pixel-perfect collision tests.
#[derive(Debug, Clone)]
pub struct CollisionMask {
    pub width: u32,
    pub height: u32,
    pub transform: SourceTransform,
    pub pitch: f32,
    pub yaw: f32,
    pub mask: Vec<bool>,
    /// Center of mass of the solid pixels in local texture coordinates, set by
    /// the scan. Kept so `reproject_hitbox` can re-derive the world-space
    /// center of mass after a move/scale/rotate without rescanning pixels.
    pub local_center: Option<(f32, f32)>,
}

impl CollisionMask {
    pub fn new(width: u32, height: u32, transform: SourceTransform) -> Self {
        let size = (width * height) as usize;
        Self {
            width,
            height,
            transform,
            pitch: 0.0,
            yaw: 0.0,
            mask: vec![false; size],
            local_center: None,
        }
    }

    pub fn new_3d(width: u32, height: u32, transform: SourceTransform, pitch: f32, yaw: f32) -> Self {
        let size = (width * height) as usize;
        Self {
            width,
            height,
            transform,
            pitch,
            yaw,
            mask: vec![false; size],
            local_center: None,
        }
    }

    #[inline(always)]
    pub fn is_solid_local(&self, x: u32, y: u32) -> bool {
        if x < self.width && y < self.height {
            let idx = (y * self.width + x) as usize;
            self.mask.get(idx).copied().unwrap_or(false)
        } else {
            false
        }
    }

    #[inline(always)]
    pub fn is_solid_world(&self, world_x: f32, world_y: f32) -> bool {
        if self.transform.scale_x.abs() < f32::EPSILON || self.transform.scale_y.abs() < f32::EPSILON {
            return false;
        }

        let eff_w = self.width as f32 * self.transform.scale_x;
        let eff_h = self.height as f32 * self.transform.scale_y;
        let cx = self.transform.pos_x + eff_w / 2.0;
        let cy = self.transform.pos_y + eff_h / 2.0;

        let dx = world_x - cx;
        let dy = world_y - cy;

        let rx = self.pitch.to_radians();
        let ry = self.yaw.to_radians();
        let rz = self.transform.rot_deg.to_radians();

        let cos_rx = rx.cos();
        let sin_rx = rx.sin();
        let cos_ry = ry.cos();
        let sin_ry = ry.sin();
        let cos_rz = rz.cos();
        let sin_rz = rz.sin();

        // 3D basis vectors u (column for lx) and v (column for ly)
        let ux = cos_ry * cos_rz - sin_ry * sin_rx * sin_rz;
        let uy = cos_ry * sin_rz + sin_ry * sin_rx * cos_rz;
        let uz = -sin_ry * cos_rx;

        let vx = -cos_rx * sin_rz;
        let vy = cos_rx * cos_rz;
        let vz = sin_rx;

        // 2x2 linear system for perspective unprojection:
        let a00 = ux - (dx / PERSPECTIVE_D) * uz;
        let a01 = vx - (dx / PERSPECTIVE_D) * vz;
        let a10 = uy - (dy / PERSPECTIVE_D) * uz;
        let a11 = vy - (dy / PERSPECTIVE_D) * vz;

        let det = a00 * a11 - a01 * a10;
        if det.abs() < 1e-6 {
            return false;
        }

        let lx = (dx * a11 - dy * a01) / det;
        let ly = (a00 * dy - a10 * dx) / det;

        let local_x = (lx + eff_w / 2.0) / self.transform.scale_x + self.transform.crop_left as f32;
        let local_y = (ly + eff_h / 2.0) / self.transform.scale_y + self.transform.crop_top as f32;

        if local_x >= 0.0 && local_y >= 0.0 {
            self.is_solid_local(local_x as u32, local_y as u32)
        } else {
            false
        }
    }

    #[inline(always)]
    pub fn set_solid_local(&mut self, x: u32, y: u32, solid: bool) {
        if x < self.width && y < self.height {
            let idx = (y * self.width + x) as usize;
            if idx < self.mask.len() {
                self.mask[idx] = solid;
            }
        }
    }

    /// Render the binary collision mask into an RGBA pixel buffer.
    pub fn render_rgba(
        &self,
        out_buf: &mut [u8],
        stride: usize,
        solid_color: [u8; 4],
        bg_color: [u8; 4],
    ) {
        let w = self.width as usize;
        let h = self.height as usize;
        for y in 0..h {
            let row_start = y * stride;
            let mask_row_start = y * w;
            for x in 0..w {
                let is_solid = self.mask[mask_row_start + x];
                let col = if is_solid { solid_color } else { bg_color };
                let px_start = row_start + x * 4;
                if px_start + 4 <= out_buf.len() {
                    out_buf[px_start] = col[0];
                    out_buf[px_start + 1] = col[1];
                    out_buf[px_start + 2] = col[2];
                    out_buf[px_start + 3] = col[3];
                }
            }
        }
    }
}

/// Convert an RGB color to HSV (H: 0-360, S: 0-1, V: 0-1)
#[inline(always)]
fn rgb_to_hsv(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let rf = r as f32 / 255.0;
    let gf = g as f32 / 255.0;
    let bf = b as f32 / 255.0;

    let max = rf.max(gf).max(bf);
    let min = rf.min(gf).min(bf);
    let delta = max - min;

    let v = max;
    let s = if max > 0.0 { delta / max } else { 0.0 };

    let h = if delta == 0.0 {
        0.0
    } else if (max - rf).abs() < f32::EPSILON {
        60.0 * (((gf - bf) / delta) % 6.0)
    } else if (max - gf).abs() < f32::EPSILON {
        60.0 * (((bf - rf) / delta) + 2.0)
    } else {
        60.0 * (((rf - gf) / delta) + 4.0)
    };

    let h = if h < 0.0 { h + 360.0 } else { h };
    (h, s, v)
}

/// Evaluates whether a pixel matches the tracking criteria.
#[inline(always)]
pub fn matches_pixel(r: u8, g: u8, b: u8, a: u8, config: &TrackingConfig) -> bool {
    match config.mode {
        0 => {
            // Mode 0: Alpha Transparency
            // Scanned directly on the source's individual layer (cleared to transparent black [0,0,0,0]).
            // Any pixel with alpha >= alpha_threshold is solid, including pure black pixels.
            a >= config.alpha_threshold
        }
        1 => {
            // Mode 1: Chroma HSV Keying
            if a < config.alpha_threshold {
                return false;
            }
            let (target_h, target_s, _) = rgb_to_hsv(config.chroma_r, config.chroma_g, config.chroma_b);
            let (h, s, v) = rgb_to_hsv(r, g, b);

            if s < 0.15 || v < 0.15 {
                return true;
            }

            let mut diff_h = (h - target_h).abs();
            if diff_h > 180.0 {
                diff_h = 360.0 - diff_h;
            }

            let h_threshold = (config.threshold as f32 / 255.0) * 90.0;
            !(diff_h <= h_threshold && (s - target_s).abs() <= 0.6)
        }
        2 => {
            // Mode 2: Brightness Luminance Threshold
            if a < config.alpha_threshold {
                return false;
            }
            let lum = (r as u32 * 299 + g as u32 * 587 + b as u32 * 114) / 1000;
            lum >= config.threshold as u32
        }
        _ => a >= config.alpha_threshold,
    }
}

#[inline(always)]
pub fn project_3d_canvas(
    lx: f32,
    ly: f32,
    cx: f32,
    cy: f32,
    roll: f32,
    pitch: f32,
    yaw: f32,
) -> (f32, f32) {
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

    // 2.5D Per-Target Perspective Projection matching OBS
    let k = PERSPECTIVE_D / (PERSPECTIVE_D + z3).max(10.0);
    (cx + x3 * k, cy + y3 * k)
}

/// The inverse of `project_3d_canvas`: the offset from the target's center,
/// in its own frame (before its rotation), that lands on canvas point
/// (px, py). Exact for roll alone; with pitch/yaw it's refined from there,
/// which holds while the target isn't turned edge-on.
pub fn unproject_3d_canvas(px: f32, py: f32, cx: f32, cy: f32, roll: f32, pitch: f32, yaw: f32) -> (f32, f32) {
    let (sin, cos) = roll.to_radians().sin_cos();
    let unroll = |dx: f32, dy: f32| (dx * cos + dy * sin, -dx * sin + dy * cos);
    let (mut lx, mut ly) = unroll(px - cx, py - cy);
    for _ in 0..12 {
        let (qx, qy) = project_3d_canvas(lx, ly, cx, cy, roll, pitch, yaw);
        let (ex, ey) = unroll(px - qx, py - qy);
        if ex.abs() < 0.01 && ey.abs() < 0.01 {
            break;
        }
        lx += ex;
        ly += ey;
    }
    (lx, ly)
}

/// Most points in a target's pixel grid.
pub const PIXEL_GRID_MAX: usize = 2500;

/// Which solid pixels (by their index in scan order) make a target's pixel
/// grid: all of them up to `max_points`, else `max_points` spread evenly.
fn pixel_grid_picks(count: usize, max_points: usize) -> Vec<usize> {
    if count <= max_points {
        return (0..count).collect();
    }
    let step = count as f32 / max_points as f32;
    (0..max_points).map(|i| ((i as f32 * step) as usize).min(count - 1)).collect()
}

/// Scans an RGBA pixel buffer and computes local hitbox, 3D Euler perspective world-transformed hitbox, and pixel grid point cloud.
pub fn scan_rgba_with_3d_transform(
    data: &[u8],
    width: u32,
    height: u32,
    stride: u32,
    transform: &SourceTransform,
    pitch: f32,
    yaw: f32,
    config: &TrackingConfig,
) -> (Hitbox, CollisionMask, Vec<Point2D>) {
    let mut min_x = width;
    let mut max_x = 0;
    let mut min_y = height;
    let mut max_y = 0;
    let mut sum_x: u64 = 0;
    let mut sum_y: u64 = 0;
    let mut count: u32 = 0;

    let mut collision_mask = CollisionMask::new_3d(width, height, *transform, pitch, yaw);

    // Effective dimensions and center of rotation
    let eff_w = (width.saturating_sub(transform.crop_left + transform.crop_right)) as f32 * transform.scale_x;
    let eff_h = (height.saturating_sub(transform.crop_top + transform.crop_bottom)) as f32 * transform.scale_y;
    let cx = transform.pos_x + eff_w / 2.0;
    let cy = transform.pos_y + eff_h / 2.0;

    // Apply crop boundaries
    let crop_x_min = transform.crop_left.min(width);
    let crop_x_max = width.saturating_sub(transform.crop_right);
    let crop_y_min = transform.crop_top.min(height);
    let crop_y_max = height.saturating_sub(transform.crop_bottom);

    // Full 1:1 pixel scan across the entire buffer
    for y in crop_y_min..crop_y_max {
        let row_offset = (y * stride) as usize;
        for x in crop_x_min..crop_x_max {
            let idx = row_offset + (x * 4) as usize;
            if idx + 3 < data.len() {
                let r = data[idx];
                let g = data[idx + 1];
                let b = data[idx + 2];
                let a = data[idx + 3];

                if matches_pixel(r, g, b, a, config) {
                    collision_mask.set_solid_local(x, y, true);
                    if x < min_x { min_x = x; }
                    if x > max_x { max_x = x; }
                    if y < min_y { min_y = y; }
                    if y > max_y { max_y = y; }

                    sum_x += x as u64;
                    sum_y += y as u64;
                    count += 1;
                }
            }
        }
    }

    if count > 0 && max_x >= min_x && max_y >= min_y {
        let local_w = (max_x - min_x + 1) as f32;
        let local_h = (max_y - min_y + 1) as f32;
        let local_cx = (sum_x as f64 / count as f64) as f32;
        let local_cy = (sum_y as f64 / count as f64) as f32;
        collision_mask.local_center = Some((local_cx, local_cy));

        // Calculate world envelope bounding box by projecting all 4 tight local box corners in 3D
        let corners_local = [
            (min_x as f32, min_y as f32),
            ((max_x + 1) as f32, min_y as f32),
            (min_x as f32, (max_y + 1) as f32),
            ((max_x + 1) as f32, (max_y + 1) as f32),
        ];

        let mut world_min_x = f32::MAX;
        let mut world_max_x = f32::MIN;
        let mut world_min_y = f32::MAX;
        let mut world_max_y = f32::MIN;

        for (lx_loc, ly_loc) in corners_local {
            let ux = (lx_loc - transform.crop_left as f32) * transform.scale_x;
            let uy = (ly_loc - transform.crop_top as f32) * transform.scale_y;
            let lx = ux - eff_w / 2.0;
            let ly = uy - eff_h / 2.0;
            let (wx, wy) = project_3d_canvas(lx, ly, cx, cy, transform.rot_deg, pitch, yaw);
            if wx < world_min_x { world_min_x = wx; }
            if wx > world_max_x { world_max_x = wx; }
            if wy < world_min_y { world_min_y = wy; }
            if wy > world_max_y { world_max_y = wy; }
        }

        // Center of Mass with 3D projection
        let ucx = (local_cx - transform.crop_left as f32) * transform.scale_x;
        let ucy = (local_cy - transform.crop_top as f32) * transform.scale_y;
        let (world_cx, world_cy) = project_3d_canvas(ucx - eff_w / 2.0, ucy - eff_h / 2.0, cx, cy, transform.rot_deg, pitch, yaw);

        // Dense Pixel Grid Budget for rich, solid mask coverage: stream-space
        // points (3D rotation and perspective) for up to PIXEL_GRID_MAX solid
        // pixels spread evenly in scan order. Only those are projected, found
        // by walking the mask again: projecting every solid pixel (six trig
        // calls each) and then keeping 2500 of them took tens of milliseconds
        // per scan of a large target, on OBS's render thread, every frame.
        let picks = pixel_grid_picks(count as usize, PIXEL_GRID_MAX);
        let mut pixel_grid = Vec::with_capacity(picks.len());
        let mut next = 0;
        let mut solid_index = 0usize;
        'rows: for y in crop_y_min..crop_y_max {
            for x in crop_x_min..crop_x_max {
                if !collision_mask.is_solid_local(x, y) {
                    continue;
                }
                while next < picks.len() && picks[next] == solid_index {
                    let ux = (x as f32 - transform.crop_left as f32) * transform.scale_x;
                    let uy = (y as f32 - transform.crop_top as f32) * transform.scale_y;
                    let (world_px, world_py) = project_3d_canvas(ux - eff_w / 2.0, uy - eff_h / 2.0, cx, cy, transform.rot_deg, pitch, yaw);
                    pixel_grid.push(Point2D { x: world_px, y: world_py });
                    next += 1;
                }
                if next == picks.len() {
                    break 'rows;
                }
                solid_index += 1;
            }
        }

        let hitbox = Hitbox {
            local_x: min_x as f32,
            local_y: min_y as f32,
            local_width: local_w,
            local_height: local_h,
            world_x: world_min_x,
            world_y: world_min_y,
            world_width: (world_max_x - world_min_x).max(1.0),
            world_height: (world_max_y - world_min_y).max(1.0),
            world_center_x: world_cx,
            world_center_y: world_cy,
            mass: count,
            active: true,
        };
        (hitbox, collision_mask, pixel_grid)
    } else {
        (Hitbox::default(), collision_mask, Vec::new())
    }
}

/// Re-derives a hitbox's world envelope and center of mass from its mask's
/// current transform (pos/scale/rotation/crop + pitch/yaw) — the same
/// projection `scan_rgba_with_3d_transform` applies, without rescanning any
/// pixels: moving, scaling or rotating a source doesn't change which pixels
/// are solid, so the local tight box and local center of mass still hold.
/// Keeps the target point on the mask's center of mass instead of falling
/// back to the middle of the source rectangle whenever the source moves.
pub fn reproject_hitbox(hb: &mut Hitbox, mask: &CollisionMask) {
    if hb.local_width <= 0.0 || hb.local_height <= 0.0 {
        return;
    }
    let t = &mask.transform;
    let eff_w = (mask.width.saturating_sub(t.crop_left + t.crop_right)) as f32 * t.scale_x;
    let eff_h = (mask.height.saturating_sub(t.crop_top + t.crop_bottom)) as f32 * t.scale_y;
    let cx = t.pos_x + eff_w / 2.0;
    let cy = t.pos_y + eff_h / 2.0;
    let project = |lx_loc: f32, ly_loc: f32| {
        let ux = (lx_loc - t.crop_left as f32) * t.scale_x;
        let uy = (ly_loc - t.crop_top as f32) * t.scale_y;
        project_3d_canvas(ux - eff_w / 2.0, uy - eff_h / 2.0, cx, cy, t.rot_deg, mask.pitch, mask.yaw)
    };

    // local_width = max_x - min_x + 1, so these are the scan's (max + 1) pixel edges
    let (x0, y0) = (hb.local_x, hb.local_y);
    let (x1, y1) = (hb.local_x + hb.local_width, hb.local_y + hb.local_height);
    let mut world_min_x = f32::MAX;
    let mut world_max_x = f32::MIN;
    let mut world_min_y = f32::MAX;
    let mut world_max_y = f32::MIN;
    for (lx_loc, ly_loc) in [(x0, y0), (x1, y0), (x0, y1), (x1, y1)] {
        let (wx, wy) = project(lx_loc, ly_loc);
        world_min_x = world_min_x.min(wx);
        world_max_x = world_max_x.max(wx);
        world_min_y = world_min_y.min(wy);
        world_max_y = world_max_y.max(wy);
    }

    // Masks built outside the scan have no recorded center of mass; the tight box middle is the best stand-in
    let (lcx, lcy) = mask.local_center.unwrap_or((x0 + hb.local_width * 0.5, y0 + hb.local_height * 0.5));
    let (world_cx, world_cy) = project(lcx, lcy);

    hb.world_x = world_min_x;
    hb.world_y = world_min_y;
    hb.world_width = (world_max_x - world_min_x).max(1.0);
    hb.world_height = (world_max_y - world_min_y).max(1.0);
    hb.world_center_x = world_cx;
    hb.world_center_y = world_cy;
}

/// Backward-compatible 2D scan wrapper
pub fn scan_rgba_with_transform(
    data: &[u8],
    width: u32,
    height: u32,
    stride: u32,
    transform: &SourceTransform,
    config: &TrackingConfig,
) -> (Hitbox, CollisionMask, Vec<Point2D>) {
    scan_rgba_with_3d_transform(data, width, height, stride, transform, 0.0, 0.0, config)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// 200x100 source whose solid pixels form an asymmetric "L" (a tall bar on
    /// the right plus a foot along the bottom), so its center of mass sits well
    /// away from both the source's middle and its tight box's middle.
    pub(crate) fn asymmetric_l_source() -> (Vec<u8>, u32, u32) {
        let (w, h) = (200u32, 100u32);
        let mut px = vec![0u8; (w * h * 4) as usize];
        for (x0, y0, x1, y1) in [(150u32, 10u32, 190u32, 95u32), (60, 80, 150, 95)] {
            for y in y0..y1 {
                for x in x0..x1 {
                    let i = ((y * w + x) * 4) as usize;
                    px[i..i + 4].copy_from_slice(&[255, 255, 255, 255]);
                }
            }
        }
        (px, w, h)
    }

    fn transform(pos_x: f32, pos_y: f32, scale: f32, rot_deg: f32) -> SourceTransform {
        SourceTransform { pos_x, pos_y, scale_x: scale, scale_y: scale, rot_deg, ..SourceTransform::default() }
    }

    fn assert_close(a: f32, b: f32, what: &str) {
        assert!((a - b).abs() < 0.01, "{what}: {a} vs {b}");
    }

    /// The pixel grid as the scan used to build it: every solid pixel
    /// projected, then PIXEL_GRID_MAX of them kept, evenly spread.
    fn pixel_grid_the_old_way(px: &[u8], w: u32, h: u32, t: &SourceTransform, pitch: f32, yaw: f32, cfg: &TrackingConfig) -> Vec<Point2D> {
        let eff_w = w as f32 * t.scale_x;
        let eff_h = h as f32 * t.scale_y;
        let (cx, cy) = (t.pos_x + eff_w / 2.0, t.pos_y + eff_h / 2.0);
        let mut raw = Vec::new();
        for y in 0..h {
            for x in 0..w {
                let i = ((y * w + x) * 4) as usize;
                if matches_pixel(px[i], px[i + 1], px[i + 2], px[i + 3], cfg) {
                    let (ux, uy) = (x as f32 * t.scale_x, y as f32 * t.scale_y);
                    let (wx, wy) = project_3d_canvas(ux - eff_w / 2.0, uy - eff_h / 2.0, cx, cy, t.rot_deg, pitch, yaw);
                    raw.push(Point2D { x: wx, y: wy });
                }
            }
        }
        if raw.len() > PIXEL_GRID_MAX {
            let step = raw.len() as f32 / PIXEL_GRID_MAX as f32;
            (0..PIXEL_GRID_MAX).map(|i| raw[((i as f32 * step) as usize).min(raw.len() - 1)]).collect()
        } else {
            raw
        }
    }

    #[test]
    fn unproject_finds_the_point_that_projects_there() {
        for (roll, pitch, yaw) in [(0.0, 0.0, 0.0), (35.0, 0.0, 0.0), (-20.0, 25.0, -30.0), (170.0, -15.0, 40.0)] {
            for (lx, ly) in [(0.0, 0.0), (120.0, -80.0), (-200.0, 150.0)] {
                let (px, py) = project_3d_canvas(lx, ly, 900.0, 500.0, roll, pitch, yaw);
                let (ux, uy) = unproject_3d_canvas(px, py, 900.0, 500.0, roll, pitch, yaw);
                assert!((ux - lx).abs() < 0.5 && (uy - ly).abs() < 0.5, "{:?} -> {:?} at {:?}", (lx, ly), (ux, uy), (roll, pitch, yaw));
            }
        }
    }

    #[test]
    fn pixel_grid_projects_only_the_points_it_keeps_with_the_same_result() {
        let cfg = TrackingConfig::default();
        let pts = |g: &[Point2D]| g.iter().map(|p| (p.x, p.y)).collect::<Vec<_>>();
        let (t, pitch, yaw) = (transform(640.0, 300.0, 1.75, 30.0), 12.0, -20.0);
        // Many solid pixels (the L: ~4750), so the grid is down-sampled…
        let (px, w, h) = asymmetric_l_source();
        let (_, _, grid) = scan_rgba_with_3d_transform(&px, w, h, w * 4, &t, pitch, yaw, &cfg);
        let old = pixel_grid_the_old_way(&px, w, h, &t, pitch, yaw, &cfg);
        assert_eq!(grid.len(), PIXEL_GRID_MAX);
        assert_eq!(pts(&grid), pts(&old));
        // …and few (a 20x20 square): every one kept
        let (w, h) = (64u32, 48u32);
        let mut small = vec![0u8; (w * h * 4) as usize];
        for y in 10..30 {
            for x in 20..40 {
                let i = ((y * w + x) * 4) as usize;
                small[i..i + 4].copy_from_slice(&[255, 255, 255, 255]);
            }
        }
        let (_, _, grid) = scan_rgba_with_3d_transform(&small, w, h, w * 4, &t, pitch, yaw, &cfg);
        assert_eq!(grid.len(), 400);
        assert_eq!(pts(&grid), pts(&pixel_grid_the_old_way(&small, w, h, &t, pitch, yaw, &cfg)));
    }

    #[test]
    fn reproject_after_move_matches_a_full_rescan() {
        let (px, w, h) = asymmetric_l_source();
        let cfg = TrackingConfig::default();
        let (mut hb, mut mask, _) = scan_rgba_with_3d_transform(&px, w, h, w * 4, &transform(100.0, 50.0, 1.0, 0.0), 0.0, 0.0, &cfg);

        // Move, scale, rotate and tilt the source, then re-derive without rescanning
        let (moved, pitch, yaw) = (transform(640.0, 300.0, 1.75, 30.0), 12.0, -20.0);
        mask.transform = moved;
        mask.pitch = pitch;
        mask.yaw = yaw;
        reproject_hitbox(&mut hb, &mask);

        let (fresh, _, _) = scan_rgba_with_3d_transform(&px, w, h, w * 4, &moved, pitch, yaw, &cfg);
        assert_close(hb.world_center_x, fresh.world_center_x, "center x");
        assert_close(hb.world_center_y, fresh.world_center_y, "center y");
        assert_close(hb.world_x, fresh.world_x, "box x");
        assert_close(hb.world_y, fresh.world_y, "box y");
        assert_close(hb.world_width, fresh.world_width, "box width");
        assert_close(hb.world_height, fresh.world_height, "box height");
    }

    #[test]
    fn target_point_is_mask_center_of_mass_not_source_middle() {
        let (px, w, h) = asymmetric_l_source();
        let (mut hb, mut mask, _) = scan_rgba_with_3d_transform(&px, w, h, w * 4, &transform(0.0, 0.0, 1.0, 0.0), 0.0, 0.0, &TrackingConfig::default());
        mask.transform = transform(300.0, 200.0, 2.0, 0.0);
        reproject_hitbox(&mut hb, &mask);

        // Pixel-mass centroid of the L in local pixels, mapped through pos + scale
        let (bar, foot) = (40.0 * 85.0, 90.0 * 15.0);
        let local_cx = (bar * 169.5 + foot * 104.5) / (bar + foot);
        let local_cy = (bar * 52.0 + foot * 87.0) / (bar + foot);
        assert_close(hb.world_center_x, 300.0 + local_cx * 2.0, "center of mass x");
        assert_close(hb.world_center_y, 200.0 + local_cy * 2.0, "center of mass y");

        // ...which is nowhere near the middle of the (400x200 scaled) source
        let (source_mid_x, source_mid_y) = (300.0 + 200.0, 200.0 + 100.0);
        assert!((hb.world_center_x - source_mid_x).abs() > 80.0, "center x {} still at source middle", hb.world_center_x);
        assert!((hb.world_center_y - source_mid_y).abs() > 15.0, "center y {} still at source middle", hb.world_center_y);
    }
}
