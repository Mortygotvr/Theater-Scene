-- shatter.lua
-- OBS Lua filter: Shatter (shattered glass effect)
-- Includes scale_factor and robust animation reset

obs = obslua

local shader_code = [[
uniform float4x4 ViewProj;
uniform texture2d image;

uniform float time;
uniform float scale_factor;
uniform float shatter_speed;
uniform float piece_size;
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

// Simple hash for pseudo-randomness per piece
float hash(float2 p) {
    return frac(sin(dot(p, float2(12.9898, 78.233))) * 43758.5453);
}

float4 PSDefault(VertOut v_in) : TARGET {
    float2 uv = v_in.uv;
    // Divide image into pieces
    float2 piece_grid = floor(uv / piece_size);
    float2 piece_center = (piece_grid + 0.5) * piece_size;
    // Each piece gets a random direction and speed
    float angle = hash(piece_grid) * 6.2831853; // [0, 2pi]
    float dist = hash(piece_grid + 0.5) * randomness;
    float shatter = min(1.0, time * shatter_speed);
    float2 offset = float2(cos(angle), sin(angle)) * dist * shatter;
    // Move the piece outward
    float2 shattered_uv = uv + offset;
    float4 color = image.Sample(textureSampler, shattered_uv);
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
filter_info.id = "custom_shatter_filter"
filter_info.type = obs.OBS_SOURCE_TYPE_FILTER
filter_info.output_flags = obs.OBS_SOURCE_VIDEO

filter_info.get_name = function(settings)
    return "Shatter (Glass Effect)"
end

filter_info.get_defaults = function(settings)
    obs.obs_data_set_default_double(settings, "scale_factor", 1.0)
    obs.obs_data_set_default_double(settings, "shatter_speed", 0.7)
    obs.obs_data_set_default_double(settings, "piece_size", 0.08)
    obs.obs_data_set_default_double(settings, "randomness", 0.5)
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
    data.shatter_speed = 0.7
    data.piece_size = 0.08
    data.randomness = 0.5
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
    data.shatter_speed = obs.obs_data_get_double(settings, "shatter_speed")
    data.piece_size = obs.obs_data_get_double(settings, "piece_size")
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
    obs.gs_effect_set_float(obs.gs_effect_get_param_by_name(data.effect, "shatter_speed"), data.shatter_speed)
    obs.gs_effect_set_float(obs.gs_effect_get_param_by_name(data.effect, "piece_size"), data.piece_size)
    obs.gs_effect_set_float(obs.gs_effect_get_param_by_name(data.effect, "randomness"), data.randomness)
    obs.obs_source_process_filter_end(data.source, data.effect, 0, 0)
end

filter_info.get_properties = function(data)
    local props = obs.obs_properties_create()
    obs.obs_properties_add_float_slider(props, "scale_factor", "Safe Zone Scale", 0.1, 1.0, 0.05)
    obs.obs_properties_add_float_slider(props, "shatter_speed", "Shatter Speed", 0.1, 2.0, 0.01)
    obs.obs_properties_add_float_slider(props, "piece_size", "Piece Size", 0.02, 0.2, 0.01)
    obs.obs_properties_add_float_slider(props, "randomness", "Randomness", 0.0, 1.0, 0.01)
    return props
end

function script_description()
    return "Shatter: Simulates a shattered glass effect. Each piece flies outward with random direction and speed. Includes scale."
end

function script_load(settings)
    obs.obs_register_source(filter_info)
end
