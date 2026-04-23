//! proper-arrows-name — require arrow functions to have a resolvable name.
//!
//! Anonymous arrows (callback inlined at a call site, default export, etc.)
//! show up as `<anonymous>` in stack traces and profiler output. The rule
//! passes when the arrow has a name via:
//! - `const foo = () => {}` (variable binding)
//! - `{ foo: () => {} }` (property key)
//! - `class X { foo = () => {} }` (class field)
//! - `x.foo = () => {}` (member assignment)

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "proper-arrows-name",
    description: "Anonymous arrow functions show up as `<anonymous>` in stack traces.",
    remediation: "Assign the arrow to a named binding (`const fn = () => ...`), or use a \
                  named `function` expression. Inline arrows passed directly to callbacks \
                  should be hoisted to a named binding when they're non-trivial.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["style"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
