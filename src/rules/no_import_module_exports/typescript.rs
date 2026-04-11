//! no-import-module-exports backend — flag files mixing `import`
//! declarations with `module.exports` / `exports.*`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    // Only process at program level to scan the whole file once.
    if node.kind() != "program" {
        return;
    }

    let mut has_import = false;
    let mut module_exports_nodes: Vec<tree_sitter::Node> = Vec::new();

    let child_count = node.named_child_count();
    for i in 0..child_count {
        let Some(child) = node.named_child(i) else { continue };

        if child.kind() == "import_statement" {
            has_import = true;
            continue;
        }

        // expression_statement containing module.exports = ... or exports.foo = ...
        if child.kind() == "expression_statement"
            && let Some(expr) = child.named_child(0)
                && expr.kind() == "assignment_expression"
                    && let Some(left) = expr.child_by_field_name("left")
                        && let Ok(left_text) = left.utf8_text(source)
                            && (left_text.starts_with("module.exports")
                                || left_text.starts_with("exports."))
                            {
                                module_exports_nodes.push(child);
                            }
    }

    if !has_import {
        return;
    }

    for me_node in module_exports_nodes {
        let pos = me_node.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "no-import-module-exports".into(),
            message: "Cannot use `module.exports`/`exports` in a module that uses `import` declarations — pick one module system.".into(),
            severity: Severity::Warning,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_mixed_modules() {
        let src = "import { a } from 'a';\nmodule.exports = { a };";
        let diags = run_on(src);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 2);
    }

    #[test]
    fn allows_pure_esm() {
        let src = "import { a } from 'a';\nexport const b = a;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_pure_cjs() {
        let src = "const a = require('a');\nmodule.exports = { a };";
        assert!(run_on(src).is_empty());
    }
}
