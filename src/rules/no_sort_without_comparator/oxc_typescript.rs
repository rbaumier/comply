use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["sort"])
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
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        if member.property.name.as_str() != "sort" {
            return;
        }
        if !call.arguments.is_empty() {
            return;
        }
        // `Object.keys(...)` / `Object.getOwnPropertyNames(...)` are spec-guaranteed
        // to return `string[]`, on which a bare `.sort()` sorts lexicographically —
        // the correct, idiomatic ordering. The numeric-coercion footgun this rule
        // targets cannot occur on a statically-`string[]` receiver.
        if let Expression::CallExpression(recv) = &member.object
            && let Expression::StaticMemberExpression(m) = &recv.callee
            && let Expression::Identifier(obj) = &m.object
            && obj.name.as_str() == "Object"
            && matches!(m.property.name.as_str(), "keys" | "getOwnPropertyNames")
        {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`.sort()` without comparator sorts lexicographically — pass an explicit compare function.".into(),
            severity: super::META.severity,
            span: None,
        });
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
    fn flags_empty_sort() {
        assert_eq!(run_on("const sorted = arr.sort();").len(), 1);
    }

    #[test]
    fn flags_sort_with_whitespace() {
        assert_eq!(run_on("const sorted = arr.sort(  );").len(), 1);
    }

    #[test]
    fn allows_sort_with_comparator() {
        assert!(run_on("const sorted = arr.sort((a, b) => a - b);").is_empty());
    }

    #[test]
    fn allows_object_keys_sort() {
        assert!(run_on("Object.keys(x).sort();").is_empty());
    }

    #[test]
    fn allows_object_get_own_property_names_sort() {
        assert!(run_on("Object.getOwnPropertyNames(x).sort();").is_empty());
    }

    #[test]
    fn allows_object_keys_sort_chained() {
        assert!(
            run_on("Object.keys(allMigrations).sort().map((name) => name);").is_empty()
        );
    }

    #[test]
    fn flags_array_literal_sort() {
        assert_eq!(run_on("const sorted = [10, 2, 1].sort();").len(), 1);
    }

    #[test]
    fn flags_object_values_sort() {
        // `Object.values(x)` is not spec-guaranteed `string[]` (values may be
        // numbers) — the footgun applies, so it must still flag.
        assert_eq!(run_on("Object.values(x).sort();").len(), 1);
    }
}
