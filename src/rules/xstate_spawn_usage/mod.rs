mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "xstate-spawn-usage",
    description: "`spawn()` must be called inside an `assign()` action.",
    remediation: "spawn() must be called inside assign() action",
    severity: Severity::Warning,
    doc_url: Some("https://stately.ai/docs/spawn"),
    categories: &["xstate"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
