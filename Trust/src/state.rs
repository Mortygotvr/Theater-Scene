use std::collections::HashMap;
use std::sync::OnceLock;
use crossbeam_channel::{Receiver, Sender};
use parking_lot::RwLock;
use tokio::runtime::Runtime;
use crate::reader_client::ReaderClient;
use crate::tracker::CollisionMask;
use crate::types::{Hitbox, Point2D, StreamEvent, TrackingConfig};
use crate::effects::EffectsManager;

#[derive(Clone)]
pub struct TargetPreviewTexture {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub frame_id: u64,
    pub base_width: u32,
    pub base_height: u32,
}

impl TargetPreviewTexture {
    #[inline]
    pub fn pad_ratio_x(&self) -> f32 {
        if self.base_width > 0 && self.width > 0 {
            self.width as f32 / self.base_width as f32
        } else {
            1.0
        }
    }

    #[inline]
    pub fn pad_ratio_y(&self) -> f32 {
        if self.base_height > 0 && self.height > 0 {
            self.height as f32 / self.base_height as f32
        } else {
            1.0
        }
    }

    #[inline]
    pub fn is_padded_reaction(&self) -> bool {
        self.pad_ratio_x() > 1.05 || self.pad_ratio_y() > 1.05
    }
}

/// A single active "screen shake" triggered by a throwable impact (see
/// `physics::Throwable::shake` / `ShakeConfig`). The whole composited output
/// gets jittered by this for `duration_ms`, decaying to nothing.
#[derive(Clone, Copy)]
pub struct ScreenShakeState {
    pub start_ms: u64,
    pub duration_ms: u32,
    pub intensity: f32,
}

/// A single active avatar reaction (Bounce/PixelDrip/Custom get a real per-pixel
/// shader on the render side; Jolt is a jitter+flash approximation).
#[derive(Clone)]
pub struct AvatarReactionState {
    pub kind: crate::physics::ImpactReactionKind,
    pub start_ms: u64,
    pub duration_ms: u32,
    pub amount: f32,
    /// User-configured overrides for a `Custom` script's own uniforms.
    pub custom_params: Vec<(String, f32)>,
    /// Where the target was hit (oldest first), for the reaction's shader
    pub hits: Vec<ReactionHit>,
}

/// Where and when a reaction's target was hit, in the target's own frame: 0..1
/// across its width and height, before its rotation — so the spot stays on
/// the same part of the target as it moves or turns.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReactionHit {
    pub x: f32,
    pub y: f32,
    pub at_ms: u64,
}

/// The most hits a reaction remembers: the most a shader's `hits` array can
/// hold (Direct3D 11 gives a shader 4096 four-float registers)
pub const MAX_REACTION_HITS: usize = 4096;

/// A target's running reaction, as the render reads it.
#[derive(Clone)]
pub struct ActiveReaction {
    pub kind: crate::physics::ImpactReactionKind,
    /// 0..1 through `duration_ms` since it was last triggered
    pub progress: f32,
    pub amount: f32,
    pub duration_ms: u32,
    pub custom_params: Vec<(String, f32)>,
    /// Its hits still within `duration_ms`: where (ReactionHit's frame) and
    /// how long ago, in seconds
    pub hits: Vec<(f32, f32, f32)>,
}

pub struct GlobalState {
    pub runtime: Runtime,
    pub reader_client: ReaderClient,
    pub client_senders: RwLock<HashMap<u32, Sender<StreamEvent>>>,
    pub client_receivers: RwLock<HashMap<u32, Receiver<StreamEvent>>>,
    pub target_slots: RwLock<HashMap<usize, String>>,
    pub physics: RwLock<crate::physics::PhysicsEngine>,
    pub effects: RwLock<EffectsManager>,
    pub config: RwLock<TrackingConfig>,
    pub hitboxes: RwLock<HashMap<String, Hitbox>>,
    pub masks: RwLock<HashMap<String, CollisionMask>>,
    pub pixel_grids: RwLock<HashMap<String, Vec<Point2D>>>,
    pub target_textures: RwLock<HashMap<String, TargetPreviewTexture>>,
    pub editor_active_triggers: RwLock<Vec<String>>,
    pub studio_sources: RwLock<Vec<crate::StudioSource>>,
    pub studio_scene_name: RwLock<String>,
    pub studio_available_scenes: RwLock<Vec<String>>,
    pub studio_selected_source: RwLock<String>,
    pub studio_composite_frame: RwLock<Option<TargetPreviewTexture>>,
    pub studio_show_hitbox: std::sync::atomic::AtomicBool,
    pub studio_black_backdrop_enabled: std::sync::atomic::AtomicBool,
    pub studio_throwables_visible: std::sync::atomic::AtomicBool,
    pub studio_throwables_last_render_ms: std::sync::atomic::AtomicU64,
    pub studio_viewport: RwLock<(u32, u32, u32, u32, u32, u32)>,
    pub studio_pointer_pos: RwLock<(f32, f32)>,
    pub studio_drag_state: RwLock<crate::types::GizmoDragState>,
    pub studio_pending_transform_changes: std::sync::atomic::AtomicBool,
    pub screen_shake: RwLock<Option<ScreenShakeState>>,
    pub avatar_reactions: RwLock<HashMap<String, AvatarReactionState>>,
}

impl GlobalState {
    pub fn new() -> Self {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("Failed to initialize Tokio runtime");

        let state = Self {
            runtime,
            reader_client: ReaderClient::new(),
            client_senders: RwLock::new(HashMap::new()),
            client_receivers: RwLock::new(HashMap::new()),
            target_slots: RwLock::new(HashMap::new()),
            physics: RwLock::new(crate::physics::PhysicsEngine::new()),
            effects: RwLock::new(EffectsManager::new()),
            config: RwLock::new(TrackingConfig::default()),
            hitboxes: RwLock::new(HashMap::new()),
            masks: RwLock::new(HashMap::new()),
            pixel_grids: RwLock::new(HashMap::new()),
            target_textures: RwLock::new(HashMap::new()),
            editor_active_triggers: RwLock::new(Vec::new()),
            studio_sources: RwLock::new(Vec::new()),
            studio_scene_name: RwLock::new("Scène".to_string()),
            studio_available_scenes: RwLock::new(Vec::new()),
            studio_selected_source: RwLock::new(String::new()),
            studio_composite_frame: RwLock::new(None),
            studio_show_hitbox: std::sync::atomic::AtomicBool::new(false),
            studio_black_backdrop_enabled: std::sync::atomic::AtomicBool::new(true),
            studio_throwables_visible: std::sync::atomic::AtomicBool::new(true),
            studio_throwables_last_render_ms: std::sync::atomic::AtomicU64::new(0),
            studio_viewport: RwLock::new((1920, 1080, 0, 0, 0, 0)),
            studio_pointer_pos: RwLock::new((0.0, 0.0)),
            studio_drag_state: RwLock::new(crate::types::GizmoDragState::default()),
            studio_pending_transform_changes: std::sync::atomic::AtomicBool::new(false),
            screen_shake: RwLock::new(None),
            avatar_reactions: RwLock::new(HashMap::new()),
        };

        // Initialize default client 0 (theater.lua) and client 1 (theater_manager.lua)
        let (s0, r0) = crossbeam_channel::unbounded();
        state.client_senders.write().insert(0, s0);
        state.client_receivers.write().insert(0, r0);

        let (s1, r1) = crossbeam_channel::unbounded();
        state.client_senders.write().insert(1, s1);
        state.client_receivers.write().insert(1, r1);

        state
    }

    pub fn broadcast_event(&self, event: StreamEvent) {
        let senders = self.client_senders.read();
        for sender in senders.values() {
            let _ = sender.send(event.clone());
        }
    }

    pub fn get_or_create_client_receiver(&self, client_id: u32) -> Receiver<StreamEvent> {
        let receivers = self.client_receivers.read();
        if let Some(rx) = receivers.get(&client_id) {
            return rx.clone();
        }
        drop(receivers);

        let mut receivers_w = self.client_receivers.write();
        if let Some(rx) = receivers_w.get(&client_id) {
            return rx.clone();
        }
        let (tx, rx) = crossbeam_channel::unbounded();
        self.client_senders.write().insert(client_id, tx);
        receivers_w.insert(client_id, rx.clone());
        rx
    }

    pub fn set_editor_triggers(&self, triggers_str: &str) {
        let mut list = self.editor_active_triggers.write();
        list.clear();
        for part in triggers_str.split(',') {
            let trimmed = part.trim();
            if !trimmed.is_empty() {
                list.push(trimmed.to_lowercase());
            }
        }
    }

    pub fn is_editor_trigger(&self, trigger_str: &str) -> bool {
        let list = self.editor_active_triggers.read();
        if list.is_empty() {
            return false;
        }
        let lower = trigger_str.to_lowercase();
        for pat in list.iter() {
            if pat == &lower || lower.contains(pat) || pat.contains(&lower) {
                return true;
            }
        }
        false
    }

    pub fn clear_editor_triggers(&self) {
        self.editor_active_triggers.write().clear();
    }

    /// Arms (or refreshes) the global screen-shake effect. Called from a
    /// throwable's impact handler when its redeem has Screen Shake enabled.
    pub fn trigger_screen_shake(&self, duration_ms: u32, intensity: f32) {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        *self.screen_shake.write() = Some(ScreenShakeState { start_ms: now_ms, duration_ms, intensity });
    }

    /// Current pixel jitter offset (x, y) to apply to the whole composited
    /// output, or (0.0, 0.0) if no shake is active/expired.
    pub fn get_screen_shake_offset(&self) -> (f32, f32) {
        let Some(shake) = *self.screen_shake.read() else {
            return (0.0, 0.0);
        };
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let elapsed_ms = now_ms.saturating_sub(shake.start_ms);
        if elapsed_ms >= shake.duration_ms as u64 {
            *self.screen_shake.write() = None;
            return (0.0, 0.0);
        }
        let progress = elapsed_ms as f32 / shake.duration_ms.max(1) as f32;
        let decay = (1.0 - progress).max(0.0);
        let t = elapsed_ms as f32 / 1000.0;
        let ox = (t * 55.0).sin() * shake.intensity * decay;
        let oy = (t * 71.0 + 1.7).cos() * shake.intensity * decay;
        (ox, oy)
    }

    /// Arms (or refreshes) a Bounce/PixelDrip/Jolt/Custom avatar reaction for
    /// the given target. `custom_params` is only meaningful for `Custom`.
    /// `hit` is where the target was hit, in its own frame (see ReactionHit).
    /// Hits add up: those of the same reaction still running are kept, each
    /// lasting `duration_ms` from when it landed.
    pub fn trigger_avatar_reaction(
        &self,
        target: &str,
        kind: crate::physics::ImpactReactionKind,
        duration_ms: u32,
        amount: f32,
        custom_params: Vec<(String, f32)>,
        hit: Option<(f32, f32)>,
    ) {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let mut reactions = self.avatar_reactions.write();
        let mut hits: Vec<ReactionHit> = reactions
            .get(target)
            .filter(|r| r.kind == kind)
            .map(|r| {
                r.hits
                    .iter()
                    .copied()
                    .filter(|h| now_ms.saturating_sub(h.at_ms) < duration_ms as u64)
                    .collect()
            })
            .unwrap_or_default();
        if let Some((x, y)) = hit {
            hits.push(ReactionHit { x, y, at_ms: now_ms });
        }
        let oldest = hits.len().saturating_sub(MAX_REACTION_HITS);
        hits.drain(..oldest);
        reactions.insert(
            target.to_string(),
            AvatarReactionState { kind, start_ms: now_ms, duration_ms, amount, custom_params, hits },
        );
    }

    /// `target`'s reaction, if it has one running.
    pub fn get_avatar_reaction_progress(&self, target: &str) -> Option<ActiveReaction> {
        let state = self.avatar_reactions.read().get(target)?.clone();
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let elapsed_ms = now_ms.saturating_sub(state.start_ms);
        if elapsed_ms >= state.duration_ms as u64 {
            return None;
        }
        let progress = elapsed_ms as f32 / state.duration_ms.max(1) as f32;
        let hits = state
            .hits
            .iter()
            .map(|h| (h.x, h.y, now_ms.saturating_sub(h.at_ms) as f32 / 1000.0))
            .filter(|&(_, _, age)| age * 1000.0 < state.duration_ms as f32)
            .collect();
        Some(ActiveReaction {
            kind: state.kind,
            progress,
            amount: state.amount,
            duration_ms: state.duration_ms,
            custom_params: state.custom_params,
            hits,
        })
    }
}

static STATE: OnceLock<GlobalState> = OnceLock::new();

pub fn get_state() -> &'static GlobalState {
    STATE.get_or_init(GlobalState::new)
}
