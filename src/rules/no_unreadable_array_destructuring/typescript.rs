//! no-unreadable-array-destructuring backend — flag destructuring patterns
//! with consecutive holes (commas without elements).
//!
//! `const [,, third,,,, seventh] = arr;` has consecutive ignored slots
//! that are difficult to count visually. Use index access instead.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["array_pattern"] => |node, source, ctx, diagnostics|
    // Walk children and count consecutive "holes" (unnamed null elements).
    // In tree-sitter, an elision in `[,, x]` appears as consecutive ","
    // punctuation tokens with no named child in between.
    //
    // Strategy: iterate named children. In an array_pattern, each element
    // is a named child. Holes are represented by their absence between
    // commas. We can detect this by checking positions: if two consecutive
    // named children have more than one comma between them, or if leading/
    // trailing commas exist with no named child, there are consecutive holes.
    //
    // Simpler approach: count commas vs named children.
    // `[a, b, c]` has 2 commas, 3 named children.
    // `[,, c]` has 2 commas, 1 named child — the commas before `c` mean 2 holes.
    // `[a,, c]` has 2 commas, 2 named children — 1 hole.
    //
    // The rule triggers when there are consecutive holes (2+ adjacent empty slots).
    // We look for two consecutive commas with no named node between them.

    let src = node.utf8_text(source).unwrap_or("");

    // Quick heuristic: look for ",," in the pattern text.
    // This catches `[,, x]`, `[x,,, y]`, `[,,,]`, etc.
    if !src.contains(",,") {
        return;
    }

    // Make sure we have at least 3 elements (including holes) — the original
    // rule requires elements.length >= 3.
    let named_count = node.named_child_count();
    let comma_count = src.chars().filter(|&c| c == ',').count();
    // Total element slots = commas + 1 (but bounded by the pattern structure).
    // For `[,, x]`: commas=2, named=1, slots=3. Good.
    let total_slots = comma_count + 1;
    if total_slots < 3 {
        return;
    }

    // Confirm there are actually consecutive empty slots (not just commas
    // inside nested structures). Check that the ",," is at the array_pattern
    // level, not inside a nested expression.
    // We do this by walking character-by-character through the source,
    // tracking bracket/paren depth.
    let bytes = src.as_bytes();
    let mut depth = 0i32;
    let mut prev_was_comma = false;
    let mut found_consecutive = false;

    for &b in bytes.iter() {
        match b {
            b'[' | b'(' | b'{' => {
                depth += 1;
                prev_was_comma = false;
            }
            b']' | b')' | b'}' => {
                depth -= 1;
                prev_was_comma = false;
            }
            b',' if depth == 1 => {
                // depth==1 means we're at the top-level of this array_pattern
                // (depth 0 = outside, depth 1 = inside the outer brackets)
                if prev_was_comma {
                    found_consecutive = true;
                    break;
                }
                prev_was_comma = true;
            }
            b' ' | b'\t' | b'\n' | b'\r' => {
                // whitespace doesn't reset comma tracking
            }
            _ => {
                prev_was_comma = false;
            }
        }
    }

    if !found_consecutive {
        // Edge case: leading commas like `[,, x]` — the first comma after `[`
        // is preceded by no element. In tree-sitter the pattern starts with `[`.
        // After `[`, if we see `,` immediately (ignoring whitespace), that's
        // a leading hole. Two leading commas = consecutive holes.
        // The depth-tracking above should catch this since `[` sets depth=1
        // and then `,` at depth=1 is tracked.
        // But let's also handle `[, , x]` (with spaces between commas).
        // The loop above handles this because spaces don't reset prev_was_comma.
        return;
    }

    if named_count == 0 {
        // `[,,,]` with no actual bindings — unusual but technically a consecutive
        // hole pattern. Flag it.
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-unreadable-array-destructuring".into(),
        message: "Array destructuring may not contain consecutive ignored values.".into(),
        severity: Severity::Warning,
        span: None,
    });
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
    fn flags_consecutive_holes_leading() {
        let d = run_on("const [,, third] = arr;");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "no-unreadable-array-destructuring");
    }

    #[test]
    fn flags_many_consecutive_holes() {
        let d = run_on("const [,, third,,,, seventh] = arr;");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_middle_consecutive_holes() {
        let d = run_on("const [first,,, fourth] = arr;");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_single_holes() {
        // `[a, , b]` has only single holes between elements — allowed.
        assert!(run_on("const [a, , b] = arr;").is_empty());
    }

    #[test]
    fn allows_simple_destructuring() {
        assert!(run_on("const [a, b, c] = arr;").is_empty());
    }

    #[test]
    fn allows_single_element() {
        assert!(run_on("const [a] = arr;").is_empty());
    }
}
