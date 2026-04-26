//! react-no-invalid-html-attribute — invalid `rel` attribute values.

mod vue;
mod react;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-invalid-html-attribute",
    description: "Invalid value in HTML `rel` attribute.",
    remediation: "Use a valid `rel` value. Common valid values for `<a>` include \
                  `noopener`, `noreferrer`, `nofollow`. For `<link>` they include \
                  `stylesheet`, `icon`, `preload`, `prefetch`.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/jsx-eslint/eslint-plugin-react/blob/master/docs/rules/no-invalid-html-attribute.md"),
    categories: &["react"],
};

pub fn register() -> RuleDef {
    let mut backends = crate::register_ts_family!(META, react).backends;
    backends.push((Language::Vue, Backend::Text(Box::new(vue::Check))));
    RuleDef { meta: META, backends }
}
