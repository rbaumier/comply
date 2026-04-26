//! no-full-import backend — flag whole-library imports from utility
//! packages known to break tree-shaking.
//!
//! Flags: `import _ from 'lodash'`, `import * as _ from 'lodash'`,
//! and the same for `underscore`, `ramda`.
//! Allows: `import { debounce } from 'lodash'` (named imports can still
//! tree-shake with `lodash-es`; `lodash` CJS cannot, but ESLint's original
//! rule scopes its warning to default/namespace forms).

use crate::diagnostic::{Diagnostic, Severity};

const HEAVY_LIBS: &[&str] = &["lodash", "underscore", "ramda"];

fn strip_quotes(s: &str) -> &str {
    s.trim_matches(|c| c == '\'' || c == '"' || c == '`')
}

crate::ast_check! { on ["import_statement"] => |node, source, ctx, diagnostics|
    let Some(src_node) = node.child_by_field_name("source") else { return };
    let raw = src_node.utf8_text(source).unwrap_or("");
    let module = strip_quotes(raw);
    if !HEAVY_LIBS.contains(&module) {
        return;
    }
    // Locate the import_clause to decide if this is a default/namespace import.
    let mut cursor = node.walk();
    let Some(clause) = node
        .named_children(&mut cursor)
        .find(|c| c.kind() == "import_clause")
    else {
        // Side-effect import `import 'lodash';` — leave to no-unassigned-import.
        return;
    };
    let mut cc = clause.walk();
    let mut has_default = false;
    let mut has_namespace = false;
    for child in clause.named_children(&mut cc) {
        match child.kind() {
            "identifier" => has_default = true,
            "namespace_import" => has_namespace = true,
            _ => {}
        }
    }
    if !has_default && !has_namespace {
        return;
    }
    let form = if has_namespace { "namespace" } else { "default" };
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-full-import".into(),
        message: format!(
            "Avoid {form} import of the whole `{module}` library — import the specific function \
             (e.g. `{module}/debounce` or a named import) to keep the bundle small."
        ),
        severity: Severity::Warning,
        span: Some((node.start_byte(), node.end_byte() - node.start_byte())),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_default_lodash_import() {
        let d = run_on("import _ from 'lodash';");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("lodash"));
    }

    #[test]
    fn flags_namespace_lodash_import() {
        let d = run_on("import * as _ from 'lodash';");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("namespace"));
    }

    #[test]
    fn flags_default_underscore_import() {
        let d = run_on("import _ from 'underscore';");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_default_ramda_import() {
        let d = run_on("import R from 'ramda';");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_named_lodash_import() {
        assert!(run_on("import { debounce } from 'lodash';").is_empty());
    }

    #[test]
    fn allows_sub_path_import() {
        assert!(run_on("import debounce from 'lodash/debounce';").is_empty());
    }

    #[test]
    fn allows_unrelated_library() {
        assert!(run_on("import React from 'react';").is_empty());
    }
}
