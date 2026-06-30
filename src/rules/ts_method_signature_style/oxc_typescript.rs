//! ts-method-signature-style oxc backend — flag shorthand method signatures
//! in interfaces and type literals.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{PropertyKey, TSSignature};
use std::collections::HashMap;
use std::sync::Arc;

pub struct Check;

/// A stable string key for grouping overload signatures, or `None` for
/// computed/unknown keys that can't be reliably compared across members.
fn stable_key<'a>(key: &'a PropertyKey<'a>) -> Option<&'a str> {
    match key {
        PropertyKey::StaticIdentifier(id) => Some(id.name.as_str()),
        PropertyKey::StringLiteral(s) => Some(s.value.as_str()),
        _ => None,
    }
}

fn key_name<'a>(key: &'a PropertyKey<'a>) -> &'a str {
    stable_key(key).unwrap_or("method")
}

fn report_method_signatures<'a>(
    members: &[TSSignature<'a>],
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    // A method whose key appears on several signatures within this body is an
    // overload; overloads can't be rewritten as a single property signature, so
    // they are left alone. Count keys first, then flag only keys seen once.
    let mut key_counts: HashMap<&str, usize> = HashMap::new();
    for sig in members {
        if let TSSignature::TSMethodSignature(method) = sig {
            if let Some(key) = stable_key(&method.key) {
                *key_counts.entry(key).or_insert(0) += 1;
            }
        }
    }

    for sig in members {
        let TSSignature::TSMethodSignature(method) = sig else { continue };
        if let Some(key) = stable_key(&method.key) {
            if key_counts.get(key).copied().unwrap_or(0) > 1 {
                continue;
            }
        }
        let name = key_name(&method.key);
        let (line, column) = byte_offset_to_line_col(ctx.source, method.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Shorthand method signature `{name}(...)` is less safe — \
                 use a property signature: `{name}: (...) => ReturnType`."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TSInterfaceDeclaration, AstType::TSTypeAliasDeclaration]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            AstKind::TSInterfaceDeclaration(decl) => {
                report_method_signatures(&decl.body.body, ctx, diagnostics);
            }
            AstKind::TSTypeAliasDeclaration(decl) => {
                if let oxc_ast::ast::TSType::TSTypeLiteral(lit) = &decl.type_annotation {
                    report_method_signatures(&lit.members, ctx, diagnostics);
                }
            }
            _ => {}
        }
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

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    #[test]
    fn flags_single_method_signature() {
        // Positive control — a single, non-overloaded method signature is the
        // rule's core target and must still be flagged.
        let src = r#"
            interface A {
                foo(x: string): void;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_distinct_named_method_signatures() {
        // Positive control — two differently-named single method signatures are
        // each non-overloaded and must both be flagged.
        let src = r#"
            interface A {
                foo(x: string): void;
                bar(y: number): void;
            }
        "#;
        assert_eq!(run(src).len(), 2);
    }

    #[test]
    fn no_fp_on_overloaded_method_signatures() {
        // Regression rbaumier/comply#6847 (trpc/trpc, procedureBuilder.ts) — two
        // `subscription` overloads (same key, different generic constraints and
        // return types) in one interface cannot be expressed as a single
        // property signature, so neither is flagged.
        let src = r#"
            interface ProcedureBuilder<TContext, TMeta> {
                subscription<$Output extends AsyncIterable<any, void, any>>(
                    resolver: ProcedureResolver<TContext, $Output>,
                ): SubscriptionProcedure<$Output>;
                subscription<$Output extends Observable<any, any>>(
                    resolver: ProcedureResolver<TContext, $Output>,
                ): LegacyObservableSubscriptionProcedure<$Output>;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn overloads_do_not_suppress_sibling_single_method() {
        // The overload skip is scoped to the overloaded key only: a sibling,
        // non-overloaded method in the same interface is still flagged.
        let src = r#"
            interface A {
                over(x: string): void;
                over(x: number): void;
                single(y: boolean): void;
            }
        "#;
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("`single(...)`"));
    }

    #[test]
    fn methods_in_separate_interfaces_are_not_overloads() {
        // Same key in two different interface bodies are independent single
        // methods, not overloads — each is flagged.
        let src = r#"
            interface A {
                run(x: string): void;
            }
            interface B {
                run(x: number): void;
            }
        "#;
        assert_eq!(run(src).len(), 2);
    }

    #[test]
    fn no_fp_on_overloaded_method_signatures_in_type_literal() {
        // The overload skip also applies to method signatures inside a type
        // literal (the `type X = { ... }` entry point), not just interfaces.
        let src = r#"
            type ProcedureBuilder = {
                subscription<$O extends AsyncIterable<any>>(r: Resolver<$O>): Sub<$O>;
                subscription<$O extends Observable<any>>(r: Resolver<$O>): LegacySub<$O>;
            };
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_single_method_signature_in_type_literal() {
        // Positive control for the type-literal entry point — a lone method
        // signature in a type literal is still flagged.
        let src = r#"
            type A = {
                foo(x: string): void;
            };
        "#;
        assert_eq!(run(src).len(), 1);
    }
}
