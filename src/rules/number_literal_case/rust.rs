//! number-literal-case Rust backend — enforce lowercase prefix, uppercase hex digits.

use crate::diagnostic::{Diagnostic, Severity};

/// Flag only mixed-case hex digits (e.g. `0xfF`). All-uppercase (`0xFF`)
/// and all-lowercase (`0xff`) are both valid Rust conventions.
/// Never touch the type suffix (`u8`, `i32`) — it must be lowercase.
fn canonical(raw: &str) -> Option<String> {
    if raw.len() < 2 {
        return None;
    }

    let prefix_lower = raw[..2].to_lowercase();
    match prefix_lower.as_str() {
        "0x" => {
            let after = &raw[2..];
            let hex_end = after
                .find(|c: char| !c.is_ascii_hexdigit() && c != '_')
                .unwrap_or(after.len());
            let hex = &after[..hex_end];

            let has_upper = hex.chars().any(|c| c.is_ascii_uppercase());
            let has_lower = hex.chars().any(|c| c.is_ascii_lowercase());
            if !(has_upper && has_lower) {
                return None;
            }
            let suffix = &after[hex_end..];
            let fixed = format!("0x{}{suffix}", hex.to_uppercase());
            if fixed == raw { None } else { Some(fixed) }
        }
        "0b" | "0o" => {
            let fixed = format!("{}{}", prefix_lower, &raw[2..]);
            if fixed == raw { None } else { Some(fixed) }
        }
        _ => None,
    }
}

crate::ast_check! { on ["integer_literal"] => |node, source, ctx, diagnostics|
    let raw = node.utf8_text(source).unwrap_or("");
    if let Some(fixed) = canonical(raw) {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "number-literal-case".into(),
            message: format!(
                "Invalid number literal casing: `{}` should be `{}`.",
                raw, fixed
            ),
            severity: Severity::Warning,
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
    
    #[test]
    fn allows_all_lowercase_hex() {
        assert!(crate::rules::test_helpers::run_rule(&Check, "fn f() { let x = 0xff; }", "t.rs").is_empty());
    }

    #[test]
    fn flags_mixed_case_hex_digits() {
        let d = crate::rules::test_helpers::run_rule(&Check, "fn f() { let x = 0xfF; }", "t.rs");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("0xFF"));
    }

    #[test]
    fn allows_all_uppercase_hex() {
        assert!(crate::rules::test_helpers::run_rule(&Check, "fn f() { let x = 0xFF; }", "t.rs").is_empty());
    }

    #[test]
    fn allows_lowercase_hex_with_type_suffix() {
        assert!(crate::rules::test_helpers::run_rule(&Check, "fn f() { let x = 0xffu8; }", "t.rs").is_empty());
    }

    #[test]
    fn flags_mixed_hex_preserves_suffix() {
        let d = crate::rules::test_helpers::run_rule(&Check, "fn f() { let x = 0xfFu8; }", "t.rs");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("0xFFu8"));
    }

    #[test]
    fn allows_correct_binary() {
        assert!(crate::rules::test_helpers::run_rule(&Check, "fn f() { let x = 0b1010; }", "t.rs").is_empty());
    }

    #[test]
    fn allows_correct_octal() {
        assert!(crate::rules::test_helpers::run_rule(&Check, "fn f() { let x = 0o777; }", "t.rs").is_empty());
    }

    #[test]
    fn allows_plain_integer() {
        assert!(crate::rules::test_helpers::run_rule(&Check, "fn f() { let x = 42; }", "t.rs").is_empty());
    }
}
