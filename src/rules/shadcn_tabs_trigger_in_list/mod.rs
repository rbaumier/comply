//! shadcn-tabs-trigger-in-list — `<TabsTrigger>` must be nested inside
//! a `<TabsList>`, not rendered directly under `<Tabs>`.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "shadcn-tabs-trigger-in-list",
    description: "`<TabsTrigger>` must live inside `<TabsList>` for correct keyboard navigation and ARIA roles.",
    remediation: "Wrap the triggers in a `<TabsList>` sibling of the `<TabsContent>` panels.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["shadcn"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
