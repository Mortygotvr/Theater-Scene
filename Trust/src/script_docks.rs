//! Rhai docks: docks in OBS written as Rhai scripts, drawn with egui (in
//! egui_docks' windows).
//!
//! ```text
//! const ID = "my_dock";             // OBS remembers the dock's place by it
//! const NAME = "🎛 My Dock";         // its title, and its entry in Theater's menu
//! const KIND = "dock";
//!
//! fn init() { this.count = 0; }     // optional: once, when the dock starts
//! fn ui(ui) {                       // draws the dock, each time something changed
//!     ui.heading("Hello");
//!     if ui.button("Click me") { this.count += 1; }
//!     ui.label(`Clicked ${this.count} times`);
//! }
//! fn on_event(ev) { … }             // optional: each relay message (chat, alerts…)
//! ```
//!
//! What `ui` draws:
//! - text: `heading(t)`, `label(t)`, `strong(t)`, `small(t)`,
//!   `colored(t, 0xAARRGGBB)`, `separator()`, `space(px)`
//! - widgets, each returning what the user did: `button(t)` and
//!   `wide_button(t)` (clicked), `selectable(t, selected)` (clicked),
//!   `checkbox(t, on)` (on), `slider(t, value, min, max)` (the value: whole
//!   numbers stay whole), `text_box(text)` or `text_box(text, hint)` (the
//!   text; `entered()` is true just after Enter was pressed in it),
//!   `choice(t, value, [options])` (the option picked), `color(t, colour)`
//!   (the colour), `progress(fraction)` or `progress(fraction, t)`
//! - layout: `row()` … `end()` puts things side by side, `section(title)` …
//!   `end()` in a part that folds (it returns whether it's open), `width()`
//!   is the room left, `repaint_after(seconds)` draws it again then (a
//!   clock, an animation; otherwise it's drawn when something changes)
//!
//! What a dock can have Theater do (OBS's part happens on its UI thread,
//! right after): `send_event(#{…})` (to Rhai sources and docks, like the
//! chat: `#{ type: "chat", user: "…", message: "…" }`, `#{ type:
//! "clear_chat" }`), `redeems()` / `trigger_redeem(name)`, `scenes()` /
//! `current_scene()` / `switch_scene(name)`, `show_source(scene, source)` /
//! `hide_source` / `toggle_source` (the current scene's with just the
//! source), `open_studio()`. Plus what every script has: `log`,
//! `share` / `shared_value`, `rgba`, `relay_connected()`…
//!
//! `this` keeps what the dock stores in it, across edits of the script too
//! (a reload runs `init()` again for new values, and keeps the ones it had).

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};

use eframe::egui;
use parking_lot::Mutex;
use rhai::{Array, Dynamic, Engine, EvalAltResult, Map};

use crate::obs_source::trust_log;
use crate::script_filters::{ScriptType, call, event_map};

type R<T> = Result<T, Box<EvalAltResult>>;

// ─── Drawing ────────────────────────────────────────────────────────────────

/// The `ui` a dock's `fn ui(ui)` gets: valid during that call only.
#[derive(Clone)]
pub struct DockUi {
    call: u64,
}

/// Where the next widget goes: the dock itself, or a row or section in it.
enum Level {
    /// The dock's own egui Ui, lent for the ui() call
    Root(*mut egui::Ui),
    /// A row or section, placed in the level below by end()
    Child { ui: Box<egui::Ui>, shown: bool },
}

thread_local! {
    static LEVELS: RefCell<Vec<Level>> = const { RefCell::new(Vec::new()) };
    /// The ui() call drawing now (0: none)
    static DRAWING: Cell<u64> = const { Cell::new(0) };
    /// Enter was just pressed in the last text box
    static ENTERED: Cell<bool> = const { Cell::new(false) };
    /// The soonest repaint_after asked for, in seconds
    static REPAINT_AFTER: Cell<Option<f64>> = const { Cell::new(None) };
}
static CALLS: AtomicU64 = AtomicU64::new(0);

/// Runs `f` on where the next widget goes.
fn top<T>(dock_ui: &DockUi, f: impl FnOnce(&mut egui::Ui) -> T) -> R<T> {
    if dock_ui.call == 0 || DRAWING.get() != dock_ui.call {
        return Err("a dock's `ui` only draws during its ui()".into());
    }
    LEVELS.with(|levels| {
        let mut levels = levels.borrow_mut();
        let ui: &mut egui::Ui = match levels.last_mut() {
            // Lent by draw_ui, which outlives the call
            Some(Level::Root(ui)) => unsafe { &mut **ui },
            Some(Level::Child { ui, .. }) => ui,
            None => return Err("a dock's `ui` only draws during its ui()".into()),
        };
        Ok(f(ui))
    })
}

/// Starts a row or section in the current level.
fn open_level(dock_ui: &DockUi, make: impl FnOnce(&mut egui::Ui) -> (egui::Ui, bool)) -> R<bool> {
    let (child, shown) = top(dock_ui, make)?;
    LEVELS.with(|levels| levels.borrow_mut().push(Level::Child { ui: Box::new(child), shown }));
    Ok(shown)
}

/// Ends the innermost row or section: it takes its room in the level below
/// (a folded section, none). False at the dock's own level.
fn close_level() -> bool {
    LEVELS.with(|levels| {
        let mut levels = levels.borrow_mut();
        if !matches!(levels.last(), Some(Level::Child { .. })) {
            return false;
        }
        let Some(Level::Child { ui: child, shown }) = levels.pop() else { return false };
        if shown {
            let rect = child.min_rect();
            match levels.last_mut() {
                Some(Level::Root(parent)) => {
                    unsafe { (**parent).advance_cursor_after_rect(rect) };
                }
                Some(Level::Child { ui: parent, .. }) => {
                    parent.advance_cursor_after_rect(rect);
                }
                None => {}
            }
        }
        true
    })
}

/// Calls the dock's `fn ui(ui)` drawing into `ui`.
fn draw_ui(ty: &ScriptType, this: &mut Dynamic, ui: &mut egui::Ui) -> Result<(), String> {
    let compiled = ty.compiled();
    let call_id = CALLS.fetch_add(1, Ordering::Relaxed) + 1;
    LEVELS.with(|levels| {
        let mut levels = levels.borrow_mut();
        levels.clear();
        levels.push(Level::Root(ui as *mut egui::Ui));
    });
    DRAWING.set(call_id);
    ENTERED.set(false);
    let result = call(&compiled, Some(this), "ui", (DockUi { call: call_id },));
    // Rows and sections it didn't end
    while close_level() {}
    DRAWING.set(0);
    LEVELS.with(|levels| levels.borrow_mut().clear());
    result.map(|_| ())
}

fn text(value: &Dynamic) -> String {
    if value.is_unit() { String::new() } else { value.to_string() }
}

fn number(value: &Dynamic) -> Option<f64> {
    value.as_float().ok().or_else(|| value.as_int().ok().map(|i| i as f64))
}

/// 0xAARRGGBB
fn color32(argb: i64) -> egui::Color32 {
    let c = argb as u32;
    egui::Color32::from_rgba_unmultiplied((c >> 16) as u8, (c >> 8) as u8, c as u8, (c >> 24) as u8)
}

fn argb(color: egui::Color32) -> i64 {
    let [r, g, b, a] = color.to_srgba_unmultiplied();
    ((a as i64) << 24) | ((r as i64) << 16) | ((g as i64) << 8) | b as i64
}

fn text_box(ui: &mut egui::Ui, value: &Dynamic, hint: Option<String>) -> String {
    let mut s = text(value);
    let mut edit = egui::TextEdit::singleline(&mut s);
    if !ui.layout().is_horizontal() {
        edit = edit.desired_width(ui.available_width());
    }
    if let Some(hint) = hint {
        edit = edit.hint_text(hint);
    }
    let response = ui.add(edit);
    let entered = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
    if entered {
        // Ready for the next one (a chat line…)
        response.request_focus();
    }
    ENTERED.set(entered);
    s
}

fn register_ui(engine: &mut Engine) {
    engine.register_type_with_name::<DockUi>("DockUi");

    // Text
    engine.register_fn("heading", |ui: &mut DockUi, t: Dynamic| top(ui, |ui| {
        ui.heading(text(&t));
    }));
    engine.register_fn("label", |ui: &mut DockUi, t: Dynamic| top(ui, |ui| {
        ui.label(text(&t));
    }));
    engine.register_fn("strong", |ui: &mut DockUi, t: Dynamic| top(ui, |ui| {
        ui.strong(text(&t));
    }));
    engine.register_fn("small", |ui: &mut DockUi, t: Dynamic| top(ui, |ui| {
        ui.small(text(&t));
    }));
    engine.register_fn("colored", |ui: &mut DockUi, t: Dynamic, color: i64| top(ui, |ui| {
        ui.colored_label(color32(color), text(&t));
    }));
    engine.register_fn("separator", |ui: &mut DockUi| top(ui, |ui| {
        ui.separator();
    }));
    engine.register_fn("space", |ui: &mut DockUi, px: Dynamic| top(ui, |ui| {
        ui.add_space(number(&px).unwrap_or(8.0) as f32);
    }));

    // Widgets
    engine.register_fn("button", |ui: &mut DockUi, t: Dynamic| top(ui, |ui| ui.button(text(&t)).clicked()));
    engine.register_fn("wide_button", |ui: &mut DockUi, t: Dynamic| top(ui, |ui| {
        let width = ui.available_width();
        ui.add(egui::Button::new(text(&t)).min_size(egui::vec2(width, 26.0))).clicked()
    }));
    engine.register_fn("selectable", |ui: &mut DockUi, t: Dynamic, selected: bool| top(ui, |ui| {
        ui.selectable_label(selected, text(&t)).clicked()
    }));
    engine.register_fn("checkbox", |ui: &mut DockUi, t: Dynamic, on: Dynamic| top(ui, |ui| {
        let mut on = on.as_bool().unwrap_or(false);
        ui.checkbox(&mut on, text(&t));
        on
    }));
    engine.register_fn("slider", |ui: &mut DockUi, t: Dynamic, value: Dynamic, min: Dynamic, max: Dynamic| top(ui, |ui| {
        let whole = value.is_int() || (value.is_unit() && min.is_int() && max.is_int());
        if whole {
            let (lo, hi) = (min.as_int().unwrap_or(0), max.as_int().unwrap_or(100));
            let mut v = value.as_int().unwrap_or(lo);
            ui.add(egui::Slider::new(&mut v, lo.min(hi)..=hi.max(lo)).text(text(&t)));
            Dynamic::from_int(v)
        } else {
            let (lo, hi) = (number(&min).unwrap_or(0.0), number(&max).unwrap_or(1.0));
            let mut v = number(&value).unwrap_or(lo);
            ui.add(egui::Slider::new(&mut v, lo.min(hi)..=hi.max(lo)).text(text(&t)));
            Dynamic::from_float(v)
        }
    }));
    engine.register_fn("text_box", |ui: &mut DockUi, value: Dynamic| top(ui, |ui| text_box(ui, &value, None)));
    engine.register_fn("text_box", |ui: &mut DockUi, value: Dynamic, hint: Dynamic| top(ui, |ui| {
        text_box(ui, &value, Some(text(&hint)))
    }));
    engine.register_fn("entered", |ui: &mut DockUi| top(ui, |_| ENTERED.get()));
    engine.register_fn("choice", |ui: &mut DockUi, t: Dynamic, value: Dynamic, options: Array| top(ui, |ui| {
        let mut picked = text(&value);
        egui::ComboBox::new(ui.next_auto_id(), text(&t)).selected_text(picked.clone()).show_ui(ui, |ui| {
            for option in &options {
                let o = text(option);
                ui.selectable_value(&mut picked, o.clone(), o);
            }
        });
        options.iter().find(|o| text(o) == picked).cloned().unwrap_or(value)
    }));
    engine.register_fn("color", |ui: &mut DockUi, t: Dynamic, color: i64| top(ui, |ui| {
        let mut c = color32(color);
        ui.horizontal(|ui| {
            ui.color_edit_button_srgba(&mut c);
            ui.label(text(&t));
        });
        argb(c)
    }));
    engine.register_fn("progress", |ui: &mut DockUi, fraction: Dynamic| top(ui, |ui| {
        ui.add(egui::ProgressBar::new(number(&fraction).unwrap_or(0.0) as f32).show_percentage());
    }));
    engine.register_fn("progress", |ui: &mut DockUi, fraction: Dynamic, t: Dynamic| top(ui, |ui| {
        ui.add(egui::ProgressBar::new(number(&fraction).unwrap_or(0.0) as f32).text(text(&t)));
    }));

    // Layout
    engine.register_fn("row", |ui: &mut DockUi| open_level(ui, |parent| {
        // One widget high to start with, like egui's horizontal_wrapped: its
        // widgets are centred on that line (not halfway down the dock), and
        // it grows as it wraps
        let rect = egui::Rect::from_min_size(
            parent.available_rect_before_wrap().min,
            egui::vec2(parent.available_width(), parent.spacing().interact_size.y),
        );
        let layout = egui::Layout::left_to_right(egui::Align::Center).with_main_wrap(true);
        (parent.new_child(egui::UiBuilder::new().max_rect(rect).layout(layout)), true)
    }));
    engine.register_fn("section", |ui: &mut DockUi, title: Dynamic| open_level(ui, |parent| {
        let title = text(&title);
        let id = parent.make_persistent_id(("theater_dock_section", &title));
        let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(parent.ctx(), id, true);
        let clicked = parent
            .horizontal(|ui| {
                state.show_toggle_button(ui, egui::collapsing_header::paint_default_icon);
                ui.add(egui::Label::new(egui::RichText::new(&title).strong()).sense(egui::Sense::click())).clicked()
            })
            .inner;
        if clicked {
            state.toggle(parent);
        }
        state.store(parent.ctx());
        let open = state.is_open();
        let mut rect = parent.available_rect_before_wrap();
        rect.min.x += parent.spacing().indent;
        let mut builder = egui::UiBuilder::new().max_rect(rect).layout(egui::Layout::top_down(egui::Align::Min));
        if !open {
            // Its contents still run, unseen and taking no room
            builder = builder.invisible();
        }
        (parent.new_child(builder), open)
    }));
    engine.register_fn("end", |ui: &mut DockUi| -> R<()> {
        top(ui, |_| ())?;
        if close_level() { Ok(()) } else { Err("end() without a row() or section() to end".into()) }
    });
    engine.register_fn("width", |ui: &mut DockUi| top(ui, |ui| ui.available_width() as f64));
    engine.register_fn("repaint_after", |ui: &mut DockUi, seconds: Dynamic| top(ui, |_| {
        let s = number(&seconds).unwrap_or(1.0).max(0.02);
        REPAINT_AFTER.set(Some(REPAINT_AFTER.get().map_or(s, |r| r.min(s))));
    }));
}

// ─── What a dock can have Theater do ────────────────────────────────────────

fn redeem_name(item: &crate::studio_app::RedeemItem) -> String {
    if item.config.name.trim().is_empty() {
        std::path::Path::new(&item.filename).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
    } else {
        item.config.name.clone()
    }
}

/// A redeem by its name (or file name), as the Redeems dock's button does.
fn trigger_redeem(name: &str) -> bool {
    let found = crate::studio_app::GLOBAL_REDEEMS
        .read()
        .iter()
        .find(|r| redeem_name(r) == name || r.filename == name)
        .map(|r| (r.filename.clone(), r.config.clone()));
    let Some((filename, config)) = found else {
        trust_log(&format!("[Rhai] ⚠️ trigger_redeem: no redeem \"{}\"", name));
        return false;
    };
    crate::studio_app::build_redeem_dispatch_json(&filename, &config).is_some_and(|json| crate::studio_app::dispatch_test_redeem_json(&json))
}

/// OBS's scenes. The list is read on OBS's UI thread, asked for at most every
/// two seconds (egui_docks also has it read when scenes change).
fn scenes() -> Array {
    static ASKED: Mutex<Option<std::time::Instant>> = Mutex::new(None);
    {
        let mut asked = ASKED.lock();
        if asked.is_none_or(|t| t.elapsed() > std::time::Duration::from_secs(2)) {
            *asked = Some(std::time::Instant::now());
            crate::obs_source::refresh_obs_scenes();
        }
    }
    crate::state::get_state().studio_available_scenes.read().iter().map(|s| Dynamic::from(s.clone())).collect()
}

fn current_scene() -> String {
    crate::obs_source::current_obs_scene_name().unwrap_or_else(|| crate::state::get_state().studio_scene_name.read().clone())
}

/// Shows, hides or toggles a source, on OBS's UI thread (an empty scene: the
/// current one).
fn source_visibility(scene: &str, source: &str, action: &'static str) -> bool {
    let task = Box::new((scene.to_string(), source.to_string(), action));
    let param = Box::into_raw(task) as *mut c_void;
    if crate::obs_source::queue_ui_task(run_source_visibility, param) {
        true
    } else {
        drop(unsafe { Box::from_raw(param as *mut (String, String, &'static str)) });
        false
    }
}

unsafe extern "C" fn run_source_visibility(param: *mut c_void) {
    let (scene, source, action) = *unsafe { Box::from_raw(param as *mut (String, String, &'static str)) };
    let scene = if scene.is_empty() { crate::obs_source::current_obs_scene_name().unwrap_or_default() } else { scene };
    if let Err(e) = crate::item_fx::set_visibility(&scene, &source, action, "", 0, "") {
        trust_log(&format!("[Rhai] ⚠️ {}_source: {}", action, e));
    }
}

fn register_actions(engine: &mut Engine) {
    engine.register_fn("send_event", |event: Map| crate::script_feed::publish(&rhai::format_map_as_json(&event)));
    engine.register_fn("redeems", || -> Array {
        crate::studio_app::GLOBAL_REDEEMS.read().iter().map(|r| Dynamic::from(redeem_name(r))).collect()
    });
    engine.register_fn("trigger_redeem", |name: &str| trigger_redeem(name));
    engine.register_fn("scenes", scenes);
    engine.register_fn("current_scene", current_scene);
    engine.register_fn("switch_scene", |name: &str| crate::obs_source::switch_obs_current_scene(name));
    for (name, action) in [("show_source", "show"), ("hide_source", "hide"), ("toggle_source", "toggle")] {
        engine.register_fn(name, move |scene: &str, source: &str| source_visibility(scene, source, action));
        engine.register_fn(name, move |source: &str| source_visibility("", source, action));
    }
    engine.register_fn("open_studio", crate::studio_app::launch_studio_native);
}

/// The dock functions, in the engine every script runs in.
pub fn register(engine: &mut Engine) {
    register_ui(engine);
    register_actions(engine);
}

// ─── One dock ───────────────────────────────────────────────────────────────

pub struct DockScript {
    ty: &'static ScriptType,
    state: Mutex<DockState>,
    events: Mutex<VecDeque<Arc<str>>>,
    subscribed: AtomicBool,
    /// Its egui window, once drawn: woken up by relay messages
    ctx: OnceLock<egui::Context>,
    viewport: egui::ViewportId,
}

struct DockState {
    this: Dynamic,
    /// The script version `this` was made with
    version: u64,
    error: Option<String>,
    /// The last error written to the log (each is written once)
    logged: Option<String>,
}

/// A new `this`: `init()` fills it.
fn fresh_this(ty: &ScriptType) -> (Dynamic, Option<String>) {
    let compiled = ty.compiled();
    let mut this = Dynamic::from_map(Map::new());
    let error = if compiled.has("init") { call(&compiled, Some(&mut this), "init", ()).err() } else { None };
    (this, error)
}

impl DockScript {
    /// Its dock's script, drawing into the window egui_docks gives `viewport`.
    pub fn new(ty: &'static ScriptType, viewport: egui::ViewportId) -> Arc<Self> {
        let (this, error) = fresh_this(ty);
        let dock = Arc::new(DockScript {
            ty,
            state: Mutex::new(DockState { this, version: ty.version(), error, logged: None }),
            events: Mutex::new(VecDeque::new()),
            subscribed: AtomicBool::new(false),
            ctx: OnceLock::new(),
            viewport,
        });
        if ty.compiled().has("on_event") {
            dock.subscribe();
        }
        dock
    }

    /// Relay messages, from now on: queued for on_event, and the dock woken.
    fn subscribe(self: &Arc<Self>) {
        if self.subscribed.swap(true, Ordering::SeqCst) {
            return;
        }
        let rx = crate::script_feed::subscribe();
        let dock = Arc::downgrade(self);
        let spawned = std::thread::Builder::new().name("theater-dock-events".into()).spawn(move || {
            while let Ok(message) = rx.recv() {
                let Some(dock) = dock.upgrade() else { break };
                {
                    let mut events = dock.events.lock();
                    if events.len() >= 256 {
                        events.pop_front();
                    }
                    events.push_back(message);
                }
                if let Some(ctx) = dock.ctx.get() {
                    ctx.request_repaint_of(dock.viewport);
                }
            }
        });
        if let Err(e) = spawned {
            trust_log(&format!("[Rhai] ⚠️ {}: no relay messages ({})", self.ty.id(), e));
        }
    }

    /// Draws the dock (the Studio's thread, in its window's frame).
    pub fn draw(self: &Arc<Self>, ctx: &egui::Context) {
        let _ = self.ctx.set(ctx.clone());
        let compiled = self.ty.compiled();
        let mut st = self.state.lock();

        // An edited script: init() again, keeping what the dock had
        let version = self.ty.version();
        if st.version != version {
            st.version = version;
            let (mut this, error) = fresh_this(self.ty);
            if let (Some(mut new), Some(old)) = (this.write_lock::<Map>(), st.this.read_lock::<Map>()) {
                for (key, value) in old.iter() {
                    new.insert(key.clone(), value.clone());
                }
            }
            st.this = this;
            st.error = error;
        }

        // Relay messages (a few at a time: the rest next frame)
        if compiled.has("on_event") {
            self.subscribe();
            let batch: Vec<Arc<str>> = {
                let mut events = self.events.lock();
                let n = events.len().min(32);
                events.drain(..n).collect()
            };
            if !self.events.lock().is_empty() {
                ctx.request_repaint();
            }
            for message in batch {
                if let Err(e) = call(&compiled, Some(&mut st.this), "on_event", (event_map(&message),)) {
                    st.error = Some(e);
                }
            }
        }

        REPAINT_AFTER.set(None);
        let mut drawn = Ok(());
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                drawn = draw_ui(self.ty, &mut st.this, ui);
                let problem = drawn.as_ref().err().or(st.error.as_ref());
                if let Some(problem) = problem {
                    ui.separator();
                    ui.colored_label(egui::Color32::from_rgb(0xf8, 0x71, 0x71), format!("⚠ {}", problem));
                }
            });
        });
        if let Err(e) = drawn {
            st.error = Some(e);
        } else if st.error.as_deref().is_some_and(|e| e.starts_with("ui()")) {
            // Drawn fine again after an edit
            st.error = None;
        }
        if st.error.is_some() && st.logged != st.error {
            trust_log(&format!("[Rhai] ⚠️ dock {}: {}", self.ty.id(), st.error.as_deref().unwrap_or_default()));
            st.logged = st.error.clone();
        }
        if let Some(seconds) = REPAINT_AFTER.take() {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(seconds));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A dock from `source`, written to a file of its own.
    fn make_dock(name: &str, source: &str) -> (Arc<DockScript>, std::path::PathBuf) {
        let path = std::env::temp_dir().join(format!("theater_dock_test_{}_{}.rhai", name, std::process::id()));
        std::fs::write(&path, source).unwrap();
        let ty = crate::script_filters::test_type(&path).unwrap();
        (DockScript::new(ty, egui::ViewportId::ROOT), path)
    }

    /// One frame of the dock, as its window would draw it: what it wrote.
    fn frame(dock: &Arc<DockScript>, ctx: &egui::Context) -> String {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(320.0, 900.0))),
            ..Default::default()
        };
        let output = ctx.run(input, |ctx| dock.draw(ctx));
        fn texts(shape: &egui::Shape, out: &mut Vec<String>) {
            match shape {
                egui::Shape::Text(t) => out.push(t.galley.text().to_string()),
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| texts(s, out)),
                _ => {}
            }
        }
        let mut out = Vec::new();
        output.shapes.iter().for_each(|s| texts(&s.shape, &mut out));
        out.join("|")
    }

    fn value(dock: &DockScript, key: &str) -> Dynamic {
        dock.state.lock().this.read_lock::<Map>().unwrap().get(key).cloned().unwrap_or(Dynamic::UNIT)
    }

    fn error(dock: &DockScript) -> Option<String> {
        dock.state.lock().error.clone()
    }

    #[test]
    fn a_dock_draws_its_widgets_and_keeps_its_values() {
        let (dock, path) = make_dock("widgets", r#"
            const ID = "widgets"; const NAME = "Widgets"; const KIND = "dock";
            fn init() { this.speed = 5; this.name = "Morty"; this.frames = 0; }
            fn ui(ui) {
                this.frames += 1;
                ui.heading("Hello dock");
                ui.row(); ui.label("left"); ui.label("right"); ui.end();
                ui.section("Things");
                if ui.button("Click") { this.clicked = true; }
                ui.end();
                this.speed = ui.slider("Speed", this.speed, 0, 10);
                this.level = ui.slider("Level", 0.5, 0.0, 1.0);
                this.name = ui.text_box(this.name);
                this.pick = ui.choice("Pick", "b", ["a", "b"]);
                this.on = ui.checkbox("On", true);
                this.tint = ui.color("Tint", 0xFF336699);
                ui.progress(0.25);
                ui.row(); ui.label("never ended");
            }
        "#);
        let ctx = egui::Context::default();
        let drawn = frame(&dock, &ctx);
        assert_eq!(error(&dock), None);
        for text in ["Hello dock", "left", "right", "Things", "Click", "Speed", "Morty", "Pick", "On", "Tint", "never ended"] {
            assert!(drawn.contains(text), "{} in {}", text, drawn);
        }
        assert_eq!(value(&dock, "speed").as_int(), Ok(5), "a whole number stays whole");
        assert_eq!(value(&dock, "level").as_float(), Ok(0.5));
        assert_eq!(value(&dock, "pick").to_string(), "b");
        assert_eq!(value(&dock, "on").as_bool(), Ok(true));
        assert_eq!(value(&dock, "tint").as_int(), Ok(0xFF336699));
        assert!(value(&dock, "clicked").is_unit(), "nothing was clicked");
        frame(&dock, &ctx);
        assert_eq!(value(&dock, "frames").as_int(), Ok(2), "`this` is kept between frames");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn a_ui_kept_for_later_and_a_stray_end_are_refused() {
        let (dock, path) = make_dock("misuse", r#"
            const ID = "misuse"; const KIND = "dock";
            fn ui(ui) {
                if type_of(this.kept) != "()" { this.kept.label("late"); }
                this.kept = ui;
            }
        "#);
        let ctx = egui::Context::default();
        frame(&dock, &ctx);
        assert_eq!(error(&dock), None);
        let drawn = frame(&dock, &ctx);
        let e = error(&dock).expect("the kept ui is refused");
        assert!(e.contains("only draws during its ui()"), "{}", e);
        assert!(drawn.contains("⚠"), "the dock shows it: {}", drawn);
        let _ = std::fs::remove_file(path);

        let (dock, path) = make_dock("stray_end", r#"
            const ID = "stray_end"; const KIND = "dock";
            fn ui(ui) { ui.label("a"); ui.end(); }
        "#);
        frame(&dock, &ctx);
        let e = error(&dock).expect("end() with nothing to end");
        assert!(e.contains("end() without"), "{}", e);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn an_edited_dock_keeps_what_it_had_and_gets_what_is_new() {
        let (dock, path) = make_dock("reload", r#"
            const ID = "reload"; const KIND = "dock";
            fn init() { this.count = 0; }
            fn ui(ui) { this.count += 1; ui.label(`count ${this.count}`); }
        "#);
        let ctx = egui::Context::default();
        frame(&dock, &ctx);
        frame(&dock, &ctx);
        assert_eq!(value(&dock, "count").as_int(), Ok(2));

        std::fs::write(&path, r#"
            const ID = "reload"; const KIND = "dock";
            fn init() { this.count = 100; this.label = "new"; }
            fn ui(ui) { ui.label(`${this.label} ${this.count}`); }
        "#).unwrap();
        crate::script_filters::test_reload(dock.ty);
        let drawn = frame(&dock, &ctx);
        assert!(drawn.contains("new 2"), "the count it had, the label init() added: {}", drawn);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn the_theater_deck_draws() {
        let path = crate::studio_app::get_filters_dir().join("theater_deck.rhai");
        if !path.is_file() {
            return;
        }
        let ty = crate::script_filters::test_type(&path).unwrap();
        assert_eq!(ty.compiled().kind, crate::script_filters::ScriptKind::Dock);
        let dock = DockScript::new(ty, egui::ViewportId::ROOT);
        let drawn = frame(&dock, &egui::Context::default());
        assert_eq!(error(&dock), None);
        for text in ["Scenes", "Redeems", "Chat", "Send", "Clear the chat"] {
            assert!(drawn.contains(text), "{} in {}", text, drawn);
        }
    }
}
