//! Pure update-detection logic. No I/O here, so every rule is unit-tested.

use std::collections::{BTreeSet, HashSet};

use crate::model::{Announcement, AppId, BuildSnapshot, Platform, Timestamp, UpdateKind, UpdateRecord};

/// How far before the build an announcement may be posted and still belong to it.
const MATCH_BEFORE_SECS: i64 = 12 * 3600;
/// How long after the build we keep looking for its patch notes.
const MATCH_AFTER_SECS: i64 = 72 * 3600;
/// Silent updates younger than this are re-checked for late patch notes.
pub const LATE_NOTES_WINDOW_SECS: i64 = 7 * 24 * 3600;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BuildDiff {
    pub changed_depots: usize,
    /// Empty = the change touches shared (all-platform) content.
    pub platforms: Vec<Platform>,
    pub download_delta: i64,
    pub changed_download: u64,
}

/// Compare two snapshots of the public branch. `None` means "same build".
pub fn diff(old: &BuildSnapshot, new: &BuildSnapshot) -> Option<BuildDiff> {
    if old.build_id == new.build_id {
        return None;
    }
    let mut changed = 0usize;
    let mut changed_download = 0u64;
    let mut platforms = BTreeSet::new();
    let mut touches_shared = false;

    for depot in &new.depots {
        let before = old.depots.iter().find(|d| d.id == depot.id);
        if before.is_some_and(|b| b.manifest == depot.manifest) {
            continue;
        }
        changed += 1;
        changed_download += depot.download;
        if depot.platforms.is_empty() {
            touches_shared = true;
        }
        platforms.extend(depot.platforms.iter().copied());
    }

    Some(BuildDiff {
        changed_depots: changed,
        platforms: if touches_shared { Vec::new() } else { platforms.into_iter().collect() },
        download_delta: new.total_download() as i64 - old.total_download() as i64,
        changed_download,
    })
}

fn is_update_event(a: &Announcement) -> bool {
    UpdateKind::from_event_type(a.event_type).is_some()
}

/// Find the announcement that describes `build_id`, released at `released_at`.
///
/// 1. An announcement the developer explicitly linked to this build wins.
/// 2. Otherwise the closest update-type announcement inside the time window,
///    skipping announcements already attributed to another build.
pub fn match_announcement<'a>(
    events: &'a [Announcement],
    build_id: u64,
    released_at: Timestamp,
    used: &HashSet<String>,
) -> Option<&'a Announcement> {
    if let Some(a) = events.iter().find(|a| a.build_id == Some(build_id)) {
        return Some(a);
    }
    events
        .iter()
        .filter(|a| is_update_event(a) && !used.contains(&a.gid))
        // An announcement linked to a *different* build is not ours.
        .filter(|a| a.build_id.is_none_or(|b| b == build_id))
        .filter(|a| {
            let dt = a.posted - released_at;
            (-MATCH_BEFORE_SECS..=MATCH_AFTER_SECS).contains(&dt)
        })
        .min_by_key(|a| (a.posted - released_at).abs())
}

pub fn kind_for(announcement: Option<&Announcement>) -> UpdateKind {
    announcement.and_then(|a| UpdateKind::from_event_type(a.event_type)).unwrap_or(UpdateKind::Silent)
}

/// Build the record for a newly detected build.
pub fn new_record(
    app_id: AppId,
    old: Option<&BuildSnapshot>,
    new: &BuildSnapshot,
    events: &[Announcement],
    used: &HashSet<String>,
    now: Timestamp,
) -> UpdateRecord {
    let d = old.and_then(|o| diff(o, new)).unwrap_or_default();
    // Steam's timestamp can lag or be missing; never report a release in the future
    // or before the previous build.
    let floor = old.map(|o| o.time_updated).unwrap_or(0);
    let released_at = if new.time_updated > 0 { new.time_updated.clamp(floor, now) } else { now };
    let announcement = match_announcement(events, new.build_id, released_at, used).cloned();
    UpdateRecord {
        app_id,
        from_build: old.map(|o| o.build_id),
        to_build: new.build_id,
        released_at,
        detected_at: now,
        kind: kind_for(announcement.as_ref()),
        platforms: d.platforms,
        changed_depots: d.changed_depots,
        download_delta: d.download_delta,
        changed_download: d.changed_download,
        announcement,
        seen: false,
    }
}

/// Try to attach patch notes that were published after a silent update was detected.
/// Returns `true` if the record changed.
pub fn attach_late_notes(record: &mut UpdateRecord, events: &[Announcement], used: &HashSet<String>) -> bool {
    if record.announcement.is_some() {
        return false;
    }
    match match_announcement(events, record.to_build, record.released_at, used) {
        Some(a) => {
            record.kind = kind_for(Some(a));
            record.announcement = Some(a.clone());
            true
        }
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::DepotState;

    fn depot(id: u32, manifest: &str, download: u64, os: &[Platform]) -> DepotState {
        DepotState {
            id,
            manifest: manifest.into(),
            download,
            size: download * 2,
            platforms: os.to_vec(),
            language: None,
        }
    }

    fn snap(build: u64, t: Timestamp, depots: Vec<DepotState>) -> BuildSnapshot {
        BuildSnapshot { build_id: build, time_updated: t, depots }
    }

    fn ann(gid: &str, event_type: u32, posted: Timestamp, build: Option<u64>) -> Announcement {
        Announcement {
            gid: gid.into(),
            title: format!("Event {gid}"),
            event_type,
            posted,
            body: String::new(),
            build_id: build,
        }
    }

    #[test]
    fn same_build_is_not_an_update() {
        let a = snap(1, 100, vec![]);
        assert!(diff(&a, &a.clone()).is_none());
    }

    #[test]
    fn diff_reports_platforms_and_sizes() {
        let old = snap(1, 100, vec![depot(1, "a", 100, &[Platform::Windows]), depot(2, "b", 50, &[Platform::Linux])]);
        let new = snap(2, 200, vec![depot(1, "a2", 130, &[Platform::Windows]), depot(2, "b", 50, &[Platform::Linux])]);
        let d = diff(&old, &new).unwrap();
        assert_eq!(d.changed_depots, 1);
        assert_eq!(d.platforms, vec![Platform::Windows]);
        assert_eq!(d.download_delta, 30);
        assert_eq!(d.changed_download, 130);
    }

    #[test]
    fn shared_depot_means_all_platforms() {
        let old = snap(1, 100, vec![depot(1, "a", 100, &[]), depot(2, "b", 50, &[Platform::Linux])]);
        let new = snap(2, 200, vec![depot(1, "a2", 100, &[]), depot(2, "b2", 50, &[Platform::Linux])]);
        assert!(diff(&old, &new).unwrap().platforms.is_empty());
    }

    #[test]
    fn explicit_build_link_wins() {
        let events = vec![ann("near", 12, 1000, None), ann("linked", 14, 5000, Some(42))];
        let a = match_announcement(&events, 42, 1000, &HashSet::new()).unwrap();
        assert_eq!(a.gid, "linked");
    }

    #[test]
    fn closest_update_event_in_window() {
        let events = vec![
            ann("news", 28, 1000, None),
            ann("far", 13, 1000 + MATCH_AFTER_SECS + 1, None),
            ann("close", 12, 1300, None),
            ann("closer-but-other-build", 13, 1001, Some(7)),
        ];
        let a = match_announcement(&events, 42, 1000, &HashSet::new()).unwrap();
        assert_eq!(a.gid, "close");
    }

    #[test]
    fn used_announcements_are_skipped() {
        let events = vec![ann("x", 13, 1000, None)];
        let used: HashSet<String> = ["x".to_string()].into();
        assert!(match_announcement(&events, 1, 1000, &used).is_none());
    }

    #[test]
    fn new_record_classifies_and_clamps_time() {
        let old = snap(1, 500, vec![depot(1, "a", 10, &[])]);
        let new = snap(2, 9_999_999, vec![depot(1, "b", 20, &[])]);
        let events = vec![ann("m", 14, 1990, None)];
        let r = new_record(5, Some(&old), &new, &events, &HashSet::new(), 2000);
        assert_eq!(r.released_at, 2000);
        assert_eq!(r.kind, UpdateKind::Major);
        assert_eq!(r.from_build, Some(1));
        assert_eq!(r.download_delta, 10);
        assert!(!r.seen);
    }

    #[test]
    fn silent_update_gets_late_notes() {
        let old = snap(1, 100, vec![]);
        let new = snap(2, 1000, vec![]);
        let mut r = new_record(5, Some(&old), &new, &[], &HashSet::new(), 1000);
        assert_eq!(r.kind, UpdateKind::Silent);
        let events = vec![ann("late", 12, 1000 + 3600, None)];
        assert!(attach_late_notes(&mut r, &events, &HashSet::new()));
        assert_eq!(r.kind, UpdateKind::Small);
        assert!(!attach_late_notes(&mut r, &events, &HashSet::new()));
    }
}
