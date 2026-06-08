use crate::diagnostic::{Diagnostic, Severity};

const DEPRECATED: &[(&str, &str, &str)] = &[
    ("overflow", "overlay", "use `auto`"),
    ("overflow-x", "overlay", "use `auto`"),
    ("overflow-y", "overlay", "use `auto`"),
    ("text-justify", "distribute", "use `inter-character`"),
    ("word-wrap", "break-word", "use `overflow-wrap: break-word`"),
    ("display", "run-in", "no longer supported"),
    ("display", "compact", "no longer supported"),
];

crate::ast_check! { on ["declaration"] => |node, source, ctx, diagnostics|
    let mut c = node.walk();
    let kids: Vec<_> = node.children(&mut c).collect();
    let Some(prop) = kids.iter().find(|n| n.kind() == "property_name") else { return; };
    let prop_name = prop.utf8_text(source).unwrap_or_default().to_ascii_lowercase();
    for value in kids.iter().filter(|n| n.kind() == "plain_value") {
        let val = value.utf8_text(source).unwrap_or_default().to_ascii_lowercase();
        for (p, v, hint) in DEPRECATED {
            if prop_name == *p && val == *v {
                diagnostics.push(Diagnostic::at_node(
                    ctx.path,
                    value,
                    super::META.id,
                    format!("`{p}: {v}` is deprecated; {hint}."),
                    Severity::Warning,
                ));
            }
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
    fn flags_overflow_overlay() {
        assert_eq!(run(".a { overflow: overlay; }").len(), 1);
    }

    #[test]
    fn allows_overflow_auto() {
        assert!(run(".a { overflow: auto; }").is_empty());
    }
}
