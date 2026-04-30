//! no-unassigned-import

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-unassigned-import",
    description: "Side-effect import with no specifiers — assign the import or remove it.",
    remediation: "Import specific bindings (`import { x } from '…'`) or remove the import if the side-effect is unnecessary. CSS/style imports are allowed.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["imports"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
