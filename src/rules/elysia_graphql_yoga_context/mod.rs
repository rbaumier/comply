//! elysia-graphql-yoga-context

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-graphql-yoga-context",
    description: "`yoga({ context })` without a `useContext` placeholder will not propagate the context into resolvers.",
    remediation: "Define a `useContext` GraphQL placeholder (or wire the context through a plugin) so resolvers can read the value.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["correctness", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
