//! prefer-classlist-toggle

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-classlist-toggle",
    description: "Prefer `Element#classList.toggle()` over conditional `add`/`remove`.",
    remediation: "Replace `if (c) el.classList.add('x') else el.classList.remove('x')` with `el.classList.toggle('x', c)`. The `toggle` method with a force argument is cleaner and avoids conditional branching.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
