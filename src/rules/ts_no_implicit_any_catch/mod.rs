//! ts-no-implicit-any-catch — flag `catch (e)` without an explicit type
//! annotation. TypeScript 4.0+ supports `catch (e: unknown)` and 4.4+
//! supports `useUnknownInCatchVariables`; an untyped catch binding
//! defaults to `any`, silently disabling type checking inside the handler.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
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
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
