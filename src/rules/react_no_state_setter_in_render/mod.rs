//! react-no-state-setter-in-render — calling a `useState` setter
//! unconditionally in the component body causes an infinite render loop. A
//! setter guarded by an `if`/ternary whose test references its paired state
//! variable is exempt: it is the React-sanctioned "adjust state during render"
//! pattern, which terminates once the state matches.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-state-setter-in-render",
    description: "`setState(...)` called directly during render — triggers an infinite render loop.",
    remediation: "Move the setter into an event handler or `useEffect`. If you need to derive state, \
                  compute it during render instead of storing it.",
    severity: Severity::Error,
    doc_url: Some("https://react.dev/learn/you-might-not-need-an-effect"),
    categories: &["react"],

    skip_in_test_dir: false,
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
