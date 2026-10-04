-- squish_bounce.lua
-- OBS Lua filter: Squishy Bounce (Jelly idle animation)
-- The image squashes (gets fat/short) and stretches (gets thin/tall) 
-- while anchored perfectly to its bottom edge.

obs = obslua

local shader_code = [[
uniform float4x4 ViewProj;
uniform texture2d image;

uniform float time;
uniform float squish_amount;
uniform float squish_speed;
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

    // Calculate squash factors (using sine wave for smooth jelly-like bounce)
    float squash = sin(time * squish_speed) * squish_amount;
    
    // Width scales by (1 + squash)
    // Height scales inversely (1 / width) to maintain the illusion of constant physical volume!
    // Ex: When it gets wider, it must get shorter.
    float sq_x = 1.0 + squash;
    float sq_y = 1.0 / max(sq_x, 0.001);

    // Apply squish/stretch anchored right at the bottom center (0.5, 1.0)
    uv.x = 0.5 + (uv.x - 0.5) / max(sq_x, 0.001);
    uv.y = 1.0 + (uv.y - 1.0) / max(sq_y, 0.001);

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
filter_info.id = "custom_squish_bounce_filter"
filter_info.type = obs.OBS_SOURCE_TYPE_FILTER
filter_info.output_flags = obs.OBS_SOURCE_VIDEO

filter_info.get_name = function(settings)
    return "Squishy Bounce (Jelly)"
end

filter_info.get_defaults = function(settings)
    obs.obs_data_set_default_double(settings, "squish_amount", 0.15)
    obs.obs_data_set_default_double(settings, "squish_speed", 6.0)
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
    data.squish_amount = 0.15
    data.squish_speed = 6.0
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
    data.squish_amount = obs.obs_data_get_double(settings, "squish_amount")
    data.squish_speed = obs.obs_data_get_double(settings, "squish_speed")
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
    obs.gs_effect_set_float(obs.gs_effect_get_param_by_name(data.effect, "squish_amount"), data.squish_amount)
    obs.gs_effect_set_float(obs.gs_effect_get_param_by_name(data.effect, "squish_speed"), data.squish_speed)
    obs.gs_effect_set_float(obs.gs_effect_get_param_by_name(data.effect, "scale_factor"), data.scale_factor)
    obs.obs_source_process_filter_end(data.source, data.effect, 0, 0)
end

filter_info.get_properties = function(data)
    local props = obs.obs_properties_create()
    obs.obs_properties_add_float_slider(props, "squish_amount", "Squish Amount", 0.0, 0.8, 0.01)
    obs.obs_properties_add_float_slider(props, "squish_speed", "Squish Speed", 1.0, 20.0, 0.5)
    obs.obs_properties_add_float_slider(props, "scale_factor", "Safe Zone Scale", 0.1, 1.0, 0.05)
    return props
end

function script_description()
    return "Squishy Bounce (Jelly): Animates a squash and stretch breathing/bounce effect anchored to the bottom. Maintains volume."
end

function script_load(settings)
    obs.obs_register_source(filter_info)
end