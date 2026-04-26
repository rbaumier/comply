//! Flag `@tailwind base/components/utilities` (Tailwind v3 syntax).

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["at_rule"] => |node, source, ctx, diagnostics|
    let mut c = node.walk();
    let Some(kw) = node.children(&mut c).find(|n| n.kind() == "at_keyword") else { return };
    if !kw.utf8_text(source).is_ok_and(|t| t.eq_ignore_ascii_case("@tailwind")) { return; }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: "Legacy `@tailwind` directive — replace the three `@tailwind base/components/utilities` lines with a single `@import \"tailwindcss\";` (Tailwind v4).".into(),
        severity: Severity::Warning,
        span: Some((node.byte_range().start, node.byte_range().len())),
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_css;

    fn run(source: &str) -> Vec<Diagnostic> {
        run_css(source, &Check)
    }

    #[test]
    fn flags_tailwind_base() {
        assert_eq!(run("@tailwind base;\n@tailwind components;\n@tailwind utilities;").len(), 3);
    }

    #[test]
    fn flags_indented_directive() {
        assert_eq!(run("  @tailwind base;").len(), 1);
    }

    #[test]
    fn allows_v4_import() {
        assert!(run("@import \"tailwindcss\";").is_empty());
    }
}
