//! Flags `import ... from 'lodash'` (default or named) — pulls the full
//! library. `lodash/map`, `lodash-es`, and `lodash-es/map` are allowed.

use crate::diagnostic::{Diagnostic, Severity};

fn import_source<'a>(node: tree_sitter::Node<'a>, source: &'a [u8]) -> Option<&'a str> {
    let src = node.child_by_field_name("source")?;
    let raw = src.utf8_text(source).ok()?;
    Some(raw.trim_matches(|c| c == '"' || c == '\''))
}

crate::ast_check! { on ["import_statement"] => |node, source, ctx, diagnostics|
    let Some(import_path) = import_source(node, source) else { return };
    if import_path != "lodash" {
        return;
    }
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: node.start_position().row + 1,
        column: node.start_position().column + 1,
        rule_id: super::META.id.into(),
        message: "Importing from `lodash` pulls the entire library — \
                  use `lodash/<fn>` subpath imports or `lodash-es` for tree-shaking.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_default_import() {
        assert_eq!(run(r#"import _ from 'lodash';"#).len(), 1);
    }

    #[test]
    fn flags_named_import() {
        assert_eq!(run(r#"import { map } from 'lodash';"#).len(), 1);
    }

    #[test]
    fn flags_namespace_import() {
        assert_eq!(run(r#"import * as _ from 'lodash';"#).len(), 1);
    }

    #[test]
    fn allows_subpath_import() {
        assert!(run(r#"import map from 'lodash/map';"#).is_empty());
    }

    #[test]
    fn allows_lodash_es() {
        assert!(run(r#"import { map } from 'lodash-es';"#).is_empty());
    }

    #[test]
    fn allows_lodash_es_subpath() {
        assert!(run(r#"import map from 'lodash-es/map';"#).is_empty());
    }
}
