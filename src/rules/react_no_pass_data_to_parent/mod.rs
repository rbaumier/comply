mod oxc_typescript;
#[cfg(test)]
mod react;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-pass-data-to-parent",
    description: "`useEffect` that only calls a parent callback to pass data up — lift state instead.",
    remediation: "Move the state to the parent component and pass down a setter, or restructure to avoid the effect.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react"],

    // Test-harness components legitimately use `useEffect(() => onReady(data), [])`
    // to expose a ref/state to the outer test — the only way to get an imperative
    // handle into the test body. There is no production render-cycle concern in a
    // fixture, so the rule is scoped out of test directories.
    skip_in_test_dir: true,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
