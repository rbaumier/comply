//! i18n-no-unnecessary-trans-component — use `t()` when there is no JSX inside.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "i18n-no-unnecessary-trans-component",
    description: "`<Trans>` is for interpolating JSX children — use `t()` for plain text.",
    remediation: "Replace `<Trans i18nKey=\"x\">Plain text</Trans>` with `{t('x')}`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["i18n"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
