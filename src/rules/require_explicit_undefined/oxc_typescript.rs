//! OXC backend for require-explicit-undefined — flag bare `return;` in functions
//! whose return type is not `void` or `never`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::MethodDefinitionKind;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ReturnStatement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ReturnStatement(ret) = node.kind() else { return };

        // Only bare `return;` — if there's an argument, skip.
        if ret.argument.is_some() {
            return;
        }

        let nodes = semantic.nodes();

        // Walk up to the nearest enclosing function-like node.
        let mut cur_id = nodes.parent_id(node.id());
        loop {
            if cur_id == nodes.parent_id(cur_id) {
                return; // hit root
            }
            let n = nodes.get_node(cur_id);

            // Extract return_type from either Function or ArrowFunctionExpression
            let ret_type_opt = match n.kind() {
                AstKind::Function(f) => {
                    // Constructors: no meaningful return type
                    let parent_of_func = nodes.parent_id(cur_id);
                    if parent_of_func != cur_id {
                        let parent = nodes.get_node(parent_of_func);
                        if let AstKind::MethodDefinition(method) = parent.kind()
                            && method.kind == MethodDefinitionKind::Constructor {
                                return;
                            }
                    }
                    Some(f.return_type.as_ref())
                }
                AstKind::ArrowFunctionExpression(arrow) => {
                    Some(arrow.return_type.as_ref())
                }
                // Stop at class boundary
                AstKind::Class(_) => return,
                _ => None,
            };

            if let Some(maybe_ret_type) = ret_type_opt {
                let Some(ret_type) = maybe_ret_type else { return };

                let ret_text = &ctx.source[ret_type.span.start as usize..ret_type.span.end as usize];
                let trimmed = ret_text.trim_start_matches(':').trim();

                if trimmed == "void" || trimmed == "never" {
                    return;
                }
                if trimmed == "Promise<void>" || trimmed == "Promise<never>" {
                    return;
                }

                let (line, column) = byte_offset_to_line_col(ctx.source, ret.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: "require-explicit-undefined".into(),
                    message: "Bare `return;` in a function that returns a value — use `return undefined;` for clarity.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
                return;
            }

            let next = nodes.parent_id(cur_id);
            if next == cur_id {
                return; // hit root
            }
            cur_id = next;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_bare_return_in_optional_return() {
        let src = "function getUser(): User | undefined { return; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_bare_return_in_undefined_only() {
        let src = "function nothing(): undefined { return; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_bare_return_in_void() {
        let src = "function sideEffect(): void { return; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_bare_return_in_never() {
        let src = "function bail(): never { throw new Error('x'); return; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_return_with_value() {
        let src = "function x(): number { return 1; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_return_in_constructor() {
        let src = "class C { constructor() { return; } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_bare_return_without_annotation() {
        let src = "function x() { return; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_promise_void() {
        let src = "async function x(): Promise<void> { return; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_in_arrow_function_with_block() {
        let src = "const f = (): string | undefined => { if (x) return; return 'x'; };";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_in_method_with_return_type() {
        let src = "class C { find(): Item | undefined { return; } }";
        assert_eq!(run_on(src).len(), 1);
    }
}
