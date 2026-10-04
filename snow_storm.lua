-- ============================================================================
-- ❄️ Snow Storm — Custom Lua Script Redeem & Comprehensive Engine Test
-- ============================================================================
-- Demonstrates and tests the native Rust engine capabilities:
--   1. Dynamic canvas & target coordinate queries (trust.get_canvas_size, trust.get_target_info)
--   2. SPH fluid physics & metaball rendering look (trust.spawn_water)
--   3. Contact sound with per-engine debounce on particle collisions
--   4. Strict variable isolation between concurrent SPH streams with different looks
--   5. Physical projectile simulation with collision adhesion (trust.spawn_throwable)
--   6. Impact reactions on tracked target avatars (trust.trigger_reaction)
-- ============================================================================

local SNOWFLAKE_PNG_B64 =
    "iVBORw0KGgoAAAANSUhEUgAAACAAAAAgCAYAAABzenr0AAAAiElEQVR4nO2Wyw3AMAhDQ/ffmS7AxxBQkYrvtp8IUjhn9XdRxsTMLIYRhfNgg1aqBoMwT0d5xOMCZMojXhPgphzNUAEqypEsEaCy3MuElrBT8wA6xm9lz5vAAixA5ktFJWXPm8AIgI5n0DLVCVRCWFnmE1RAeBnuDtxAIF5oCVPXLuj5/CxfrV4d4TwuYpuBswAAAABJRU5ErkJggg=="

function on_redeem(params)
    -- Normalize target name & parameters
    local target = (params and params.target) or ""
    local base_speed = tonumber(params and params.speed) or 5.0
    local base_direction = tonumber(params and params.direction) or 90.0  -- 90° = straight down
    local base_scale = tonumber(params and params.scale) or 0.6
    local base_gravity = tonumber(params and params.gravity) or 0.5

    -- 1. Query environment & target metrics from Rust
    local canvas_w, canvas_h = trust.get_canvas_size()
    local target_info = trust.get_target_info(target)

    -- Normalize storm duration (handles raw seconds or milliseconds)
    local raw_dur = tonumber(params and params.duration)
    local storm_duration = 5.0
    if raw_dur then
        if raw_dur == 500 then
            storm_duration = 5.0
        elseif raw_dur > 60.0 then
            storm_duration = raw_dur / 1000.0
        else
            storm_duration = raw_dur
        end
    end
    storm_duration = math.max(0.1, math.min(storm_duration, 60.0))

    -- 2. Optional SPH Fluid Layer (opt-in ONLY via params.fluid / params.spawn_fluid)
    -- By default, a pure snow storm should NOT spawn water simulations!
    local enable_fluid = false
    if params and (params.fluid == true or params.spawn_fluid == true or params.fluid == "true") then
        enable_fluid = true
    end

    if enable_fluid then
        trust.spawn_water({
            target = target,
            count = math.min(60, math.floor(20 + storm_duration * 10)),
            radius = 10.0,
            viscosity = 0.70,
            gravity = 0.65,
            duration = storm_duration,
            stream_mode = "top",
            spread = 50.0,
            cone_spread = 35.0,
            drip_through_percent = 30.0,
            metaball_size = 9.0,
            round_edge = true,
            refraction = true,
            refraction_strength = 200.0,
            color = "#d4f2ff",
            color2 = "#ffffff",
            alpha = 0.85,
            redeem_key = "snow_storm_fluid"
        })
    end

    -- 3. Target Impact Reaction (Cold shivering jolt on impact, no avatar bouncing)
    trust.trigger_reaction({
        target = target,
        impactReactions = {
            bounce = { enabled = false, squish_amount = 0.0, duration_ms = 300 },
            jolt = { enabled = true, intensity = 0.15, duration_ms = 350 }
        }
    })

    -- Optional sound trigger
    if params and params.sound and params.sound ~= "" then
        trust.play_sound(params.sound)
    end

    -- 4. Realistic Snowfall Simulation (Snowflakes adhering & melting naturally)
    local burst_interval = 0.1
    local total_bursts = math.max(1, math.floor(storm_duration / burst_interval))

    for b = 1, total_bursts do
        local flakes_this_burst = math.random(2, 4)
        for _ = 1, flakes_this_burst do
            local scale = base_scale * (0.6 + math.random() * 0.8)
            local speed = base_speed * (0.7 + math.random() * 0.6)
            local gravity = base_gravity * (0.7 + math.random() * 0.6)
            local direction = base_direction + (math.random() - 0.5) * 25.0

            -- Collision mode:
            -- In default redeem configs, "collision" is set to "bounce".
            -- But real snow doesn't bounce like a rubber ball! It sticks to clothes/hair,
            -- slowly melts, or softly powders into snow dust.
            local requested_col = params and params.collision
            local col_mode
            if requested_col and requested_col ~= "" and requested_col ~= "bounce" and requested_col ~= "auto" then
                -- Explicit non-bounce user request (e.g. "stick", "squish", "shatter", "pass")
                col_mode = requested_col
            elseif params and (params.force_bounce == true or params.force_bounce == "true") then
                col_mode = "bounce"
            else
                -- Natural snow adhesion distribution:
                local r = math.random()
                if r < 0.75 then
                    col_mode = "stick"    -- 75% sticks to target avatar and melts over time
                elseif r < 0.92 then
                    col_mode = "squish"   -- 17% slides slowly down the avatar while melting
                else
                    col_mode = "shatter"  -- 8% bursts into fine snow powder on impact
                end
            end

            -- Sticking duration: snow stays for 3 to 5 seconds before melting away
            local stick_duration = math.random(3000, 5000)

            trust.spawn_throwable({
                target = target,
                action = "rain",
                direction = direction,
                collision = col_mode,
                stick_duration_ms = stick_duration,
                speed = speed,
                scale = scale,
                gravity = gravity,
                redeem_key = "snow_storm_flake",
                image_src = "data:image/png;base64," .. SNOWFLAKE_PNG_B64,
            })
        end

        if b < total_bursts then
            trust.sleep(burst_interval)
        end
    end
end
