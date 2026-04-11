//! playwright-no-unsafe-references — flag `page.evaluate()` with only a function argument (no explicit args).

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "playwright-no-unsafe-references",
    description: "`page.evaluate()` runs in the browser — outer-scope variables are not available unless passed as the second argument.",
    remediation: "Pass captured variables as the second argument to \
                  `page.evaluate((arg) => { ... }, arg)`. Variables from \
                  the Node.js scope are not serialized into the browser \
                  context automatically — they will be `undefined` at \
                  runtime.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["testing"],
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
