

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "drizzle-no-sql-raw-with-variable",
    description: "`sql.raw()` with a non-literal argument is a SQL injection vector.",
    remediation: "Use `sql` tagged template literals with parameterized interpolation, or `sql.identifier()` for identifiers.",
    severity: Severity::Error,
    doc_url: Some("https://orm.drizzle.team/docs/sql#sqlraw"),
    categories: &["drizzle", "security"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
