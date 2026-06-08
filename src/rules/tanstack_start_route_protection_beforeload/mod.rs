//! tanstack-start-route-protection-beforeload — protected routes should
//! gate with `beforeLoad` + `throw redirect()`, not `useEffect` + `navigate`.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tanstack-start-route-protection-beforeload",
    description: "Protect routes with `beforeLoad` + `throw redirect()`, not \
                  `useEffect` + `navigate`.",
    remediation: "Move the auth check to `beforeLoad` and `throw redirect({ to: '/login' })`. \
                  This runs before render and avoids the protected UI flashing.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tanstack-start"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
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
