-- transparency_fix.lua
-- OBS Lua filter: Transparency Fix for captures with an alpha channel
-- (Game Capture "Allow Transparency", Spout, VTuber apps...)
--
-- OBS draws a capture as straight alpha. Many games/apps don't write that:
--  * Premultiplied: colour is already multiplied by alpha. OBS multiplies again,
--    so translucent parts come out darker/fainter and additive glows (alpha 0)
--    vanish entirely.
--  * Premultiplied + squared alpha: translucency blended onto a cleared,
--    transparent background with the same blend on the alpha channel leaves
--    alpha = a*a (typical of Unity-based apps). OBS then shows colour*a^3 over
--    a mostly visible background, so translucent objects nearly disappear.
-- The filter pipeline copies the source without blending, so the shader sees
-- the raw pixels, reinterprets them, and draws them premultiplied.

obs = obslua

local shader_code = [[
uniform float4x4 ViewProj;
uniform texture2d image;

uniform float alpha_mode;  // 1 = premultiplied, 2 = premultiplied + squared alpha
uniform float alpha_boost; // >= 1 firms up partial alpha (1 = unchanged)

sampler_state textureSampler {
    Filter    = Linear;
    AddressU  = Clamp;
    AddressV  = Clamp;
};

struct VertData {
    float4 pos : POSITION;
    float2 uv  : TEXCOORD0;
};

struct VertOut {
    float4 pos : POSITION;
    float2 uv  : TEXCOORD0;
};

VertOut VSDefault(VertData v_in) {
    VertOut vert_out;
    vert_out.pos = mul(float4(v_in.pos.xyz, 1.0), ViewProj);
    vert_out.uv  = v_in.uv;
    return vert_out;
}

float4 PSFix(VertOut v_in) : TARGET {
    // Raw pixel as the capture wrote it
    float4 c = image.Sample(textureSampler, v_in.uv);
    float a = saturate(c.a);
    if (alpha_mode > 1.5) {
        a = sqrt(a);
    }
    a = pow(a, 1.0 / max(alpha_boost, 1.0));
    // Colour is already premultiplied: output premultiplied, drawn with ONE / INVSRCALPHA
    return float4(c.rgb, a);
}

technique Draw {
    pass {
        vertex_shader = VSDefault(v_in);
        pixel_shader  = PSFix(v_in);
    }
}
]]

local MODE_STANDARD = 0
local MODE_PREMULTIPLIED = 1
local MODE_SQUARED = 2

local filter_info = {}
filter_info.id = "theater_transparency_fix_filter"
filter_info.type = obs.OBS_SOURCE_TYPE_FILTER
filter_info.output_flags = obs.OBS_SOURCE_VIDEO

filter_info.get_name = function(settings)
    return "Theater Transparency Fix"
end

filter_info.get_defaults = function(settings)
    obs.obs_data_set_default_int(settings, "alpha_mode", MODE_SQUARED)
    obs.obs_data_set_default_double(settings, "alpha_boost", 1.0)
end

filter_info.create = function(settings, source)
    local data = {}
    data.source = source
    obs.obs_enter_graphics()
    data.effect = obs.gs_effect_create(shader_code, nil, nil)
    obs.obs_leave_graphics()
    if data.effect == nil then
        obs.blog(obs.LOG_ERROR, "[Transparency Fix] Effect compilation failed!")
    end
    filter_info.update(data, settings)
    return data
end

filter_info.destroy = function(data)
    if data.effect ~= nil then
        obs.obs_enter_graphics()
        obs.gs_effect_destroy(data.effect)
        data.effect = nil
        obs.obs_leave_graphics()
    end
end

filter_info.update = function(data, settings)
    data.alpha_mode = obs.obs_data_get_int(settings, "alpha_mode")
    data.alpha_boost = obs.obs_data_get_double(settings, "alpha_boost")
end

filter_info.video_render = function(data, effect)
    if not data.effect or data.alpha_mode == MODE_STANDARD then
        obs.obs_source_skip_video_filter(data.source)
        return
    end
    if not obs.obs_source_process_filter_begin(data.source, obs.GS_RGBA, obs.OBS_NO_DIRECT_RENDERING) then
        return
    end
    obs.gs_effect_set_float(obs.gs_effect_get_param_by_name(data.effect, "alpha_mode"), data.alpha_mode)
    obs.gs_effect_set_float(obs.gs_effect_get_param_by_name(data.effect, "alpha_boost"), data.alpha_boost)
    -- The shader outputs premultiplied colour
    obs.gs_blend_state_push()
    obs.gs_blend_function_separate(obs.GS_BLEND_ONE, obs.GS_BLEND_INVSRCALPHA, obs.GS_BLEND_ONE, obs.GS_BLEND_INVSRCALPHA)
    obs.obs_source_process_filter_end(data.source, data.effect, 0, 0)
    obs.gs_blend_state_pop()
end

filter_info.get_properties = function(data)
    local props = obs.obs_properties_create()
    local mode = obs.obs_properties_add_list(props, "alpha_mode", "Capture alpha", obs.OBS_COMBO_TYPE_LIST, obs.OBS_COMBO_FORMAT_INT)
    obs.obs_property_list_add_int(mode, "Standard (as OBS draws it)", MODE_STANDARD)
    obs.obs_property_list_add_int(mode, "Premultiplied", MODE_PREMULTIPLIED)
    obs.obs_property_list_add_int(mode, "Premultiplied + squared alpha (Unity / VTuber apps)", MODE_SQUARED)
    obs.obs_properties_add_float_slider(props, "alpha_boost", "Opacity boost (1 = off)", 1.0, 30.0, 0.05)
    return props
end

function script_description()
    return "Theater Transparency Fix: keeps translucent parts of captures with transparency (game capture, Spout, VTuber apps) from fading out or disappearing. Pick the capture alpha mode that looks right."
end

function script_load(settings)
    -- Theater Scene runs this filter from transparency_fix.rhai when that's
    -- there: its id is taken. Registering it again made OBS free this script's
    -- filter definition but keep it listed, and OBS crashed unloading scripts
    -- on exit. This copy stays out of the way then.
    if obs.obs_source_get_display_name(filter_info.id) ~= nil then
        print("Theater Transparency Fix runs from Theater Scene's Rhai version: this Lua copy is idle")
        return
    end
    obs.obs_register_source(filter_info)
end
