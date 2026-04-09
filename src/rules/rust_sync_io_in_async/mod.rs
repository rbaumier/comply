//! rust-sync-io-in-async — synchronous I/O blocks the runtime.
//!
//! Calling `std::fs::*` or `std::net::TcpStream::*` from inside an
//! `async fn` blocks the OS thread on a syscall. With tokio's
//! multi-thread runtime, that worker thread can no longer poll any
//! other future for the entire duration of the syscall. Use the
//! async equivalents (`tokio::fs::*`, `tokio::net::TcpStream::*`)
//! or wrap the sync call in `tokio::task::spawn_blocking`.

mod rust;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "rust-sync-io-in-async",
    description: "Synchronous I/O calls inside `async fn` block the runtime.",
    remediation: "Replace `std::fs::*` with `tokio::fs::*`, `std::net::TcpStream::*` \
                  with `tokio::net::TcpStream::*`, etc. If no async equivalent \
                  exists, wrap the call in `tokio::task::spawn_blocking(|| ...)` \
                  so it runs on the dedicated blocking pool.",
    severity: Severity::Error,
    doc_url: None,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Rust, Backend::TreeSitter(Box::new(rust::Check)))],
    }
}
