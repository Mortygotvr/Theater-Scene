local obs = obslua
local bit = require("bit")

-- We must load the .effect HLSL file to run the custom pixel shader
local effect_file = "theater_perspective.effect"

-- This table defines our brand new OBS Video Filter
local filter_info = {}
filter_info.id = "theater_perspective_filter"
filter_info.type = obs.OBS_SOURCE_TYPE_FILTER
filter_info.output_flags = bit.bor(obs.OBS_SOURCE_VIDEO)

filter_info.get_name = function(type_data)
    return "Theater 2.5D Perspective"
end

filter_info.get_defaults = function(settings)
    obs.obs_data_set_default_double(settings, "scale_factor", 1.0)
    obs.obs_data_set_default_double(settings, "tilt_x", 0.0)
    obs.obs_data_set_default_double(settings, "tilt_y", 0.0)
    obs.obs_data_set_default_double(settings, "pivot_x", 0.5)
    obs.obs_data_set_default_double(settings, "pivot_y", 0.5)
end

filter_info.create = function(settings, source)
    -- This fires when the user adds our filter to a source
    local context = {}
    context.source = source
    context.tilt_x = 0.0
    context.tilt_y = 0.0
    context.pivot_x = 0.5
    context.pivot_y = 0.5
    context.scale_factor = 1.0

    -- Load the HLSL effect from the same folder as the script
    obs.obs_enter_graphics()
    local effect_path = script_path() .. effect_file
    context.effect = obs.gs_effect_create_from_file(effect_path, nil)
    obs.obs_leave_graphics()

    if context.effect == nil then
        obs.script_log(obs.LOG_ERROR, "Failed to load effect file: " .. effect_path)
    end

    -- Hook up the dynamic parameters
    context.params = {}
    if context.effect ~= nil then
        context.params.tilt_x = obs.gs_effect_get_param_by_name(context.effect, "tilt_x")
        context.params.tilt_y = obs.gs_effect_get_param_by_name(context.effect, "tilt_y")
        context.params.pivot_x = obs.gs_effect_get_param_by_name(context.effect, "pivot_x")
        context.params.pivot_y = obs.gs_effect_get_param_by_name(context.effect, "pivot_y")
        context.params.scale_factor = obs.gs_effect_get_param_by_name(context.effect, "scale_factor")
    end

    -- Force an initial update so saved settings are applied immediately on OBS startup
    filter_info.update(context, settings)

    return context
end

filter_info.destroy = function(context)
    -- Clean up shader memory when filter is removed
    if context.effect ~= nil then
        obs.obs_enter_graphics()
        obs.gs_effect_destroy(context.effect)
        obs.obs_leave_graphics()
    end
end

filter_info.get_properties = function(data)
    -- Create the UI sliders that show up in the OBS Filters window
    local props = obs.obs_properties_create()
    obs.obs_properties_add_float_slider(props, "scale_factor", "Scale (Shrink)", 0.1, 3.0, 0.01)
    obs.obs_properties_add_float_slider(props, "tilt_x", "Pitch (Tilt Front/Back)", -1.0, 1.0, 0.01)
    obs.obs_properties_add_float_slider(props, "tilt_y", "Yaw (Tilt Left/Right)", -1.0, 1.0, 0.01)
    obs.obs_properties_add_float_slider(props, "pivot_x", "Pivot Axis X", 0.0, 1.0, 0.01)
    obs.obs_properties_add_float_slider(props, "pivot_y", "Pivot Axis Y", 0.0, 1.0, 0.01)
    return props
end

filter_info.update = function(context, settings)
    -- Fires whenever the slider properties change
    context.tilt_x = obs.obs_data_get_double(settings, "tilt_x")
    context.tilt_y = obs.obs_data_get_double(settings, "tilt_y")
    context.pivot_x = obs.obs_data_get_double(settings, "pivot_x")
    context.pivot_y = obs.obs_data_get_double(settings, "pivot_y")
    
    -- Default to 1.0 if scale undefined to prevent dividing by zero scaling issues during initial add
    context.scale_factor = obs.obs_data_get_double(settings, "scale_factor")
    if context.scale_factor == 0.0 then context.scale_factor = 1.0 end
end

filter_info.video_render = function(context, effect)
    -- This runs 60 times a second on the GPU to draw the actual pixels
    local target = obs.obs_filter_get_target(context.source)
    if target == nil or context.effect == nil then
        -- Failsafe: just draw the normal source if shader crashed
        obs.obs_source_skip_video_filter(context.source)
        return
    end

    -- Tell the HLSL shader our current tilt slider values
    obs.gs_effect_set_float(context.params.tilt_x, context.tilt_x)
    obs.gs_effect_set_float(context.params.tilt_y, context.tilt_y)
    obs.gs_effect_set_float(context.params.pivot_x, context.pivot_x)
    obs.gs_effect_set_float(context.params.pivot_y, context.pivot_y)
    obs.gs_effect_set_float(context.params.scale_factor, context.scale_factor)

    -- Draw the source using our custom pixel shader math!
    obs.obs_source_process_filter_begin(context.source, obs.GS_RGBA, obs.OBS_NO_DIRECT_RENDERING)
    obs.obs_source_process_filter_end(context.source, context.effect, 0, 0)
end

-- Finally, when this Lua script loads, register the filter with the OBS engine
function script_load(settings)
    obs.obs_register_source(filter_info)
end

-- Expose a hook for the WebSocket animations to grab!
if not TheaterAnimations then _G.TheaterAnimations = {} end
-- We would add our animation interpolation loop here that finds this filter and math.lerps the tilt_x values!
