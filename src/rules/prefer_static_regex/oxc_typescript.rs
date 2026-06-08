//! prefer-static-regex — OXC backend.
//! Flag regex literals inside functions (recompiled on each call).

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test.", ".e2e."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    if TEST_MARKERS.iter().any(|m| s.contains(m)) {
        return true;
    }
    path.components()
        .any(|c| c.as_os_str() == "tests" || c.as_os_str() == "e2e")
}

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
        let AstKind::RegExpLiteral(regex) = node.kind() else { return };

        if is_test_file(ctx.path) {
            return;
        }

        // Walk ancestors to check if inside a function.
        let mut inside_function = false;
        for ancestor in semantic.nodes().ancestors(node.id()) {
            match ancestor.kind() {
                AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => {
                    inside_function = true;
                    break;
                }
                _ => {}
            }
        }

        if !inside_function {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, regex.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Regex literal inside function is recompiled on each call. Hoist to module scope.".into(),
            severity: Severity::Warning,
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

    fn run(code: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, code, "t.ts")
    }

    fn run_at(code: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, code, path)
    }

    #[test]
    fn flags_regex_in_function() {
        assert_eq!(run("function f() { return /abc/.test(s); }").len(), 1);
        assert_eq!(run("const f = () => /abc/.test(s)").len(), 1);
    }

    #[test]
    fn flags_regex_in_method() {
        let code = "class C { m() { return /abc/.test(s); } }";
        assert_eq!(run(code).len(), 1);
    }

    #[test]
    fn allows_module_level_regex() {
        assert!(run("const RE = /abc/;").is_empty());
        assert!(run("const RE = /abc/g;").is_empty());
    }

    #[test]
    fn allows_class_property_regex() {
        assert!(run("class C { re = /abc/; }").is_empty());
    }

    #[test]
    fn allows_regex_in_test_file() {
        let code = "function f() { return /abc/.test(s); }";
        assert!(run_at(code, "src/foo.test.ts").is_empty());
        assert!(run_at(code, "src/foo.spec.ts").is_empty());
        assert!(run_at(code, "src/__tests__/foo.ts").is_empty());
        assert!(run_at(code, "e2e/foo.ts").is_empty());
        assert!(run_at(code, "tests/foo.ts").is_empty());
    }
}
