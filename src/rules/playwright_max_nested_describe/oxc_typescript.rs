use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test.", ".e2e."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

pub struct Check;

fn is_describe_call(expr: &Expression) -> bool {
    match expr {
        Expression::Identifier(id) => id.name.as_str() == "describe",
        Expression::StaticMemberExpression(member) => {
            if let Expression::Identifier(obj) = &member.object {
                obj.name.as_str() == "describe"
                    || member.property.name.as_str() == "describe"
            } else {
                false
            }
        }
        _ => false,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [oxc_ast::AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if !is_test_file(ctx.path) {
            return Vec::new();
        }
        if !crate::rules::playwright::is_playwright_context(ctx) {
            return Vec::new();
        }

        let max_depth = ctx.config.threshold("playwright-max-nested-describe", "max", ctx.lang);
        let mut diagnostics = Vec::new();

        // For each describe call node, count how many describe-call ancestors it has.
        for node in semantic.nodes().iter() {
            let AstKind::CallExpression(call) = node.kind() else { continue };
            if !is_describe_call(&call.callee) {
                continue;
            }

            // Count describe ancestors
            let mut depth = 0usize;
            let mut cur = node.id();
            loop {
                let parent_id = semantic.nodes().parent_id(cur);
                if parent_id == cur {
                    // Reached root
                    break;
                }
                let parent = semantic.nodes().get_node(parent_id);
                if let AstKind::CallExpression(pc) = parent.kind()
                    && is_describe_call(&pc.callee) {
                        depth += 1;
                    }
                cur = parent_id;
            }

            // depth is the number of describe ancestors; total nesting = depth + 1
            let total_depth = depth + 1;
            if total_depth > max_depth {
                let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!("Describe depth {total_depth} exceeds maximum allowed {max_depth}."),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }

        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_oxc_ts_with_path;

    const PW_IMPORT: &str = "import { test, expect } from \"@playwright/test\";\n";


    fn run_oxc_ts(source: &str) -> Vec<Diagnostic> {
        run_oxc_ts_with_path(&format!("{PW_IMPORT}{source}"), &Check, "app.test.ts")
    }


    #[test]
    fn flags_deeply_nested_describe() {
        let src = "\
describe('1', () => {
  describe('2', () => {
    describe('3', () => {
      describe('4', () => {
        describe('5', () => {
          describe('6', () => {
            test('deep', () => {});
          });
        });
      });
    });
  });
});";
        let d = run_oxc_ts(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "playwright-max-nested-describe");
    }


    #[test]
    fn allows_five_levels() {
        let src = "\
describe('1', () => {
  describe('2', () => {
    describe('3', () => {
      describe('4', () => {
        describe('5', () => {
          test('ok', () => {});
        });
      });
    });
  });
});";
        let d = run_oxc_ts(src);
        assert!(d.is_empty());
    }
}
