// ⚕ Dead Space RIG: Isaac's health and stasis on stream, from the ds-vr VR
// mod, in the look of the game's holograms (blue lines that glow), with a
// flash and a number on each hit or heal. Drawn by Theater Scene.
//
// The mod sends its game data to the Theater relay (Theater Reader's
// WebSocket, ws://127.0.0.1:41837) as messages typed deadspace_*:
//   #{ type: "deadspace_state", health, health_max, stasis, stasis_max, action, game_camera, in_game }
//     (when something changes, and once a second)
//   #{ type: "deadspace_damage", amount, health } / "deadspace_heal"
//   #{ type: "deadspace_takeover_start", action } / "deadspace_takeover_end"
//     (the game's camera took the view: grabs, scripted moments)
// Or straight from the game, at an address of your own (the Address
// setting: "192.168.1.20" means ws://192.168.1.20:47601, the mod's feed):
// for a game on another PC (its "Game feed open to the network" on), or
// without Theater Reader. Its messages are the same, typed without the
// deadspace_ prefix.
// It ships with Theater Scene ("⚕ Dead Space RIG" in OBS's list of
// sources). A newer copy from the mod's package goes in Theater Scene's
// filters folder (%APPDATA%\obs-studio\plugin_config\theater-scene\
// filters\), then restart OBS once. Listening to an address needs Theater
// Scene with this.feed (2026-10-15 or later); the relay works with any.

const ID = "deadspace_rig_source";
const NAME = "⚕ Dead Space RIG";
const KIND = "source";

// ─── Settings ───────────────────────────────────────────────────────────────

fn defaults() {
    #{
        width: 520, height: 180,
        address: "",
        title: "RIG",
        segments: 12,
        show_stasis: true, show_numbers: true, show_hits: true,
        hide_offline: false, panel: true, glow: true, scanlines: true,
        font: "Roboto Mono",
        color: rgba(77, 184, 255, 1.0),
        warn_color: rgba(255, 158, 38, 1.0),
        danger_color: rgba(255, 56, 46, 1.0),
        heal_color: rgba(90, 255, 160, 1.0),
        panel_color: rgba(0, 12, 24, 0.45),
    }
}

fn properties() {
    [
        info("Isaac's health and stasis from the ds-vr VR mod. Shows \"RIG OFFLINE\" while the game isn't sending."),
        text("address", "Game address (empty: through Theater Reader's relay; else the game PC's IP, or ws://host:port)"),
        info("Another PC: turn on \"Game feed open to the network\" in the mod's console, restart the game, and put that PC's IP here."),
        button("test_hit", "💥 Test a hit"),
        button("test_heal", "💚 Test a heal"),
        int_slider("width", "Width", 200, 1920, 10),
        int_slider("height", "Height", 80, 1080, 10),
        text("title", "Title"),
        int_slider("segments", "Health segments", 4, 24, 1),
        toggle("show_stasis", "Stasis bar"),
        toggle("show_numbers", "Percentages"),
        toggle("show_hits", "Hit and heal numbers"),
        toggle("hide_offline", "Hide while the game isn't sending"),
        toggle("panel", "Dark panel behind"),
        toggle("glow", "Glow"),
        toggle("scanlines", "Scan lines"),
        font_list("font", "Font"),
        color("color", "Hologram colour"),
        color("warn_color", "Health colour, below 60%"),
        color("danger_color", "Health colour, below 30% (and hits)"),
        color("heal_color", "Heal colour"),
        color("panel_color", "Panel colour"),
    ]
}

// ─── Helpers ────────────────────────────────────────────────────────────────

fn fmax(a, b) { if a > b { a } else { b } }
fn fmin(a, b) { if a < b { a } else { b } }
fn none(v) { type_of(v) == "()" }
fn num(v) {
    if type_of(v) == "i64" { v.to_float() }
    else if type_of(v) == "f64" { v }
    else { 0.0 }
}
fn field(m, k) { if type_of(m) == "map" { m[k] } else { () } }
// A colour (0xAARRGGBB) with its alpha multiplied by `a`
fn fade(c, a) {
    let alpha = ((c >> 24) & 0xFF).to_float() * fmax(0.0, fmin(1.0, a));
    (c & 0xFFFFFF) | (alpha.to_int() << 24)
}
fn ease_out(t) { let u = 1.0 - t; 1.0 - u * u * u }

// ─── Events ─────────────────────────────────────────────────────────────────

// The address typed, as a WebSocket URL: "" (the relay), an IP or name
// (the mod's port, 47601), or a full ws:// URL
fn feed_url(address) {
    if type_of(address) != "string" { return ""; }
    let a = address;
    a.trim();
    if a == "" { return ""; }
    if !a.starts_with("ws://") && !a.starts_with("wss://") { a = "ws://" + a; }
    // No port after the host: the mod's
    let host = a.sub_string(a.index_of("//") + 2);
    if !host.contains(":") { a += ":47601"; }
    a
}

fn update() {
    let s = this.settings;
    this.size = [s.width, s.height];
    // Straight from the game when an address is set (Theater listens to it)
    this.feed = feed_url(s.address);
    if none(this.hits) { this.hits = []; }
    #{}
}

fn add_hit(kind, amount) {
    this.hits.push(#{ kind: kind, amount: amount, at: this.time });
    while this.hits.len() > 8 { this.hits.remove(0); }
}

fn on_event(ev) {
    let t = field(ev, "type");
    if type_of(t) != "string" { return; }
    // Through the relay: typed deadspace_*; straight from the game (when
    // listening to its address): the same, without the prefix
    if t.starts_with("deadspace_") {
        t = t.sub_string(10);
    } else if this.feed == "" || !["state", "damage", "heal", "takeover_start", "takeover_end"].contains(t) {
        return;
    }
    this.seen = this.time;
    if t == "state" {
        this.health = ev.health;
        this.health_max = ev.health_max;
        this.stasis = ev.stasis;
        this.stasis_max = ev.stasis_max;
        this.in_game = ev.in_game == true;
        this.taken = ev.game_camera == true;
    } else if t == "damage" {
        this.add_hit("damage", num(ev.amount));
    } else if t == "heal" {
        this.add_hit("heal", num(ev.amount));
    } else if t == "takeover_start" {
        this.taken = true;
    } else if t == "takeover_end" {
        this.taken = false;
    }
}

fn on_button(key) {
    // A pretend state first, if nothing came yet
    if none(this.health) {
        this.health = 75.0; this.health_max = 100.0; this.stasis = 60.0; this.stasis_max = 100.0;
        this.in_game = true;
    }
    this.seen = this.time;
    if key == "test_hit" {
        let hit = 15.0;
        this.health = fmax(0.0, num(this.health) - hit);
        this.add_hit("damage", hit);
    } else if key == "test_heal" {
        let heal = 25.0;
        this.health = fmin(num(this.health_max), num(this.health) + heal);
        this.add_hit("heal", heal);
    }
}

// ─── Drawing ────────────────────────────────────────────────────────────────

fn tick(dt) {
    let s = this.settings;
    let w = num(this.size[0]);
    let h = num(this.size[1]);
    let now = this.time;
    let font = s.font;
    let blue = s.color;
    let online = !none(this.seen) && now - this.seen < 3.0 && this.in_game == true
        && !none(this.health) && num(this.health_max) > 0.0;
    if !online && s.hide_offline { return []; }

    let out = [];
    if s.panel {
        out.push(#{ rect: [0.0, 0.0, w, h], fill: s.panel_color, radius: 6.0 });
    }

    // Layout, from the height
    let pad = h * 0.09;
    let title_size = h * 0.13;
    let label_size = h * 0.10;
    let row_h = h * 0.20;
    let flicker = 0.93 + 0.07 * (now * 47.0).sin() * (now * 13.0).sin();

    // What glows: drawn twice, sharp and soft
    let glowing = [];

    // Corner brackets
    let arm = fmin(w, h) * 0.16;
    let t = 2.0;
    for c in [[0.0, 0.0, 1.0, 1.0], [w, 0.0, -1.0, 1.0], [0.0, h, 1.0, -1.0], [w, h, -1.0, -1.0]] {
        let x = c[0]; let y = c[1]; let dx = c[2]; let dy = c[3];
        let hx = if dx > 0.0 { x } else { x - arm };
        let vy = if dy > 0.0 { y } else { y - arm };
        let hy = if dy > 0.0 { y } else { y - t };
        let vx = if dx > 0.0 { x } else { x - t };
        glowing.push(#{ rect: [hx, hy, arm, t], fill: blue });
        glowing.push(#{ rect: [vx, vy, t, arm], fill: blue });
    }

    // Title, with a line after it
    let title = if type_of(s.title) == "string" { s.title } else { "RIG" };
    glowing.push(#{ text: title, x: pad, y: pad, font: font, size: title_size, weight: 700, color: blue });
    let line_x = pad + title_size * (title.len().to_float() * 0.7 + 1.0);
    glowing.push(#{ rect: [line_x, pad + title_size * 0.55, fmax(0.0, w - pad - line_x), 1.5], fill: fade(blue, 0.5) });

    if !online {
        glowing.push(#{ text: "RIG OFFLINE", x: pad, y: h * 0.45, font: font, size: label_size * 1.4, weight: 700,
                        color: fade(blue, 0.45 + 0.25 * (now * 2.0).sin().abs()) });
        // Where it listens, and whether that's connected
        let where_ = if this.feed == "" { "via Theater relay" + (if relay_connected() { "" } else { " (no relay)" }) }
                     else { this.feed + (if feed_connected(this.feed) { "" } else { " (not reachable)" }) };
        glowing.push(#{ text: where_, x: pad, y: h * 0.45 + label_size * 2.0, font: font, size: label_size * 0.8,
                        weight: 500, color: fade(blue, 0.5) });
        return out + draw_glowing(glowing, s, flicker, w, h, now);
    }

    // Vitals: a row of segments, coloured by how much is left
    let health = num(this.health);
    let frac = fmax(0.0, fmin(1.0, health / num(this.health_max)));
    let colour = if frac > 0.6 { blue } else if frac > 0.3 { s.warn_color } else { s.danger_color };
    let pulse = if frac <= 0.3 { 0.55 + 0.45 * (now * 6.0).sin().abs() } else { 1.0 };
    let y_vitals = pad + title_size + h * 0.08;
    glowing.push(#{ text: "VITALS", x: pad, y: y_vitals, font: font, size: label_size, weight: 600, color: blue });
    if s.show_numbers {
        let pct = (frac * 100.0).round().to_int().to_string() + "%";
        let m = measure_text(pct, font, label_size, 600);
        glowing.push(#{ text: pct, x: w - pad - num(m[0]), y: y_vitals, font: font, size: label_size, weight: 600, color: colour });
    }
    let n = fmax(1.0, num(s.segments));
    let gap = fmax(2.0, w * 0.008);
    let seg_y = y_vitals + label_size * 1.4;
    let seg_w = (w - pad * 2.0 - gap * (n - 1.0)) / n;
    let lit = (frac * n).ceiling();
    let i = 0.0;
    while i < n {
        let x = pad + i * (seg_w + gap);
        if i < lit {
            glowing.push(#{ rect: [x, seg_y, seg_w, row_h], fill: fade(colour, pulse) });
        } else {
            glowing.push(#{ rect: [x, seg_y, seg_w, row_h], fill: 0, border: 1.0, border_color: fade(blue, 0.3) });
        }
        i += 1.0;
    }

    // Stasis: a bar
    if s.show_stasis && num(this.stasis_max) > 0.0 {
        let sfrac = fmax(0.0, fmin(1.0, num(this.stasis) / num(this.stasis_max)));
        let y_stasis = seg_y + row_h + h * 0.07;
        glowing.push(#{ text: "STASIS", x: pad, y: y_stasis, font: font, size: label_size, weight: 600, color: blue });
        let bar_x = pad + label_size * 5.0;
        let bar_w = fmax(10.0, w - pad - bar_x - (if s.show_numbers { label_size * 3.5 } else { 0.0 }));
        let bar_h = label_size * 0.8;
        let bar_y = y_stasis + label_size * 0.15;
        glowing.push(#{ rect: [bar_x, bar_y, bar_w, bar_h], fill: 0, border: 1.5, border_color: blue });
        if sfrac > 0.0 {
            glowing.push(#{ rect: [bar_x + 3.0, bar_y + 3.0, fmax(0.0, (bar_w - 6.0) * sfrac), fmax(1.0, bar_h - 6.0)], fill: blue });
        }
        if s.show_numbers {
            let pct = (sfrac * 100.0).round().to_int().to_string() + "%";
            let m = measure_text(pct, font, label_size, 600);
            glowing.push(#{ text: pct, x: w - pad - num(m[0]), y: y_stasis, font: font, size: label_size, weight: 600, color: blue });
        }
    }

    // The game's camera has the view (a grab, a scripted moment): the frame
    // pulses
    if this.taken == true {
        let a = 0.35 + 0.35 * (now * 8.0).sin().abs();
        glowing.push(#{ rect: [1.0, 1.0, w - 2.0, h - 2.0], fill: 0, border: 2.0, border_color: fade(s.danger_color, a), radius: 6.0 });
    }

    out = out + draw_glowing(glowing, s, flicker, w, h, now);

    // Hits and heals: a flash over the panel, and the number floating up
    let keep = [];
    for hit in this.hits {
        let age = now - hit.at;
        if age > 1.6 { continue; }
        keep.push(hit);
        let c = if hit.kind == "damage" { s.danger_color } else { s.heal_color };
        if age < 0.5 {
            out.push(#{ rect: [0.0, 0.0, w, h], fill: fade(c, 0.35 * (1.0 - age / 0.5)), radius: 6.0 });
        }
        if s.show_hits && hit.amount >= 0.5 {
            let t = age / 1.6;
            let sign = if hit.kind == "damage" { "-" } else { "+" };
            let label = sign + hit.amount.round().to_int().to_string();
            out.push(#{ text: label, x: w * 0.62, y: seg_y - h * 0.18 * ease_out(t), font: font,
                        size: label_size * 1.6, weight: 800, color: fade(c, 1.0 - t), shadow: true });
        }
    }
    this.hits = keep;
    out
}

// The hologram lines: a soft glow under (added light), the sharp lines,
// scan lines over, all flickering a little
fn draw_glowing(glowing, s, flicker, w, h, now) {
    let out = [];
    if s.glow {
        let soft = [];
        for c in glowing {
            let g = c;
            if !none(field(g, "rect")) { g.soft = 6.0; }
            soft.push(g);
        }
        out.push(#{ group: soft, x: 0.0, y: 0.0, alpha: 0.45 * flicker, blend: "add" });
    }
    out.push(#{ group: glowing, x: 0.0, y: 0.0, alpha: flicker });
    if s.scanlines {
        let lines = [];
        let y = 0.0;
        let scroll = (now * 8.0) % 3.0;
        while y < h {
            lines.push(#{ rect: [0.0, y + scroll, w, 1.0], fill: rgba(0, 0, 0, 0.18) });
            y += 3.0;
        }
        out.push(#{ group: lines, x: 0.0, y: 0.0, clip: [0.0, 0.0, w, h] });
    }
    out
}
