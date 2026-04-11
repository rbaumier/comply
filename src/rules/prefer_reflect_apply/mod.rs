//! prefer-reflect-apply — prefer `Reflect.apply()` over `Function#apply()`.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-reflect-apply",
    description: "Prefer `Reflect.apply(fn, thisArg, args)` over `fn.apply(thisArg, args)`.",
    remediation: "Replace `fn.apply(ctx, args)` with `Reflect.apply(fn, ctx, args)`. \
                  `Reflect.apply` cannot be overridden and makes the intent explicit.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
