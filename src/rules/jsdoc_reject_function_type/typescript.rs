use crate::diagnostic::{Diagnostic, Severity};

/// Detects JSDoc type annotations using bare `Function` or `function` instead
/// of a specific function signature, e.g. `@param {Function} cb`.
///
/// Walks tree-sitter `comment` nodes whose text starts with `/**` (JSDoc) and
/// scans each line for `{Function}` / `{function}` braces.
fn find_bare_function_types_in_line(line: &str) -> Vec<usize> {
    let mut hits = Vec::new();
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'{' {
            let start = i;
            let mut j = i + 1;
            while j < bytes.len() && bytes[j] != b'}' {
                j += 1;
            }
            if j < bytes.len() {
                let type_content = line[start + 1..j].trim();
                if type_content == "Function" || type_content == "function" {
                    hits.push(start);
                }
            }
        }
        i += 1;
    }
    hits
}

crate::ast_check! { on ["comment"] => |node, source, ctx, diagnostics|
    let Ok(raw) = node.utf8_text(source) else { return };
    if !raw.starts_with("/**") { return; }

    let start = node.start_position();
    for (line_idx, line) in raw.lines().enumerate() {
        for col in find_bare_function_types_in_line(line) {
            let abs_line = start.row + line_idx + 1;
            let abs_col = if line_idx == 0 { start.column + col + 1 } else { col + 1 };
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: abs_line,
                column: abs_col,
                rule_id: super::META.id.into(),
                message: "JSDoc uses bare `Function` type \u{2014} provide a specific function signature instead.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_bare_function() {
        let src = "/**\n * @param {Function} cb - callback\n */\nfunction f(cb) {}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_lowercase_function() {
        let src = "/**\n * @param {function} handler\n */\nfunction f(handler) {}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_specific_signature() {
        let src = "/**\n * @param {(x: string) => void} cb\n */\nfunction f(cb) {}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_non_jsdoc_comment() {
        let src = "// @param {Function} cb\nfunction f(cb) {}";
        assert!(run(src).is_empty());
    }
}
