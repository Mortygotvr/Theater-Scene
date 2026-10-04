-- fall_back.lua
-- OBS Lua filter: Fall Back effect (3D perspective hinge drop)
-- The image tilts backward, hinging from the bottom, simulating a falling billboard.

obs = obslua

local shader_code = [[
uniform float4x4 ViewProj;
uniform texture2d image;

uniform float time;
uniform float fall_speed;
uniform float perspective;
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

    // Progress with quadratic ease-in to simulate gravity acceleration
    float progress = min(time * fall_speed, 1.0);
    float ease = progress * progress; 
    
    // Rotate up to 90 degrees (1.570796 radians)
    float angle = ease * 1.570796; 

    float s = sin(angle);
    float c = cos(angle);
    float D = perspective;

    // Hinge at the bottom center of the image (0.5, 1.0)
    float2 centered = uv - float2(0.5, 1.0);
    
    float x_screen = centered.x;
    float y_screen = -centered.y; // distance from hinge upwards

    // Calculate perspective projection divisor
    float denom = D * c - y_screen * s;
    
    // If the denominator reaches 0, the plane has tipped past the camera's view ray
    if (denom <= 0.0) {
        return float4(0.0, 0.0, 0.0, 0.0);
    }

    // Inverse 3D projection to find the original texture coordinate
    float V = (y_screen * D) / denom;
    float U = (x_screen * (D + V * s)) / D;

    // Revert the center/hinge offset
    float2 src_uv = float2(U, -V);
    src_uv += float2(0.5, 1.0);

    // Sample the image (out-of-bounds UVs will return transparent naturally due to Border sampler)
    float4 color = image.Sample(textureSampler, src_uv);
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
filter_info.id = "custom_fall_back_filter"
filter_info.type = obs.OBS_SOURCE_TYPE_FILTER
filter_info.output_flags = obs.OBS_SOURCE_VIDEO

filter_info.get_name = function(settings)
    return "Fall Back (3D Drop)"
end

filter_info.get_defaults = function(settings)
    obs.obs_data_set_default_double(settings, "fall_speed", 1.0)
    obs.obs_data_set_default_double(settings, "perspective", 2.0)
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
    data.fall_speed = 1.0
    data.perspective = 2.0
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
    data.fall_speed = obs.obs_data_get_double(settings, "fall_speed")
    data.perspective = obs.obs_data_get_double(settings, "perspective")
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
    obs.gs_effect_set_float(obs.gs_effect_get_param_by_name(data.effect, "fall_speed"), data.fall_speed)
    obs.gs_effect_set_float(obs.gs_effect_get_param_by_name(data.effect, "perspective"), data.perspective)
    obs.gs_effect_set_float(obs.gs_effect_get_param_by_name(data.effect, "scale_factor"), data.scale_factor)
    obs.obs_source_process_filter_end(data.source, data.effect, 0, 0)
end

filter_info.get_properties = function(data)
    local props = obs.obs_properties_create()
    obs.obs_properties_add_float_slider(props, "fall_speed", "Fall Duration/Speed", 0.1, 3.0, 0.1)
    obs.obs_properties_add_float_slider(props, "perspective", "Perspective Distance", 1.0, 5.0, 0.1)
    obs.obs_properties_add_float_slider(props, "scale_factor", "Safe Zone Scale", 0.1, 1.0, 0.05)
    return props
end

function script_description()
    return "Fall Back: Tilts the image backward in 3D perspective from the bottom hinge like a falling trapdoor. Resets on enable."
end

function script_load(settings)
    obs.obs_register_source(filter_info)
end