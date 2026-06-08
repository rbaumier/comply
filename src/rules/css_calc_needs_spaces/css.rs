use crate::diagnostic::{Diagnostic, Severity};

fn is_calc_like(name: &str) -> bool {
    matches!(name, "calc" | "min" | "max" | "clamp")
}

fn has_unspaced_operator(text: &str) -> bool {
    // Look for digit/unit char directly adjacent to + or - without a space.
    // We only care about + and - (binary). * and / don't require spaces in calc().
    let bytes = text.as_bytes();
    for i in 1..bytes.len().saturating_sub(1) {
        let b = bytes[i];
        if b != b'+' && b != b'-' {
            continue;
        }
        let prev = bytes[i - 1];
        let next = bytes[i + 1];
        // unary minus/plus directly after `(`, `,`, or another operator is fine.
        let prev_is_separator = matches!(
            prev,
            b'(' | b',' | b' ' | b'\t' | b'\n' | b'+' | b'-' | b'*' | b'/'
        );
        if prev_is_separator {
            continue;
        }
        // Operator must have whitespace on at least one side; flag if neither side has a space
        // OR if the previous side is alnum/% with no space.
        let prev_is_alnum = prev.is_ascii_alphanumeric() || prev == b'%' || prev == b')';
        let next_is_alnum_or_open = next.is_ascii_alphanumeric() || next == b'(' || next == b'.';
        if prev_is_alnum && !prev.is_ascii_whitespace() {
            // need space before
            if prev != b' ' && prev != b'\t' {
                return true;
            }
        }
        if next_is_alnum_or_open && next != b' ' && next != b'\t' {
            // The check above covers prev side; next side: if no space, flag.
            if !matches!(prev, b' ' | b'\t' | b'\n') {
                return true;
            }
        }
    }
    false
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let mut c = node.walk();
    let kids: Vec<_> = node.children(&mut c).collect();
    let Some(name_node) = kids.iter().find(|n| n.kind() == "function_name") else { return; };
    let name = name_node.utf8_text(source).unwrap_or_default().to_ascii_lowercase();
    if !is_calc_like(&name) { return; }
    let Some(args) = kids.iter().find(|n| n.kind() == "arguments") else { return; };
    let txt = args.utf8_text(source).unwrap_or_default();
    if has_unspaced_operator(txt) {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            super::META.id,
            format!("`{name}()` operator missing surrounding spaces."),
            Severity::Warning,
        ));
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

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.css")
    }

    #[test]
    fn flags_unspaced_calc() {
        assert_eq!(run(".a { width: calc(100%-10px); }").len(), 1);
    }

    #[test]
    fn allows_spaced_calc() {
        assert!(run(".a { width: calc(100% - 10px); }").is_empty());
    }
}
