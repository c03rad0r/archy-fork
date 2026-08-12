// pub(crate): `detect_ollama()` is reused by
// `crate::assistant::backends::select_backend` (13-10, D-04) — the
// existing probe, not a second one.
pub(crate) mod assistant;
mod bitcoin_ops;
mod flash;
mod messaging;
mod safety;
mod status;
mod typed_messages;
