//! import-no-commonjs backend — forbid CommonJS require/module.exports.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    let kind = node.kind();

    // Flag `require(...)` calls.
    if kind == "call_expression" {
        let Some(callee) = node.child_by_field_name("function") else { return };
        if callee.kind() == "identifier" && callee.utf8_text(source).unwrap_or("") == "require" {
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "import-no-commonjs".into(),
                message: "Expected `import` instead of `require()`.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }
        return;
    }

    // Flag `module.exports` or `exports.X`.
    if kind == "member_expression" {
        let Some(obj) = node.child_by_field_name("object") else { return };
        let Some(prop) = node.child_by_field_name("property") else { return };
        let obj_name = obj.utf8_text(source).unwrap_or("");
        let prop_name = prop.utf8_text(source).unwrap_or("");

        if obj_name == "module" && prop_name == "exports" {
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "import-no-commonjs".into(),
                message: "Expected `export` or `export default` instead of `module.exports`.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_require() {
        let d = run_on("const x = require('fs');");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("require()"));
    }

    #[test]
    fn flags_module_exports() {
        let d = run_on("module.exports = foo;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("module.exports"));
    }

    #[test]
    fn allows_import() {
        assert!(run_on("import fs from 'fs';").is_empty());
    }
}
