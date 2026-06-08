use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["block"] => |node, source, ctx, diagnostics|
    let mut c = node.walk();
    let decls: Vec<_> = node
        .children(&mut c)
        .filter(|n| n.kind() == "declaration")
        .collect();
    let mut seen: Vec<String> = Vec::new();
    for decl in &decls {
        let mut dc = decl.walk();
        let Some(prop) = decl.children(&mut dc).find(|n| n.kind() == "property_name") else { continue; };
        let name = prop.utf8_text(source).unwrap_or_default().to_string();
        if !name.starts_with("--") { continue; }
        if seen.iter().any(|n| n == &name) {
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                decl,
                super::META.id,
                format!("Duplicate custom property `{name}`."),
                Severity::Warning,
            ));
        } else {
            seen.push(name);
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
        crate::rules::test_helpers::run_rule(&Check, s, "t.css")
    }

    #[test]
    fn flags_duplicate_custom_property() {
        let css = ".a { --color: red; --size: 14px; --color: blue; }";
        assert_eq!(run(css).len(), 1);
    }

    #[test]
    fn allows_distinct_custom_properties() {
        let css = ".a { --color: red; --size: 14px; }";
        assert!(run(css).is_empty());
    }
}
