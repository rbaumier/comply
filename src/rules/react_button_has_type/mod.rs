//! react-button-has-type — `<button>` without explicit `type` attribute.

mod text;
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "react-button-has-type",
    description: "`<button>` without an explicit `type` attribute defaults to `submit`, which may cause unexpected form submissions.",
    remediation: "Add an explicit `type` attribute (`button`, `submit`, or `reset`) \
                  to every `<button>` element so the intent is clear.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/jsx-eslint/eslint-plugin-react/blob/master/docs/rules/button-has-type.md"),
    categories: &["react"],
};

pub fn register() -> RuleDef {
    let mut backends = crate::register_ts_family!(META, typescript).backends;
    backends.push((Language::Vue, Backend::Text(Box::new(text::Check))));
    RuleDef { meta: META, backends }
}
