//! react-prefer-use-optimistic — manually setting optimistic state then
//! rolling back inside a `catch { setState(...) }` is the exact pattern
//! `useOptimistic` automates and gets right.

mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-prefer-use-optimistic",
    description: "Manual try/catch rollback of state — use `useOptimistic` for cleaner, race-safe code.",
    remediation: "Switch to `const [optimistic, addOptimistic] = useOptimistic(state, reducer);` \
                  and call `addOptimistic(...)` before the action — React handles rollback.",
    severity: Severity::Warning,
    doc_url: Some("https://react.dev/reference/react/useOptimistic"),
    categories: &["react"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (
                Language::TypeScript,
                Backend::Text(Box::new(typescript::Check)),
            ),
            (
                Language::JavaScript,
                Backend::Text(Box::new(typescript::Check)),
            ),
            (Language::Tsx, Backend::Text(Box::new(typescript::Check))),
        ],
    }
}
