//! Shared dual-stack TCP bind helper for local OAuth callback
//! servers.
//!
//! Extracted 2026-04-22 after the Firebase-identity sign-in
//! regression: `identity_server.rs` silently bound IPv4-only while
//! its sibling `redirect_server.rs` (engagement-OAuth flow) had the
//! dual-stack fix since commit 78f807a. The parallel implementations
//! drifted. Both now delegate into this module so a third OAuth-
//! callback listener added in future (Apple Sign-in? Microsoft?
//! direct GitHub OAuth?) cannot silently regress by forgetting the
//! IPv6 bind.
//!
//! ## Why dual-stack matters on macOS
//!
//! `/etc/hosts` on macOS maps `localhost` to BOTH `127.0.0.1` AND
//! `::1`, but macOS's resolver returns `::1` first. Google's OAuth
//! redirect URI is `http://localhost:{port}/oauth/callback`; the
//! user's browser picks `::1` first. If the Rust listener only
//! bound `127.0.0.1`, the browser's TCP connect hits an unbound
//! IPv6 socket (or an unrelated process like `rapportd` that holds
//! the IPv6 wildcard on port 49152+), and the callback never
//! reaches us. The Rust task waits on its IPv4 socket until
//! timeout; the frontend's promise resolves late or not at all.
//!
//! ## Binding strategy
//!
//! For each port in `preferred..=preferred + fallback_range`:
//!   1. Try `[::1]:port` (IPv6 loopback).
//!   2. Try `127.0.0.1:port` (IPv4 loopback).
//!   3. If BOTH bound → return (v6, Some(v4), port). Accept races
//!      both via `tokio::select!`; whichever stack the browser
//!      hits first wins.
//!   4. If only ONE bound → return (that, None, port). Accept on
//!      the single listener.
//!   5. If NEITHER bound → advance to next port.
//!   6. Exhausted → Err with the range in the message.

use tokio::net::TcpListener;

/// The result of a dual-stack bind attempt — a "primary" listener
/// that MUST be used, an optional "secondary" listener on the other
/// stack, and the port that was successfully bound.
///
/// "Primary" is IPv6-first preference when available (because
/// macOS browsers prefer `::1` for `localhost`), but in IPv4-only
/// environments (CI without IPv6, kernels with IPv6 disabled) the
/// primary falls back to IPv4. Callers should treat primary as
/// "the listener that's definitely bound" and secondary as
/// "an additional listener to race via select! if present".
pub struct DualStackBind {
    pub primary: TcpListener,
    pub secondary: Option<TcpListener>,
    pub port: u16,
}

/// Attempt dual-stack bind starting from `preferred_port`, trying
/// up to `fallback_range + 1` consecutive ports. Returns both
/// listeners when available; one when only one stack is bound;
/// Err when no port in the range could be bound on either stack.
///
/// The fallback range exists to tolerate transient port collisions
/// (another dev tool, a leaked listener from a prior crash, etc.)
/// without making the user choose a new port manually.
pub async fn bind_dual_stack(
    preferred_port: u16,
    fallback_range: u16,
    context_label: &str,
) -> Result<DualStackBind, String> {
    for port in preferred_port..=preferred_port.saturating_add(fallback_range) {
        let v6 = TcpListener::bind(format!("[::1]:{port}")).await;
        let v4 = TcpListener::bind(format!("127.0.0.1:{port}")).await;
        match (v6, v4) {
            (Ok(v6), Ok(v4)) => {
                log::debug!(
                    "[{context_label}] dual-stack bind succeeded on port {port} (IPv6+IPv4)"
                );
                return Ok(DualStackBind {
                    primary: v6,
                    secondary: Some(v4),
                    port,
                });
            }
            (Ok(v6), Err(e)) => {
                log::debug!(
                    "[{context_label}] bound IPv6 only on port {port} (IPv4 failed: {e})"
                );
                return Ok(DualStackBind {
                    primary: v6,
                    secondary: None,
                    port,
                });
            }
            (Err(e), Ok(v4)) => {
                log::debug!(
                    "[{context_label}] bound IPv4 only on port {port} (IPv6 failed: {e})"
                );
                return Ok(DualStackBind {
                    primary: v4,
                    secondary: None,
                    port,
                });
            }
            (Err(_), Err(_)) => continue,
        }
    }
    Err(format!(
        "[{context_label}] could not bind to any port in range {}-{}",
        preferred_port,
        preferred_port.saturating_add(fallback_range)
    ))
}

/// Accept a single connection from either the primary or secondary
/// listener — whichever receives one first. When `secondary` is
/// None, accepts on primary alone.
///
/// Error messages include which stack failed so post-mortem logs
/// pinpoint the issue without needing source-line references.
pub async fn accept_from_either(
    bind: &DualStackBind,
    context_label: &str,
) -> Result<(tokio::net::TcpStream, std::net::SocketAddr), String> {
    match &bind.secondary {
        None => bind
            .primary
            .accept()
            .await
            .map_err(|e| format!("[{context_label}] accept failed: {e}")),
        Some(secondary) => {
            tokio::select! {
                r = bind.primary.accept() => r.map_err(|e| {
                    format!("[{context_label}] accept on primary stack failed: {e}")
                }),
                r = secondary.accept() => r.map_err(|e| {
                    format!("[{context_label}] accept on secondary stack failed: {e}")
                }),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn bind_dual_stack_returns_both_listeners_when_ipv6_supported() {
        let ipv6_ok = TcpListener::bind("[::1]:0").await.is_ok();
        if !ipv6_ok {
            eprintln!("skipping — no IPv6 loopback on this host");
            return;
        }
        let bind = bind_dual_stack(55601, 10, "test-dual")
            .await
            .expect("should bind");
        assert!(
            bind.secondary.is_some(),
            "when IPv6 loopback is available, we expect both listeners"
        );
        let primary_v6 = bind.primary.local_addr().map(|a| a.is_ipv6()).unwrap_or(false);
        assert!(primary_v6, "primary should be IPv6 when dual-stack succeeds");
    }

    #[tokio::test]
    async fn bind_dual_stack_errors_when_range_exhausted() {
        let start = 55800_u16;
        let range = 5_u16;
        let mut holders = Vec::new();
        for p in start..=start + range {
            if let Ok(l) = TcpListener::bind(format!("[::1]:{p}")).await {
                holders.push(l);
            }
            if let Ok(l) = TcpListener::bind(format!("127.0.0.1:{p}")).await {
                holders.push(l);
            }
        }
        let res = bind_dual_stack(start, range, "test-exhaust").await;
        drop(holders);
        assert!(res.is_err(), "expected Err when range is exhausted");
    }

    #[tokio::test]
    async fn bind_dual_stack_uses_fallback_when_preferred_taken() {
        let start = 55900_u16;
        // Take only the preferred port so the helper must advance.
        let _v6_hold = TcpListener::bind(format!("[::1]:{start}")).await.ok();
        let _v4_hold = TcpListener::bind(format!("127.0.0.1:{start}")).await.ok();
        let bind = bind_dual_stack(start, 10, "test-fallback")
            .await
            .expect("should find a free port within the fallback range");
        assert_ne!(
            bind.port, start,
            "should have advanced past the taken port"
        );
        assert!(
            bind.port > start && bind.port <= start + 10,
            "port {} should be within fallback range",
            bind.port,
        );
    }
}
