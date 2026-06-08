//! prefer-module backend — flag CommonJS patterns in ESM-capable files.
//!
//! Detected patterns (via AST):
//! - `require("…")` calls
//! - `module.exports` assignments
//! - `exports.foo` assignments
//! - `__dirname` / `__filename` identifiers

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if !crate::rules::module_system::is_es_module_context(ctx.path, ctx.project) {
        return;
    }

    match node.kind() {
        // `require("…")` calls
        "call_expression" => {
            let Some(func) = node.child_by_field_name("function") else { return };
            if func.kind() != "identifier" { return; }
            let name = func.utf8_text(source).unwrap_or("");
            if name != "require" { return; }

            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "prefer-module".into(),
                message: "Use `import` instead of `require()` — prefer ESM over CommonJS.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }
        // `__dirname` / `__filename` identifiers
        "identifier" => {
            let name = node.utf8_text(source).unwrap_or("");
            let msg = match name {
                "__dirname" => "Use `import.meta.dirname` instead of `__dirname`.",
                "__filename" => "Use `import.meta.filename` instead of `__filename`.",
                _ => return,
            };

            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "prefer-module".into(),
                message: msg.into(),
                severity: Severity::Warning,
                span: None,
            });
        }
        // `module.exports` or `exports.foo`
        "member_expression" => {
            let Some(object) = node.child_by_field_name("object") else { return };
            let Some(property) = node.child_by_field_name("property") else { return };

            let obj_name = object.utf8_text(source).unwrap_or("");
            let prop_name = property.utf8_text(source).unwrap_or("");

            // Avoid double-reporting: skip if parent is also a member_expression
            // with `module.exports` as the object (e.g. `module.exports.foo`).
            if let Some(parent) = node.parent()
                && parent.kind() == "member_expression"
                    && let Some(parent_obj) = parent.child_by_field_name("object")
                        && std::ptr::eq(
                            &*std::format!("{}", parent_obj.id()),
                            &*std::format!("{}", node.id()),
                        ) {
                            // This node is the object of a parent member_expression;
                            // let the parent handle reporting.
                        }

            if obj_name == "module" && prop_name == "exports" {
                let pos = node.start_position();
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: pos.row + 1,
                    column: pos.column + 1,
                    rule_id: "prefer-module".into(),
                    message: "Use `export` instead of `module.exports` — prefer ESM over CommonJS.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            } else if obj_name == "exports" && object.kind() == "identifier" {
                // `exports.foo` but NOT `module.exports`
                let pos = node.start_position();
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: pos.row + 1,
                    column: pos.column + 1,
                    rule_id: "prefer-module".into(),
                    message: "Use `export` instead of `exports.x = …` — prefer ESM over CommonJS.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        _ => {}
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
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "module.mjs")
    }

    fn run_on_path(source: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, path)
    }

    #[test]
    fn allows_commonjs_when_package_type_is_absent() {
        let d = run_on_path(
            r#"
            const fs = require("fs");
            module.exports = fs;
            "#,
            "server.js",
        );
        assert!(d.is_empty());
    }

    #[test]
    fn flags_require() {
        let d = run_on(r#"const fs = require("fs");"#);
        assert!(!d.is_empty());
        assert!(d[0].message.contains("require()"));
    }

    #[test]
    fn flags_module_exports() {
        let d = run_on("module.exports = foo;");
        assert!(!d.is_empty());
        assert!(d.iter().any(|d| d.message.contains("module.exports")));
    }

    #[test]
    fn flags_exports_member() {
        let d = run_on("exports.bar = 42;");
        assert!(!d.is_empty());
        assert!(d.iter().any(|d| d.message.contains("exports.x")));
    }

    #[test]
    fn flags_dirname() {
        let d = run_on("const dir = __dirname;");
        assert!(!d.is_empty());
        assert!(d.iter().any(|d| d.message.contains("import.meta.dirname")));
    }

    #[test]
    fn flags_filename() {
        let d = run_on("const file = __filename;");
        assert!(!d.is_empty());
        assert!(d.iter().any(|d| d.message.contains("import.meta.filename")));
    }

    #[test]
    fn allows_esm_import() {
        assert!(run_on(r#"import fs from "node:fs";"#).is_empty());
    }
}
