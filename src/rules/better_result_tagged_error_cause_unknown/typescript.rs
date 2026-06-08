use crate::diagnostic::{Diagnostic, Severity};

fn extends_tagged_error(class_node: &tree_sitter::Node<'_>, source: &[u8]) -> bool {
    let mut cursor = class_node.walk();
    for child in class_node.children(&mut cursor) {
        if child.kind() == "class_heritage" {
            let text = child.utf8_text(source).unwrap_or("");
            if text.contains("TaggedError") {
                return true;
            }
        }
    }
    false
}

crate::ast_check! { on ["class_declaration"] prefilter = ["TaggedError"] => |node, source, ctx, diagnostics|
    if !extends_tagged_error(&node, source) {
        return;
    }
    let Some(body) = node.child_by_field_name("body") else { return; };
    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        if !matches!(
            child.kind(),
            "public_field_definition" | "property_signature" | "field_definition"
        ) {
            continue;
        }
        let Some(name) = child.child_by_field_name("name") else { continue; };
        if name.utf8_text(source).unwrap_or("") != "cause" {
            continue;
        }
        let Some(ty) = child.child_by_field_name("type") else { continue; };
        let ty_text = ty.utf8_text(source).unwrap_or("").trim().trim_start_matches(':').trim();
        if ty_text != "unknown" {
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &child,
                super::META.id,
                format!("cause field must be typed `unknown`, found `{ty_text}`."),
                Severity::Warning,
            ));
        }
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
        crate::rules::test_helpers::run_rule(&Check, s, "t.ts")
    }
    #[test]
    fn flags_cause_error_type() {
        let src = "class E extends TaggedError('E') { cause: Error = new Error(); }";
        assert_eq!(run(src).len(), 1);
    }
    #[test]
    fn allows_cause_unknown() {
        let src = "class E extends TaggedError('E') { cause: unknown = undefined; }";
        assert!(run(src).is_empty());
    }
}
