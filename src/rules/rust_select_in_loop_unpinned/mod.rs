//! rust-select-in-loop-unpinned — a `select!` branch whose future is *built* in
//! the branch itself is rebuilt from scratch on every turn of the surrounding
//! loop. Whenever another branch completes first, `select!` drops the losing
//! futures, and with them whatever progress they had made: bytes already read
//! off the socket, a half-decoded frame, a query already in flight. The next
//! iteration starts that work over, and the lost bytes are gone for good.
//!
//! The rule fires on a `select!` (`tokio::select!`, `futures::select!`) inside a
//! `loop` / `while` / `for` body when a branch reads
//! `pattern = some_call(..)` or `pattern = receiver.method(..)` — a future
//! constructed on the spot.
//!
//! A branch selecting on a binding or on `&mut binding` is already pinned
//! outside the loop and is fine, and so is a call to one of the cancel-safe
//! primitives (`recv`, `tick`, `cancelled`, `changed`, `notified`, `accept`,
//! `readable`, `writable`, `sleep`), which lose nothing when dropped.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rust-select-in-loop-unpinned",
    description: "A `select!` branch that builds its own future inside a loop loses that future's partial progress every time another branch wins.",
    remediation: "Build the future once before the loop and pin it — `let fut = read_frame(&mut conn); tokio::pin!(fut);` — then poll it from the branch as `r = &mut fut => …`, so a losing round keeps what it had already read.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["rust", "async"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
