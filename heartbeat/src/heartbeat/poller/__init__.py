"""Phase G — Telegram bot poller (bidirectional commands).

Thin message receiver: reads Telegram updates, writes to Firestore
command queue, optionally triggers ad-hoc ticks. The tick is the sole
writer to ikrs_tasks, local files, and observations.
"""
