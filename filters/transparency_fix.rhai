// transparency_fix.rhai — Theater Transparency Fix, as a Rhai filter
// hosted by Theater Scene (the Lua version, transparency_fix.lua, is what
// it replaces: same filter id and settings, so scenes keep their filters).
//
// For captures with an alpha channel (Game Capture "Allow Transparency",
// Spout, VTuber apps...). OBS draws a capture as straight alpha. Many
// games/apps don't write that:
//  * Premultiplied: colour is already multiplied by alpha. OBS multiplies
//    again, so translucent parts come out darker/fainter and additive glows
//    (alpha 0) vanish entirely.
//  * Premultiplied + squared alpha: translucency blended onto a cleared,
//    transparent background with the same blend on the alpha channel leaves
//    alpha = a*a (typical of Unity-based apps). OBS then shows colour*a^3
//    over a mostly visible background, so translucent objects nearly
//    disappear.
// The filter pipeline copies the source without blending, so the shader sees
// the raw pixels, reinterprets them, and draws them premultiplied.

const ID = "theater_transparency_fix_filter";
const NAME = "Theater Transparency Fix";
const BLEND = "premultiplied";

const SHADER = `
uniform float4x4 ViewProj;
uniform texture2d image;

uniform float alpha_mode;  // 1 = premultiplied, 2 = premultiplied + squared alpha
uniform float alpha_boost; // >= 1 firms up partial alpha (1 = unchanged)

sampler_state textureSampler {
    Filter    = Linear;
    AddressU  = Clamp;
    AddressV  = Clamp;
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

float4 PSFix(VertOut v_in) : TARGET {
    // Raw pixel as the capture wrote it
    float4 c = image.Sample(textureSampler, v_in.uv);
    float a = saturate(c.a);
    if (alpha_mode > 1.5) {
        a = sqrt(a);
    }
    a = pow(a, 1.0 / max(alpha_boost, 1.0));
    // Colour is already premultiplied: output premultiplied, drawn with ONE / INVSRCALPHA
    return float4(c.rgb, a);
}

technique Draw {
    pass {
        vertex_shader = VSDefault(v_in);
        pixel_shader  = PSFix(v_in);
    }
}
`;

fn defaults() {
    #{ alpha_mode: 2, alpha_boost: 1.0 }
}

fn properties() {
    [
        choice("alpha_mode", "Capture alpha", [
            ["Standard (as OBS draws it)", 0],
            ["Premultiplied", 1],
            ["Premultiplied + squared alpha (Unity / VTuber apps)", 2],
        ]),
        slider("alpha_boost", "Opacity boost (1 = off)", 1.0, 30.0, 0.05),
    ]
}

fn tick(dt) {
    // Standard: the source is drawn as OBS draws it
    if this.settings.alpha_mode == 0 {
        return;
    }
    #{ alpha_mode: this.settings.alpha_mode, alpha_boost: this.settings.alpha_boost }
}
