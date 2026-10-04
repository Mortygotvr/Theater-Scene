// legacy_alert.rhai — 🔔 Legacy Alert: a copy of 🔔 Theater Alerts
// (theater_alerts.rhai), its own source, with the light bar + lightning
// entrance, and looking like the 💬 Legacy Chat when asked.
//
// What Theater Reader announces (subs, cheers, raids,
// redeems, Super Chats, members…) as a card, one at a time, in the chat's
// themes, with a sound per kind. Drawn by Theater: no browser needed.
//
// Theater Reader sends #{ type: "alert", source, event, trigger, data }:
// `event` says what happened ("Twitch sub", "Kick follow", "paid_message"…),
// `data` holds the rest (username, bits, viewers, reward_title, amount…).

const ID = "legacy_alert_source";
const NAME = "🔔 Legacy Alert";
const KIND = "source";

// ─── Settings ───────────────────────────────────────────────────────────────

fn defaults() {
    #{
        preset: "glass", applied_preset: "", match_chat: false,
        width: 900, height: 320,
        position: "center", animation: "bloom", duration: 7.0,
        // The light bar and lightning entrance
        fx_color_mode: "accent", fx_color: rgba(140, 200, 255, 1.0), fx_lightning: true, fx_duration: 0.6,
        font: "Poppins", fontsize: 26,
        accent_mode: "platform", accent: rgba(255, 159, 10, 1.0),
        bg: rgba(20, 20, 30, 0.78), text_color: rgba(255, 255, 255, 1.0), detail_color: rgba(255, 255, 255, 0.72),
        border_radius: 18, frame: "none", shadow: true, show_progress: true,
        volume: 0.8,
        sound_sub: "", sound_gift: "", sound_cheer: "", sound_raid: "", sound_redeem: "",
        sound_superchat: "", sound_member: "", sound_follow: "", sound_hype: "", sound_other: "",
    }
}

// The chat's themes, for cards
fn presets() {
    #{
        glass: #{ bg: rgba(20, 20, 30, 0.78), text_color: rgba(255, 255, 255, 1.0), detail_color: rgba(255, 255, 255, 0.72),
                  border_radius: 18, font: "Poppins", shadow: true, frame: "none" },
        cyberpunk: #{ bg: rgba(10, 5, 20, 0.92), text_color: rgba(255, 255, 255, 1.0), detail_color: rgba(0, 255, 255, 0.85),
                      border_radius: 0, font: "Roboto Mono", shadow: true, frame: "bar" },
        midnight: #{ bg: rgba(15, 15, 20, 0.95), text_color: rgba(229, 229, 234, 1.0), detail_color: rgba(229, 229, 234, 0.65),
                     border_radius: 10, font: "Inter", shadow: false, frame: "none" },
        terminal: #{ bg: rgba(0, 15, 0, 0.95), text_color: rgba(57, 255, 20, 1.0), detail_color: rgba(57, 255, 20, 0.7),
                     border_radius: 4, font: "Roboto Mono", shadow: true, frame: "bar" },
        glow: #{ bg: rgba(25, 20, 35, 0.85), text_color: rgba(255, 255, 255, 1.0), detail_color: rgba(255, 255, 255, 0.75),
                 border_radius: 22, font: "Outfit", shadow: true, frame: "outline" },
    }
}

fn properties() {
    let sounds = "Sounds (*.wav *.mp3 *.ogg *.flac *.m4a)";
    [
        info("Theater Reader's alerts (subs, cheers, raids, redeems, Super Chats…), one at a time. Which ones come is set per chat in Theater Reader. Drawn by Theater: no browser needed."),
        button("test", "🔔 Show test alerts"),
        choice("preset", "Theme", [["Glass", "glass"], ["Cyberpunk", "cyberpunk"], ["Midnight", "midnight"], ["Terminal", "terminal"], ["Glow", "glow"]]),
        toggle("match_chat", "Look like the 💬 Legacy Chat (its colours, font, corners, borders; follows its changes)"),
        int_slider("width", "Width", 300, 3840, 10),
        int_slider("height", "Height", 150, 2160, 10),
        choice("position", "Where", [["Top", "top"], ["Middle", "center"], ["Bottom", "bottom"]]),
        choice("animation", "Entrance", [["Light bar + lightning", "bloom"], ["Pop", "pop"], ["Slide", "slide"], ["Fade", "fade"]]),
        choice("fx_color_mode", "Light colour (light bar + lightning)", [["The card's accent", "accent"], ["One colour", "custom"]]),
        color("fx_color", "Light colour (one colour)"),
        toggle("fx_lightning", "Lightning when an alert comes in"),
        slider("fx_duration", "Light bar sweep (s)", 0.2, 2.0, 0.05),
        slider("duration", "Seconds on screen", 2.0, 20.0, 0.5),
        font_list("font", "Font"),
        int_slider("fontsize", "Text size", 14, 60, 1),
        choice("accent_mode", "Accent", [["The platform's colour", "platform"], ["One colour", "custom"]]),
        color("accent", "Accent colour"),
        color("bg", "Card colour"),
        color("text_color", "Text colour"),
        color("detail_color", "Detail colour"),
        int_slider("border_radius", "Corner radius", 0, 40, 1),
        choice("frame", "Card edge", [["None", "none"], ["Accent bar", "bar"], ["Accent outline", "outline"]]),
        toggle("shadow", "Shadow"),
        toggle("show_progress", "Time bar"),
        info("Sounds: played through the Throwables Layer, like redeem sounds"),
        slider("volume", "Volume", 0.0, 2.0, 0.05),
        file("sound_sub", "Sub", sounds),
        file("sound_gift", "Gifted sub", sounds),
        file("sound_cheer", "Cheer", sounds),
        file("sound_raid", "Raid", sounds),
        file("sound_redeem", "Redeem", sounds),
        file("sound_superchat", "Super Chat", sounds),
        file("sound_member", "Member", sounds),
        file("sound_follow", "Follow", sounds),
        file("sound_hype", "Hype train", sounds),
        file("sound_other", "Anything else", sounds),
    ]
}

// ─── Helpers ────────────────────────────────────────────────────────────────

fn fmax(a, b) { if a > b { a } else { b } }
fn fmin(a, b) { if a < b { a } else { b } }
fn num(v) {
    if type_of(v) == "i64" { v.to_float() }
    else if type_of(v) == "f64" { v }
    else if type_of(v) == "string" {
        // (try is a statement in Rhai, not a value)
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
fn field(m, k) { if is_map(m) { m[k] } else { () } }
// The first non-empty text among `values`
fn first_text(values) {
    for v in values {
        let t = text_of(v);
        if t != "" { return t; }
    }
    ""
}
fn ease_back(t) { let c1 = 1.70158; let c3 = c1 + 1.0; let u = t - 1.0; 1.0 + c3 * u * u * u + c1 * u * u }
fn ease_out(t) { let u = 1.0 - t; 1.0 - u * u * u }
// 1 000 000 → "1,000,000"
fn thousands(n) {
    let s = n.to_string();
    let out = "";
    let count = 0;
    let i = s.len() - 1;
    while i >= 0 {
        if count > 0 && count % 3 == 0 && s[i] != '-' { out = "," + out; }
        out = s[i] + out;
        count += 1;
        i -= 1;
    }
    out
}

// ─── What an alert says ─────────────────────────────────────────────────────

fn platform_of(text) {
    let p = text.to_lower();
    if p.contains("twitch") { "twitch" } else if p.contains("kick") { "kick" } else if p.contains("youtube") { "youtube" } else { "" }
}

fn platform_color(p) {
    switch p {
        "twitch" => 0xFF9146FF,
        "kick" => 0xFF53FC18,
        "youtube" => 0xFFFF0000,
        _ => 0,
    }
}

// Symbol a kind shows when there's no picture
fn icon_of(kind) {
    switch kind {
        "sub" => "★", "gift" => "✚", "cheer" => "♦", "raid" => "➤", "redeem" => "✦",
        "superchat" => "$", "member" => "♛", "follow" => "♥", "hype" => "⚡",
        "moderation" => "⚠", "stream" => "●", _ => "✔",
    }
}

fn tier_of(plan) {
    switch plan {
        "1000" => "Tier 1", "2000" => "Tier 2", "3000" => "Tier 3", "Prime" => "Prime",
        _ => "",
    }
}

// A trigger without its second word, the channel it came from: "Twitch
// mortygotvr redeem Doom" → "Twitch redeem Doom"
fn without_owner(text) {
    let words = text.split(" ");
    if words.len() >= 3 && platform_of(words[0]) != "" { words.remove(1); }
    let out = "";
    for w in words {
        out += if out == "" { w } else { " " + w };
    }
    out
}

// An alert from Theater Reader's message, or () when it isn't one
fn alert_of(ev) {
    if text_of(ev["type"]) != "alert" { return (); }
    let data = ev["data"];
    if !is_map(data) { data = #{}; }
    let source = text_of(ev["source"]);
    // Theater Reader's tester sends its trigger as the event ("Twitch
    // testserver cheer"); real alerts, the event's own name ("Twitch cheer")
    let event = text_of(ev["event"]);
    if source == "Test UI" { event = without_owner(event); }
    let e = event.to_lower();
    // What a platform sent that Theater Reader doesn't know: not an alert
    if e.ends_with(" other") { return (); }
    let platform = platform_of(source + " " + event);
    if e.contains("paid_message") || e.contains("sticker_message") || e.contains("member_message") || e.contains("super chat") || e.contains("super sticker") {
        platform = "youtube";
    }
    let user = first_text([data["display_name"], data["username"], data["user"], data["author"], data["gifter_username"], data["host_username"]]);
    let who = if user == "" { "Someone" } else { user };
    let message = first_text([data["message"], data["user_input"]]);
    let image = first_text([data["image"], data["avatar"]]);
    let a = #{ kind: "other", platform: platform, user: who, action: "", detail: "", message: message, image: image, amount: 0.0 };

    if e.contains("redeem") {
        // The IRC copy of a redeem has no title: the PubSub one follows
        if e.contains("(irc)") { return (); }
        a.kind = "redeem";
        let title = first_text([data["reward_title"], data["title"]]);
        if title == "" || title == "Unknown" {
            // From the trigger: "Twitch <owner> redeem <title>"
            let t = without_owner(text_of(ev["trigger"]));
            let at = t.index_of("redeem ");
            title = if at >= 0 { t.sub_string(at + 7) } else { "" };
            title.trim();
        }
        a.action = if title == "" { "redeemed a reward" } else { "redeemed " + title };
        let cost = num(data["cost"]);
        if cost > 0.0 { a.detail = thousands(cost.to_int()) + " points"; }
    } else if e.contains("gift") || ["subgift", "anonsubgift", "submysterygift", "giftpaidupgrade"].contains(text_of(data["type"])) {
        a.kind = "gift";
        let t = text_of(data["type"]);
        if t == "anonsubgift" { a.user = "An anonymous gifter"; }
        // (Theater Reader's test gifts carry an amount)
        let gifted = data["gifted_usernames"];
        let n = if type_of(gifted) == "array" { gifted.len() } else { num(first_text([data["count"], data["quantity"], data["amount"]])).to_int() };
        a.action = if t == "giftpaidupgrade" { "is continuing their gifted sub!" }
                   else if n > 1 { "gifted " + n + " subs!" } else { "gifted a sub!" };
        a.detail = tier_of(text_of(data["plan"]));
        a.amount = n.to_float();
    } else if e.contains("sub") {
        a.kind = "sub";
        a.action = if text_of(data["type"]) == "resub" { "resubscribed!" } else { "subscribed!" };
        a.detail = tier_of(text_of(data["plan"]));
    } else if e.contains("cheer") {
        a.kind = "cheer";
        let bits = num(first_text([data["bits"], data["amount"]]));
        a.amount = bits;
        a.action = "cheered " + thousands(bits.to_int()) + (if bits == 1.0 { " bit!" } else { " bits!" });
    } else if e.contains("raid") {
        if e.contains("end") { return (); }
        a.kind = "raid";
        let viewers = num(first_text([data["viewers"], data["viewers_count"], data["viewer_count"], data["amount"]]));
        a.amount = viewers;
        a.action = if viewers > 0.0 { "is raiding with " + thousands(viewers.to_int()) + " viewers!" } else { "is raiding!" };
    } else if e.contains("paid_message") || e.contains("sticker_message") || e.contains("super chat") || e.contains("super sticker") {
        a.kind = "superchat";
        a.action = if e.contains("sticker") { "sent a Super Sticker!" } else { "sent a Super Chat!" };
        a.detail = first_text([data["amount"]]);
    } else if e.contains("member") {
        a.kind = "member";
        a.action = "became a member!";
    } else if e.contains("follow") {
        a.kind = "follow";
        a.action = "followed!";
    } else if e.contains("hype train") {
        a.kind = "hype";
        let level = num(data["level"]).to_int();
        a.user = "Hype Train";
        a.action = if level > 0 { "Level " + level + "!" } else { "has started!" };
        let goal = num(data["goal"]);
        if goal > 0.0 { a.detail = "" + (num(data["progress"]) * 100.0 / goal).to_int() + "% to the next level"; }
    } else if e.contains("timeout") || e.contains("ban") {
        a.kind = "moderation";
        let secs = num(data["duration"]).to_int();
        a.action = if e.contains("timeout") { "was timed out" + (if secs > 0 { " (" + secs + " s)" } else { "" }) } else { "was banned" };
        a.message = "";
    } else if e.contains("stream start") || e.contains("stream end") {
        a.kind = "stream";
        a.user = if e.contains("start") { "We're live!" } else { "Stream over" };
        a.action = if e.contains("start") { "The stream has started" } else { "Thanks for watching!" };
    } else {
        a.action = if event == "" { text_of(ev["trigger"]) } else { event };
    }
    a
}

// ─── The card ───────────────────────────────────────────────────────────────

fn style_of(s) {
    let size = fmax(8.0, num(s.fontsize));
    let font = text_of(s.font);
    #{
        font: if font == "" { "Poppins" } else { font },
        name_size: size * 1.2, action_size: size, detail_size: size * 0.7, msg_size: size * 0.8,
        icon: size * 2.8, pad: size * 0.85,
        bg: s.bg, text: s.text_color, detail: s.detail_color, accent: s.accent, accent_mode: text_of(s.accent_mode),
        radius: num(s.border_radius), frame: text_of(s.frame), shadow: s.shadow == true,
        border: (), border_color: 0, name_weight: 700, opacity: 1.0,
        position: text_of(s.position), animation: text_of(s.animation),
        duration: fmax(1.0, num(s.duration)), progress: s.show_progress == true,
        fx_mode: text_of(s.fx_color_mode), fx_color: s.fx_color, fx_lightning: s.fx_lightning == true,
        sweep: fmax(0.1, num(s.fx_duration)),
    }
}

// The 💬 Theater Chat's look over the card's own ("Look like the chat"); the
// card's own when there's no chat
fn chat_look(st, c) {
    if !is_map(c) { return st; }
    let font = text_of(c.font);
    if font != "" { st.font = font; }
    st.bg = if text_of(c.bg_mode) == "transparent" || text_of(c.bubble) == "minimal" { 0 } else { c.bg };
    st.text = c.text;
    st.detail = with_alpha(c.text, 0.7);
    st.radius = num(c.radius);
    st.shadow = c.shadow == true;
    st.opacity = fmax(0.0, fmin(1.0, num(c.opacity)));
    let weight = num(c.weight_user);
    if weight > 0.0 { st.name_weight = weight.to_int(); }
    // Names: the platform's colour, or the chat's one colour (the chatter's
    // own colour isn't in an alert: the platform's then)
    st.accent = c.name;
    st.accent_mode = if text_of(c.name_mode) == "custom" { "custom" } else { "platform" };
    // Its bubble's edge
    if text_of(c.bubble) == "left-border" {
        st.frame = "bar";
    } else {
        st.frame = "none";
        st.border = c.border;
        st.border_color = c.border_color;
    }
    st.look_stamp = c.stamp;
    st
}

fn accent_of(a, st) {
    let p = platform_color(a.platform);
    if st.accent_mode == "platform" && p != 0 { p } else { st.accent }
}

// `text` in lines at most `width` wide, at most `max_lines` (the last one
// cut short with "…")
fn wrap(text, font, size, weight, width, max_lines) {
    let lines = [];
    let line = "";
    let words = text.split(" ");
    let i = 0;
    while i < words.len() {
        let word = words[i];
        let tried = if line == "" { word } else { line + " " + word };
        if line != "" && measure_text(tried, font, size, weight)[0] > width {
            lines.push(line);
            line = word;
            if lines.len() == max_lines { break; }
        } else {
            line = tried;
        }
        i += 1;
    }
    if lines.len() < max_lines && line != "" {
        lines.push(line);
    } else if lines.len() == max_lines {
        // More than fits: the last line ends in "…"
        let last = lines.pop();
        while last.len() > 1 && measure_text(last + "…", font, size, weight)[0] > width {
            last.truncate(last.len() - 1);
        }
        lines.push(last + "…");
    }
    lines
}

// The card for `a`, laid out once: #{ w, h, cmds, pending }
fn layout(a, st, max_w) {
    let pad = st.pad;
    let icon = st.icon;
    let accent = accent_of(a, st);
    let text_x = pad + icon + pad * 0.9;
    let text_w = fmax(60.0, max_w - text_x - pad);
    let lh = 1.22;

    let rows = [];
    for line in wrap(a.user, st.font, st.name_size, st.name_weight, text_w, 1) {
        rows.push(#{ text: line, size: st.name_size, weight: st.name_weight, color: accent });
    }
    if a.action != "" {
        for line in wrap(a.action, st.font, st.action_size, 600, text_w, 2) {
            rows.push(#{ text: line, size: st.action_size, weight: 600, color: st.text });
        }
    }
    if a.detail != "" {
        rows.push(#{ text: a.detail, size: st.detail_size, weight: 600, color: st.detail, gap: st.detail_size * 0.25 });
    }
    if a.message != "" {
        let first = true;
        for line in wrap("“" + a.message + "”", st.font, st.msg_size, 400, text_w, 3) {
            rows.push(#{ text: line, size: st.msg_size, weight: 400, color: st.text, gap: if first { st.msg_size * 0.45 } else { 0.0 } });
            first = false;
        }
    }

    // How big: the text's height, or the icon's
    let text_h = 0.0;
    let widest = 0.0;
    for r in rows {
        text_h += r.size * lh + num(r.gap);
        widest = fmax(widest, measure_text(r.text, st.font, r.size, r.weight)[0]);
    }
    let h = fmax(icon, text_h) + pad * 2.0;
    let w = fmin(max_w, text_x + widest + pad);

    let cmds = [];
    if st.shadow {
        cmds.push(#{ rect: [0.0, 5.0, w, h], fill: rgba(0, 0, 0, 0.35), radius: st.radius });
    }
    let card = #{ rect: [0.0, 0.0, w, h], fill: st.bg, radius: st.radius };
    if st.frame == "bar" {
        card.border = [0.0, 0.0, 0.0, 6.0];
        card.border_color = accent;
    } else if st.frame == "outline" {
        card.border = 2.0;
        card.border_color = accent;
    } else if !none(st.border) {
        // The chat's own borders
        card.border = st.border;
        card.border_color = st.border_color;
    }
    cmds.push(card);

    // The picture (a reward's, the viewer's), or the kind's symbol meanwhile
    let icon_y = (h - icon) / 2.0;
    let pending = false;
    let shown_image = false;
    if a.image != "" {
        let size = image_size(a.image);
        if none(size) {
            pending = true;
        } else if size[0] > 0 {
            cmds.push(#{ image: a.image, x: pad, y: icon_y, w: icon, h: icon, radius: icon * 0.22 });
            shown_image = true;
        }
    }
    if !shown_image {
        cmds.push(#{ rect: [pad, icon_y, icon, icon], fill: with_alpha(accent, 0.18), radius: icon / 2.0, border: 2.0, border_color: accent });
        let glyph = icon_of(a.kind);
        let gs = icon * 0.5;
        let m = measure_text(glyph, st.font, gs, 700);
        cmds.push(#{ text: glyph, x: pad + (icon - m[0]) / 2.0, y: icon_y + (icon - m[1]) / 2.0, font: st.font, size: gs, weight: 700, color: accent });
    }

    // The text, centred against the icon
    let y = (h - text_h) / 2.0;
    for r in rows {
        y += num(r.gap);
        cmds.push(#{ text: r.text, x: text_x, y: y, font: st.font, size: r.size, weight: r.weight, color: r.color, shadow: st.shadow });
        y += r.size * lh;
    }
    #{ w: w, h: h, cmds: cmds, pending: pending, accent: accent }
}

// ─── The light bar and lightning entrance ("bloom") ─────────────────────────
//
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

// ─── Behaviour ──────────────────────────────────────────────────────────────

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
    this.style = style_of(s);
    if s.match_chat == true { this.style = chat_look(this.style, shared_value("legacy_chat_look")); }
    this.size = [s.width, s.height];
    if none(this.queue) { this.queue = []; this.next_at = 0.0; this.recent = []; }
    // The one showing, laid out again with the new style
    if !none(this.current) { this.current.laid_ok = false; }
    changes
}

// Queued, unless it's the same one again (which ones come at all is set in
// Theater Reader, per chat)
fn add_alert(a) {
    let key = a.kind + "|" + a.user + "|" + a.action + "|" + a.message;
    let now = this.time;
    let keep = [];
    for r in this.recent {
        if now - r.at < 3.0 { keep.push(r); }
    }
    for r in keep {
        if r.key == key { this.recent = keep; return; }
    }
    keep.push(#{ key: key, at: now });
    this.recent = keep;
    a.laid_ok = false;
    this.queue.push(a);
    while this.queue.len() > 30 { this.queue.remove(0); }
}

fn on_event(ev) {
    let a = alert_of(ev);
    if !none(a) { this.add_alert(a); }
}

// Samples of each kind (the test button)
fn test_alerts() {
    [
        #{ type: "alert", source: "TwitchParser", event: "Twitch sub", data: #{ username: "StreamFan42", type: "resub", plan: "1000", message: "Six months already, love this stream!" } },
        #{ type: "alert", source: "TwitchParser", event: "Twitch cheer", data: #{ username: "KappaKing", bits: "500", message: "Take my bits!" } },
        #{ type: "alert", source: "TwitchParser", event: "Twitch raid", data: #{ username: "BigStreamer", viewers: "128" } },
        #{ type: "alert", source: "TwitchParser", event: "Twitch redeem (pubsub)", data: #{ display_name: "ModSquad", reward_title: "Hydrate!", cost: 1500,
            image: "https://static-cdn.jtvnw.net/custom-reward-images/default-4.png" } },
        #{ type: "alert", source: "YouTubeParser", event: "paid_message", data: #{ author: "TubeWatcher", amount: "$10.00", message: "Greetings from YouTube!" } },
        #{ type: "alert", source: "KickParser", event: "Kick follow", data: #{ username: "KickRunner" } },
    ]
}

fn on_button(key) {
    if key == "test" {
        let tests = test_alerts();
        for ev in tests {
            let a = alert_of(ev);
            if !none(a) { this.add_alert(a); }
        }
    }
}

fn tick(dt) {
    if none(this.queue) { return []; }
    let now = this.time;
    // Looking like the chat: its changes followed (checked every second)
    if this.settings.match_chat == true && now - num(this.look_checked) >= 1.0 {
        this.look_checked = now;
        let c = shared_value("legacy_chat_look");
        if is_map(c) && c.stamp != this.style.look_stamp {
            this.style = chat_look(style_of(this.settings), c);
            if !none(this.current) { this.current.laid_ok = false; }
        }
    }
    let st = this.style;
    let w = num(this.size[0]);
    let h = num(this.size[1]);
    let enter = 0.45;
    let leave = 0.45;

    // The one showing is done: the next after a short breath
    if !none(this.current) && now - this.current.start > st.duration {
        this.current = ();
        this.next_at = now + 0.35;
    }
    if none(this.current) {
        if this.queue.len() == 0 || now < this.next_at { return []; }
        let a = this.queue.remove(0);
        a.start = now;
        this.current = a;
        let sound = text_of(this.settings["sound_" + a.kind]);
        if sound == "" { sound = text_of(this.settings.sound_other); }
        if sound != "" { play_sound(sound, num(this.settings.volume)); }
    }

    // Laid out once (again while its picture loads)
    let redo = !this.current.laid_ok || (this.current.laid.pending && now - this.current.laid_at > 0.25);
    if redo {
        this.current.laid = layout(this.current, st, fmax(100.0, w - 40.0));
        this.current.laid_ok = true;
        this.current.laid_at = now;
    }
    let cw = this.current.laid.w;
    let ch = this.current.laid.h;
    let age = now - this.current.start;
    let left = st.duration - age;

    // Where, and how it comes and goes
    let x = (w - cw) / 2.0;
    let y = if st.position == "top" { 20.0 } else if st.position == "bottom" { h - ch - 20.0 } else { (h - ch) / 2.0 };
    let alpha = 1.0;
    let scale = 1.0;
    let dy = 0.0;
    if age < enter {
        let t = age / enter;
        if st.animation == "pop" { scale = 0.7 + 0.3 * ease_back(t); alpha = fmin(1.0, t * 2.0); }
        else if st.animation == "slide" { dy = (if st.position == "bottom" { 60.0 } else { -60.0 }) * (1.0 - ease_out(t)); alpha = t; }
        else { alpha = t; }
    } else if left < leave {
        let t = 1.0 - fmax(0.0, left) / leave;
        alpha = 1.0 - ease_out(t);
        if st.animation == "pop" { scale = 1.0 - 0.06 * t; }
        else if st.animation == "slide" { dy = (if st.position == "bottom" { 40.0 } else { -40.0 }) * ease_out(t); }
    }

    let card = [#{ group: this.current.laid.cmds, x: 0.0, y: 0.0 }];
    // The time left, along the card's bottom
    if st.progress {
        let frac = fmax(0.0, fmin(1.0, left / st.duration));
        let inset = fmax(st.radius * 0.6, 4.0);
        let bw = (cw - inset * 2.0) * frac;
        if bw > 1.0 {
            card.push(#{ rect: [inset, ch - 5.0, bw, 3.0], fill: this.current.laid.accent, radius: 1.5 });
        }
    }
    // The light bar + lightning entrance: the bar shows the card (it leaves
    // fading, like the Fade entrance)
    if st.animation == "bloom" && age < bloom_length(st) {
        let color = if st.fx_mode == "custom" { st.fx_color } else { this.current.laid.accent };
        let seed = (this.current.start * 1000.0).floor().to_int() % 100000 + 1;
        return [#{ group: bloom_parts(card, cw, ch, age, st, color, seed), x: x, y: y, alpha: st.opacity }];
    }
    [#{ group: card, x: x, y: y + dy, scale: scale, origin: [cw / 2.0, ch / 2.0], alpha: alpha * st.opacity }]
}
