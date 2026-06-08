//! ts-no-restricted-types OXC backend.
//!
//! Flags banned types (`Function`, `Object`) in type annotation positions
//! by scanning all TSTypeReference nodes in the semantic tree.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use oxc_ast::AstKind;
use std::sync::Arc;

/// Banned type names and replacement messages.
const BANNED_TYPES: &[(&str, &str)] = &[
    (
        "Function",
        "Use a specific function type like `() => void` instead of `Function`.",
    ),
    (
        "Object",
        "Use `object` or `Record<string, unknown>` instead of `Object`.",
    ),
];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        for node in semantic.nodes().iter() {
            match node.kind() {
                // TSTypeReference with a single identifier name matching banned types.
                AstKind::TSTypeReference(type_ref) => {
                    let name = type_ref.type_name.to_string();
                    if let Some(&(_, msg)) = BANNED_TYPES.iter().find(|&&(t, _)| t == name.as_str())
                    {
                        let (line, column) =
                            byte_offset_to_line_col(ctx.source, type_ref.span.start as usize);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: "ts-no-restricted-types".into(),
                            message: msg.to_string(),
                            severity: Severity::Warning,
                            span: None,
                        });
                    }
                }
                _ => {}
            }
        }

        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_function_type() {
        let d = run_on("const f: Function = () => {};");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Function"));
    }


    #[test]
    fn flags_object_type() {
        let d = run_on("const o: Object = {};");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Object"));
    }


    #[test]
    fn allows_specific_function_type() {
        assert!(run_on("const f: () => void = () => {};").is_empty());
    }


    #[test]
    fn allows_record_type() {
        assert!(run_on("const o: Record<string, unknown> = {};").is_empty());
    }
}
