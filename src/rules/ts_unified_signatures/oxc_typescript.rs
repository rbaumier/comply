//! ts-unified-signatures OXC backend — flag adjacent function overload signatures
//! in interfaces/type literals that share the same name.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{PropertyKey, TSCallSignatureDeclaration, TSLiteral, TSSignature, TSType};
use oxc_span::GetSpan;
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

/// The source text of a call signature's declared return type, or `None` when it
/// has no annotation (an inferred return is treated as a distinct return type).
fn return_type_text<'a>(call: &TSCallSignatureDeclaration<'a>, source: &'a str) -> Option<&'a str> {
    let annotation = call.return_type.as_ref()?;
    Some(&source[annotation.type_annotation.span().start as usize..annotation.type_annotation.span().end as usize])
}

/// Whether a group of call signatures could be merged into one with a union or
/// optional trailing parameter.
///
/// * Parameter counts may differ by at most one — a larger gap would need more
///   than one optional trailing parameter, which the overloads do not express.
/// * When the counts *do* differ, the unified form has to add an optional
///   trailing parameter, so the declared return types must be identical;
///   otherwise the merge would erase the per-overload return-type distinction
///   (the curried zero-arg vs one-arg overload idiom). When the counts are equal
///   the merge unions a single parameter's type and the return types need not
///   match.
fn call_signatures_are_unifiable<'a>(
    calls: &[&'a TSCallSignatureDeclaration<'a>],
    source: &'a str,
) -> bool {
    let mut counts = calls.iter().map(|c| c.params.items.len());
    let Some(first) = counts.next() else {
        return true;
    };
    let (mut min, mut max) = (first, first);
    for count in counts {
        min = min.min(count);
        max = max.max(count);
    }
    if max - min > 1 {
        return false;
    }
    if min == max {
        return true;
    }

    let first_return = return_type_text(calls[0], source);
    calls[1..].iter().all(|c| return_type_text(c, source) == first_return)
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

    if call_signatures_are_path_discriminated(&call_sigs)
        || !call_signatures_are_unifiable(&call_sigs, ctx.source)
    {
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

    // Regression #1977: zustand `Create` — a one-argument signature returning the
    // bound store directly, alongside a zero-argument curried overload returning a
    // factory. Different arity *and* different return types: no single signature
    // (union or optional parameter) expresses both without collapsing the returns.
    #[test]
    fn allows_curried_zero_arg_vs_one_arg_overload() {
        assert!(
            run_on(
                "type Create = {\n  \
                 <T, Mos extends [string, unknown][] = []>(initializer: StateCreator<T, [], Mos>): UseBoundStore<Mutate<StoreApi<T>, Mos>>;\n  \
                 <T>(): <Mos extends [string, unknown][] = []>(initializer: StateCreator<T, [], Mos>) => UseBoundStore<Mutate<StoreApi<T>, Mos>>;\n}"
            )
            .is_empty()
        );
    }

    // Guard: call signatures differing by exactly one trailing optional parameter
    // and sharing the same return type are genuinely unifiable, so still fire.
    #[test]
    fn flags_trailing_optional_parameter_overload() {
        let diags = run_on(
            "interface Foo {\n  \
             (a: number): void;\n  \
             (a: number, b?: string): void;\n}",
        );
        assert_eq!(diags.len(), 1);
    }

    // Guard: param counts differing by more than one cannot be merged with a
    // single optional trailing parameter, so they are not a smell.
    #[test]
    fn allows_call_signatures_differing_by_more_than_one_param() {
        assert!(
            run_on(
                "interface Foo {\n  \
                 (a: number): void;\n  \
                 (a: number, b: string, c: string): void;\n}"
            )
            .is_empty()
        );
    }
}
