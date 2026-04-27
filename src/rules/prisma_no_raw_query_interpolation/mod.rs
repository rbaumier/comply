//! prisma-no-raw-query-interpolation — string-built `$queryRaw`/`$executeRaw`.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "prisma-no-raw-query-interpolation",
    description: "`$queryRaw`/`$executeRaw` called with a non-tagged string concatenates input — SQL injection risk.",
    remediation: "Use `$queryRaw\\`SELECT ... ${value}\\`` (tagged template) or `$queryRawUnsafe(sql, ...params)` with placeholders.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["prisma", "security"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
