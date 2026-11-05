//! Background worker: polls Steam, detects updates, persists, notifies.
//!
//! The worker owns all network I/O for update checks and runs independently of
//! the UI, so checks and notifications keep working while the window is hidden
//! in the tray. Shared state is a single mutex that is only ever held for short,
//! non-blocking sections (never across a network request).

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::Duration;

use crate::detect;
use crate::model::{AppId, Game, Library, Settings, Timestamp, UpdateKind, now};
use crate::platform;
use crate::steam::{self, SearchHit, SteamClient};
use crate::store::{self, Paths};

#[derive(Clone, Debug, Default)]
pub struct Status {
    /// Game currently being checked.
    pub checking: Option<AppId>,
    /// Games waiting in the current check run.
    pub pending: usize,
    pub last_check: Option<Timestamp>,
    pub next_check: Option<Timestamp>,
    pub offline: bool,
    /// The very first check after launch has completed.
    pub startup_check_done: bool,
}

#[derive(Debug, Default)]
pub struct Shared {
    pub library: Library,
    pub settings: Settings,
    pub status: Status,
}

#[derive(Clone)]
pub struct SharedState {
    inner: Arc<Mutex<Shared>>,
    paths: Arc<Paths>,
}

impl SharedState {
    pub fn new(paths: Paths, library: Library, settings: Settings) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Shared { library, settings, status: Status::default() })),
            paths: Arc::new(paths),
        }
    }

    pub fn lock(&self) -> MutexGuard<'_, Shared> {
        // A panic while holding the lock must not brick the app: recover the data.
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn paths(&self) -> &Paths {
        &self.paths
    }

    pub fn save_library(&self, shared: &Shared) {
        if let Err(e) = store::save(&self.paths.library_file(), &shared.library) {
            log::error!("saving library failed: {e:#}");
        }
    }

    pub fn save_settings(&self, shared: &Shared) {
        if let Err(e) = store::save(&self.paths.settings_file(), &shared.settings) {
            log::error!("saving settings failed: {e:#}");
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ImageKey {
    Header(AppId),
    Url(String),
}

pub enum Command {
    CheckAll,
    Check(AppId),
    Add(AppId),
    Search {
        request: u64,
        term: String,
    },
    LoadImage(ImageKey),
    /// Settings changed (e.g. the check interval): recompute the schedule.
    Reschedule,
}

#[derive(Clone, Debug)]
pub struct FoundUpdate {
    pub app_id: AppId,
    pub name: String,
    pub kind: UpdateKind,
    pub title: String,
}

pub enum Event {
    Added { app_id: AppId, name: String },
    AddFailed { app_id: AppId, error: String },
    SearchResults { request: u64, hits: Vec<SearchHit> },
    SearchFailed { request: u64, error: String },
    Image { key: ImageKey, image: egui::ColorImage },
    UpdatesFound(Vec<FoundUpdate>),
    CheckFailed { app_id: AppId, error: String },
}

#[derive(Clone)]
pub struct WorkerHandle {
    tx: Sender<Command>,
    images: Sender<ImageKey>,
    /// Used to run interactive work (search, add) immediately on its own thread,
    /// instead of queueing it behind a potentially long check run.
    checker: Checker,
}

impl WorkerHandle {
    pub fn send(&self, cmd: Command) {
        match cmd {
            Command::LoadImage(key) => {
                let _ = self.images.send(key);
            }
            Command::Search { request, term } => self.checker.search(request, term),
            Command::Add(id) => {
                let checker = self.checker.clone();
                thread::spawn(move || checker.add(id));
            }
            other => {
                let _ = self.tx.send(other);
            }
        }
    }
}

/// Spawn the worker threads. `repaint` is called whenever the UI should refresh.
pub fn spawn(state: SharedState, repaint: impl Fn() + Send + Sync + 'static) -> (WorkerHandle, Receiver<Event>) {
    let (cmd_tx, cmd_rx) = mpsc::channel();
    let (img_tx, img_rx) = mpsc::channel();
    let (ev_tx, ev_rx) = mpsc::channel();
    let repaint: Arc<dyn Fn() + Send + Sync> = Arc::new(repaint);
    let client = SteamClient::new();

    let checker =
        Checker { state: state.clone(), client: client.clone(), events: ev_tx.clone(), repaint: repaint.clone() };
    {
        let checker = checker.clone();
        thread::Builder::new()
            .name("ada-checker".into())
            .spawn(move || checker.run(cmd_rx))
            .expect("spawn checker thread");
    }
    {
        let cache = state.paths().image_cache();
        thread::Builder::new()
            .name("ada-images".into())
            .spawn(move || image_loop(img_rx, client, cache, state, ev_tx, repaint))
            .expect("spawn image thread");
    }

    (WorkerHandle { tx: cmd_tx, images: img_tx, checker }, ev_rx)
}

#[derive(Clone)]
struct Checker {
    state: SharedState,
    client: SteamClient,
    events: Sender<Event>,
    repaint: Arc<dyn Fn() + Send + Sync>,
}

impl Checker {
    fn emit(&self, e: Event) {
        let _ = self.events.send(e);
        (self.repaint)();
    }

    fn run(&self, rx: Receiver<Command>) {
        // Check right away on launch: this is what surfaces updates that shipped
        // while the app was closed.
        self.check_all();
        loop {
            let wait = {
                let s = self.state.lock();
                let next = s.status.next_check.unwrap_or_else(now);
                Duration::from_secs((next - now()).clamp(1, 24 * 3600) as u64)
            };
            match rx.recv_timeout(wait) {
                Ok(Command::CheckAll) | Err(RecvTimeoutError::Timeout) => self.check_all(),
                Ok(Command::Check(id)) => {
                    let found = self.check_games(&[id], true);
                    self.finish(found, false);
                }
                // Handled directly by `WorkerHandle::send`.
                Ok(Command::Add(_) | Command::Search { .. }) => {}
                Ok(Command::Reschedule) => {
                    let mut s = self.state.lock();
                    let base = s.status.last_check.unwrap_or_else(now);
                    s.status.next_check = Some(base + s.settings.interval_secs());
                }
                Ok(Command::LoadImage(_)) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }
    }

    fn search(&self, request: u64, term: String) {
        // Searches run on their own short-lived thread so they never wait behind a check run.
        let (client, events, repaint) = (self.client.clone(), self.events.clone(), self.repaint.clone());
        thread::spawn(move || {
            let ev = match client.search(&term) {
                Ok(hits) => Event::SearchResults { request, hits },
                Err(e) => Event::SearchFailed { request, error: e.to_string() },
            };
            let _ = events.send(ev);
            repaint();
        });
    }

    fn check_all(&self) {
        let ids: Vec<AppId> = self.state.lock().library.games.iter().map(|g| g.app_id).collect();
        let found = self.check_games(&ids, false);
        self.finish(found, true);
    }

    fn finish(&self, found: Vec<FoundUpdate>, full_run: bool) {
        {
            let mut s = self.state.lock();
            s.status.checking = None;
            s.status.pending = 0;
            if full_run {
                let t = now();
                s.status.last_check = Some(t);
                s.status.next_check = Some(t + s.settings.interval_secs());
                s.status.startup_check_done = true;
            }
        }
        if !found.is_empty() {
            let (enabled, muted): (bool, HashSet<AppId>) = {
                let s = self.state.lock();
                (s.settings.notifications, s.library.games.iter().filter(|g| g.muted).map(|g| g.app_id).collect())
            };
            let audible: Vec<&FoundUpdate> = found.iter().filter(|f| !muted.contains(&f.app_id)).collect();
            if enabled && !audible.is_empty() {
                platform::notify_updates(&audible);
            }
            self.emit(Event::UpdatesFound(found));
        }
        (self.repaint)();
    }

    /// `user_initiated`: report failures to the UI (background runs only update the status).
    fn check_games(&self, ids: &[AppId], user_initiated: bool) -> Vec<FoundUpdate> {
        let mut found = Vec::new();
        let mut failures = 0usize;
        let mut offline_failures = 0usize;
        for (i, &id) in ids.iter().enumerate() {
            {
                let mut s = self.state.lock();
                s.status.checking = Some(id);
                s.status.pending = ids.len() - i;
            }
            (self.repaint)();
            match self.check_one(id) {
                Ok(Some(f)) => found.push(f),
                Ok(None) => {}
                Err(e) => {
                    failures += 1;
                    let msg = e.to_string();
                    if msg.contains("internet") || msg.contains("timed out") {
                        offline_failures += 1;
                    }
                    log::warn!("check of {id} failed: {msg}");
                    let mut s = self.state.lock();
                    if let Some(g) = s.library.game_mut(id) {
                        g.last_error = Some(msg.clone());
                        g.last_checked = Some(now());
                    }
                    drop(s);
                    if user_initiated {
                        self.emit(Event::CheckFailed { app_id: id, error: msg });
                    }
                }
            }
            // Be a good citizen towards the free community API.
            if i + 1 < ids.len() {
                thread::sleep(Duration::from_millis(350));
            }
        }
        let mut s = self.state.lock();
        if !ids.is_empty() {
            s.status.offline = failures == ids.len() && offline_failures > 0;
        }
        self.state.save_library(&s);
        found
    }

    /// Check a single game. Returns the update if a new build was found.
    fn check_one(&self, id: AppId) -> anyhow::Result<Option<FoundUpdate>> {
        let info = self.client.app_info(id)?;
        let t = now();

        let (old, needs_events) = {
            let s = self.state.lock();
            let Some(g) = s.library.game(id) else {
                return Ok(None); // removed while we were fetching
            };
            let old = g.snapshot.clone();
            let changed = old.as_ref().is_none_or(|o| o.build_id != info.snapshot.build_id);
            let awaiting_notes = s
                .library
                .updates_for(id)
                .any(|u| u.kind == UpdateKind::Silent && t - u.released_at < detect::LATE_NOTES_WINDOW_SECS);
            (old, changed || awaiting_notes)
        };

        // Announcements are best-effort: a failure here must not hide the update.
        let events = if needs_events {
            self.client.events(id).unwrap_or_else(|e| {
                log::warn!("events for {id} unavailable: {e}");
                Vec::new()
            })
        } else {
            Vec::new()
        };

        let mut s = self.state.lock();
        let Shared { library, .. } = &mut *s;
        if library.game(id).is_none() {
            return Ok(None);
        }

        let mut used: HashSet<String> =
            library.updates_for(id).filter_map(|u| u.announcement.as_ref().map(|a| a.gid.clone())).collect();

        // Late patch notes for recent silent updates.
        for u in library.history.iter_mut().filter(|u| u.app_id == id) {
            if u.kind == UpdateKind::Silent
                && t - u.released_at < detect::LATE_NOTES_WINDOW_SECS
                && detect::attach_late_notes(u, &events, &used)
                && let Some(a) = &u.announcement
            {
                used.insert(a.gid.clone());
            }
        }

        let mut result = None;
        match &old {
            Some(o) if o.build_id == info.snapshot.build_id => {}
            Some(o) => {
                let record = detect::new_record(id, Some(o), &info.snapshot, &events, &used, t);
                result =
                    Some(FoundUpdate { app_id: id, name: info.name.clone(), kind: record.kind, title: record.title() });
                library.push_update(record);
            }
            None => {
                let mut record = detect::new_record(id, None, &info.snapshot, &events, &used, t);
                record.seen = true; // baseline: the state when the game was added
                library.push_update(record);
            }
        }

        let g = library.game_mut(id).expect("checked above");
        g.name = info.name;
        if info.header_path.is_some() {
            g.header_path = info.header_path;
        }
        g.snapshot = Some(info.snapshot);
        g.last_checked = Some(t);
        g.last_error = None;
        Ok(result)
    }

    fn add(&self, id: AppId) {
        if self.state.lock().library.contains(id) {
            self.emit(Event::AddFailed { app_id: id, error: "This game is already in your library".into() });
            return;
        }
        match self.client.app_info(id) {
            Ok(info) => {
                let name = info.name.clone();
                {
                    let mut s = self.state.lock();
                    let mut g = Game::new(id, &info.name);
                    g.header_path = info.header_path.clone();
                    s.library.games.push(g);
                }
                // Establishes the baseline build and attaches the current patch notes.
                let _ = self.check_one(id);
                {
                    let s = self.state.lock();
                    self.state.save_library(&s);
                }
                self.emit(Event::Added { app_id: id, name });
            }
            Err(e) => self.emit(Event::AddFailed { app_id: id, error: e.to_string() }),
        }
    }
}

// ---------------------------------------------------------------------------
// Images
// ---------------------------------------------------------------------------

/// Minimum delay before a failed image download is attempted again.
pub const IMAGE_RETRY_SECS: i64 = 30;

fn image_loop(
    rx: Receiver<ImageKey>,
    client: SteamClient,
    cache: PathBuf,
    state: SharedState,
    events: Sender<Event>,
    repaint: Arc<dyn Fn() + Send + Sync>,
) {
    let mut failed: HashMap<ImageKey, Timestamp> = HashMap::new();
    while let Ok(key) = rx.recv() {
        if failed.get(&key).is_some_and(|t| now() - t < IMAGE_RETRY_SECS) {
            continue;
        }
        let (file, urls) = match &key {
            ImageKey::Header(id) => {
                let header = state.lock().library.game(*id).and_then(|g| g.header_path.clone());
                // Keyed by the art path too, so a game's new artwork replaces the cached one.
                let tag = fnv1a(header.as_deref().unwrap_or(""));
                (cache.join(format!("header-{id}-{tag:08x}.jpg")), steam::header_urls(*id, header.as_deref()))
            }
            ImageKey::Url(url) => (cache.join(format!("url-{:016x}.img", fnv1a(url))), vec![url.clone()]),
        };
        let bytes = std::fs::read(&file).ok().or_else(|| {
            let bytes = urls.iter().find_map(|u| client.get_bytes(u).ok())?;
            let _ = std::fs::create_dir_all(&cache);
            let _ = std::fs::write(&file, &bytes);
            Some(bytes)
        });
        let decoded = bytes.and_then(|b| image::load_from_memory(&b).ok());
        match decoded {
            Some(img) => {
                let rgba = img.to_rgba8();
                let size = [rgba.width() as usize, rgba.height() as usize];
                let image = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
                let _ = events.send(Event::Image { key, image });
                repaint();
            }
            None => {
                let _ = std::fs::remove_file(&file);
                failed.insert(key, now());
            }
        }
    }
}

/// Stable, dependency-free hash for cache file names.
fn fnv1a(s: &str) -> u64 {
    s.bytes().fold(0xcbf29ce484222325, |h, b| (h ^ u64::from(b)).wrapping_mul(0x100000001b3))
}

#[cfg(test)]
mod tests {
    #[test]
    fn fnv_is_stable() {
        assert_eq!(super::fnv1a(""), 0xcbf29ce484222325);
        assert_ne!(super::fnv1a("a"), super::fnv1a("b"));
    }
}
