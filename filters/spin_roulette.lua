obs = obslua
-- spin_roulette.lua



local shader_code = [[
uniform float4x4 ViewProj;
uniform texture2d image;

uniform float angle;
uniform float aspect_ratio;
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
    // 1. Shift UVs to center
    float2 uv = v_in.uv - 0.5;

    // 2. Aspect ratio correction (prevents stretching during rotation)
    uv.x *= aspect_ratio;

    // 3. Calculate rotation math
    float s = sin(angle);
    float c = cos(angle);

    float2 rotated_uv = float2(
        uv.x * c - uv.y * s,
        uv.x * s + uv.y * c
    );

    // 4. Revert aspect ratio correction
    rotated_uv.x /= aspect_ratio;

    // 5. Shift UVs back to normal
    rotated_uv += 0.5;

    return image.Sample(textureSampler, rotated_uv);
}

technique Draw {
    pass {
        vertex_shader = VSDefault(v_in);
        pixel_shader  = PSDefault(v_in);
    }
}
]]

local filter_info = {}
filter_info.id = "custom_spin_roulette"
filter_info.type = obs.OBS_SOURCE_TYPE_FILTER
filter_info.output_flags = obs.OBS_SOURCE_VIDEO

filter_info.get_name = function(settings)
    return "Spin Roulette"
end

filter_info.get_defaults = function(settings)
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

    math.randomseed(os.time())
    data.scale_factor = 1.0

    -- Animation State Variables
    data.current_angle = 0.0
    data.target_angle = 0.0
    data.start_angle = 0.0
    data.is_spinning = false
    data.spin_start_time = 0
    data.spin_duration = 0

    filter_info.update(data, settings)

    return data
end

filter_info.show = function(data)
    if data then
        data.is_spinning = true
        data.spin_start_time = obs.os_gettime_ns()
        data.spin_duration = 3.0 + (math.random() * 2.0)
        local full_rotations = math.random(5, 10)
        local random_stop_angle = math.random() * (math.pi * 2)
        data.start_angle = data.current_angle
        data.target_angle = data.start_angle + (full_rotations * math.pi * 2) + random_stop_angle
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
end

filter_info.video_render = function(data, effect)
    if not data.effect then
        obs.obs_source_skip_video_filter(data.source)
        return
    end

    if not obs.obs_source_process_filter_begin(data.source, obs.GS_RGBA, obs.OBS_NO_DIRECT_RENDERING) then
        return
    end

    -- Calculate current Aspect Ratio to prevent 16:9 squishing when rotated
    local target = obs.obs_filter_get_target(data.source)
    local aspect_ratio = 1.0
    if target then
        local width = obs.obs_source_get_base_width(target)
        local height = obs.obs_source_get_base_height(target)
        if height > 0 then
            aspect_ratio = width / height
        end
    end

    -- Animation Logic over time
    local now = obs.os_gettime_ns()
    
    -- Detect if the filter was just enabled (no render for > 0.2 seconds)
    if data.last_render_time == nil or (now - data.last_render_time > 200000000) then
        if filter_info.show then filter_info.show(data) end
    end
    data.last_render_time = now

    if data.is_spinning then
        local elapsed = (now - data.spin_start_time) / 1000000000.0 -- ns to seconds
        
        if elapsed >= data.spin_duration then
            data.current_angle = data.target_angle
            data.is_spinning = false
        else
            -- Cubic Ease-Out algorithm (Fast start, smoothly decelerates to 0)
            local t = elapsed / data.spin_duration
            local ease = 1.0 - math.pow(1.0 - t, 3.0)
            data.current_angle = data.start_angle + (data.target_angle - data.start_angle) * ease
        end
    end

    -- Send Variables to Shader
    obs.gs_effect_set_float(obs.gs_effect_get_param_by_name(data.effect, "angle"), data.current_angle)
    obs.gs_effect_set_float(obs.gs_effect_get_param_by_name(data.effect, "aspect_ratio"), aspect_ratio)
    obs.gs_effect_set_float(obs.gs_effect_get_param_by_name(data.effect, "scale_factor"), data.scale_factor)

    obs.obs_source_process_filter_end(data.source, data.effect, 0, 0)
end

filter_info.get_properties = function(data)
    local props = obs.obs_properties_create()
    
    obs.obs_properties_add_float_slider(props, "scale_factor", "Safe Zone Scale", 0.1, 1.0, 0.05)
    
    -- When clicked, this button triggers the animation math in `video_render`
    obs.obs_properties_add_button(props, "btn_spin", "Spin Roulette!", function(properties, button)
        if filter_info.show then
            filter_info.show(data)
        end
        return true
    end)
    
    return props
end

function script_description()
    return "A UI-triggered filter that dynamically spins an image to a random stop over 3+ seconds."
end

function script_load(settings)
    obs.obs_register_source(filter_info)
end