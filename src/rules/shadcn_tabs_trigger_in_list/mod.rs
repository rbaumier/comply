//! shadcn-tabs-trigger-in-list — `<TabsTrigger>` must be nested inside
//! a `<TabsList>`, not rendered directly under `<Tabs>`.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "shadcn-tabs-trigger-in-list",
    description: "`<TabsTrigger>` must live inside `<TabsList>` for correct keyboard navigation and ARIA roles.",
    remediation: "Wrap the triggers in a `<TabsList>` sibling of the `<TabsContent>` panels.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["shadcn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
