//! Body parts of a Theater Puppet source (Theater Puppet, its own plugin):
//! a redeem can aim at one, and a hit says which one it landed on.
//!
//! A target names a part after a colon, `Puppet:head`, or alone, `head`
//! (the avatar in slot 1). Parts: torso, head, left_arm, right_arm,
//! left_hand, right_hand, left_leg, right_leg, left_foot, right_foot (the
//! avatar's own left and right). Everything else about the target (its box,
//! its mask) is the source's, as without a part, so a throw aimed at the
//! head still hits the arm in its way.
//!
//! Puppet answers through OBS's procedures (obs_procs::puppet_parts /
//! puppet_part_at) in its source's own pixels; the target's collision mask
//! turns them into canvas points and back. Every hit is announced on the
//! `theater_scene_impact` signal, with the part for a Puppet source.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::tracker::CollisionMask;

pub const PARTS: [&str; 10] = [
    "torso", "head", "left_arm", "right_arm", "left_hand", "right_hand", "left_leg", "right_leg", "left_foot", "right_foot",
];

/// How long a source's parts are kept before asking Puppet again (it
/// redraws them ten times a second)
const FRESH: Duration = Duration::from_millis(100);

/// `Puppet:head` -> ("Puppet", Some("head")); `head` -> ("1", Some("head"))
/// (the avatar in slot 1); anything else -> (target, None)
pub fn split(target: &str) -> (&str, Option<&'static str>) {
    let part_named = |p: &str| PARTS.iter().copied().find(|k| k.eq_ignore_ascii_case(p.trim()));
    if let Some((base, part)) = target.rsplit_once(':') {
        if let Some(p) = part_named(part) {
            return (base.trim(), Some(p));
        }
    }
    if let Some(p) = part_named(target) {
        return ("1", Some(p));
    }
    (target, None)
}

/// Only `Source:part` -> ("Source", Some(part)), for the Studio's lists (a
/// bare part stays a name there)
pub fn split_named(target: &str) -> (&str, Option<&'static str>) {
    if let Some((base, part)) = target.rsplit_once(':') {
        if let Some(p) = PARTS.iter().copied().find(|k| k.eq_ignore_ascii_case(part.trim())) {
            return (base.trim(), Some(p));
        }
    }
    (target, None)
}

/// A target naming `part` of `source` (none: the whole source)
pub fn with_part(source: &str, part: Option<&str>) -> String {
    match part {
        Some(p) => format!("{source}:{p}"),
        None => source.to_string(),
    }
}

/// A part's name in the Studio
pub fn label(part: &str) -> String {
    let words = part.replace('_', " ");
    let mut c = words.chars();
    c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
}

/// Whether `source` is a Theater Puppet source (it has body parts)
pub fn is_puppet(source: &str) -> bool {
    parts_of(source).is_some()
}

/// A Puppet source's parts: its size and each part's centre, its pixels
#[derive(Clone, Debug, Default, PartialEq)]
struct Parts {
    width: f32,
    height: f32,
    centres: HashMap<String, (f32, f32)>,
}

fn parse(json: &str) -> Option<Parts> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    let f = |v: &serde_json::Value, k: &str| v.get(k).and_then(|x| x.as_f64()).map(|x| x as f32);
    let centres = v
        .get("parts")?
        .as_object()?
        .iter()
        .filter_map(|(name, b)| Some((name.clone(), (f(b, "cx")?, f(b, "cy")?))))
        .collect();
    Some(Parts { width: f(&v, "width")?, height: f(&v, "height")?, centres })
}

static CACHE: Mutex<Option<HashMap<String, (Instant, Option<Parts>)>>> = Mutex::new(None);

/// `source`'s parts, asked of Puppet at most every FRESH (None: not a Puppet source)
fn parts_of(source: &str) -> Option<Parts> {
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    let cache = cache.get_or_insert_with(HashMap::new);
    if let Some((at, parts)) = cache.get(source) {
        if at.elapsed() < FRESH {
            return parts.clone();
        }
    }
    let parts = crate::obs_procs::puppet_parts(source).and_then(|j| parse(&j));
    cache.insert(source.to_string(), (Instant::now(), parts.clone()));
    parts
}

/// A Puppet pixel as a pixel of the source's mask (the same unless sizes differ)
fn to_mask(mask: &CollisionMask, parts: &Parts, x: f32, y: f32) -> (f32, f32) {
    (x * mask.width as f32 / parts.width.max(1.0), y * mask.height as f32 / parts.height.max(1.0))
}

/// Where a throw at `target` aims when it names a body part: that part's
/// centre on the canvas. None for no part, or a part not in view.
pub fn aim(target: &str, slots: &HashMap<usize, String>, masks: &HashMap<String, CollisionMask>) -> Option<(f32, f32)> {
    let (base, part) = split(target);
    let part = part?;
    let (name, mask) = crate::resolve_target_mask(base, slots, masks)?;
    let parts = parts_of(name)?;
    let &(cx, cy) = parts.centres.get(part)?;
    let (lx, ly) = to_mask(mask, &parts, cx, cy);
    Some(mask.local_to_world(lx, ly))
}

/// The body part at canvas point (x, y) of `target`'s source, if it's a
/// Puppet source: (source name, part)
pub fn part_at(target: &str, slots: &HashMap<usize, String>, masks: &HashMap<String, CollisionMask>, x: f32, y: f32) -> Option<(String, String)> {
    let (base, _) = split(target);
    let (name, mask) = crate::resolve_target_mask(base, slots, masks)?;
    let parts = parts_of(name)?;
    let (lx, ly) = mask.world_to_local(x, y)?;
    // The mask's pixel as Puppet's
    let (px, py) = (lx * parts.width / mask.width.max(1) as f32, ly * parts.height / mask.height.max(1) as f32);
    let part = crate::obs_procs::puppet_part_at(name, px, py)?;
    Some((name.to_string(), part))
}

/// A throwable hit `target` at (x, y): announced on `theater_scene_impact`,
/// with the body part when the target is a Puppet source.
pub fn report_hit(target: &str, slots: &HashMap<usize, String>, masks: &HashMap<String, CollisionMask>, x: f32, y: f32, force: f32) {
    let (source, part) = match part_at(target, slots, masks, x, y) {
        Some(found) => found,
        None => {
            let (base, _) = split(target);
            let name = crate::resolve_target_mask(base, slots, masks).map(|(n, _)| n.to_string()).unwrap_or_else(|| base.to_string());
            (name, String::new())
        }
    };
    if !part.is_empty() {
        crate::obs_source::trust_log(&format!("[Trust Puppet] 🎯 Hit {source}'s {part} at ({x:.0}, {y:.0}), force {force:.1}"));
    }
    crate::obs_procs::emit_impact(&source, &part, x, y, force);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn targets_name_parts() {
        assert_eq!(split("Puppet:head"), ("Puppet", Some("head")));
        assert_eq!(split("My Avatar : Left_Hand"), ("My Avatar", Some("left_hand")));
        assert_eq!(split("torso"), ("1", Some("torso")));
        assert_eq!(split("Avatar"), ("Avatar", None));
        // A colon that isn't a part stays in the name
        assert_eq!(split("Scene: Cam"), ("Scene: Cam", None));
        // The Studio's lists: only Source:part splits
        assert_eq!(split_named("head"), ("head", None));
        assert_eq!(split_named("🎭 Theater Puppet:right_foot"), ("🎭 Theater Puppet", Some("right_foot")));
        assert_eq!(with_part("🎭 Theater Puppet", Some("head")), "🎭 Theater Puppet:head");
        assert_eq!(with_part("Cam", None), "Cam");
        assert_eq!(label("left_hand"), "Left hand");
    }

    #[test]
    fn puppet_answers_are_read() {
        let json = r#"{"source":"Puppet","width":1080,"height":1080,"parts":{"head":{"x":320,"y":16,"w":440,"h":588,"cx":540.5,"cy":245.0,"pixels":78528}}}"#;
        let p = parse(json).unwrap();
        assert_eq!((p.width, p.height), (1080.0, 1080.0));
        assert_eq!(p.centres.get("head"), Some(&(540.5, 245.0)));
        assert!(parse("{}").is_none());
    }

    #[test]
    fn mask_points_go_both_ways() {
        let t = crate::types::SourceTransform { pos_x: 100.0, pos_y: 50.0, scale_x: 0.5, scale_y: 0.5, rot_deg: 20.0, ..Default::default() };
        let mask = CollisionMask::new(1080, 1080, t);
        let (wx, wy) = mask.local_to_world(540.0, 245.0);
        let (lx, ly) = mask.world_to_local(wx, wy).unwrap();
        assert!((lx - 540.0).abs() < 0.01 && (ly - 245.0).abs() < 0.01, "{lx}, {ly}");
        // The middle of the source is the middle of its box on the canvas
        let (mx, my) = mask.local_to_world(540.0, 540.0);
        assert!((mx - 370.0).abs() < 0.01 && (my - 320.0).abs() < 0.01, "{mx}, {my}");
    }
}
