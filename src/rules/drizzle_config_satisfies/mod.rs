//! drizzle-config-satisfies

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "drizzle-config-satisfies",
    description: "`drizzle.config.ts` should use `satisfies Config` instead of `: Config` annotations.",
    remediation: "Replace `const config: Config = { ... }` with `export default { ... } satisfies Config` so Drizzle kit narrows the config type without widening.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["drizzle"],
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
