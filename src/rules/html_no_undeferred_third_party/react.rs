//! Flags `<script src="https://...">` without `defer` or `async`.

use crate::diagnostic::{Diagnostic, Severity};

fn is_third_party_src(value: &str) -> bool {
    let trimmed = value.trim_matches(|c| c == '\'' || c == '"' || c == '`');
    trimmed.starts_with("http://") || trimmed.starts_with("https://") || trimmed.starts_with("//")
}

crate::ast_check! { on ["jsx_self_closing_element", "jsx_opening_element"] => |node, source, ctx, diagnostics|
    let Some(tag) = node.child_by_field_name("name") else { return };
    let tag_text = tag.utf8_text(source).ok().unwrap_or("");
    if tag_text != "script" {
        return;
    }

    let mut has_third_party_src = false;
    let mut has_defer_or_async = false;

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() != "jsx_attribute" {
            continue;
        }
        let Some(attr_name) = crate::rules::jsx::jsx_attribute_name(child, source) else {
            continue;
        };
        match attr_name {
            "src" => {
                if let Some(val) = crate::rules::jsx::jsx_attribute_value(child) {
                    let text = val.utf8_text(source).ok().unwrap_or("");
                    if is_third_party_src(text) {
                        has_third_party_src = true;
                    }
                }
            }
            "defer" | "async" => {
                has_defer_or_async = true;
            }
            _ => {}
        }
    }

    if has_third_party_src && !has_defer_or_async {
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: node.start_position().row + 1,
            column: node.start_position().column + 1,
            rule_id: super::META.id.into(),
            message: "Third-party `<script>` without `defer` or `async` blocks HTML parsing."
                .into(),
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
    use crate::diagnostic::Diagnostic;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.tsx")
    }

    #[test]
    fn flags_third_party_without_defer() {
        assert_eq!(
            run(r#"<script src="https://cdn.example.com/lib.js" />"#).len(),
            1
        );
    }

    #[test]
    fn flags_protocol_relative() {
        assert_eq!(run(r#"<script src="//cdn.example.com/lib.js" />"#).len(), 1);
    }

    #[test]
    fn allows_with_defer() {
        assert!(run(r#"<script src="https://cdn.example.com/lib.js" defer />"#).is_empty());
    }

    #[test]
    fn allows_with_async() {
        assert!(run(r#"<script src="https://cdn.example.com/lib.js" async />"#).is_empty());
    }

    #[test]
    fn allows_local_script() {
        assert!(run(r#"<script src="/js/app.js" />"#).is_empty());
    }

    #[test]
    fn allows_relative_script() {
        assert!(run(r#"<script src="./bundle.js" />"#).is_empty());
    }
}
