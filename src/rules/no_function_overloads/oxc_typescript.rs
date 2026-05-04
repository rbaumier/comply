use std::collections::HashMap;

use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use oxc_ast::ast::{Declaration, Statement};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for node in semantic.nodes().iter() {
            if let AstKind::Program(program) = node.kind() {
                let mut counts: HashMap<String, Vec<u32>> = HashMap::new();
                for stmt in &program.body {
                    if let Some((name, span_start)) = extract_overload_sig(stmt) {
                        counts.entry(name).or_default().push(span_start);
                    }
                }
                for (name, offsets) in counts {
                    if offsets.len() < 2 {
                        continue;
                    }
                    for offset in offsets {
                        let (line, column) = byte_offset_to_line_col(ctx.source, offset as usize);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: super::META.id.into(),
                            message: format!(
                                "Function '{name}' has overload signatures — overloads \
                                 don't constrain the implementation and break inference. \
                                 Use a union parameter type or a generic signature instead."
                            ),
                            severity: super::META.severity,
                            span: None,
                        });
                    }
                }
            }
        }
        diagnostics
    }
}

/// Extract the function name from a statement if it's an overload signature
/// (a function declaration without a body).
fn extract_overload_sig(stmt: &Statement) -> Option<(String, u32)> {
    match stmt {
        Statement::FunctionDeclaration(f) => {
            // Overload signature = no body
            if f.body.is_some() {
                return None;
            }
            let name = f.id.as_ref()?.name.to_string();
            Some((name, f.span.start))
        }
        Statement::ExportNamedDeclaration(exp) => {
            if let Some(ref decl) = exp.declaration
                && let Declaration::FunctionDeclaration(f) = decl {
                    if f.body.is_some() {
                        return None;
                    }
                    let name = f.id.as_ref()?.name.to_string();
                    return Some((name, f.span.start));
                }
            None
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_overloaded_function() {
        let source = "
function foo(x: number): string;
function foo(x: string): number;
function foo(x: number | string): string | number { return x as any; }
";
        assert_eq!(run_on(source).len(), 2);
    }

    #[test]
    fn allows_single_signature() {
        assert!(run_on("function foo(x: number): string { return String(x); }").is_empty());
    }

    #[test]
    fn allows_distinct_functions() {
        let source = "function foo(): void {} function bar(): void {}";
        assert!(run_on(source).is_empty());
    }
}
