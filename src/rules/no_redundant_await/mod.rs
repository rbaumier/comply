//! no-redundant-await — flag `return await x` outside of try blocks.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

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
    crate::register_ts_family!(META, typescript)
}
