//! next-no-server-action-without-use-server — files named `actions.ts` or
//! `*-actions.ts` (the convention for server-action collections) that
//! export `async` functions must declare `'use server'` at the top.

mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "next-no-server-action-without-use-server",
    description: "Server-action files must declare `'use server'` — without it, the action runs on the client.",
    remediation: "Add `'use server';` at the top of the file (before any imports), \
                  or rename the file if it isn't meant to host server actions.",
    severity: Severity::Error,
    doc_url: Some(
        "https://nextjs.org/docs/app/building-your-application/data-fetching/server-actions-and-mutations",
    ),
    categories: &["nextjs"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (
                Language::TypeScript,
                Backend::Text(Box::new(typescript::Check)),
            ),
            (
                Language::JavaScript,
                Backend::Text(Box::new(typescript::Check)),
            ),
            (Language::Tsx, Backend::Text(Box::new(typescript::Check))),
        ],
    }
}
