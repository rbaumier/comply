//! react-no-javascript-urls

mod oxc_typescript;
#[cfg(test)]
mod react;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-javascript-urls",
    description: "Do not use `javascript:` URLs in JSX `href` / `src` / `action`.",
    remediation: "`javascript:` URLs execute arbitrary code and are an XSS vector. Use an `onClick` handler for behaviour or a real URL for navigation.",
    severity: Severity::Error,
    doc_url: Some(
        "https://react.dev/reference/react-dom/components/common#javascript-urls-are-blocked",
    ),
    categories: &["react", "security"],
};

pub fn register() -> RuleDef {
    let backends = vec![
        (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
        (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
        (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
    ];
    RuleDef {
        meta: META,
        backends,
    }
}
