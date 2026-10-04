-- ============================================================================
-- 🌌 Doom — Custom Lua Script Redeem (Gravitational Void Singularity)
-- ============================================================================
-- Features:
--   1. Target Swirl Vortex:
--      Applies the GPU Swirl shader (filters/swirl.lua) to the target avatar/source,
--      transforming it into an inward-twisting gravitational black hole.
--   2. Multi-Source Gravitational Pull:
--      Queries all other Rust sources (state.studio_sources) and pulls them toward
--      the target coordinate in a floating orbital spiral.
--   3. Continuous Shrinking & Rotational Tidal Spin:
--      All pulled sources progressively shrink down from 100% to 0% and spin with
--      gravitational torque until they vanish into the target singularity.
--   4. Singularity Implosion & Clean Restoration:
--      After swallowing all other sources, the singularity holds dramatically,
--      then smoothly restores all sources back to their default scene coordinates.
--   5. Tumbling: each pulled source also pitches and yaws at one constant
--      speed, in its own random direction.
-- ============================================================================

-- Sliders in the Studio's redeem editor (-- @param name default min max label):
-- @param swirl_intensity 8 0 20 Swirl strength
-- @param swirl_radius 0.85 0.1 1.5 Swirl size
-- @param swirl_speed 2.5 0 10 Swirl spin speed
-- @param tumble_speed 180 0 720 Tumble speed (°/s)

function on_redeem(params)
    -- A slider's value (params.<name>), or its default
    local function setting(name, default)
        return tonumber(params and params[name]) or default
    end

    -- 1. Extract target name and duration
    local target = (params and params.target) or ""
    local base_speed = tonumber(params and params.speed) or 4.5
    local speed_ratio = math.max(0.2, math.min(10.0, base_speed / 4.5))

    local raw_dur = tonumber(params and params.duration)
    local doom_duration = 5.0
    if raw_dur then
        if raw_dur == 500 then
            doom_duration = 5.0
        elseif raw_dur > 60.0 then
            doom_duration = raw_dur / 1000.0
        else
            doom_duration = raw_dur
        end
    end
    doom_duration = math.max(0.8, math.min(doom_duration, 60.0))

    -- 2. Query target metrics and coordinates
    local canvas_w, canvas_h = trust.get_canvas_size()
    local target_info = trust.get_target_info(target)
    local target_name = target_info.name or target
    local target_cx = target_info.center_x
    local target_cy = target_info.center_y

    -- Fallback target coordinates if needed
    if not target_cx or target_cx <= 0 then
        target_cx = canvas_w * 0.5
    end
    if not target_cy or target_cy <= 0 then
        target_cy = canvas_h * 0.5
    end

    -- 3. Trigger GPU Swirl Effect on the Target Avatar
    -- Applies the vortex twist shader directly to the target source;
    -- the bounce squish and jolt below mix in with it
    trust.trigger_reaction({
        target = target_name,
        impactReactions = {
            custom_script = {
                enabled = true,
                -- Relative to the theater-scene folder (works on Windows and Linux)
                path = "filters/swirl.lua",
                duration_ms = math.floor(doom_duration * 1000),
                params = {
                    swirl_radius = setting("swirl_radius", 0.85),
                    swirl_intensity = setting("swirl_intensity", 8.0),
                    swirl_speed = setting("swirl_speed", 2.5) * speed_ratio
                }
            },
            -- Inward gravitational avatar compression
            bounce = { enabled = true, squish_amount = -0.22, duration_ms = math.floor(doom_duration * 1000) },
            jolt = { enabled = true, intensity = 0.15, duration_ms = 400 }
        }
    })

    -- Optional launch sound
    if params and params.sound and params.sound ~= "" then
        trust.play_sound(params.sound)
    end

    -- 4. Query All Rust Sources in the Scene
    local all_sources = trust.get_sources()
    local victims = {}

    -- Each redeem runs in a fresh Lua state, where math.random would repeat
    -- the same "random" directions every time: seed it
    if os and os.time then
        math.randomseed(os.time() + math.floor((os.clock() * 1000000) % 1000000))
    end
    local tumble_speed = setting("tumble_speed", 180.0)

    for _, s in ipairs(all_sources) do
        -- Skip target itself and internal engine overlay layers
        local is_target_source = (s.name == target_name)
            or (target ~= "" and s.name:lower() == target:lower())
            or (target_info.name and s.name:lower() == target_info.name:lower())
        local is_overlay = s.name:find("Throwables Layer") or s.name:find("Native Chat")

        if not is_target_source and not is_overlay and s.is_visible then
            local sw = (s.width > 0) and s.width or 200.0
            local sh = (s.height > 0) and s.height or 200.0
            local scx = s.x + sw * 0.5
            local scy = s.y + sh * 0.5
            local dx = scx - target_cx
            local dy = scy - target_cy
            local dist = math.sqrt(dx * dx + dy * dy)
            local angle = math.atan2(dy, dx)
            -- Tumble direction: the same speed for all, a random direction
            -- each (how much of it goes to pitch and how much to yaw)
            local tumble_dir = math.random() * 2.0 * math.pi

            table.insert(victims, {
                tumble_pitch = math.cos(tumble_dir) * tumble_speed,
                tumble_yaw = math.sin(tumble_dir) * tumble_speed,
                name = s.name,
                orig_x = s.x,
                orig_y = s.y,
                orig_w = sw,
                orig_h = sh,
                orig_roll = s.roll,
                orig_pitch = s.pitch or 0.0,
                orig_yaw = s.yaw or 0.0,
                orig_visible = s.is_visible,
                initial_dist = math.max(10.0, dist),
                initial_angle = angle,
            })
        end
    end

    -- 5. Animate Gravitational Spiral Pull & Shrinking
    -- Pull speed scales with speed_ratio, then holds at singularity
    local pull_duration = math.min(doom_duration * 0.9, math.max(0.5, (doom_duration * 0.85) / math.sqrt(speed_ratio)))
    local dt = 0.02
    local total_steps = math.max(1, math.floor(pull_duration / dt))

    for step = 1, total_steps do
        local progress = step / total_steps
        local grav_progress = progress ^ 1.6

        for i, vic in ipairs(victims) do
            -- Inward spiral radius collapsing to 0
            local radius = math.max(0.0, vic.initial_dist * (1.0 - grav_progress))

            -- Orbital rotation around singularity center
            local orbit_speed = (6.0 + (i * 1.2)) * speed_ratio
            local angle = vic.initial_angle + (progress ^ 1.8) * orbit_speed

            -- Continuous shrinking scale (1.0 -> 0.0)
            local scale = math.max(0.001, (1.0 - grav_progress) ^ 1.2)
            local curr_w = vic.orig_w * scale
            local curr_h = vic.orig_h * scale

            -- Source center positioned on the spiral orbit
            local curr_cx = target_cx + radius * math.cos(angle)
            local curr_cy = target_cy + radius * math.sin(angle)
            local curr_x = curr_cx - curr_w * 0.5
            local curr_y = curr_cy - curr_h * 0.5

            -- Rotational tidal spin
            local curr_roll = vic.orig_roll + (progress ^ 1.8) * (540.0 + i * 90.0) * speed_ratio

            -- Tumbling: pitch and yaw at a constant speed (degrees per second)
            local elapsed = step * dt
            local curr_pitch = vic.orig_pitch + vic.tumble_pitch * elapsed
            local curr_yaw = vic.orig_yaw + vic.tumble_yaw * elapsed

            -- Disappear completely into the target coordinate when swallowed
            local is_vis = (progress < 0.94) and (radius > 12.0) and (scale > 0.03)

            trust.set_source_transform(vic.name, {
                x = curr_x,
                y = curr_y,
                width = curr_w,
                height = curr_h,
                roll = curr_roll,
                pitch = curr_pitch,
                yaw = curr_yaw,
                is_visible = is_vis
            })
        end

        trust.sleep(dt)
    end

    -- Ensure all victims are hidden at the end of the pull
    for _, vic in ipairs(victims) do
        trust.set_source_transform(vic.name, {
            x = target_cx,
            y = target_cy,
            width = 0.0,
            height = 0.0,
            is_visible = false
        })
    end

    -- 6. Singularity Hold (Sources remain consumed inside the void)
    local hold_time = math.max(0.4, doom_duration - pull_duration)
    trust.sleep(hold_time)

    -- 7. Clean Scene Restoration
    -- Smoothly blossom all victims back to their exact original poses (last pose sent)
    local restore_steps = math.max(5, math.floor(15 / math.sqrt(speed_ratio)))
    local restore_dt = 0.02
    for step = 1, restore_steps do
        local p = step / restore_steps
        -- Ease out curve from singularity out to last pose
        local ease = math.sin(p * math.pi * 0.5)
        for _, vic in ipairs(victims) do
            local cur_x = target_cx + (vic.orig_x - target_cx) * ease
            local cur_y = target_cy + (vic.orig_y - target_cy) * ease
            local cur_w = math.max(1.0, vic.orig_w * ease)
            local cur_h = math.max(1.0, vic.orig_h * ease)
            local cur_roll = vic.orig_roll * ease
            local cur_pitch = vic.orig_pitch * ease
            local cur_yaw = vic.orig_yaw * ease
            trust.set_source_transform(vic.name, {
                x = cur_x,
                y = cur_y,
                width = cur_w,
                height = cur_h,
                roll = cur_roll,
                pitch = cur_pitch,
                yaw = cur_yaw,
                is_visible = (p > 0.05) and vic.orig_visible,
            })
        end
        trust.sleep(restore_dt)
    end

    -- Snap 100% to exact captured last pose
    for _, vic in ipairs(victims) do
        trust.set_source_transform(vic.name, {
            x = vic.orig_x,
            y = vic.orig_y,
            width = vic.orig_w,
            height = vic.orig_h,
            roll = vic.orig_roll,
            pitch = vic.orig_pitch,
            yaw = vic.orig_yaw,
            is_visible = vic.orig_visible,
        })
    end

    -- Clean reset for any remaining sources back to their last pose sent
    trust.reset_sources()
end
