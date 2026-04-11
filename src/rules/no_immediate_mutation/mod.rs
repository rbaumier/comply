//! no-immediate-mutation — flag mutation on a variable immediately after its declaration.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-immediate-mutation",
    description: "Disallow immediate mutation after variable assignment.",
    remediation: "Chain the mutation onto the initialiser: \
                  `const arr = [3,1,2].sort()` instead of declaring then \
                  mutating on the next line. This makes the intent clearer \
                  and avoids an intermediate mutable state.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
