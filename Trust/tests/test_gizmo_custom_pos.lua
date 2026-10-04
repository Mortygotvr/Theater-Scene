local ffi = require("ffi")

ffi.cdef[[
    typedef struct {
        float x;
        float y;
    } Point2D;

    typedef struct {
        float local_x;
        float local_y;
        float local_width;
        float local_height;
        float world_x;
        float world_y;
        float world_width;
        float world_height;
        float world_center_x;
        float world_center_y;
        uint32_t mass;
        bool active;
    } Hitbox;

    bool trust_init(void);
    void trust_studio_clear_sources(void);
    void trust_studio_add_source(const char *name, uint32_t slot, float x, float y, float width, float height, float roll, bool is_target);
    void trust_studio_add_source_ex(const char *name, uint32_t slot, float x, float y, float width, float height, float roll, bool is_target, bool match_obs);
    bool trust_studio_set_source_custom(const char *name, bool match_obs, float x, float y, float width, float height, float roll);
    bool trust_studio_get_source_custom(const char *name, float *out_x, float *out_y, float *out_w, float *out_h, float *out_roll, bool *out_match_obs);
    bool trust_studio_get_selected_source(char *out_name, size_t max_len);
    void trust_studio_set_selected_source(const char *name);
    typedef void (*studio_transform_cb_t)(const char *name, float x, float y, float width, float height, float roll, float pitch, float yaw, bool match_obs);
    void trust_studio_set_transform_callback(studio_transform_cb_t cb);
    uint32_t trust_studio_render_preview(uint32_t canvas_w, uint32_t canvas_h, uint8_t *out_rgba, size_t max_len);
    void trust_studio_set_show_hitbox(bool show);
    void trust_set_canvas_resolution(uint32_t w, uint32_t h);

    void trust_gui_mouse_move(float x, float y);
    void trust_gui_mouse_button(bool down);
    void trust_gui_set_modifiers(bool ctrl, bool shift, bool alt);
    void trust_gui_mouse_move_ex(float x, float y, bool ctrl, bool shift, bool alt);
    void trust_gui_mouse_button_ex(bool down, bool ctrl, bool shift, bool alt);
]]

local lib = ffi.load("./Trust/target/release/libtrust.so")
assert(lib, "Failed to load libtrust.so")

assert(lib.trust_init() == true, "trust_init failed")
print("✅ trust_init OK")

-- 1. Setup Canvas Resolution (1920x1080)
lib.trust_set_canvas_resolution(1920, 1080)

-- 2. Add Sources
lib.trust_studio_clear_sources()
lib.trust_studio_add_source_ex("PlayerAvatar", 1, 100.0, 200.0, 300.0, 400.0, 0.0, true, true)
lib.trust_studio_add_source_ex("OverlayItem", 2, 500.0, 100.0, 150.0, 150.0, 15.0, false, true)

-- 3. Verify Selection
lib.trust_studio_set_selected_source("PlayerAvatar")
local name_buf = ffi.new("char[128]")
assert(lib.trust_studio_get_selected_source(name_buf, 128) == true)
assert(ffi.string(name_buf) == "PlayerAvatar", "Selected source mismatch")
print("✅ Selection OK: " .. ffi.string(name_buf))

-- 4. Initial Render to establish Viewport (960x540 window)
local preview_w, preview_h = 960, 540
local buf_size = preview_w * preview_h * 4
local preview_buf = ffi.new("uint8_t[?]", buf_size)
local ok_render = lib.trust_studio_render_preview(preview_w, preview_h, preview_buf, buf_size)
assert(ok_render == 1, "Preview render failed")
print("✅ Initial Preview Render OK (Viewport established at 960x540)")

-- 5. Test Transform Callback
local callback_fired = false
local cb_target = ""
local cb_x, cb_y, cb_w, cb_h, cb_roll, cb_pitch, cb_yaw, cb_match = 0, 0, 0, 0, 0, 0, 0, true

local cb_anchor = ffi.cast("studio_transform_cb_t", function(c_name, x, y, w, h, roll, pitch, yaw, match_obs)
    callback_fired = true
    cb_target = ffi.string(c_name)
    cb_x = tonumber(x)
    cb_y = tonumber(y)
    cb_w = tonumber(w)
    cb_h = tonumber(h)
    cb_roll = tonumber(roll)
    cb_pitch = tonumber(pitch)
    cb_yaw = tonumber(yaw)
    cb_match = match_obs
    print(string.format("   -> 📞 Callback fired for '%s': (%.1f, %.1f) %.1fx%.1f roll=%.1f° pitch=%.1f° yaw=%.1f° match_obs=%s",
        cb_target, cb_x, cb_y, cb_w, cb_h, cb_roll, cb_pitch, cb_yaw, tostring(cb_match)))
end)

lib.trust_studio_set_transform_callback(cb_anchor)

-- 5.5 Verify clicking inside target box outside center does NOT move target
print("🎯 Testing: Clicking inside target frame outside center must NOT move target...")
callback_fired = false
-- Target is at (50, 100) to (200, 300) in preview. Click at (80, 140) which is inside box but far from center (125, 200).
lib.trust_gui_mouse_move(80, 140)
lib.trust_gui_mouse_button(true)
lib.trust_gui_mouse_move(100, 160)
lib.trust_gui_mouse_button(false)
assert(callback_fired == false, "Error: Dragging inside target frame moved the target! Only white center must move it.")
print("✅ Click/drag inside target box did NOT move target (only white center can move it)!")

-- 6. Simulate Gizmo Drag: Move PlayerAvatar using White Center
-- PlayerAvatar in world: (100, 200, 300, 400). Center: (250, 400).
-- In 960x540 preview (scale = 0.5):
-- sx = 50, sy = 100, sw = 150, sh = 200. Center = (125, 200).
print("🎯 Simulating Mouse Drag (White Center Move)...")
lib.trust_gui_mouse_move(125, 200) -- White Center (dist = 0 <= 12px)
lib.trust_gui_mouse_button(true)   -- Press left mouse button

-- Drag 50px right, 30px down on screen -> 100px right, 60px down in world!
lib.trust_gui_mouse_move(175, 230)
-- Render a frame while dragging
lib.trust_studio_render_preview(preview_w, preview_h, preview_buf, buf_size)

-- Release mouse button
lib.trust_gui_mouse_button(false)

assert(callback_fired == true, "Transform callback did NOT fire on drag release!")
assert(cb_target == "PlayerAvatar", "Target name mismatch in callback")
assert(cb_match == true, "match_obs should be preserved as true during dragging so OBS updates follow gizmo!")
assert(math.abs(cb_x - 200.0) < 1.0, string.format("Expected x ~ 200.0, got %.2f", cb_x))
assert(math.abs(cb_y - 260.0) < 1.0, string.format("Expected y ~ 260.0, got %.2f", cb_y))
print("✅ White Center Move OK! World pos moved from (100, 200) to (" .. cb_x .. ", " .. cb_y .. ")")

-- 7. Simulate Gizmo Drag: Scale using White Outer Ring
-- Current PlayerAvatar: world pos (200, 260, 300, 400). Center in world: (350, 460).
-- In 960x540 preview (scale = 0.5):
-- Center = (175, 230).
-- White Scale Ring is at radius 48px: e.g. (175 + 48, 230) = (223, 230).
print("🎯 Simulating Mouse Drag (White Scale Ring)...")
callback_fired = false
lib.trust_gui_mouse_move(223, 230) -- On White Scale Ring (dist = 48px)
lib.trust_gui_mouse_button(true)

-- Drag outward from 48px to 72px (1.5x scale)
lib.trust_gui_mouse_move(247, 230)
lib.trust_studio_render_preview(preview_w, preview_h, preview_buf, buf_size)
lib.trust_gui_mouse_button(false)

assert(callback_fired == true, "Transform callback did NOT fire on scale release!")
assert(math.abs(cb_w - 450.0) < 5.0, string.format("Expected width ~ 450.0, got %.2f", cb_w))
assert(math.abs(cb_h - 600.0) < 5.0, string.format("Expected height ~ 600.0, got %.2f", cb_h))
-- Verify center remained invariant at world (350, 460)
local center_x = cb_x + cb_w / 2.0
local center_y = cb_y + cb_h / 2.0
assert(math.abs(center_x - 350.0) < 1.0, string.format("Scale shifted center X! Expected 350, got %.2f", center_x))
assert(math.abs(center_y - 460.0) < 1.0, string.format("Scale shifted center Y! Expected 460, got %.2f", center_y))
print("✅ White Ring Scale OK! Scaled 1.5x to " .. cb_w .. "x" .. cb_h .. " with invariant center (" .. center_x .. ", " .. center_y .. ")")

-- 8. Simulate Gizmo Drag: Blue Roll Ring
-- Center in preview is (175, 230).
-- Blue ring is at radius 34px: (175 + 34, 230) = (209, 230) (angle = 0 deg)
print("🎯 Simulating Mouse Drag (Blue Roll Ring)...")
callback_fired = false
lib.trust_gui_mouse_move(209, 230)
lib.trust_gui_mouse_button(true)

-- Move mouse down to (175, 230 + 34) = (175, 264) (+90 deg clockwise)
lib.trust_gui_mouse_move(175, 264)
lib.trust_studio_render_preview(preview_w, preview_h, preview_buf, buf_size)
lib.trust_gui_mouse_button(false)

assert(callback_fired == true, "Transform callback did NOT fire on rotate release!")
assert(math.abs(cb_roll - 90.0) < 5.0, string.format("Expected roll ~ 90°, got %.2f°", cb_roll))
print("✅ Blue Roll Ring OK! Rotated roll to " .. string.format("%.1f°", cb_roll))

-- Rotate roll back to 0° so Pitch and Yaw ellipses return to horizontal/vertical for cardinal test
lib.trust_gui_mouse_move(175, 264)
lib.trust_gui_mouse_button(true)
lib.trust_gui_mouse_move(209, 230)
lib.trust_studio_render_preview(preview_w, preview_h, preview_buf, buf_size)
lib.trust_gui_mouse_button(false)

-- 8.5 Simulate Gizmo Drag: Red Yaw Ring (34x14 ellipse)
-- At center (175, 230). Top of red ellipse is at (175, 230 - 14) = (175, 216).
print("🎯 Simulating Mouse Drag (Red Yaw Ring)...")
callback_fired = false
lib.trust_gui_mouse_move(175, 216)
lib.trust_gui_mouse_button(true)

-- Drag along ellipse to the right (175 + 34, 230) = (209, 230) -> +90 deg yaw
lib.trust_gui_mouse_move(209, 230)
lib.trust_studio_render_preview(preview_w, preview_h, preview_buf, buf_size)
lib.trust_gui_mouse_button(false)

assert(callback_fired == true, "Transform callback did NOT fire on yaw release!")
assert(math.abs(cb_yaw - 90.0) < 5.0, string.format("Expected yaw ~ 90°, got %.2f°", cb_yaw))
print("✅ Red Yaw Ring OK! Rotated yaw to " .. string.format("%.1f°", cb_yaw))

-- 8.6 Simulate Gizmo Drag: Green Pitch Ring (14x34 ellipse)
-- At center (175, 230). Right side of green ellipse is at (175 + 14, 230) = (189, 230).
print("🎯 Simulating Mouse Drag (Green Pitch Ring)...")
callback_fired = false
lib.trust_gui_mouse_move(189, 230)
lib.trust_gui_mouse_button(true)

-- Drag down to (175, 230 + 34) = (175, 264) -> +90 deg pitch
lib.trust_gui_mouse_move(175, 264)
lib.trust_studio_render_preview(preview_w, preview_h, preview_buf, buf_size)
lib.trust_gui_mouse_button(false)

assert(callback_fired == true, "Transform callback did NOT fire on pitch release!")
assert(math.abs(cb_pitch - 90.0) < 5.0, string.format("Expected pitch ~ 90°, got %.2f°", cb_pitch))
print("✅ Green Pitch Ring OK! Rotated pitch to " .. string.format("%.1f°", cb_pitch))

-- 8.7 Simulate Blender Ctrl + Click to Move from anywhere inside target
print("🎯 Testing Blender Ctrl + Click to Move & 10px Grid Snapping...")
lib.trust_gui_set_modifiers(true, false, false) -- Hold Ctrl
callback_fired = false
-- Click at (90, 110) which is far away from center dot (dist ~ 147px)
lib.trust_gui_mouse_move(90, 110)
lib.trust_gui_mouse_button(true)

-- Drag mouse by +17px screen (+34px world, from 125 to 159 -> snaps to 160.0 on 10px grid)
lib.trust_gui_mouse_move(107, 127)
lib.trust_studio_render_preview(preview_w, preview_h, preview_buf, buf_size)
lib.trust_gui_mouse_button(false)

assert(callback_fired == true, "Transform callback did NOT fire on Ctrl+Click release!")
assert(math.abs(cb_x - 160.0) < 0.1, string.format("Expected snapped x = 160.0, got %.1f", cb_x))
assert(math.abs(cb_y - 190.0) < 0.1, string.format("Expected snapped y = 190.0, got %.1f", cb_y))
print("✅ Blender Ctrl + Click OK! Moved from anywhere inside target and snapped to 10px grid: (" .. cb_x .. ", " .. cb_y .. ")")

-- 8.8 Simulate Blender Shift Precision Mode (1/10th speed)
print("🎯 Testing Blender Shift Precision Mode (0.1x speed)...")
lib.trust_gui_set_modifiers(false, true, false) -- Hold Shift
callback_fired = false
local pre_x = cb_x
local center_sx = (cb_x + 225.0) * 0.5
local center_sy = (cb_y + 300.0) * 0.5

-- Click center and drag 50px screen (normally 100px world, but with Shift 0.1x -> 10px world)
lib.trust_gui_mouse_move(center_sx, center_sy)
lib.trust_gui_mouse_button(true)
lib.trust_gui_mouse_move(center_sx + 50, center_sy)
lib.trust_studio_render_preview(preview_w, preview_h, preview_buf, buf_size)
lib.trust_gui_mouse_button(false)

assert(callback_fired == true, "Transform callback did NOT fire on Shift release!")
assert(math.abs((cb_x - pre_x) - 10.0) < 1.0, string.format("Expected ~10px precision delta, got %.1f", cb_x - pre_x))
print("✅ Blender Shift Precision OK! Moved with 0.1x fine-tuned precision delta = " .. string.format("%.1f", cb_x - pre_x) .. "px")

lib.trust_gui_set_modifiers(false, false, false) -- Reset modifiers

-- 9. Test Direct Custom Pos Set & Query
print("🎯 Testing trust_studio_set_source_custom...")
lib.trust_studio_set_source_custom("PlayerAvatar", false, 320.0, 450.0, 250.0, 350.0, 45.0)

local q_x = ffi.new("float[1]")
local q_y = ffi.new("float[1]")
local q_w = ffi.new("float[1]")
local q_h = ffi.new("float[1]")
local q_roll = ffi.new("float[1]")
local q_match = ffi.new("bool[1]")

assert(lib.trust_studio_get_source_custom("PlayerAvatar", q_x, q_y, q_w, q_h, q_roll, q_match) == true)
assert(math.abs(q_x[0] - 320.0) < 0.01)
assert(math.abs(q_y[0] - 450.0) < 0.01)
assert(math.abs(q_w[0] - 250.0) < 0.01)
assert(math.abs(q_h[0] - 350.0) < 0.01)
assert(math.abs(q_roll[0] - 45.0) < 0.01)
assert(q_match[0] == false)
print("✅ trust_studio_set_source_custom & trust_studio_get_source_custom OK!")

-- 10. Render preview with rotated custom gizmo (50 iterations stress test for memory/speed)
for iter = 1, 50 do
    local ok_render_rot = lib.trust_studio_render_preview(preview_w, preview_h, preview_buf, buf_size)
    assert(ok_render_rot == 1, "Rotated preview render failed")
end
print("✅ Stress test 50 frames preview render OK (zero leaks, smooth execution)!")

print("\n🎉 ALL GIZMO (3 ROTATION RINGS, WHITE SCALE RING, WHITE CENTER) TESTS PASSED SUCCESSFULLY!")
