//! react-no-access-state-in-setstate — `this.state` inside `setState`.

mod oxc_typescript;
#[cfg(test)]
mod react;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-access-state-in-setstate",
    description: "`this.state` inside `setState()` reads stale state.",
    remediation: "Use the updater callback form: `this.setState(prevState => ({ \
                  count: prevState.count + 1 }))`. Reading `this.state` inside \
                  `setState` may read a stale value because React batches state \
                  updates.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react"],
};

pub fn register() -> RuleDef {
    let backends = vec![
        (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
        (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
        (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
    ];
    RuleDef {
        meta: META,
        backends,
    }
}
