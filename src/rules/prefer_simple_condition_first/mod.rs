//! prefer-simple-condition-first — flag complex left operand when right is simple.

mod rust;
mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-simple-condition-first",
    description: "Prefer simple condition first in logical expressions.",
    remediation: "Swap the operands so the simple condition comes first: \
                  `if (simple && complex())` instead of `if (complex() && simple)`. \
                  Short-circuit evaluation skips the expensive right operand \
                  when the cheap left operand determines the result.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family_with_rust!(META, typescript, rust)
}
