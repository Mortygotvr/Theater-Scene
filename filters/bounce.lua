-- bounce.lua
-- OBS Lua filter: Bounce effect (makes the image bounce up and down)
-- Includes scale_factor and animation reset logic, with perfect transparency.

obs = obslua

local shader_code = [[
uniform float4x4 ViewProj;
uniform texture2d image;

uniform float time;
uniform float bounce_height;
uniform float bounce_speed;
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

float4 PSDefault(VertOut v_in) : TARGET {
    float2 uv = v_in.uv;

    // Calculate the bounce offset
    // abs(sin) creates a perfect curved bounce cycle
    float bounce = abs(sin(time * bounce_speed)) * bounce_height;

    // OBS Y is inverted (0 is top). By adding to uv.y, we sample lower in the texture,
    // which makes the image visually move UP on the screen.
    uv.y += bounce;

    float4 color = image.Sample(textureSampler, uv);
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
filter_info.id = "custom_bounce_filter"
filter_info.type = obs.OBS_SOURCE_TYPE_FILTER
filter_info.output_flags = obs.OBS_SOURCE_VIDEO

filter_info.get_name = function(settings)
    return "Bounce Animation"
end

filter_info.get_defaults = function(settings)
    obs.obs_data_set_default_double(settings, "bounce_height", 0.2)
    obs.obs_data_set_default_double(settings, "bounce_speed", 5.0)
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
    data.bounce_height = 0.2
    data.bounce_speed = 5.0
    data.scale_factor = 1.0
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
    data.bounce_height = obs.obs_data_get_double(settings, "bounce_height")
    data.bounce_speed = obs.obs_data_get_double(settings, "bounce_speed")
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
    if data.last_render_time == nil or (now - data.last_render_time > 200000000) then
        data.start_time = now
    end
    data.last_render_time = now

    local time = (now - (data.start_time or now)) / 1000000000.0
    obs.gs_effect_set_float(obs.gs_effect_get_param_by_name(data.effect, "time"), time)
    obs.gs_effect_set_float(obs.gs_effect_get_param_by_name(data.effect, "bounce_height"), data.bounce_height)
    obs.gs_effect_set_float(obs.gs_effect_get_param_by_name(data.effect, "bounce_speed"), data.bounce_speed)
    obs.gs_effect_set_float(obs.gs_effect_get_param_by_name(data.effect, "scale_factor"), data.scale_factor)
    obs.obs_source_process_filter_end(data.source, data.effect, 0, 0)
end

filter_info.get_properties = function(data)
    local props = obs.obs_properties_create()
    obs.obs_properties_add_float_slider(props, "bounce_height", "Bounce Height", 0.05, 0.5, 0.01)
    obs.obs_properties_add_float_slider(props, "bounce_speed", "Bounce Speed", 1.0, 15.0, 0.5)
    obs.obs_properties_add_float_slider(props, "scale_factor", "Safe Zone Scale", 0.1, 1.0, 0.05)
    return props
end

function script_description()
    return "Bounce Animation: Makes the source bounce up and down like a ball. Resets on enable."
end

function script_load(settings)
    obs.obs_register_source(filter_info)
end