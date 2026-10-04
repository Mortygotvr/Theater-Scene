//! A filter's OBS properties — what its OBS Filters window shows — for the
//! Studio's filter list: the same controls, labels, ranges and dropdowns
//! (a Theatre 360 Layer's "Select Master Viewer" list, a Scale filter's
//! sampling list…). Edits are made the way that window makes them: the value
//! goes into the filter's settings, then the property's own reaction runs (its
//! modified callback: a Layer's "Remote Master Control" moving its Master, a
//! choice showing or hiding other properties), then the filter updates.

use std::ffi::{CStr, CString, c_char, c_void};
use std::sync::OnceLock;

use crate::obs_source::{obs_symbol, studio_key_source_name};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ListFormat {
    Int,
    Float,
    String,
    Bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ListItem {
    pub label: String,
    pub value: serde_json::Value,
    pub disabled: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    Bool,
    Int { min: i64, max: i64, step: i64, slider: bool },
    Float { min: f64, max: f64, step: f64, slider: bool },
    Text { password: bool, multiline: bool, info: bool },
    Path,
    List { format: ListFormat, items: Vec<ListItem> },
    /// 0xAABBGGRR, as OBS stores colors.
    Color { alpha: bool },
    Group { checkable: bool, children: Vec<Prop> },
    /// Font, editable list, frame rate, button: edited in OBS's own window.
    Other,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Prop {
    /// The setting it edits.
    pub name: String,
    pub label: String,
    pub enabled: bool,
    pub kind: Kind,
}

// ─── libobs ──────────────────────────────────────────────────────────────────

type P = *mut c_void;

struct Fns {
    get_source_by_name: unsafe extern "C" fn(*const c_char) -> P,
    source_release: unsafe extern "C" fn(P),
    get_filter_by_name: unsafe extern "C" fn(P, *const c_char) -> P,
    source_properties: unsafe extern "C" fn(P) -> P,
    properties_destroy: unsafe extern "C" fn(P),
    properties_first: unsafe extern "C" fn(P) -> P,
    property_next: unsafe extern "C" fn(*mut P) -> bool,
    properties_get: unsafe extern "C" fn(P, *const c_char) -> P,
    property_name: unsafe extern "C" fn(P) -> *const c_char,
    property_description: unsafe extern "C" fn(P) -> *const c_char,
    property_get_type: unsafe extern "C" fn(P) -> i32,
    property_visible: unsafe extern "C" fn(P) -> bool,
    property_enabled: unsafe extern "C" fn(P) -> bool,
    int_min: unsafe extern "C" fn(P) -> i32,
    int_max: unsafe extern "C" fn(P) -> i32,
    int_step: unsafe extern "C" fn(P) -> i32,
    int_type: unsafe extern "C" fn(P) -> i32,
    float_min: unsafe extern "C" fn(P) -> f64,
    float_max: unsafe extern "C" fn(P) -> f64,
    float_step: unsafe extern "C" fn(P) -> f64,
    float_type: unsafe extern "C" fn(P) -> i32,
    text_type: unsafe extern "C" fn(P) -> i32,
    list_format: unsafe extern "C" fn(P) -> i32,
    list_item_count: unsafe extern "C" fn(P) -> usize,
    list_item_name: unsafe extern "C" fn(P, usize) -> *const c_char,
    list_item_string: unsafe extern "C" fn(P, usize) -> *const c_char,
    list_item_int: unsafe extern "C" fn(P, usize) -> i64,
    list_item_float: unsafe extern "C" fn(P, usize) -> f64,
    list_item_bool: unsafe extern "C" fn(P, usize) -> bool,
    list_item_disabled: unsafe extern "C" fn(P, usize) -> bool,
    group_type: unsafe extern "C" fn(P) -> i32,
    group_content: unsafe extern "C" fn(P) -> P,
    property_modified: unsafe extern "C" fn(P, P) -> bool,
    source_get_settings: unsafe extern "C" fn(P) -> P,
    source_update: unsafe extern "C" fn(P, P),
    data_release: unsafe extern "C" fn(P),
    data_set_int: unsafe extern "C" fn(P, *const c_char, i64),
    data_set_double: unsafe extern "C" fn(P, *const c_char, f64),
    data_set_bool: unsafe extern "C" fn(P, *const c_char, bool),
    data_set_string: unsafe extern "C" fn(P, *const c_char, *const c_char),
}

// Function pointers into libobs, which outlives us.
unsafe impl Send for Fns {}
unsafe impl Sync for Fns {}

fn fns() -> Option<&'static Fns> {
    static FNS: OnceLock<Option<Fns>> = OnceLock::new();
    FNS.get_or_init(|| {
        fn sym<T>(name: &CStr) -> Option<T> {
            let p = obs_symbol(name);
            // SAFETY: T is the function pointer type of the libobs export `name`.
            (!p.is_null()).then(|| unsafe { std::mem::transmute_copy(&p) })
        }
        Some(Fns {
            get_source_by_name: sym(c"obs_get_source_by_name")?,
            source_release: sym(c"obs_source_release")?,
            get_filter_by_name: sym(c"obs_source_get_filter_by_name")?,
            source_properties: sym(c"obs_source_properties")?,
            properties_destroy: sym(c"obs_properties_destroy")?,
            properties_first: sym(c"obs_properties_first")?,
            property_next: sym(c"obs_property_next")?,
            properties_get: sym(c"obs_properties_get")?,
            property_name: sym(c"obs_property_name")?,
            property_description: sym(c"obs_property_description")?,
            property_get_type: sym(c"obs_property_get_type")?,
            property_visible: sym(c"obs_property_visible")?,
            property_enabled: sym(c"obs_property_enabled")?,
            int_min: sym(c"obs_property_int_min")?,
            int_max: sym(c"obs_property_int_max")?,
            int_step: sym(c"obs_property_int_step")?,
            int_type: sym(c"obs_property_int_type")?,
            float_min: sym(c"obs_property_float_min")?,
            float_max: sym(c"obs_property_float_max")?,
            float_step: sym(c"obs_property_float_step")?,
            float_type: sym(c"obs_property_float_type")?,
            text_type: sym(c"obs_property_text_type")?,
            list_format: sym(c"obs_property_list_format")?,
            list_item_count: sym(c"obs_property_list_item_count")?,
            list_item_name: sym(c"obs_property_list_item_name")?,
            list_item_string: sym(c"obs_property_list_item_string")?,
            list_item_int: sym(c"obs_property_list_item_int")?,
            list_item_float: sym(c"obs_property_list_item_float")?,
            list_item_bool: sym(c"obs_property_list_item_bool")?,
            list_item_disabled: sym(c"obs_property_list_item_disabled")?,
            group_type: sym(c"obs_property_group_type")?,
            group_content: sym(c"obs_property_group_content")?,
            property_modified: sym(c"obs_property_modified")?,
            source_get_settings: sym(c"obs_source_get_settings")?,
            source_update: sym(c"obs_source_update")?,
            data_release: sym(c"obs_data_release")?,
            data_set_int: sym(c"obs_data_set_int")?,
            data_set_double: sym(c"obs_data_set_double")?,
            data_set_bool: sym(c"obs_data_set_bool")?,
            data_set_string: sym(c"obs_data_set_string")?,
        })
    })
    .as_ref()
}

// obs_property_type
const PROPERTY_BOOL: i32 = 1;
const PROPERTY_INT: i32 = 2;
const PROPERTY_FLOAT: i32 = 3;
const PROPERTY_TEXT: i32 = 4;
const PROPERTY_PATH: i32 = 5;
const PROPERTY_LIST: i32 = 6;
const PROPERTY_COLOR: i32 = 7;
const PROPERTY_GROUP: i32 = 12;
const PROPERTY_COLOR_ALPHA: i32 = 13;
// obs_combo_format
const COMBO_FORMAT_INT: i32 = 1;
const COMBO_FORMAT_FLOAT: i32 = 2;
const COMBO_FORMAT_STRING: i32 = 3;
const COMBO_FORMAT_BOOL: i32 = 4;
// obs_number_type, obs_text_type, obs_group_type
const NUMBER_SLIDER: i32 = 1;
const TEXT_PASSWORD: i32 = 1;
const TEXT_MULTILINE: i32 = 2;
const TEXT_INFO: i32 = 3;
const GROUP_CHECKABLE: i32 = 2;

fn string(s: *const c_char) -> String {
    if s.is_null() { String::new() } else { unsafe { CStr::from_ptr(s) }.to_string_lossy().into_owned() }
}

/// Runs `f` with filter `filter_name` of source `source_name` (a Studio name).
fn with_filter<T>(fns: &Fns, source_name: &str, filter_name: &str, f: impl FnOnce(P) -> T) -> Option<T> {
    let source_name = CString::new(studio_key_source_name(source_name)).ok()?;
    let filter_name = CString::new(filter_name).ok()?;
    unsafe {
        let source = (fns.get_source_by_name)(source_name.as_ptr());
        if source.is_null() {
            return None;
        }
        let filter = (fns.get_filter_by_name)(source, filter_name.as_ptr());
        let result = (!filter.is_null()).then(|| f(filter));
        if !filter.is_null() {
            (fns.source_release)(filter);
        }
        (fns.source_release)(source);
        result
    }
}

fn read_list(fns: &Fns, p: P) -> Kind {
    let format = match unsafe { (fns.list_format)(p) } {
        COMBO_FORMAT_INT => ListFormat::Int,
        COMBO_FORMAT_FLOAT => ListFormat::Float,
        COMBO_FORMAT_BOOL => ListFormat::Bool,
        COMBO_FORMAT_STRING => ListFormat::String,
        _ => return Kind::Other,
    };
    let items = (0..unsafe { (fns.list_item_count)(p) })
        .map(|i| unsafe {
            ListItem {
                label: string((fns.list_item_name)(p, i)),
                value: match format {
                    ListFormat::Int => serde_json::json!((fns.list_item_int)(p, i)),
                    ListFormat::Float => serde_json::json!((fns.list_item_float)(p, i)),
                    ListFormat::Bool => serde_json::json!((fns.list_item_bool)(p, i)),
                    ListFormat::String => serde_json::json!(string((fns.list_item_string)(p, i))),
                },
                disabled: (fns.list_item_disabled)(p, i),
            }
        })
        .collect();
    Kind::List { format, items }
}

/// The visible properties of `props`, in order (groups with their contents).
fn read_props(fns: &Fns, props: P) -> Vec<Prop> {
    let mut out = Vec::new();
    let mut p = unsafe { (fns.properties_first)(props) };
    while !p.is_null() {
        if unsafe { (fns.property_visible)(p) } {
            let kind = unsafe {
                match (fns.property_get_type)(p) {
                    PROPERTY_BOOL => Kind::Bool,
                    PROPERTY_INT => Kind::Int {
                        min: (fns.int_min)(p) as i64,
                        max: (fns.int_max)(p) as i64,
                        step: ((fns.int_step)(p) as i64).max(1),
                        slider: (fns.int_type)(p) == NUMBER_SLIDER,
                    },
                    PROPERTY_FLOAT => Kind::Float {
                        min: (fns.float_min)(p),
                        max: (fns.float_max)(p),
                        step: (fns.float_step)(p),
                        slider: (fns.float_type)(p) == NUMBER_SLIDER,
                    },
                    PROPERTY_TEXT => {
                        let t = (fns.text_type)(p);
                        Kind::Text { password: t == TEXT_PASSWORD, multiline: t == TEXT_MULTILINE, info: t == TEXT_INFO }
                    }
                    PROPERTY_PATH => Kind::Path,
                    PROPERTY_LIST => read_list(fns, p),
                    PROPERTY_COLOR => Kind::Color { alpha: false },
                    PROPERTY_COLOR_ALPHA => Kind::Color { alpha: true },
                    PROPERTY_GROUP => {
                        let content = (fns.group_content)(p);
                        Kind::Group {
                            checkable: (fns.group_type)(p) == GROUP_CHECKABLE,
                            children: if content.is_null() { Vec::new() } else { read_props(fns, content) },
                        }
                    }
                    _ => Kind::Other,
                }
            };
            let name = string(unsafe { (fns.property_name)(p) });
            let label = string(unsafe { (fns.property_description)(p) });
            out.push(Prop {
                label: if label.is_empty() { name.clone() } else { label },
                name,
                enabled: unsafe { (fns.property_enabled)(p) },
                kind,
            });
        }
        if !unsafe { (fns.property_next)(&mut p) } {
            break;
        }
    }
    out
}

/// The filter's properties as its OBS Filters window shows them, or None when
/// it has none (or can't be found). Like opening that window, this runs the
/// filter's property code (Lua for script filters).
pub fn query(source_name: &str, filter_name: &str) -> Option<Vec<Prop>> {
    let fns = fns()?;
    with_filter(fns, source_name, filter_name, |filter| unsafe {
        let props = (fns.source_properties)(filter);
        if props.is_null() {
            return None;
        }
        let list = read_props(fns, props);
        (fns.properties_destroy)(props);
        (!list.is_empty()).then_some(list)
    })
    .flatten()
}

/// Sets property `name` of the filter to `value` the way its OBS Filters
/// window does: into its settings (typed by the property), then the
/// property's own reaction, then the filter's update. Some(true) when that
/// reaction changed the properties (read them again), None if nothing was set.
pub fn set(source_name: &str, filter_name: &str, name: &str, value: &serde_json::Value) -> Option<bool> {
    let fns = fns()?;
    let cname = CString::new(name).ok()?;
    with_filter(fns, source_name, filter_name, |filter| unsafe {
        let props = (fns.source_properties)(filter);
        if props.is_null() {
            return None;
        }
        let prop = (fns.properties_get)(props, cname.as_ptr());
        let settings = (fns.source_get_settings)(filter);
        let written = !prop.is_null() && !settings.is_null() && {
            let key = cname.as_ptr();
            match ((fns.property_get_type)(prop), value) {
                (PROPERTY_INT | PROPERTY_COLOR | PROPERTY_COLOR_ALPHA, v) if v.is_number() => {
                    (fns.data_set_int)(settings, key, v.as_i64().or_else(|| v.as_u64().map(|u| u as i64)).unwrap_or_else(|| v.as_f64().unwrap_or(0.0).round() as i64));
                    true
                }
                (PROPERTY_FLOAT, v) if v.is_number() => {
                    (fns.data_set_double)(settings, key, v.as_f64().unwrap_or(0.0));
                    true
                }
                (PROPERTY_BOOL | PROPERTY_GROUP, serde_json::Value::Bool(b)) => {
                    (fns.data_set_bool)(settings, key, *b);
                    true
                }
                (PROPERTY_TEXT | PROPERTY_PATH, serde_json::Value::String(s)) => match CString::new(s.as_str()) {
                    Ok(s) => {
                        (fns.data_set_string)(settings, key, s.as_ptr());
                        true
                    }
                    Err(_) => false,
                },
                (PROPERTY_LIST, v) => match ((fns.list_format)(prop), v) {
                    (COMBO_FORMAT_INT, v) if v.is_number() => {
                        (fns.data_set_int)(settings, key, v.as_i64().unwrap_or(0));
                        true
                    }
                    (COMBO_FORMAT_FLOAT, v) if v.is_number() => {
                        (fns.data_set_double)(settings, key, v.as_f64().unwrap_or(0.0));
                        true
                    }
                    (COMBO_FORMAT_BOOL, serde_json::Value::Bool(b)) => {
                        (fns.data_set_bool)(settings, key, *b);
                        true
                    }
                    (COMBO_FORMAT_STRING, serde_json::Value::String(s)) => match CString::new(s.as_str()) {
                        Ok(s) => {
                            (fns.data_set_string)(settings, key, s.as_ptr());
                            true
                        }
                        Err(_) => false,
                    },
                    _ => false,
                },
                _ => false,
            }
        };
        let refresh = written && (fns.property_modified)(prop, settings);
        if written {
            (fns.source_update)(filter, std::ptr::null_mut());
        }
        if !settings.is_null() {
            (fns.data_release)(settings);
        }
        (fns.properties_destroy)(props);
        written.then_some(refresh)
    })
    .flatten()
}

/// OBS color (0xAABBGGRR) → egui color.
pub fn color_from_obs(v: u64) -> egui::Color32 {
    let v = v as u32;
    egui::Color32::from_rgba_unmultiplied(v as u8, (v >> 8) as u8, (v >> 16) as u8, (v >> 24) as u8)
}

/// egui color → OBS color (0xAABBGGRR); opaque unless `alpha`.
pub fn color_to_obs(c: egui::Color32, alpha: bool) -> u64 {
    let [r, g, b, a] = c.to_srgba_unmultiplied();
    let a = if alpha { a } else { 255 };
    (u32::from_le_bytes([r, g, b, a])) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn obs_colors_round_trip() {
        // 0xAABBGGRR: opaque pure red, half-transparent blue
        let red = color_from_obs(0xFF0000FF);
        assert_eq!(red, egui::Color32::from_rgb(255, 0, 0));
        assert_eq!(color_to_obs(red, false), 0xFF0000FF);
        let blue = color_from_obs(0x80FF0000);
        assert_eq!(blue.to_srgba_unmultiplied(), [0, 0, 255, 0x80]);
        assert_eq!(color_to_obs(blue, true), 0x80FF0000);
        // A plain (no alpha) color property is always stored opaque
        assert_eq!(color_to_obs(blue, false), 0xFFFF0000);
    }
}
