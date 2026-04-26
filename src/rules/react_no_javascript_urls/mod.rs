//! react-no-javascript-urls

mod react;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-javascript-urls",
    description: "Do not use `javascript:` URLs in JSX `href` / `src` / `action`.",
    remediation: "`javascript:` URLs execute arbitrary code and are an XSS vector. Use an `onClick` handler for behaviour or a real URL for navigation.",
    severity: Severity::Error,
    doc_url: Some("https://react.dev/reference/react-dom/components/common#javascript-urls-are-blocked"),
    categories: &["react", "security"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, react)
}
