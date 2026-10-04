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

    typedef struct {
        float pos_x;
        float pos_y;
        float scale_x;
        float scale_y;
        uint32_t crop_left;
        uint32_t crop_top;
        uint32_t crop_right;
        uint32_t crop_bottom;
        float rot_deg;
    } SourceTransform;

    bool trust_init();
    void trust_shutdown();
    bool trust_connect_reader(uint16_t port);
    bool trust_is_reader_connected();
    void trust_update_config(uint8_t mode, uint8_t alpha_threshold, uint8_t r, uint8_t g, uint8_t b, uint8_t threshold, uint8_t precision);
    Hitbox trust_process_rgba_transformed(const char* target_name, const uint8_t* data, uint32_t width, uint32_t height, uint32_t stride, SourceTransform transform);
    uint32_t trust_get_pixel_grid(const char* target_name, Point2D* out_points, uint32_t max_points);
    bool trust_get_hitbox(const char* target_name, Hitbox* out_hitbox);
    bool trust_is_pixel_solid(const char* target_name, float world_x, float world_y);
    uint32_t trust_get_target_count();
    bool trust_get_target_name(uint32_t index, char* out_name_buf, size_t max_len);
    void trust_remove_target(const char* target_name);
    void trust_clear_targets();
    bool trust_poll_event(char* out_json_buf, size_t max_len);
    uint32_t trust_get_event_count();
]]

local lib = ffi.load("./Trust/target/release/libtrust.so")
assert(lib, "Failed to load libtrust.so")

print("1. Testing trust_init()...")
assert(lib.trust_init() == true)
print("   -> OK")

print("2. Configuring Alpha Mode (Mode 0, alpha_threshold 10, precision 10)...")
lib.trust_update_config(0, 10, 0, 0, 0, 0, 10)
print("   -> OK")

-- Create an RGBA buffer for "Avatar" (50x50, with a 20x20 opaque character at local 10,10)
local w, h = 50, 50
local stride = w * 4
local avatar_buf = ffi.new("uint8_t[?]", h * stride)

for y = 10, 29 do
    for x = 10, 29 do
        local idx = (y * stride) + (x * 4)
        avatar_buf[idx + 0] = 255 -- R
        avatar_buf[idx + 1] = 100 -- G
        avatar_buf[idx + 2] = 150 -- B
        avatar_buf[idx + 3] = 255 -- Alpha (Opaque)
    end
end

-- Avatar Transform: Placed at (200, 150) in 1920x1080 canvas, scaled 1.5x
local avatar_xform = ffi.new("SourceTransform")
avatar_xform.pos_x = 200
avatar_xform.pos_y = 150
avatar_xform.scale_x = 1.5
avatar_xform.scale_y = 1.5

print("3. Processing Target 1: 'Avatar' with Stream Transform...")
local av_hb = lib.trust_process_rgba_transformed("Avatar", avatar_buf, w, h, stride, avatar_xform)
assert(av_hb.active == true)
assert(av_hb.local_x == 10 and av_hb.local_y == 10)
assert(av_hb.local_width == 20 and av_hb.local_height == 20)
-- World X: 200 + 10 * 1.5 = 215, World Width: 20 * 1.5 = 30
assert(av_hb.world_x == 215)
assert(av_hb.world_y == 165)
assert(av_hb.world_width == 30)
assert(av_hb.world_height == 30)
print(string.format("   -> Avatar World Hitbox: (%.1f, %.1f) size %.1fx%.1f, center (%.1f, %.1f)",
    av_hb.world_x, av_hb.world_y, av_hb.world_width, av_hb.world_height, av_hb.world_center_x, av_hb.world_center_y))

-- Create an RGBA buffer for Target 2: "Chat" (30x30, opaque box at 0,0)
local chat_buf = ffi.new("uint8_t[?]", 30 * 30 * 4)
for i = 0, (30 * 30) - 1 do
    chat_buf[i * 4 + 0] = 50
    chat_buf[i * 4 + 1] = 50
    chat_buf[i * 4 + 2] = 50
    chat_buf[i * 4 + 3] = 200 -- Semi-transparent alpha (solid)
end

-- Chat Transform: Placed at (1400, 300), 1.0x scale
local chat_xform = ffi.new("SourceTransform")
chat_xform.pos_x = 1400
chat_xform.pos_y = 300
chat_xform.scale_x = 1.0
chat_xform.scale_y = 1.0

print("4. Processing Target 2: 'Chat' (Multi-Source Targeting)...")
local chat_hb = lib.trust_process_rgba_transformed("Chat", chat_buf, 30, 30, 30 * 4, chat_xform)
assert(chat_hb.active == true)
assert(chat_hb.world_x == 1400 and chat_hb.world_y == 300)
assert(chat_hb.world_width == 30 and chat_hb.world_height == 30)
print(string.format("   -> Chat World Hitbox: (%.1f, %.1f) size %.1fx%.1f",
    chat_hb.world_x, chat_hb.world_y, chat_hb.world_width, chat_hb.world_height))

print("5. Verifying Multi-Target Retrieval...")
assert(lib.trust_get_target_count() == 2, "Should have 2 tracked targets")
local name_buf = ffi.new("char[64]")
lib.trust_get_target_name(0, name_buf, 64)
local t1 = ffi.string(name_buf)
lib.trust_get_target_name(1, name_buf, 64)
local t2 = ffi.string(name_buf)
print(string.format("   -> Tracked Targets: '%s', '%s'", t1, t2))

print("6. Testing World-Space Pixel-Solid Checks & Pixel Scan...")
assert(lib.trust_is_pixel_solid("Avatar", 220, 170) == true, "Avatar pixel inside hitbox should be solid")
assert(lib.trust_is_pixel_solid("Avatar", 50, 50) == false, "Avatar pixel outside should be empty")
assert(lib.trust_is_pixel_solid("Chat", 1410, 310) == true, "Chat pixel inside hitbox should be solid")
assert(lib.trust_is_pixel_solid("Chat", 200, 200) == false, "Chat pixel outside should be empty")

local pts = ffi.new("Point2D[600]")
local num_pts = lib.trust_get_pixel_grid("Avatar", pts, 600)
assert(num_pts > 0, "Avatar should have sampled pixel grid points")
print(string.format("   -> Avatar Pixel Scan: Retrieved %d solid point-cloud vertices (e.g. (%.1f, %.1f))", 
    num_pts, pts[0].x, pts[0].y))
print("   -> OK")

print("7. Testing Rotation Transform (90 degrees)...")
local rot_xform = ffi.new("SourceTransform")
rot_xform.pos_x = 500
rot_xform.pos_y = 500
rot_xform.scale_x = 1.0
rot_xform.scale_y = 1.0
rot_xform.rot_deg = 90.0

local rot_hb = lib.trust_process_rgba_transformed("RotatedTarget", avatar_buf, w, h, stride, rot_xform)
assert(rot_hb.active == true)
print(string.format("   -> Rotated 90deg Hitbox: World=(%.1f, %.1f) size %.1fx%.1f, center=(%.1f, %.1f)",
    rot_hb.world_x, rot_hb.world_y, rot_hb.world_width, rot_hb.world_height, rot_hb.world_center_x, rot_hb.world_center_y))
assert(lib.trust_is_pixel_solid("RotatedTarget", rot_hb.world_center_x, rot_hb.world_center_y) == true, "Rotated center should be solid")
print("   -> OK")

print("8. Testing trust_shutdown()...")
lib.trust_shutdown()
print("   -> OK")

print("\n🎉 ALL ALPHA, SCENE TRANSFORM & MULTI-TARGET TESTS PASSED PERFECTLY!")
