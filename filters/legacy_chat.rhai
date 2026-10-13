// legacy_chat.rhai — 💬 Legacy Chat: a copy of 💬 Theater Chat
// (theater_chat.rhai), its own source, to change apart from it.
//
// The stream chat, drawn by Theater Scene
// itself. No browser (CEF/Chromium) and no Control Panel needed: Theater
// connects to the relay (Theater Reader / Streamer Alert Relay) and hands this
// script each message (on_event); the script lays the chat out and returns
// what to draw each frame (tick).
//
// The Rhai version of chat_overlay.html (its presets, layouts, bubble
// styles, colours, badges, emotes, avatars, whispers, clean mode, expiry and
// animations) and of what Theater's chat.rs used to do (reading the relay's
// messages, emote tags, badges, username colours). Theater itself only
// fetches pictures (from Twitch, Kick, YouTube and the emote sites), sets
// text and draws.
//
// Edit it and save: it reloads by itself.

const ID = "legacy_chat_source";
const NAME = "💬 Legacy Chat";
const KIND = "source";

// ─── Settings ───────────────────────────────────────────────────────────────

fn defaults() {
    #{
        preset: "glass", applied_preset: "",
        width: 450, height: 700,
        layout_mode: "inline", bubble_style: "bubble",
        font: "Poppins", fontsize: 16, font_weight_user: 700, font_weight_msg: 400,
        msg_color: rgba(255, 255, 255, 1.0),
        username_color_mode: "platform", user_color: rgba(255, 159, 10, 1.0),
        body_color_mode: "custom", bg: rgba(20, 20, 30, 0.45),
        header_color_mode: "custom", header_bg: rgba(10, 10, 15, 0.85),
        padding: 8, border_radius: 12,
        bubble_width: 0, user_bubble_width: 0, bubble_overlap: 3,
        border_color: rgba(255, 255, 255, 0.15),
        border_top: 1, border_right: 1, border_bottom: 1, border_left: 1,
        shadow: true, emotes: true, user_badges: true, user_images: true,
        clean: false, animation: "bloom", expiry: 0, max_messages: 50,
        global_opacity: 1.0,
        // The light bar and lightning entrance
        fx_color_mode: "platform", fx_color: rgba(140, 200, 255, 1.0), fx_lightning: true, fx_duration: 0.6,
        // An old overlay link (chat_overlay.html#…) whose look it copies
        overlay_url: "", applied_url: "",
    }
}

// The HTML overlay's theme presets (picking one sets these)
fn presets() {
    #{
        glass: #{
            bg: rgba(20, 20, 30, 0.45), msg_color: rgba(255, 255, 255, 1.0), user_color: rgba(255, 159, 10, 1.0),
            padding: 8, border_radius: 12, font: "Poppins", fontsize: 16, shadow: true,
            bubble_style: "bubble", layout_mode: "inline", username_color_mode: "platform",
            body_color_mode: "custom", header_color_mode: "custom", header_bg: rgba(10, 10, 15, 0.85),
            font_weight_user: 700, font_weight_msg: 400, bubble_width: 0, user_bubble_width: 0, bubble_overlap: 3,
        },
        cyberpunk: #{
            bg: rgba(10, 5, 20, 0.9), msg_color: rgba(255, 255, 255, 1.0), user_color: rgba(0, 255, 255, 1.0),
            padding: 8, border_radius: 0, font: "Roboto Mono", fontsize: 15, shadow: true,
            bubble_style: "left-border", layout_mode: "stacked", username_color_mode: "platform",
            body_color_mode: "custom", header_color_mode: "custom", header_bg: rgba(10, 5, 20, 0.95),
            font_weight_user: 700, font_weight_msg: 400, bubble_width: 0, user_bubble_width: 0, bubble_overlap: 4,
        },
        midnight: #{
            bg: rgba(15, 15, 20, 0.95), msg_color: rgba(229, 229, 234, 1.0), user_color: rgba(255, 255, 255, 1.0),
            padding: 8, border_radius: 6, font: "Inter", fontsize: 16, shadow: false,
            bubble_style: "bubble", layout_mode: "inline", username_color_mode: "platform",
            body_color_mode: "custom", header_color_mode: "custom", header_bg: rgba(10, 10, 15, 0.95),
            font_weight_user: 600, font_weight_msg: 400, bubble_width: 0, user_bubble_width: 0, bubble_overlap: 3,
        },
        terminal: #{
            bg: rgba(0, 15, 0, 0.95), msg_color: rgba(57, 255, 20, 1.0), user_color: rgba(57, 255, 20, 1.0),
            padding: 6, border_radius: 4, font: "Roboto Mono", fontsize: 15, shadow: true,
            bubble_style: "left-border", layout_mode: "inline", username_color_mode: "custom",
            body_color_mode: "custom", header_color_mode: "custom", header_bg: rgba(0, 15, 0, 0.95),
            font_weight_user: 700, font_weight_msg: 400, bubble_width: 0, user_bubble_width: 0, bubble_overlap: 4,
        },
        glow: #{
            bg: rgba(25, 20, 35, 0.8), msg_color: rgba(255, 255, 255, 1.0), user_color: rgba(255, 159, 10, 1.0),
            padding: 10, border_radius: 16, font: "Outfit", fontsize: 16, shadow: true,
            bubble_style: "bubble", layout_mode: "inline", username_color_mode: "platform",
            body_color_mode: "custom", header_color_mode: "custom", header_bg: rgba(15, 10, 25, 0.9),
            font_weight_user: 700, font_weight_msg: 400, bubble_width: 0, user_bubble_width: 0, bubble_overlap: 4,
        },
    }
}

fn properties() {
    let weights = [["Regular", 400], ["Semi-bold", 600], ["Bold", 700]];
    let color_modes = [["One color", "custom"], ["The chatter's color", "user"], ["Transparent", "transparent"]];
    [
        info("Chat from the relay (Theater Reader / Streamer Alert Relay), drawn by Theater: no browser needed."),
        button("test", "💬 Send a test message"),
        button("copy_vf", "📋 Copy this look for VTuber Friends (the Twitch extension)"),
        text("overlay_url", "Old overlay link (chat_overlay.html#…): paste it to copy its look"),
        info("Its colours, font, layout, bubbles and borders become the settings below (change them after if you like). To copy it again, clear the field and paste it again."),
        choice("preset", "Theme", [["Glass", "glass"], ["Cyberpunk", "cyberpunk"], ["Midnight", "midnight"], ["Terminal", "terminal"], ["Glow", "glow"]]),
        int_slider("width", "Width", 150, 3840, 10),
        int_slider("height", "Height", 100, 2160, 10),
        choice("layout_mode", "Layout", [
            ["Single line", "inline"], ["Two lines", "stacked"], ["Two lines, split header", "stacked-split"],
            ["Single line, aligned badges", "inline-align"], ["Two lines, aligned badges", "stacked-align"],
        ]),
        choice("bubble_style", "Bubble", [
            ["Full bubble", "bubble"], ["Double bubble (name and message)", "double-bubble"], ["Left border accent", "left-border"],
            ["Underline separator", "bottom-line"], ["No bubble (text only)", "minimal"],
        ]),
        slider("global_opacity", "Opacity", 0.0, 1.0, 0.05),
        font_list("font", "Font"),
        int_slider("fontsize", "Font size", 12, 32, 1),
        choice("font_weight_user", "Name weight", weights),
        choice("font_weight_msg", "Message weight", weights),
        color("msg_color", "Message color"),
        choice("username_color_mode", "Name color", [["Platform colors", "platform"], ["The chatters' own colors", "user"], ["One color", "custom"]]),
        color("user_color", "Name color (one color)"),
        choice("body_color_mode", "Bubble color", color_modes),
        color("bg", "Bubble background"),
        choice("header_color_mode", "Name bubble color (double bubble)", color_modes),
        color("header_bg", "Name bubble background"),
        int_slider("padding", "Padding", 4, 24, 1),
        int_slider("border_radius", "Corner radius", 0, 30, 1),
        int_slider("bubble_width", "Bubble width (0: fits the message)", 0, 800, 10),
        int_slider("user_bubble_width", "Name bubble width (0: fits the name)", 0, 600, 10),
        int_slider("bubble_overlap", "Double bubble overlap", 0, 30, 1),
        color("border_color", "Border color"),
        int_slider("border_top", "Border: top", 0, 12, 1),
        int_slider("border_right", "Border: right", 0, 12, 1),
        int_slider("border_bottom", "Border: bottom", 0, 12, 1),
        int_slider("border_left", "Border: left", 0, 12, 1),
        toggle("shadow", "Text shadow"),
        toggle("emotes", "Emotes"),
        toggle("user_badges", "Badges"),
        toggle("user_images", "Avatars"),
        toggle("clean", "Remove messages the moderation flags"),
        choice("animation", "Entrance", [["Light bar + lightning", "bloom"], ["Slide up", "slide"], ["Fade in", "fade"], ["Scale up", "scale"], ["Slide in from the left", "left"]]),
        choice("fx_color_mode", "Light colour (light bar + lightning)", [["The platform's colour", "platform"], ["The chatter's colour", "user"], ["One colour", "custom"]]),
        color("fx_color", "Light colour (one colour)"),
        toggle("fx_lightning", "Lightning when a message comes in"),
        slider("fx_duration", "Light bar sweep (s)", 0.2, 2.0, 0.05),
        int_slider("expiry", "Hide messages after (s, 0: never)", 0, 60, 5),
        int_slider("max_messages", "Messages kept", 5, 100, 1),
    ]
}

// ─── Small helpers ──────────────────────────────────────────────────────────

fn fmax(a, b) { if a > b { a } else { b } }
fn fmin(a, b) { if a < b { a } else { b } }
fn num(v) { if type_of(v) == "i64" { v.to_float() } else if type_of(v) == "f64" { v } else { 0.0 } }
fn is_map(v) { type_of(v) == "map" }
fn text_of(v) { if type_of(v) == "string" { v } else { "" } }

// The first non-empty text among `values`
fn first_text(values) {
    for v in values {
        if type_of(v) == "string" && v != "" { return v; }
    }
    ""
}

// A field of a map that may not be one
fn field(m, key) { if is_map(m) { m[key] } else { () } }

// Whether `v` is () (nothing): comparing other types with () isn't reliable
fn none(v) { type_of(v) == "()" }

// cubic-bezier(0.34, 1.56, 0.64, 1): the overlay's springy entrance
fn ease_back(t) {
    let c1 = 1.70158;
    let c3 = c1 + 1.0;
    let u = t - 1.0;
    1.0 + c3 * u * u * u + c1 * u * u
}

fn ease_out(t) { let u = 1.0 - t; 1.0 - u * u * u }

// ─── The style: settings as the layout uses them ────────────────────────────

fn style_of(s) {
    let size = num(s.fontsize);
    let pad = num(s.padding);
    let bubble = s.bubble_style;
    let transparent = bubble == "minimal" || bubble == "bottom-line";
    #{
        font: if s.font == "" { "Poppins" } else { s.font },
        size: size,
        lh: (size * 1.5).round(),
        w_user: s.font_weight_user,
        w_msg: s.font_weight_msg,
        pad_y: if transparent { 0.0 } else { pad },
        pad_x: if transparent { 0.0 } else { pad + 4.0 },
        pad_small: fmax(2.0, pad - 4.0),
        radius: num(s.border_radius),
        border: [num(s.border_top), num(s.border_right), num(s.border_bottom), num(s.border_left)],
        border_color: s.border_color,
        bubble: bubble,
        transparent: transparent,
        double: bubble == "double-bubble",
        layout: s.layout_mode,
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
        fx_mode: s.fx_color_mode,
        fx_color: s.fx_color,
        fx_lightning: s.fx_lightning == true,
        sweep: fmax(0.1, num(s.fx_duration)),
        expiry: num(s.expiry),
        clean: s.clean,
        max_messages: s.max_messages,
        opacity: num(s.global_opacity),
    }
}

// ─── Reading the relay's messages (what chat.rs did) ────────────────────────

fn platform_of(text) {
    let p = text.to_lower();
    if p.contains("twitch") { "twitch" } else if p.contains("youtube") { "youtube" } else if p.contains("kick") { "kick" } else { "" }
}

// The name colour the overlay gives each platform, and its accent (the
// avatar ring, the chatter's colour when they have none)
fn platform_name_color(platform) {
    switch platform { "twitch" => 0xFFB19FFB, "youtube" => 0xFFFF5E5E, "kick" => 0xFF00E701, _ => 0xFFF0F0F0 }
}
fn platform_ring(platform) {
    switch platform { "twitch" => 0xFF9146FF, "youtube" => 0xFFFF0000, "kick" => 0xFF00E701, _ => 0x33FFFFFF }
}

// An emote tag's picture (#{ t: "emote", src }), or text for the few that
// are emoji, or () when `tag` isn't one: [twitch_emote:ID], [emote:ID:NAME]
// (Kick), [yt_emoji:URL]
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

// A message's text and emotes, in order: [#{ t: "text", s }, #{ t: "emote", src }, …]
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

// One badge: a system one (#{ kind: "icon", code }) or a picture
// (#{ kind: "image", src, avatar })
fn badge_of(tag) {
    let t = tag;
    t.trim();
    if t == "" { return (); }
    if t.starts_with("badge:") {
        let content = t.sub_string(6);
        // "badge:URL:TITLE": the URL has colons of its own
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

// The badges of a message: "[MOD] [SUB] [badge:URL:TITLE]", or a list
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

// A relay chat message as the chat keeps it, or () when it isn't one
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
        // Its number: its lightning's shape
        seq: n,
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

// ─── Laying a message out ───────────────────────────────────────────────────

// The chatter's colour (the bubble's in "the chatter's color" modes)
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

// A bubble's fill for a colour mode
fn fill_of(mode, own, m) {
    switch mode {
        "transparent" => 0,
        "user" => (chatter_color(m) & 0xFFFFFF) | (own & 0xFF000000),
        _ => own,
    }
}

// A system badge: its colour and symbol (the overlay's icons, as glyphs)
fn badge_look(code) {
    if code.contains("MOD") { return [0xFF00AD03, "⚔"]; }
    switch code {
        "BROADCASTER" => [0xFFE91916, "▶"],
        "VIP" => [0xFFE005B9, "♦"],
        "SUB" => [0xFF8205B3, "★"],
        "MEMBER" => [0xFF8205B3, "★"],
        "SUBSCRIBER" => [0xFF8205B3, "★"],
        "VERIFIED" => [0xFF1DA1F2, "✔"],
        "OWNER" => [0xFFF4D03F, "♛"],
        "OG" => [0xFF00E701, "ℹ"],
        "FOUNDER" => [0xFFFFD700, "★"],
        _ => [0xFF888888, "●"],
    }
}

// An inline box (an icon, a badge, an emote): its size and its drawing at 0,0
fn icon_item(w, h, cmds) { #{ kind: "box", w: w, h: h, cmds: cmds } }

// Moves `cmds` by (dx, dy): a box's drawing where it's placed
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
        return icon_item(s, s, [#{ image: b.src, x: 0.0, y: 0.0, w: s, h: s, radius: if b.avatar { s / 2.0 } else { 0.0 } }]);
    }
    if !st.badges { return (); }
    let look = badge_look(b.code);
    let size = 15.0;
    let m = measure_text(look[1], st.font, size, 700);
    icon_item(18.0, 18.0, [#{ text: look[1], x: ((18.0 - m[0]) / 2.0).round(), y: ((18.0 - m[1]) / 2.0).round(), font: st.font, size: size, weight: 700, color: look[0], shadow: false }])
}

// The chatter's avatar with its platform ring, or the platform's mark
fn service_item(m, st) {
    if !st.images { return (); }
    if m.avatar != "" {
        return icon_item(20.0, 20.0, [
            #{ rect: [0.0, 0.0, 20.0, 20.0], fill: platform_ring(m.platform), radius: 10.0 },
            #{ image: m.avatar, x: 2.0, y: 2.0, w: 16.0, h: 16.0, radius: 8.0 },
        ]);
    }
    if m.platform == "" { return (); }
    let mark = switch m.platform { "twitch" => "T", "youtube" => "▶", _ => "K" };
    let ink = if m.platform == "kick" { 0xFF000000 } else { 0xFFFFFFFF };
    let t = measure_text(mark, st.font, 11.0, 800);
    icon_item(18.0, 18.0, [
        #{ rect: [0.0, 0.0, 18.0, 18.0], fill: platform_ring(m.platform), radius: 4.0 },
        #{ text: mark, x: ((18.0 - t[0]) / 2.0).round(), y: ((18.0 - t[1]) / 2.0).round(), font: st.font, size: 11.0, weight: 800, color: ink, shadow: false },
    ])
}

// The meta items: badges, then the avatar or platform mark
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

// Words of `text` as inline text items (a space stays with the word before it)
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

// The message's words and emotes as inline items, and whether an emote's
// picture is still loading (it's laid out again once it's there)
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
                // Still loading: laid out again once it's there
                pending = true;
            } else if num(size[0]) <= 0.0 {
                // It won't load: its name instead
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

// Places inline `items` in lines at most `max_w` wide (a box or word that
// doesn't fit starts a line). Returns #{ w, h, cmds } from 0,0.
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
        // Text runs: words of one colour and weight side by side, drawn at once
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

// A bubble behind (x, y, w, h): the style's fill, border and accent
fn bubble_cmds(x, y, w, h, fill, st, m) {
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

// The header: name (and its meta items, or the aligned slots)
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

// The aligned layouts' slot column: empty slots first, then the badges and avatar
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

// A message's whole drawing: #{ w, h, cmds, pending }, from 0,0
fn layout(m, st) {
    let body_list = body_items(m, st);
    let pending = body_list.pending;
    let aligned = st.layout == "inline-align" || st.layout == "stacked-align";
    let stacked = st.layout == "stacked" || st.layout == "stacked-split" || st.layout == "stacked-align";
    let px = st.pad_x;
    let py = st.pad_y;
    let body_fill = if st.transparent { 0 } else { fill_of(st.body_mode, st.bg, m) };

    if st.double {
        // The name in a bubble of its own, the message in another under or beside it
        let split = st.layout == "stacked-split";
        let head_items = if aligned { slot_items(m, st) + header_items(m, st, false) } else { header_items(m, st, !split) };
        let head = flow(head_items, 100000.0, st);
        // Split header: the badges apart, on the right
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
                // As wide as the line (the overlay's 100%)
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
        // A line as wide as the bubble (a fixed width's, else the widest of
        // the header and the message, 160 px at least): the name on the
        // left, the badges on the right, centred on it
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
        let sep = ": ";
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

// ─── The old overlay's link: its look ───────────────────────────────────────
//
// chat_overlay.html keeps its settings after the # of its address, e.g.
// …chat_overlay.html#bg_r=20&bg_g=20&bg_b=30&bgopacity=0.7&font=%27Poppins%27…

// A link's value as text: + and the %-codes its settings use
fn url_decoded(v) {
    let s = v;
    s.replace("+", " ");
    for pair in [["%20", " "], ["%27", "'"], ["%22", "\""], ["%2C", ","], ["%2c", ","], ["%23", "#"], ["%2F", "/"], ["%2f", "/"],
                 ["%3A", ":"], ["%3a", ":"], ["%3D", "="], ["%3d", "="], ["%26", "&"], ["%2B", "+"], ["%2b", "+"], ["%25", "%"]] {
        s.replace(pair[0], pair[1]);
    }
    s
}

// A link's settings (what follows its #): name → text
fn link_params(link) {
    let out = #{};
    let hash = link.index_of("#");
    let query = if hash >= 0 { link.sub_string(hash + 1) } else { link };
    for pair in query.split("&") {
        let eq = pair.index_of("=");
        if eq > 0 { out[url_decoded(pair.sub_string(0, eq))] = url_decoded(pair.sub_string(eq + 1)); }
    }
    out
}

fn has(p, key) { type_of(p[key]) == "string" }

// A setting of the link as a number (`fallback` when it isn't one)
fn link_num(p, key, fallback) {
    if !has(p, key) { return fallback; }
    let t = p[key];
    t.trim();
    let out = fallback;
    try { out = parse_float(t); } catch { }
    out
}

// One 0–255 channel of a colour the link gives
fn link_channel(p, key) { fmax(0.0, fmin(255.0, link_num(p, key, 255.0))).round().to_int() }

// A colour the link gives as `prefix`_r, _g, _b and an opacity (`alpha_key`, "" for opaque)
fn link_color(p, prefix, alpha_key) {
    let a = if alpha_key == "" { 1.0 } else { fmax(0.0, fmin(1.0, link_num(p, alpha_key, 1.0))) };
    rgba(link_channel(p, prefix + "_r"), link_channel(p, prefix + "_g"), link_channel(p, prefix + "_b"), a)
}

// The first font of a CSS font list, without its quotes ("'Roboto Mono', monospace")
fn link_font(v) {
    let f = v;
    let comma = f.index_of(",");
    if comma >= 0 { f = f.sub_string(0, comma); }
    f.replace("'", "");
    f.replace("\"", "");
    f.trim();
    f
}

// The look an old overlay link describes, as this chat's settings; what the
// link doesn't say is left as it is (its entrance too: this chat keeps its own)
fn overlay_look(link) {
    let p = link_params(link);
    let look = #{};
    for pair in [["padding", "padding"], ["border_radius", "border_radius"], ["fontsize", "fontsize"],
                 ["font_weight_user", "font_weight_user"], ["font_weight_msg", "font_weight_msg"],
                 ["bubble_width", "bubble_width"], ["user_bubble_width", "user_bubble_width"], ["bubble_overlap", "bubble_overlap"],
                 ["border_top", "border_top"], ["border_right", "border_right"], ["border_bottom_w", "border_bottom"], ["border_left_w", "border_left"]] {
        if has(p, pair[0]) { look[pair[1]] = link_num(p, pair[0], 0.0).round().to_int(); }
    }
    for key in ["layout_mode", "bubble_style", "username_color_mode", "body_color_mode", "header_color_mode"] {
        if has(p, key) && p[key] != "" { look[key] = p[key]; }
    }
    for key in ["shadow", "emotes", "user_badges", "user_images", "clean"] {
        if has(p, key) { look[key] = p[key] == "true"; }
    }
    if has(p, "global_opacity") { look.global_opacity = fmax(0.0, fmin(1.0, link_num(p, "global_opacity", 1.0))); }
    if has(p, "font") {
        let f = link_font(p["font"]);
        if f != "" { look.font = f; }
    }
    if has(p, "bg_r") { look.bg = link_color(p, "bg", "bgopacity"); }
    if has(p, "msg_r") { look.msg_color = link_color(p, "msg", ""); }
    if has(p, "user_r") { look.user_color = link_color(p, "user", ""); }
    if has(p, "header_bg_r") { look.header_bg = link_color(p, "header_bg", "header_bgopacity"); }
    if has(p, "border_r") {
        look.border_color = link_color(p, "border", "border_opacity");
    } else if has(p, "border_color") && p["border_color"].starts_with("#") && p["border_color"].len() >= 7 {
        // The older #rrggbb border
        let hex = p["border_color"];
        let r = 255; let g = 255; let b = 255;
        try { r = parse_int(hex.sub_string(1, 2), 16); g = parse_int(hex.sub_string(3, 2), 16); b = parse_int(hex.sub_string(5, 2), 16); } catch { }
        look.border_color = rgba(r, g, b, fmax(0.0, fmin(1.0, link_num(p, "border_opacity", 0.15))));
    }
    // Its theme, marked as applied: picking it here doesn't undo the link's colours
    if has(p, "preset") && !none(presets()[p["preset"]]) {
        look.preset = p["preset"];
        look.applied_preset = p["preset"];
    }
    look
}

// ─── The light bar and lightning entrance ("bloom") ─────────────────────────
//
// A new message is shown from a glowing bar that sweeps across it right to
// left (what the bar has passed is shown), while a lightning bolt runs
// across it, right to left at mid-height, the moment it comes, crackles and
// fades.

// A number 0..1 from `seed`, and the next seed: the same message always
// strikes the same way
fn rnd(seed) {
    let next = (seed * 1103515245 + 12345) % 2147483648;
    [next.to_float() / 2147483648.0, next]
}

fn ease_in_out(t) {
    if t < 0.5 { 4.0 * t * t * t } else { let u = 2.0 - 2.0 * t; 1.0 - u * u * u / 2.0 }
}

// `c` with alpha `a` (0–1)
fn with_alpha(c, a) {
    (c & 0xFFFFFF) | ((fmax(0.0, fmin(1.0, a)) * 255.0).round().to_int() << 24)
}

// The light's colour for a message of `platform` by a chatter of `color`
fn fx_color_of(platform, color, st) {
    switch st.fx_mode {
        "custom" => st.fx_color,
        "user" => { if color != 0 { color | 0xFF000000 } else { platform_name_color(platform) } },
        _ => { if platform == "" { st.fx_color } else { platform_ring(platform) } },
    }
}

// How long the entrance lasts (the bar, then its fading)
fn bloom_length(st) { st.sweep + 0.25 }

// The lightning bolt across the message (`w` × `h`), `age` seconds in: from
// its right end to its left end, jagging up and down at mid-height, with
// forks, and a flash at each end. A new shape every few frames: it crackles.
fn bolt_cmds(w, h, age, color, seed) {
    let s = seed * 7919 + (age * 18.0).floor().to_int() + 1;
    let r = rnd(s); s = r[1];
    let mid_y = h * 0.5;
    // How far it may jag from the middle: most of the bubble's height
    let jag = fmax(4.0, fmin(h * 0.42, 16.0));
    let x0 = w + 6.0;
    let x1 = -6.0;
    let pts = [x0, mid_y];
    let forks = [];
    let n = fmax(4.0, ((x0 - x1) / 22.0).round()).to_int();
    for k in 1..n {
        let f = k.to_float() / n.to_float();
        r = rnd(s); s = r[1];
        // Calmer at its ends, wild in the middle
        let swing = (r[0] - 0.5) * 2.0 * jag * (0.35 + 0.65 * (1.0 - (2.0 * f - 1.0) * (2.0 * f - 1.0)));
        let px = x0 + (x1 - x0) * f;
        let py = mid_y + swing;
        pts.push(px); pts.push(py);
        // Now and then a fork, up or down, trailing left
        r = rnd(s); s = r[1];
        if r[0] < 0.3 && forks.len() < 3 {
            let dir = if swing < 0.0 { -1.0 } else { 1.0 };
            let bx = px;
            let by = py;
            let fork = [bx, by];
            for j in 0..3 {
                r = rnd(s); s = r[1];
                bx -= 6.0 + r[0] * 9.0;
                by += dir * (3.0 + r[0] * 5.0);
                fork.push(bx); fork.push(by);
            }
            forks.push(fork);
        }
    }
    pts.push(x1); pts.push(mid_y);
    let glow = with_alpha(color, 0.45);
    let mid = with_alpha(color, 0.95);
    let out = [
        #{ line: pts, width: 9.0, color: glow, soft: 10.0 },
        #{ line: pts, width: 3.5, color: mid, soft: 2.0 },
        #{ line: pts, width: 1.4, color: 0xFFFFFFFF },
    ];
    for fork in forks {
        out.push(#{ line: fork, width: 5.0, color: glow, soft: 6.0 });
        out.push(#{ line: fork, width: 1.6, color: mid, soft: 1.0 });
    }
    // The flashes where it starts and ends
    for fx in [x0, x1] {
        out.push(#{ rect: [fx - 5.0, mid_y - 5.0, 10.0, 10.0], fill: with_alpha(color, 0.85), radius: 5.0, soft: 24.0 });
        out.push(#{ rect: [fx - 2.5, mid_y - 2.5, 5.0, 5.0], fill: 0xFFFFFFFF, radius: 2.5, soft: 4.0 });
    }
    out
}

// A message's drawing (`cmds`, laid out `w` × `h`) `age` seconds after it
// came: shown from the bar rightwards, the bar and its glow, the lightning
fn bloom_parts(cmds, w, h, age, st, color, seed) {
    let t = fmin(1.0, age / st.sweep);
    let bar_x = (w + 6.0) - (w + 12.0) * ease_in_out(t);
    let parts = [];
    if t < 1.0 {
        // Only what the bar has passed (room around it for shadows and glyphs)
        parts.push(#{ group: cmds, clip: [bar_x, -60.0, w + 60.0 - bar_x, h + 120.0] });
    } else {
        parts.push(#{ group: cmds });
    }
    // The bar, its glow, and a light wake on what it just showed; through,
    // it fades
    let bar_alpha = if age < st.sweep { 1.0 } else { 1.0 - (age - st.sweep) / 0.25 };
    if bar_alpha > 0.0 {
        let top = -8.0;
        let bh = h + 16.0;
        parts.push(#{ group: [
            #{ rect: [bar_x, 0.0, fmin(36.0, w + 6.0 - bar_x), h], fill: with_alpha(color, 0.22), radius: 6.0, soft: 16.0 },
            #{ rect: [bar_x - 1.5, top, 3.0, bh], fill: with_alpha(color, 0.35), radius: 1.5, soft: 34.0 },
            #{ rect: [bar_x - 1.5, top, 3.0, bh], fill: with_alpha(color, 0.9), radius: 1.5, soft: 10.0 },
            #{ rect: [bar_x - 1.0, top + 2.0, 2.0, bh - 4.0], fill: 0xFFFFFFFF, radius: 1.0, soft: 1.5 },
        ], blend: "add", alpha: fmin(1.0, bar_alpha) });
    }
    // The lightning: the moment it comes, then gone in 0.4 s, flickering
    if st.fx_lightning && age < 0.4 {
        let lt = age / 0.4;
        let flick = [1.0, 0.45, 0.95, 0.7, 1.0, 0.3][(age * 40.0).floor().to_int() % 6];
        let fade = (1.0 - lt) * (1.0 - lt) * flick;
        parts.push(#{ group: bolt_cmds(w, h, age, color, seed), blend: "add", alpha: fmax(0.0, fmin(1.0, fade)) });
    }
    parts
}

// ─── The chat ───────────────────────────────────────────────────────────────

fn update() {
    let s = this.settings;
    let changes = #{};
    // An old overlay link pasted: its look, used right away and saved
    if text_of(s.overlay_url) != text_of(s.applied_url) {
        let look = overlay_look(text_of(s.overlay_url));
        for k in look.keys() { changes[k] = look[k]; s[k] = look[k]; }
        changes.applied_url = s.overlay_url;
    }
    // A theme picked: its settings, used right away and saved
    if s.preset != s.applied_preset {
        let p = presets()[s.preset];
        if !none(p) {
            for k in p.keys() { changes[k] = p[k]; s[k] = p[k]; }
        }
        changes.applied_preset = s.preset;
    }
    this.style = style_of(s);
    this.size = [s.width, s.height];
    // Its look, shared like 💬 Theater Chat's, under its own name (🔔 Theater
    // Alerts' "Look like the 💬 Theater Chat" keeps following Theater Chat;
    // the stamp tells a reader something changed)
    let before = shared_value("legacy_chat_look");
    share("legacy_chat_look", #{
        stamp: if is_map(before) { before.stamp + 1 } else { 1 },
        bg: s.bg, bg_mode: s.body_color_mode, text: s.msg_color, name: s.user_color, name_mode: s.username_color_mode,
        font: s.font, weight_user: s.font_weight_user, radius: s.border_radius, shadow: s.shadow, bubble: s.bubble_style,
        border: [s.border_top, s.border_right, s.border_bottom, s.border_left], border_color: s.border_color,
        opacity: s.global_opacity,
    });
    if none(this.messages) { this.messages = []; this.count = 0; }
    // Everything laid out again with the new style
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
        // Everything goes, the way a removed message does (a dock's button…)
        for i in 0..this.messages.len() {
            if none(this.messages[i].leaving) {
                this.messages[i].leaving = this.time;
                this.messages[i].leave_kind = "removed";
            }
        }
        return;
    }
    if kind == "moderation" {
        // Clean mode: a message the moderation flags goes
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

// Test messages (the overlay's simulator)
fn test_messages() {
    [
        #{ type: "chat", source: "Twitch", username: "StreamFan42", color: "#9146FF", badges: "[BROADCASTER] [VERIFIED]", message: "Hey everyone! How's the stream going? [twitch_emote:30259]" },
        #{ type: "chat", source: "Twitch", username: "KappaKing", color: "#1E90FF", badges: "[SUB] [OG]", message: "Kappa Kappa LUL [twitch_emote:25] [twitch_emote:425618]" },
        #{ type: "chat", source: "YouTube", username: "TubeWatcher", badges: "[MEMBER]", message: "Hello from YouTube! [yt_emoji:yt-smile]" },
        #{ type: "chat", source: "Twitch", username: "ModSquad", color: "#00AD03", badges: "[MOD]", message: "Please keep the chat friendly guys." },
        #{ type: "chat", source: "Kick", username: "KickRunner", badges: "[VIP]", message: "Kick chat checking in, that last play was insane!" },
        #{ type: "whisper", source: "Twitch", username: "SecretFriend", message: "psst, this one is a whisper" },
        #{ type: "chat", source: "Twitch", username: "NewViewer", color: "#FF69B4", badges: "[SUB]", message: "First time here, loving the vibes! [twitch_emote:88]" },
    ]
}

// This chat's look as a code for VTuber Friends, the Twitch extension: pasted
// in its settings ("Import from Theater"), its cards look like this chat
fn vf_look_code(s) {
    let look = #{
        vf_look: 1, font: text_of(s.font), size: num(s.fontsize),
        bg: s.bg, bg_mode: text_of(s.body_color_mode), text: s.msg_color, name: s.user_color, name_mode: text_of(s.username_color_mode),
        weight: num(s.font_weight_user), radius: num(s.border_radius),
        border: [num(s.border_top), num(s.border_right), num(s.border_bottom), num(s.border_left)], border_color: s.border_color,
        shadow: s.shadow == true, bubble: text_of(s.bubble_style), opacity: num(s.global_opacity),
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

    // (Each frame reads single fields: copying whole messages, drawings and
    // all, cost milliseconds on OBS's render thread)

    // Expired ones start leaving; those done leaving go
    let i = this.messages.len() - 1;
    while i >= 0 {
        let leaving = this.messages[i].leaving;
        if none(leaving) {
            if st.expiry > 0.0 && now - this.messages[i].born > st.expiry {
                this.messages[i].leaving = now;
                this.messages[i].leave_kind = "expired";
            }
        } else if now - leaving > (if this.messages[i].leave_kind == "removed" { 0.3 } else { 0.4 }) {
            this.messages.remove(i);
        }
        i -= 1;
    }

    // Laid out once (again after a style change, or while an emote loads)
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

    // From the bottom up, 15 px in, 8 px apart
    let out = [];
    let bottom = h - 15.0;
    let i = this.messages.len() - 1;
    while i >= 0 && bottom > 0.0 {
        let laid_h = this.messages[i].laid.h;
        let laid_w = this.messages[i].laid.w;
        let leaving = this.messages[i].leaving;
        let y = bottom - laid_h;
        let age = now - this.messages[i].born;
        let alpha = 1.0;
        let dx = 0.0;
        let dy = 0.0;
        let scale = 1.0;
        // Entrance: 0.35 s, springy
        if age < 0.35 {
            let t = fmax(0.0, age / 0.35);
            let e = ease_back(t);
            alpha = fmin(1.0, ease_out(t));
            switch st.animation {
                // The light bar shows it (bloom_parts)
                "bloom" => { alpha = 1.0; },
                "fade" => {},
                "scale" => { scale = 0.85 + 0.15 * e; },
                "left" => { dx = -30.0 * (1.0 - e); },
                _ => { dy = 20.0 * (1.0 - e); },
            }
        }
        // Leaving: expired ones rise and fade, removed ones slide away
        if !none(leaving) {
            let removed = this.messages[i].leave_kind == "removed";
            let t = fmin(1.0, (now - leaving) / (if removed { 0.3 } else { 0.4 }));
            let e = ease_out(t);
            alpha *= 1.0 - e;
            if removed { dx = -30.0 * e; scale = 1.0 - 0.1 * e; } else { dy = -10.0 * e; scale = 1.0 - 0.05 * e; }
        }
        if st.animation == "bloom" && none(leaving) && age < bloom_length(st) {
            let color = fx_color_of(this.messages[i].platform, this.messages[i].color, st);
            let parts = bloom_parts(this.messages[i].laid.cmds, laid_w, laid_h, age, st, color, this.messages[i].seq ?? (i + 1));
            out.push(#{ group: parts, x: 15.0 + dx, y: y + dy, alpha: alpha });
        } else {
            out.push(#{ group: this.messages[i].laid.cmds, x: 15.0 + dx, y: y + dy, scale: scale, origin: [laid_w / 2.0, laid_h / 2.0], alpha: alpha });
        }
        bottom = y - 8.0;
        i -= 1;
    }
    out.reverse();

    // The relay's state, a few seconds after it changes (and while it's down)
    let connected = relay_connected();
    if this.was_connected != connected {
        this.was_connected = connected;
        this.status_since = now;
    }
    if !connected || now - (this.status_since ?? 0.0) < 3.0 {
        let label = if connected { "Connected to the relay" } else { "Waiting for the relay…" };
        let t = measure_text(label, "Inter", 13.0, 600);
        let pill_w = t[0] + 36.0;
        out.push(#{ rect: [15.0, 15.0, pill_w, 28.0], fill: rgba(18, 18, 24, 0.7), radius: 14.0, border: 1.0, border_color: rgba(255, 255, 255, 0.08) });
        out.push(#{ rect: [27.0, 25.0, 8.0, 8.0], fill: if connected { 0xFF30D158 } else { 0xFFFF9F0A }, radius: 4.0 });
        out.push(#{ text: label, x: 41.0, y: 15.0 + ((28.0 - t[1]) / 2.0).round(), font: "Inter", size: 13.0, weight: 600, color: rgba(255, 255, 255, 0.7) });
    }

    if st.opacity < 1.0 { [#{ group: out, alpha: st.opacity }] } else { out }
}
