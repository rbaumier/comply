//! drizzle-prefer-select-columns

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "drizzle-prefer-select-columns",
    description: "`db.select()` with no argument fetches every column — list explicitly the columns you actually need.",
    remediation: "Pass a column projection: `db.select({ id: users.id, email: users.email })`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["performance", "drizzle", "database"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
