//! react-no-invalid-html-attribute — invalid `rel` attribute values.

mod react;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-invalid-html-attribute",
    description: "Invalid value in HTML `rel` attribute.",
    remediation: "Use a valid `rel` value. Common valid values for `<a>` include \
                  `noopener`, `noreferrer`, `nofollow`. For `<link>` they include \
                  `stylesheet`, `icon`, `preload`, `prefetch`.",
    severity: Severity::Warning,
    doc_url: Some(
        "https://github.com/jsx-eslint/eslint-plugin-react/blob/master/docs/rules/no-invalid-html-attribute.md",
    ),
    categories: &["react"],
};

pub fn register() -> RuleDef {
    let backends = crate::register_ts_family!(META, react).backends;
    RuleDef {
        meta: META,
        backends,
    }
}
