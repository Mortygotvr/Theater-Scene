-- crack_radial.lua
-- OBS Lua filter: Radial Crack (shattered glass with radial cracks and offset pieces)
-- Cracks radiate from the center, offsetting image zones along the cracks and a broken ring

obs = obslua

local shader_code = [[
uniform float4x4 ViewProj;
uniform texture2d image;

uniform float time;
uniform float scale_factor;
// no crack_speed, instant effect
uniform float crack_count;
uniform float crack_width;
uniform float ring_radius;
uniform float ring_width;
uniform float displacement;
uniform float randomness;

sampler_state textureSampler {
    Filter    = Linear;
    AddressU  = Border;
    AddressV  = Border;
    BorderColor = 00000000;
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

// Simple hash for pseudo-randomness per crack
float hash(float x) {
    return frac(sin(x * 91.3458) * 47453.5453);
}

float mod(float x, float y) {
    return x - y * floor(x / y);
}

float4 PSDefault(VertOut v_in) : TARGET {
    float2 uv = v_in.uv;
    float2 center = float2(0.5, 0.5);
    float2 rel = uv - center;
    float r = length(rel);
    float theta = atan2(rel.y, rel.x);
    float t = 1.0; // instant effect
    float max_crack = 0.0;
    float2 crack_offset = float2(0.0, 0.0);
    for (int i = 0; i < 16; ++i) {
        if (i >= int(crack_count)) break;
        float angle = (6.2831853 / crack_count) * i + hash(i) * randomness;
        float dtheta = abs(mod(theta - angle + 3.1415927, 6.2831853) - 3.1415927);
        float crack = 1.0 - smoothstep(0.0, crack_width, dtheta); // wider, more visible cracks
        float piece_disp = t * displacement * crack;
        crack_offset += normalize(rel) * piece_disp * crack;
        max_crack = max(max_crack, crack);
    }
    // Broken ring (circle)
    float ring = 1.0 - smoothstep(0.0, ring_width, abs(r - ring_radius));
    float ring_disp = t * displacement * ring;
    crack_offset += normalize(rel) * ring_disp * ring;
    // Apply offset to regions near cracks or ring
    float2 cracked_uv = uv + crack_offset;

    float4 color = image.Sample(textureSampler, cracked_uv);
    return color;
}

technique Draw {
    pass {
        vertex_shader = VSDefault(v_in);
        pixel_shader  = PSDefault(v_in);
    }
}
]]

local filter_info = {}
filter_info.id = "custom_crack_radial_filter"
filter_info.type = obs.OBS_SOURCE_TYPE_FILTER
filter_info.output_flags = obs.OBS_SOURCE_VIDEO

filter_info.get_name = function(settings)
    return "Crack Radial (Shattered Glass)"
end

filter_info.get_defaults = function(settings)
    obs.obs_data_set_default_double(settings, "scale_factor", 1.0)
    obs.obs_data_set_default_double(settings, "crack_count", 7.0)
    obs.obs_data_set_default_double(settings, "crack_width", 0.15)
    obs.obs_data_set_default_double(settings, "ring_radius", 0.25)
    obs.obs_data_set_default_double(settings, "ring_width", 0.07)
    obs.obs_data_set_default_double(settings, "displacement", 0.18)
    obs.obs_data_set_default_double(settings, "randomness", 0.7)
end

filter_info.create = function(settings, source)
    local data = {}
    data.source = source
    obs.obs_enter_graphics()
    data.effect = obs.gs_effect_create(shader_code, nil, nil)
    obs.obs_leave_graphics()
    if data.effect == nil then
        obs.blog(obs.LOG_ERROR, "Effect compilation failed!")
    end
    data.scale_factor = 1.0
    data.crack_speed = 0.7
    data.crack_count = 7.0
    data.crack_width = 0.15
    data.ring_radius = 0.25
    data.ring_width = 0.07
    data.displacement = 0.18
    data.randomness = 0.7
    data.start_time = obs.os_gettime_ns()
    filter_info.update(data, settings)
    return data
end

filter_info.show = function(data)
    if data then
        data.start_time = obs.os_gettime_ns()
    end
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
    data.scale_factor = obs.obs_data_get_double(settings, "scale_factor")
    data.crack_count = obs.obs_data_get_double(settings, "crack_count")
    data.crack_width = obs.obs_data_get_double(settings, "crack_width")
    data.ring_radius = obs.obs_data_get_double(settings, "ring_radius")
    data.ring_width = obs.obs_data_get_double(settings, "ring_width")
    data.displacement = obs.obs_data_get_double(settings, "displacement")
    data.randomness = obs.obs_data_get_double(settings, "randomness")
end

filter_info.video_render = function(data, effect)
    if not data.effect then
        obs.obs_source_skip_video_filter(data.source)
        return
    end
    if not obs.obs_source_process_filter_begin(data.source, obs.GS_RGBA, obs.OBS_NO_DIRECT_RENDERING) then
        return
    end
    local now = obs.os_gettime_ns()
    if data.last_render_time == nil or (now - data.last_render_time > 200000000) then
        data.start_time = now
    end
    data.last_render_time = now
    local time = (now - (data.start_time or now)) / 1000000000.0
    obs.gs_effect_set_float(obs.gs_effect_get_param_by_name(data.effect, "time"), time)
    obs.gs_effect_set_float(obs.gs_effect_get_param_by_name(data.effect, "scale_factor"), data.scale_factor)
    obs.gs_effect_set_float(obs.gs_effect_get_param_by_name(data.effect, "crack_count"), data.crack_count)
    obs.gs_effect_set_float(obs.gs_effect_get_param_by_name(data.effect, "crack_width"), data.crack_width)
    obs.gs_effect_set_float(obs.gs_effect_get_param_by_name(data.effect, "ring_radius"), data.ring_radius)
    obs.gs_effect_set_float(obs.gs_effect_get_param_by_name(data.effect, "ring_width"), data.ring_width)
    obs.gs_effect_set_float(obs.gs_effect_get_param_by_name(data.effect, "displacement"), data.displacement)
    obs.gs_effect_set_float(obs.gs_effect_get_param_by_name(data.effect, "randomness"), data.randomness)
    obs.obs_source_process_filter_end(data.source, data.effect, 0, 0)
end

filter_info.get_properties = function(data)
    local props = obs.obs_properties_create()
    obs.obs_properties_add_float_slider(props, "scale_factor", "Safe Zone Scale", 0.1, 1.0, 0.05)
    obs.obs_properties_add_int_slider(props, "crack_count", "Crack Count", 2, 16, 1)
    obs.obs_properties_add_float_slider(props, "crack_width", "Crack Width", 0.05, 0.5, 0.01)
    obs.obs_properties_add_float_slider(props, "ring_radius", "Ring Radius", 0.05, 0.45, 0.01)
    obs.obs_properties_add_float_slider(props, "ring_width", "Ring Width", 0.01, 0.2, 0.01)
    obs.obs_properties_add_float_slider(props, "displacement", "Displacement", 0.01, 0.5, 0.01)
    obs.obs_properties_add_float_slider(props, "randomness", "Randomness", 0.0, 1.0, 0.01)
    return props
end

function script_description()
    return "Crack Radial: Simulates a shattered glass effect with radial cracks and offset pieces. Includes scale."
end

function script_load(settings)
    obs.obs_register_source(filter_info)
end
