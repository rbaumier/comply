//! react-no-blocking-log-after-mutation — `await log()`/`await track()` after a main `await`.

mod react;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-blocking-log-after-mutation",
    description: "Awaiting a telemetry/log call after a main mutation in a server action \
                  delays the response for every request.",
    remediation: "Fire-and-forget the telemetry call (drop the `await`) or schedule it \
                  via `after()` / `waitUntil()` so the response ships first.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, react)
}
