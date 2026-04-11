//! react-no-access-state-in-setstate — `this.state` inside `setState`.

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

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
    RuleDef {
        meta: META,
        backends: TS_FAMILY
            .iter()
            .map(|&lang| (lang, Backend::Text(Box::new(text::Check))))
            .collect(),
    }
}
