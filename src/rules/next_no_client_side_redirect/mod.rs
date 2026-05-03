//! next-no-client-side-redirect — `window.location` mutations bypass Next.js
//! routing; use `redirect()` from `next/navigation` or `useRouter().push()`.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "next-no-client-side-redirect",
    description: "Client-side `window.location` redirect — use Next.js `redirect()` or `useRouter().push()`.",
    remediation: "Replace `window.location.href = '/x'` (or `.replace(...)`/`.assign(...)`) with \
                  `redirect('/x')` from `next/navigation`, or `router.push('/x')` from \
                  `useRouter()`. `window.location` triggers a full page reload, dropping the \
                  React tree and Next.js cache.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["nextjs"],
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
