mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "mysql-no-multiple-statements",
    description: "`multipleStatements: true` on mysql connections amplifies SQL injection risk.",
    remediation: "Don't enable multipleStatements, it amplifies SQL injection risk.",
    severity: Severity::Error,
    doc_url: Some("https://github.com/mysqljs/mysql#multiple-statement-queries"),
    categories: &["security"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
