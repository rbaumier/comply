//! no-boolean-flag-param — split boolean-flagged functions into two.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-boolean-flag-param",
    description: "Boolean flag parameters hide two behaviors behind one signature.",
    remediation: "Split into two named functions. \
                  `sendNotification(msg, isUrgent)` → \
                  `sendUrgentNotification(msg)` + `sendNormalNotification(msg)`. \
                  A ternary or options object is not a fix — the boolean \
                  must disappear from the signature.",
    severity: Severity::Error,
    doc_url: None,
};pub fn register() -> RuleDef {
    crate::register_ts_family_with_clippy_marker!(META, typescript, "clippy::fn_params_excessive_bools")
}
