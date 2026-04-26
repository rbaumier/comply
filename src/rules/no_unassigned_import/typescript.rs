//! no-unassigned-import backend — flag side-effect imports.

use crate::diagnostic::{Diagnostic, Severity};

/// Known CSS/style extensions that are legitimate side-effect imports.
const STYLE_EXTENSIONS: &[&str] = &[
    ".css", ".scss", ".sass", ".less", ".styl", ".stylus", ".pcss", ".postcss",
];

/// Check if the import source is a known style/CSS import.
fn is_style_import(source: &str) -> bool {
    STYLE_EXTENSIONS.iter().any(|ext| source.ends_with(ext))
}

crate::ast_check! { on ["import_statement"] => |node, source, ctx, diagnostics|
    // A side-effect import has no import clause — just `import 'foo';`.
    // In tree-sitter, an import with specifiers has an "import_clause" child.
    // If there's no import_clause, it's a bare side-effect import.
    let has_clause = {
        let mut cursor = node.walk();
        node.children(&mut cursor).any(|c| c.kind() == "import_clause")
    };

    if has_clause {
        return;
    }

    // Get the source string (the module specifier).
    let Some(source_node) = node.child_by_field_name("source") else { return };
    let src_text = source_node.utf8_text(source).unwrap_or("");
    // Strip quotes.
    let unquoted = src_text.trim_matches(|c| c == '\'' || c == '"');

    if is_style_import(unquoted) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-unassigned-import".into(),
        message: format!("Side-effect import `{}` \u{2014} imported module should be assigned.", unquoted),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_side_effect_import() {
        let d = crate::rules::test_helpers::run_ts("import 'polyfill';", &Check);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("polyfill"));
    }

    #[test]
    fn allows_css_import() {
        let d = crate::rules::test_helpers::run_ts("import './styles.css';", &Check);
        assert!(d.is_empty());
    }

    #[test]
    fn allows_named_import() {
        let d = crate::rules::test_helpers::run_ts("import { foo } from 'bar';", &Check);
        assert!(d.is_empty());
    }

    #[test]
    fn flags_double_quoted_side_effect() {
        let d = crate::rules::test_helpers::run_ts(r#"import "reflect-metadata";"#, &Check);
        assert_eq!(d.len(), 1);
    }
}
