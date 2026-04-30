//! js-no-moment — moment.js is 300kB+; prefer `date-fns`, `dayjs`, or
//! the native `Temporal` API.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "js-no-moment",
    description: "moment.js is 300kB+ — use `date-fns`, `dayjs`, or `Temporal` instead.",
    remediation: "Replace `moment` with a smaller library (`date-fns`, `dayjs`) or the \
                  native `Temporal` API.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["bundle-size"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
