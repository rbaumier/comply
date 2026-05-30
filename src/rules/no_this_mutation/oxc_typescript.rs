use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, MethodDefinitionKind};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::AssignmentExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::AssignmentExpression(assign) = node.kind() else { return };

        // Check if the left side is `this.something`
        let oxc_ast::ast::AssignmentTarget::StaticMemberExpression(member) = &assign.left else {
            return;
        };
        if !matches!(&member.object, Expression::ThisExpression(_)) {
            return;
        }

        // Walk ancestors to determine if we're inside a constructor.
        // In OXC's AST, a constructor is: MethodDefinition(Constructor) → Function → body.
        // We must peek ahead when we hit Function to check if its parent is a constructor.
        let mut first = true;
        let mut ancestors = semantic.nodes().ancestors(node.id()).peekable();
        while let Some(ancestor) = ancestors.next() {
            if first {
                first = false;
                continue;
            }
            match ancestor.kind() {
                AstKind::MethodDefinition(method) => {
                    if method.kind == MethodDefinitionKind::Constructor {
                        return; // Inside constructor, allowed
                    }
                    break; // Inside a method but not constructor
                }
                AstKind::Function(_) => {
                    // The constructor body is wrapped in a Function node in OXC's AST.
                    // If the next ancestor is a constructor MethodDefinition, we're inside it.
                    if let Some(next) = ancestors.peek() {
                        if let AstKind::MethodDefinition(method) = next.kind() {
                            if method.kind == MethodDefinitionKind::Constructor {
                                return;
                            }
                        }
                    }
                    break;
                }
                AstKind::ArrowFunctionExpression(_) => {
                    break; // Inside an arrow function, not a constructor
                }
                AstKind::PropertyDefinition(_) => {
                    // Direct assignment in class body (field initializer) is OK
                    return;
                }
                _ => {}
            }
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, assign.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Mutation of `this` outside constructor \u{2014} initialize properties in constructor.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_oxc_ts;

    fn run(code: &str) -> Vec<Diagnostic> {
        run_oxc_ts(code, &Check)
    }

    #[test]
    fn flags_this_mutation_in_method() {
        let code = r#"
            class Foo {
                update() { this.value = 1; }
            }
        "#;
        assert_eq!(run(code).len(), 1);
    }

    #[test]
    fn allows_constructor_assignment() {
        let code = r#"
            class Foo {
                constructor() { this.value = 1; }
            }
        "#;
        assert!(run(code).is_empty());
    }

    #[test]
    fn allows_constructor_with_super_call() {
        // Regression for issue #580: assignments in constructors that call super() were
        // incorrectly flagged as "outside constructor"
        let code = r#"
            class ProblemError extends Error {
                constructor(problem) {
                    super(JSON.stringify(problem));
                    this.name = "ProblemError";
                    this.problem = problem;
                }
            }
        "#;
        assert!(run(code).is_empty());
    }

    #[test]
    fn flags_setter() {
        let code = r#"
            class Foo {
                set value(v) { this._value = v; }
            }
        "#;
        assert_eq!(run(code).len(), 1);
    }

    #[test]
    fn flags_regular_method() {
        let code = r#"
            class Foo {
                mutate() {
                    this.x = 1;
                    this.y = 2;
                }
            }
        "#;
        assert_eq!(run(code).len(), 2);
    }
}
