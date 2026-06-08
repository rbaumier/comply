//! no-anonymous-default-export backend — flag `export default function() {}`
//! and `export default class {}` (anonymous default exports).

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["export_statement"] prefilter = ["export default"] => |node, source, ctx, diagnostics|
    // Look for a `default` keyword child.
    let mut has_default = false;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "default" || (child.kind() == "identifier" && child.utf8_text(source).ok() == Some("default")) {
            has_default = true;
        }
    }
    // Also check via the text prefix for robustness.
    if !has_default {
        let Ok(text) = node.utf8_text(source) else { return };
        if !text.starts_with("export default") {
            return;
        }
    }

    // Find the declaration child (function_declaration, generator_function_declaration, class_declaration).
    let mut cursor2 = node.walk();
    for child in node.children(&mut cursor2) {
        let kind = child.kind();
        let is_fn = kind == "function_declaration"
            || kind == "generator_function_declaration"
            || kind == "function"
            || kind == "function_expression"
            || kind == "generator_function_expression";
        let is_class = kind == "class_declaration" || kind == "class" || kind == "class_expression";

        if !is_fn && !is_class {
            continue;
        }

        // Check if it has a name.
        let has_name = child
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(source).ok())
            .is_some_and(|n| !n.is_empty());

        if has_name {
            return;
        }

        let label = if is_fn { "function" } else { "class" };
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "no-anonymous-default-export".into(),
            message: format!(
                "Anonymous default export {label} — give it a name for \
                 better stack traces and refactoring support."
            ),
            severity: Severity::Warning,
            span: None,
        });
        return;
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
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_anonymous_function() {
        let d = run_on("export default function() {}");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("function"));
    }

    #[test]
    fn flags_anonymous_class() {
        let d = run_on("export default class {}");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("class"));
    }

    #[test]
    fn allows_named_function() {
        assert!(run_on("export default function myFn() {}").is_empty());
    }

    #[test]
    fn allows_named_class() {
        assert!(run_on("export default class MyClass {}").is_empty());
    }

    #[test]
    fn allows_identifier_export() {
        assert!(run_on("export default myVariable;").is_empty());
    }
}
