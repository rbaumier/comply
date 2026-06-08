//! compose-depends-on-condition — `depends_on:` as a simple list only waits
//! for container start, not readiness. Use the long form with `condition:`.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "compose-depends-on-condition",
    description: "`depends_on:` must use the long form with `condition: service_healthy` (or `_completed_successfully`).",
    remediation: "Rewrite `depends_on: [db]` as a map where each dependency declares `condition:`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["docker", "docker-compose"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Yaml, Backend::TreeSitter(Box::new(text::Check)))],
    }
}
