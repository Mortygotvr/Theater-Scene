-- bite_vanish.lua
-- Theater Scene Impact Reaction: Bite Vanish
--
-- Every contact takes a bite out of the target where it was touched. Bites
-- add up, then heal back over time.
--
-- Pick "bite_vanish" as a redeem's Impact Reaction (timing: Impact). Theater
-- tells the shader where each throwable touched the target; each hit bites
-- there, the bites stay while more land (up to 1024 at once: the target is
-- long eaten by then, and past that the oldest goes first), and each one
-- heals back by the end of the reaction's Duration (its heal time). The
-- defaults at the bottom are the reaction's settings in the Studio.
--
-- It isn't an OBS filter: the bites need Theater's contact points, which it
-- hands the shader as one array a Lua script can't set.

obs = obslua

local shader_code = [[
uniform float4x4 ViewProj;
uniform texture2d image;

// Theater Scene sets these
uniform float duration;       // heal time, seconds
uniform float view_width;     // canvas pixels across uv 0..1
uniform float view_height;
uniform float target_width;   // the target's size, canvas pixels
uniform float target_height;
uniform float hit_count;      // hits still healing, oldest first
uniform float4 hits[1024];    // x, y (uv), age (seconds)

// The bite's look
uniform float bite_size;       // radius, share of the target's smaller side
uniform float teeth_count;     // tooth marks around a bite
uniform float teeth_depth;     // how deep they dent its edge (share of its radius)
uniform float chomp_speed;     // how fast a bite snaps shut (1 / seconds)
uniform float heal_start;      // when healing starts, share of the heal time
uniform float edge_darkening;  // darker rim around a bite, share of its radius

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

VertData VSDefault(VertData v_in) {
    VertData vert_out;
    vert_out.pos = mul(float4(v_in.pos.xyz, 1.0), ViewProj);
    vert_out.uv  = v_in.uv;
    return vert_out;
}

float4 PSDefault(VertData v_in) : TARGET {
    float2 uv = v_in.uv;
    float4 color = image.Sample(textureSampler, uv);

    float2 view_px = float2(max(view_width, 1.0), max(view_height, 1.0));
    float bite_px = bite_size * min(target_width, target_height);
    // Nothing of a bite (teeth and rim included) reaches past this
    float reach = bite_px * (1.0 + teeth_depth + edge_darkening) + 1.0;
    float heal_time = max(duration, 0.001);
    float heal_from = min(heal_start, 0.99);
    float nearest = 100000.0; // pixels to the nearest bite's edge
    int count = int(hit_count + 0.5);

    for (int i = 0; i < 1024; i++) {
        if (i >= count) break;
        float4 hit = hits[i];
        // In canvas pixels, so a bite is round whatever the target's shape
        float2 d = (uv - hit.xy) * view_px;
        float dist = length(d);
        if (dist > reach) continue;

        // Snaps shut, holds, then heals back by the end of the heal time
        float chomp = 1.0 - pow(1.0 - saturate(hit.z * chomp_speed), 3.0);
        float heal = 1.0 - smoothstep(heal_from, 1.0, hit.z / heal_time);
        float radius = bite_px * chomp * heal;
        if (radius < 0.5) continue;

        float edge = radius * (1.0 + teeth_depth * cos(atan2(d.y, d.x) * teeth_count));
        if (dist < edge) {
            return float4(0.0, 0.0, 0.0, 0.0);
        }
        nearest = min(nearest, dist - edge);
    }

    float rim = edge_darkening * bite_px;
    if (rim > 0.001 && nearest < rim) {
        color.rgb *= lerp(0.55, 1.0, smoothstep(0.0, rim, nearest));
    }
    return color;
}

technique Draw {
    pass {
        vertex_shader = VSDefault(v_in);
        pixel_shader  = PSDefault(v_in);
    }
}
]]

-- The bite's look: Theater reads these defaults as the reaction's settings
function get_defaults(settings)
    obs.obs_data_set_default_double(settings, "bite_size", 0.18)
    obs.obs_data_set_default_double(settings, "teeth_count", 9.0)
    obs.obs_data_set_default_double(settings, "teeth_depth", 0.08)
    obs.obs_data_set_default_double(settings, "chomp_speed", 12.0)
    obs.obs_data_set_default_double(settings, "heal_start", 0.6)
    obs.obs_data_set_default_double(settings, "edge_darkening", 0.08)
end

function script_description()
    return [[<h2>Bite Vanish</h2>
<p>A Theater Scene Impact Reaction, not an OBS filter: every contact takes a bite out of the target where it was touched; bites add up, then heal back over the reaction's Duration.</p>]]
end
