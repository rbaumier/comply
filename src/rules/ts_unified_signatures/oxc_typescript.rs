//! ts-unified-signatures OXC backend — flag adjacent function overload signatures
//! in interfaces/type literals that share the same name.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{PropertyKey, TSSignature};
use std::collections::HashMap;
use std::sync::Arc;

pub struct Check;

fn collect_signatures<'a>(
    members: &[TSSignature<'a>],
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut seen: HashMap<String, Vec<u32>> = HashMap::new();

    for sig in members {
        match sig {
            TSSignature::TSCallSignatureDeclaration(call) => {
                seen.entry("[[call]]".to_string())
                    .or_default()
                    .push(call.span.start);
            }
            TSSignature::TSMethodSignature(method) => {
                let name = match &method.key {
                    PropertyKey::StaticIdentifier(id) => id.name.to_string(),
                    PropertyKey::StringLiteral(s) => s.value.to_string(),
                    _ => continue,
                };
                seen.entry(name).or_default().push(method.span.start);
            }
            _ => {}
        }
    }

    for (name, offsets) in &seen {
        if offsets.len() < 2 {
            continue;
        }
        for &offset in &offsets[1..] {
            let display_name = if name == "[[call]]" {
                "Call signatures".to_string()
            } else {
                format!("`{name}` signatures")
            };
            let (line, _column) = byte_offset_to_line_col(ctx.source, offset as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column: 1,
                rule_id: super::META.id.into(),
                message: format!(
                    "{display_name} can be unified into a single signature \
                     with a union or optional parameter."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
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
                collect_signatures(&decl.body.body, ctx, diagnostics);
            }
            AstKind::TSTypeAliasDeclaration(decl) => {
                if let oxc_ast::ast::TSType::TSTypeLiteral(lit) = &decl.type_annotation {
                    collect_signatures(&lit.members, ctx, diagnostics);
                }
            }
            _ => {}
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
    fn flags_duplicate_method_signatures() {
        let diags = run_on("interface Foo {\n  bar(x: string): void;\n  bar(x: number): void;\n}");
        assert_eq!(diags.len(), 1);
    }


    #[test]
    fn allows_different_method_names() {
        assert!(
            run_on("interface Foo {\n  bar(x: string): void;\n  baz(x: number): void;\n}")
                .is_empty()
        );
    }


    #[test]
    fn flags_duplicate_call_signatures() {
        let diags = run_on("interface Foo {\n  (x: string): void;\n  (x: number): void;\n}");
        assert_eq!(diags.len(), 1);
    }
}
