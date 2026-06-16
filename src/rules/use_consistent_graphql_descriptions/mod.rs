//! use-consistent-graphql-descriptions

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "use-consistent-graphql-descriptions",
    description: "Mixing block (\"\"\"…\"\"\") and inline (\"…\") description styles in one GraphQL schema makes it harder to read and to grep.",
    remediation: "Write every description in the configured style. The default is `block` (triple-quoted); set `[rules.use-consistent-graphql-descriptions] style = \"inline\"` to require single-quoted descriptions instead.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["graphql"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::GraphQl, Backend::Text(Box::new(text::Check)))],
    }
}
