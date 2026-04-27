//! react-no-setstate-no-cancel-flag — async work inside `useEffect` that
//! `setState`s after `await` without a cancellation flag triggers warnings
//! ("can't perform a state update on an unmounted component") and can leak
//! stale results when the effect re-runs.

mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-setstate-no-cancel-flag",
    description: "`useEffect` awaits then calls `setState` without a cancellation flag — risks updating an unmounted component.",
    remediation: "Track a `cancelled` flag inside the effect and skip the setter when set; \
                  return a cleanup that flips it. Alternative: use `AbortController`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Text(Box::new(typescript::Check))),
            (Language::JavaScript, Backend::Text(Box::new(typescript::Check))),
            (Language::Tsx, Backend::Text(Box::new(typescript::Check))),
        ],
    }
}
