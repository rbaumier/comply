//! OxcCheck backend for testing-no-undefined-mock-var — flag bare
//! `vi.fn()` / `jest.fn()` mocks never configured.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::VariableDeclaration]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !is_test_file(ctx.path) {
            return;
        }

        let AstKind::VariableDeclaration(decl) = node.kind() else {
            return;
        };

        for declarator in &decl.declarations {
            let Some(init) = &declarator.init else {
                continue;
            };

            // Must be a call expression: vi.fn() or jest.fn()
            let Expression::CallExpression(call) = init else {
                continue;
            };
            let Expression::StaticMemberExpression(member) = &call.callee else {
                continue;
            };
            if member.property.name.as_str() != "fn" {
                continue;
            }
            let Expression::Identifier(obj) = &member.object else {
                continue;
            };
            let obj_text = obj.name.as_str();
            if obj_text != "vi" && obj_text != "jest" {
                continue;
            }

            // If the caller passed an implementation factory, the mock is configured.
            if !call.arguments.is_empty() {
                continue;
            }

            // If the type argument explicitly declares a void or undefined return type,
            // undefined is the correct return value — no configuration needed.
            if let Some(type_args) = &call.type_arguments {
                let ta_src =
                    &ctx.source[type_args.span.start as usize..type_args.span.end as usize];
                if ta_src.contains("=> void") || ta_src.contains("=> undefined") {
                    continue;
                }
            }

            let oxc_ast::ast::BindingPattern::BindingIdentifier(binding) =
                &declarator.id
            else {
                continue;
            };
            let var_name = binding.name.as_str();
            if !var_name
                .chars()
                .all(|c: char| c.is_alphanumeric() || c == '_')
            {
                continue;
            }

            // Scan full source for configuration methods.
            let configured = ["mockReturnValue", "mockResolvedValue", "mockImplementation"]
                .iter()
                .any(|m| ctx.source_contains(&format!("{var_name}.{m}")));
            if configured {
                continue;
            }

            // If the mock is used as a spy (appears in expect()), the undefined return is fine.
            if ctx.source_contains(&format!("expect({var_name})")) {
                continue;
            }

            let (line, column) =
                byte_offset_to_line_col(ctx.source, call.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "`{var_name}` is a `{obj_text}.fn()` mock with no `.mockReturnValue` / \
                     `.mockResolvedValue` / `.mockImplementation` configuration — it will \
                     always return `undefined`. Configure it or pass an implementation to \
                     `fn(impl)`."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts_with_path(s, &Check, "foo.test.ts")
    }

    fn run_non_test(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts_with_path(s, &Check, "foo.ts")
    }

    #[test]
    fn flags_bare_vi_fn() {
        assert_eq!(run("const m = vi.fn();").len(), 1);
    }

    #[test]
    fn flags_bare_jest_fn() {
        assert_eq!(run("const m = jest.fn();").len(), 1);
    }

    #[test]
    fn allows_configured_mock_return_value() {
        assert!(run("const m = vi.fn(); m.mockReturnValue(1);").is_empty());
    }

    #[test]
    fn allows_configured_mock_resolved_value() {
        assert!(run("const m = jest.fn(); m.mockResolvedValue({ok: true});").is_empty());
    }

    #[test]
    fn allows_configured_mock_implementation() {
        assert!(run("const m = vi.fn(); m.mockImplementation(() => 1);").is_empty());
    }

    #[test]
    fn allows_impl_passed_to_fn() {
        assert!(run("const m = vi.fn(() => 1);").is_empty());
    }

    #[test]
    fn allows_spy_in_expect() {
        assert!(run("const spy = vi.fn(); expect(spy).toHaveBeenCalled();").is_empty());
    }

    #[test]
    fn allows_spy_with_called_with() {
        assert!(
            run("const handler = jest.fn(); expect(handler).toHaveBeenCalledWith('a', 'b');")
                .is_empty()
        );
    }

    #[test]
    fn ignores_non_test_file() {
        assert!(run_non_test("const m = vi.fn();").is_empty());
    }

    // Regression for #335: vi.fn<T>() with a void return type must not be flagged.
    #[test]
    fn allows_void_return_type_parameter() {
        assert!(run(
            "const onStateChange = vi.fn<(next: DataTableState) => void>();"
        )
        .is_empty());
    }

    #[test]
    fn allows_undefined_return_type_parameter() {
        assert!(run("const cb = vi.fn<() => undefined>();").is_empty());
    }

    #[test]
    fn flags_non_void_return_type_parameter() {
        assert_eq!(
            run("const fetcher = vi.fn<() => string>();").len(),
            1
        );
    }
}
