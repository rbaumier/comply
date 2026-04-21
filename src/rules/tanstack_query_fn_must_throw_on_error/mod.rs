mod text;
use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "tanstack-query-fn-must-throw-on-error",
    description: "`queryFn` must throw on HTTP errors so TanStack Query can retry and surface them.",
    remediation: "Check `res.ok` and throw: `if (!res.ok) throw new Error(...)`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tanstack"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Text(Box::new(text::Check))),
            (Language::Tsx, Backend::Text(Box::new(text::Check))),
        ],
    }
}
