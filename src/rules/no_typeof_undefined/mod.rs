//! no-typeof-undefined — flag `typeof x === 'undefined'`.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-typeof-undefined",
    description: "Compare with `undefined` directly instead of using `typeof`.",
    remediation: "Replace `typeof x === 'undefined'` with `x === undefined`. \
                  Modern JS engines handle `undefined` safely; the `typeof` \
                  guard is no longer necessary.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
