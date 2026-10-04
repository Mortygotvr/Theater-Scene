-- crack_mosaic.lua
-- OBS Lua filter: Crack Mosaic (pixelated shatter effect)
-- Each tile moves outward along a random direction, simulating a cracked mosaic

obs = obslua

local shader_code = [[
uniform float4x4 ViewProj;
uniform texture2d image;

uniform float time;
uniform float scale_factor;
// no crack_speed, instant effect
uniform float tile_size;
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

// Simple hash for pseudo-randomness per tile
float hash(float2 p) {
    return frac(sin(dot(p, float2(12.9898, 78.233))) * 43758.5453);
}

float4 PSDefault(VertOut v_in) : TARGET {
    float2 uv = v_in.uv;
    // Mosaic: snap to tile grid
    float2 tile_grid = floor(uv / tile_size);
    float2 tile_center = (tile_grid + 0.5) * tile_size;
    // Each tile gets a random direction and offset
    float angle = hash(tile_grid) * 6.2831853; // [0, 2pi]
    float dist = hash(tile_grid + 0.5) * randomness;
    float2 offset = float2(cos(angle), sin(angle)) * dist;
    // Move the whole tile outward
    float2 cracked_uv = uv + offset;

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
filter_info.id = "custom_crack_mosaic_filter"
filter_info.type = obs.OBS_SOURCE_TYPE_FILTER
filter_info.output_flags = obs.OBS_SOURCE_VIDEO

filter_info.get_name = function(settings)
    return "Crack Mosaic (Pixel Shatter)"
end

filter_info.get_defaults = function(settings)
    obs.obs_data_set_default_double(settings, "scale_factor", 1.0)
    obs.obs_data_set_default_double(settings, "tile_size", 0.06)
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
    data.crack_speed = 0.7
    data.tile_size = 0.06
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
    data.tile_size = obs.obs_data_get_double(settings, "tile_size")
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
    obs.gs_effect_set_float(obs.gs_effect_get_param_by_name(data.effect, "tile_size"), data.tile_size)
    obs.gs_effect_set_float(obs.gs_effect_get_param_by_name(data.effect, "randomness"), data.randomness)
    obs.obs_source_process_filter_end(data.source, data.effect, 0, 0)
end

filter_info.get_properties = function(data)
    local props = obs.obs_properties_create()
    obs.obs_properties_add_float_slider(props, "scale_factor", "Safe Zone Scale", 0.1, 1.0, 0.05)
    obs.obs_properties_add_float_slider(props, "tile_size", "Tile Size", 0.02, 0.2, 0.01)
    obs.obs_properties_add_float_slider(props, "randomness", "Randomness", 0.0, 1.0, 0.01)
    return props
end

function script_description()
    return "Crack Mosaic: Simulates a pixelated shatter effect. Each tile moves outward along a random direction. Includes scale."
end

function script_load(settings)
    obs.obs_register_source(filter_info)
end
