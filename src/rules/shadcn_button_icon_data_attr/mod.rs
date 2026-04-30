//! shadcn-button-icon-data-attr — icons inside `<Button>` must declare
//! their position via `data-icon="inline-start"` / `"inline-end"`,
//! never via `mr-2` / `ml-2` margin utilities (which break in RTL and
//! when the button has no label).

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "shadcn-button-icon-data-attr",
    description: "Icons inside `<Button>` must use `data-icon` instead of `mr-2`/`ml-2` for positioning.",
    remediation: "Replace `className=\"mr-2\"` with `data-icon=\"inline-start\"` (and `ml-2` with `data-icon=\"inline-end\"`).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["shadcn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
