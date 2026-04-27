//! In an `index.ts` file, only allow re-export statements.

use crate::diagnostic::{Diagnostic, Severity};

fn is_barrel_path(path: &std::path::Path) -> bool {
    matches!(
        path.file_name().and_then(|n| n.to_str()),
        Some("index.ts") | Some("public-api.ts") | Some("public_api.ts")
    )
}

crate::ast_check! { on ["program"] => |node, source, ctx, diagnostics|
    if !is_barrel_path(ctx.path) { return; }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "export_statement" | "import_statement" | "comment" | "empty_statement" => continue,
            _ => {
                let text = std::str::from_utf8(&source[child.byte_range()]).unwrap_or("");
                let snippet: String = text.chars().take(60).collect();
                diagnostics.push(Diagnostic::at_node(
                    ctx.path,
                    &child,
                    super::META.id,
                    format!("Barrel file should only re-export — found side-effecting statement: `{snippet}`."),
                    Severity::Warning,
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_idx(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts_with_path(s, &Check, "index.ts")
    }
    fn run_other(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts_with_path(s, &Check, "thing.ts")
    }

    #[test]
    fn flags_side_effect_in_barrel() {
        let src = "export * from './a';\nconsole.log('side');";
        assert_eq!(run_idx(src).len(), 1);
    }

    #[test]
    fn allows_pure_reexports() {
        let src = "export * from './a';\nexport { B } from './b';";
        assert!(run_idx(src).is_empty());
    }

    #[test]
    fn ignores_non_barrel_files() {
        let src = "console.log('side');";
        assert!(run_other(src).is_empty());
    }
}
