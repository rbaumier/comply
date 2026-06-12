//! ts-unified-signatures OXC backend — flag adjacent function overload signatures
//! in interfaces/type literals that share the same name.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{PropertyKey, TSCallSignatureDeclaration, TSLiteral, TSSignature, TSType};
use rustc_hash::FxHashSet;
use std::collections::HashMap;
use std::sync::Arc;

pub struct Check;

/// The single string-literal value typing a call signature's first parameter,
/// e.g. `"/geocode"` for `(path: "/geocode"): T`. `None` when the signature does
/// not have exactly one parameter, or that parameter is not a string-literal type.
fn first_param_string_literal<'a>(call: &'a TSCallSignatureDeclaration<'a>) -> Option<&'a str> {
    let [param] = call.params.items.as_slice() else {
        return None;
    };
    let TSType::TSLiteralType(lit) = &param.type_annotation.as_ref()?.type_annotation else {
        return None;
    };
    match &lit.literal {
        TSLiteral::StringLiteral(s) => Some(s.value.as_str()),
        _ => None,
    }
}

/// Path-discriminated dispatchers (e.g. the Azure SDK `Routes` interface) map a
/// distinct string-literal path to a distinct return type per overload. Unifying
/// them would erase the per-path return-type inference, so they are not a smell.
/// True when every call signature is typed by a *distinct* string literal.
fn call_signatures_are_path_discriminated<'a>(
    calls: &[&'a TSCallSignatureDeclaration<'a>],
) -> bool {
    let mut literals = FxHashSet::default();
    for call in calls {
        let Some(literal) = first_param_string_literal(call) else {
            return false;
        };
        if !literals.insert(literal) {
            return false;
        }
    }
    true
}

fn collect_signatures<'a>(
    members: &[TSSignature<'a>],
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut seen: HashMap<String, Vec<u32>> = HashMap::new();
    let mut call_sigs: Vec<&TSCallSignatureDeclaration<'a>> = Vec::new();

    for sig in members {
        match sig {
            TSSignature::TSCallSignatureDeclaration(call) => {
                seen.entry("[[call]]".to_string())
                    .or_default()
                    .push(call.span.start);
                call_sigs.push(call);
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

    if call_signatures_are_path_discriminated(&call_sigs) {
        seen.remove("[[call]]");
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
    fn flags_duplicate_call_signatures() {
        let diags = run_on("interface Foo {\n  (x: string): void;\n  (x: number): void;\n}");
        assert_eq!(diags.len(), 1);
    }

    // Regression #1088: Azure SDK `path()` routing — each call signature maps a
    // distinct string-literal path to its own return type. Unifying would erase
    // the per-path return-type inference, so these are not unifiable.
    #[test]
    fn allows_path_discriminated_call_signatures() {
        assert!(
            run_on(
                "export interface Routes {\n  \
                 (path: \"/geocode\"): GetGeocoding;\n  \
                 (path: \"/geocode:batch\"): GetGeocodingBatch;\n  \
                 (path: \"/search/polygon\"): GetPolygon;\n  \
                 (path: \"/reverseGeocode\"): GetReverseGeocoding;\n}"
            )
            .is_empty()
        );
    }

    #[test]
    fn flags_call_signatures_with_duplicate_string_literal() {
        let diags = run_on(
            "interface Foo {\n  \
             (path: \"/a\"): X;\n  \
             (path: \"/a\"): Y;\n}",
        );
        assert_eq!(diags.len(), 1);
    }
}
