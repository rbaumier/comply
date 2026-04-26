//! no-named-default backend — flag `import { default as foo }` patterns.
//!
//! The named form `{ default as foo }` is verbose and obscures intent.
//! The idiomatic form is `import foo from './m'`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["import_statement"] => |node, source, ctx, diagnostics|
    // Walk named import specifiers inside the import_clause
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        visit_named_default(child, source, ctx, diagnostics);
    }
}

fn visit_named_default(
    node: tree_sitter::Node,
    source: &[u8],
    ctx: &crate::rules::backend::CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if node.kind() == "import_specifier" {
        let Some(name_node) = node.child_by_field_name("name") else {
            return;
        };
        let name = match name_node.utf8_text(source) {
            Ok(t) => t,
            Err(_) => return,
        };
        if name != "default" {
            return;
        }
        let alias = node
            .child_by_field_name("alias")
            .and_then(|a| a.utf8_text(source).ok())
            .unwrap_or("default");
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "no-named-default".into(),
            message: format!(
                "Replace `{{ default as {alias} }}` with `import {alias} from …` \
                 — prefer the default import syntax."
            ),
            severity: Severity::Warning,
            span: None,
        });
        return;
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        visit_named_default(child, source, ctx, diagnostics);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_named_default_import() {
        let d = run_on(r#"import { default as foo } from './m';"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("import foo from"));
    }

    #[test]
    fn flags_named_default_with_others() {
        let d = run_on(r#"import { default as foo, bar } from './m';"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("foo"));
    }

    #[test]
    fn allows_regular_default_import() {
        assert!(run_on(r#"import foo from './m';"#).is_empty());
    }

    #[test]
    fn allows_named_imports() {
        assert!(run_on(r#"import { bar, baz } from './m';"#).is_empty());
    }
}
