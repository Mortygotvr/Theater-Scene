use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use futures_util::StreamExt;
use serde_json::Value;
use tokio::runtime::Runtime;
use tokio_tungstenite::connect_async;
use crate::types::StreamEvent;

/// Theater's connection to the relay port, for the triggers: whatever reads
/// the chats serves it (the Theater Reader plugin, the Theater Reader app…).
pub struct ReaderClient {
    running: Arc<AtomicBool>,
    connected: Arc<AtomicBool>,
    /// Which start this is: a stopped loop that wakes up late ends
    generation: Arc<AtomicU64>,
}

impl ReaderClient {
    pub fn new() -> Self {
        Self {
            running: Arc::new(AtomicBool::new(false)),
            connected: Arc::new(AtomicBool::new(false)),
            generation: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Relaxed)
    }

    pub fn start(&self, runtime: &Runtime, host: String, port: u16) {
        if self.running.swap(true, Ordering::SeqCst) {
            // Already running
            return;
        }

        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let current = self.generation.clone();
        let running = self.running.clone();
        let connected = self.connected.clone();
        let alive = move || running.load(Ordering::Relaxed) && current.load(Ordering::SeqCst) == generation;

        runtime.spawn(async move {
            let uri = format!("ws://{}:{}", host, port);
            println!("[Trust WS] Connecting to Theater Reader at {}...", uri);

            while alive() {
                match connect_async(&uri).await {
                    Ok((ws_stream, _)) => {
                        println!("[Trust WS] Connected to Theater Reader.");
                        connected.store(true, Ordering::SeqCst);

                        let (_, mut read) = ws_stream.split();

                        while let Some(msg_result) = read.next().await {
                            if !alive() {
                                break;
                            }

                            match msg_result {
                                Ok(tokio_tungstenite::tungstenite::Message::Text(text)) => on_relay_message(&text),
                                Ok(tokio_tungstenite::tungstenite::Message::Close(_)) => {
                                    println!("[Trust WS] Theater Reader connection closed.");
                                    break;
                                }
                                Err(e) => {
                                    println!("[Trust WS] Stream error: {}", e);
                                    break;
                                }
                                _ => {}
                            }
                        }

                        connected.store(false, Ordering::SeqCst);
                    }
                    Err(_e) => {
                        connected.store(false, Ordering::Relaxed);
                        if alive() {
                            tokio::time::sleep(Duration::from_secs(3)).await;
                        }
                    }
                }

                if alive() {
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }
            }

            println!("[Trust WS] Reader loop terminated.");
        });
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
        self.connected.store(false, Ordering::SeqCst);
    }
}

/// A relay message, for Theater's triggers: alerts fire what matches them,
/// tests (test_trigger, test_object) what they name. Chat is the Rhai chat
/// source's, on its own connection (script_feed): here it only reaches the
/// trigger check, where chat never fires anything.
pub fn on_relay_message(text: &str) {
    if let Ok(v) = serde_json::from_str::<Value>(text) {
        // An alert's channel and redeem title, for the trigger lists
        theater_shared::seen::note(&v);
        let msg_type = v.get("type").and_then(|t| t.as_str()).unwrap_or("");
        if msg_type == "moderation" || msg_type == "sys_diag" || msg_type == "sub_cache" {
            return;
        } else if msg_type == "test_object" {
            if let Some(payload) = v.get("payload") {
                if let Ok(p_str) = serde_json::to_string(payload) {
                    crate::studio_app::dispatch_test_redeem_json(&p_str);
                }
            }
            return;
        } else if msg_type == "test_trigger" {
            if let Some(payload) = v.get("payload") {
                let trig_str = payload.get("trigger").or_else(|| payload.get("event")).and_then(|t| t.as_str()).unwrap_or("");
                let ev = crate::types::StreamEvent {
                    event_type: "test_trigger".to_string(),
                    trigger: trig_str.to_string(),
                    user: "TestUser".to_string(),
                    message: trig_str.to_string(),
                    raw_json: text.to_string(),
                };
                crate::studio_app::process_trigger_event(&ev);
            }
            return;
        }
    }

    // (Not broadcast_event: nothing reads those queues since the Lua scripts
    // went, and they'd only grow)
    if let Some(event) = parse_reader_message(text) {
        crate::studio_app::process_trigger_event(&event);
    }
}

pub fn parse_reader_message(raw_json: &str) -> Option<StreamEvent> {
    if let Ok(v) = serde_json::from_str::<Value>(raw_json) {
        let msg_type = v.get("type").and_then(|t| t.as_str()).unwrap_or("event");

        let user = v.get("user")
            .or_else(|| v.get("username"))
            .or_else(|| v.get("payload").and_then(|p| p.get("username").or_else(|| p.get("user"))))
            .or_else(|| v.get("data").and_then(|d| d.get("username").or_else(|| d.get("user")).or_else(|| d.get("displayName"))))
            .and_then(|u| u.as_str())
            .unwrap_or("Viewer");

        let message = v.get("message")
            .or_else(|| v.get("text"))
            .or_else(|| v.get("payload").and_then(|p| p.get("message").or_else(|| p.get("text"))))
            .or_else(|| v.get("data").and_then(|d| d.get("message").or_else(|| d.get("text"))))
            .and_then(|m| m.as_str())
            .unwrap_or("");

        let trigger = v.get("trigger")
            .or_else(|| v.get("event"))
            .or_else(|| v.get("payload").and_then(|p| p.get("trigger").or_else(|| p.get("event"))))
            .or_else(|| v.get("data").and_then(|d| d.get("trigger").or_else(|| d.get("event"))))
            .and_then(|t| t.as_str())
            .unwrap_or(message);

        Some(StreamEvent {
            event_type: msg_type.to_string(),
            trigger: trigger.to_string(),
            user: user.to_string(),
            message: message.to_string(),
            raw_json: raw_json.to_string(),
        })
    } else {
        let trimmed = raw_json.trim();
        if trimmed.is_empty() { return None; }
        Some(StreamEvent {
            event_type: "raw".to_string(),
            trigger: trimmed.to_string(),
            user: "Viewer".to_string(),
            message: trimmed.to_string(),
            raw_json: raw_json.to_string(),
        })
    }
}
