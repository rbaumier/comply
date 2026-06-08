//! k8s-disallow-privilege-escalation — securityContext.allowPrivilegeEscalation must be false.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "k8s-disallow-privilege-escalation",
    description: "Containers must set securityContext.allowPrivilegeEscalation: false.",
    remediation: "Add `securityContext: { allowPrivilegeEscalation: false }` to each container.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["kubernetes"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Yaml, Backend::TreeSitter(Box::new(text::Check)))],
    }
}
