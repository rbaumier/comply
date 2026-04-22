//! drizzle-chunk-large-batch-insert — flag `db.insert(...).values([ ... ])`
//! calls whose array literal has more than the configured threshold of
//! elements. PostgreSQL caps bind parameters at 65535 per statement and
//! very large single-statement inserts also defeat the Node driver's
//! ability to stream backpressure, so chunking is the idiomatic fix.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "drizzle-chunk-large-batch-insert",
    description: "Drizzle `.values([...])` with a very large array risks exceeding bind-parameter limits.",
    remediation: "Split the input into fixed-size chunks (e.g. `chunk(rows, 500).forEach(c => db.insert(t).values(c))`).",
    severity: Severity::Warning,
    doc_url: Some("https://orm.drizzle.team/docs/insert#insert-multiple-rows"),
    categories: &["drizzle", "database"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
