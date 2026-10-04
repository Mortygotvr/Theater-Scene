-- ============================================================================
-- 🧪 Comprehensive Throwing & Redeem System Integration Test
-- ============================================================================

local ffi = require("ffi")

ffi.cdef[[
    bool trust_init();
    uint32_t trust_decode_image_base64(const char* base64_str, uint8_t* out_rgba_buf, uint32_t max_bytes, uint32_t* out_frame_width, uint32_t* out_frame_height, uint32_t* out_frame_count, uint32_t* out_frame_delay_ms);
    uint32_t trust_encode_file_base64(const char* file_path, const char* mime_type, char* out_buf, size_t max_len);
    uint32_t trust_read_file_text(const char* file_path, char* out_buf, size_t max_len);
    bool trust_write_file_text(const char* file_path, const char* text_content);
    uint32_t trust_list_redeem_keys(const char* dir_path, char* out_buf, size_t max_len);
    uint32_t trust_render_text(const char* text, uint32_t scale, uint32_t fg_rgba, uint32_t bg_rgba, uint8_t* out_rgba, size_t max_buf_len, uint32_t* out_w, uint32_t* out_h);
    uint32_t trust_generate_font_atlas(uint8_t* out_rgba, size_t max_buf_len, uint32_t* out_w, uint32_t* out_h);
    void trust_set_editor_triggers(const char* triggers_csv);
    bool trust_is_editor_trigger(const char* trigger_str);
    void trust_clear_editor_triggers();
    bool trust_set_target_slot(size_t slot, const char* target_name);
    uint32_t trust_get_target_slot_name(size_t slot, char* out_buf, size_t max_len);
]]
local trust = ffi.load("Trust/target/release/libtrust.so")
assert(trust.trust_init(), "trust_init should succeed")

print("\n--- 1. Testing Rust File IO & Listing ---")
local k_buf = ffi.new("char[16384]")
local k_len = trust.trust_list_redeem_keys("redeems", k_buf, 16384)
assert(k_len > 0, "trust_list_redeem_keys should find files")
local keys_json = ffi.string(k_buf, k_len)
assert(keys_json:find("tomato"), "Keys should contain tomato")
print("✅ Listed redeem keys via Rust: " .. keys_json)

local file_buf = ffi.new("char[1024 * 1024]")
local read_len = trust.trust_read_file_text("redeems/tomato.json", file_buf, 1024 * 1024)
assert(read_len > 0, "trust_read_file_text should read tomato.json")
local content = ffi.string(file_buf, read_len)
assert(content:find('"name"%s*:%s*"Tomato"'), "Content should contain Tomato")
print("✅ Read tomato.json via Rust: " .. read_len .. " bytes")

local img_b64 = content:match('"imageSrc"%s*:%s*"([^"]+)"')
assert(img_b64 ~= nil, "imageSrc must exist")

print("\n--- 2. Testing Trust Core Image Base64 Decoding ---")
local buf = ffi.new("uint8_t[1024 * 1024 * 4]")
local out_fw = ffi.new("uint32_t[1]")
local out_fh = ffi.new("uint32_t[1]")
local out_fc = ffi.new("uint32_t[1]")
local out_fd = ffi.new("uint32_t[1]")
local num_bytes = trust.trust_decode_image_base64(img_b64, buf, 1024*1024*4, out_fw, out_fh, out_fc, out_fd)
assert(num_bytes > 0, "Decoded bytes must be > 0")
assert(out_fh[0] > 0, "Decoded height must be > 0")
print("✅ Decoded tomato image (native size): " .. out_fw[0] .. "x" .. out_fh[0] .. " (frames=" .. out_fc[0] .. ", delay=" .. out_fd[0] .. "ms, " .. num_bytes .. " bytes)")

print("\n--- 3. Testing Trust Core File Base64 Encoding ---")
local enc_buf = ffi.new("char[1024*1024]")
local enc_len = trust.trust_encode_file_base64("Trust/Cargo.toml", "text/plain", enc_buf, 1024*1024)
assert(enc_len > 0, "Encoded length must be > 0")
local enc_str = ffi.string(enc_buf, enc_len)
assert(enc_str:find("^data:text/plain;base64,"), "Encoded string must have data URL prefix")
print("✅ Encoded file into Data URL: " .. enc_len .. " chars")

print("\n--- 3.5. Testing Trust Font Rasterization & Text Engine ---")
local text_buf = ffi.new("uint8_t[65536]")
local tw = ffi.new("uint32_t[1]")
local th = ffi.new("uint32_t[1]")
local text_bytes = trust.trust_render_text("Slot 1: Avatar", 1, 0xFFFFFFFF, 0x00000000, text_buf, 65536, tw, th)
assert(text_bytes > 0, "Rendered text bytes must be > 0")
assert(tw[0] == 14 * 8, "Rendered width for 14 chars must be 112px")
assert(th[0] == 16, "Rendered height must be 16px")
print("✅ Rasterized text 'Slot 1: Avatar' -> " .. tw[0] .. "x" .. th[0] .. " RGBA pixels (" .. text_bytes .. " bytes)")

print("\n--- 4. Testing Rust Target Slot Engine ---")
trust.trust_set_target_slot(1, "Avatar")

local target_name_buf = ffi.new("char[256]")
local slot_len = trust.trust_get_target_slot_name(1, target_name_buf, 256)
local resolved_target = ffi.string(target_name_buf, slot_len)
assert(resolved_target == "Avatar", "Slot 1 must resolve to Avatar")
print("✅ Target slot resolution verified: Slot 1 -> " .. resolved_target)

print("\n--- 5. Testing Editor Triggers & Suppression ---")
trust.trust_set_editor_triggers("Twitch Mortygotvr redeem Throw Tomato")
assert(trust.trust_is_editor_trigger("Twitch Mortygotvr redeem Throw Tomato") == true, "The Reader event must be identified as editor trigger")
trust.trust_clear_editor_triggers()
assert(trust.trust_is_editor_trigger("Twitch Mortygotvr redeem Throw Tomato") == false, "Cleared editor trigger must return false")
print("✅ Editor trigger suppression verified.")

print("\n--- 6. Testing theater_editor.lua Initialization ---")
dofile("theater_editor.lua")
script_load({})
print("✅ theater_editor.lua successfully initialized.")
print("✅ theater_editor.lua successfully initialized.")

print("\n🎉 ALL THROWING SYSTEM INTEGRATION TESTS PASSED!")
