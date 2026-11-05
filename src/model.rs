//! Domain model: everything that is persisted to disk lives here.
//!
//! All types are plain data with `serde` derives so the on-disk format stays
//! human-readable JSON and is easy to migrate (every field that was added
//! after v0.1 must carry `#[serde(default)]`).

use serde::{Deserialize, Serialize};

pub type AppId = u32;

/// Unix timestamp in seconds (UTC).
pub type Timestamp = i64;

pub fn now() -> Timestamp {
    chrono::Utc::now().timestamp()
}

// ---------------------------------------------------------------------------
// Build snapshots
// ---------------------------------------------------------------------------

/// The operating system a depot targets.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Platform {
    Windows,
    MacOs,
    Linux,
}

impl Platform {
    pub fn from_oslist(s: &str) -> Vec<Platform> {
        s.split(',')
            .filter_map(|p| match p.trim() {
                "windows" => Some(Platform::Windows),
                "macos" => Some(Platform::MacOs),
                "linux" => Some(Platform::Linux),
                _ => None,
            })
            .collect()
    }

    pub fn label(self) -> &'static str {
        match self {
            Platform::Windows => "Windows",
            Platform::MacOs => "macOS",
            Platform::Linux => "Linux",
        }
    }
}

/// State of a single depot on the public branch.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DepotState {
    pub id: u32,
    /// Manifest GID — changes whenever the depot's content changes.
    pub manifest: String,
    /// Compressed download size in bytes.
    pub download: u64,
    /// Installed size in bytes.
    pub size: u64,
    /// Empty = all platforms.
    #[serde(default)]
    pub platforms: Vec<Platform>,
    #[serde(default)]
    pub language: Option<String>,
}

/// What the public branch of an app looked like at a given point in time.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildSnapshot {
    pub build_id: u64,
    /// When the public branch last changed (Steam's `timeupdated`).
    pub time_updated: Timestamp,
    #[serde(default)]
    pub depots: Vec<DepotState>,
}

impl BuildSnapshot {
    pub fn total_download(&self) -> u64 {
        self.depots.iter().map(|d| d.download).sum()
    }
}

// ---------------------------------------------------------------------------
// Updates
// ---------------------------------------------------------------------------

/// How the developer classified the update (mirrors Steam's event types),
/// or `Silent` when a new build shipped without any announcement.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum UpdateKind {
    Major,
    Regular,
    Small,
    Silent,
}

impl UpdateKind {
    /// Map a Steam partner-event type to an update kind.
    pub fn from_event_type(event_type: u32) -> Option<Self> {
        match event_type {
            14 => Some(Self::Major),
            13 => Some(Self::Regular),
            12 => Some(Self::Small),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Major => "Major update",
            Self::Regular => "Regular update",
            Self::Small => "Small update",
            Self::Silent => "Silent update",
        }
    }

    pub fn short_label(self) -> &'static str {
        match self {
            Self::Major => "Major",
            Self::Regular => "Update",
            Self::Small => "Patch",
            Self::Silent => "Silent",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::Major => "A large content update flagged as major by the developer.",
            Self::Regular => "A standard content or feature update with release notes.",
            Self::Small => "A small update, hotfix or patch with patch notes.",
            Self::Silent => {
                "A new build shipped without an announcement. Notes are attached automatically if they are published later."
            }
        }
    }
}

/// A Steam announcement (patch notes / news post).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Announcement {
    pub gid: String,
    pub title: String,
    pub event_type: u32,
    pub posted: Timestamp,
    /// BBCode body as returned by Steam.
    pub body: String,
    /// The build this announcement was attached to, if the developer linked one.
    #[serde(default)]
    pub build_id: Option<u64>,
}

impl Announcement {
    pub fn url(&self, app_id: AppId) -> String {
        format!("https://store.steampowered.com/news/app/{app_id}/view/{}", self.gid)
    }
}

/// One detected update.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UpdateRecord {
    pub app_id: AppId,
    pub from_build: Option<u64>,
    pub to_build: u64,
    /// When Steam says the build went live.
    pub released_at: Timestamp,
    /// When this app noticed it.
    pub detected_at: Timestamp,
    pub kind: UpdateKind,
    /// Platforms whose depots changed. Empty = unknown.
    #[serde(default)]
    pub platforms: Vec<Platform>,
    #[serde(default)]
    pub changed_depots: usize,
    /// Change of total compressed download size (bytes, may be negative).
    #[serde(default)]
    pub download_delta: i64,
    /// Sum of compressed sizes of changed depots: an upper bound for the patch size.
    #[serde(default)]
    pub changed_download: u64,
    #[serde(default)]
    pub announcement: Option<Announcement>,
    #[serde(default)]
    pub seen: bool,
}

impl UpdateRecord {
    pub fn key(&self) -> (AppId, u64) {
        (self.app_id, self.to_build)
    }

    pub fn title(&self) -> String {
        match &self.announcement {
            Some(a) if !a.title.trim().is_empty() => a.title.trim().to_owned(),
            _ => format!("Build {}", self.to_build),
        }
    }
}

// ---------------------------------------------------------------------------
// Library
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Game {
    pub app_id: AppId,
    pub name: String,
    pub added_at: Timestamp,
    /// Path of the header image relative to the Steam asset CDN.
    #[serde(default)]
    pub header_path: Option<String>,
    #[serde(default)]
    pub snapshot: Option<BuildSnapshot>,
    #[serde(default)]
    pub last_checked: Option<Timestamp>,
    #[serde(default)]
    pub last_error: Option<String>,
    /// Per-game notification opt-out.
    #[serde(default)]
    pub muted: bool,
}

impl Game {
    pub fn new(app_id: AppId, name: impl Into<String>) -> Self {
        Self {
            app_id,
            name: name.into(),
            added_at: now(),
            header_path: None,
            snapshot: None,
            last_checked: None,
            last_error: None,
            muted: false,
        }
    }

    pub fn store_url(&self) -> String {
        format!("https://store.steampowered.com/app/{}/", self.app_id)
    }

    pub fn steamdb_url(&self) -> String {
        format!("https://steamdb.info/app/{}/patchnotes/", self.app_id)
    }
}

/// Upper bound for the persisted update history.
pub const MAX_HISTORY: usize = 1000;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Library {
    #[serde(default)]
    pub games: Vec<Game>,
    /// Newest first.
    #[serde(default)]
    pub history: Vec<UpdateRecord>,
}

impl Library {
    pub fn game(&self, app_id: AppId) -> Option<&Game> {
        self.games.iter().find(|g| g.app_id == app_id)
    }

    pub fn game_mut(&mut self, app_id: AppId) -> Option<&mut Game> {
        self.games.iter_mut().find(|g| g.app_id == app_id)
    }

    pub fn contains(&self, app_id: AppId) -> bool {
        self.game(app_id).is_some()
    }

    pub fn remove(&mut self, app_id: AppId) {
        self.games.retain(|g| g.app_id != app_id);
        self.history.retain(|u| u.app_id != app_id);
    }

    pub fn latest_update(&self, app_id: AppId) -> Option<&UpdateRecord> {
        self.history.iter().find(|u| u.app_id == app_id)
    }

    pub fn updates_for(&self, app_id: AppId) -> impl Iterator<Item = &UpdateRecord> {
        self.history.iter().filter(move |u| u.app_id == app_id)
    }

    pub fn unseen(&self) -> impl Iterator<Item = &UpdateRecord> {
        self.history.iter().filter(|u| !u.seen)
    }

    pub fn unseen_count(&self) -> usize {
        self.unseen().count()
    }

    pub fn mark_seen(&mut self, app_id: Option<AppId>) {
        for u in &mut self.history {
            if app_id.is_none_or(|id| id == u.app_id) {
                u.seen = true;
            }
        }
    }

    /// Insert a record (newest first), replacing one for the same build.
    pub fn push_update(&mut self, record: UpdateRecord) {
        self.history.retain(|u| u.key() != record.key());
        let pos = self.history.iter().position(|u| u.released_at <= record.released_at).unwrap_or(self.history.len());
        self.history.insert(pos, record);
        self.history.truncate(MAX_HISTORY);
    }
}

// ---------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThemeChoice {
    #[default]
    System,
    Dark,
    Light,
}

pub const CHECK_INTERVALS_MIN: [u32; 6] = [5, 10, 15, 30, 60, 180];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub launch_at_startup: bool,
    pub start_minimized: bool,
    pub close_to_tray: bool,
    pub notifications: bool,
    pub check_interval_min: u32,
    pub theme: ThemeChoice,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            launch_at_startup: false,
            start_minimized: true,
            close_to_tray: true,
            notifications: true,
            check_interval_min: 10,
            theme: ThemeChoice::System,
        }
    }
}

impl Settings {
    pub fn interval_secs(&self) -> i64 {
        i64::from(self.check_interval_min.clamp(5, 24 * 60)) * 60
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(app: AppId, build: u64, at: Timestamp) -> UpdateRecord {
        UpdateRecord {
            app_id: app,
            from_build: None,
            to_build: build,
            released_at: at,
            detected_at: at,
            kind: UpdateKind::Silent,
            platforms: vec![],
            changed_depots: 0,
            download_delta: 0,
            changed_download: 0,
            announcement: None,
            seen: false,
        }
    }

    #[test]
    fn history_is_sorted_and_deduplicated() {
        let mut lib = Library::default();
        lib.push_update(rec(1, 10, 100));
        lib.push_update(rec(2, 20, 300));
        lib.push_update(rec(1, 11, 200));
        lib.push_update(rec(1, 11, 200));
        let order: Vec<_> = lib.history.iter().map(|u| u.to_build).collect();
        assert_eq!(order, vec![20, 11, 10]);
    }

    #[test]
    fn mark_seen_scoped_to_game() {
        let mut lib = Library::default();
        lib.push_update(rec(1, 10, 100));
        lib.push_update(rec(2, 20, 300));
        lib.mark_seen(Some(1));
        assert_eq!(lib.unseen_count(), 1);
        lib.mark_seen(None);
        assert_eq!(lib.unseen_count(), 0);
    }

    #[test]
    fn oslist_parsing() {
        assert_eq!(Platform::from_oslist("windows, linux,unknown"), vec![Platform::Windows, Platform::Linux]);
    }

    #[test]
    fn settings_tolerate_missing_fields() {
        let s: Settings = serde_json::from_str(r#"{"notifications":false}"#).unwrap();
        assert!(!s.notifications);
        assert_eq!(s.check_interval_min, 10);
    }
}
