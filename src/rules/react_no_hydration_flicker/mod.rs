//! react-no-hydration-flicker — `useEffect(() => setState(x), [])` on mount
//! causes a content flash during SSR hydration.

mod oxc_typescript;
#[cfg(test)]
mod react;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-hydration-flicker",
    description: "`useEffect(setState, [])` on mount causes a hydration flash.",
    remediation: "Use `useSyncExternalStore` with `getServerSnapshot` for SSR-safe \
                  external state, or add `suppressHydrationWarning` if the mismatch \
                  is intentional (e.g. timestamps, viewport size).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react"],

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
