//! html-no-script-without-defer — `<script src="...">` without `defer` or
//! `async` blocks the parser and delays First Contentful Paint.

mod react;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "html-no-script-without-defer",
    description: "`<script src=\"...\">` without `defer` or `async` blocks HTML parsing.",
    remediation: "Add `defer` (preserves execution order, runs after parse) for DOM-dependent \
                  scripts, or `async` (runs as soon as it loads) for independent scripts. \
                  Without either, the browser stops parsing HTML to fetch and execute the \
                  script, delaying First Contentful Paint.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["performance"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, react)
}
