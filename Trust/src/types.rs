use serde::{Deserialize, Serialize};

/// 2D point in stream canvas space for granular pixel scans.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct Point2D {
    pub x: f32,
    pub y: f32,
}

/// C-compatible representation of a tracked hitbox in both local and world/canvas space.
/// Directly accessible via LuaJIT ffi.cdef.
#[repr(C)]
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Hitbox {
    // Local source texture coordinates
    pub local_x: f32,
    pub local_y: f32,
    pub local_width: f32,
    pub local_height: f32,

    // World / Stream Canvas space coordinates (1920x1080)
    pub world_x: f32,
    pub world_y: f32,
    pub world_width: f32,
    pub world_height: f32,
    pub world_center_x: f32,
    pub world_center_y: f32,

    // Detection stats
    pub mass: u32,
    pub active: bool,
}

impl Default for Hitbox {
    fn default() -> Self {
        Self {
            local_x: 0.0,
            local_y: 0.0,
            local_width: 0.0,
            local_height: 0.0,
            world_x: 0.0,
            world_y: 0.0,
            world_width: 0.0,
            world_height: 0.0,
            world_center_x: 0.0,
            world_center_y: 0.0,
            mass: 0,
            active: false,
        }
    }
}

/// C-compatible OBS Source Transform parameters from active scene item.
#[repr(C)]
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct SourceTransform {
    pub pos_x: f32,
    pub pos_y: f32,
    pub scale_x: f32,
    pub scale_y: f32,
    pub crop_left: u32,
    pub crop_top: u32,
    pub crop_right: u32,
    pub crop_bottom: u32,
    pub rot_deg: f32,
}

impl Default for SourceTransform {
    fn default() -> Self {
        Self {
            pos_x: 0.0,
            pos_y: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            crop_left: 0,
            crop_top: 0,
            crop_right: 0,
            crop_bottom: 0,
            rot_deg: 0.0,
        }
    }
}

/// C-compatible tracking configuration parameters.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TrackingConfig {
    pub mode: u8,            // 0 = Alpha Transparency (Default), 1 = Chroma HSV, 2 = Brightness Luminance
    pub alpha_threshold: u8, // Minimum Alpha value (0-255) to count as solid, default 10
    pub chroma_r: u8,
    pub chroma_g: u8,
    pub chroma_b: u8,
    pub threshold: u8,
    pub precision: u8,       // 1 to 10 step size control
}

impl Default for TrackingConfig {
    fn default() -> Self {
        Self {
            mode: 0, // Default to Alpha Transparency
            alpha_threshold: 10,
            chroma_r: 0,
            chroma_g: 255,
            chroma_b: 0,
            threshold: 40,
            precision: 8,
        }
    }
}

/// Incoming message from Theater Reader: an event (redeem, sub, bits, alert…),
/// which can fire a pose or redeem, or a chat message, which never does (there
/// are no chat triggers; see studio_app::process_trigger_event).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamEvent {
    pub event_type: String,
    pub trigger: String,
    pub user: String,
    pub message: String,
    pub raw_json: String,
}

/// Drag interaction mode for Scene Studio Transform Gizmo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GizmoDragMode {
    None,
    PanFree,      // White Center: translates on the plane (X + Y) - ONLY white center moves target
    RotateRoll,   // Blue Rotation Ring: in-plane Z-axis rotation (Roll)
    RotatePitch,  // Green Rotation Ring: 3D X-axis vertical tilt (Pitch)
    RotateYaw,    // Red Rotation Ring: 3D Y-axis horizontal turn (Yaw)
    Scale,        // White Outer Ring: uniform scaling
    Move,         // Alias for PanFree
    Rotate,       // Alias for RotateRoll
}

impl Default for GizmoDragMode {
    fn default() -> Self {
        GizmoDragMode::None
    }
}

/// Active drag state for Scene Studio 2.5D Gizmo.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GizmoDragState {
    pub mode: GizmoDragMode,
    pub target_name: String,
    pub start_mx: f32,
    pub start_my: f32,
    pub start_x: f32,
    pub start_y: f32,
    pub start_w: f32,
    pub start_h: f32,
    pub start_roll: f32,
    pub start_pitch: f32,
    pub start_yaw: f32,
}


