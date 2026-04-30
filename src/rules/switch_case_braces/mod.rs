//! switch-case-braces — require braces around `case` clause bodies.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "switch-case-braces",
    description: "Missing braces in `case` clause.",
    remediation: "Wrap `case` clause body in `{ }` to create a block scope. \
                  Without braces, `let`/`const`/`class`/`function` declarations \
                  leak into the enclosing `switch` scope and can cause \
                  `SyntaxError` or surprising variable sharing between cases.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
