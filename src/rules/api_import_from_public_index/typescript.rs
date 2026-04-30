//! AST backend: flag relative imports that (a) cross two or more
//! parent segments (`../../`) and (b) target a specific internal file
//! rather than the feature's index. `types` and `utils` are
//! whitelisted as commonly-shared leaves.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["import_statement"] => |node, source, ctx, diagnostics|
    let source_node = match node.child_by_field_name("source") {
        Some(s) => s,
        None => return,
    };
    let import_path = source_node
        .utf8_text(source)
        .unwrap_or("")
        .trim_matches(|c: char| c == '\'' || c == '"');

    // Only cross-feature imports (2+ parent segments).
    let parent_count = import_path.split('/').filter(|s| *s == "..").count();
    if parent_count < 2 { return; }

    // A bare feature-root import (`../../users`) has exactly one
    // non-`..` segment — the feature name — and that *is* the public
    // index. Anything deeper (`../../users/db/queries`) has 2+ and is
    // reaching into internals.
    let non_parent_segments: Vec<&str> = import_path
        .split('/')
        .filter(|s| *s != ".." && !s.is_empty())
        .collect();
    if non_parent_segments.len() <= 1 { return; }

    // Flag if the import doesn't end at an index file.
    let last_segment = *non_parent_segments.last().unwrap_or(&"");
    if last_segment == "index" { return; }
    // Skip obvious shared-leaf imports.
    if last_segment == "types" || last_segment == "utils" { return; }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &source_node,
        super::META.id,
        format!("Import from `{import_path}` crosses a feature boundary — import from the public index instead."),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::Check;
    use crate::diagnostic::Diagnostic;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_deep_cross_feature_import() {
        assert_eq!(
            run("import { query } from '../../users/db/queries'").len(),
            1
        );
    }

    #[test]
    fn allows_index_import() {
        assert!(run("import { User } from '../../users'").is_empty());
    }

    #[test]
    fn allows_single_parent() {
        assert!(run("import { helper } from '../utils/format'").is_empty());
    }
}
