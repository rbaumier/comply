//! tailwind-prefer-cn-utility

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tailwind-prefer-cn-utility",
    description: "Ternary or concatenation in `className` should use `cn()` or `clsx()` for readability.",
    remediation: "Replace `className={x ? 'a' : 'b'}` with `className={cn('a', { b: x })}`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tailwind"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
