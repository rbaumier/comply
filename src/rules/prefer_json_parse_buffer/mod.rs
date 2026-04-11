//! prefer-json-parse-buffer — prefer reading a JSON file as a buffer.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-json-parse-buffer",
    description: "Prefer reading a JSON file as a buffer.",
    remediation: "Remove the `'utf-8'` / `'utf8'` encoding argument from \
                  `fs.readFileSync()` when the result is passed to `JSON.parse()`. \
                  `JSON.parse()` accepts a `Buffer` directly, which avoids an \
                  intermediate string allocation.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
