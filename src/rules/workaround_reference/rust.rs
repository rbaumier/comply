use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["line_comment", "block_comment"] => |node, source, ctx, diagnostics|
    let Ok(text) = node.utf8_text(source) else { return };
    if !super::has_keyword(text) { return; }
    if super::has_reference(text) { return; }

    let row = node.start_position().row;
    let lines: Vec<&str> = ctx.source.lines().collect();
    let lookahead = (row + 1..=(row + 2).min(lines.len().saturating_sub(1)))
        .any(|i| super::has_reference(lines[i]));
    if lookahead { return; }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "Workaround/hack/compat comment without an issue reference — \
         add a link or ticket number.".into(),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(s, &Check)
    }

    #[test]
    fn flags_workaround_without_ref() {
        assert_eq!(run("// Workaround for fish\nfn f() {}").len(), 1);
    }

    #[test]
    fn allows_workaround_with_issue_ref() {
        assert!(run("// Workaround for a fish bug (see #739, #279)\nfn f() {}").is_empty());
    }

    #[test]
    fn allows_workaround_with_url() {
        assert!(run("// Workaround for https://github.com/org/repo/issues/1\nfn f() {}").is_empty());
    }

    #[test]
    fn allows_ref_on_next_line() {
        assert!(run("// Workaround for fish bug\n// See #739\nfn f() {}").is_empty());
    }

    #[test]
    fn flags_hack_without_ref() {
        assert_eq!(run("// hack to fix rendering\nfn f() {}").len(), 1);
    }

    #[test]
    fn flags_compat_without_ref() {
        assert_eq!(run("// compat shim for old API\nfn f() {}").len(), 1);
    }

    #[test]
    fn allows_jira_ref() {
        assert!(run("// Workaround for PROJ-123\nfn f() {}").is_empty());
    }
}
