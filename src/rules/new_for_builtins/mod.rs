//! new-for-builtins — enforce `new` for builtins that need it, disallow for Symbol/BigInt.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "new-for-builtins",
    description: "Enforce `new` for constructors and disallow it for `Symbol`/`BigInt`.",
    remediation: "Use `new Map()` instead of `Map()` for constructors that \
                  require it. Conversely, use `Symbol()` and `BigInt()` without \
                  `new` — they are factory functions, not constructors.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
