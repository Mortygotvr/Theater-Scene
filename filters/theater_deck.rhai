// 🎛 Theater Deck: a dock written in Rhai (Theater's menu → 🎛 Theater Deck).
//
// Make your own: copy this file, give it another ID and NAME, and restart
// OBS (a new dock appears when OBS starts; after that, what you save shows
// straight away). What `ui` can draw and what a dock can do are listed at
// the top of Trust/src/script_docks.rs.

const ID = "theater_deck";
const NAME = "🎛 Theater Deck";
const KIND = "dock";

fn init() {
    this.message = "";
}

fn ui(ui) {
    // Scenes: the current one lit, a click switches to it
    ui.section("🎬 Scenes");
    let current = current_scene();
    let list = scenes();
    if list.is_empty() {
        ui.small("Reading OBS's scenes…");
        ui.repaint_after(0.5);
    }
    for scene in list {
        if ui.selectable(scene, scene == current) {
            switch_scene(scene);
        }
    }
    ui.end();

    // Redeems: one button each, like the Studio's Test button
    ui.section("🎁 Redeems");
    let list = redeems();
    if list.is_empty() {
        ui.small("No redeems yet: make one in the Studio.");
    }
    for name in list {
        if ui.wide_button(name) {
            trigger_redeem(name);
        }
    }
    ui.end();

    // The 💬 Theater Chat: a line to it, and a way to clear it
    ui.section("💬 Chat");
    this.message = ui.text_box(this.message, "A message for the chat…");
    let send = ui.entered();
    ui.row();
    if ui.button("📨 Send") {
        send = true;
    }
    if ui.button("🧹 Clear the chat") {
        send_event(#{ type: "clear_chat" });
    }
    ui.end();
    let text = this.message;
    text.trim();
    if send && text != "" {
        send_event(#{ type: "chat", source: "Twitch", user: "Theater Deck", color: "#7dd3fc", message: text });
        this.message = "";
    }
    ui.small(if relay_connected() { "📡 Relay connected" } else { "📡 No relay: only Theater sees these" });
    ui.end();
}
