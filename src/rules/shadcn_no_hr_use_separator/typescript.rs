//! Flag `<hr>` / `<hr />` JSX elements.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["jsx_opening_element", "jsx_self_closing_element"] => |node, source, ctx, diagnostics|    let Some(tag) = crate::rules::jsx::jsx_element_tag_name(node, source) else {
        return;
    };
    if tag != "hr" {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "Use the shadcn `<Separator />` component instead of a raw `<hr />`.".into(),
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
        crate::rules::test_helpers::run_rule(&Check, s, "t.tsx")
    }

    #[test]
    fn flags_self_closing_hr() {
        assert_eq!(run(r#"const x = <div><hr /></div>;"#).len(), 1);
    }

    #[test]
    fn flags_open_close_hr() {
        assert_eq!(run(r#"const x = <div><hr></hr></div>;"#).len(), 1);
    }

    #[test]
    fn allows_separator() {
        assert!(run(r#"const x = <div><Separator /></div>;"#).is_empty());
    }
}
