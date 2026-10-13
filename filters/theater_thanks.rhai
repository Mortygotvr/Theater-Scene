// 💖 Theater Thanks: your supporters as end credits, rolling up the screen or
// running along the bottom, with fireworks and hearts. Drawn by Theater: no
// browser needed (the Control Panel's thanks.html, in Rhai).
//
// The names come from the Theater API plugins (supporters()): Twitch
// subscribers, Patreon patrons, Ko-fi supporters. Someone who subs, pledges
// or donates during the stream is added at once (their alert).

const ID = "theater_thanks_source";
const NAME = "💖 Theater Thanks";
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
    }
}

fn properties() {
    [
        info("Your supporters as end credits: Twitch subscribers, Patreon patrons and Ko-fi supporters, from the Theater API plugins. New ones join during the stream. Drawn by Theater: no browser needed."),
        button("test", "💖 Show test names"),
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
        // Patreon's tiers by what they give
        #{ title: title, order: order + rank * 10.0 + fmin(num(s.amount), 999.0) / 100.0 }
    } else {
        #{ title: "", order: 0.0 }
    }
}

// ─── The names ──────────────────────────────────────────────────────────────

fn update() {
    let s = this.settings;
    this.size = [s.width, s.height];
    if none(this.live) { this.live = []; this.test = []; this.pos = 0.0; this.seed = 12345; this.sparks = []; this.hearts = []; this.next_burst = 0.0; this.checked = -10.0; }
    this.key = "";
    this.laid = ();
    this.dirty = true;
    #{}
}

// Someone who supports during the stream joins the credits at once
fn on_event(ev) {
    let a = ev["alert"];
    if type_of(a) != "map" || a.test == true { return; }
    let kind = text_of(a.kind);
    if !["sub", "resub", "gift", "pledge", "pledge_change", "donation", "shop_order", "commission", "member", "superchat"].contains(kind) { return; }
    let name = text_of(a.user);
    if name == "" || name == "Someone" || name == "Anonymous" || a.anonymous == true { return; }
    let tier = text_of(a.tier);
    if kind == "member" || kind == "superchat" { tier = "Members"; }
    this.live.push(#{ name: name, platform: text_of(a.platform), tier: tier, amount: num(a.amount), gift: false });
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
    let s = this.settings;
    let w = num(s.width);
    let h = num(s.height);
    if none(this.live) { this.update(); }

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
    let bg = s.bg;
    if horizontal {
        cmds.push(#{ rect: [0.0, h - bar_h, w, bar_h], fill: bg });
        cmds.push(#{ rect: [0.0, h - bar_h, w, 1.0], fill: rgba(255, 255, 255, 0.06) });
    } else {
        cmds.push(#{ rect: [0.0, 0.0, w, h], fill: bg });
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
    cmds
}
