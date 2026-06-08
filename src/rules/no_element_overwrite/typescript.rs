//! no-element-overwrite backend — flag consecutive writes to the same
//! collection element.

use crate::diagnostic::{Diagnostic, Severity};

/// Extract the assignment target from bracket-notation: `arr[0] = ...` -> `arr[0]`
fn bracket_target(text: &str) -> Option<String> {
    let trimmed = text.trim();
    let bracket_end = trimmed.find(']')?;
    let _bracket_start = trimmed[..bracket_end].find('[')?;
    let after = trimmed[bracket_end + 1..].trim_start();
    if after.starts_with('=') && !after.starts_with("==") {
        Some(trimmed[..bracket_end + 1].to_string())
    } else {
        None
    }
}

/// Extract the key from `.set("key", ...)` -> `<receiver>.set(<key>)`
fn map_set_target(text: &str) -> Option<String> {
    let trimmed = text.trim();
    let pos = trimmed.find(".set(")?;
    let receiver = trimmed[..pos].trim();
    let args_start = pos + 5;
    let rest = &trimmed[args_start..];
    let comma = rest.find(',')?;
    let key = rest[..comma].trim();
    Some(format!("{}.set({})", receiver, key))
}

crate::ast_check! { on ["expression_statement"] => |node, source, ctx, diagnostics|
    let Some(next_sibling) = node.next_named_sibling() else { return };
    if next_sibling.kind() != "expression_statement" {
        return;
    }
    let Ok(text1) = node.utf8_text(source) else { return };
    let Ok(text2) = next_sibling.utf8_text(source) else { return };

    // Check bracket notation
    if let (Some(t1), Some(t2)) = (bracket_target(text1), bracket_target(text2))
        && t1 == t2 {
            let pos = next_sibling.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "no-element-overwrite".into(),
                message: format!("`{}` is assigned on the previous line and immediately overwritten.", t1),
                severity: Severity::Error,
                span: None,
            });
            return;
        }

    // Check .set() calls
    if let (Some(t1), Some(t2)) = (map_set_target(text1), map_set_target(text2))
        && t1 == t2 {
            let pos = next_sibling.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "no-element-overwrite".into(),
                message: "`.set()` with the same key on the previous line — first write is dead.".into(),
                severity: Severity::Error,
                span: None,
            });
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
    fn flags_consecutive_bracket_writes() {
        let src = "arr[0] = 1;\narr[0] = 2;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_consecutive_map_set() {
        let src = "map.set(\"key\", 1);\nmap.set(\"key\", 2);";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_different_indices() {
        let src = "arr[0] = 1;\narr[1] = 2;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_different_keys() {
        let src = "map.set(\"a\", 1);\nmap.set(\"b\", 2);";
        assert!(run_on(src).is_empty());
    }
}
