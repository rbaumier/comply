//! k8s-dnsconfig-options — pods should set dnsConfig.options for DNS tuning.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "k8s-dnsconfig-options",
    description: "Pods should set dnsConfig.options (e.g. `ndots:2`) to reduce DNS lookup latency.",
    remediation: "Add `dnsConfig.options: [{name: ndots, value: '2'}]` to the pod spec.",
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
