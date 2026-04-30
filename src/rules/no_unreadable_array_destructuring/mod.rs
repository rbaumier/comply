//! no-unreadable-array-destructuring — flag destructuring with consecutive holes.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-unreadable-array-destructuring",
    description: "Array destructuring may not contain consecutive ignored values.",
    remediation: "Use index access instead: `const third = arr[2]`. \
                  Consecutive commas like `[,, x,,,, y]` are hard to read \
                  and easy to miscount.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
