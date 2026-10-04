-- squish_oneshot.lua
-- OBS Lua filter: Squish Once (Jelly single bounce)
-- The image squashes and stretches and naturally settles to a rest,
-- playing only once each time the filter is activated!

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

    // Calculate squash factors (using a damped sine wave so it bounces then settles)
    float phase = time * squish_speed;
    float max_phase = 18.8495559; // 3 full wobble cycles (6 * PI)
    float squash = 0.0;
    
    if (phase < max_phase) {
        // Damping causes the bounce to lose momentum and stop exactly at 0
        float damping = 1.0 - (phase / max_phase);
        
        // Use an ease-out square of the damping so it snaps back to rest smoothly
        squash = sin(phase) * squish_amount * (damping * damping);
    }
    
    // Width scales by (1 + squash)
    // Height scales inversely (1 / width) to maintain physics volume.
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
filter_info.id = "custom_squish_oneshot_filter"
filter_info.type = obs.OBS_SOURCE_TYPE_FILTER
filter_info.output_flags = obs.OBS_SOURCE_VIDEO

filter_info.get_name = function(settings)
    return "Squishy Bounce (One-Shot)"
end

filter_info.get_defaults = function(settings)
    obs.obs_data_set_default_double(settings, "squish_amount", 0.2)
    obs.obs_data_set_default_double(settings, "squish_speed", 15.0)
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
    data.squish_amount = 0.2
    data.squish_speed = 15.0
    data.scale_factor = 1.0
    data.start_time = obs.os_gettime_ns()
    data.was_enabled = true
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

filter_info.video_tick = function(data, seconds)
    if not data.source then return end
    local enabled = obs.obs_source_enabled(data.source)
    if not data.was_enabled and enabled then
        -- The filter was just enabled! Reset the animation timer.
        data.start_time = obs.os_gettime_ns()
    end
    data.was_enabled = enabled
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
    obs.obs_properties_add_float_slider(props, "squish_speed", "Animation Speeed (Duration)", 1.0, 30.0, 0.5)
    obs.obs_properties_add_float_slider(props, "scale_factor", "Safe Zone Scale", 0.1, 1.0, 0.05)
    return props
end

function script_description()
    return "Squishy Bounce (One-Shot): Plays a high-quality squash & stretch \"boing\" effect that settles completely after playing once per activation."
end

function script_load(settings)
    obs.obs_register_source(filter_info)
end