//! no-double-cast — reject `as X as Y` double casts.

mod rust;
mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-double-cast",
    description: "Double casts `as X as Y` hide misaligned types.",
    remediation: "Remove the double cast and fix the real misalignment. \
                  Either align the producer's type with the consumer's, \
                  or validate the value at the boundary using a type guard \
                  or Zod schema that actually checks the runtime shape.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family_with_rust!(META, typescript, rust)
}
