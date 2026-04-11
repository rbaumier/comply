//! no-redundant-jump backend — redundant `return;` or `continue;` at end of block.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "program" {
        return;
    }

    let text = std::str::from_utf8(source).unwrap_or("");
    let lines: Vec<&str> = text.lines().collect();

    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim();

        let is_return = trimmed == "return;" || trimmed == "return ;";
        let is_continue = trimmed == "continue;" || trimmed == "continue ;";

        if !is_return && !is_continue {
            continue;
        }

        // Find next non-blank line.
        let next_idx = lines[idx + 1..]
            .iter()
            .position(|l| !l.trim().is_empty())
            .map(|p| idx + 1 + p);

        let next_is_close = matches!(next_idx, Some(ni) if lines[ni].trim() == "}");

        if !next_is_close {
            continue;
        }

        // The `}` right after the jump must itself be at the end of the
        // enclosing scope.
        let close_idx = next_idx.unwrap();
        let after_close = lines[close_idx + 1..]
            .iter()
            .find(|l| !l.trim().is_empty());
        let is_end_of_scope =
            after_close.is_none() || matches!(after_close, Some(l) if l.trim() == "}");

        if is_end_of_scope {
            let keyword = if is_return { "return;" } else { "continue;" };
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: idx + 1,
                column: 1,
                rule_id: "no-redundant-jump".into(),
                message: format!(
                    "Redundant `{keyword}` — execution already falls through here."
                ),
                severity: Severity::Warning,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_redundant_return() {
        let src = "function foo() {\n  doStuff();\n  return;\n}";
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("return;"));
    }

    #[test]
    fn flags_redundant_continue() {
        let src = "for (const x of xs) {\n  doStuff();\n  continue;\n}";
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("continue;"));
    }

    #[test]
    fn allows_return_before_more_code() {
        let src = "function foo() {\n  if (x) {\n    return;\n  }\n  bar();\n}";
        assert!(run_on(src).is_empty());
    }
}
