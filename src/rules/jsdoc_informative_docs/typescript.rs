use crate::diagnostic::{Diagnostic, Severity};

/// Detects JSDoc descriptions that merely repeat the name of the symbol.
/// Example:
/// ```
/// /** The foo function. */
/// function foo() {}
/// ```
///
/// Walks tree-sitter `comment` nodes that are single-line JSDoc (`/** ... */`),
/// extracts the description, then reads up to 3 source lines after the comment
/// to identify the documented symbol's name.
fn extract_jsdoc_body(line: &str) -> String {
    let s = line.trim_start_matches("/**").trim_end_matches("*/").trim();
    s.to_lowercase()
}

fn get_next_symbol_name(lines: &[&str], from: usize) -> Option<String> {
    for line in lines.iter().take(lines.len().min(from + 3)).skip(from) {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("function ") {
            return extract_ident(rest);
        }
        for kw in &[
            "const ",
            "let ",
            "var ",
            "export const ",
            "export let ",
            "export function ",
        ] {
            if let Some(rest) = t.strip_prefix(kw) {
                return extract_ident(rest);
            }
        }
        if let Some(rest) = t.strip_prefix("class ") {
            return extract_ident(rest);
        }
        if let Some(rest) = t.strip_prefix("export class ") {
            return extract_ident(rest);
        }
        if let Some(paren) = t.find('(') {
            let candidate = t[..paren].trim();
            if !candidate.is_empty() && candidate.chars().all(|c| c.is_alphanumeric() || c == '_') {
                return Some(candidate.to_lowercase());
            }
        }
    }
    None
}

fn extract_ident(s: &str) -> Option<String> {
    let ident: String = s
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    if ident.is_empty() {
        None
    } else {
        Some(ident.to_lowercase())
    }
}

fn is_trivial_description(body: &str, name: &str) -> bool {
    if body.is_empty() || name.is_empty() {
        return false;
    }
    let normalized = body
        .replace("the ", "")
        .replace("a ", "")
        .replace("an ", "")
        .replace("this ", "")
        .replace("function", "")
        .replace("method", "")
        .replace("class", "")
        .replace("variable", "")
        .replace(".", "")
        .trim()
        .to_lowercase();
    normalized == *name || normalized.is_empty()
}

crate::ast_check! { on ["comment"] => |node, source, ctx, diagnostics|
    let Ok(raw) = node.utf8_text(source) else { return };
    let trimmed = raw.trim();
    // Only single-line JSDoc: /** ... */ on one line.
    if !(trimmed.starts_with("/**") && trimmed.ends_with("*/")) { return; }
    if trimmed.contains('\n') { return; }

    let body = extract_jsdoc_body(trimmed);
    let start = node.start_position();

    // Read source lines once to look ahead for the documented symbol.
    let src_str = std::str::from_utf8(source).unwrap_or("");
    let lines: Vec<&str> = src_str.lines().collect();
    let Some(name) = get_next_symbol_name(&lines, start.row + 1) else { return };

    if !is_trivial_description(&body, &name) { return; }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "JSDoc description merely repeats the symbol name without adding useful information.".into(),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_trivial_doc() {
        let src = "/** The foo function. */\nfunction foo() {}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_informative_doc() {
        let src = "/** Calculates the total price including tax. */\nfunction calculateTotal() {}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_just_name() {
        let src = "/** foo */\nconst foo = 1;";
        assert_eq!(run(src).len(), 1);
    }
}
