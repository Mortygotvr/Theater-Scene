// Rosia_chat.rhai — 🌹 Rosia Chat: 8-bit stream chat with a rose-wrapped gray border
// Drawn by Theater Scene with OBS graphics API (no browser / CEF needed).
//
// Features:
// - Authentic 8-bit pixel aesthetic: monospaced typography, sharp dialog box framing
// - Gray stone/metallic beveled border with corner rivets
// - Procedural pixel rose blossoms and thorny vines winding & wrapping around the gray border
// - Curated Gothic/Retro presets: Rosia Gothic, Ruby Noir, Tea Rose, Retro Arcade
// - Full relay integration (Twitch, YouTube, Kick), emotes, badges, avatars, whispers, clean moderation

const ID = "rosia_chat_source";
const NAME = "🌹 Rosia Chat";
const KIND = "source";

// ─── Settings & Presets ─────────────────────────────────────────────────────

fn defaults() {
    #{
        preset: "rosia_gothic", applied_preset: "",
        width: 480, height: 720,
        layout_mode: "stacked", bubble_style: "rose-frame", flow: "up",
        rose_style: "wrap",
        border_gray: rgba(120, 120, 142, 1.0),
        rose_color: rgba(225, 25, 85, 1.0),
        vine_color: rgba(45, 135, 70, 1.0),
        magic_stars: true,
        magic_color: rgba(255, 215, 80, 1.0),
        font: "Roboto Mono", fontsize: 15, font_weight_user: 700, font_weight_msg: 400,
        msg_color: rgba(240, 240, 250, 1.0),
        username_color_mode: "custom", user_color: rgba(255, 75, 125, 1.0),
        body_color_mode: "custom", bg: rgba(18, 18, 26, 0.90),
        header_color_mode: "custom", header_bg: rgba(14, 14, 20, 0.95),
        padding: 10, border_radius: 14,
        bubble_width: 0, user_bubble_width: 0, bubble_overlap: 3,
        border_color: rgba(120, 120, 142, 1.0),
        border_top: 3, border_right: 3, border_bottom: 3, border_left: 3,
        shadow: true, emotes: true, user_badges: true, user_images: true,
        clean: false, animation: "magic", expiry: 0, max_messages: 50,
        global_opacity: 1.0,
    }
}

fn presets() {
    #{
        rosia_gothic: #{
            bg: rgba(18, 18, 26, 0.90), msg_color: rgba(240, 240, 250, 1.0), user_color: rgba(255, 75, 125, 1.0),
            border_gray: rgba(120, 120, 142, 1.0), rose_color: rgba(225, 25, 85, 1.0), vine_color: rgba(45, 135, 70, 1.0),
            magic_stars: true, magic_color: rgba(255, 215, 80, 1.0),
            padding: 10, border_radius: 14, font: "Roboto Mono", fontsize: 15, shadow: true,
            bubble_style: "rose-frame", rose_style: "wrap", layout_mode: "stacked", username_color_mode: "custom",
            body_color_mode: "custom", header_color_mode: "custom", header_bg: rgba(14, 14, 20, 0.95),
            font_weight_user: 700, font_weight_msg: 400, bubble_width: 0, user_bubble_width: 0, bubble_overlap: 3,
            animation: "magic",
        },
        ruby_noir: #{
            bg: rgba(10, 10, 15, 0.95), msg_color: rgba(255, 255, 255, 1.0), user_color: rgba(255, 60, 95, 1.0),
            border_gray: rgba(140, 140, 160, 1.0), rose_color: rgba(200, 15, 55, 1.0), vine_color: rgba(30, 100, 50, 1.0),
            magic_stars: true, magic_color: rgba(255, 140, 180, 1.0),
            padding: 10, border_radius: 14, font: "Roboto Mono", fontsize: 15, shadow: true,
            bubble_style: "rose-frame", rose_style: "wrap", layout_mode: "stacked", username_color_mode: "custom",
            body_color_mode: "custom", header_color_mode: "custom", header_bg: rgba(10, 10, 15, 0.95),
            font_weight_user: 700, font_weight_msg: 400, bubble_width: 0, user_bubble_width: 0, bubble_overlap: 3,
            animation: "magic",
        },
        tea_rose: #{
            bg: rgba(28, 24, 28, 0.90), msg_color: rgba(250, 245, 240, 1.0), user_color: rgba(255, 140, 170, 1.0),
            border_gray: rgba(130, 125, 135, 1.0), rose_color: rgba(255, 130, 165, 1.0), vine_color: rgba(70, 145, 95, 1.0),
            magic_stars: true, magic_color: rgba(255, 200, 140, 1.0),
            padding: 10, border_radius: 16, font: "Roboto Mono", fontsize: 15, shadow: true,
            bubble_style: "rose-frame", rose_style: "wrap", layout_mode: "stacked", username_color_mode: "custom",
            body_color_mode: "custom", header_color_mode: "custom", header_bg: rgba(22, 18, 22, 0.95),
            font_weight_user: 700, font_weight_msg: 400, bubble_width: 0, user_bubble_width: 0, bubble_overlap: 3,
            animation: "magic",
        },
        retro_arcade: #{
            bg: rgba(0, 0, 0, 0.92), msg_color: rgba(240, 255, 250, 1.0), user_color: rgba(255, 215, 0, 1.0),
            border_gray: rgba(100, 110, 130, 1.0), rose_color: rgba(255, 0, 110, 1.0), vine_color: rgba(0, 220, 90, 1.0),
            magic_stars: true, magic_color: rgba(255, 230, 0, 1.0),
            padding: 10, border_radius: 12, font: "Roboto Mono", fontsize: 15, shadow: true,
            bubble_style: "rose-frame", rose_style: "wrap", layout_mode: "stacked", username_color_mode: "custom",
            body_color_mode: "custom", header_color_mode: "custom", header_bg: rgba(0, 0, 0, 0.95),
            font_weight_user: 700, font_weight_msg: 400, bubble_width: 0, user_bubble_width: 0, bubble_overlap: 3,
            animation: "magic",
        },
    }
}

fn properties() {
    let weights = [["Regular", 400], ["Semi-bold", 600], ["Bold", 700]];
    let color_modes = [["One color", "custom"], ["The chatter's color", "user"], ["Transparent", "transparent"]];
    [
        info("🌹 Rosia Chat: 8-bit stream chat with a rounded stone gray border wrapped in high-resolution roses."),
        button("test", "💬 Send a test message"),
        button("copy_vf", "📋 Copy this look for VTuber Friends (the Twitch extension)"),
        choice("preset", "Theme", [
            ["Rosia Gothic", "rosia_gothic"],
            ["Ruby Noir", "ruby_noir"],
            ["Tea Rose", "tea_rose"],
            ["Retro Arcade", "retro_arcade"],
        ]),
        int_slider("width", "Width", 150, 3840, 10),
        int_slider("height", "Height", 100, 2160, 10),
        choice("layout_mode", "Layout", [
            ["Two lines (RPG dialogue)", "stacked"],
            ["Two lines, split header", "stacked-split"],
            ["Single line", "inline"],
            ["Two lines, aligned badges", "stacked-align"],
            ["Single line, aligned badges", "inline-align"],
        ]),
        choice("flow", "Flow", [
            ["Newest at the bottom (older ones go up)", "up"],
            ["Newest at the top (older ones go down)", "down"],
        ]),
        choice("bubble_style", "Bubble style", [
            ["Rose & Vine Frame", "rose-frame"],
            ["Full bubble", "bubble"],
            ["Double bubble", "double-bubble"],
            ["Left border accent", "left-border"],
            ["Underline separator", "bottom-line"],
            ["No bubble (text only)", "minimal"],
        ]),
        choice("rose_style", "Rose & vine detail", [
            ["Full Rose & Vine Wrap", "wrap"],
            ["Corners Only (Roses)", "corners"],
            ["Vines Only", "vines"],
            ["Plain Gray Border (No Roses)", "none"],
        ]),
        color("border_gray", "Gray border color"),
        color("rose_color", "Rose color"),
        color("vine_color", "Vine color"),
        slider("global_opacity", "Opacity", 0.0, 1.0, 0.05),
        font_list("font", "Font (8-bit monospaced)"),
        int_slider("fontsize", "Font size", 12, 32, 1),
        choice("font_weight_user", "Name weight", weights),
        choice("font_weight_msg", "Message weight", weights),
        color("msg_color", "Message text color"),
        choice("username_color_mode", "Name color mode", [["One color", "custom"], ["Platform colors", "platform"], ["The chatter's color", "user"]]),
        color("user_color", "Name color (one color)"),
        choice("body_color_mode", "Dialog background mode", color_modes),
        color("bg", "Dialog background"),
        choice("header_color_mode", "Name bubble background mode", color_modes),
        color("header_bg", "Name bubble background"),
        int_slider("padding", "Padding", 4, 24, 1),
        int_slider("border_radius", "Corner radius (0 for 8-bit)", 0, 30, 1),
        int_slider("bubble_width", "Bubble width (0: auto-fit)", 0, 800, 10),
        int_slider("user_bubble_width", "Name bubble width (0: auto-fit)", 0, 600, 10),
        int_slider("bubble_overlap", "Double bubble overlap", 0, 30, 1),
        toggle("shadow", "Text shadow (8-bit crisp)"),
        toggle("emotes", "Emotes"),
        toggle("user_badges", "Badges"),
        toggle("user_images", "Avatars"),
        toggle("clean", "Remove moderation-flagged messages"),
        choice("animation", "Entrance animation", [
            ["✨ Magic (Stars & Dust)", "magic"],
            ["8-bit Pixel Pop", "pixel_pop"],
            ["Slide in", "slide"],
            ["Fade in", "fade"],
            ["Scale up", "scale"],
            ["Slide from left", "left"],
        ]),
        toggle("magic_stars", "✨ Magic stars & stardust"),
        color("magic_color", "Magic sparkle color"),
        int_slider("expiry", "Hide messages after (s, 0: never)", 0, 60, 5),
        int_slider("max_messages", "Messages kept", 5, 100, 1),
    ]
}

// ─── Helpers & Color Math ───────────────────────────────────────────────────

fn fmax(a, b) { if a > b { a } else { b } }
fn fmin(a, b) { if a < b { a } else { b } }
fn num(v) { if type_of(v) == "i64" { v.to_float() } else if type_of(v) == "f64" { v } else { 0.0 } }
fn is_map(v) { type_of(v) == "map" }
fn text_of(v) { if type_of(v) == "string" { v } else { "" } }
fn none(v) { type_of(v) == "()" }

fn first_text(values) {
    for v in values {
        if type_of(v) == "string" && v != "" { return v; }
    }
    ""
}

fn field(m, key) { if is_map(m) { m[key] } else { () } }

fn clamp_int(v, lo, hi) {
    if v < lo { lo } else if v > hi { hi } else { v }
}

fn blend_rgb(c1, c2, t) {
    let r1 = (c1 >> 16) & 0xFF;
    let g1 = (c1 >> 8) & 0xFF;
    let b1 = c1 & 0xFF;
    let r2 = (c2 >> 16) & 0xFF;
    let g2 = (c2 >> 8) & 0xFF;
    let b2 = c2 & 0xFF;
    let r = clamp_int((r1.to_float() * (1.0 - t) + r2.to_float() * t).round().to_int(), 0, 255);
    let g = clamp_int((g1.to_float() * (1.0 - t) + g2.to_float() * t).round().to_int(), 0, 255);
    let b = clamp_int((b1.to_float() * (1.0 - t) + b2.to_float() * t).round().to_int(), 0, 255);
    let a = (c1 >> 24) & 0xFF;
    (a << 24) | (r << 16) | (g << 8) | b
}

fn color_alpha(c, a) {
    (c & 0xFFFFFF) | ((fmax(0.0, fmin(1.0, a)) * 255.0).round().to_int() << 24)
}

fn rnd(seed) {
    let s = seed % 2147483647;
    if s < 0 { s = -s; }
    let next = (s * 1103515245 + 12345) % 2147483647;
    [next.to_float() / 2147483647.0, next]
}

fn cycle(t) {
    let frac = t - t.floor();
    if frac < 0.5 { frac * 2.0 } else { (1.0 - frac) * 2.0 }
}

fn ease_back(t) {
    let c1 = 1.70158;
    let c3 = c1 + 1.0;
    let u = t - 1.0;
    1.0 + c3 * u * u * u + c1 * u * u
}

fn ease_out(t) { let u = 1.0 - t; 1.0 - u * u * u }

// ─── Style Configuration ────────────────────────────────────────────────────

fn style_of(s) {
    let size = num(s.fontsize);
    let pad = num(s.padding);
    let bubble = s.bubble_style;
    let transparent = bubble == "minimal" || bubble == "bottom-line";
    #{
        font: if s.font == "" { "Roboto Mono" } else { s.font },
        size: size,
        lh: (size * 1.5).round(),
        w_user: s.font_weight_user,
        w_msg: s.font_weight_msg,
        pad_y: if transparent { 0.0 } else { pad },
        pad_x: if transparent { 0.0 } else { pad + 4.0 },
        pad_small: fmax(2.0, pad - 4.0),
        radius: num(s.border_radius),
        border_gray: s.border_gray,
        rose_color: s.rose_color,
        vine_color: s.vine_color,
        rose_style: s.rose_style,
        border: [num(s.border_top), num(s.border_right), num(s.border_bottom), num(s.border_left)],
        border_color: s.border_gray,
        bubble: bubble,
        transparent: transparent,
        double: bubble == "double-bubble",
        layout: s.layout_mode,
        // "down": the newest message at the top, older ones below it
        top_down: text_of(s.flow) == "down",
        body_mode: s.body_color_mode,
        bg: s.bg,
        header_mode: s.header_color_mode,
        header_bg: s.header_bg,
        msg_color: s.msg_color,
        user_mode: s.username_color_mode,
        user_color: s.user_color,
        shadow: s.shadow,
        emotes: s.emotes,
        badges: s.user_badges,
        images: s.user_images,
        emote_h: (size * 1.75).round(),
        max_w: if num(s.bubble_width) > 0.0 { num(s.bubble_width) } else { ((num(s.width) - 30.0) * 0.85).floor() },
        fixed_w: num(s.bubble_width),
        user_fixed_w: num(s.user_bubble_width),
        overlap: num(s.bubble_overlap),
        animation: s.animation,
        magic_stars: s.magic_stars ?? true,
        magic_color: s.magic_color ?? rgba(255, 215, 80, 1.0),
        expiry: num(s.expiry),
        clean: s.clean,
        max_messages: s.max_messages,
        opacity: num(s.global_opacity),
    }
}

// ─── Reading Relay Messages ─────────────────────────────────────────────────

fn platform_of(text) {
    let p = text.to_lower();
    if p.contains("twitch") { "twitch" } else if p.contains("youtube") { "youtube" } else if p.contains("kick") { "kick" } else { "" }
}

fn platform_name_color(platform) {
    switch platform { "twitch" => 0xFFB19FFB, "youtube" => 0xFFFF5E5E, "kick" => 0xFF00E701, _ => 0xFFF0F0F0 }
}

fn platform_ring(platform) {
    switch platform { "twitch" => 0xFF9146FF, "youtube" => 0xFFFF0000, "kick" => 0xFF00E701, _ => 0x33FFFFFF }
}

fn emote_of(tag) {
    if tag.starts_with("twitch_emote:") {
        let id = tag.sub_string(13);
        if id == "" { return (); }
        return #{ t: "emote", src: "https://static-cdn.jtvnw.net/emoticons/v2/" + id + "/default/dark/2.0", name: id };
    }
    if tag.starts_with("emote:") {
        let rest = tag.sub_string(6);
        let colon = rest.index_of(":");
        let id = if colon < 0 { rest } else { rest.sub_string(0, colon) };
        let name = if colon < 0 { rest } else { rest.sub_string(colon + 1) };
        if id == "" { return (); }
        if id == "kick" || id == "demo" { return #{ t: "text", s: ":" + name + ":" }; }
        return #{ t: "emote", src: "https://files.kick.com/emotes/" + id + "/fullsize", name: name };
    }
    if tag.starts_with("yt_emoji:") {
        let url = tag.sub_string(9);
        if url == "yt-smile" { return #{ t: "text", s: "😊" }; }
        if url == "yt-thumbsup" { return #{ t: "text", s: "👍" }; }
        if url == "" { return (); }
        return #{ t: "emote", src: url, name: "emoji" };
    }
    ()
}

fn tokenize(text) {
    let parts = [];
    let rest = text;
    let pending = "";
    loop {
        let open = rest.index_of("[");
        if open < 0 { pending += rest; break; }
        let close = rest.index_of("]", open);
        if close < 0 { pending += rest; break; }
        let found = emote_of(rest.sub_string(open + 1, close - open - 1));
        if none(found) {
            pending += rest.sub_string(0, open + 1);
            rest = rest.sub_string(open + 1);
            continue;
        }
        pending += rest.sub_string(0, open);
        if found.t == "text" {
            pending += found.s;
        } else {
            if pending != "" { parts.push(#{ t: "text", s: pending }); pending = ""; }
            parts.push(found);
        }
        rest = rest.sub_string(close + 1);
    }
    if pending != "" { parts.push(#{ t: "text", s: pending }); }
    parts
}

fn badge_of(tag) {
    let t = tag;
    t.trim();
    if t == "" { return (); }
    if t.starts_with("badge:") {
        let content = t.sub_string(6);
        let last = -1;
        let i = content.len() - 1;
        while i >= 0 {
            if content[i] == ':' { last = i; break; }
            i -= 1;
        }
        let url = if last > 0 { content.sub_string(0, last) } else { content };
        return #{ kind: "image", src: url, avatar: url.contains("profile_image") };
    }
    #{ kind: "icon", code: t.to_upper() }
}

fn badges_of(v) {
    let list = [];
    if type_of(v) == "string" {
        for piece in v.split("[") {
            let close = piece.index_of("]");
            if close > 0 {
                let b = badge_of(piece.sub_string(0, close));
                if !none(b) { list.push(b); }
            }
        }
    } else if type_of(v) == "array" {
        for item in v {
            let tag = if is_map(item) { first_text([item["name"], item["id"]]) } else { text_of(item) };
            let b = badge_of(tag);
            if !none(b) { list.push(b); }
        }
    }
    list
}

fn message_of(ev, now, n) {
    let kind = text_of(ev["type"]);
    if kind != "chat" && kind != "whisper" { return (); }
    let payload = ev["payload"];
    let data = ev["data"];
    let user = first_text([ev["username"], ev["user"], ev["displayName"], field(payload, "username"), field(payload, "user"), field(data, "username"), field(data, "user"), "Viewer"]);
    let text = first_text([ev["message"], ev["text"], field(payload, "message"), field(payload, "text"), field(data, "message")]);
    let platform = platform_of(first_text([ev["source"], ev["platform"], ev["service"], field(payload, "source")]));
    let color_text = first_text([ev["color"], ev["user_color"], field(ev["customData"], "color"), field(payload, "color")]);
    let badges = badges_of(if !none(ev["badges"]) { ev["badges"] } else { field(payload, "badges") });
    #{
        id: first_text([text_of(ev["msg_id"]), text_of(ev["id"]), text_of(field(payload, "msg_id")), "m" + n]),
        user: user,
        platform: platform,
        color: parse_color(color_text, 0),
        whisper: kind == "whisper",
        badges: badges,
        avatar: first_text([ev["avatar"], field(payload, "avatar")]),
        parts: tokenize(text),
        born: now,
        laid: (),
        laid_ok: false,
        pending: false,
        laid_at: 0.0,
        leaving: (),
        leave_kind: "",
    }
}

// ─── 8-Bit Rose Wrapped Gray Border Drawing ─────────────────────────────────

fn chatter_color(m) {
    if m.color != 0 { m.color | 0xFF000000 } else { platform_name_color(m.platform) }
}

fn name_color(m, st) {
    if m.whisper { return 0xFFFF00E5; }
    switch st.user_mode {
        "custom" => st.user_color,
        "user" => { if m.color != 0 { m.color | 0xFF000000 } else { platform_name_color(m.platform) } },
        _ => platform_name_color(m.platform),
    }
}

fn fill_of(mode, own, m) {
    switch mode {
        "transparent" => 0,
        "user" => (chatter_color(m) & 0xFFFFFF) | (own & 0xFF000000),
        _ => own,
    }
}

fn badge_look(code) {
    if code.contains("MOD") { return [0xFF00E676, "⚔"]; }
    switch code {
        "BROADCASTER" => [0xFFFF3366, "👑"],
        "VIP" => [0xFFFF4081, "♦"],
        "SUB" => [0xFFFFD700, "★"],
        "MEMBER" => [0xFFFFD700, "★"],
        "SUBSCRIBER" => [0xFFFFD700, "★"],
        "VERIFIED" => [0xFF00E5FF, "✔"],
        "OWNER" => [0xFFFFD700, "♛"],
        "OG" => [0xFF00E676, "ℹ"],
        "FOUNDER" => [0xFFFFD700, "★"],
        _ => [0xFFAAAAAA, "●"],
    }
}

// Draws a high-resolution blooming botanical rose blossom
fn rose_blossom_cmds(cx, cy, scale, rose_mid, rose_hi, rose_dark, rose_core, vine_mid, vine_hi, vine_dark) {
    let out = [];

    // 1. Contoured emerald leaves with delicate central leaf veins
    out.push(#{ rect: [cx + 4.0 * scale, cy - 15.0 * scale, 15.0 * scale, 8.0 * scale], radius: 4.0 * scale, fill: vine_dark });
    out.push(#{ rect: [cx + 5.0 * scale, cy - 14.0 * scale, 12.0 * scale, 6.0 * scale], radius: 3.0 * scale, fill: vine_mid });
    out.push(#{ line: [cx + 4.0 * scale, cy - 10.0 * scale, cx + 18.0 * scale, cy - 14.0 * scale], width: 1.2 * scale, color: vine_hi, soft: 0.4 });

    out.push(#{ rect: [cx - 16.0 * scale, cy + 5.0 * scale, 8.0 * scale, 15.0 * scale], radius: 4.0 * scale, fill: vine_dark });
    out.push(#{ rect: [cx - 15.0 * scale, cy + 6.0 * scale, 6.0 * scale, 12.0 * scale], radius: 3.0 * scale, fill: vine_mid });
    out.push(#{ line: [cx - 8.0 * scale, cy + 5.0 * scale, cx - 15.0 * scale, cy + 18.0 * scale], width: 1.2 * scale, color: vine_hi, soft: 0.4 });

    out.push(#{ rect: [cx - 16.0 * scale, cy - 14.0 * scale, 12.0 * scale, 9.0 * scale], radius: 4.5 * scale, fill: vine_dark });
    out.push(#{ rect: [cx - 14.0 * scale, cy - 13.0 * scale, 9.0 * scale, 7.0 * scale], radius: 3.5 * scale, fill: vine_mid });
    out.push(#{ line: [cx - 5.0 * scale, cy - 5.0 * scale, cx - 15.0 * scale, cy - 13.0 * scale], width: 1.2 * scale, color: vine_hi, soft: 0.4 });

    // 2. Deep velvet shadow base
    out.push(#{ rect: [cx - 13.0 * scale, cy - 13.0 * scale, 26.0 * scale, 26.0 * scale], radius: 13.0 * scale, fill: rose_core });

    // 3. Outer Petal Cups (overlapping curved petals with highlighted rims)
    out.push(#{ rect: [cx - 8.0 * scale, cy - 14.0 * scale, 16.0 * scale, 13.0 * scale], radius: 6.5 * scale, fill: rose_dark });
    out.push(#{ line: [cx - 7.0 * scale, cy - 12.5 * scale, cx, cy - 14.5 * scale, cx + 7.0 * scale, cy - 12.5 * scale], width: 1.6 * scale, color: rose_hi, soft: 0.4 });

    out.push(#{ rect: [cx + 1.0 * scale, cy - 8.0 * scale, 13.0 * scale, 16.0 * scale], radius: 6.5 * scale, fill: rose_dark });
    out.push(#{ line: [cx + 12.5 * scale, cy - 7.0 * scale, cx + 14.5 * scale, cy, cx + 12.5 * scale, cy + 7.0 * scale], width: 1.6 * scale, color: rose_hi, soft: 0.4 });

    out.push(#{ rect: [cx - 8.0 * scale, cy + 1.0 * scale, 16.0 * scale, 13.0 * scale], radius: 6.5 * scale, fill: rose_dark });
    out.push(#{ line: [cx - 7.0 * scale, cy + 12.5 * scale, cx, cy + 14.5 * scale, cx + 7.0 * scale, cy + 12.5 * scale], width: 1.6 * scale, color: rose_hi, soft: 0.4 });

    out.push(#{ rect: [cx - 14.0 * scale, cy - 8.0 * scale, 13.0 * scale, 16.0 * scale], radius: 6.5 * scale, fill: rose_dark });
    out.push(#{ line: [cx - 12.5 * scale, cy - 7.0 * scale, cx - 14.5 * scale, cy, cx - 12.5 * scale, cy + 7.0 * scale], width: 1.6 * scale, color: rose_hi, soft: 0.4 });

    // 4. Middle Petal Layer (luxurious blooming petals)
    out.push(#{ rect: [cx - 10.0 * scale, cy - 10.0 * scale, 20.0 * scale, 20.0 * scale], radius: 10.0 * scale, fill: rose_mid });
    out.push(#{ line: [cx - 6.5 * scale, cy - 6.0 * scale, cx + 1.0 * scale, cy - 9.0 * scale, cx + 7.5 * scale, cy - 4.0 * scale], width: 1.8 * scale, color: rose_hi, soft: 0.4 });
    out.push(#{ line: [cx + 7.5 * scale, cy - 1.0 * scale, cx + 6.0 * scale, cy + 6.5 * scale, cx - 1.5 * scale, cy + 8.5 * scale], width: 1.8 * scale, color: rose_hi, soft: 0.4 });
    out.push(#{ line: [cx - 4.5 * scale, cy + 7.5 * scale, cx - 8.5 * scale, cy + 2.0 * scale, cx - 6.5 * scale, cy - 4.0 * scale], width: 1.8 * scale, color: rose_hi, soft: 0.4 });

    // 5. Center Bud Cavity & Spiral Petal Whorl
    out.push(#{ rect: [cx - 5.0 * scale, cy - 5.0 * scale, 10.0 * scale, 10.0 * scale], radius: 5.0 * scale, fill: rose_core });
    out.push(#{ line: [cx - 3.0 * scale, cy - 3.5 * scale, cx + 3.5 * scale, cy - 2.5 * scale, cx + 3.0 * scale, cy + 3.0 * scale, cx - 2.5 * scale, cy + 2.5 * scale, cx - 1.0 * scale, cy - 1.0 * scale, cx + 1.0 * scale, cy], width: 1.5 * scale, color: rose_hi, soft: 0.4 });
    out.push(#{ rect: [cx - 1.0 * scale, cy - 1.5 * scale, 2.0 * scale, 2.0 * scale], radius: 1.0 * scale, fill: 0xFFFFFFFF });

    out
}

// Draws a blooming miniature rose along the vine
fn mini_rose_cmds(cx, cy, rose_mid, rose_hi, rose_dark, rose_core, vine_mid, vine_hi) {
    let out = [];
    out.push(#{ rect: [cx - 6.0, cy - 4.0, 5.0, 4.0], radius: 2.0, fill: vine_mid });
    out.push(#{ rect: [cx + 1.0, cy + 1.0, 5.0, 4.0], radius: 2.0, fill: vine_mid });
    out.push(#{ line: [cx - 6.0, cy - 2.0, cx + 6.0, cy + 3.0], width: 1.0, color: vine_hi, soft: 0.3 });

    out.push(#{ rect: [cx - 6.0, cy - 6.0, 12.0, 12.0], radius: 6.0, fill: rose_core });
    out.push(#{ rect: [cx - 5.0, cy - 5.0, 10.0, 10.0], radius: 5.0, fill: rose_dark });
    out.push(#{ rect: [cx - 4.0, cy - 4.0, 8.0, 8.0], radius: 4.0, fill: rose_mid });
    out.push(#{ line: [cx - 2.5, cy - 2.0, cx + 2.0, cy - 2.0, cx + 2.0, cy + 2.0, cx - 1.5, cy + 1.5], width: 1.4, color: rose_hi, soft: 0.3 });
    out.push(#{ rect: [cx - 0.5, cy - 1.0, 1.5, 1.5], radius: 0.75, fill: 0xFFFFFFFF });
    out
}

// Draws a high-resolution rosebud
fn rose_bud_cmds(bx, by, scale, rose_mid, rose_hi, rose_dark, rose_core, vine_mid, vine_hi, vine_dark) {
    let out = [];
    out.push(#{ rect: [bx - 4.0 * scale, by + 3.0 * scale, 10.0 * scale, 9.0 * scale], radius: 4.5 * scale, fill: vine_dark });
    out.push(#{ line: [bx - 1.0 * scale, by + 5.0 * scale, bx - 6.0 * scale, by + 10.0 * scale], width: 1.5 * scale, color: vine_hi, soft: 0.4 });

    out.push(#{ rect: [bx - 2.0 * scale, by - 2.0 * scale, 13.0 * scale, 13.0 * scale], radius: 6.5 * scale, fill: rose_mid });
    out.push(#{ rect: [bx - 1.0 * scale, by - 1.0 * scale, 10.0 * scale, 10.0 * scale], radius: 5.0 * scale, fill: rose_dark });

    out.push(#{ line: [bx - 2.0 * scale, by + 2.0 * scale, bx + 1.0 * scale, by - 2.5 * scale, bx + 5.5 * scale, by + 1.0 * scale, bx + 2.0 * scale, by + 4.5 * scale], width: 1.5 * scale, color: rose_hi, soft: 0.4 });
    out.push(#{ rect: [bx + 1.0 * scale, by - 1.0 * scale, 2.0 * scale, 2.0 * scale], radius: 1.0 * scale, fill: 0xFFFFFFFF });
    out
}

// Draws the rounded stone gray border wrapped in high-resolution rose vines and blossoms
fn rose_frame_cmds(x, y, w, h, fill, st, m) {
    let out = [];
    let rad = num(st.radius);
    if rad <= 0.0 { rad = 14.0; }

    // 1. Rounded Dialog background
    let bg_color = if m.whisper { 0xD9221026 } else { fill };
    out.push(#{ rect: [x, y, w, h], fill: bg_color, radius: rad });

    // 2. The Rounded Stone Gray Beveled Border
    let gray_base = st.border_gray;
    let gray_hi = blend_rgb(gray_base, 0xFFFFFFFF, 0.35);
    let gray_dark = blend_rgb(gray_base, 0xFF000000, 0.40);
    let gray_rim = 0xFF121218;

    // Outer subtle dark rim contour
    out.push(#{ rect: [x - 0.5, y - 0.5, w + 1.0, h + 1.0], fill: 0, radius: rad + 0.5, border: 1.0, border_color: gray_rim });

    // Main 2.5px stone gray rounded border
    out.push(#{ rect: [x, y, w, h], fill: 0, radius: rad, border: 2.5, border_color: gray_base });

    // Top/left highlight bevel
    out.push(#{ rect: [x, y, w, h], fill: 0, radius: rad, border: [1.5, 0.0, 0.0, 1.5], border_color: gray_hi });

    // Inner soft shadow bevel
    out.push(#{ rect: [x + 2.5, y + 2.5, w - 5.0, h - 5.0], fill: 0, radius: fmax(0.0, rad - 2.5), border: 1.0, border_color: gray_dark });

    // 4 Corner studs / rivets in silver, nestled inside the corner curve
    let stud = 0xFFD0D0E0;
    let offset = fmax(6.0, rad * 0.45);
    out.push(#{ rect: [x + offset, y + offset, 3.0, 3.0], fill: stud, radius: 1.5 });
    out.push(#{ rect: [x + w - offset - 3.0, y + offset, 3.0, 3.0], fill: stud, radius: 1.5 });
    out.push(#{ rect: [x + offset, y + h - offset - 3.0, 3.0, 3.0], fill: stud, radius: 1.5 });
    out.push(#{ rect: [x + w - offset - 3.0, y + h - offset - 3.0, 3.0, 3.0], fill: stud, radius: 1.5 });

    if st.rose_style == "none" {
        return out;
    }

    // 3. Roses & Vine Wrap Palette
    let rose_mid = if m.whisper { 0xFFFF00A0 } else { st.rose_color };
    let rose_hi = blend_rgb(rose_mid, 0xFFFFFFFF, 0.45);
    let rose_dark = blend_rgb(rose_mid, 0xFF000000, 0.45);
    let rose_core = blend_rgb(rose_mid, 0xFF000000, 0.70);

    let vine_mid = st.vine_color;
    let vine_hi = blend_rgb(vine_mid, 0xFF80FF80, 0.35);
    let vine_dark = blend_rgb(vine_mid, 0xFF000000, 0.45);

    let draw_vines = st.rose_style == "wrap" || st.rose_style == "vines";
    let draw_roses = st.rose_style == "wrap" || st.rose_style == "corners";

    // 4. Smooth winding vine wrapping along the rounded gray border
    if draw_vines {
        let vx = x + rad + 6.0;
        let step = 42.0;
        while vx < x + w - rad - 12.0 {
            let seg_w = fmin(step - 12.0, x + w - rad - 12.0 - vx);
            // Vine stem along top edge
            out.push(#{ line: [vx, y + 1.0, vx + seg_w, y + 1.0], width: 2.2, color: vine_dark, soft: 0.4 });

            // Wrapping loop curving over the rounded gray border
            out.push(#{ line: [vx + 6.0, y - 3.0, vx + 9.0, y, vx + 11.0, y + 4.5], width: 3.0, color: vine_mid, soft: 0.5 });
            out.push(#{ line: [vx + 7.0, y - 2.0, vx + 9.5, y + 3.5], width: 1.4, color: vine_hi, soft: 0.4 });

            // Pointed thorn
            out.push(#{ line: [vx + 19.0, y - 0.5, vx + 23.0, y - 5.0], width: 1.5, color: vine_dark, soft: 0.4 });

            // Smooth curved leaf
            out.push(#{ rect: [vx + 27.0, y - 6.0, 9.0, 5.5], radius: 2.5, fill: vine_mid });
            out.push(#{ line: [vx + 25.0, y - 2.0, vx + 35.0, y - 5.5], width: 1.2, color: vine_hi, soft: 0.4 });

            vx += step;
        }

        // Vine climbing down the left rounded border
        let vy = y + rad + 6.0;
        let vstep = 36.0;
        while vy < y + h - rad - 8.0 {
            let seg_h = fmin(vstep - 10.0, y + h - rad - 8.0 - vy);
            // Stem
            out.push(#{ line: [x + 1.0, vy, x + 1.0, vy + seg_h], width: 2.2, color: vine_dark, soft: 0.4 });

            // Wrap loop around the left border
            out.push(#{ line: [x - 3.0, vy + 6.0, x, vy + 9.0, x + 4.5, vy + 11.0], width: 3.0, color: vine_mid, soft: 0.5 });
            out.push(#{ line: [x - 2.0, vy + 7.0, x + 3.5, vy + 9.5], width: 1.4, color: vine_hi, soft: 0.4 });

            // Thorn
            out.push(#{ line: [x - 0.5, vy + 17.0, x - 5.0, vy + 21.0], width: 1.5, color: vine_dark, soft: 0.4 });

            // Leaf
            out.push(#{ rect: [x - 6.0, vy + 24.0, 5.5, 9.0], radius: 2.5, fill: vine_mid });
            out.push(#{ line: [x - 2.0, vy + 22.0, x - 5.5, vy + 32.0], width: 1.2, color: vine_hi, soft: 0.4 });

            vy += vstep;
        }

        // Bottom border vine wrapping toward corner bud
        let bx = x + w - rad - 28.0;
        if bx > x + rad + 20.0 {
            out.push(#{ line: [bx, y + h - 1.5, bx + 16.0, y + h - 1.5], width: 2.0, color: vine_dark, soft: 0.4 });
            out.push(#{ line: [bx + 5.0, y + h - 4.5, bx + 8.0, y + h - 1.0, bx + 10.0, y + h + 2.5], width: 2.8, color: vine_mid, soft: 0.5 });
            out.push(#{ line: [bx + 6.0, y + h - 3.5, bx + 9.0, y + h + 1.5], width: 1.3, color: vine_hi, soft: 0.4 });
        }
    }

    // 5. Blooming High-Resolution Botanical Roses
    if draw_roses {
        // Main Corner Blossom at Top-Left
        let rx = x + 4.0;
        let ry = y + 4.0;
        for c in rose_blossom_cmds(rx, ry, 1.0, rose_mid, rose_hi, rose_dark, rose_core, vine_mid, vine_hi, vine_dark) {
            out.push(c);
        }

        // Mini blooming rose along the top border vine
        let mx = x + w * 0.58;
        if mx > x + 80.0 && mx < x + w - 40.0 {
            for c in mini_rose_cmds(mx, y + 1.0, rose_mid, rose_hi, rose_dark, rose_core, vine_mid, vine_hi) {
                out.push(c);
            }
        }

        // High-Resolution Rose Bud at Bottom-Right Corner
        let bx = x + w - 8.0;
        let by = y + h - 8.0;
        for c in rose_bud_cmds(bx, by, 1.0, rose_mid, rose_hi, rose_dark, rose_core, vine_mid, vine_hi, vine_dark) {
            out.push(c);
        }
    }

    out
}

// ─── Magic Stars & Magic Dust Entrance ──────────────────────────────────────

// Draws a twinkling 4-pointed radiant sparkle star with glowing rays & diamond core
fn star_cmds(cx, cy, size, color, glow_color, alpha) {
    let out = [];
    if size < 1.0 || alpha <= 0.01 { return out; }

    let c_glow = color_alpha(glow_color, 0.45 * alpha);
    let c_main = color_alpha(color, 0.90 * alpha);
    let c_white = color_alpha(0xFFFFFFFF, alpha);

    // 1. Soft glowing halo around center
    let halo_r = fmax(2.0, size * 0.4);
    out.push(#{ rect: [cx - halo_r, cy - halo_r, halo_r * 2.0, halo_r * 2.0], fill: c_glow, radius: halo_r, soft: halo_r * 1.5 });

    // 2. Horizontal ray
    out.push(#{ line: [cx - size * 1.15, cy, cx + size * 1.15, cy], width: 3.0, color: c_glow, soft: 2.5 });
    out.push(#{ line: [cx - size, cy, cx + size, cy], width: 1.6, color: c_main, soft: 0.6 });
    out.push(#{ line: [cx - size * 0.45, cy, cx + size * 0.45, cy], width: 1.0, color: c_white });

    // 3. Vertical ray
    out.push(#{ line: [cx, cy - size * 1.15, cx, cy + size * 1.15], width: 3.0, color: c_glow, soft: 2.5 });
    out.push(#{ line: [cx, cy - size, cx, cy + size], width: 1.6, color: c_main, soft: 0.6 });
    out.push(#{ line: [cx, cy - size * 0.45, cx, cy + size * 0.45], width: 1.0, color: c_white });

    // 4. Diagonal sub-glints (for larger stars >= 8px)
    if size >= 8.0 {
        let d = size * 0.35;
        out.push(#{ line: [cx - d, cy - d, cx + d, cy + d], width: 1.1, color: color_alpha(color, 0.70 * alpha), soft: 0.5 });
        out.push(#{ line: [cx - d, cy + d, cx + d, cy - d], width: 1.1, color: color_alpha(color, 0.70 * alpha), soft: 0.5 });
    }

    // 5. Bright center diamond / glint core
    let core = fmax(1.0, size * 0.16);
    out.push(#{ rect: [cx - core, cy - core, core * 2.0, core * 2.0], fill: c_white, radius: core * 0.5 });

    out
}

// Generates an ethereal magic aura, twinkling sparkle stars, and drifting stardust motes
fn magic_dust_cmds(w, h, age, st, m, seed) {
    let out = [];
    let magic_c = if m.whisper { 0xFFFF40D0 } else { st.magic_color };
    let glow_c = blend_rgb(magic_c, 0xFFFFFFFF, 0.45);
    let rad = num(st.radius);
    if rad <= 0.0 { rad = 14.0; }

    // Overall envelope over 0.75s: fast rise in 0.15s, gentle fade-out over next 0.60s
    let env = 0.0;
    if age < 0.15 {
        env = age / 0.15;
    } else if age < 0.75 {
        env = 1.0 - (age - 0.15) / 0.60;
    }
    if env <= 0.0 { return []; }

    // 1. Auroral boundary glow (soft ambient magic aura surrounding the rose frame)
    let aura_a = 0.22 * env;
    out.push(#{ rect: [-6.0, -6.0, w + 12.0, h + 12.0], fill: color_alpha(magic_c, aura_a), radius: rad + 6.0, soft: 14.0 });
    out.push(#{ rect: [-2.0, -2.0, w + 4.0, h + 4.0], fill: color_alpha(glow_c, aura_a * 0.7), radius: rad + 2.0, soft: 6.0 });

    // 2. Twinkling Sparkle Stars (8 strategic stars around corners, roses, and borders)
    let stars = [
        [8.0, 4.0, 0.02, 14.0],           // Near top-left botanical rose blossom
        [w * 0.48, -3.0, 0.09, 11.0],      // Along top vine / mini rose
        [w - 10.0, 6.0, 0.04, 13.0],       // Top-right corner
        [w - 8.0, h - 8.0, 0.12, 15.0],    // Bottom-right rose bud
        [16.0, h - 4.0, 0.18, 10.0],       // Bottom-left corner
        [-4.0, h * 0.48, 0.20, 11.0],      // Left vine climb
        [w + 3.0, h * 0.52, 0.15, 9.0],    // Right border edge
        [36.0, -7.0, 0.06, 12.0],          // Floating above chatter name
    ];

    for s in stars {
        let sx = s[0];
        let sy = s[1];
        let delay = s[2];
        let base_sz = s[3];
        let s_age = age - delay;
        if s_age > 0.0 && s_age < 0.55 {
            let st_prog = s_age / 0.55;
            let flare = 0.0;
            if st_prog < 0.25 {
                flare = ease_back(st_prog / 0.25);
            } else {
                let u = (st_prog - 0.25) / 0.75;
                flare = fmax(0.0, 1.0 - u);
            }
            let twinkle = 0.70 + 0.30 * cycle(s_age * 16.0);
            let cur_sz = base_sz * flare * twinkle;
            let star_alpha = fmin(1.0, flare * 1.3) * env;
            if cur_sz > 1.2 && star_alpha > 0.05 {
                for scmd in star_cmds(sx, sy, cur_sz, magic_c, glow_c, star_alpha) {
                    out.push(scmd);
                }
            }
        }
    }

    // 3. Drifting Magic Stardust Particles (22 motes of sparkling dust)
    let s = seed;
    for k in 0..22 {
        let r = rnd(s); s = r[1];
        let rx = r[0];
        r = rnd(s); s = r[1];
        let ry = r[0];
        r = rnd(s); s = r[1];
        let speed = 18.0 + r[0] * 32.0;
        r = rnd(s); s = r[1];
        let p_size = 1.2 + r[0] * 2.2;
        r = rnd(s); s = r[1];
        let sway_speed = 3.0 + r[0] * 4.0;
        r = rnd(s); s = r[1];
        let sway_amp = 6.0 + r[0] * 12.0;

        let start_x = rx * (w + 16.0) - 8.0;
        let start_y = ry * (h + 16.0) - 8.0;

        let px = start_x + (cycle(age * sway_speed + rx * 5.0) - 0.5) * sway_amp;
        let py = start_y - speed * age;

        let p_twinkle = 0.4 + 0.6 * cycle(age * (10.0 + ry * 12.0) + rx * 7.0);
        let p_alpha = env * p_twinkle;

        if p_alpha > 0.05 {
            out.push(#{ rect: [px - p_size, py - p_size, p_size * 2.0, p_size * 2.0], fill: color_alpha(magic_c, 0.75 * p_alpha), radius: p_size, soft: p_size * 1.5 });
            out.push(#{ rect: [px - p_size * 0.4, py - p_size * 0.4, p_size * 0.8, p_size * 0.8], fill: color_alpha(0xFFFFFFFF, 0.95 * p_alpha), radius: p_size * 0.4 });
        }
    }

    [#{ group: out, blend: "add" }]
}

fn bubble_cmds(x, y, w, h, fill, st, m) {
    if st.bubble == "rose-frame" {
        return rose_frame_cmds(x, y, w, h, fill, st, m);
    }
    let out = [];
    if st.bubble == "minimal" { return out; }
    if st.bubble == "bottom-line" {
        out.push(#{ rect: [x, y + h - 1.0, w, 1.0], fill: st.border_color | 0x4D000000 });
        return out;
    }
    if m.whisper { fill = 0xD9280028; }
    out.push(#{ rect: [x, y, w, h], fill: fill, radius: st.radius, border: st.border, border_color: st.border_color });
    if m.whisper || st.bubble == "left-border" {
        let accent = if m.whisper { 0xFFFF00E5 } else { chatter_color(m) };
        out.push(#{ rect: [x, y, 4.0, h], fill: accent, radius: fmin(st.radius, 2.0) });
    }
    out
}

fn icon_item(w, h, cmds) { #{ kind: "box", w: w, h: h, cmds: cmds } }

fn moved(cmds, dx, dy) {
    let out = [];
    for c in cmds {
        let d = c;
        if !none(d.rect) { d.rect = [d.rect[0] + dx, d.rect[1] + dy, d.rect[2], d.rect[3]]; }
        else { d.x = num(d.x) + dx; d.y = num(d.y) + dy; }
        out.push(d);
    }
    out
}

fn badge_item(b, st) {
    if b.kind == "image" {
        if !st.images && b.avatar { return (); }
        if !st.badges && !b.avatar { return (); }
        let s = if b.avatar { 20.0 } else { 18.0 };
        return icon_item(s, s, [#{ image: b.src, x: 0.0, y: 0.0, w: s, h: s, radius: if b.avatar { 0.0 } else { 0.0 } }]);
    }
    if !st.badges { return (); }
    let look = badge_look(b.code);
    let size = 14.0;
    let m = measure_text(look[1], st.font, size, 700);
    icon_item(18.0, 18.0, [#{ text: look[1], x: ((18.0 - m[0]) / 2.0).round(), y: ((18.0 - m[1]) / 2.0).round(), font: st.font, size: size, weight: 700, color: look[0], shadow: st.shadow }])
}

fn service_item(m, st) {
    if !st.images { return (); }
    if m.avatar != "" {
        return icon_item(20.0, 20.0, [
            #{ rect: [0.0, 0.0, 20.0, 20.0], fill: platform_ring(m.platform), radius: 0.0 },
            #{ image: m.avatar, x: 2.0, y: 2.0, w: 16.0, h: 16.0, radius: 0.0 },
        ]);
    }
    if m.platform == "" { return (); }
    let mark = switch m.platform { "twitch" => "T", "youtube" => "▶", _ => "K" };
    let ink = if m.platform == "kick" { 0xFF000000 } else { 0xFFFFFFFF };
    let t = measure_text(mark, st.font, 11.0, 800);
    icon_item(18.0, 18.0, [
        #{ rect: [0.0, 0.0, 18.0, 18.0], fill: platform_ring(m.platform), radius: 0.0 },
        #{ text: mark, x: ((18.0 - t[0]) / 2.0).round(), y: ((18.0 - t[1]) / 2.0).round(), font: st.font, size: 11.0, weight: 800, color: ink, shadow: false },
    ])
}

fn meta_items(m, st) {
    let items = [];
    for b in m.badges {
        let it = badge_item(b, st);
        if !none(it) { items.push(it); }
    }
    let service = service_item(m, st);
    if !none(service) { items.push(service); }
    items
}

fn word_items(text, color, weight, st) {
    let out = [];
    let word = "";
    for c in text.chars() {
        word += c;
        if c == ' ' {
            out.push(#{ kind: "text", s: word, color: color, weight: weight, w: measure_text(word, st.font, st.size, weight)[0] });
            word = "";
        }
    }
    if word != "" {
        out.push(#{ kind: "text", s: word, color: color, weight: weight, w: measure_text(word, st.font, st.size, weight)[0] });
    }
    out
}

fn body_items(m, st) {
    let items = [];
    let pending = false;
    for p in m.parts {
        if p.t == "text" {
            items += word_items(p.s, st.msg_color, st.w_msg, st);
        } else if st.emotes {
            let size = image_size(p.src);
            let h = st.emote_h;
            let w = h;
            if none(size) {
                pending = true;
            } else if num(size[0]) <= 0.0 {
                items += word_items(p.name + " ", st.msg_color, st.w_msg, st);
                continue;
            } else {
                w = (num(size[0]) * h / fmax(1.0, num(size[1]))).round();
            }
            items.push(icon_item(w + 4.0, h, [#{ image: p.src, x: 2.0, y: 0.0, w: w, h: h }]));
        }
    }
    #{ items: items, pending: pending }
}

fn flow(items, max_w, st) {
    let lines = [];
    let line = [];
    let x = 0.0;
    let widest = 0.0;
    for it in items {
        if x > 0.0 && x + it.w > max_w + 0.5 {
            lines.push(#{ items: line, w: x });
            widest = fmax(widest, x);
            line = [];
            x = 0.0;
        }
        let placed = it;
        placed.x = x;
        line.push(placed);
        x += it.w;
    }
    if line.len() > 0 { lines.push(#{ items: line, w: x }); widest = fmax(widest, x); }

    let cmds = [];
    let y = 0.0;
    let metric = measure_text("Ag", st.font, st.size, st.w_msg);
    for l in lines {
        let lh = st.lh;
        for it in l.items {
            if it.kind == "box" { lh = fmax(lh, it.h - 6.0); }
        }
        let run = ();
        for it in l.items {
            if it.kind == "text" {
                if !none(run) && run.color == it.color && run.weight == it.weight {
                    run.text += it.s;
                } else {
                    if !none(run) { cmds.push(run); }
                    run = #{ text: it.s, x: it.x, y: y + ((lh - metric[1]) / 2.0).round(), font: st.font, size: st.size, weight: it.weight, color: it.color, shadow: st.shadow };
                }
            } else {
                if !none(run) { cmds.push(run); run = (); }
                for c in moved(it.cmds, it.x, y + ((lh - it.h) / 2.0).round()) { cmds.push(c); }
            }
        }
        if !none(run) { cmds.push(run); }
        y += lh;
    }
    #{ w: widest, h: y, cmds: cmds }
}

fn header_items(m, st, with_meta) {
    let items = [];
    if with_meta {
        for it in meta_items(m, st) {
            let spaced = it;
            spaced.w = it.w + (if it.w >= 20.0 { 6.0 } else { 4.0 });
            items.push(spaced);
        }
    }
    if m.whisper {
        let tag = "[WHISPER] ";
        items.push(#{ kind: "text", s: tag, color: 0xFFFF00E5, weight: 700, w: measure_text(tag, st.font, st.size, 700)[0] });
    }
    let name = m.user;
    items.push(#{ kind: "text", s: name, color: name_color(m, st), weight: st.w_user, w: measure_text(name, st.font, st.size, st.w_user)[0] });
    items
}

fn slot_items(m, st) {
    let slots = (if st.badges { 3 } else { 0 }) + (if st.images { 1 } else { 0 });
    let active = [];
    let badges = [];
    for b in m.badges {
        if b.kind == "icon" || !b.avatar {
            let it = badge_item(b, st);
            if !none(it) { badges.push(it); }
        }
    }
    while badges.len() > 3 { badges.remove(0); }
    for b in badges { active.push(b); }
    let service = service_item(m, st);
    if !none(service) { active.push(service); }
    while active.len() > slots { active.remove(0); }
    let items = [];
    for i in 0..(slots - active.len()) { items.push(#{ kind: "box", w: 24.0, h: 20.0, cmds: [] }); }
    for it in active {
        let slot = it;
        slot.cmds = moved(it.cmds, ((20.0 - it.w) / 2.0).round(), 0.0);
        slot.w = 24.0;
        items.push(slot);
    }
    items
}

fn layout(m, st) {
    let body_list = body_items(m, st);
    let pending = body_list.pending;
    let aligned = st.layout == "inline-align" || st.layout == "stacked-align";
    let stacked = st.layout == "stacked" || st.layout == "stacked-split" || st.layout == "stacked-align";
    let px = st.pad_x;
    let py = st.pad_y;
    let body_fill = if st.transparent { 0 } else { fill_of(st.body_mode, st.bg, m) };

    if st.double {
        let split = st.layout == "stacked-split";
        let head_items = if aligned { slot_items(m, st) + header_items(m, st, false) } else { header_items(m, st, !split) };
        let head = flow(head_items, 100000.0, st);
        let meta = if split { flow(meta_items(m, st), 100000.0, st) } else { #{ w: 0.0, h: 0.0, cmds: [] } };
        let hpx = px;
        let hpy = st.pad_small;
        let head_w = if st.user_fixed_w > 0.0 { st.user_fixed_w } else { head.w + hpx * 2.0 };
        let line_h = fmax(head.h, meta.h);
        let head_h = line_h + hpy * 2.0;
        let head_fill = fill_of(st.header_mode, st.header_bg, m);
        let cmds = [];
        if stacked {
            let bpy = py;
            let inner = (if st.fixed_w > 0.0 { st.fixed_w } else { st.max_w }) - px * 2.0;
            let body = flow(body_list.items, inner, st);
            let body_w = if st.fixed_w > 0.0 { st.fixed_w } else { fmax(body.w + px * 2.0, 40.0) };
            if split && st.user_fixed_w <= 0.0 {
                head_w = fmax(fmax(160.0, head.w + 12.0 + meta.w + hpx * 2.0), body_w);
            }
            let body_y = head_h - st.overlap;
            let body_h = body.h + bpy * 2.0;
            for c in bubble_cmds(0.0, body_y, body_w, body_h, body_fill, st, m) { cmds.push(c); }
            for c in moved(body.cmds, px, body_y + bpy) { cmds.push(c); }
            for c in bubble_cmds(0.0, 0.0, head_w, head_h, head_fill, st, m) { cmds.push(c); }
            for c in moved(head.cmds, hpx, hpy + ((line_h - head.h) / 2.0).round()) { cmds.push(c); }
            if split {
                for c in moved(meta.cmds, head_w - hpx - meta.w, hpy + ((line_h - meta.h) / 2.0).round()) { cmds.push(c); }
            }
            return #{ w: fmax(head_w, body_w), h: body_y + body_h, cmds: cmds, pending: pending };
        }
        let inner = fmax(st.max_w - head_w + st.overlap - px * 2.0, 60.0);
        let body = flow(body_list.items, inner, st);
        let body_w = body.w + px * 2.0;
        let body_h = body.h + st.pad_small * 2.0;
        let h = fmax(head_h, body_h);
        let body_x = head_w - st.overlap;
        let head_y = ((h - head_h) / 2.0).round();
        let body_y = ((h - body_h) / 2.0).round();
        for c in bubble_cmds(body_x, body_y, body_w, body_h, body_fill, st, m) { cmds.push(c); }
        for c in moved(body.cmds, body_x + px, body_y + st.pad_small) { cmds.push(c); }
        for c in bubble_cmds(0.0, head_y, head_w, head_h, head_fill, st, m) { cmds.push(c); }
        for c in moved(head.cmds, hpx, head_y + hpy) { cmds.push(c); }
        return #{ w: body_x + body_w, h: h, cmds: cmds, pending: pending };
    }

    let inner_max = (if st.fixed_w > 0.0 { st.fixed_w } else { st.max_w }) - px * 2.0;
    let content = ();
    if stacked && st.layout == "stacked-split" {
        let name = flow(header_items(m, st, false), 100000.0, st);
        let meta = flow(meta_items(m, st), 100000.0, st);
        let body = flow(body_list.items, inner_max, st);
        let line_w = if st.fixed_w > 0.0 { inner_max } else { fmax(fmax(160.0 - px * 2.0, name.w + 12.0 + meta.w), body.w) };
        let line_h = fmax(name.h, meta.h);
        let gap = 4.0;
        let cmds = moved(name.cmds, 0.0, ((line_h - name.h) / 2.0).round());
        for c in moved(meta.cmds, line_w - meta.w, ((line_h - meta.h) / 2.0).round()) { cmds.push(c); }
        for c in moved(body.cmds, 0.0, line_h + gap) { cmds.push(c); }
        content = #{ w: line_w, h: line_h + gap + body.h, cmds: cmds };
    } else if stacked {
        let head = if aligned {
            flow(slot_items(m, st) + header_items(m, st, false), 100000.0, st)
        } else {
            flow(header_items(m, st, true), 100000.0, st)
        };
        let indent = if aligned { slot_items(m, st).len().to_float() * 24.0 } else { 0.0 };
        let body = flow(body_list.items, inner_max - indent, st);
        let gap = 4.0;
        content = #{ w: fmax(head.w, indent + body.w), h: head.h + gap + body.h, cmds: head.cmds + moved(body.cmds, indent, head.h + gap) };
    } else {
        let items = if aligned { slot_items(m, st) } else { [] };
        for it in header_items(m, st, !aligned) { items.push(it); }
        let sep = " ▶ ";
        items.push(#{ kind: "text", s: sep, color: st.msg_color, weight: st.w_msg, w: measure_text(sep, st.font, st.size, st.w_msg)[0] });
        for it in body_list.items { items.push(it); }
        content = flow(items, inner_max, st);
    }
    let w = if st.fixed_w > 0.0 { st.fixed_w } else { content.w + px * 2.0 };
    let h = content.h + py * 2.0;
    let cmds = bubble_cmds(0.0, 0.0, w, h, body_fill, st, m);
    for c in moved(content.cmds, px, py) { cmds.push(c); }
    #{ w: w, h: h, cmds: cmds, pending: pending }
}

// ─── Lifecycle & Rendering ──────────────────────────────────────────────────

fn update() {
    let s = this.settings;
    let changes = #{};
    if s.preset != s.applied_preset {
        let p = presets()[s.preset];
        if !none(p) {
            for k in p.keys() { changes[k] = p[k]; s[k] = p[k]; }
        }
        changes.applied_preset = s.preset;
    }
    this.style = style_of(s);
    this.size = [s.width, s.height];

    let before = shared_value("rosia_chat_look");
    share("rosia_chat_look", #{
        stamp: if is_map(before) { before.stamp + 1 } else { 1 },
        bg: s.bg, bg_mode: s.body_color_mode, text: s.msg_color, name: s.user_color, name_mode: s.username_color_mode,
        font: s.font, weight_user: s.font_weight_user, radius: s.border_radius, shadow: s.shadow, bubble: s.bubble_style,
        border: [s.border_top, s.border_right, s.border_bottom, s.border_left], border_color: s.border_gray,
        rose_color: s.rose_color, vine_color: s.vine_color,
        opacity: s.global_opacity,
    });
    if none(this.messages) { this.messages = []; this.count = 0; }
    for i in 0..this.messages.len() { this.messages[i].laid_ok = false; }
    changes
}

fn add_message(m) {
    this.messages.push(m);
    let keep = fmax(1.0, num(this.style.max_messages));
    while this.messages.len().to_float() > keep { this.messages.remove(0); }
}

fn leave(id, kind) {
    for i in 0..this.messages.len() {
        if this.messages[i].id == id && none(this.messages[i].leaving) {
            this.messages[i].leaving = this.time;
            this.messages[i].leave_kind = kind;
        }
    }
}

fn on_event(ev) {
    let kind = text_of(ev["type"]);
    if kind == "clear_chat" {
        for i in 0..this.messages.len() {
            if none(this.messages[i].leaving) {
                this.messages[i].leaving = this.time;
                this.messages[i].leave_kind = "removed";
            }
        }
        return;
    }
    if kind == "moderation" {
        let p = ev["payload"];
        if this.style.clean && is_map(p) && text_of(p["trigger"]).contains("FLAGGED") {
            this.leave(text_of(p["msg_id"]), "removed");
        }
        return;
    }
    this.count += 1;
    let m = message_of(ev, this.time, this.count);
    if !none(m) { this.add_message(m); }
}

fn test_messages() {
    [
        #{ type: "chat", source: "Twitch", username: "RosiaGuard", color: "#FF4081", badges: "[BROADCASTER] [VERIFIED]", message: "Welcome to the Gothic garden, traveler! [twitch_emote:30259]" },
        #{ type: "chat", source: "Twitch", username: "RetroKnight", color: "#00E676", badges: "[MOD]", message: "Steel armor polished, thorny roses bloomed." },
        #{ type: "chat", source: "YouTube", username: "PixelAdventurer", badges: "[MEMBER]", message: "8-bit dialog box looking pristine! [yt_emoji:yt-smile]" },
        #{ type: "chat", source: "Twitch", username: "ShadowMage", color: "#B19FFB", badges: "[SUB] [OG]", message: "Roses wrapping around the gray border! [twitch_emote:25]" },
        #{ type: "chat", source: "Kick", username: "CrimsonThorn", badges: "[VIP]", message: "The retro RPG vibes are immaculate." },
        #{ type: "whisper", source: "Twitch", username: "SecretFlower", message: "psst, a whispered confession from the garden" },
    ]
}

// This chat's look as a code for VTuber Friends, the Twitch extension: pasted
// in its settings ("Import from Theater"), its cards look like this chat
// (its rose frame as the extension's: the stone border and roses in its colour)
fn vf_look_code(s) {
    let look = #{
        vf_look: 1, font: text_of(s.font), size: num(s.fontsize),
        bg: s.bg, bg_mode: text_of(s.body_color_mode), text: s.msg_color, name: s.user_color, name_mode: text_of(s.username_color_mode),
        weight: num(s.font_weight_user), radius: num(s.border_radius),
        border: [num(s.border_top), num(s.border_right), num(s.border_bottom), num(s.border_left)], border_color: s.border_gray,
        shadow: s.shadow == true, bubble: text_of(s.bubble_style), opacity: num(s.global_opacity),
        rose_color: s.rose_color,
    };
    "VTF-LOOK " + look.to_json()
}

fn on_button(key) {
    if key == "copy_vf" {
        if copy_text(vf_look_code(this.settings)) { log("Copied this look for VTuber Friends: paste it in the extension's settings, Import from Theater"); }
        return;
    }
    if key == "test" {
        let list = test_messages();
        this.tests = (this.tests ?? 0) + 1;
        let ev = list[(this.tests - 1) % list.len()];
        ev.msg_id = "test" + this.tests;
        this.on_event(ev);
    }
}

fn tick(dt) {
    if none(this.messages) { return []; }
    let st = this.style;
    let now = this.time;
    let w = num(this.size[0]);
    let h = num(this.size[1]);

    // Expired or removed messages cleanup
    let i = this.messages.len() - 1;
    while i >= 0 {
        let leaving = this.messages[i].leaving;
        if none(leaving) {
            if st.expiry > 0.0 && now - this.messages[i].born > st.expiry {
                this.messages[i].leaving = now;
                this.messages[i].leave_kind = "expired";
            }
        } else if now - leaving > (if this.messages[i].leave_kind == "removed" { 0.25 } else { 0.35 }) {
            this.messages.remove(i);
        }
        i -= 1;
    }

    // Re-layout if pending emote loads or style changed
    for i in 0..this.messages.len() {
        let redo = !this.messages[i].laid_ok
            || (this.messages[i].pending && now - this.messages[i].laid_at > 0.25 && now - this.messages[i].born < 8.0);
        if redo {
            let laid = layout(this.messages[i], st);
            this.messages[i].pending = laid.pending;
            this.messages[i].laid = laid;
            this.messages[i].laid_ok = true;
            this.messages[i].laid_at = now;
        }
    }

    // Newest first: from the bottom up (older ones above), or with Flow
    // "newest at the top", from the top down (older ones below)
    let out = [];
    let down = st.top_down;
    let edge = if down { 15.0 } else { h - 15.0 };
    let i = this.messages.len() - 1;
    while i >= 0 && (if down { edge < h } else { edge > 0.0 }) {
        let laid_h = this.messages[i].laid.h;
        let laid_w = this.messages[i].laid.w;
        let leaving = this.messages[i].leaving;
        let y = if down { edge } else { edge - laid_h };
        let age = now - this.messages[i].born;
        let alpha = 1.0;
        let dx = 0.0;
        let dy = 0.0;
        let scale = 1.0;

        // Entrance animation
        if age < 0.35 {
            let t = fmax(0.0, age / 0.35);
            let e = ease_back(t);
            alpha = fmin(1.0, ease_out(t));
            switch st.animation {
                "pixel_pop" => {
                    // 8-bit stepped pixel pop
                    let step = (age * 12.0).floor().to_int();
                    if step <= 0 { scale = 0.80; alpha = 0.70; }
                    else if step == 1 { scale = 0.92; alpha = 0.90; }
                    else { scale = 1.0; alpha = 1.0; }
                },
                "magic" => {
                    // Ethereal magic materialization: soft bloom scale & glow
                    scale = 0.88 + 0.12 * e;
                    alpha = fmin(1.0, ease_out(t * 1.4));
                },
                "fade" => {},
                "scale" => { scale = 0.85 + 0.15 * e; },
                "left" => { dx = -30.0 * (1.0 - e); },
                // In from the side the messages come from
                _ => { dy = (if down { -16.0 } else { 16.0 }) * (1.0 - e); },
            }
        }

        // Exit animation
        if !none(leaving) {
            let removed = this.messages[i].leave_kind == "removed";
            let t = fmin(1.0, (now - leaving) / (if removed { 0.25 } else { 0.35 }));
            let e = ease_out(t);
            alpha *= 1.0 - e;
            if removed { dx = -25.0 * e; scale = 1.0 - 0.1 * e; } else { dy = -8.0 * e; scale = 1.0 - 0.05 * e; }
        }

        // Magic Stars & Magic Dust entrance effect (pushed before message so that after reverse it is drawn on top)
        if none(leaving) && age < 0.75 && (st.animation == "magic" || st.magic_stars) {
            let seed = this.messages[i].born.round().to_int() * 73 + i * 37 + 101;
            for m_cmd in magic_dust_cmds(laid_w, laid_h, age, st, this.messages[i], seed) {
                let m_grp = m_cmd;
                m_grp.x = 15.0 + dx;
                m_grp.y = y + dy;
                m_grp.scale = scale;
                m_grp.origin = [laid_w / 2.0, laid_h / 2.0];
                out.push(m_grp);
            }
        }

        out.push(#{ group: this.messages[i].laid.cmds, x: 15.0 + dx, y: y + dy, scale: scale, origin: [laid_w / 2.0, laid_h / 2.0], alpha: alpha });
        edge = if down { y + laid_h + 10.0 } else { y - 10.0 };
        i -= 1;
    }
    out.reverse();

    // 8-Bit Relay Status Indicator Bar
    let connected = relay_connected();
    if this.was_connected != connected {
        this.was_connected = connected;
        this.status_since = now;
    }
    if !connected || now - (this.status_since ?? 0.0) < 3.0 {
        let label = if connected { "[RELAY: CONNECTED]" } else { "[RELAY: WAITING…]" };
        let t = measure_text(label, st.font, 12.0, 700);
        let pill_w = t[0] + 32.0;
        let gray_base = st.border_gray;
        let status_color = if connected { 0xFF00E676 } else { 0xFFFF9100 };

        // Rounded stone gray status box (at the bottom when the newest
        // message is at the top, so it doesn't cover it)
        let py = if down { h - 39.0 } else { 15.0 };
        out.push(#{ rect: [15.0, py, pill_w, 24.0], fill: rgba(14, 14, 20, 0.90), radius: 8.0, border: 1.0, border_color: gray_base });
        out.push(#{ rect: [23.0, py + 9.0, 6.0, 6.0], fill: status_color, radius: 3.0 });
        out.push(#{ text: label, x: 35.0, y: py + ((24.0 - t[1]) / 2.0).round(), font: st.font, size: 12.0, weight: 700, color: rgba(240, 240, 250, 0.85), shadow: true });
    }

    if st.opacity < 1.0 { [#{ group: out, alpha: st.opacity }] } else { out }
}
