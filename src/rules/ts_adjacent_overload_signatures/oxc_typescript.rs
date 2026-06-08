//! ts-adjacent-overload-signatures oxc backend — walk program/class/interface
//! bodies and flag non-adjacent overload signatures that share the same name.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[
            AstType::Program,
            AstType::ClassBody,
            AstType::TSInterfaceDeclaration,
        ]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        use oxc_ast::ast::*;
        use oxc_ast::AstKind;

        match node.kind() {
            AstKind::Program(program) => {
                check_body_items(
                    program.body.iter().filter_map(|stmt| extract_overload_name_from_stmt(stmt, ctx.source)),
                    ctx,
                    diagnostics,
                );
            }
            AstKind::ClassBody(body) => {
                let items = body.body.iter().filter_map(|elem| {
                    match elem {
                        ClassElement::MethodDefinition(m) => {
                            let name = property_key_name(&m.key, ctx.source)?;
                            let is_static = m.r#static;
                            let span = m.span;
                            if is_static {
                                Some((format!("static {name}"), span))
                            } else {
                                Some((name, span))
                            }
                        }
                        ClassElement::TSIndexSignature(_) => None,
                        ClassElement::PropertyDefinition(_) => None,
                        ClassElement::AccessorProperty(_) => None,
                        ClassElement::StaticBlock(_) => None,
                    }
                });
                check_body_items(items, ctx, diagnostics);
            }
            AstKind::TSInterfaceDeclaration(iface) => {
                let items = iface.body.body.iter().filter_map(|sig| {
                    match sig {
                        TSSignature::TSMethodSignature(m) => {
                            let name = property_key_name(&m.key, ctx.source)?;
                            Some((name, m.span))
                        }
                        TSSignature::TSCallSignatureDeclaration(c) => {
                            Some(("call".to_string(), c.span))
                        }
                        TSSignature::TSConstructSignatureDeclaration(c) => {
                            Some(("new".to_string(), c.span))
                        }
                        _ => None,
                    }
                });
                check_body_items(items, ctx, diagnostics);
            }
            _ => {}
        }
    }
}

fn property_key_name(key: &oxc_ast::ast::PropertyKey, source: &str) -> Option<String> {
    use oxc_span::GetSpan;
    match key {
        oxc_ast::ast::PropertyKey::StaticIdentifier(id) => Some(id.name.to_string()),
        oxc_ast::ast::PropertyKey::StringLiteral(s) => Some(s.value.to_string()),
        oxc_ast::ast::PropertyKey::NumericLiteral(n) => Some(n.value.to_string()),
        _ => {
            let span = key.span();
            Some(source[span.start as usize..span.end as usize].to_string())
        }
    }
}

fn extract_overload_name_from_stmt(
    stmt: &oxc_ast::ast::Statement,
    _source: &str,
) -> Option<(String, oxc_span::Span)> {
    use oxc_ast::ast::*;

    match stmt {
        Statement::FunctionDeclaration(f) => {
            let name = f.id.as_ref()?.name.to_string();
            Some((name, f.span))
        }
        Statement::TSTypeAliasDeclaration(_) => None,
        Statement::ExportNamedDeclaration(exp) => {
            if let Some(ref decl) = exp.declaration {
                match decl {
                    Declaration::FunctionDeclaration(f) => {
                        let name = f.id.as_ref()?.name.to_string();
                        Some((name, f.span))
                    }
                    _ => None,
                }
            } else {
                None
            }
        }
        _ => None,
    }
}

fn check_body_items(
    items: impl Iterator<Item = (String, oxc_span::Span)>,
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut seen: Vec<String> = Vec::new();
    let mut last_name: Option<String> = None;

    for (name, span) in items {
        let is_adjacent = last_name.as_deref() == Some(&name);
        let was_seen = seen.contains(&name);

        if was_seen && !is_adjacent {
            let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!("All `{name}` signatures should be adjacent."),
                severity: Severity::Warning,
                span: None,
            });
        } else if !was_seen {
            seen.push(name.clone());
        }

        last_name = Some(name);
    }
}

#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_non_adjacent_overloads() {
        let diags = run_on(
            r#"
function foo(): void;
function bar(): void;
function foo(x: number): void;
"#,
        );
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("foo"));
    }

    #[test]
    fn allows_adjacent_overloads() {
        let diags = run_on(
            r#"
function foo(): void;
function foo(x: number): void;
function bar(): void;
"#,
        );
        assert!(diags.is_empty());
    }

    #[test]
    fn flags_in_interface() {
        let diags = run_on(
            r#"
interface I {
    foo(): void;
    bar(): void;
    foo(x: number): void;
}
"#,
        );
        assert_eq!(diags.len(), 1);
    }
}
