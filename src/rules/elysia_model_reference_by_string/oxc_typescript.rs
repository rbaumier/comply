//! elysia-model-reference-by-string OXC backend — flag imported schema
//! identifiers used directly as route schema values instead of string refs.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, PropertyKey};
use std::collections::HashSet;
use std::sync::Arc;

pub struct Check;

const SCHEMA_KEYS: &[&str] = &["body", "response", "query", "params", "headers"];

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if !ctx.project.has_framework("elysia") {
            return Vec::new();
        }

        // Collect imported identifiers ending in Schema/Model from relative paths.
        let mut schema_names: HashSet<&str> = HashSet::new();
        for snode in semantic.nodes().iter() {
            let AstKind::ImportDeclaration(import) = snode.kind() else {
                continue;
            };
            let src = import.source.value.as_str();
            if !(src.starts_with("./") || src.starts_with("../")) {
                continue;
            }
            let Some(specifiers) = &import.specifiers else {
                continue;
            };
            for spec in specifiers {
                let name = match spec {
                    oxc_ast::ast::ImportDeclarationSpecifier::ImportSpecifier(s) => {
                        s.local.name.as_str()
                    }
                    oxc_ast::ast::ImportDeclarationSpecifier::ImportDefaultSpecifier(s) => {
                        s.local.name.as_str()
                    }
                    oxc_ast::ast::ImportDeclarationSpecifier::ImportNamespaceSpecifier(s) => {
                        s.local.name.as_str()
                    }
                };
                if name.ends_with("Schema") || name.ends_with("Model") {
                    schema_names.insert(name);
                }
            }
        }

        if schema_names.is_empty() {
            return Vec::new();
        }

        // Find `pair` nodes where key is a schema key and value is an identifier
        // matching an imported schema.
        let mut out = Vec::new();
        for snode in semantic.nodes().iter() {
            let AstKind::ObjectProperty(prop) = snode.kind() else {
                continue;
            };
            let key_name = match &prop.key {
                PropertyKey::StaticIdentifier(id) => id.name.as_str(),
                _ => continue,
            };
            if !SCHEMA_KEYS.contains(&key_name) {
                continue;
            }
            let Expression::Identifier(val_id) = &prop.value else {
                continue;
            };
            let val_name = val_id.name.as_str();
            if !schema_names.contains(val_name) {
                continue;
            }
            let (line, column) = byte_offset_to_line_col(ctx.source, prop.span.start as usize);
            out.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!("`{key_name}: {val_name}` references an imported schema directly — register it with `.model({{ ... }})` and pass a string key for cross-route reuse."),
                severity: Severity::Warning,
                span: None,
            });
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts_with_framework(source, &Check, "elysia")
    }


    #[test]
    fn flags_imported_schema_used_inline() {
        let src = "import { UserSchema } from './schema';\napp.post('/x', () => 1, { body: UserSchema });";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn allows_string_reference() {
        let src =
            "import { UserSchema } from './schema';\napp.post('/x', () => 1, { body: 'user' });";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn allows_inline_typebox() {
        let src = "import { t } from 'elysia';\napp.post('/x', () => 1, { body: t.Object({}) });";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_non_elysia_files() {
        let src = "import { UserSchema } from './schema';\napp.post('/x', () => 1, { body: UserSchema });";
        assert!(crate::rules::test_helpers::run_oxc_ts(src, &Check).is_empty());
    }
}
