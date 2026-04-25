"""Firestore writer.

Two write surfaces:
1. ``tasks/{action.id}`` — KanbanTaskAction → engagement's Kanban board.
2. ``heartbeat_health/{tickId}`` — one telemetry doc per tick.

Tenant/engagement IDs are denormalised onto each doc so the existing
Tauri app's listeners (which query by tenantId + engagementId) pick them
up without schema changes.

Auth: Firebase Admin SDK with a service-account JSON. Initialised once
per process; subsequent calls re-use the same client.

Spec: docs/specs/m3-phase-e-autonomous-heartbeat.md §Outputs and
§Telemetry.
"""

from __future__ import annotations

import logging
import threading
from typing import TYPE_CHECKING, Any

from heartbeat.actions import KanbanTaskAction
from heartbeat.outputs.secrets import OutputSecrets
from heartbeat.telemetry import HeartbeatHealthDoc

if TYPE_CHECKING:
    from google.cloud.firestore import Client as FirestoreClient

logger = logging.getLogger("heartbeat.outputs.firestore")


class FirestoreError(RuntimeError):
    """Raised by Firestore writes when the SDK or the network fails.

    Wraps the underlying SDK exception with a typed ``error_code`` so the
    dispatcher can record it in the audit log + telemetry without leaking
    SDK internals upward.
    """

    def __init__(self, message: str, *, error_code: str = "firestore_error") -> None:
        super().__init__(message)
        self.error_code = error_code


# Process-wide client cache. The Admin SDK does not like being initialised
# twice with different credentials, so we keep one app + one client per
# (credentials_path) and re-use them.
_CLIENT_LOCK = threading.Lock()
_CLIENT_CACHE: dict[str, Any] = {}


def get_firestore_client(secrets: OutputSecrets) -> Any:
    """Return a process-wide-cached Firestore client.

    Lazy-imports firebase_admin so unit tests that pass ``_client=`` to
    ``write_*`` never need the SDK installed.
    """

    if secrets.firestore_credentials_path is None:
        raise FirestoreError(
            "no Firebase service-account key configured. Populate "
            "FIREBASE_SA_KEY_PATH (or GOOGLE_APPLICATION_CREDENTIALS) "
            "in /etc/ikrs-heartbeat/secrets.env.",
            error_code="missing_credentials",
        )
    key = str(secrets.firestore_credentials_path)
    with _CLIENT_LOCK:
        if key in _CLIENT_CACHE:
            return _CLIENT_CACHE[key]

        try:
            import firebase_admin
            from firebase_admin import credentials, firestore
        except ImportError as exc:  # pragma: no cover - pre-install sanity
            raise FirestoreError(
                f"firebase-admin not installed: {exc}",
                error_code="sdk_import_failed",
            ) from exc

        try:
            cred = credentials.Certificate(key)  # type: ignore[no-untyped-call]
            # Use a uniquely-named app per key path so tests with multiple
            # paths don't collide on firebase_admin._DEFAULT_APP_NAME.
            app_name = f"heartbeat-{abs(hash(key)) % 10**8}"
            try:
                app = firebase_admin.get_app(app_name)  # type: ignore[no-untyped-call]
            except ValueError:
                app = firebase_admin.initialize_app(  # type: ignore[no-untyped-call]
                    cred, name=app_name
                )
            client = firestore.client(app=app)
        except Exception as exc:  # noqa: BLE001
            raise FirestoreError(
                f"Firebase Admin SDK init failed: {type(exc).__name__}: {exc}",
                error_code="init_failed",
            ) from exc

        _CLIENT_CACHE[key] = client
        return client


def write_kanban_task(
    *,
    tenant_id: str,
    engagement_id: str,
    action: KanbanTaskAction,
    client: FirestoreClient | Any | None = None,
) -> None:
    """Upsert a Kanban task to ``tasks/{action.id}``.

    Idempotent: re-running with the same ``action.id`` overwrites the
    existing doc, so a retried tick can't double-create. The tick
    orchestrator's UUIDs make collisions astronomically unlikely.
    """

    if client is None:
        raise ValueError("Firestore client must be provided (use get_firestore_client).")

    doc = {
        "tenantId": tenant_id,
        "engagementId": engagement_id,
        "id": action.id,
        "title": action.title,
        "description": action.description,
        "priority": action.priority,
        "rationale": action.rationale,
        "source": "heartbeat-tier-ii",
        "createdAt": action.emitted_at,
        "type": "kanban_task",
    }
    try:
        client.collection("tasks").document(action.id).set(doc, merge=False)
    except Exception as exc:  # noqa: BLE001
        raise FirestoreError(
            f"task write failed for {action.id}: {type(exc).__name__}: {exc}",
            error_code="task_write_failed",
        ) from exc


def write_heartbeat_health(
    *,
    doc: HeartbeatHealthDoc,
    tick_id: str,
    client: FirestoreClient | Any | None = None,
) -> None:
    """Write one ``heartbeat_health/{tick_id}`` doc.

    Spec §Telemetry. ``tick_id`` is a deterministic per-tick ID so a
    retried tick (same tick_id) overwrites the prior attempt. Caller
    generates this — typically ``f"{tenantId}-{engagementId}-{tickTs}"``.
    """

    if client is None:
        raise ValueError("Firestore client must be provided (use get_firestore_client).")

    try:
        client.collection("heartbeat_health").document(tick_id).set(
            doc.to_dict(), merge=False
        )
    except Exception as exc:  # noqa: BLE001
        raise FirestoreError(
            f"heartbeat_health write failed for {tick_id}: "
            f"{type(exc).__name__}: {exc}",
            error_code="health_write_failed",
        ) from exc


def reset_client_cache_for_tests() -> None:
    """Test-only: drop the cached client so the next call re-initialises."""

    with _CLIENT_LOCK:
        _CLIENT_CACHE.clear()
