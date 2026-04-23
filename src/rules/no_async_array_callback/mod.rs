//! no-async-array-callback — flag `arr.forEach(async ...)` and friends.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-async-array-callback",
    description: "`async` callback passed to a non-awaiting array method.",
    remediation: "`forEach`/`map`/`filter`/`some`/`every`/`find` don't await their \
                  callbacks — your async work runs in parallel (or not at all) and \
                  rejections become unhandled. Use `for (const x of arr)` with \
                  `await` inside, or `Promise.all(arr.map(async ...))` when you \
                  want parallel + awaited.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["async"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
