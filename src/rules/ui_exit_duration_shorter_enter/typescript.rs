//! Detect `<motion.*>` JSX nodes whose `exit={{ transition: { duration: X } }}`
//! is longer than the `animate` / `initial` transition duration.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["jsx_opening_element", "jsx_self_closing_element"] prefilter = ["motion."] => |node, source, ctx, diagnostics|
    let Some(tag) = crate::rules::jsx::jsx_element_tag_name(node, source) else { return };
    if !tag.starts_with("motion.") {
        return;
    }

    let mut animate_dur: Option<f64> = None;
    let mut exit_dur: Option<f64> = None;
    let mut exit_node: Option<tree_sitter::Node> = None;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() != "jsx_attribute" { continue; }
        let Some(name) = crate::rules::jsx::jsx_attribute_name(child, source) else { continue };
        let Ok(text) = child.utf8_text(source) else { continue };
        let dur = extract_duration(text);
        match name {
            "animate" | "initial" | "transition" => {
                if dur.is_some() && animate_dur.is_none() {
                    animate_dur = dur;
                }
            }
            "exit" => {
                exit_dur = dur;
                exit_node = Some(child);
            }
            _ => {}
        }
    }

    let (Some(enter), Some(exit)) = (animate_dur, exit_dur) else { return };
    if exit <= enter { return; }

    let report_node = exit_node.unwrap_or(node);
    let pos = report_node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: format!(
            "<{tag}> exit duration {exit}s is longer than enter duration {enter}s — dismiss will feel sluggish."
        ),
        severity: Severity::Warning,
        span: None,
    });
}

/// Very small helper: find `duration: <number>` inside the raw attribute text.
fn extract_duration(text: &str) -> Option<f64> {
    let key = "duration:";
    let idx = text.find(key)?;
    let rest = text[idx + key.len()..].trim_start();
    let n: String = rest
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    n.parse().ok()
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
        crate::rules::test_helpers::run_rule(&Check, s, "t.tsx")
    }

    #[test]
    fn flags_exit_longer_than_enter() {
        let src = r#"
            const x = <motion.div
                animate={{ transition: { duration: 0.2 } }}
                exit={{ transition: { duration: 0.5 } }}
            />;
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_exit_shorter_than_enter() {
        let src = r#"
            const x = <motion.div
                animate={{ transition: { duration: 0.3 } }}
                exit={{ transition: { duration: 0.15 } }}
            />;
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_non_motion() {
        let src = r#"
            const x = <div
                animate={{ transition: { duration: 0.2 } }}
                exit={{ transition: { duration: 0.5 } }}
            />;
        "#;
        assert!(run(src).is_empty());
    }
}
