//! Tauri commands exposed to the frontend for the Tier I heartbeat.
//!
//! Currently just one: `heartbeat_run_now` notifies the Tier I tokio
//! loop to fire an out-of-band tick. The UI's "Run now" button binds
//! to this.

use tauri::State;

use crate::heartbeat::HeartbeatState;

/// Trigger an immediate Tier I tick. Idempotent: calling twice in
/// quick succession produces two events (the loop respects each
/// notify), but a flood of calls is debounced to one tick per
/// `Notify::notify_waiters` cycle.
///
/// Returns the current monotonic tick count so the UI can show
/// "tick #N fired" feedback.
#[tauri::command]
pub async fn heartbeat_run_now(state: State<'_, HeartbeatState>) -> Result<u64, String> {
    state.run_now_signal.notify_waiters();
    let count = state
        .tick_count
        .load(std::sync::atomic::Ordering::Relaxed);
    Ok(count)
}
