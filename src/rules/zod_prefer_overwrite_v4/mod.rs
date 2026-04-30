//! zod-prefer-overwrite-v4 — prefer `.overwrite()` over `.transform()` when the
//! transform returns a value of the same shape.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "zod-prefer-overwrite-v4",
    description: "`.transform()` widens the output type to whatever the callback \
                  returns, which breaks `z.input` vs `z.output` parity. When the \
                  callback returns a value of the same shape, Zod v4's `.overwrite()` \
                  keeps the input type intact.",
    remediation: "Replace `.transform(fn)` with `.overwrite(fn)` whenever `fn` returns \
                  the same shape as its input (e.g. `s => s.trim()`, `n => Math.round(n)`).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["zod"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
