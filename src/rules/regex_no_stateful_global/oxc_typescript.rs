//! regex-no-stateful-global oxc backend.
//!
//! Visits `RegExpLiteral` nodes. A regex carrying the `g` flag is flagged
//! when it is bound to a `const`/`let`/`var` whose binding is later used
//! as the receiver of `.test()` or `.exec()`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BindingPattern, Expression, RegExpFlags};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::RegExpLiteral]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::RegExpLiteral(regex) = node.kind() else {
            return;
        };

        // Must have the `g` flag.
        if !regex.regex.flags.contains(RegExpFlags::G) {
            return;
        }

        // Walk up to find the enclosing variable declarator.
        let var_name = find_enclosing_binding(node, semantic);
        let Some(var_name) = var_name else {
            return;
        };

        // Check if `var_name.test(...)` or `var_name.exec(...)` appears in the source.
        if !has_stateful_usage(ctx.source, var_name) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, regex.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Regex `{var_name}` has the `g` flag and is used with `.test()`/`.exec()` \u{2014} `lastIndex` is stateful and causes subtle bugs."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

/// Walk ancestors to find the enclosing `VariableDeclarator` and return
/// the binding identifier name.
fn find_enclosing_binding<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> Option<&'a str> {
    for ancestor in semantic.nodes().ancestors(node.id()) {
        if let AstKind::VariableDeclarator(decl) = ancestor.kind() {
            if let BindingPattern::BindingIdentifier(id) = &decl.id {
                return Some(id.name.as_str());
            }
            return None;
        }
    }
    None
}

/// True if the source contains `<var_name>.test(` or `<var_name>.exec(`.
fn has_stateful_usage(source: &str, var_name: &str) -> bool {
    let test_pattern = format!("{var_name}.test(");
    let exec_pattern = format!("{var_name}.exec(");
    source.contains(&test_pattern) || source.contains(&exec_pattern)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_global_regex_with_test() {
        let src = "const re = /foo/g;\nif (re.test(str)) {}";
        let diags = run_on(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("lastIndex"));
    }

    #[test]
    fn flags_global_regex_with_exec() {
        let src = "const re = /bar/gi;\nconst m = re.exec(input);";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_global_regex_without_test_exec() {
        let src = "const re = /foo/g;\nconst result = str.match(re);";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_non_global_regex_with_test() {
        let src = "const re = /foo/i;\nif (re.test(str)) {}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_tailwind_arbitrary_value_in_string() {
        let src = r#"const x = "has-[>svg]:grid-cols-[auto_1fr]";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_url_in_string() {
        let src = r#"const u = "http://example.com/a/b";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_scoped_import_path() {
        let src = r#"import X from "@tanstack/react-query";"#;
        assert!(run_on(src).is_empty());
    }
}
