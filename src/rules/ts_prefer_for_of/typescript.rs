//! ts-prefer-for-of backend — flag `for (let i = 0; i < arr.length; i++)`
//! loops where `i` is only used as `arr[i]` (never assigned or used
//! standalone).
//!
//! Simplified heuristic: check the canonical `for` pattern and look at
//! the loop body text to see if `i` only appears in `arr[i]` contexts.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["for_statement"] => |node, source, ctx, diagnostics|
    // 1. Init: `let i = 0` or `var i = 0`
    let Some(init) = node.child_by_field_name("initializer") else {
        return;
    };
    // Should be a variable/lexical declaration with one declarator
    if init.kind() != "variable_declaration" && init.kind() != "lexical_declaration" {
        return;
    }
    let mut ic = init.walk();
    let declarators: Vec<_> = init.named_children(&mut ic)
        .filter(|c| c.kind() == "variable_declarator")
        .collect();
    if declarators.len() != 1 {
        return;
    }
    let decl = declarators[0];
    let Some(name_node) = decl.child_by_field_name("name") else {
        return;
    };
    if name_node.kind() != "identifier" {
        return;
    }
    let idx_name = match std::str::from_utf8(&source[name_node.byte_range()]) {
        Ok(s) => s.trim(),
        Err(_) => return,
    };
    // Check init value is 0
    let Some(init_val) = decl.child_by_field_name("value") else {
        return;
    };
    if init_val.kind() != "number" {
        return;
    }
    let init_text = match std::str::from_utf8(&source[init_val.byte_range()]) {
        Ok(s) => s.trim(),
        Err(_) => return,
    };
    if init_text != "0" {
        return;
    }
    // 2. Condition: `i < arr.length`
    let Some(condition) = node.child_by_field_name("condition") else {
        return;
    };
    if condition.kind() != "binary_expression" {
        return;
    }
    // Find the `<` operator among anonymous children
    let mut has_lt = false;
    {
        let mut oc = condition.walk();
        for child in condition.children(&mut oc) {
            if !child.is_named() && &source[child.byte_range()] == b"<" {
                has_lt = true;
                break;
            }
        }
    }
    if !has_lt {
        return;
    }
    let Some(cond_left) = condition.child_by_field_name("left") else {
        return;
    };
    if cond_left.kind() != "identifier" {
        return;
    }
    let cond_left_name = match std::str::from_utf8(&source[cond_left.byte_range()]) {
        Ok(s) => s.trim(),
        Err(_) => return,
    };
    if cond_left_name != idx_name {
        return;
    }
    // Right should be `something.length`
    let Some(cond_right) = condition.child_by_field_name("right") else {
        return;
    };
    if cond_right.kind() != "member_expression" {
        return;
    }
    let Some(length_prop) = cond_right.child_by_field_name("property") else {
        return;
    };
    if &source[length_prop.byte_range()] != b"length" {
        return;
    }
    let Some(arr_node) = cond_right.child_by_field_name("object") else {
        return;
    };
    let arr_text = match std::str::from_utf8(&source[arr_node.byte_range()]) {
        Ok(s) => s.trim().to_string(),
        Err(_) => return,
    };
    // 3. Increment: `i++` or `++i` or `i += 1`
    let Some(increment) = node.child_by_field_name("increment") else {
        return;
    };
    let inc_text = match std::str::from_utf8(&source[increment.byte_range()]) {
        Ok(s) => s.trim().to_string(),
        Err(_) => return,
    };
    let valid_inc = inc_text == format!("{idx_name}++")
        || inc_text == format!("++{idx_name}")
        || inc_text == format!("{idx_name} += 1")
        || inc_text == format!("{idx_name}+=1");
    if !valid_inc {
        return;
    }
    // 4. Check that in the body, the index var is only used as arr[i]
    let Some(body) = node.child_by_field_name("body") else {
        return;
    };
    let body_text = match std::str::from_utf8(&source[body.byte_range()]) {
        Ok(s) => s,
        Err(_) => return,
    };
    // Simple heuristic: every occurrence of the index variable in the body
    // should be preceded by `arr_text[` and followed by `]`
    let pattern_bracket = format!("{arr_text}[{idx_name}]");
    // Remove all valid arr[i] occurrences, then check if i still appears
    let cleaned = body_text.replace(&pattern_bracket, "");
    // Check if idx_name still appears as a word boundary identifier
    let mut still_used = false;
    let idx_bytes = idx_name.as_bytes();
    let cleaned_bytes = cleaned.as_bytes();
    let idx_len = idx_bytes.len();
    for pos in 0..cleaned_bytes.len() {
        if pos + idx_len > cleaned_bytes.len() {
            break;
        }
        if &cleaned_bytes[pos..pos + idx_len] == idx_bytes {
            // Check boundaries
            let before_ok = pos == 0 || !cleaned_bytes[pos - 1].is_ascii_alphanumeric() && cleaned_bytes[pos - 1] != b'_';
            let after_ok = pos + idx_len == cleaned_bytes.len()
                || !cleaned_bytes[pos + idx_len].is_ascii_alphanumeric()
                    && cleaned_bytes[pos + idx_len] != b'_';
            if before_ok && after_ok {
                still_used = true;
                break;
            }
        }
    }
    if still_used {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "ts-prefer-for-of".into(),
        message: "Use `for-of` instead of an index-only `for` loop.".into(),
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
    fn flags_simple_index_loop() {
        let src = "for (let i = 0; i < arr.length; i++) { console.log(arr[i]); }";
        let diags = run_on(src);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_index_used_standalone() {
        let src = "for (let i = 0; i < arr.length; i++) { console.log(i); }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_non_standard_for() {
        let src = "for (let i = 1; i < arr.length; i++) { console.log(arr[i]); }";
        assert!(run_on(src).is_empty());
    }
}
