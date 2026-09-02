//! rust-unbuffered-file-io-in-loop — a raw `File` performs one `read(2)` /
//! `write(2)` syscall per call. Reading or writing it from inside a loop
//! therefore pays a kernel round-trip per iteration, where a `BufReader` /
//! `BufWriter` would amortise the same work over 8 KiB blocks.
//!
//! The rule fires on a `let` that binds an unbuffered `File`
//! (`File::open`, `File::create`, an `OpenOptions::…open` chain) when the
//! bound handle is later read from or written to inside a loop body in the
//! same function. A handle wrapped in a buffer — at the binding or anywhere
//! later in the function — is not flagged, and neither is a handle used only
//! for whole-file or non-`io` operations.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rust-unbuffered-file-io-in-loop",
    description: "Unbuffered `File` read or written inside a loop — one syscall per iteration.",
    remediation: "Buffer the handle at the binding: `let mut f = BufReader::new(File::open(path)?);` \
                  for reads, `let mut f = BufWriter::new(File::create(path)?);` for writes. \
                  A `BufWriter` must be flushed explicitly — call `f.flush()?` before it drops, \
                  otherwise a failing final write is silently swallowed in `Drop`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["rust", "performance"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
