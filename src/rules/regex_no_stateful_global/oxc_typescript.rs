//! regex-no-stateful-global oxc backend.
//!
//! Visits `RegExpLiteral` nodes. A regex carrying the `g` flag is flagged
//! when it is bound to a `const`/`let`/`var` whose binding is later used
//! as the receiver of `.test()` or `.exec()`. Bindings whose `lastIndex` is
//! manually managed (e.g. `re.lastIndex = 0` before each call) are not
//! flagged: the author has acknowledged and mitigated the statefulness.
//! Bindings whose `.exec(...)` call sits in the test/condition of an enclosing
//! loop are also not flagged: there the `lastIndex` advance is the intended
//! match-iteration driver, not a footgun.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BindingPattern, Expression, RegExpFlags};
use oxc_span::GetSpan;
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

        // If the binding's `lastIndex` is manually managed (e.g. reset to 0
        // before each call), the statefulness is controlled — not a footgun.
        if manages_last_index(ctx.source, var_name) {
            return;
        }

        // If a `<var_name>.exec(...)` call drives an enclosing loop (it sits in
        // the loop's test/condition), the `lastIndex` advance is the intended
        // match-iteration mechanism, not a footgun.
        if exec_drives_loop(var_name, semantic) {
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
    crate::oxc_helpers::source_contains(source, &test_pattern) || crate::oxc_helpers::source_contains(source, &exec_pattern)
}

/// True if the source assigns `<var_name>.lastIndex`, i.e. the author manually
/// manages the cursor (typically resetting it before each call). Such code has
/// already mitigated the `lastIndex` statefulness, so it must not be flagged.
fn manages_last_index(source: &str, var_name: &str) -> bool {
    let pattern = format!("{var_name}.lastIndex");
    crate::oxc_helpers::source_contains(source, &pattern)
}

/// True when a `<var_name>.exec(...)` call sits inside the **test/condition** of
/// an enclosing loop — the canonical `while ((m = re.exec(s)) !== null)` shape.
/// Here the `lastIndex` advance is what drives and terminates the loop, so it is
/// intended, not a footgun. An `exec` call in the loop **body** is reuse (the
/// cursor carries across iterations of an unrelated condition) and still flags,
/// hence the containment check keys on the loop's `test` span, not loop ancestry.
fn exec_drives_loop<'a>(var_name: &str, semantic: &'a oxc_semantic::Semantic<'a>) -> bool {
    for node in semantic.nodes().iter() {
        let AstKind::CallExpression(call) = node.kind() else {
            continue;
        };
        let Expression::StaticMemberExpression(member) = &call.callee else {
            continue;
        };
        if member.property.name != "exec" {
            continue;
        }
        let Expression::Identifier(obj) = &member.object else {
            continue;
        };
        if obj.name != var_name {
            continue;
        }

        let call_span = call.span();
        for ancestor in semantic.nodes().ancestors(node.id()) {
            let test_span = match ancestor.kind() {
                AstKind::WhileStatement(stmt) => Some(stmt.test.span()),
                AstKind::DoWhileStatement(stmt) => Some(stmt.test.span()),
                AstKind::ForStatement(stmt) => stmt.test.as_ref().map(GetSpan::span),
                _ => None,
            };
            let Some(test_span) = test_span else {
                continue;
            };
            if test_span.start <= call_span.start && call_span.end <= test_span.end {
                return true;
            }
        }
    }
    false
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

    #[test]
    fn allows_global_regex_with_manual_last_index_reset() {
        let src = "const BYTE = /^(?:[A-Za-z0-9+/]{4})*$/gm;\n\
                   function byte(str) {\n\
                   \tBYTE.lastIndex = 0;\n\
                   \treturn BYTE.test(str);\n\
                   }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_global_regex_reused_without_reset() {
        let src = "const BYTE = /^foo$/gm;\n\
                   function byte(str) {\n\
                   \treturn BYTE.test(str);\n\
                   }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_exec_driving_while_loop() {
        let src = "const regex = /[:]([a-zA-Z_]\\w*)/g;\n\
                   let match;\n\
                   while ((match = regex.exec(segment)) !== null) { out.push(match[1]); }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_bare_exec_driving_while_loop() {
        let src = "const re = /x/g;\nwhile (re.exec(s)) {}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_exec_driving_for_loop() {
        let src = "const re = /x/g;\nfor (let m; (m = re.exec(s)) !== null; ) {}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_exec_in_loop_body_not_test() {
        let src = "const re = /x/g;\nwhile (cond) { re.exec(s); }";
        assert_eq!(run_on(src).len(), 1);
    }
}
