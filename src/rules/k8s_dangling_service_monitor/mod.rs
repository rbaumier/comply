//! k8s-dangling-service-monitor — ServiceMonitor.spec.selector.matchLabels must
//! resolve to at least one workload in the same namespace.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "k8s-dangling-service-monitor",
    description: "ServiceMonitor.spec.selector.matchLabels must match at least one workload in the namespace.",
    remediation: "Align the ServiceMonitor selector with a Service/workload that exists in the namespace, or remove the ServiceMonitor.",
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
