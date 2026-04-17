//! regex-complexity

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "regex-complexity",
    description: "Regex pattern is overly complex (score > 20).",
    remediation: "Break the regex into smaller named patterns or use a parser. Complex regex is hard to read, test, and maintain.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality", "regex"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
