//! The relay's messages for Rhai sources (`fn on_event(ev)` in a
//! `KIND = "source"` script): Theater's own connection to Theater Reader /
//! Streamer Alert Relay (ws://127.0.0.1:41837 unless set otherwise), so a
//! chat source works without the Control Panel or a browser. It's separate
//! from the connection redeems and poses use: a source seeing a message
//! never makes anything fire. Test messages (publish) reach every source
//! the same way.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender, TrySendError};
use futures_util::StreamExt;
use parking_lot::Mutex;

use crate::obs_source::trust_log;

/// The relay's port when none is set.
pub const DEFAULT_PORT: u16 = 41837;

/// Messages a source hasn't taken yet, at most: one that isn't ticking (OBS
/// paused it) doesn't pile them up forever.
const BACKLOG: usize = 256;

static SUBSCRIBERS: Mutex<Vec<Sender<Arc<str>>>> = Mutex::new(Vec::new());
static STARTED: AtomicBool = AtomicBool::new(false);
static CONNECTED: AtomicBool = AtomicBool::new(false);

/// Whether the relay is connected.
pub fn connected() -> bool {
    CONNECTED.load(Ordering::Relaxed)
}

/// Every relay message from now on (its text, JSON as the relay sends it).
/// The first subscriber connects to the relay.
pub fn subscribe() -> Receiver<Arc<str>> {
    let (tx, rx) = crossbeam_channel::bounded(BACKLOG);
    SUBSCRIBERS.lock().push(tx);
    connect_once();
    rx
}

/// Hands `message` to every subscriber, as if the relay had sent it.
pub fn publish(message: &str) {
    let message: Arc<str> = Arc::from(message);
    SUBSCRIBERS.lock().retain(|tx| !matches!(tx.try_send(message.clone()), Err(TrySendError::Disconnected(_))));
}

/// The relay's port: theater_settings.json's relay_port, or DEFAULT_PORT.
pub fn relay_port() -> u16 {
    crate::obs_source::setting_u64("relay_port").and_then(|p| u16::try_from(p).ok()).filter(|p| *p > 0).unwrap_or(DEFAULT_PORT)
}

fn connect_once() {
    if STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    let port = relay_port();
    crate::state::get_state().runtime.spawn(async move {
        let uri = format!("ws://127.0.0.1:{}", port);
        let mut said_waiting = false;
        loop {
            match tokio_tungstenite::connect_async(&uri).await {
                Ok((stream, _)) => {
                    trust_log(&format!("[Rhai] 📡 Connected to the relay at {} (chat for Rhai sources)", uri));
                    CONNECTED.store(true, Ordering::Relaxed);
                    said_waiting = false;
                    let (_, mut read) = stream.split();
                    while let Some(message) = read.next().await {
                        match message {
                            // (Not a Twitch alert Theater API · Twitch sends too)
                            Ok(tokio_tungstenite::tungstenite::Message::Text(text)) if crate::obs_procs::api_sends_this_twitch_alert(&text) => {}
                            Ok(tokio_tungstenite::tungstenite::Message::Text(text)) => publish(text.as_str()),
                            Ok(tokio_tungstenite::tungstenite::Message::Close(_)) | Err(_) => break,
                            _ => {}
                        }
                    }
                    CONNECTED.store(false, Ordering::Relaxed);
                    trust_log(&format!("[Rhai] 📡 The relay at {} closed: reconnecting", uri));
                }
                Err(e) => {
                    if !said_waiting {
                        trust_log(&format!("[Rhai] 📡 No relay at {} yet ({}): trying every few seconds", uri, e));
                        said_waiting = true;
                    }
                }
            }
            tokio::time::sleep(Duration::from_secs(3)).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn published_messages_reach_every_subscriber() {
        // (No relay in the tests: connecting just keeps retrying in the background.
        // Other tests publish too, so each looks for its own messages)
        let a = subscribe();
        let b = subscribe();
        let first = r#"{"type":"chat","message":"feed test hi"}"#;
        publish(first);
        assert!(a.try_iter().any(|m| &*m == first));
        assert!(b.try_iter().any(|m| &*m == first));
        drop(b);
        publish("feed test second");
        assert!(a.try_iter().any(|m| &*m == "feed test second"));
        // A dropped subscriber is let go of
        assert!(SUBSCRIBERS.lock().len() >= 1);
    }
}
