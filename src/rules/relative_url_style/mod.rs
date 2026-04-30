//! relative-url-style

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "relative-url-style",
    description: "Remove the `./` prefix from relative URLs in `new URL()`.",
    remediation: "Remove the leading `./` from the first argument of `new URL()`: \
                  use `new URL('file.js', base)` instead of `new URL('./file.js', base)`. \
                  The `./` is redundant in URL resolution.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
