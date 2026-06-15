mod text;
use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-quickfix-biome",
    description: "`quickfix.biome` is used in an editor settings file.",
    remediation: "Replace `quickfix.biome` with `source.fixAll.biome`. The `quickfix.biome` code action applies fixes from rules and other actions without coordinating between them, which can produce malformed code when two fixes touch the same lines.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["json"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Json, Backend::Text(Box::new(text::Check)))],
    }
}
