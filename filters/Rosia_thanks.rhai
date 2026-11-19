// Rosia_thanks.rhai — 🌹 Rosia Thanks: your supporters as end credits in
// 🌹 Rosia's style: 8-bit lettering inside a rose-wrapped stone frame,
// twinkling stars and stardust (or falling petals), and a rose-framed banner
// with Rosia's magic entrance for whoever supports during the stream.
// Drawn by Theater Scene: no browser needed.
//
// The names come from the Theater API plugins (supporters()): Twitch
// subscribers, Patreon patrons, Ko-fi supporters. Someone who subs, pledges
// or donates during the stream gets the banner, then joins the credits.

const ID = "rosia_thanks_source";
const NAME = "🌹 Rosia Thanks";
const KIND = "source";

// ─── Settings & Presets ─────────────────────────────────────────────────────

fn defaults() {
    #{
        preset: "rosia_gothic", applied_preset: "", match_chat: false,
        width: 1920, height: 1080,
        title: "Legendary Supporters", subtitle: "Thank you for helping the garden bloom",
        include_twitch: true, include_patreon: true, include_kofi: true,
        grouping: "platform", direction: "vertical", speed: 50.0, badges: true,
        font: "Roboto Mono", title_size: 52, name_size: 30, spacing: 12,
        bg: rgba(18, 18, 26, 0.92), title_color: rgba(255, 75, 125, 1.0), subtitle_color: rgba(200, 200, 220, 0.85),
        group_color: rgba(255, 130, 165, 1.0), name_color: rgba(240, 240, 250, 1.0),
        border_gray: rgba(120, 120, 142, 1.0), rose_color: rgba(225, 25, 85, 1.0), vine_color: rgba(45, 135, 70, 1.0),
        magic_color: rgba(255, 215, 80, 1.0), border_radius: 16, rose_style: "wrap",
        rose_amount: "lush",
        background: "magic", banner: true, banner_text: "Thank you, {name}!", banner_seconds: 5.5,
    }
}

fn presets() {
    #{
        rosia_gothic: #{
            bg: rgba(18, 18, 26, 0.92), title_color: rgba(255, 75, 125, 1.0), subtitle_color: rgba(200, 200, 220, 0.85),
            group_color: rgba(255, 130, 165, 1.0), name_color: rgba(240, 240, 250, 1.0),
            border_gray: rgba(120, 120, 142, 1.0), rose_color: rgba(225, 25, 85, 1.0), vine_color: rgba(45, 135, 70, 1.0),
            magic_color: rgba(255, 215, 80, 1.0), border_radius: 16, font: "Roboto Mono", rose_style: "wrap",
        },
        ruby_noir: #{
            bg: rgba(10, 10, 15, 0.96), title_color: rgba(255, 60, 95, 1.0), subtitle_color: rgba(220, 200, 210, 0.85),
            group_color: rgba(255, 110, 140, 1.0), name_color: rgba(255, 255, 255, 1.0),
            border_gray: rgba(140, 140, 160, 1.0), rose_color: rgba(200, 15, 55, 1.0), vine_color: rgba(30, 100, 50, 1.0),
            magic_color: rgba(255, 140, 180, 1.0), border_radius: 16, font: "Roboto Mono", rose_style: "wrap",
        },
        tea_rose: #{
            bg: rgba(28, 24, 28, 0.92), title_color: rgba(255, 140, 170, 1.0), subtitle_color: rgba(230, 215, 210, 0.85),
            group_color: rgba(255, 175, 195, 1.0), name_color: rgba(250, 245, 240, 1.0),
            border_gray: rgba(130, 125, 135, 1.0), rose_color: rgba(255, 130, 165, 1.0), vine_color: rgba(70, 145, 95, 1.0),
            magic_color: rgba(255, 200, 140, 1.0), border_radius: 18, font: "Roboto Mono", rose_style: "wrap",
        },
        retro_arcade: #{
            bg: rgba(0, 0, 0, 0.94), title_color: rgba(255, 215, 0, 1.0), subtitle_color: rgba(200, 240, 230, 0.85),
            group_color: rgba(255, 0, 110, 1.0), name_color: rgba(240, 255, 250, 1.0),
            border_gray: rgba(100, 110, 130, 1.0), rose_color: rgba(255, 0, 110, 1.0), vine_color: rgba(0, 220, 90, 1.0),
            magic_color: rgba(255, 230, 0, 1.0), border_radius: 12, font: "Roboto Mono", rose_style: "wrap",
        },
    }
}

fn properties() {
    [
        info("Your supporters as end credits in Rosia's style: Twitch subscribers, Patreon patrons and Ko-fi supporters, from the Theater API plugins. Whoever supports during the stream gets a rose-framed banner, then joins the credits. Drawn by Theater: no browser needed."),
        button("test", "🌹 Show test names and a banner"),
        button("restart", "⟲ Start the roll again"),
        choice("preset", "Theme", [["Rosia Gothic", "rosia_gothic"], ["Ruby Noir", "ruby_noir"], ["Tea Rose", "tea_rose"], ["Retro Arcade", "retro_arcade"]]),
        toggle("match_chat", "Look like the 🌹 Rosia Chat (its colours, font, roses; follows its changes)"),
        text("title", "Title"),
        text("subtitle", "Subtitle"),
        toggle("include_twitch", "Twitch subscribers (Theater API · Twitch)"),
        toggle("include_patreon", "Patreon patrons (Theater API · Patreon)"),
        toggle("include_kofi", "Ko-fi supporters (Theater API · Ko-fi)"),
        choice("grouping", "Group", [["By platform", "platform"], ["By tier", "tier"], ["One list", "none"]]),
        choice("direction", "Roll", [["Up the screen (credits)", "vertical"], ["Along the bottom (ticker)", "horizontal"]]),
        slider("speed", "Speed (pixels a second)", 10.0, 400.0, 5.0),
        toggle("badges", "Platform badges"),
        choice("rose_style", "Roses", [["Roses and vines", "wrap"], ["Corner roses", "corners"], ["Vines", "vines"], ["None", "none"]]),
        choice("rose_amount", "Rose garland", [["Lush: roses all around", "lush"], ["Some", "some"], ["Classic (Rosia Alert's three)", "classic"]]),
        choice("background", "Animation", [["Stars and stardust", "magic"], ["Falling petals", "petals"], ["Both", "both"], ["None", "none"]]),
        toggle("banner", "Show a banner when someone supports during the stream"),
        text("banner_text", "Banner text ({name}: who)"),
        slider("banner_seconds", "Banner seconds", 2.0, 15.0, 0.5),
        int_slider("width", "Width", 300, 3840, 10),
        int_slider("height", "Height", 100, 2160, 10),
        font_list("font", "Font"),
        int_slider("title_size", "Title size", 16, 140, 1),
        int_slider("name_size", "Name size", 12, 96, 1),
        int_slider("spacing", "Space between names", 0, 60, 1),
        color("title_color", "Title colour"),
        color("subtitle_color", "Subtitle colour"),
        color("group_color", "Group title colour"),
        color("name_color", "Name colour"),
        color("bg", "Panel colour"),
        color("border_gray", "Stone border colour"),
        color("rose_color", "Rose colour"),
        color("vine_color", "Vine colour"),
        color("magic_color", "Stars colour"),
        int_slider("border_radius", "Corner radius", 0, 40, 1),
    ]
}

// ─── Helpers ────────────────────────────────────────────────────────────────

fn fmax(a, b) { if a > b { a } else { b } }
fn fmin(a, b) { if a < b { a } else { b } }
fn num(v) {
    if type_of(v) == "i64" { v.to_float() }
    else if type_of(v) == "f64" { v }
    else if type_of(v) == "string" {
        let t = v;
        t.trim();
        let n = 0.0;
        try { n = parse_float(t); } catch { }
        n
    }
    else { 0.0 }
}
fn none(v) { type_of(v) == "()" }
fn is_map(v) { type_of(v) == "map" }
fn text_of(v) {
    if type_of(v) == "string" { v }
    else if type_of(v) == "i64" || type_of(v) == "f64" { v.to_string() }
    else { "" }
}

fn platform_color(p) {
    switch p {
        "twitch" => rgba(169, 112, 255, 1.0),
        "kick" => rgba(83, 252, 24, 1.0),
        "youtube" => rgba(255, 75, 75, 1.0),
        "patreon" => rgba(255, 66, 77, 1.0),
        "kofi" => rgba(41, 171, 224, 1.0),
        _ => rgba(255, 255, 255, 1.0),
    }
}

fn platform_name(p) {
    switch p { "twitch" => "Twitch", "patreon" => "Patreon", "kofi" => "Ko-fi", "kick" => "Kick", "youtube" => "YouTube", _ => p }
}

// A group's title, and its place among the groups (higher first)
fn group_of(s, grouping) {
    let tier = text_of(s.tier);
    let rank = switch tier { "Tier 3" => 3.0, "Tier 2" => 2.0, "Tier 1" => 1.0, "Prime" => 0.5, _ => 0.0 };
    let order = switch s.platform { "twitch" => 300.0, "patreon" => 200.0, "kofi" => 100.0, _ => 0.0 };
    if grouping == "platform" {
        let title = switch s.platform { "twitch" => "Twitch Subscribers", "patreon" => "Patreon Supporters", "kofi" => "Ko-fi Supporters", _ => platform_name(s.platform) };
        #{ title: title, order: order }
    } else if grouping == "tier" {
        let title = if s.platform == "twitch" { "Twitch " + (if tier == "" { "Subscribers" } else { tier }) }
                    else if tier == "" { platform_name(s.platform) + " Supporters" } else { tier };
        #{ title: title, order: order + rank * 10.0 + fmin(num(s.amount), 999.0) / 100.0 }
    } else {
        #{ title: "", order: 0.0 }
    }
}

// The roses' and stars' style, for Rosia's pieces
fn rose_style_of(s) {
    #{
        radius: num(s.border_radius), border_gray: s.border_gray, rose_color: s.rose_color, vine_color: s.vine_color,
        rose_style: text_of(s.rose_style), magic_color: s.magic_color, shadow: true,
    }
}

// The settings with the 🌹 Rosia Chat's look over them, when asked
fn look_of(s, c) {
    if s.match_chat != true || !is_map(c) { return s; }
    let m = s;
    let font = text_of(c.font);
    if font != "" { m.font = font; }
    m.bg = c.bg;
    m.name_color = c.text;
    m.subtitle_color = color_alpha(c.text, 0.75);
    m.border_radius = num(c.radius);
    m.border_gray = c.border_color;
    m.rose_color = c.rose_color;
    m.vine_color = c.vine_color;
    if text_of(c.name_mode) == "custom" { m.title_color = c.name; m.group_color = c.name; }
    m
}

// ─── The names ──────────────────────────────────────────────────────────────

fn update() {
    let s = this.settings;
    let changes = #{};
    // A theme picked: its settings, used right away and saved
    if s.preset != s.applied_preset {
        let p = presets()[s.preset];
        if !none(p) {
            for k in p.keys() { changes[k] = p[k]; s[k] = p[k]; }
        }
        changes.applied_preset = s.preset;
    }
    this.size = [s.width, s.height];
    if none(this.live) {
        this.live = []; this.test = []; this.pos = 0.0; this.seed = 4242; this.stars = []; this.petals = []; this.checked = -10.0;
        this.banners = []; this.banner = (); this.look = (); this.look_checked = -10.0;
    }
    if s.match_chat == true { this.look = shared_value("rosia_chat_look"); }
    this.key = "";
    this.laid = ();
    this.dirty = true;
    changes
}

// Someone who supports during the stream: a banner, then the credits
fn on_event(ev) {
    let a = ev["alert"];
    if type_of(a) != "map" || a.test == true { return; }
    let kind = text_of(a.kind);
    if !["sub", "resub", "gift", "pledge", "pledge_change", "donation", "shop_order", "commission", "member", "superchat"].contains(kind) { return; }
    let name = text_of(a.user);
    if name == "" || name == "Someone" || name == "Anonymous" || a.anonymous == true { return; }
    let tier = text_of(a.tier);
    if kind == "member" || kind == "superchat" { tier = "Members"; }
    let p = #{ name: name, platform: text_of(a.platform), tier: tier, amount: num(a.amount), gift: false };
    this.live.push(p);
    this.banners.push(p);
    while this.banners.len() > 10 { this.banners.remove(0); }
    this.dirty = true;
}

fn on_button(key) {
    if key == "test" {
        this.test = [
            #{ name: "RosiaGuard", platform: "twitch", tier: "Tier 1", amount: 0.0, gift: false },
            #{ name: "RubyKnight", platform: "twitch", tier: "Tier 3", amount: 0.0, gift: false },
            #{ name: "ThornWhisper", platform: "twitch", tier: "Tier 2", amount: 0.0, gift: true },
            #{ name: "BotanicalQueen", platform: "patreon", tier: "Golden Rose", amount: 10.0, gift: false },
            #{ name: "ShadowMage", platform: "patreon", tier: "Silver Thorn", amount: 5.0, gift: false },
            #{ name: "CrimsonPetal", platform: "kofi", tier: "", amount: 5.0, gift: false },
        ];
        this.banners.push(#{ name: "BotanicalQueen", platform: "patreon", tier: "Golden Rose", amount: 10.0, gift: false });
        this.dirty = true;
        this.pos = 0.0;
    } else if key == "restart" {
        this.pos = 0.0;
    }
}

// Everyone to show: the plugins' lists, the stream's new supporters, the test names
fn everyone(s, from_plugins, live, test) {
    let out = [];
    let seen = #{};
    for list in [from_plugins, live, test] {
        for p in list {
            let wanted = (p.platform == "twitch" && s.include_twitch != false) || (p.platform == "patreon" && s.include_patreon != false)
                      || (p.platform == "kofi" && s.include_kofi != false) || !["twitch", "patreon", "kofi"].contains(p.platform);
            let k = p.platform + "|" + p.name.to_lower();
            if wanted && !seen.contains(k) {
                seen[k] = true;
                out.push(p);
            }
        }
    }
    out
}

// The groups, each { title, names } in order (names alphabetical)
fn grouped(people, grouping) {
    let groups = #{};
    for p in people {
        let g = group_of(p, grouping);
        let key = g.title;
        if !groups.contains(key) { groups[key] = #{ title: g.title, order: g.order, names: [] }; }
        let entry = groups[key];
        if g.order > entry.order { entry.order = g.order; }
        entry.names.push(p);
        groups[key] = entry;
    }
    let list = groups.values();
    list.sort(|a, b| if a.order > b.order { -1 } else if a.order < b.order { 1 } else if a.title < b.title { -1 } else { 1 });
    // (A for loop's items are copies: each group is written back sorted)
    for i in 0..list.len() {
        let g = list[i];
        g.names.sort(|a, b| { let x = a.name.to_lower(); let y = b.name.to_lower(); if x < y { -1 } else if x > y { 1 } else { 0 } });
        list[i] = g;
    }
    list
}

// ─── Layout ─────────────────────────────────────────────────────────────────

// An 8-bit badge: square, the platform's colour
fn badge_cmds(p, x, y, size, font) {
    let label = platform_name(p.platform).to_upper();
    let bs = size * 0.36;
    let m = measure_text(label, font, bs, 700);
    let bw = m[0] + bs * 0.9;
    let bh = bs * 1.7;
    let c = platform_color(p.platform);
    #{
        w: bw,
        cmds: [
            #{ rect: [x, y + (size * 1.25 - bh) / 2.0, bw, bh], fill: with_alpha(c, 0.14), radius: 0.0, border: 1.5, border_color: with_alpha(c, 0.55) },
            #{ text: label, x: x + bs * 0.45, y: y + (size * 1.25 - m[1]) / 2.0, font: font, size: bs, weight: 700, color: c },
        ]
    }
}

// The credits, top to bottom: [#{ y, h, cmds }] and their whole height
fn layout_vertical(s, groups, w) {
    let font = text_of(s.font);
    let ts = num(s.title_size);
    let ns = num(s.name_size);
    let gap = num(s.spacing);
    let rows = [];
    let y = 0.0;
    let centered = |t, size, weight, color| {
        let m = measure_text(t, font, size, weight);
        #{ text: t, x: (w - m[0]) / 2.0, y: 0.0, font: font, size: size, weight: weight, color: color, shadow: true }
    };
    if text_of(s.title) != "" {
        rows.push(#{ y: y, h: ts * 1.3, cmds: [centered.call(s.title, ts, 700, s.title_color)] });
        y += ts * 1.4;
    }
    if text_of(s.subtitle) != "" {
        rows.push(#{ y: y, h: ts * 0.5, cmds: [centered.call(s.subtitle, ts * 0.4, 400, s.subtitle_color)] });
        y += ts * 0.4 * 1.3;
    }
    y += ts * 1.2;
    for g in groups {
        if g.title != "" {
            // ✿ TITLE ✿
            rows.push(#{ y: y, h: ns * 1.3, cmds: [centered.call("✿ " + g.title.to_upper() + " ✿", ns * 0.85, 700, s.group_color)] });
            y += ns * 1.3 + gap;
        }
        for p in g.names {
            let m = measure_text(p.name, font, ns, 500);
            let cmds = [];
            if s.badges != false {
                let b = badge_cmds(p, 0.0, 0.0, ns, font);
                let total = b.w + 10.0 + m[0];
                let x = (w - total) / 2.0;
                for c in b.cmds {
                    if !none(c.rect) { c.rect[0] += x; } else { c.x += x; }
                    cmds.push(c);
                }
                cmds.push(#{ text: p.name, x: x + b.w + 10.0, y: (ns * 1.25 - m[1]) / 2.0, font: font, size: ns, weight: 500, color: s.name_color, shadow: true });
            } else {
                cmds.push(#{ text: p.name, x: (w - m[0]) / 2.0, y: (ns * 1.25 - m[1]) / 2.0, font: font, size: ns, weight: 500, color: s.name_color, shadow: true });
            }
            rows.push(#{ y: y, h: ns * 1.25, cmds: cmds });
            y += ns * 1.25 + gap;
        }
        y += ns * 1.4;
    }
    if groups.len() == 0 {
        rows.push(#{ y: y, h: ns * 1.3, cmds: [centered.call("The garden awaits its legends…", ns * 0.7, 500, color_alpha(s.name_color, 0.6))] });
        y += ns * 1.3;
    }
    #{ rows: rows, h: y }
}

// The ticker, left to right: [#{ x, w, cmds }] and its whole width
fn layout_horizontal(s, groups, bar_h) {
    let font = text_of(s.font);
    let ns = num(s.name_size) * 0.72;
    let gap = fmax(num(s.spacing), 8.0) * 1.5;
    let items = [];
    let x = 0.0;
    let y_of = |size| (bar_h - size * 1.25) / 2.0;
    let add = |t, size, weight, color| {
        let m = measure_text(t, font, size, weight);
        #{ x: 0.0, w: m[0], cmds: [#{ text: t, x: 0.0, y: (bar_h - m[1]) / 2.0, font: font, size: size, weight: weight, color: color, shadow: true }] }
    };
    if text_of(s.title) != "" {
        let it = add.call(s.title, ns * 1.1, 700, s.title_color);
        it.x = x; items.push(it); x += it.w + gap * 1.5;
    }
    for g in groups {
        if g.title != "" {
            let it = add.call(g.title.to_upper() + ":", ns * 0.9, 700, s.group_color);
            it.x = x; items.push(it); x += it.w + gap;
        }
        let i = 0;
        for p in g.names {
            let cmds = [];
            let w = 0.0;
            if s.badges != false {
                let b = badge_cmds(p, 0.0, y_of.call(ns), ns, font);
                for c in b.cmds { cmds.push(c); }
                w = b.w + 8.0;
            }
            let m = measure_text(p.name, font, ns, 500);
            cmds.push(#{ text: p.name, x: w, y: (bar_h - m[1]) / 2.0, font: font, size: ns, weight: 500, color: s.name_color, shadow: true });
            w += m[0];
            items.push(#{ x: x, w: w, cmds: cmds });
            x += w;
            i += 1;
            if i < g.names.len() {
                let dot = add.call("✿", ns * 0.8, 500, color_alpha(s.rose_color, 0.8));
                dot.x = x + gap / 2.0 - dot.w / 2.0;
                items.push(dot);
                x += gap;
            }
        }
        x += gap * 2.0;
    }
    #{ items: items, w: x }
}

// A new supporter's banner, rose-framed
fn banner_cmds(b, s, max_w) {
    let font = text_of(s.font);
    let size = num(s.name_size) * 1.05;
    let line = text_of(s.banner_text);
    line.replace("{name}", b.name);
    let detail = platform_name(b.platform) + (if text_of(b.tier) != "" { " · " + b.tier } else { "" });
    let m1 = measure_text(line, font, size, 700);
    let m2 = measure_text(detail, font, size * 0.6, 500);
    let pad = size * 1.1;
    let cw = fmin(fmax(m1[0], m2[0]) + pad * 2.0, max_w);
    let ch = m1[1] + m2[1] + pad * 1.6;
    let cmds = rose_frame_cmds(0.0, 0.0, cw, ch, s.bg, rose_style_of(s));
    cmds.push(#{ text: line, x: (cw - m1[0]) / 2.0, y: pad * 0.7, font: font, size: size, weight: 700, color: s.title_color, shadow: true });
    cmds.push(#{ text: detail, x: (cw - m2[0]) / 2.0, y: pad * 0.7 + m1[1] + pad * 0.25, font: font, size: size * 0.6, weight: 500, color: platform_color(b.platform) });
    #{ w: cw, h: ch, cmds: cmds }
}

// ─── More roses ─────────────────────────────────────────────────────────────

// Roses all around a big panel (Rosia's frame has three: a blossom, a small
// rose, a bud): a blossom in every corner, small roses, buds and little
// blossoms along every edge, vines on every side. "lush" or "some";
// "classic" leaves Rosia's frame as it is.
fn garland_cmds(x, y, w, h, st, amount) {
    let out = [];
    if amount == "classic" || st.rose_style == "none" { return out; }
    let rose_mid = st.rose_color;
    let rose_hi = blend_rgb(rose_mid, 0xFFFFFFFF, 0.45);
    let rose_dark = blend_rgb(rose_mid, 0xFF000000, 0.45);
    let rose_core = blend_rgb(rose_mid, 0xFF000000, 0.70);
    let vine_mid = st.vine_color;
    let vine_hi = blend_rgb(vine_mid, 0xFF80FF80, 0.35);
    let vine_dark = blend_rgb(vine_mid, 0xFF000000, 0.45);
    let rad = fmax(16.0, num(st.radius));
    let draw_vines = st.rose_style == "wrap" || st.rose_style == "vines";
    let draw_roses = st.rose_style == "wrap" || st.rose_style == "corners";

    // The vines the frame doesn't have: down the right side, along the bottom
    if draw_vines {
        let vy = y + rad + 8.0;
        while vy < y + h - rad - 10.0 {
            let seg_h = fmin(30.0, y + h - rad - 10.0 - vy);
            let rx = x + w - 1.0;
            out.push(#{ line: [rx, vy, rx, vy + seg_h], width: 2.2, color: vine_dark, soft: 0.4 });
            out.push(#{ line: [rx + 3.5, vy + 6.0, rx, vy + 9.0, rx - 5.0, vy + 11.5], width: 3.2, color: vine_mid, soft: 0.5 });
            out.push(#{ line: [rx + 0.5, vy + 18.0, rx + 5.5, vy + 22.5], width: 1.6, color: vine_dark, soft: 0.4 });
            out.push(#{ rect: [rx + 1.0, vy + 26.0, 6.0, 10.0], radius: 3.0, fill: vine_mid });
            vy += 42.0;
        }
        let vx = x + rad + 8.0;
        while vx < x + w - rad - 50.0 {
            let seg_w = fmin(34.0, x + w - rad - 50.0 - vx);
            let by = y + h - 1.0;
            out.push(#{ line: [vx, by, vx + seg_w, by], width: 2.2, color: vine_dark, soft: 0.4 });
            out.push(#{ line: [vx + 6.0, by + 3.5, vx + 9.0, by, vx + 11.5, by - 5.0], width: 3.2, color: vine_mid, soft: 0.5 });
            out.push(#{ line: [vx + 20.0, by + 0.5, vx + 24.5, by + 5.5], width: 1.6, color: vine_dark, soft: 0.4 });
            out.push(#{ rect: [vx + 28.0, by + 1.0, 10.0, 6.0], radius: 3.0, fill: vine_mid });
            vx += 48.0;
        }
    }

    if draw_roses {
        let blossom = |cx, cy, scale| rose_blossom_cmds(cx, cy, scale, rose_mid, rose_hi, rose_dark, rose_core, vine_mid, vine_hi, vine_dark);
        let bud = |bx, by, scale| rose_bud_cmds(bx, by, scale, rose_mid, rose_hi, rose_dark, rose_core, vine_mid, vine_hi, vine_dark);
        let mini = |cx, cy| mini_rose_cmds(cx, cy, rose_mid, rose_hi, rose_dark, rose_core, vine_mid, vine_hi);
        // A blossom in the corners the frame leaves bare, and a second, smaller one by each corner when lush
        let corner = if amount == "lush" { 1.25 } else { 1.0 };
        for c in blossom.call(x + w - 6.0, y + 6.0, corner) { out.push(c); }
        for c in blossom.call(x + 6.0, y + h - 6.0, corner) { out.push(c); }
        for c in blossom.call(x + w - 8.0, y + h - 8.0, corner * 0.9) { out.push(c); }
        if amount == "lush" {
            for c in blossom.call(x + 44.0, y + 4.0, 0.7) { out.push(c); }
            for c in blossom.call(x + w - 44.0, y + 4.0, 0.7) { out.push(c); }
            for c in blossom.call(x + 4.0, y + h - 44.0, 0.7) { out.push(c); }
            for c in blossom.call(x + w - 4.0, y + h - 44.0, 0.7) { out.push(c); }
        }
        if st.rose_style == "corners" { return out; }

        // Along every edge, away from the corners: small roses, buds and little blossoms, in turn
        let spacing = if amount == "lush" { 110.0 } else { 220.0 };
        let i = 0;
        let px = x + rad + 90.0;
        while px < x + w - rad - 90.0 {
            let k = i % 3;
            let top = if k == 0 { mini.call(px, y + 1.0) } else if k == 1 { bud.call(px, y - 4.0, 0.85) } else { blossom.call(px, y + 1.0, 0.55) };
            let bottom = if k == 1 { mini.call(px + spacing / 2.0, y + h - 1.0) } else if k == 2 { bud.call(px + spacing / 2.0, y + h - 8.0, 0.85) } else { blossom.call(px + spacing / 2.0, y + h - 1.0, 0.55) };
            for c in top { out.push(c); }
            if px + spacing / 2.0 < x + w - rad - 90.0 { for c in bottom { out.push(c); } }
            px += spacing;
            i += 1;
        }
        let j = 0;
        let py = y + rad + 90.0;
        while py < y + h - rad - 90.0 {
            let k = j % 3;
            let left = if k == 0 { blossom.call(x + 1.0, py, 0.55) } else if k == 1 { mini.call(x + 1.0, py) } else { bud.call(x - 4.0, py, 0.85) };
            let right = if k == 0 { mini.call(x + w - 1.0, py + spacing / 2.0) } else if k == 1 { bud.call(x + w - 8.0, py + spacing / 2.0, 0.85) } else { blossom.call(x + w - 1.0, py + spacing / 2.0, 0.55) };
            for c in left { out.push(c); }
            if py + spacing / 2.0 < y + h - rad - 90.0 { for c in right { out.push(c); } }
            py += spacing;
            j += 1;
        }
    }
    out
}

// ─── Stars, stardust and petals ─────────────────────────────────────────────

fn rand() {
    this.seed = (this.seed * 1103515245 + 12345) % 2147483648;
    this.seed.to_float() / 2147483648.0
}

fn effects(dt, x0, y0, w, h, s) {
    let cmds = [];
    let kind = text_of(s.background);
    if kind == "magic" || kind == "both" {
        // Twinkling 4-pointed stars, and a slow drift of dust
        while this.stars.len() < 26 {
            this.stars.push(#{ x: x0 + this.rand() * w, y: y0 + this.rand() * h, size: 5.0 + this.rand() * 11.0, phase: this.rand(), speed: 0.25 + this.rand() * 0.5, dust: this.rand() < 0.5 });
        }
        let moved = [];
        for p in this.stars {
            let tw = cycle(this.time * p.speed + p.phase);
            if p.dust {
                p.y -= 8.0 * dt;
                if p.y < y0 { p.y = y0 + h; p.x = x0 + this.rand() * w; }
                cmds.push(#{ rect: [p.x - 1.5, p.y - 1.5, 3.0, 3.0], fill: color_alpha(s.magic_color, 0.25 + 0.5 * tw), radius: 1.5, soft: 4.0 });
            } else {
                for c in star_cmds(p.x, p.y, p.size * (0.6 + 0.4 * tw), s.magic_color, blend_rgb(s.magic_color, 0xFFFFFFFF, 0.45), 0.25 + 0.6 * tw) { cmds.push(c); }
            }
            moved.push(p);
        }
        this.stars = moved;
    }
    if kind == "petals" || kind == "both" {
        while this.petals.len() < 16 {
            this.petals.push(#{ x: x0 + this.rand() * w, y: y0 - this.rand() * h, size: 6.0 + this.rand() * 8.0, v: 20.0 + this.rand() * 30.0, phase: this.rand() * 6.28 });
        }
        let moved = [];
        for p in this.petals {
            p.y += p.v * dt;
            if p.y > y0 + h + 20.0 { p.y = y0 - 20.0; p.x = x0 + this.rand() * w; }
            let sway = (this.time * 1.1 + p.phase).sin() * 18.0;
            let flip = (this.time * 2.0 + p.phase).cos().abs();
            cmds.push(#{ rect: [p.x + sway, p.y, p.size, p.size * (0.45 + 0.4 * flip)], fill: color_alpha(s.rose_color, 0.65), radius: p.size * 0.5 });
            moved.push(p);
        }
        this.petals = moved;
    }
    [#{ group: cmds, x: 0.0, y: 0.0, blend: "add" }]
}

// ─── Each frame ─────────────────────────────────────────────────────────────

fn tick(dt) {
    if none(this.live) { this.update(); }
    // Looking like the chat: its changes followed (checked every second)
    if this.settings.match_chat == true && this.time - this.look_checked >= 1.0 {
        this.look_checked = this.time;
        let c = shared_value("rosia_chat_look");
        if is_map(c) && (!is_map(this.look) || c.stamp != this.look.stamp) {
            this.look = c;
            this.dirty = true;
        }
    }
    let s = look_of(this.settings, this.look);
    let w = num(s.width);
    let h = num(s.height);
    let horizontal = s.direction == "horizontal";
    let bar_h = num(s.name_size) * 2.1;
    // The panel the credits roll in: inside the rose frame (room for its roses)
    let margin = fmax(28.0, fmin(w, h) * 0.04);
    let px = margin;
    let py = if horizontal { h - bar_h - margin * 0.5 } else { margin };
    let pw = w - margin * 2.0;
    let ph = if horizontal { bar_h } else { h - margin * 2.0 };

    // The plugins' lists, looked at every few seconds (or now, when the
    // settings or the names changed); laid out again only when what's
    // shown changes
    if this.time - this.checked > 3.0 || this.dirty != false || none(this.laid) {
        this.checked = this.time;
        this.dirty = false;
        this.from_plugins = supporters();
        let people = everyone(s, this.from_plugins, this.live, this.test);
        let key = "";
        for v in [w, h, s.grouping, s.direction, s.font, s.name_size, s.title_size, s.spacing, s.badges, s.title, s.subtitle, s.title_color, s.subtitle_color, s.name_color, s.group_color, s.rose_color] {
            key += "|" + v;
        }
        for p in people { key += "|" + p.name; }
        if key != this.key || none(this.laid) {
            let groups = grouped(people, text_of(s.grouping));
            this.laid = if horizontal { layout_horizontal(s, groups, ph) } else { layout_vertical(s, groups, pw) };
            this.key = key;
        }
    }

    let cmds = [];
    // The frame and its roses: drawn again only when they change
    let fkey = "";
    for v in [px, py, pw, ph, s.bg, s.border_gray, s.rose_color, s.vine_color, s.rose_style, s.rose_amount, s.border_radius] { fkey += "|" + v; }
    if fkey != this.frame_key {
        let frame = rose_frame_cmds(px, py, pw, ph, s.bg, rose_style_of(s));
        for c in garland_cmds(px, py, pw, ph, rose_style_of(s), text_of(s.rose_amount)) { frame.push(c); }
        this.frame_cmds = frame;
        this.frame_key = fkey;
    }
    cmds.push(#{ group: this.frame_cmds });
    for e in this.effects(dt, px, py, pw, ph, s) { cmds.push(e); }

    // The roll, inside the panel: from its bottom (or right side) to past its other edge, again and again
    this.pos += num(s.speed) * dt;
    let inside = [];
    if horizontal {
        let total = this.laid.w;
        if this.pos > pw + total { this.pos = 0.0; }
        let x0 = pw - this.pos;
        for it in this.laid.items {
            let x = x0 + it.x;
            if x + it.w >= 0.0 && x <= pw {
                inside.push(#{ group: it.cmds, x: x, y: 0.0 });
            }
        }
    } else {
        let total = this.laid.h;
        if this.pos > ph + total { this.pos = 0.0; }
        let y0 = ph - this.pos;
        for r in this.laid.rows {
            let y = y0 + r.y;
            if y + r.h >= 0.0 && y <= ph {
                inside.push(#{ group: r.cmds, x: 0.0, y: y });
            }
        }
    }
    let inset = fmax(6.0, num(s.border_radius) * 0.5);
    cmds.push(#{ group: inside, x: px, y: py, clip: [inset, inset, pw - inset * 2.0, ph - inset * 2.0] });

    // A new supporter's banner, one at a time: Rosia's magic entrance, then it fades
    if s.banner != false {
        if none(this.banner) && this.banners.len() > 0 {
            let b = this.banners.remove(0);
            b.start = this.time;
            this.banner = b;
        }
        if !none(this.banner) {
            let b = this.banner;
            let age = this.time - b.start;
            let total = fmax(1.5, num(s.banner_seconds));
            if age > total {
                this.banner = ();
            } else {
                let laid = banner_cmds(b, s, w * 0.85);
                let bx = ((w - laid.w) / 2.0).round();
                let by = if horizontal { py - laid.h - margin } else { h * 0.1 };
                let enter = 0.45;
                let scale = if age < enter { 0.88 + 0.12 * ease_back(age / enter) } else { 1.0 };
                let alpha = fmin(fmin(1.0, ease_out(fmin(1.0, age / enter * 1.4))), fmin(1.0, (total - age) / 0.5));
                cmds.push(#{ group: laid.cmds, x: bx, y: by, scale: scale, origin: [laid.w / 2.0, laid.h / 2.0], alpha: alpha });
                if age < 0.85 {
                    let seed = (b.start * 1000.0).floor().to_int() % 100000 + 77;
                    for m in magic_dust_cmds(laid.w, laid.h, age, rose_style_of(s), seed) {
                        let g = m;
                        g.x = bx;
                        g.y = by;
                        g.scale = scale;
                        g.origin = [laid.w / 2.0, laid.h / 2.0];
                        cmds.push(g);
                    }
                }
            }
        }
    }
    cmds
}

// ─── Rosia's pieces (🌹 Rosia Alert's): colours, roses, the frame, stars, stardust ───

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

// ─── Botanical Roses & Frame Rendering ──────────────────────────────────────

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
fn rose_frame_cmds(x, y, w, h, fill, st) {
    let out = [];
    let rad = num(st.radius);
    if rad <= 0.0 { rad = 16.0; }

    // 1. Rounded Dialog background
    out.push(#{ rect: [x, y, w, h], fill: fill, radius: rad });

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

    // 4 Corner studs / rivets in silver
    let stud = 0xFFD0D0E0;
    let offset = fmax(6.0, rad * 0.45);
    out.push(#{ rect: [x + offset, y + offset, 3.5, 3.5], fill: stud, radius: 1.75 });
    out.push(#{ rect: [x + w - offset - 3.5, y + offset, 3.5, 3.5], fill: stud, radius: 1.75 });
    out.push(#{ rect: [x + offset, y + h - offset - 3.5, 3.5, 3.5], fill: stud, radius: 1.75 });
    out.push(#{ rect: [x + w - offset - 3.5, y + h - offset - 3.5, 3.5, 3.5], fill: stud, radius: 1.75 });

    if st.rose_style == "none" {
        return out;
    }

    // 3. Roses & Vine Wrap Palette
    let rose_mid = st.rose_color;
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
        let vx = x + rad + 8.0;
        let step = 48.0;
        while vx < x + w - rad - 16.0 {
            let seg_w = fmin(step - 14.0, x + w - rad - 16.0 - vx);
            // Vine stem along top edge
            out.push(#{ line: [vx, y + 1.0, vx + seg_w, y + 1.0], width: 2.2, color: vine_dark, soft: 0.4 });

            // Wrapping loop curving over the rounded gray border
            out.push(#{ line: [vx + 6.0, y - 3.5, vx + 9.0, y, vx + 11.5, y + 5.0], width: 3.2, color: vine_mid, soft: 0.5 });
            out.push(#{ line: [vx + 7.0, y - 2.5, vx + 9.5, y + 4.0], width: 1.5, color: vine_hi, soft: 0.4 });

            // Pointed thorn
            out.push(#{ line: [vx + 20.0, y - 0.5, vx + 24.5, y - 5.5], width: 1.6, color: vine_dark, soft: 0.4 });

            // Smooth curved leaf
            out.push(#{ rect: [vx + 28.0, y - 7.0, 10.0, 6.0], radius: 3.0, fill: vine_mid });
            out.push(#{ line: [vx + 26.0, y - 2.0, vx + 37.0, y - 6.0], width: 1.3, color: vine_hi, soft: 0.4 });

            vx += step;
        }

        // Vine climbing down the left rounded border
        let vy = y + rad + 8.0;
        let vstep = 42.0;
        while vy < y + h - rad - 10.0 {
            let seg_h = fmin(vstep - 12.0, y + h - rad - 10.0 - vy);
            out.push(#{ line: [x + 1.0, vy, x + 1.0, vy + seg_h], width: 2.2, color: vine_dark, soft: 0.4 });

            out.push(#{ line: [x - 3.5, vy + 6.0, x, vy + 9.0, x + 5.0, vy + 11.5], width: 3.2, color: vine_mid, soft: 0.5 });
            out.push(#{ line: [x - 2.5, vy + 7.0, x + 4.0, vy + 10.0], width: 1.5, color: vine_hi, soft: 0.4 });

            out.push(#{ line: [x - 0.5, vy + 18.0, x - 5.5, vy + 22.5], width: 1.6, color: vine_dark, soft: 0.4 });

            out.push(#{ rect: [x - 7.0, vy + 26.0, 6.0, 10.0], radius: 3.0, fill: vine_mid });
            out.push(#{ line: [x - 2.0, vy + 24.0, x - 6.0, vy + 35.0], width: 1.3, color: vine_hi, soft: 0.4 });

            vy += vstep;
        }

        // Bottom border vine wrapping toward corner bud
        let bx = x + w - rad - 34.0;
        if bx > x + rad + 24.0 {
            out.push(#{ line: [bx, y + h - 1.5, bx + 20.0, y + h - 1.5], width: 2.2, color: vine_dark, soft: 0.4 });
            out.push(#{ line: [bx + 6.0, y + h - 5.0, bx + 9.5, y + h - 1.0, bx + 12.0, y + h + 3.0], width: 3.0, color: vine_mid, soft: 0.5 });
            out.push(#{ line: [bx + 7.0, y + h - 4.0, bx + 10.5, y + h + 2.0], width: 1.4, color: vine_hi, soft: 0.4 });
        }
    }

    // 5. Blooming High-Resolution Botanical Roses
    if draw_roses {
        // Main Corner Blossom at Top-Left (Scale 1.15 for alert card prominence)
        let rx = x + 6.0;
        let ry = y + 6.0;
        for c in rose_blossom_cmds(rx, ry, 1.15, rose_mid, rose_hi, rose_dark, rose_core, vine_mid, vine_hi, vine_dark) {
            out.push(c);
        }

        // Mini blooming rose along the top border vine
        let mx = x + w * 0.54;
        if mx > x + 100.0 && mx < x + w - 60.0 {
            for c in mini_rose_cmds(mx, y + 1.0, rose_mid, rose_hi, rose_dark, rose_core, vine_mid, vine_hi) {
                out.push(c);
            }
        }

        // High-Resolution Rose Bud at Bottom-Right Corner
        let bx = x + w - 10.0;
        let by = y + h - 10.0;
        for c in rose_bud_cmds(bx, by, 1.15, rose_mid, rose_hi, rose_dark, rose_core, vine_mid, vine_hi, vine_dark) {
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
    out.push(#{ line: [cx - size * 1.15, cy, cx + size * 1.15, cy], width: 3.2, color: c_glow, soft: 2.5 });
    out.push(#{ line: [cx - size, cy, cx + size, cy], width: 1.7, color: c_main, soft: 0.6 });
    out.push(#{ line: [cx - size * 0.45, cy, cx + size * 0.45, cy], width: 1.0, color: c_white });

    // 3. Vertical ray
    out.push(#{ line: [cx, cy - size * 1.15, cx, cy + size * 1.15], width: 3.2, color: c_glow, soft: 2.5 });
    out.push(#{ line: [cx, cy - size, cx, cy + size], width: 1.7, color: c_main, soft: 0.6 });
    out.push(#{ line: [cx, cy - size * 0.45, cx, cy + size * 0.45], width: 1.0, color: c_white });

    // 4. Diagonal sub-glints (for larger stars >= 8px)
    if size >= 8.0 {
        let d = size * 0.35;
        out.push(#{ line: [cx - d, cy - d, cx + d, cy + d], width: 1.2, color: color_alpha(color, 0.70 * alpha), soft: 0.5 });
        out.push(#{ line: [cx - d, cy + d, cx + d, cy - d], width: 1.2, color: color_alpha(color, 0.70 * alpha), soft: 0.5 });
    }

    // 5. Bright center diamond / glint core
    let core = fmax(1.0, size * 0.16);
    out.push(#{ rect: [cx - core, cy - core, core * 2.0, core * 2.0], fill: c_white, radius: core * 0.5 });

    out
}

// Generates an ethereal magic aura, twinkling sparkle stars, and drifting stardust motes
fn magic_dust_cmds(w, h, age, st, seed) {
    let out = [];
    let magic_c = st.magic_color;
    let glow_c = blend_rgb(magic_c, 0xFFFFFFFF, 0.45);
    let rad = num(st.radius);
    if rad <= 0.0 { rad = 16.0; }

    // Overall envelope over 0.85s: fast rise in 0.18s, gentle fade-out over next 0.67s
    let env = 0.0;
    if age < 0.18 {
        env = age / 0.18;
    } else if age < 0.85 {
        env = 1.0 - (age - 0.18) / 0.67;
    }
    if env <= 0.0 { return []; }

    // 1. Auroral boundary glow (soft ambient magic aura surrounding the rose frame)
    let aura_a = 0.25 * env;
    out.push(#{ rect: [-8.0, -8.0, w + 16.0, h + 16.0], fill: color_alpha(magic_c, aura_a), radius: rad + 8.0, soft: 18.0 });
    out.push(#{ rect: [-3.0, -3.0, w + 6.0, h + 6.0], fill: color_alpha(glow_c, aura_a * 0.75), radius: rad + 3.0, soft: 8.0 });

    // 2. Twinkling Sparkle Stars (8 strategic stars around corners, roses, and card edges)
    let stars = [
        [10.0, 8.0, 0.02, 16.0],           // Near top-left botanical rose blossom
        [w * 0.50, -4.0, 0.08, 13.0],      // Along top vine / mini rose
        [w - 14.0, 8.0, 0.04, 15.0],       // Top-right corner rivet
        [w - 10.0, h - 10.0, 0.12, 18.0],  // Bottom-right rose bud
        [18.0, h - 6.0, 0.16, 12.0],       // Bottom-left corner
        [-4.0, h * 0.48, 0.20, 13.0],      // Left vine climb
        [w + 4.0, h * 0.52, 0.14, 11.0],   // Right border edge
        [48.0, -8.0, 0.06, 14.0],          // Floating above alert icon / name
    ];

    for s in stars {
        let sx = s[0];
        let sy = s[1];
        let delay = s[2];
        let base_sz = s[3];
        let s_age = age - delay;
        if s_age > 0.0 && s_age < 0.60 {
            let st_prog = s_age / 0.60;
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

    // 3. Drifting Magic Stardust Particles (26 motes of sparkling dust)
    let s = seed;
    for k in 0..26 {
        let r = rnd(s); s = r[1];
        let rx = r[0];
        r = rnd(s); s = r[1];
        let ry = r[0];
        r = rnd(s); s = r[1];
        let speed = 20.0 + r[0] * 38.0;
        r = rnd(s); s = r[1];
        let p_size = 1.3 + r[0] * 2.5;
        r = rnd(s); s = r[1];
        let sway_speed = 3.0 + r[0] * 4.0;
        r = rnd(s); s = r[1];
        let sway_amp = 8.0 + r[0] * 14.0;

        let start_x = rx * (w + 20.0) - 10.0;
        let start_y = ry * (h + 20.0) - 10.0;

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

