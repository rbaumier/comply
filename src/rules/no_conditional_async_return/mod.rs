mod typescript;
use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-conditional-async-return",
    description: "Function returns `T` on one branch and `Promise<T>` on another — always return a promise for consistency.",
    remediation: "If the function is async, every branch must return a value (or `await` a promise). If sync, don't return a promise on some branches.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
