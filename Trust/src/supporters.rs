//! Your supporters, for Rhai scripts (`supporters()`): Twitch subscribers,
//! Patreon patrons and Ko-fi supporters, as the Theater API plugins list
//! them (their OBS procedures). A script never calls OBS: Theater asks the
//! plugins on a thread of its own, at most every REFRESH, and a script reads
//! what came back last. An empty list when no Theater API plugin is installed.

use std::time::{Duration, Instant};

use parking_lot::Mutex;
use serde_json::Value;

const REFRESH: Duration = Duration::from_secs(30);

/// One supporter
#[derive(Clone, Debug, PartialEq)]
pub struct Supporter {
    pub name: String,
    /// "twitch", "patreon", "kofi"
    pub platform: String,
    /// "Tier 1", "Gold"… ("" when there's none)
    pub tier: String,
    /// What they give (a patron's pledge, a Ko-fi amount), 0 when unknown
    pub amount: f64,
    /// A gifted sub
    pub gift: bool,
}

struct Cache {
    list: Vec<Supporter>,
    asked: Option<Instant>,
    asking: bool,
}

static CACHE: Mutex<Cache> = Mutex::new(Cache { list: Vec::new(), asked: None, asking: false });

/// The supporters known now; asks the plugins again when it's been a while.
pub fn list() -> Vec<Supporter> {
    let mut cache = CACHE.lock();
    let stale = cache.asked.is_none_or(|t| t.elapsed() >= REFRESH);
    if stale && !cache.asking {
        cache.asking = true;
        let _ = std::thread::Builder::new().name("theater-supporters".into()).spawn(|| {
            let list = gather();
            let mut cache = CACHE.lock();
            cache.list = list;
            cache.asked = Some(Instant::now());
            cache.asking = false;
        });
    }
    cache.list.clone()
}

fn gather() -> Vec<Supporter> {
    let json = |proc: &std::ffi::CStr| crate::obs_procs::call_json(proc).and_then(|j| serde_json::from_str::<Value>(&j).ok());
    from_lists(
        json(c"theater_api_twitch_subscribers").as_ref(),
        json(c"theater_api_patreon_members").as_ref(),
        json(c"theater_api_kofi_recent").as_ref(),
    )
}

fn text(v: &Value, key: &str) -> String {
    v.get(key).and_then(Value::as_str).unwrap_or("").trim().to_string()
}

/// The plugins' lists as one: Twitch's subscribers, Patreon's active
/// patrons, Ko-fi's recent supporters (each once, anonymous ones left out)
pub fn from_lists(twitch: Option<&Value>, patreon: Option<&Value>, kofi: Option<&Value>) -> Vec<Supporter> {
    let mut out: Vec<Supporter> = Vec::new();
    let mut push = |s: Supporter| {
        let anonymous = s.name.is_empty() || ["someone", "anonymous", "a patron"].contains(&s.name.to_lowercase().as_str());
        if !anonymous && !out.iter().any(|o| o.platform == s.platform && o.name.eq_ignore_ascii_case(&s.name)) {
            out.push(s);
        }
    };
    for s in twitch.and_then(Value::as_array).into_iter().flatten() {
        push(Supporter { name: text(s, "user"), platform: "twitch".into(), tier: text(s, "tier"), amount: 0.0, gift: s.get("gift").and_then(Value::as_bool).unwrap_or(false) });
    }
    for m in patreon.and_then(Value::as_array).into_iter().flatten() {
        if text(m, "status") != "active_patron" {
            continue;
        }
        let cents = m.get("cents").and_then(Value::as_f64).unwrap_or(0.0);
        push(Supporter { name: text(m, "name"), platform: "patreon".into(), tier: text(m, "tier"), amount: cents / 100.0, gift: false });
    }
    for k in kofi.and_then(Value::as_array).into_iter().flatten() {
        let tier = if text(k, "type") == "pledge" { "Members".to_string() } else { String::new() };
        push(Supporter { name: text(k, "user"), platform: "kofi".into(), tier, amount: k.get("amount").and_then(Value::as_f64).unwrap_or(0.0), gift: false });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_plugins_lists_as_one() {
        let twitch = json!([{ "user": "Ann", "tier": "Tier 1", "gift": false }, { "user": "Bob", "tier": "Tier 3", "gift": true }]);
        let patreon = json!([{ "name": "Pat", "status": "active_patron", "tier": "Gold", "cents": 500 }, { "name": "Gone", "status": "former_patron", "tier": "", "cents": 0 }]);
        let kofi = json!([{ "user": "Kind", "amount": 5.0, "type": "donation" }, { "user": "Kind", "amount": 3.0, "type": "donation" }, { "user": "Someone", "amount": 1.0, "type": "donation" }]);
        let list = from_lists(Some(&twitch), Some(&patreon), Some(&kofi));
        let names: Vec<(&str, &str, &str)> = list.iter().map(|s| (s.name.as_str(), s.platform.as_str(), s.tier.as_str())).collect();
        assert_eq!(names, vec![("Ann", "twitch", "Tier 1"), ("Bob", "twitch", "Tier 3"), ("Pat", "patreon", "Gold"), ("Kind", "kofi", "")]);
        assert!(list[1].gift);
        assert_eq!(list[2].amount, 5.0);
        assert!(from_lists(None, None, None).is_empty(), "no Theater API plugin");
    }
}
