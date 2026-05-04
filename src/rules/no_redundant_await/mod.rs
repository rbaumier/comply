//! no-redundant-await — flag `return await x` outside of try blocks.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-redundant-await",
    description: "`return await` outside a try block is redundant.",
    remediation: "Drop the `await` — an `async` function already wraps its \
                  return value in a Promise, so `return await p` is equivalent \
                  to `return p` but adds a microtask. Keep `return await` only \
                  inside a `try` block, where it affects catch semantics.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["async"],
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
