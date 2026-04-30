//! shadcn-no-hr-use-separator — forbid raw `<hr>` in JSX; require the
//! shadcn `<Separator />` component so theming and a11y stay uniform.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "shadcn-no-hr-use-separator",
    description: "Raw `<hr>` bypasses shadcn theming — use the `<Separator />` component.",
    remediation: "Replace `<hr />` with `<Separator />` (or `<Separator orientation=\"vertical\" />`).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["shadcn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
