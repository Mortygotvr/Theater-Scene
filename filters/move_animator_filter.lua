-- move_animator_filter.lua
-- Animates Position, Rotation, and Scale of a source via scene-item transforms.
-- Features custom math to maintain pivot stability and decouple pos/rot conflicts.

obs = obslua

local function decode_alignment(align)
    local xf, yf = 0.5, 0.5
    if align ~= 0 then
        if bit.band(align, 1) ~= 0 then xf = 0.0 end
        if bit.band(align, 2) ~= 0 then xf = 1.0 end
        if bit.band(align, 4) ~= 0 then yf = 0.0 end
        if bit.band(align, 8) ~= 0 then yf = 1.0 end
    end
    return xf, yf
end

local function anchor_to_point(ax, ay, w, h, xf, yf, rot_deg, px, py)
    local vx  = w * px - w * xf
    local vy  = h * py - h * yf
    local rad = rot_deg * math.pi / 180.0
    return ax + vx * math.cos(rad) - vy * math.sin(rad),
           ay + vx * math.sin(rad) + vy * math.cos(rad)
end

local function point_to_anchor(cx, cy, w, h, xf, yf, rot_deg, px, py)
    local vx  = w * px - w * xf
    local vy  = h * py - h * yf
    local rad = rot_deg * math.pi / 180.0
    return cx - (vx * math.cos(rad) - vy * math.sin(rad)),
           cy - (vx * math.sin(rad) + vy * math.cos(rad))
end

-- Apply callback to all scene items whose source name matches, across every scene.
-- Items are valid ONLY inside the callback; never store them for later use.
local function apply_to_scene_items(source_name, callback)
    local scenes = obs.obs_frontend_get_scenes()
    if not scenes then return end
    for _, scene_src in ipairs(scenes) do
        local scene = obs.obs_scene_from_source(scene_src)
        if scene then
            local items = obs.obs_scene_enum_items(scene)
            if items then
                for _, item in ipairs(items) do
                    local s = obs.obs_sceneitem_get_source(item)
                    if s and obs.obs_source_get_name(s) == source_name then
                        callback(item, s)
                    end
                end
                obs.sceneitem_list_release(items)
            end
        end
        obs.obs_source_release(scene_src)
    end
    obs.source_list_release(scenes)
end

-- Registry of all active filter instances; keyed by data table.
-- Iterated by the main-thread timer to apply scene-item transforms.
local active_filters = {}

-- --- Filter table -------------------------------------------------------------

local filter_info = {}
filter_info.id           = "custom_move_animator_filter"
filter_info.type         = obs.OBS_SOURCE_TYPE_FILTER
filter_info.output_flags = obs.OBS_SOURCE_VIDEO   -- required to appear in filter list

filter_info.get_name = function()
    return "Transform Animator (Move / Rotate / Scale)"
end

filter_info.get_defaults = function(settings)
    obs.obs_data_set_default_double(settings, "duration",          1.0)
    obs.obs_data_set_default_string(settings, "easing",            "Smooth")
    obs.obs_data_set_default_bool  (settings, "return_on_disable", true)

    obs.obs_data_set_default_bool  (settings, "anim_pos",   false)
    obs.obs_data_set_default_bool  (settings, "anim_scale", false)
    obs.obs_data_set_default_bool  (settings, "anim_rot",   true)

    obs.obs_data_set_default_bool  (settings, "random_mode_coord", false)
    obs.obs_data_set_default_bool  (settings, "random_mode_scale", false)
    obs.obs_data_set_default_bool  (settings, "random_mode_rot",   false)

    obs.obs_data_set_default_double(settings, "target_x",       0.0)
    obs.obs_data_set_default_double(settings, "target_y",       0.0)
    obs.obs_data_set_default_double(settings, "target_scale_x", 1.0)
    obs.obs_data_set_default_double(settings, "target_scale_y", 1.0)
    obs.obs_data_set_default_double(settings, "target_rot",     360.0)

    obs.obs_data_set_default_double(settings, "rand_scale_min",  0.5)
    obs.obs_data_set_default_double(settings, "rand_scale_max",  1.5)
    obs.obs_data_set_default_double(settings, "rand_rot_min",   -360.0)
    obs.obs_data_set_default_double(settings, "rand_rot_max",    360.0)
    obs.obs_data_set_default_bool  (settings, "rand_rot_side",   false)

    obs.obs_data_set_default_int   (settings, "pivot_align",     5)
end

filter_info.create = function(settings, source)
    local data          = {}
    data.source         = source
    data.is_animating   = false
    data.time_elapsed   = 0.0
    data.was_enabled    = false  -- explicit false prevents spurious first-tick trigger
    math.randomseed(obs.os_gettime_ns())
    filter_info.update(data, settings)
    active_filters[data] = data
    return data
end

filter_info.destroy = function(data)
    active_filters[data] = nil
end

filter_info.update = function(data, settings)
    data.duration          = obs.obs_data_get_double(settings, "duration")
    data.easing            = obs.obs_data_get_string(settings, "easing")
    data.return_on_disable = obs.obs_data_get_bool  (settings, "return_on_disable")

    data.anim_pos          = obs.obs_data_get_bool  (settings, "anim_pos")
    data.anim_scale        = obs.obs_data_get_bool  (settings, "anim_scale")
    data.anim_rot          = obs.obs_data_get_bool  (settings, "anim_rot")

    data.random_mode_coord = obs.obs_data_get_bool  (settings, "random_mode_coord")
    data.random_mode_scale = obs.obs_data_get_bool  (settings, "random_mode_scale")
    data.random_mode_rot   = obs.obs_data_get_bool  (settings, "random_mode_rot")

    data.target_x          = obs.obs_data_get_double(settings, "target_x")
    data.target_y          = obs.obs_data_get_double(settings, "target_y")
    data.target_scale_x    = obs.obs_data_get_double(settings, "target_scale_x")
    data.target_scale_y    = obs.obs_data_get_double(settings, "target_scale_y")
    data.target_rot        = obs.obs_data_get_double(settings, "target_rot")

    data.rand_scale_min    = obs.obs_data_get_double(settings, "rand_scale_min")
    data.rand_scale_max    = obs.obs_data_get_double(settings, "rand_scale_max")
    data.rand_rot_min      = obs.obs_data_get_double(settings, "rand_rot_min")
    data.rand_rot_max      = obs.obs_data_get_double(settings, "rand_rot_max")
    data.rand_rot_side     = obs.obs_data_get_bool  (settings, "rand_rot_side")

    local p_align = obs.obs_data_get_int(settings, "pivot_align")
    if     p_align == 1 then data.pivot_x = 0.0; data.pivot_y = 0.0
    elseif p_align == 2 then data.pivot_x = 0.5; data.pivot_y = 0.0
    elseif p_align == 3 then data.pivot_x = 1.0; data.pivot_y = 0.0
    elseif p_align == 4 then data.pivot_x = 0.0; data.pivot_y = 0.5
    elseif p_align == 5 then data.pivot_x = 0.5; data.pivot_y = 0.5
    elseif p_align == 6 then data.pivot_x = 1.0; data.pivot_y = 0.5
    elseif p_align == 7 then data.pivot_x = 0.0; data.pivot_y = 1.0
    elseif p_align == 8 then data.pivot_x = 0.5; data.pivot_y = 1.0
    elseif p_align == 9 then data.pivot_x = 1.0; data.pivot_y = 1.0
    else                     data.pivot_x = 0.5; data.pivot_y = 0.5 end
end

-- Mandatory no-op passthrough � we manipulate transforms, not pixels.
filter_info.video_render = function(data, effect)
    obs.obs_source_skip_video_filter(data.source)
end

-- --- Animation initialisation -------------------------------------------------

-- Called from video_tick (graphics thread): only sets a flag.
-- All scene-item reads happen in on_animation_timer (main thread).
local function start_animation(data)
    data.snapshot_requested = true
end

-- --- Tick (graphics thread) --------------------------------------------------
-- Only does time-advance and interpolation math. No scene-item API calls.

filter_info.video_tick = function(data, seconds)
    if not data.source then return end

    local enabled = obs.obs_source_enabled(data.source)

    -- Rising edge: request snapshot + animation start (handled by timer)
    if enabled and not data.was_enabled then
        data.time_elapsed      = 0.0
        data.is_animating      = false  -- timer sets true after snapshot
        start_animation(data)           -- sets data.snapshot_requested = true
    end

    -- Falling edge: request reset (handled by timer)
    if not enabled and data.was_enabled then
        data.is_animating  = false
        data.reset_requested = true
    end

    data.was_enabled = enabled

    if not data.is_animating then return end
    if data.start_x == nil   then return end

    -- Advance time and clamp
    data.time_elapsed = data.time_elapsed + seconds
    local t = math.min(data.time_elapsed / data.duration, 1.0)
    if t >= 1.0 then data.is_animating = false end

    -- Apply easing
    local et = t
    if     data.easing == "Smooth"       then et = t * t * (3.0 - 2.0 * t)
    elseif data.easing == "Fast to Slow" then et = t * (2.0 - t)
    elseif data.easing == "Slow to Fast" then et = t * t
    end

    data.easing_t = et
    data.transform_dirty = true
end

-- --- Main-thread timer --------------------------------------------------------
-- Runs on OBS main thread: safe to call obs_frontend_get_scenes and scene-item APIs.

local function on_animation_timer()
    for _, data in pairs(active_filters) do
        if data.source then
            local parent = obs.obs_filter_get_parent(data.source)
            if parent then
                local name = obs.obs_source_get_name(parent)

                -- Snapshot scene-item state and initialise animation
                if data.snapshot_requested then
                    data.snapshot_requested = false
                    local found = false
                    apply_to_scene_items(name, function(item, src)
                        if found then return end
                        found = true
                        local pos   = obs.vec2()
                        local scale = obs.vec2()
                        obs.obs_sceneitem_get_pos  (item, pos)
                        obs.obs_sceneitem_get_scale(item, scale)
                        data.start_x       = pos.x
                        data.start_y       = pos.y
                        data.start_scale_x = scale.x
                        data.start_scale_y = scale.y
                        data.start_rot     = obs.obs_sceneitem_get_rot(item)
                        
                        local src_w = obs.obs_source_get_base_width(src)
                        local src_h = obs.obs_source_get_base_height(src)
                        if src_w == 0 then src_w = obs.obs_source_get_width(src) end
                        if src_h == 0 then src_h = obs.obs_source_get_height(src) end

                        local crop = obs.obs_sceneitem_crop()
                        obs.obs_sceneitem_get_crop(item, crop)
                        data.base_w = src_w - crop.left - crop.right
                        data.base_h = src_h - crop.top - crop.bottom

                        if obs.obs_sceneitem_get_bounds_type(item) ~= obs.OBS_BOUNDS_NONE then
                            local bounds = obs.vec2()
                            obs.obs_sceneitem_get_bounds(item, bounds)
                            data.base_w = bounds.x
                            data.base_h = bounds.y
                        end
                        local align = obs.obs_sceneitem_get_alignment(item)
                        data.align_xf, data.align_yf = decode_alignment(align)
                        
                        local sw0 = data.base_w * data.start_scale_x
                        local sh0 = data.base_h * data.start_scale_y
                        data.start_cx, data.start_cy = anchor_to_point(
                            data.start_x, data.start_y, sw0, sh0,
                            data.align_xf, data.align_yf, data.start_rot, data.pivot_x, data.pivot_y)
                    end)
                    if found then
                        -- Resolve end scale
                        if data.random_mode_scale then
                            local s = data.rand_scale_min + math.random() * (data.rand_scale_max - data.rand_scale_min)
                            data.end_scale_x, data.end_scale_y = s, s
                        else
                            data.end_scale_x = data.target_scale_x
                            data.end_scale_y = data.target_scale_y
                        end
                        -- Resolve end rotation
                        if data.random_mode_rot then
                            local off = data.rand_rot_min + math.random() * (data.rand_rot_max - data.rand_rot_min)
                            if data.rand_rot_side and math.random() > 0.5 then off = -off end
                            data.end_rot = data.start_rot + data.target_rot + off
                        else
                            data.end_rot = data.start_rot + data.target_rot
                        end
                        -- Resolve end position
                        if data.anim_pos and data.random_mode_coord then
                            local ovi = obs.obs_video_info()
                            local cw, ch = 1920, 1080
                            if obs.obs_get_video_info(ovi) then cw = ovi.base_width ; ch = ovi.base_height end
                            data.end_cx = math.random() * cw
                            data.end_cy = math.random() * ch
                        elseif data.anim_pos then
                            local sw1 = data.base_w * data.end_scale_x
                            local sh1 = data.base_h * data.end_scale_y
                            data.end_cx, data.end_cy = anchor_to_point(
                                data.target_x, data.target_y, sw1, sh1,
                                data.align_xf, data.align_yf, data.end_rot, data.pivot_x, data.pivot_y)
                        else
                            data.end_cx = data.start_cx
                            data.end_cy = data.start_cy
                        end
                        -- Seed pending values to start so first timer tick is a no-op
                        data.easing_t     = 0.0
                        data.is_animating = true
                    end
                end

                -- Reset to start transform
                if data.reset_requested then
                    data.reset_requested = false
                    if data.return_on_disable and data.start_x ~= nil then
                        apply_to_scene_items(name, function(item)
                            if data.anim_rot then
                                obs.obs_sceneitem_set_rot(item, data.start_rot)
                            end
                            if data.anim_scale then
                                local sc = obs.vec2()
                                obs.vec2_set(sc, data.start_scale_x, data.start_scale_y)
                                obs.obs_sceneitem_set_scale(item, sc)
                            end
                            if data.anim_pos or data.anim_rot or data.anim_scale then
                                local p = obs.vec2()
                                obs.vec2_set(p, data.start_x, data.start_y)
                                obs.obs_sceneitem_set_pos(item, p)
                            end
                        end)
                    end
                end

                -- Apply interpolated transform
                if data.transform_dirty then
                    data.transform_dirty = false
                    apply_to_scene_items(name, function(item)
                        local pos = obs.vec2()
                        local scale = obs.vec2()
                        obs.obs_sceneitem_get_pos(item, pos)
                        obs.obs_sceneitem_get_scale(item, scale)
                        local live_rot = obs.obs_sceneitem_get_rot(item)

                        local next_scale_x = scale.x
                        local next_scale_y = scale.y
                        if data.anim_scale then
                            next_scale_x = data.start_scale_x + (data.end_scale_x - data.start_scale_x) * data.easing_t
                            next_scale_y = data.start_scale_y + (data.end_scale_y - data.start_scale_y) * data.easing_t
                        end

                        local next_rot = live_rot
                        if data.anim_rot then
                            next_rot = data.start_rot + (data.end_rot - data.start_rot) * data.easing_t
                        end

                        local next_cx, next_cy
                        if data.anim_pos then
                            next_cx = data.start_cx + (data.end_cx - data.start_cx) * data.easing_t
                            next_cy = data.start_cy + (data.end_cy - data.start_cy) * data.easing_t
                        else
                            -- We are NOT animating position. Maintain the live pivot center of rotation!
                            next_cx, next_cy = anchor_to_point(pos.x, pos.y, data.base_w * scale.x, data.base_h * scale.y, data.align_xf, data.align_yf, live_rot, data.pivot_x, data.pivot_y)
                        end

                        if data.anim_rot then
                            obs.obs_sceneitem_set_rot(item, next_rot)
                        end
                        if data.anim_scale then
                            local sc = obs.vec2()
                            obs.vec2_set(sc, next_scale_x, next_scale_y)
                            obs.obs_sceneitem_set_scale(item, sc)
                        end
                        if data.anim_pos or data.anim_rot or data.anim_scale then
                            local next_x, next_y = point_to_anchor(next_cx, next_cy, data.base_w * next_scale_x, data.base_h * next_scale_y, data.align_xf, data.align_yf, next_rot, data.pivot_x, data.pivot_y)
                            local p = obs.vec2()
                            obs.vec2_set(p, next_x, next_y)
                            obs.obs_sceneitem_set_pos(item, p)
                        end
                    end)
                end
            end
        end
    end
end

-- --- Properties UI ------------------------------------------------------------

local function refresh_visibility(props, _, settings)
    local anim_pos   = obs.obs_data_get_bool(settings, "anim_pos")
    local anim_scale = obs.obs_data_get_bool(settings, "anim_scale")
    local anim_rot   = obs.obs_data_get_bool(settings, "anim_rot")
    local rand_coord = obs.obs_data_get_bool(settings, "random_mode_coord")
    local rand_scale = obs.obs_data_get_bool(settings, "random_mode_scale")
    local rand_rot   = obs.obs_data_get_bool(settings, "random_mode_rot")

    local function vis(name, v)
        obs.obs_property_set_visible(obs.obs_properties_get(props, name), v)
    end

    vis("random_mode_coord", anim_pos)
    vis("target_x",          anim_pos and not rand_coord)
    vis("target_y",          anim_pos and not rand_coord)

    vis("random_mode_scale", anim_scale)
    vis("target_scale_x",    anim_scale and not rand_scale)
    vis("target_scale_y",    anim_scale and not rand_scale)
    vis("rand_scale_min",    anim_scale and rand_scale)
    vis("rand_scale_max",    anim_scale and rand_scale)

    vis("random_mode_rot",   anim_rot)
    vis("target_rot",        anim_rot)
    vis("rand_rot_min",      anim_rot and rand_rot)
    vis("rand_rot_max",      anim_rot and rand_rot)
    vis("rand_rot_side",     anim_rot and rand_rot)

    return true
end

filter_info.get_properties = function(data)
    local props = obs.obs_properties_create()
    local function cb(p) obs.obs_property_set_modified_callback(p, refresh_visibility) end

    cb(obs.obs_properties_add_bool(props, "anim_pos",   "Animate Position"))
    cb(obs.obs_properties_add_bool(props, "anim_scale", "Animate Scale"))
    cb(obs.obs_properties_add_bool(props, "anim_rot",   "Animate Rotation"))

    obs.obs_properties_add_float_slider(props, "duration", "Duration (s)", 0.1, 10.0, 0.1)
    obs.obs_properties_add_bool        (props, "return_on_disable", "Reset when deactivated")

    local el = obs.obs_properties_add_list(props, "easing", "Easing",
        obs.OBS_COMBO_TYPE_LIST, obs.OBS_COMBO_FORMAT_STRING)
    obs.obs_property_list_add_string(el, "Linear",             "Linear")
    obs.obs_property_list_add_string(el, "Smooth (In-Out)",    "Smooth")
    obs.obs_property_list_add_string(el, "Slow to Fast (In)",  "Slow to Fast")
    obs.obs_property_list_add_string(el, "Fast to Slow (Out)", "Fast to Slow")

    -- Position
    cb(obs.obs_properties_add_bool(props, "random_mode_coord", "Randomize Destination"))
    obs.obs_properties_add_float(props, "target_x", "Target X", -10000.0, 10000.0, 0.1)
    obs.obs_properties_add_float(props, "target_y", "Target Y", -10000.0, 10000.0, 0.1)

    -- Scale
    cb(obs.obs_properties_add_bool(props, "random_mode_scale", "Randomize Scale"))
    obs.obs_properties_add_float(props, "target_scale_x", "Target Scale X", 0.01, 10.0, 0.01)
    obs.obs_properties_add_float(props, "target_scale_y", "Target Scale Y", 0.01, 10.0, 0.01)
    obs.obs_properties_add_float(props, "rand_scale_min", "Min Scale",       0.01, 10.0, 0.01)
    obs.obs_properties_add_float(props, "rand_scale_max", "Max Scale",       0.01, 10.0, 0.01)

    -- Rotation
    cb(obs.obs_properties_add_bool(props, "random_mode_rot",  "Randomize Rotation"))
    obs.obs_properties_add_float(props, "target_rot",    "Rotation Offset (deg)", -3600.0, 3600.0, 1.0)
    obs.obs_properties_add_float(props, "rand_rot_min",  "Min Rot Offset (deg)",  -3600.0, 3600.0, 1.0)
    obs.obs_properties_add_float(props, "rand_rot_max",  "Max Rot Offset (deg)",  -3600.0, 3600.0, 1.0)
    obs.obs_properties_add_bool (props, "rand_rot_side", "50% chance flip direction")

    local pivot_list = obs.obs_properties_add_list(props, "pivot_align", "Rotation Pivot Target", obs.OBS_COMBO_TYPE_LIST, obs.OBS_COMBO_FORMAT_INT)
    obs.obs_property_list_add_int(pivot_list, "Top Left",      1)
    obs.obs_property_list_add_int(pivot_list, "Top Center",    2)
    obs.obs_property_list_add_int(pivot_list, "Top Right",     3)
    obs.obs_property_list_add_int(pivot_list, "Center Left",   4)
    obs.obs_property_list_add_int(pivot_list, "Center",        5)
    obs.obs_property_list_add_int(pivot_list, "Center Right",  6)
    obs.obs_property_list_add_int(pivot_list, "Bottom Left",   7)
    obs.obs_property_list_add_int(pivot_list, "Bottom Center", 8)
    obs.obs_property_list_add_int(pivot_list, "Bottom Right",  9)

    return props
end

-- --- Script entry -------------------------------------------------------------

function script_description()
    return "Transform Animator: Animate Position, Rotation, Scale."
end

function script_load(settings)
    obs.obs_register_source(filter_info)
    obs.timer_add(on_animation_timer, 16)
end

function script_unload()
    obs.timer_remove(on_animation_timer)
end
