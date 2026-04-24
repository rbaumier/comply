mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-redundant-state",
    description: "`useState` whose setter is never used — the value never changes.",
    remediation: "Replace with a plain `const` or `useMemo`. A state variable that \
                  is never updated adds unnecessary re-render machinery.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
