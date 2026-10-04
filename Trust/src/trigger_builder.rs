//! Triggers made from lists: Platform · Channel · Event · Detail, the same
//! in the Studio's redeem editor as in the Control Panel (theater_shared's,
//! which Theater Reader's tester uses too).
//!
//! Theater Scene's own part: what the lists offer. The channels come from the
//! chats Theater Reader reads (asked through its `theater_reader_state` OBS
//! procedure, when that plugin is installed) and from the alerts that came
//! through the relay (theater_reader_seen.json); 🎯 Catch next listens to the
//! relay (script_feed).

pub use theater_shared::builder::{channels, pattern_ui, triggers_ui};
pub use theater_shared::triggers::*;

/// Gives the lists Theater's chats, alerts and relay. Call once, at load.
pub fn install() {
    theater_shared::seen::set_file(crate::paths::file("theater_reader_seen.json"));
    theater_shared::builder::set_host(theater_shared::builder::Host { chats: reader_chats, subscribe: crate::script_feed::subscribe });
}

/// The chats Theater Reader reads (none when it isn't installed or is off)
fn reader_chats(platform: Option<theater_shared::Platform>) -> Vec<String> {
    crate::obs_procs::reader_chats()
        .into_iter()
        .filter(|(p, _)| platform.is_none_or(|want| want == *p))
        .map(|(_, channel)| channel)
        .collect()
}
