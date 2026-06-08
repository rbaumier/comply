use crate::diagnostic::{Diagnostic, Severity};

const NEEDLES: &[&str] = &[
    "::-webkit-",
    "::-moz-",
    "::-ms-",
    "::-o-",
    ":-webkit-",
    ":-moz-",
    ":-ms-",
    ":-o-",
];

crate::ast_check! { on ["pseudo_class_selector", "pseudo_element_selector"] => |node, source, ctx, diagnostics|
    let text = node.utf8_text(source).unwrap_or_default();
    if !NEEDLES.iter().any(|n| text.contains(n)) { return; }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        format!("Vendor-prefixed selector `{text}`; remove the prefix."),
        Severity::Warning,
    ));
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
    fn flags_webkit_input_placeholder() {
        assert_eq!(
            run("input::-webkit-input-placeholder { color: gray; }").len(),
            1
        );
    }

    #[test]
    fn flags_moz_focus_inner() {
        assert_eq!(run("button::-moz-focus-inner { border: 0; }").len(), 1);
    }

    #[test]
    fn allows_unprefixed_placeholder() {
        assert!(run("input::placeholder { color: gray; }").is_empty());
    }

    #[test]
    fn allows_focus_pseudo() {
        assert!(run("button:focus { outline: 2px solid blue; }").is_empty());
    }
}
