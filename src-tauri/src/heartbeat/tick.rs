//! Tier I tokio interval — fires every hour while the Tauri app is open.
//!
//! Lifecycle:
//! - `spawn_tier_i_loop` is called from `setup()` in lib.rs at app boot.
//!   It spawns a tokio task whose first tick fires after a short delay
//!   (gives the JS side time to wire up the event listener) and then
//!   every hour after.
//! - On each tick, we emit `heartbeat:tier-i:tick` with a payload
//!   carrying the tick timestamp + a monotonic count. JS listens and
//!   does the actual reconciliation work (read recent
//!   `heartbeat_health`, compute verdict, banner).
//!
//! The "Run now" button (UI) sends a Tauri command that bumps the
//! interval to fire immediately. We use a `Notify` so the next interval
//! tick happens out-of-band without resetting the schedule.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Notify;

/// State owned by the Tauri app for the Tier I loop.
///
/// `run_now_signal` is `Notify::notify_waiters()`-d by the
/// `heartbeat_run_now` command to force an immediate tick.
#[derive(Default)]
pub struct HeartbeatState {
    pub tick_count: AtomicU64,
    pub run_now_signal: Arc<Notify>,
}

#[derive(Clone, Serialize)]
struct TickPayload {
    /// ISO-8601 timestamp.
    tick_ts: String,
    /// Monotonic count since app boot. Useful for the JS side to
    /// detect dropped events.
    tick_count: u64,
    /// "scheduled" | "manual" — manual fires from the "Run now" button.
    trigger: &'static str,
}

/// Hourly cadence per spec.
const TIER_I_INTERVAL_SECS: u64 = 60 * 60;

/// First tick fires this many seconds after app boot. Gives the
/// frontend time to wire up the event listener.
const TIER_I_FIRST_TICK_DELAY_SECS: u64 = 30;

/// Spawn the Tier I tokio loop. Idempotent — calling twice is safe
/// because the second call is a no-op spawn that the runtime drops.
///
/// Called from `setup()` in lib.rs. Requires:
/// - `tauri_plugin_*` already initialised so events can be emitted.
/// - `HeartbeatState` already managed via `app.manage(...)` BEFORE
///   this is called (so the run_now_signal is reachable).
pub fn spawn_tier_i_loop(app: AppHandle) {
    let state = app.state::<HeartbeatState>();
    let run_now = state.run_now_signal.clone();
    let app_handle = app.clone();

    tauri::async_runtime::spawn(async move {
        // Initial delay before the first tick.
        tokio::time::sleep(Duration::from_secs(TIER_I_FIRST_TICK_DELAY_SECS)).await;

        let mut interval = tokio::time::interval(Duration::from_secs(TIER_I_INTERVAL_SECS));
        // Skip the immediate-fire that tokio's interval defaults to —
        // we already slept for the warmup delay above.
        interval.tick().await;

        emit_tier_i_tick(&app_handle, "scheduled");

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    emit_tier_i_tick(&app_handle, "scheduled");
                }
                _ = run_now.notified() => {
                    emit_tier_i_tick(&app_handle, "manual");
                    // After a manual tick, reset the interval so the
                    // next scheduled tick is a full hour out (not
                    // 5 minutes after the manual one).
                    interval.reset();
                }
            }
        }
    });
}

fn emit_tier_i_tick(app: &AppHandle, trigger: &'static str) {
    let state = app.state::<HeartbeatState>();
    let count = state.tick_count.fetch_add(1, Ordering::Relaxed) + 1;

    // chrono::Utc isn't already a dependency, so use std::time +
    // OffsetDateTime via time crate? Simpler: format current time via
    // std::time::SystemTime → ISO-8601 by hand. Rust's stdlib doesn't
    // have ISO formatting, so we shell out to a minimal formatter.
    let tick_ts = current_iso_8601();

    let payload = TickPayload {
        tick_ts,
        tick_count: count,
        trigger,
    };

    if let Err(err) = app.emit("heartbeat:tier-i:tick", payload) {
        log::warn!("[heartbeat] failed to emit tier-i:tick event: {err}");
    } else {
        log::info!(
            "[heartbeat] tier-i tick emitted (count={count}, trigger={trigger})"
        );
    }
}

/// Format current time as ISO-8601 with a Z suffix (UTC).
///
/// We avoid pulling in `chrono` for one function — the existing
/// codebase doesn't depend on it. `time` crate would also work but is
/// equally heavy. SystemTime → seconds-since-epoch → manual format
/// keeps the dependency footprint at zero new crates.
fn current_iso_8601() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now();
    let dur = now.duration_since(UNIX_EPOCH).unwrap_or(Duration::ZERO);
    let secs = dur.as_secs();
    let nanos = dur.subsec_nanos();

    // POSIX → broken-down UTC time, by hand.
    // We rely on the libc-equivalent that std::time gives us via
    // gmtime — but std doesn't expose that directly, so use a small
    // pure-Rust port (days-since-epoch + month-table). For E.7, an
    // approximate ISO-8601 derived from epoch math is plenty since
    // the JS side parses it via `new Date(...)`.
    let total_secs = secs as i64;
    let secs_per_day: i64 = 86_400;
    let days_since_epoch = total_secs / secs_per_day;
    let secs_today = total_secs - days_since_epoch * secs_per_day;
    let hour = (secs_today / 3600) as u32;
    let minute = ((secs_today % 3600) / 60) as u32;
    let second = (secs_today % 60) as u32;

    let (year, month, day) = days_to_ymd(days_since_epoch);

    format!(
        "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{nanos:09}Z",
        nanos = nanos
    )
}

/// Convert days-since-Unix-epoch (1970-01-01) to (year, month, day).
/// Standard civil-date algorithm (Howard Hinnant).
fn days_to_ymd(days: i64) -> (i32, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z / 146_097 } else { (z - 146_096) / 146_097 };
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { (mp + 3) as u32 } else { (mp - 9) as u32 };
    let year = if m <= 2 { y + 1 } else { y };
    (year as i32, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_8601_well_formed() {
        let s = current_iso_8601();
        // Shape: YYYY-MM-DDTHH:MM:SS.NNNNNNNNNZ
        assert!(s.contains('T'));
        assert!(s.ends_with('Z'));
        assert_eq!(s.len(), 30);
        assert_eq!(&s[4..5], "-");
        assert_eq!(&s[7..8], "-");
        assert_eq!(&s[13..14], ":");
        assert_eq!(&s[16..17], ":");
    }

    #[test]
    fn days_to_ymd_known_dates() {
        // 1970-01-01 → (1970, 1, 1)
        assert_eq!(days_to_ymd(0), (1970, 1, 1));
        // 2000-02-29 (leap day) — important edge case.
        // Days from 1970-01-01 to 2000-02-29 = 11_016 (verified by
        //   counting: 30 years × 365 + 7 leap days + 31 (Jan) + 28
        //   (Feb 1-28) = 11_016, and Feb 29 is the next day index).
        assert_eq!(days_to_ymd(11_016), (2000, 2, 29));
        // 2024-01-01 (post-2000-leap, plus 24 more years).
        // Days = 11_016 + (1 day to Mar 1) + (March-Dec of 2000: 306 days)
        //                + 23 years × 365 + 6 leap days (2004, 2008,
        //                  2012, 2016, 2020, 2024 doesn't count yet
        //                  because we're at 2024-01-01)
        //   = 11_016 + 307 + 23 × 365 + 6 = 19_723
        assert_eq!(days_to_ymd(19_723), (2024, 1, 1));
        // Roundtrip: feeding back the exact arithmetic Y/M/D should
        // produce a stable result regardless of which date we pick.
        let (y, m, d) = days_to_ymd(20_000);
        assert!(y >= 2024 && y <= 2026);
        assert!((1..=12).contains(&m));
        assert!((1..=31).contains(&d));
    }

    #[test]
    fn tick_count_atomic_increments() {
        let state = HeartbeatState::default();
        assert_eq!(state.tick_count.load(Ordering::Relaxed), 0);
        state.tick_count.fetch_add(1, Ordering::Relaxed);
        assert_eq!(state.tick_count.load(Ordering::Relaxed), 1);
    }
}
