//! zod-no-optional-and-default-together — chaining `.optional()` after
//! `.default()` (or vice versa) is redundant and hides intent.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "zod-no-optional-and-default-together",
    description: "Chaining `.optional()` and `.default()` on the same schema is redundant.",
    remediation: "Pick one: `.default(x)` already makes the field effectively \
                  optional (missing input → default). Remove the `.optional()` \
                  call — combining the two makes the schema accept `undefined` \
                  without applying the default, which is almost never intended.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["zod"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
