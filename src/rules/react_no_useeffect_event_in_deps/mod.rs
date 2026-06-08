//! react-no-useeffect-event-in-deps — `useEffectEvent` returns a stable
//! reference; passing it in a `useEffect` dependency array is meaningless
//! (and the official guidance is to never do it).

mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-useeffect-event-in-deps",
    description: "Values returned by `useEffectEvent` must not appear in dependency arrays.",
    remediation: "Remove the effect-event from the deps array — its identity is intentionally stable. \
                  Capture other variables it reads in the deps directly.",
    severity: Severity::Error,
    doc_url: Some("https://react.dev/reference/react/experimental_useEffectEvent"),
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
