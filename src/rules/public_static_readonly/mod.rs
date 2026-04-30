//! public-static-readonly

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "public-static-readonly",
    description: "`public static` fields without `readonly` allow accidental mutation.",
    remediation: "Add `readonly` to `public static` fields: `public static readonly X = ...`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
