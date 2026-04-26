use crate::diagnostic::{Diagnostic, Severity};

/// Detects JSDoc type annotations using `*` or `any` which defeat the purpose
/// of type documentation, e.g. `@param {*} x` or `@returns {any}`.
///
/// Walks tree-sitter `comment` nodes whose text starts with `/**` (JSDoc) and
/// scans each line for `{*}` / `{any}` braces.
fn find_any_types_in_line(line: &str) -> Vec<usize> {
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
                if type_content == "*" || type_content.eq_ignore_ascii_case("any") {
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
        for col in find_any_types_in_line(line) {
            let abs_line = start.row + line_idx + 1;
            let abs_col = if line_idx == 0 { start.column + col + 1 } else { col + 1 };
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: abs_line,
                column: abs_col,
                rule_id: super::META.id.into(),
                message: "JSDoc uses `*` or `any` type \u{2014} provide a specific type instead.".into(),
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
    fn flags_star_type() {
        let src = "/**\n * @param {*} x - the value\n */\nfunction f(x) {}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_any_type() {
        let src = "/**\n * @returns {any}\n */\nfunction f() {}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_specific_type() {
        let src = "/**\n * @param {string} x - the value\n */\nfunction f(x) {}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_non_jsdoc_comment() {
        // Regular `// {any}` comment must not be flagged.
        let src = "// @param {any}\nfunction f() {}";
        assert!(run(src).is_empty());
    }
}
