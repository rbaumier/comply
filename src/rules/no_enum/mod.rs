//! no-enum — replace TS enums with `as const satisfies` or discriminated unions.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "no-enum",
    description: "TypeScript enums emit runtime code and don't narrow cleanly.",
    remediation: "Replace `enum` with `const X = { ... } as const satisfies \
                  Record<string, string>` for config, or a discriminated \
                  union with a `type`/`kind` field for tagged data.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["typescript"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: TS_FAMILY
            .iter()
            .map(|&lang| (lang, Backend::Oxc(Box::new(oxc_typescript::Check))))
            .collect(),
    }
}
