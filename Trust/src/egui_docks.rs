//! Docks drawn with egui, like the Studio, inside OBS's own docks.
//!
//! A process gets one window event loop, and the Studio's thread has it: each
//! dock is one more window of that thread (a deferred egui viewport), drawn
//! when it has something new to show, whether the Studio is open or not.
//!
//! OBS gets a dock of Theater's while it loads (trust_ui_create_dock: its
//! saved place and size come back like the other docks'). Once the egui window
//! exists, trust_qt puts it inside (trust_ui_embed_native_window), and takes
//! it back out when OBS closes (trust_ui_release_native_window), before Qt
//! takes the docks down.
//!
//! Each dock is a Rhai script (`const KIND = "dock"`, script_docks), found
//! with the other scripts when OBS starts.
//!
//! Windows only for now: X11 could embed the same way, Wayland can't.

use std::ffi::{CString, c_void};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};

use eframe::egui;
use parking_lot::Mutex;

use crate::obs_source::{
    OBS_FRONTEND_EVENT_EXIT, OBS_TASK_UI, ObsFrontendEventCbRegFn, ObsQueueTaskFn, TrustUiAddMenuActionFn,
    TrustUiCreateDockFn, TrustUiShowDockFn, frontend_sym, open_frontend_api, open_libobs_core, open_trust_qt_lib,
    trust_log,
};

type EmbedFn = unsafe extern "C" fn(container_handle: u32, native_window: usize) -> bool;
type ReleaseFn = unsafe extern "C" fn(container_handle: u32);

pub struct Dock {
    id: String,
    title: String,
    /// trust_qt's handle of the OBS dock it's shown in
    handle: u32,
    /// Its egui window, once there
    hwnd: AtomicIsize,
    /// Has its entry in Theater's menu
    in_menu: AtomicBool,
    ui: Box<dyn Fn(&egui::Context) + Send + Sync>,
}

static DOCKS: Mutex<Vec<Arc<Dock>>> = Mutex::new(Vec::new());
/// Theater's menu, once OBS has it (registering it clears Theater's items,
/// so the docks' entries go in after)
static MENU: Mutex<Option<TrustUiAddMenuActionFn>> = Mutex::new(None);
/// OBS is closing: the windows are out of their docks, and go
static CLOSING: AtomicBool = AtomicBool::new(false);
/// The Studio's egui, once the docks are drawn: OBS's scene changes redraw them
static EGUI: std::sync::OnceLock<egui::Context> = std::sync::OnceLock::new();

/// What the egui window is called: how it's found, before it's in its dock
fn window_title(id: &str) -> String {
    format!("Theater dock · {}", id)
}

/// The egui window of the dock `id`
fn viewport_id(id: &str) -> egui::ViewportId {
    egui::ViewportId::from_hash_of(("theater_dock", id))
}

/// Makes the egui docks, on OBS's UI thread while it loads (with Theater's
/// other docks). Nothing happens without a trust_qt that can embed windows.
pub fn create_docks(create_dock: TrustUiCreateDockFn, show_dock: TrustUiShowDockFn) {
    if !cfg!(target_os = "windows") {
        return;
    }
    let scripts: Vec<_> = crate::script_filters::types()
        .into_iter()
        .filter(|t| t.compiled().kind == crate::script_filters::ScriptKind::Dock)
        .collect();
    if scripts.is_empty() {
        return;
    }
    if unsafe { frontend_sym(open_trust_qt_lib(), b"trust_ui_embed_native_window\0") }.is_null() {
        trust_log("[Trust Core] ⚠️ This trust_qt can't hold egui docks (no trust_ui_embed_native_window)");
        return;
    }

    for ty in scripts {
        // Apart from other plugins' docks (OBS keeps them all by id)
        let id = format!("theater_rhai_{}", ty.id());
        let script = crate::script_docks::DockScript::new(ty, viewport_id(&id));
        add_dock(create_dock, show_dock, &id, &ty.compiled().name, Box::new(move |ctx| script.draw(ctx)));
    }
    if DOCKS.lock().is_empty() {
        return;
    }
    add_menu_entries();

    let add_cb_ptr = unsafe { frontend_sym(open_frontend_api(), b"obs_frontend_add_event_callback\0") };
    if !add_cb_ptr.is_null() {
        let add_cb: ObsFrontendEventCbRegFn = unsafe { std::mem::transmute(add_cb_ptr) };
        unsafe { add_cb(frontend_event, std::ptr::null_mut()) };
    }
    // The docks' windows belong to the Studio's thread: it runs from now on,
    // its own window hidden until it's opened
    crate::studio_app::start_studio_hidden();
    let _ = std::thread::Builder::new().name("trust-dock-finder".into()).spawn(find_windows);
}

fn add_dock(
    create_dock: TrustUiCreateDockFn,
    show_dock: TrustUiShowDockFn,
    id: &str,
    title: &str,
    ui: Box<dyn Fn(&egui::Context) + Send + Sync>,
) {
    let (Ok(c_id), Ok(c_title)) = (CString::new(id), CString::new(title)) else { return };
    let handle = unsafe { create_dock(c_id.as_ptr(), c_title.as_ptr()) };
    if handle == 0 {
        trust_log(&format!("[Trust Core] ⚠️ trust_ui_create_dock returned a null handle ({})", id));
        return;
    }
    unsafe { show_dock(handle) };
    DOCKS.lock().push(Arc::new(Dock {
        id: id.to_string(),
        title: title.to_string(),
        handle,
        hwnd: AtomicIsize::new(0),
        in_menu: AtomicBool::new(false),
        ui,
    }));
    trust_log(&format!("[Trust Core] 🧩 egui dock '{}' created; its window follows once the Studio's thread is up", id));
}

/// Theater's menu is there (OBS's UI thread, like create_docks: whichever
/// comes second adds the docks' entries).
pub fn menu_ready(add_action: TrustUiAddMenuActionFn) {
    *MENU.lock() = Some(add_action);
    add_menu_entries();
}

/// An entry in Theater's menu for each dock that has none yet: it shows the
/// dock (brought back if it was closed).
fn add_menu_entries() {
    let Some(add_action) = *MENU.lock() else { return };
    for dock in DOCKS.lock().iter() {
        if dock.in_menu.swap(true, Ordering::SeqCst) {
            continue;
        }
        let Ok(title) = CString::new(dock.title.as_str()) else { continue };
        unsafe { add_action(title.as_ptr(), show_dock_menu_cb, dock.handle as usize as *mut c_void) };
    }
}

unsafe extern "C" fn show_dock_menu_cb(handle: *mut c_void) {
    crate::obs_source::show_dock_by_handle(handle as usize as u32);
}

/// Keeps the docks' windows open: from the Studio's update, every frame of
/// its own, before anything else (hidden, it has frames only at start).
pub fn show(ctx: &egui::Context) {
    if CLOSING.load(Ordering::SeqCst) {
        return;
    }
    let _ = EGUI.set(ctx.clone());
    let docks: Vec<Arc<Dock>> = DOCKS.lock().clone();
    for dock in docks {
        // Hidden until Qt shows it in its dock; a borderless window with no
        // taskbar button meanwhile
        let builder = egui::ViewportBuilder::default()
            .with_title(window_title(&dock.id))
            .with_decorations(false)
            .with_taskbar(false)
            .with_visible(false)
            .with_inner_size([320.0, 420.0]);
        ctx.show_viewport_deferred(viewport_id(&dock.id), builder, move |ctx, _class| {
            take_keyboard_on_click(ctx, &dock);
            (dock.ui)(ctx)
        });
    }
}

/// A click reaches the dock's window, but the keyboard stays with Qt's: a
/// window inside another one isn't given it the way a window of its own is.
/// The dock takes it when clicked (text boxes need it), and Qt takes it back
/// the same way when one of its widgets is clicked.
fn take_keyboard_on_click(ctx: &egui::Context, dock: &Dock) {
    #[cfg(target_os = "windows")]
    {
        unsafe extern "system" {
            fn GetFocus() -> isize;
            fn SetFocus(hwnd: isize) -> isize;
        }
        let hwnd = dock.hwnd.load(Ordering::SeqCst);
        if hwnd != 0 && ctx.input(|i| i.pointer.any_pressed()) && unsafe { GetFocus() } != hwnd {
            unsafe { SetFocus(hwnd) };
        }
    }
    #[cfg(not(target_os = "windows"))]
    let _ = (ctx, dock);
}

/// Waits for the docks' windows to exist, then hands each to OBS's UI thread
/// to put in its dock.
fn find_windows() {
    #[cfg(target_os = "windows")]
    {
        unsafe extern "system" {
            fn FindWindowW(class_name: *const u16, window_name: *const u16) -> isize;
        }
        let queue_task_ptr = unsafe { frontend_sym(open_libobs_core(), b"obs_queue_task\0") };
        if queue_task_ptr.is_null() {
            trust_log("[Trust Core] ⚠️ obs_queue_task not found: the egui docks stay empty");
            return;
        }
        let queue_task: ObsQueueTaskFn = unsafe { std::mem::transmute(queue_task_ptr) };
        let started = std::time::Instant::now();
        loop {
            let waiting: Vec<Arc<Dock>> =
                DOCKS.lock().iter().filter(|d| d.hwnd.load(Ordering::SeqCst) == 0).cloned().collect();
            if waiting.is_empty() || CLOSING.load(Ordering::SeqCst) {
                return;
            }
            if started.elapsed() > std::time::Duration::from_secs(60) {
                for dock in waiting {
                    trust_log(&format!("[Trust Core] ⚠️ egui dock '{}': its window never appeared", dock.id));
                }
                return;
            }
            for dock in waiting {
                let title: Vec<u16> = window_title(&dock.id).encode_utf16().chain(std::iter::once(0)).collect();
                let hwnd = unsafe { FindWindowW(std::ptr::null(), title.as_ptr()) };
                if hwnd != 0 {
                    dock.hwnd.store(hwnd, Ordering::SeqCst);
                    let param = Arc::into_raw(dock) as *mut c_void;
                    unsafe { queue_task(OBS_TASK_UI, embed_on_ui_thread, param, false) };
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }
}

unsafe extern "C" fn embed_on_ui_thread(param: *mut c_void) {
    let dock = unsafe { Arc::from_raw(param as *const Dock) };
    if CLOSING.load(Ordering::SeqCst) {
        return;
    }
    let embed_ptr = unsafe { frontend_sym(open_trust_qt_lib(), b"trust_ui_embed_native_window\0") };
    if embed_ptr.is_null() {
        return;
    }
    let embed: EmbedFn = unsafe { std::mem::transmute(embed_ptr) };
    let ok = unsafe { embed(dock.handle, dock.hwnd.load(Ordering::SeqCst) as usize) };
    trust_log(&format!("[Trust Core] 🧩 egui dock '{}' {}", dock.id, if ok { "is in its dock" } else { "couldn't be put in its dock" }));
}

/// OBS delivers frontend events on the UI thread.
unsafe extern "C" fn frontend_event(event: i32, _private_data: *mut c_void) {
    // Scene changed, scene list changed, other scene collection, OBS ready:
    // the scenes are read again (a UI task, after this one), then the docks
    // redrawn with them
    const SCENE_CHANGED: i32 = 8;
    const SCENE_LIST_CHANGED: i32 = 9;
    const SCENE_COLLECTION_CHANGED: i32 = 13;
    const FINISHED_LOADING: i32 = 26;
    if matches!(event, SCENE_CHANGED | SCENE_LIST_CHANGED | SCENE_COLLECTION_CHANGED | FINISHED_LOADING) {
        crate::obs_source::refresh_obs_scenes();
        if let Some(ctx) = EGUI.get() {
            for dock in DOCKS.lock().iter() {
                ctx.request_repaint_after_for(std::time::Duration::from_millis(150), viewport_id(&dock.id));
            }
        }
        return;
    }
    if event != OBS_FRONTEND_EVENT_EXIT {
        return;
    }
    CLOSING.store(true, Ordering::SeqCst);
    let release_ptr = unsafe { frontend_sym(open_trust_qt_lib(), b"trust_ui_release_native_window\0") };
    if !release_ptr.is_null() {
        let release: ReleaseFn = unsafe { std::mem::transmute(release_ptr) };
        for dock in DOCKS.lock().iter() {
            if dock.hwnd.load(Ordering::SeqCst) != 0 {
                unsafe { release(dock.handle) };
            }
        }
    }
    let remove_ptr = unsafe { frontend_sym(open_frontend_api(), b"obs_frontend_remove_event_callback\0") };
    if !remove_ptr.is_null() {
        let remove: ObsFrontendEventCbRegFn = unsafe { std::mem::transmute(remove_ptr) };
        unsafe { remove(frontend_event, std::ptr::null_mut()) };
    }
}

