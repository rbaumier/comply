//! no-redundant-clsx

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-redundant-clsx",
    description: "`clsx()` / `cn()` called with a single static string is redundant.",
    remediation: "Remove clsx/cn wrapper when using single static string",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
