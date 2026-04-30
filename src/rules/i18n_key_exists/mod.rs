mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "i18n-key-exists",
    description: "t() key is malformed (consecutive/leading/trailing dots, empty segments, or non-alphanumeric chars) and cannot resolve to a locale entry. Cross-file existence checks aren't performed.",
    remediation: "Fix the key shape so it matches `domain.subkey` with alphanumeric segments separated by single dots.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["i18n"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
