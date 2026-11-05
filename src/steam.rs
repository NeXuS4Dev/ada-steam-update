//! Thin, blocking HTTP client for the public Steam data sources the app relies on.
//!
//! * `api.steamcmd.net`  – PICS product info (public build id, depots, manifests).
//!   This is the authoritative "did a new build ship?" signal and also catches
//!   silent updates that never get an announcement.
//! * `store.steampowered.com/events` – partner events, which carry the developer's
//!   own classification (major / regular / small update) and the patch notes.
//! * `store.steampowered.com/api/storesearch` – name search for the add dialog.
//!
//! None of these endpoints require an API key.

use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use serde::Deserialize;
use serde_json::Value;

use crate::model::{Announcement, AppId, BuildSnapshot, DepotState, Platform, Timestamp};

const USER_AGENT: &str =
    concat!("AdaSteamUpdate/", env!("CARGO_PKG_VERSION"), " (+https://github.com/your-name/ada-steam-update)");

/// Everything we learn about an app from a single product-info request.
#[derive(Clone, Debug)]
pub struct AppInfo {
    pub name: String,
    pub header_path: Option<String>,
    pub snapshot: BuildSnapshot,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchHit {
    pub app_id: AppId,
    pub name: String,
    pub thumbnail: Option<String>,
}

#[derive(Clone)]
pub struct SteamClient {
    agent: ureq::Agent,
}

impl Default for SteamClient {
    fn default() -> Self {
        Self::new()
    }
}

impl SteamClient {
    pub fn new() -> Self {
        let config =
            ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(20))).user_agent(USER_AGENT).build();
        Self { agent: ureq::Agent::new_with_config(config) }
    }

    fn get_json(&self, url: &str) -> Result<Value> {
        let mut resp = self.agent.get(url).call().map_err(map_http_error)?;
        resp.body_mut()
            .with_config()
            .limit(16 * 1024 * 1024)
            .read_json::<Value>()
            .context("invalid response from server")
    }

    pub fn get_bytes(&self, url: &str) -> Result<Vec<u8>> {
        let mut resp = self.agent.get(url).call().map_err(map_http_error)?;
        Ok(resp.body_mut().with_config().limit(8 * 1024 * 1024).read_to_vec()?)
    }

    /// Fetch the public-branch build and depot manifests of an app.
    pub fn app_info(&self, app_id: AppId) -> Result<AppInfo> {
        let v = self.get_json(&format!("https://api.steamcmd.net/v1/info/{app_id}"))?;
        parse_app_info(app_id, &v)
    }

    /// Recent partner events (announcements), newest first.
    pub fn events(&self, app_id: AppId) -> Result<Vec<Announcement>> {
        let v = self.get_json(&format!(
            "https://store.steampowered.com/events/ajaxgetpartnereventspageable/\
             ?clan_accountid=0&appid={app_id}&offset=0&count=25&l=english"
        ))?;
        Ok(parse_events(&v))
    }

    pub fn search(&self, term: &str) -> Result<Vec<SearchHit>> {
        let v = self.get_json(&format!(
            "https://store.steampowered.com/api/storesearch/?term={}&l=english&cc=US",
            urlencoding::encode(term)
        ))?;
        #[derive(Deserialize)]
        struct Item {
            id: AppId,
            name: String,
            tiny_image: Option<String>,
        }
        let items: Vec<Item> =
            serde_json::from_value(v.get("items").cloned().unwrap_or(Value::Array(vec![]))).unwrap_or_default();
        Ok(items.into_iter().map(|i| SearchHit { app_id: i.id, name: i.name, thumbnail: i.tiny_image }).collect())
    }
}

/// Header image candidates, best first.
pub fn header_urls(app_id: AppId, header_path: Option<&str>) -> Vec<String> {
    let mut urls = Vec::new();
    if let Some(p) = header_path {
        urls.push(format!("https://shared.akamai.steamstatic.com/store_item_assets/steam/apps/{app_id}/{p}"));
    }
    urls.push(format!("https://shared.akamai.steamstatic.com/store_item_assets/steam/apps/{app_id}/header.jpg"));
    urls.push(format!("https://cdn.cloudflare.steamstatic.com/steam/apps/{app_id}/header.jpg"));
    urls
}

fn map_http_error(e: ureq::Error) -> anyhow::Error {
    match e {
        ureq::Error::StatusCode(429) => anyhow!("Rate limited by the server — will retry later"),
        ureq::Error::StatusCode(code) if code >= 500 => anyhow!("Server error ({code})"),
        ureq::Error::StatusCode(code) => anyhow!("Request failed ({code})"),
        ureq::Error::Timeout(_) => anyhow!("The request timed out"),
        ureq::Error::Io(_) | ureq::Error::HostNotFound | ureq::Error::ConnectionFailed => {
            anyhow!("No internet connection")
        }
        other => anyhow!(other),
    }
}

fn str_u64(v: &Value) -> Option<u64> {
    match v {
        Value::String(s) => s.trim().parse().ok(),
        Value::Number(n) => n.as_u64(),
        _ => None,
    }
}

pub fn parse_app_info(app_id: AppId, v: &Value) -> Result<AppInfo> {
    if v.get("status").and_then(Value::as_str) != Some("success") {
        bail!("Steam did not return product info");
    }
    let app = v
        .pointer(&format!("/data/{app_id}"))
        .filter(|a| a.as_object().is_some_and(|o| !o.is_empty()))
        .ok_or_else(|| anyhow!("No Steam app with ID {app_id}"))?;

    let common = app.get("common");
    let name = common
        .and_then(|c| c.get("name"))
        .and_then(Value::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| format!("App {app_id}"));
    let header_path =
        common.and_then(|c| c.pointer("/header_image/english")).and_then(Value::as_str).map(str::to_owned);

    let depots = app.get("depots");
    let public = depots
        .and_then(|d| d.pointer("/branches/public"))
        .ok_or_else(|| anyhow!("{name} has no public build information"))?;
    let build_id = public.get("buildid").and_then(str_u64).ok_or_else(|| anyhow!("{name} has no public build id"))?;
    let time_updated =
        public.get("timeupdated").or_else(|| public.get("timebuildupdated")).and_then(str_u64).unwrap_or(0)
            as Timestamp;

    let mut depot_states = Vec::new();
    if let Some(obj) = depots.and_then(Value::as_object) {
        for (key, depot) in obj {
            let Ok(id) = key.parse::<u32>() else { continue };
            let Some(manifest) = depot.pointer("/manifests/public") else {
                continue;
            };
            let gid = manifest
                .get("gid")
                .and_then(|g| g.as_str().map(str::to_owned).or_else(|| Some(g.to_string())))
                .unwrap_or_default();
            if gid.is_empty() {
                continue;
            }
            let config = depot.get("config");
            depot_states.push(DepotState {
                id,
                manifest: gid,
                download: manifest.get("download").and_then(str_u64).unwrap_or(0),
                size: manifest.get("size").and_then(str_u64).unwrap_or(0),
                platforms: config
                    .and_then(|c| c.get("oslist"))
                    .and_then(Value::as_str)
                    .map(Platform::from_oslist)
                    .unwrap_or_default(),
                language: config
                    .and_then(|c| c.get("language"))
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned),
            });
        }
    }
    depot_states.sort_by_key(|d| d.id);

    Ok(AppInfo { name, header_path, snapshot: BuildSnapshot { build_id, time_updated, depots: depot_states } })
}

pub fn parse_events(v: &Value) -> Vec<Announcement> {
    let Some(events) = v.get("events").and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut out: Vec<Announcement> = events
        .iter()
        .filter_map(|e| {
            let body = e.get("announcement_body");
            let gid = body.and_then(|b| b.get("gid")).or_else(|| e.get("gid")).and_then(Value::as_str)?.to_owned();
            let event_type = e.get("event_type").and_then(Value::as_u64).unwrap_or(0) as u32;
            let title = body
                .and_then(|b| b.get("headline"))
                .or_else(|| e.get("event_name"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            let posted = body
                .and_then(|b| b.get("posttime"))
                .or_else(|| e.get("rtime32_start_time"))
                .and_then(Value::as_i64)
                .unwrap_or(0);
            let build_id = e.get("build_id").and_then(str_u64).filter(|b| *b != 0);
            Some(Announcement {
                gid,
                title,
                event_type,
                posted,
                body: body.and_then(|b| b.get("body")).and_then(Value::as_str).unwrap_or_default().to_owned(),
                build_id,
            })
        })
        .collect();
    out.sort_by_key(|a| std::cmp::Reverse(a.posted));
    out
}

/// Parse user input into an App ID: a bare number, or any Steam / SteamDB URL.
pub fn parse_app_id(input: &str) -> Option<AppId> {
    let s = input.trim();
    if let Ok(id) = s.parse::<AppId>() {
        return (id > 0).then_some(id);
    }
    for marker in ["/app/", "/apps/", "/news/app/"] {
        if let Some(i) = s.find(marker) {
            let rest = &s[i + marker.len()..];
            let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
            if let Ok(id) = digits.parse::<AppId>() {
                return (id > 0).then_some(id);
            }
        }
    }
    if let Some(rest) = s.strip_prefix("steam://rungameid/").or_else(|| s.strip_prefix("steam://run/")) {
        return rest.split(['/', '?']).next()?.parse().ok();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_app_ids() {
        assert_eq!(parse_app_id("730"), Some(730));
        assert_eq!(parse_app_id("  570 "), Some(570));
        assert_eq!(parse_app_id("https://store.steampowered.com/app/1091500/Cyberpunk_2077/"), Some(1091500));
        assert_eq!(parse_app_id("https://steamdb.info/app/730/patchnotes/"), Some(730));
        assert_eq!(parse_app_id("steam://rungameid/440"), Some(440));
        assert_eq!(parse_app_id("cyberpunk"), None);
        assert_eq!(parse_app_id("0"), None);
    }

    #[test]
    fn parses_product_info() {
        let v = json!({
            "status": "success",
            "data": { "730": {
                "common": { "name": "Counter-Strike 2", "header_image": { "english": "abc/header.jpg" } },
                "depots": {
                    "branches": { "public": { "buildid": "26445667", "timeupdated": "1793839972" } },
                    "2347771": { "config": { "oslist": "windows" },
                                 "manifests": { "public": { "gid": "301", "download": "100", "size": "200" } } },
                    "2347773": { "config": { "oslist": "linux", "language": "" },
                                 "manifests": { "public": { "gid": "851", "download": "50", "size": "70" } } },
                    "731": { "manifests": {} },
                    "workshopdepot": "0"
                }
            }}
        });
        let info = parse_app_info(730, &v).unwrap();
        assert_eq!(info.name, "Counter-Strike 2");
        assert_eq!(info.header_path.as_deref(), Some("abc/header.jpg"));
        assert_eq!(info.snapshot.build_id, 26445667);
        assert_eq!(info.snapshot.time_updated, 1793839972);
        assert_eq!(info.snapshot.depots.len(), 2);
        assert_eq!(info.snapshot.depots[0].platforms, vec![Platform::Windows]);
        assert_eq!(info.snapshot.depots[1].language, None);
        assert_eq!(info.snapshot.total_download(), 150);
    }

    #[test]
    fn unknown_app_is_an_error() {
        let v = json!({ "status": "success", "data": { "99": {} } });
        assert!(parse_app_info(99, &v).is_err());
    }

    #[test]
    fn parses_events_newest_first() {
        let v = json!({ "events": [
            { "gid": "1", "event_type": 28, "event_name": "News", "rtime32_start_time": 10, "build_id": 0 },
            { "event_type": 14, "build_id": 555, "rtime32_start_time": 20,
              "announcement_body": { "gid": "2", "headline": "Update 2.0", "posttime": 21, "body": "[p]hi[/p]" } }
        ]});
        let ev = parse_events(&v);
        assert_eq!(ev.len(), 2);
        assert_eq!(ev[0].gid, "2");
        assert_eq!(ev[0].build_id, Some(555));
        assert_eq!(ev[0].posted, 21);
        assert_eq!(ev[1].build_id, None);
    }
}
