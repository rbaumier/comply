//! html-no-undeferred-third-party — `<script src="https://...">` without
//! `defer` or `async` blocks HTML parsing for a cross-origin fetch.

mod react;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "html-no-undeferred-third-party",
    description: "Third-party `<script>` without `defer`/`async` blocks parsing.",
    remediation: "Add `defer` or `async` to external `<script>` tags, or load \
                  them via `next/script` with `strategy=\"lazyOnload\"`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["performance"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, react)
}
