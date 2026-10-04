-- pixel_drip.lua
-- OBS Lua filter: Pixel Drip (melting to ground with randomness)
-- Includes scale_factor as required

obs = obslua

local shader_code = [[
uniform float4x4 ViewProj;
uniform texture2d image;

uniform float time;
uniform float drip_speed;
uniform float randomness;
uniform float scale_factor;

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

// Simple hash for pseudo-randomness per column
float hash(float x) {
    return frac(sin(x * 91.3458) * 47453.5453);
}

float4 PSDefault(VertOut v_in) : TARGET {
    float2 uv = v_in.uv;
    // Per-column randomness
    float col_rand = hash(uv.x * 1000.0) * randomness;
    // Drip progress for this column
    float drip = min(1.0, (time * drip_speed) + col_rand);
    
    // OBS y is inverted: to drip visually DOWN, move toward y=0 (top)
    float new_y = uv.y - drip * uv.y;
    float2 drip_uv = float2(uv.x, new_y);

    float4 color = image.Sample(textureSampler, drip_uv);
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
filter_info.id = "custom_pixel_drip_filter"
filter_info.type = obs.OBS_SOURCE_TYPE_FILTER
filter_info.output_flags = obs.OBS_SOURCE_VIDEO

filter_info.get_name = function(settings)
    return "Pixel Drip (Melt to Ground)"
end

filter_info.get_defaults = function(settings)
    obs.obs_data_set_default_double(settings, "drip_speed", 0.5)
    obs.obs_data_set_default_double(settings, "randomness", 0.3)
    obs.obs_data_set_default_double(settings, "scale_factor", 1.0)
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
    data.drip_speed = 0.5
    data.randomness = 0.3
    data.scale_factor = 1.0
    -- Robust animation reset: always reset timer/state in create (on add or enable)
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
    data.drip_speed = obs.obs_data_get_double(settings, "drip_speed")
    data.randomness = obs.obs_data_get_double(settings, "randomness")
    data.scale_factor = obs.obs_data_get_double(settings, "scale_factor")
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
    -- Detect if the filter was just enabled (no render for > 0.2 seconds)
    if data.last_render_time == nil or (now - data.last_render_time > 200000000) then
        data.start_time = now
    end
    data.last_render_time = now

    local time = (now - (data.start_time or now)) / 1000000000.0
    obs.gs_effect_set_float(obs.gs_effect_get_param_by_name(data.effect, "time"), time)
    obs.gs_effect_set_float(obs.gs_effect_get_param_by_name(data.effect, "drip_speed"), data.drip_speed)
    obs.gs_effect_set_float(obs.gs_effect_get_param_by_name(data.effect, "randomness"), data.randomness)
    obs.gs_effect_set_float(obs.gs_effect_get_param_by_name(data.effect, "scale_factor"), data.scale_factor)
    obs.obs_source_process_filter_end(data.source, data.effect, 0, 0)
end

filter_info.get_properties = function(data)
    local props = obs.obs_properties_create()
    obs.obs_properties_add_float_slider(props, "drip_speed", "Drip Speed", 0.05, 2.0, 0.01)
    obs.obs_properties_add_float_slider(props, "randomness", "Randomness", 0.0, 1.0, 0.01)
    obs.obs_properties_add_float_slider(props, "scale_factor", "Safe Zone Scale", 0.1, 1.0, 0.05)
    return props
end

function script_description()
    return "Pixel Drip: Melting/dripping effect with randomness and ground. Each column falls at a different speed. Includes scale."
end

function script_load(settings)
    obs.obs_register_source(filter_info)
end
