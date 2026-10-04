local ffi = require("ffi")

ffi.cdef[[
    bool trust_init(void);
    void trust_set_canvas_resolution(uint32_t w, uint32_t h);
    void trust_studio_clear_sources(void);
    void trust_studio_add_source_ex(const char *name, uint32_t slot, float x, float y, float width, float height, float roll, bool is_target, bool match_obs);
    void trust_studio_set_selected_source(const char *name);
    uint32_t trust_studio_render_preview(uint32_t canvas_w, uint32_t canvas_h, uint8_t *out_rgba, size_t max_len);
]]

local lib = ffi.load("./Trust/target/release/libtrust.so")
assert(lib.trust_init())

lib.trust_set_canvas_resolution(1920, 1080)
lib.trust_studio_clear_sources()
-- Place a target in center
lib.trust_studio_add_source_ex("Avatar", 1, 800, 400, 320, 280, 0, true, true)
lib.trust_studio_set_selected_source("Avatar")

local w, h = 960, 540
local buf_size = w * h * 4
local buf = ffi.new("uint8_t[?]", buf_size)
assert(lib.trust_studio_render_preview(w, h, buf, buf_size) == 1)

-- Check for presence of Gizmo colors:
-- Red Rotation Ring: [239, 68, 68]
-- Green Rotation Ring: [34, 197, 94]
-- Blue Rotation Ring: [59, 130, 246]
-- White Center & Scale Ring: [255, 255, 255]

local found_red = false
local found_green = false
local found_blue = false
local found_white = false

for i = 0, (w * h - 1) do
    local r = buf[i * 4]
    local g = buf[i * 4 + 1]
    local b = buf[i * 4 + 2]

    if r == 239 and g == 68 and b == 68 then found_red = true end
    if r == 34 and g == 197 and b == 94 then found_green = true end
    if r == 59 and g == 130 and b == 246 then found_blue = true end
    if r == 255 and g == 255 and b == 255 then found_white = true end
end

print("Red Rotation Ring present: " .. tostring(found_red))
print("Green Rotation Ring present: " .. tostring(found_green))
print("Blue Rotation Ring present: " .. tostring(found_blue))
print("White Scale Ring & Center present: " .. tostring(found_white))

assert(found_red, "Red Rotation Ring was not rendered!")
assert(found_green, "Green Rotation Ring was not rendered!")
assert(found_blue, "Blue Rotation Ring was not rendered!")
assert(found_white, "White Scale Ring / Center was not rendered!")

print("✅ All Transform Gizmo elements (3 Rotation Rings, White Scale Ring, White Center) rendered correctly!")
