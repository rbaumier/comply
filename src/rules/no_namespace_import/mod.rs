//! no-namespace-import

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "no-namespace-import",
    description: "Namespace import (`import * as`) — prefer named imports.",
    remediation: "Replace `import * as X from 'y'` with named imports `import { a, b } from 'y'`. Namespace imports defeat tree-shaking and obscure the actual API surface.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["imports"],
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
