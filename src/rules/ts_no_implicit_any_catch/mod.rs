//! ts-no-implicit-any-catch — flag `catch (e)` without an explicit type
//! annotation. TypeScript 4.0+ supports `catch (e: unknown)` and 4.4+
//! supports `useUnknownInCatchVariables`; an untyped catch binding
//! defaults to `any`, silently disabling type checking inside the handler.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-no-implicit-any-catch",
    description: "catch binding without an explicit type annotation falls back to implicit any.",
    remediation: "Add explicit type annotation: catch (e: unknown)",
    severity: Severity::Warning,
    doc_url: Some(
        "https://www.typescriptlang.org/docs/handbook/release-notes/typescript-4-4.html#use-unknown-catch-variables",
    ),
    categories: &["typescript"],

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
