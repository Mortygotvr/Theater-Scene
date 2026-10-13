// legacy_thanks.rhai — 💖 Legacy Thanks: a copy of 💖 Theater Thanks
// (theater_thanks.rhai), its own source: your supporters as end credits,
// with a banner for whoever supports during the stream that comes in with
// the light bar + lightning entrance (💬 Legacy Chat's), and looking like the
// 💬 Legacy Chat when asked. Drawn by Theater: no browser needed.
//
// The names come from the Theater API plugins (supporters()): Twitch
// subscribers, Patreon patrons, Ko-fi supporters. Someone who subs, pledges
// or donates during the stream gets the banner, then joins the credits.

const ID = "legacy_thanks_source";
const NAME = "💖 Legacy Thanks";
const KIND = "source";

// ─── Settings ───────────────────────────────────────────────────────────────

fn defaults() {
    #{
        width: 1920, height: 1080,
        title: "Legendary Supporters", subtitle: "Thank you for making the Theatre possible",
        include_twitch: true, include_patreon: true, include_kofi: true,
        grouping: "platform", direction: "vertical", speed: 60.0, badges: true,
        font: "Outfit", title_size: 56, name_size: 32, spacing: 12,
        title_color: rgba(0, 255, 204, 1.0), subtitle_color: rgba(255, 255, 255, 0.65),
        group_color: rgba(0, 255, 204, 1.0), name_color: rgba(255, 255, 255, 1.0),
        bg: rgba(5, 5, 5, 1.0), background: "both",
        banner: true, banner_text: "Thank you, {name}!", banner_seconds: 5.0, banner_bg: rgba(12, 12, 20, 0.88),
        sweep: 0.9, fx_lightning: true, fx_mode: "platform", fx_color: rgba(120, 200, 255, 1.0),
        match_chat: false,
    }
}

fn properties() {
    [
        info("Your supporters as end credits: Twitch subscribers, Patreon patrons and Ko-fi supporters, from the Theater API plugins. Whoever supports during the stream gets a banner (light bar + lightning), then joins the credits. Drawn by Theater: no browser needed."),
        button("test", "💖 Show test names and a banner"),
        button("restart", "⟲ Start the roll again"),
        text("title", "Title"),
        text("subtitle", "Subtitle"),
        toggle("include_twitch", "Twitch subscribers (Theater API · Twitch)"),
        toggle("include_patreon", "Patreon patrons (Theater API · Patreon)"),
        toggle("include_kofi", "Ko-fi supporters (Theater API · Ko-fi)"),
        choice("grouping", "Group", [["By platform", "platform"], ["By tier", "tier"], ["One list", "none"]]),
        choice("direction", "Roll", [["Up the screen (credits)", "vertical"], ["Along the bottom (ticker)", "horizontal"]]),
        slider("speed", "Speed (pixels a second)", 10.0, 400.0, 5.0),
        toggle("badges", "Platform badges"),
        info("A new supporter's banner"),
        toggle("banner", "Show a banner when someone supports during the stream"),
        text("banner_text", "Banner text ({name}: who)"),
        slider("banner_seconds", "Banner seconds", 2.0, 15.0, 0.5),
        color("banner_bg", "Banner colour"),
        slider("sweep", "Light bar sweep (seconds)", 0.3, 3.0, 0.1),
        toggle("fx_lightning", "Lightning across it as it comes"),
        choice("fx_mode", "Light colour", [["The platform's colour", "platform"], ["One colour", "custom"]]),
        color("fx_color", "Light colour (One colour)"),
        toggle("match_chat", "Look like the 💬 Legacy Chat (its font and colours; follows its changes)"),
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
        color("bg", "Background colour (its alpha: how see-through)"),
        choice("background", "Animation", [["Fireworks and hearts", "both"], ["Fireworks", "fireworks"], ["Hearts", "hearts"], ["None", "none"]]),
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

// The settings with the 💬 Legacy Chat's look over them, when asked
fn look_of(s, c) {
    if s.match_chat != true || !is_map(c) { return s; }
    let m = s;
    let font = text_of(c.font);
    if font != "" { m.font = font; }
    m.name_color = c.text;
    if text_of(c.name_mode) == "custom" { m.group_color = c.name; m.title_color = c.name; }
    m.banner_bg = if text_of(c.bg_mode) == "transparent" { 0 } else { c.bg };
    m
}

// ─── The names ──────────────────────────────────────────────────────────────

fn update() {
    let s = this.settings;
    this.size = [s.width, s.height];
    if none(this.live) {
        this.live = []; this.test = []; this.pos = 0.0; this.seed = 12345; this.sparks = []; this.hearts = []; this.next_burst = 0.0; this.checked = -10.0;
        this.banners = []; this.banner = (); this.look = (); this.look_checked = -10.0;
    }
    if s.match_chat == true { this.look = shared_value("legacy_chat_look"); }
    this.key = "";
    this.laid = ();
    this.dirty = true;
    #{}
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
            #{ name: "StreamFan42", platform: "twitch", tier: "Tier 1", amount: 0.0, gift: false },
            #{ name: "KappaKing", platform: "twitch", tier: "Tier 3", amount: 0.0, gift: false },
            #{ name: "NightOwl", platform: "twitch", tier: "Tier 1", amount: 0.0, gift: true },
            #{ name: "PixelPal", platform: "twitch", tier: "Tier 2", amount: 0.0, gift: false },
            #{ name: "GoldenPatron", platform: "patreon", tier: "Gold", amount: 10.0, gift: false },
            #{ name: "SilverLining", platform: "patreon", tier: "Silver", amount: 5.0, gift: false },
            #{ name: "CoffeeLover99", platform: "kofi", tier: "", amount: 5.0, gift: false },
        ];
        this.banners.push(#{ name: "GoldenPatron", platform: "patreon", tier: "Gold", amount: 10.0, gift: false });
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

fn badge_cmds(p, x, y, size, font) {
    let label = platform_name(p.platform).to_upper();
    let bs = size * 0.38;
    let m = measure_text(label, font, bs, 800);
    let bw = m[0] + bs * 0.9;
    let bh = bs * 1.7;
    let c = platform_color(p.platform);
    #{
        w: bw,
        cmds: [
            #{ rect: [x, y + (size * 1.25 - bh) / 2.0, bw, bh], fill: with_alpha(c, 0.10), radius: 4.0, border: 1.0, border_color: with_alpha(c, 0.35) },
            #{ text: label, x: x + bs * 0.45, y: y + (size * 1.25 - m[1]) / 2.0, font: font, size: bs, weight: 800, color: c },
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
        rows.push(#{ y: y, h: ts * 1.3, cmds: [centered.call(s.title, ts, 800, s.title_color)] });
        y += ts * 1.4;
    }
    if text_of(s.subtitle) != "" {
        rows.push(#{ y: y, h: ts * 0.5, cmds: [centered.call(s.subtitle, ts * 0.4, 400, s.subtitle_color)] });
        y += ts * 0.4 * 1.3;
    }
    y += ts * 1.2;
    for g in groups {
        if g.title != "" {
            rows.push(#{ y: y, h: ns * 1.3, cmds: [centered.call(g.title.to_upper(), ns * 0.85, 700, s.group_color)] });
            y += ns * 1.3 + gap;
        }
        for p in g.names {
            let m = measure_text(p.name, font, ns, 600);
            let cmds = [];
            if s.badges != false {
                let b = badge_cmds(p, 0.0, 0.0, ns, font);
                let total = b.w + 10.0 + m[0];
                let x = (w - total) / 2.0;
                for c in b.cmds {
                    if !none(c.rect) { c.rect[0] += x; } else { c.x += x; }
                    cmds.push(c);
                }
                cmds.push(#{ text: p.name, x: x + b.w + 10.0, y: (ns * 1.25 - m[1]) / 2.0, font: font, size: ns, weight: 600, color: s.name_color, shadow: true });
            } else {
                cmds.push(#{ text: p.name, x: (w - m[0]) / 2.0, y: (ns * 1.25 - m[1]) / 2.0, font: font, size: ns, weight: 600, color: s.name_color, shadow: true });
            }
            rows.push(#{ y: y, h: ns * 1.25, cmds: cmds });
            y += ns * 1.25 + gap;
        }
        y += ns * 1.4;
    }
    if groups.len() == 0 {
        rows.push(#{ y: y, h: ns * 1.3, cmds: [centered.call("Seeking legends…", ns * 0.7, 600, with_alpha(s.name_color, 0.6))] });
        y += ns * 1.3;
    }
    #{ rows: rows, h: y }
}

// The ticker, left to right: [#{ x, w, cmds }] and its whole width
fn layout_horizontal(s, groups, bar_h) {
    let font = text_of(s.font);
    let ns = num(s.name_size) * 0.75;
    let gap = fmax(num(s.spacing), 8.0) * 1.5;
    let items = [];
    let x = 0.0;
    let y_of = |size| (bar_h - size * 1.25) / 2.0;
    let add = |t, size, weight, color| {
        let m = measure_text(t, font, size, weight);
        #{ x: 0.0, w: m[0], cmds: [#{ text: t, x: 0.0, y: (bar_h - m[1]) / 2.0, font: font, size: size, weight: weight, color: color, shadow: true }] }
    };
    if text_of(s.title) != "" {
        let it = add.call(s.title, ns * 1.1, 800, s.title_color);
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
            let m = measure_text(p.name, font, ns, 600);
            cmds.push(#{ text: p.name, x: w, y: (bar_h - m[1]) / 2.0, font: font, size: ns, weight: 600, color: s.name_color, shadow: true });
            w += m[0];
            items.push(#{ x: x, w: w, cmds: cmds });
            x += w;
            i += 1;
            if i < g.names.len() {
                let dot = add.call("•", ns, 600, with_alpha(s.name_color, 0.25));
                dot.x = x + gap / 2.0 - dot.w / 2.0;
                items.push(dot);
                x += gap;
            }
        }
        x += gap * 2.0;
    }
    #{ items: items, w: x }
}

// A new supporter's banner: "Thank you, NAME!" and where they support
fn banner_cmds(b, s, max_w) {
    let font = text_of(s.font);
    let size = num(s.name_size) * 1.1;
    let line = text_of(s.banner_text);
    line.replace("{name}", b.name);
    let detail = platform_name(b.platform) + (if text_of(b.tier) != "" { " · " + b.tier } else { "" });
    let m1 = measure_text(line, font, size, 800);
    let m2 = measure_text(detail, font, size * 0.6, 500);
    let pad = size * 0.8;
    let cw = fmin(fmax(m1[0], m2[0]) + pad * 2.0 + 8.0, max_w);
    let ch = m1[1] + m2[1] + pad * 1.5;
    let color = platform_color(b.platform);
    #{
        w: cw, h: ch, color: color,
        cmds: [
            #{ rect: [0.0, 0.0, cw, ch], fill: s.banner_bg, radius: 12.0 },
            #{ rect: [0.0, 0.0, 6.0, ch], fill: color, radius: 3.0 },
            #{ text: line, x: pad + 6.0, y: pad * 0.6, font: font, size: size, weight: 800, color: s.name_color, shadow: true },
            #{ text: detail, x: pad + 6.0, y: pad * 0.6 + m1[1] + pad * 0.25, font: font, size: size * 0.6, weight: 500, color: color },
        ]
    }
}

// ─── Fireworks and hearts ───────────────────────────────────────────────────

fn rand() {
    this.seed = (this.seed * 1103515245 + 12345) % 2147483648;
    this.seed.to_float() / 2147483648.0
}

fn burst(w, h) {
    let colors = [rgba(0, 255, 204, 1.0), rgba(157, 78, 221, 1.0), rgba(255, 214, 10, 1.0), rgba(255, 92, 138, 1.0), rgba(80, 160, 255, 1.0)];
    let c = colors[(this.rand() * 5.0).to_int() % 5];
    let cx = w * (0.15 + this.rand() * 0.7);
    let cy = h * (0.12 + this.rand() * 0.4);
    let n = 26;
    for i in 0..n {
        let angle = i.to_float() / n.to_float() * 6.2832 + this.rand() * 0.3;
        let speed = 60.0 + this.rand() * 140.0;
        this.sparks.push(#{ x: cx, y: cy, vx: angle.cos() * speed, vy: angle.sin() * speed, life: 1.4 + this.rand() * 0.8, age: 0.0, c: c });
    }
}

fn effects(dt, w, h, kind) {
    let cmds = [];
    if kind == "fireworks" || kind == "both" {
        if this.time >= this.next_burst {
            if this.sparks.len() < 160 { this.burst(w, h); }
            this.next_burst = this.time + 0.9 + this.rand() * 1.4;
        }
        let keep = [];
        for p in this.sparks {
            p.age += dt;
            if p.age < p.life {
                p.vy += 70.0 * dt;
                p.vx *= 0.985;
                p.x += p.vx * dt;
                p.y += p.vy * dt;
                keep.push(p);
                let a = 1.0 - p.age / p.life;
                cmds.push(#{ rect: [p.x - 2.0, p.y - 2.0, 4.0, 4.0], fill: with_alpha(p.c, a), radius: 2.0, soft: 6.0 });
            }
        }
        this.sparks = keep;
    }
    if kind == "hearts" || kind == "both" {
        while this.hearts.len() < 18 {
            this.hearts.push(#{ x: this.rand() * w, y: h + this.rand() * h, s: 14.0 + this.rand() * 26.0, v: 25.0 + this.rand() * 45.0, phase: this.rand() * 6.28 });
        }
        // (A for loop's items are copies: the moved hearts make a new list)
        let moved = [];
        for p in this.hearts {
            p.y -= p.v * dt;
            if p.y < -60.0 { p.y = h + 40.0; p.x = this.rand() * w; }
            let sway = (this.time * 1.2 + p.phase).sin() * 14.0;
            let a = fmin(1.0, fmax(0.0, (p.y + 60.0) / (h * 0.5))) * 0.35;
            cmds.push(#{ text: "♥", x: p.x + sway, y: p.y, font: "Outfit", size: p.s, weight: 600, color: rgba(255, 92, 138, a) });
            moved.push(p);
        }
        this.hearts = moved;
    }
    [#{ group: cmds, x: 0.0, y: 0.0, blend: "add" }]
}

// ─── Each frame ─────────────────────────────────────────────────────────────

fn tick(dt) {
    if none(this.live) { this.update(); }
    // Looking like the chat: its changes followed (checked every second)
    if this.settings.match_chat == true && this.time - this.look_checked >= 1.0 {
        this.look_checked = this.time;
        let c = shared_value("legacy_chat_look");
        if is_map(c) && (!is_map(this.look) || c.stamp != this.look.stamp) {
            this.look = c;
            this.dirty = true;
        }
    }
    let s = look_of(this.settings, this.look);
    let w = num(s.width);
    let h = num(s.height);
    let horizontal = s.direction == "horizontal";
    let bar_h = num(s.name_size) * 1.9;
    // The plugins' lists, looked at every few seconds (or now, when the
    // settings or the names changed); laid out again only when what's
    // shown changes
    if this.time - this.checked > 3.0 || this.dirty != false || none(this.laid) {
        this.checked = this.time;
        this.dirty = false;
        this.from_plugins = supporters();
        let people = everyone(s, this.from_plugins, this.live, this.test);
        let key = "";
        for v in [w, h, s.grouping, s.direction, s.font, s.name_size, s.title_size, s.spacing, s.badges, s.title, s.subtitle, s.title_color, s.subtitle_color, s.name_color, s.group_color] {
            key += "|" + v;
        }
        for p in people { key += "|" + p.name; }
        if key != this.key || none(this.laid) {
            let groups = grouped(people, text_of(s.grouping));
            this.laid = if horizontal { layout_horizontal(s, groups, bar_h) } else { layout_vertical(s, groups, w) };
            this.key = key;
        }
    }

    let cmds = [];
    if horizontal {
        cmds.push(#{ rect: [0.0, h - bar_h, w, bar_h], fill: s.bg });
        cmds.push(#{ rect: [0.0, h - bar_h, w, 1.0], fill: rgba(255, 255, 255, 0.06) });
    } else {
        cmds.push(#{ rect: [0.0, 0.0, w, h], fill: s.bg });
    }
    for e in this.effects(dt, w, if horizontal { h - bar_h } else { h }, text_of(s.background)) { cmds.push(e); }

    // The roll: from below the screen (or off its right side) to past its other edge, again and again
    this.pos += num(s.speed) * dt;
    if horizontal {
        let total = this.laid.w;
        if this.pos > w + total { this.pos = 0.0; }
        let x0 = w - this.pos;
        let ticker = [];
        for it in this.laid.items {
            let x = x0 + it.x;
            if x + it.w >= 0.0 && x <= w {
                ticker.push(#{ group: it.cmds, x: x, y: 0.0 });
            }
        }
        cmds.push(#{ group: ticker, x: 0.0, y: h - bar_h, clip: [0.0, 0.0, w, bar_h] });
    } else {
        let total = this.laid.h;
        if this.pos > h + total { this.pos = 0.0; }
        let y0 = h - this.pos;
        for r in this.laid.rows {
            let y = y0 + r.y;
            if y + r.h >= 0.0 && y <= h {
                cmds.push(#{ group: r.cmds, x: 0.0, y: y });
            }
        }
    }

    // A new supporter's banner, one at a time: the light bar shows it, the
    // lightning strikes across it, then it fades
    if s.banner != false {
        if none(this.banner) && this.banners.len() > 0 {
            let b = this.banners.remove(0);
            b.start = this.time;
            this.banner = b;
        }
        if !none(this.banner) {
            let b = this.banner;
            let age = this.time - b.start;
            let sweep = fmax(0.2, num(s.sweep));
            let total = fmax(sweep + 0.6, num(s.banner_seconds));
            if age > total {
                this.banner = ();
            } else {
                let laid = banner_cmds(b, s, w * 0.9);
                let bx = (w - laid.w) / 2.0;
                let by = if horizontal { h - bar_h - laid.h - 16.0 } else { h * 0.08 };
                let color = if s.fx_mode == "custom" { s.fx_color } else { laid.color };
                let fade = fmin(1.0, (total - age) / 0.5);
                let st = #{ sweep: sweep, fx_lightning: s.fx_lightning != false };
                let seed = (b.start * 1000.0).floor().to_int() % 100000 + 1;
                cmds.push(#{ group: bloom_parts(laid.cmds, laid.w, laid.h, age, st, color, seed), x: bx, y: by, alpha: fade });
            }
        }
    }
    cmds
}

// ─── The light bar + lightning entrance (Legacy Alert's) ────────────────────
// 💬 Legacy Chat's: a new card is shown from a glowing bar that sweeps
// across it right to left (what the bar has passed is shown), while a
// lightning bolt runs across it, right to left at mid-height, the moment it
// comes, crackles and fades.

// A number 0..1 from `seed`, and the next seed: the same alert always
// strikes the same way
fn rnd(seed) {
    let next = (seed * 1103515245 + 12345) % 2147483648;
    [next.to_float() / 2147483648.0, next]
}

fn ease_in_out(t) {
    if t < 0.5 { 4.0 * t * t * t } else { let u = 2.0 - 2.0 * t; 1.0 - u * u * u / 2.0 }
}

// How long the entrance lasts (the bar, then its fading)
fn bloom_length(st) { st.sweep + 0.25 }

// The lightning bolt across the card (`w` × `h`), `age` seconds in: from its
// right end to its left end, jagging up and down at mid-height, with forks,
// and a flash at each end. A new shape every few frames: it crackles.
fn bolt_cmds(w, h, age, color, seed) {
    let s = seed * 7919 + (age * 18.0).floor().to_int() + 1;
    let r = rnd(s); s = r[1];
    let mid_y = h * 0.5;
    // How far it may jag from the middle
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

// The card's drawing (`cmds`, `w` × `h`) `age` seconds after it came: shown
// from the bar rightwards, the bar and its glow, the lightning
fn bloom_parts(cmds, w, h, age, st, color, seed) {
    let t = fmin(1.0, age / st.sweep);
    let bar_x = (w + 6.0) - (w + 12.0) * ease_in_out(t);
    let parts = [];
    if t < 1.0 {
        // Only what the bar has passed (room around it for shadows)
        parts.push(#{ group: cmds, clip: [bar_x, -60.0, w + 60.0 - bar_x, h + 120.0] });
    } else {
        parts.push(#{ group: cmds });
    }
    // The bar, its glow, and a light wake on what it just showed; through,
    // it fades
    let bar_alpha = if age < st.sweep { 1.0 } else { 1.0 - (age - st.sweep) / 0.25 };
    if bar_alpha > 0.0 {
        let top = -10.0;
        let bh = h + 20.0;
        parts.push(#{ group: [
            #{ rect: [bar_x, 0.0, fmin(48.0, w + 6.0 - bar_x), h], fill: with_alpha(color, 0.22), radius: 8.0, soft: 20.0 },
            #{ rect: [bar_x - 2.0, top, 4.0, bh], fill: with_alpha(color, 0.35), radius: 2.0, soft: 40.0 },
            #{ rect: [bar_x - 2.0, top, 4.0, bh], fill: with_alpha(color, 0.9), radius: 2.0, soft: 12.0 },
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
