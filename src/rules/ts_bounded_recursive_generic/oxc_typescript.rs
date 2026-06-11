//! ts-bounded-recursive-generic OXC backend — flag recursive conditional/mapped
//! types that lack a depth accumulator parameter.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TSTypeAliasDeclaration]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::TSTypeAliasDeclaration(alias) = node.kind() else {
            return;
        };

        let name = alias.id.name.as_str();
        if name.is_empty() {
            return;
        }

        // Get the full source text of the type annotation to check for
        // conditional/mapped types and self-references.
        let ann_text =
            &ctx.source[alias.type_annotation.span().start as usize..alias.type_annotation.span().end as usize];

        // Must be a conditional or mapped type (heuristic: check text).
        let is_conditional_or_mapped =
            ann_text.contains(" extends ") || ann_text.contains("[") && ann_text.contains(" in ");
        if !is_conditional_or_mapped {
            return;
        }

        // Must reference itself.
        if !references_name(ann_text, name) {
            return;
        }

        // Must lack both a depth parameter and an accumulator parameter.
        if let Some(type_params) = &alias.type_parameters
            && (has_depth_parameter(type_params, ctx.source)
                || has_accumulator_parameter(type_params))
        {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, alias.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Recursive type `{name}` has no depth parameter; add one to bound recursion."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

/// Check if the type annotation text references the given name as a standalone
/// identifier (followed by `<` or non-alphanumeric).
fn references_name(text: &str, name: &str) -> bool {
    let mut start = 0;
    while let Some(pos) = text[start..].find(name) {
        let abs = start + pos;
        let after = abs + name.len();
        // Check the character before is not alphanumeric/_
        let ok_before = abs == 0
            || !text.as_bytes()[abs - 1].is_ascii_alphanumeric()
                && text.as_bytes()[abs - 1] != b'_';
        // Check the character after is not alphanumeric/_
        let ok_after = after >= text.len()
            || !text.as_bytes()[after].is_ascii_alphanumeric()
                && text.as_bytes()[after] != b'_';
        if ok_before && ok_after {
            return true;
        }
        start = abs + 1;
    }
    false
}

fn has_depth_parameter(
    type_params: &oxc_ast::ast::TSTypeParameterDeclaration,
    source: &str,
) -> bool {
    for tp in &type_params.params {
        let text = &source[tp.span.start as usize..tp.span.end as usize];
        if text.contains("Depth") || text.contains("Count") {
            return true;
        }
        if text.contains("extends number") || text.contains("extends 0") {
            return true;
        }
    }
    false
}

/// Check if any type parameter defaults to an empty-collection initializer
/// (`[]`, `never`, `{}`, or `''`). Such a default marks the accumulator
/// pattern, where recursion is bounded by structural shrinkage of the input
/// each step rather than by a numeric depth counter.
fn has_accumulator_parameter(type_params: &oxc_ast::ast::TSTypeParameterDeclaration) -> bool {
    use oxc_ast::ast::{TSLiteral, TSType};
    type_params.params.iter().any(|tp| match &tp.default {
        Some(TSType::TSTupleType(tuple)) => tuple.element_types.is_empty(),
        Some(TSType::TSNeverKeyword(_)) => true,
        Some(TSType::TSTypeLiteral(literal)) => literal.members.is_empty(),
        Some(TSType::TSLiteralType(literal)) => {
            matches!(&literal.literal, TSLiteral::StringLiteral(s) if s.value.is_empty())
        }
        _ => false,
    })
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
    fn exempts_union_to_tuple_accumulator() {
        let src = r#"
export type UnionToTuple<
  union,
  output extends any[] = []
> = UnionToIntersection<union extends any ? (t: union) => union : never> extends (_: any) => infer elem
  ? UnionToTuple<Exclude<union, elem>, [elem, ...output]>
  : output;
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn exempts_flatten_accumulator() {
        let src = r#"
export type Flatten<
  xs extends readonly any[],
  output extends any[] = []
> = xs extends readonly [infer head, ...infer tail]
  ? Flatten<tail, [...output, ...Extract<head, readonly any[]>]>
  : output;
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn exempts_minimal_tuple_accumulator() {
        let src = "type Acc<T, Out extends any[] = []> = T extends [infer H, ...infer R] ? Acc<R, [...Out, H]> : Out;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn exempts_never_accumulator() {
        let src = "type Values<T, Out = never> = T extends [infer H, ...infer R] ? Values<R, Out | H> : Out;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn exempts_depth_parameter() {
        let src = "type Walk<T, Depth extends number = 0> = T extends object ? Walk<T[keyof T], Depth> : T;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_unbounded_single_param_recursion() {
        let src = "type InfiniteLoop<T> = T extends any ? InfiniteLoop<T> : never;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_non_accumulator_default() {
        let src = "type Loop<T, Fallback = string> = T extends any ? Loop<T, Fallback> : never;";
        assert_eq!(run_on(src).len(), 1);
    }
}
