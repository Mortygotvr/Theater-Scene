// Rosia_alert.rhai — 🌹 Rosia Alert: 8-bit stream alerts with a rose-wrapped gray border & magic entrance
// Drawn by Theater Scene with OBS graphics API (no browser / CEF needed).
//
// Features:
// - Authentic 8-bit pixel aesthetic: monospaced typography, sharp RPG dialogue framing
// - Rounded stone gray metallic beveled border with corner rivets
// - High-resolution botanical rose blossoms and thorny vines wrapping around the card
// - Magical entrance effect on new alerts: radiant 4-pointed stars, drifting stardust particles, glowing aura
// - Curated Gothic/Retro presets: Rosia Gothic, Ruby Noir, Tea Rose, Retro Arcade
// - Seamless synchronization with 🌹 Rosia Chat ("match_chat")
// - Full Theater Reader alert integration: subs, gifts, cheers/bits, raids, redeems, Super Chats, follows, hype train
// - Per-kind sound effects, volume control, and time progress indicator

const ID = "rosia_alert_source";
const NAME = "🌹 Rosia Alert";
const KIND = "source";

// ─── Settings & Presets ─────────────────────────────────────────────────────

fn defaults() {
    #{
        preset: "rosia_gothic", applied_preset: "", match_chat: false,
        width: 800, height: 260,
        position: "top", animation: "magic", duration: 6.5,
        magic_stars: true, magic_color: rgba(255, 215, 80, 1.0),
        rose_style: "wrap",
        border_gray: rgba(120, 120, 142, 1.0),
        rose_color: rgba(225, 25, 85, 1.0),
        vine_color: rgba(45, 135, 70, 1.0),
        font: "Roboto Mono", fontsize: 22,
        accent_mode: "custom", accent: rgba(255, 75, 125, 1.0),
        bg: rgba(18, 18, 26, 0.92), text_color: rgba(240, 240, 250, 1.0), detail_color: rgba(200, 200, 220, 0.85),
        border_radius: 16, shadow: true, show_progress: true,
        volume: 0.8,
        sound_sub: "", sound_gift: "", sound_cheer: "", sound_raid: "", sound_redeem: "",
        sound_superchat: "", sound_member: "", sound_follow: "", sound_hype: "", sound_other: "",
        global_opacity: 1.0,
    }
}

fn presets() {
    #{
        rosia_gothic: #{
            bg: rgba(18, 18, 26, 0.92), text_color: rgba(240, 240, 250, 1.0), detail_color: rgba(200, 200, 220, 0.85),
            accent: rgba(255, 75, 125, 1.0), border_gray: rgba(120, 120, 142, 1.0),
            rose_color: rgba(225, 25, 85, 1.0), vine_color: rgba(45, 135, 70, 1.0),
            magic_stars: true, magic_color: rgba(255, 215, 80, 1.0),
            border_radius: 16, font: "Roboto Mono", shadow: true, rose_style: "wrap", animation: "magic",
        },
        ruby_noir: #{
            bg: rgba(10, 10, 15, 0.96), text_color: rgba(255, 255, 255, 1.0), detail_color: rgba(220, 200, 210, 0.85),
            accent: rgba(255, 60, 95, 1.0), border_gray: rgba(140, 140, 160, 1.0),
            rose_color: rgba(200, 15, 55, 1.0), vine_color: rgba(30, 100, 50, 1.0),
            magic_stars: true, magic_color: rgba(255, 140, 180, 1.0),
            border_radius: 16, font: "Roboto Mono", shadow: true, rose_style: "wrap", animation: "magic",
        },
        tea_rose: #{
            bg: rgba(28, 24, 28, 0.92), text_color: rgba(250, 245, 240, 1.0), detail_color: rgba(230, 215, 210, 0.85),
            accent: rgba(255, 140, 170, 1.0), border_gray: rgba(130, 125, 135, 1.0),
            rose_color: rgba(255, 130, 165, 1.0), vine_color: rgba(70, 145, 95, 1.0),
            magic_stars: true, magic_color: rgba(255, 200, 140, 1.0),
            border_radius: 18, font: "Roboto Mono", shadow: true, rose_style: "wrap", animation: "magic",
        },
        retro_arcade: #{
            bg: rgba(0, 0, 0, 0.94), text_color: rgba(240, 255, 250, 1.0), detail_color: rgba(200, 240, 230, 0.85),
            accent: rgba(255, 215, 0, 1.0), border_gray: rgba(100, 110, 130, 1.0),
            rose_color: rgba(255, 0, 110, 1.0), vine_color: rgba(0, 220, 90, 1.0),
            magic_stars: true, magic_color: rgba(255, 230, 0, 1.0),
            border_radius: 12, font: "Roboto Mono", shadow: true, rose_style: "wrap", animation: "magic",
        },
    }
}

fn properties() {
    let sounds = "Sounds (*.wav *.mp3 *.ogg *.flac *.m4a)";
    [
        info("🌹 Rosia Alert: 8-bit stream alerts with rounded stone gray border wrapped in roses & magical star entrance."),
        button("test", "🔔 Show test alerts"),
        choice("preset", "Theme", [
            ["Rosia Gothic", "rosia_gothic"],
            ["Ruby Noir", "ruby_noir"],
            ["Tea Rose", "tea_rose"],
            ["Retro Arcade", "retro_arcade"],
        ]),
        toggle("match_chat", "Look like 🌹 Rosia Chat (matches font, colors, border, roses)"),
        int_slider("width", "Width", 300, 3840, 10),
        int_slider("height", "Height", 120, 2160, 10),
        choice("position", "Position", [["Top", "top"], ["Middle", "center"], ["Bottom", "bottom"]]),
        choice("animation", "Entrance animation", [
            ["✨ Magic (Stars & Dust)", "magic"],
            ["8-bit Pixel Pop", "pop"],
            ["Slide", "slide"],
            ["Fade", "fade"],
        ]),
        toggle("magic_stars", "✨ Magic stars & stardust"),
        color("magic_color", "Magic sparkle color"),
        slider("duration", "Seconds on screen", 2.0, 20.0, 0.5),
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
        int_slider("fontsize", "Text size", 14, 50, 1),
        choice("accent_mode", "Name color mode", [["Custom accent", "custom"], ["Platform color", "platform"]]),
        color("accent", "Accent color"),
        color("bg", "Card background"),
        color("text_color", "Action text color"),
        color("detail_color", "Detail text color"),
        int_slider("border_radius", "Corner radius (0 for sharp 8-bit)", 0, 40, 1),
        toggle("shadow", "8-bit crisp shadow"),
        toggle("show_progress", "Time remaining bar"),
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

// ─── Helpers & Color Math ───────────────────────────────────────────────────

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
fn field(m, k) { if is_map(m) { m[k] } else { () } }

fn first_text(values) {
    for v in values {
        let t = text_of(v);
        if t != "" { return t; }
    }
    ""
}

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

fn platform_color(p) {
    switch p {
        "twitch" => 0xFF9146FF,
        "kick" => 0xFF53FC18,
        "youtube" => 0xFFFF0000,
        _ => 0,
    }
}

fn icon_of(kind) {
    switch kind {
        "sub" => "★", "gift" => "✚", "cheer" => "♦", "raid" => "➤", "redeem" => "✦",
        "superchat" => "$", "member" => "♛", "follow" => "♥", "hype" => "⚡",
        "moderation" => "⚠", "stream" => "●", _ => "✔",
    }
}

// The card for an alert, or () when it isn't one. Theater hands every alert
// in one shape, whatever sent it: ev.alert, with kind, platform, user,
// message, image, amount, currency, title, cost, tier, months, duration,
// progress, goal, anonymous, test, trigger and raw (the platform's own data).
// Here it's only put into words; the card's kinds are those with a sound.
fn alert_of(ev) {
    let s = ev["alert"];
    if !is_map(s) { return (); }
    let kind = text_of(s.kind);
    let amount = num(s.amount);
    let user = text_of(s.user);
    let a = #{ kind: "other", platform: text_of(s.platform), user: if user == "" { "Someone" } else { user },
               action: "", detail: "", message: text_of(s.message), image: text_of(s.image), amount: amount };
    // Money as the platform wrote it ("$10.00"), else currency and amount
    let money = first_text([field(s.raw, "amount")]);
    if money == "" { money = if amount == amount.floor() { thousands(amount.to_int()) } else { "" + amount }; }
    // A currency code as its symbol where there's a common one ("USD" → "$5.00"), else after it ("5.00 CHF")
    let currency = text_of(s.currency);
    let symbol = switch currency { "USD" => "$", "EUR" => "€", "GBP" => "£", "JPY" => "¥", "CAD" => "CA$", "AUD" => "A$", _ => currency };
    if currency != "" && !money.contains(symbol) && !money.contains(currency) {
        let code = symbol == currency && currency.len() >= 3 && currency.to_upper() == currency;
        money = if code { money + " " + currency } else { symbol + money };
    }

    switch kind {
        "redeem" => {
            a.kind = "redeem";
            let title = text_of(s.title);
            a.action = if title == "" { "redeemed a reward" } else { "redeemed " + title };
            let cost = num(s.cost);
            if cost > 0.0 { a.detail = thousands(cost.to_int()) + " points"; }
        }
        "gift" | "gift_upgrade" => {
            a.kind = "gift";
            if s.anonymous == true { a.user = "An anonymous gifter"; }
            let n = amount.to_int();
            a.action = if kind == "gift_upgrade" { "is continuing their gifted sub!" }
                       else if n > 1 { "gifted " + n + " subs!" } else { "gifted a sub!" };
            a.detail = text_of(s.tier);
        }
        "sub" | "resub" => {
            a.kind = "sub";
            a.action = if kind == "resub" { "resubscribed!" } else { "subscribed!" };
            a.detail = text_of(s.tier);
            let months = num(s.months).to_int();
            if months > 1 { a.detail += (if a.detail == "" { "" } else { " · " }) + months + " months"; }
        }
        "cheer" => {
            a.kind = "cheer";
            a.action = "cheered " + thousands(amount.to_int()) + (if amount == 1.0 { " bit!" } else { " bits!" });
        }
        "raid" => {
            a.kind = "raid";
            a.action = if amount > 0.0 { "is raiding with " + thousands(amount.to_int()) + " viewers!" } else { "is raiding!" };
        }
        "superchat" | "supersticker" => {
            a.kind = "superchat";
            a.action = if kind == "supersticker" { "sent a Super Sticker!" } else { "sent a Super Chat!" };
            a.detail = money;
        }
        // Ko-fi, Patreon…: the money card
        "donation" | "pledge" => {
            a.kind = "superchat";
            a.action = if kind == "pledge" { "pledged " + money + "!" } else { "donated " + money + "!" };
            a.detail = text_of(s.tier);
        }
        "pledge_change" => { a.kind = "superchat"; a.action = "changed their pledge to " + money + "!"; a.detail = text_of(s.tier); }
        "shop_order" => {
            a.kind = "superchat";
            let what = text_of(s.title);
            a.action = if what == "" { "bought something for " + money + "!" } else { "bought " + what + "!" };
        }
        "commission" => { a.kind = "superchat"; a.action = "ordered a commission for " + money + "!"; }
        // An ended pledge fires its trigger, but no card shows it
        "pledge_end" => { return (); }
        "member" => { a.kind = "member"; a.action = "became a member!"; }
        "follow" => { a.kind = "follow"; a.action = "followed!"; }
        "hype_train" => {
            a.kind = "hype";
            let level = amount.to_int();
            a.user = "Hype Train";
            a.action = if level > 0 { "Level " + level + "!" } else { "has started!" };
            let goal = num(s.goal);
            if goal > 0.0 { a.detail = "" + (num(s.progress) * 100.0 / goal).to_int() + "% to the next level"; }
        }
        "timeout" | "ban" => {
            a.kind = "moderation";
            let secs = num(s.duration).to_int();
            a.action = if kind == "timeout" { "was timed out" + (if secs > 0 { " (" + secs + " s)" } else { "" }) } else { "was banned" };
            a.message = "";
        }
        "stream_start" => { a.kind = "stream"; a.user = "We're live!"; a.action = "The stream has started"; }
        "stream_end" => { a.kind = "stream"; a.user = "Stream over"; a.action = "Thanks for watching!"; }
        _ => {
            let event = text_of(ev["event"]);
            a.action = if event == "" { text_of(s.trigger) } else { event };
        }
    }
    a
}

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

// ─── Style & Layout ─────────────────────────────────────────────────────────

fn style_of(s) {
    let size = fmax(10.0, num(s.fontsize));
    let font = text_of(s.font);
    #{
        font: if font == "" { "Roboto Mono" } else { font },
        name_size: size * 1.15, action_size: size, detail_size: size * 0.75, msg_size: size * 0.82,
        icon: (size * 2.6).round(), pad: (size * 0.95).round(),
        bg: s.bg, text: s.text_color, detail: s.detail_color, accent: s.accent, accent_mode: text_of(s.accent_mode),
        radius: num(s.border_radius),
        border_gray: s.border_gray,
        rose_color: s.rose_color,
        vine_color: s.vine_color,
        rose_style: s.rose_style,
        shadow: s.shadow == true,
        show_progress: s.show_progress == true,
        opacity: num(s.global_opacity),
        position: text_of(s.position),
        animation: text_of(s.animation),
        magic_stars: s.magic_stars ?? true,
        magic_color: s.magic_color ?? rgba(255, 215, 80, 1.0),
        duration: fmax(1.0, num(s.duration)),
    }
}

// Mirrors 🌹 Rosia Chat's theme when "match_chat" is checked
fn chat_look(st, c) {
    if !is_map(c) { return st; }
    let font = text_of(c.font);
    if font != "" { st.font = font; }
    st.bg = c.bg;
    st.text = c.text;
    st.detail = color_alpha(c.text, 0.75);
    st.radius = num(c.radius);
    st.shadow = c.shadow == true;
    st.border_gray = c.border_color;
    st.rose_color = c.rose_color;
    st.vine_color = c.vine_color;
    st.accent = c.name;
    st.accent_mode = if text_of(c.name_mode) == "custom" { "custom" } else { "platform" };
    st.opacity = num(c.opacity);
    st.look_stamp = c.stamp;
    st
}

fn accent_of(a, st) {
    let p = platform_color(a.platform);
    if st.accent_mode == "platform" && p != 0 { p } else { st.accent }
}

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
        let last = lines.pop();
        while last.len() > 1 && measure_text(last + "…", font, size, weight)[0] > width {
            last.truncate(last.len() - 1);
        }
        lines.push(last + "…");
    }
    lines
}

fn layout(a, st, max_w) {
    let pad = st.pad;
    let icon = st.icon;
    let accent = accent_of(a, st);
    // Indent for botanical corner rose
    let left_margin = fmax(pad, 36.0);
    let text_x = left_margin + icon + pad * 0.85;
    let text_w = fmax(60.0, max_w - text_x - pad - 20.0);
    let lh = 1.25;

    let rows = [];
    for line in wrap(a.user, st.font, st.name_size, 700, text_w, 1) {
        rows.push(#{ text: line, size: st.name_size, weight: 700, color: accent });
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
        for line in wrap("“" + a.message + "”", st.font, st.msg_size, 400, text_w, 2) {
            rows.push(#{ text: line, size: st.msg_size, weight: 400, color: st.text, gap: if first { st.msg_size * 0.40 } else { 0.0 } });
            first = false;
        }
    }

    let text_h = 0.0;
    let widest = 0.0;
    for r in rows {
        text_h += r.size * lh + num(r.gap);
        widest = fmax(widest, measure_text(r.text, st.font, r.size, r.weight)[0]);
    }
    let h = fmax(icon + pad * 2.0, text_h + pad * 2.2);
    let w = fmin(max_w, fmax(320.0, text_x + widest + pad + 32.0));

    let cmds = [];
    if st.shadow {
        cmds.push(#{ rect: [0.0, 5.0, w, h], fill: rgba(0, 0, 0, 0.45), radius: st.radius });
    }

    // 1. Rounded stone gray border wrapped in botanical roses
    for c in rose_frame_cmds(0.0, 0.0, w, h, st.bg, st) {
        cmds.push(c);
    }

    // 2. Event Icon Box (8-bit beveled avatar/symbol frame)
    let icon_y = ((h - icon) / 2.0).round();
    let pending = false;
    let shown_image = false;
    if a.image != "" {
        let size = image_size(a.image);
        if none(size) {
            pending = true;
        } else if size[0] > 0 {
            cmds.push(#{ rect: [left_margin - 2.0, icon_y - 2.0, icon + 4.0, icon + 4.0], fill: 0, radius: 4.0, border: 1.5, border_color: st.border_gray });
            cmds.push(#{ image: a.image, x: left_margin, y: icon_y, w: icon, h: icon, radius: 3.0 });
            shown_image = true;
        }
    }
    if !shown_image {
        cmds.push(#{ rect: [left_margin, icon_y, icon, icon], fill: color_alpha(accent, 0.16), radius: 6.0, border: 1.8, border_color: accent });
        let glyph = icon_of(a.kind);
        let gs = icon * 0.52;
        let m = measure_text(glyph, st.font, gs, 700);
        cmds.push(#{ text: glyph, x: left_margin + ((icon - m[0]) / 2.0).round(), y: icon_y + ((icon - m[1]) / 2.0).round(), font: st.font, size: gs, weight: 700, color: accent, shadow: st.shadow });
    }

    // 3. Typography
    let y = ((h - text_h) / 2.0).round();
    for r in rows {
        y += num(r.gap);
        cmds.push(#{ text: r.text, x: text_x, y: y, font: st.font, size: r.size, weight: r.weight, color: r.color, shadow: st.shadow });
        y += r.size * lh;
    }

    #{ w: w, h: h, cmds: cmds, pending: pending, accent: accent }
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
    if s.match_chat == true { this.style = chat_look(this.style, shared_value("rosia_chat_look")); }
    this.size = [s.width, s.height];
    if none(this.queue) { this.queue = []; this.next_at = 0.0; this.recent = []; }
    if !none(this.current) { this.current.laid_ok = false; }
    changes
}

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

// Samples of each kind (the test button), in the shape Theater hands alerts
fn test_alerts() {
    [
        #{ type: "alert", alert: #{ kind: "resub", platform: "twitch", user: "RosiaGuard", tier: "Tier 1", months: 6, message: "Bloomed in the gothic garden for 6 months!" } },
        #{ type: "alert", alert: #{ kind: "cheer", platform: "twitch", user: "RubyKnight", amount: 500, message: "Take these thorny bits!" } },
        #{ type: "alert", alert: #{ kind: "raid", platform: "twitch", user: "BotanicalQueen", amount: 240 } },
        #{ type: "alert", alert: #{ kind: "redeem", platform: "twitch", user: "ShadowMage", title: "Water the Roses", cost: 1200,
            image: "https://static-cdn.jtvnw.net/custom-reward-images/default-4.png" } },
        #{ type: "alert", alert: #{ kind: "superchat", platform: "youtube", user: "PixelAdventurer", amount: 15.0, currency: "$", raw: #{ amount: "$15.00" }, message: "The 8-bit vibes are legendary!" } },
        #{ type: "alert", alert: #{ kind: "follow", platform: "kick", user: "CrimsonThorn" } },
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

    // Follow 🌹 Rosia Chat look if enabled
    if this.settings.match_chat == true && now - num(this.look_checked) >= 1.0 {
        this.look_checked = now;
        let c = shared_value("rosia_chat_look");
        if is_map(c) && c.stamp != this.style.look_stamp {
            this.style = chat_look(style_of(this.settings), c);
            if !none(this.current) { this.current.laid_ok = false; }
        }
    }

    let st = this.style;
    let w = num(this.size[0]);
    let h = num(this.size[1]);
    let enter = 0.40;
    let leave = 0.40;

    // Dismiss expired alert and transition to next
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

    // Re-layout if needed or image loaded
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

    // Coordinates
    let x = ((w - cw) / 2.0).round();
    let y = if st.position == "top" { 20.0 } else if st.position == "bottom" { h - ch - 20.0 } else { ((h - ch) / 2.0).round() };
    let alpha = 1.0;
    let scale = 1.0;
    let dy = 0.0;

    // Entrance Animation
    if age < enter {
        let t = age / enter;
        if st.animation == "magic" {
            scale = 0.88 + 0.12 * ease_back(t);
            alpha = fmin(1.0, ease_out(t * 1.4));
        } else if st.animation == "pop" {
            let step = (age * 12.0).floor().to_int();
            if step <= 0 { scale = 0.80; alpha = 0.70; }
            else if step == 1 { scale = 0.92; alpha = 0.90; }
            else { scale = 1.0; alpha = 1.0; }
        } else if st.animation == "slide" {
            dy = (if st.position == "bottom" { 60.0 } else { -60.0 }) * (1.0 - ease_out(t));
            alpha = t;
        } else {
            alpha = t;
        }
    } else if left < leave {
        let t = 1.0 - fmax(0.0, left) / leave;
        alpha = 1.0 - ease_out(t);
        if st.animation == "magic" || st.animation == "pop" {
            scale = 1.0 - 0.08 * t;
        } else if st.animation == "slide" {
            dy = (if st.position == "bottom" { 40.0 } else { -40.0 }) * ease_out(t);
        }
    }

    let card = [#{ group: this.current.laid.cmds, x: 0.0, y: 0.0 }];

    // Progress bar along the bottom of the card
    if st.show_progress {
        let frac = fmax(0.0, fmin(1.0, left / st.duration));
        let inset = fmax(st.radius * 0.75, 14.0);
        let bw = (cw - inset * 2.0) * frac;
        if bw > 1.0 {
            card.push(#{ rect: [inset, ch - 8.0, bw, 3.5], fill: this.current.laid.accent, radius: 1.75 });
        }
    }

    let out = [];

    // Base card group
    out.push(#{ group: card, x: x, y: y + dy, scale: scale, origin: [cw / 2.0, ch / 2.0], alpha: alpha * st.opacity });

    // Magic Stars & Magic Dust entrance effect
    if age < 0.85 && (st.animation == "magic" || st.magic_stars) {
        let seed = (this.current.start * 1000.0).floor().to_int() % 100000 + 77;
        for m_cmd in magic_dust_cmds(cw, ch, age, st, seed) {
            let m_grp = m_cmd;
            m_grp.x = x;
            m_grp.y = y + dy;
            m_grp.scale = scale;
            m_grp.origin = [cw / 2.0, ch / 2.0];
            out.push(m_grp);
        }
    }

    out
}
