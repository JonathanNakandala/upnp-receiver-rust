//! Simulated [`MediaBackend`](super::MediaBackend) used when no real
//! playback engine is available (or explicitly requested via
//! `--media-backend dummy`). Runs a subprocess that tracks a plain position
//! counter — no real audio decoding — over the same TCP/JSON protocol a
//! future real out-of-process backend could use.

pub mod backend;
pub mod error;
pub mod manager;
pub mod process;
pub mod protocol;

pub use backend::DummyBackend;
pub use manager::DummyBackendManager;
pub use process::run_dummy_process;
