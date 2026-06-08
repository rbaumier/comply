//! compose-healthcheck-required — every compose service should declare a
//! `healthcheck:` so orchestrators can tell whether it's actually serving.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "compose-healthcheck-required",
    description: "Every compose service should declare a `healthcheck:`.",
    remediation: "Add a `healthcheck:` block to the service so Docker can \
                  distinguish 'process running' from 'service ready'. \
                  `depends_on` with `condition: service_healthy` only works \
                  when the upstream actually has a healthcheck. If the image \
                  ships its own `HEALTHCHECK` and you really want to inherit \
                  it, add `healthcheck: { disable: false }` (or any explicit \
                  block) to opt in.",
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
