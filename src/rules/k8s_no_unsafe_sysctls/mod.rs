//! k8s-no-unsafe-sysctls — Pod specs must not declare unsafe sysctls.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "k8s-no-unsafe-sysctls",
    description: "Pod securityContext.sysctls must not contain unsafe namespaced sysctls.",
    remediation: "Remove unsafe sysctls (kernel.msg*, kernel.sem*, kernel.shm*, fs.mqueue.*, net.*) or restrict via PodSecurityPolicy / kubelet allowed-unsafe-sysctls.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["kubernetes"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Yaml, Backend::TreeSitter(Box::new(text::Check)))],
    }
}
