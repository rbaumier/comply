mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "i18n-use-singleton-outside-react",
    description: "useTranslation() called outside a React component.",
    remediation: "Use the `i18n.t()` singleton in non-React contexts (head(), Zod error maps, QueryCache handlers).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["i18n"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
