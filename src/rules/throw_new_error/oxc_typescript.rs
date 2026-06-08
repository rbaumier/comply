//! throw-new-error OXC backend — flag `Error(...)` calls without `new`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

/// Matches PascalCase names ending in "Error": Error, TypeError, MyCustomError, etc.
fn is_error_like(name: &str) -> bool {
    if !name.ends_with("Error") || name.is_empty() {
        return false;
    }
    name.starts_with(|c: char| c.is_ascii_uppercase())
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        let callee_name = match &call.callee {
            // Direct call: `Error('x')`
            Expression::Identifier(id) => {
                let name = id.name.as_str();
                if !is_error_like(name) {
                    return;
                }
                // TaggedError("tag") is an Effect class factory, not an error constructor.
                if name == "TaggedError" {
                    return;
                }
                name
            }
            // Member access: `module.CustomError('x')`
            Expression::StaticMemberExpression(member) => {
                let name = member.property.name.as_str();
                if !is_error_like(name) {
                    return;
                }
                // Exclude Data.TaggedError (Effect library)
                if let Expression::Identifier(obj) = &member.object
                    && obj.name.as_str() == "Data" && name == "TaggedError" {
                        return;
                    }
                name
            }
            _ => return,
        };

        let _ = callee_name;

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Use `new` when creating an error.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_error_without_new() {
        let d = run_on("throw Error('oops');");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "throw-new-error");
    }


    #[test]
    fn flags_typeerror_without_new() {
        let d = run_on("throw TypeError('bad type');");
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn flags_custom_error_without_new() {
        let d = run_on("throw MyCustomError('fail');");
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn allows_new_error() {
        assert!(run_on("throw new Error('oops');").is_empty());
    }


    #[test]
    fn allows_new_typeerror() {
        assert!(run_on("throw new TypeError('bad');").is_empty());
    }


    #[test]
    fn allows_non_error_call() {
        assert!(run_on("console.log('hello');").is_empty());
    }


    #[test]
    fn allows_non_error_function() {
        assert!(run_on("foo();").is_empty());
    }


    #[test]
    fn flags_member_error_without_new() {
        let d = run_on("throw lib.CustomError('x');");
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn allows_data_tagged_error() {
        // Effect library exception — Data.TaggedError is not an error constructor.
        assert!(run_on("Data.TaggedError('x');").is_empty());
    }


    #[test]
    fn allows_lowercase_function() {
        // `error()` is not PascalCase — not an error constructor.
        assert!(run_on("error('x');").is_empty());
    }
}
