//! Tier I heartbeat — runs while the Tauri app is open.
//!
//! Per `docs/specs/m3-phase-e-autonomous-heartbeat.md` §Tier I:
//! - tokio interval every hour while the app is open.
//! - Verify Tier II's actions: read recent `heartbeat_health` docs from
//!   Firestore, sanity-check, escalate anything questionable to a UI
//!   banner.
//! - Also handles user-initiated "Run now" button.
//!
//! Architecture decision (E.7): the Rust side is a minimal cadence
//! driver. It emits a Tauri event (`heartbeat:tier-i:tick`) on each
//! interval and exposes a `heartbeat_run_now` command for the UI's
//! "Run now" button. The actual verification logic — reading Firestore
//! `heartbeat_health` docs, computing a verdict, emitting a UI banner
//! — lives on the JS side, which already has live Firestore listeners
//! and the engagement context. This keeps the Rust surface small and
//! the heavy lifting where the data already lives.

mod tick;

pub use tick::{spawn_tier_i_loop, HeartbeatState};
