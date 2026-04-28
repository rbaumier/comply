//! react-no-unescaped-entities — unescaped HTML entities in JSX text.

mod react;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-unescaped-entities",
    description: "Unescaped `>`, `\"`, `'`, or `}` in JSX text can cause unexpected rendering.",
    remediation: "Replace the character with its HTML entity: `>` with `&gt;`, \
                  `\"` with `&quot;`, `'` with `&apos;`, `}` with `&#125;`.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/jsx-eslint/eslint-plugin-react/blob/master/docs/rules/no-unescaped-entities.md"),
    categories: &["react"],
};

pub fn register() -> RuleDef {
    let backends = crate::register_ts_family!(META, react).backends;
    RuleDef { meta: META, backends }
}
