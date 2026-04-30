//! switch-case-break-position — flag `break`/`return`/`continue`/`throw`
//! placed outside the block in a `case` clause.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "switch-case-break-position",
    description: "`break`/`return` should be inside the case block, not after it.",
    remediation: "Move the `break`/`return`/`continue`/`throw` statement \
                  inside the `{ }` block of the case clause. Placing it \
                  outside creates an inconsistent style where the block looks \
                  complete but the terminator dangles after the closing brace.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
