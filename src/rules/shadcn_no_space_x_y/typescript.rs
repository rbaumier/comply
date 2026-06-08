//! Flag any `space-x-*` / `space-y-*` utility inside a JSX `className`.

use crate::diagnostic::{Diagnostic, Severity};

fn is_space_class(class: &str) -> bool {
    // Allow a `hover:` / `md:` / `dark:` etc. variant prefix.
    let utility = class.rsplit(':').next().unwrap_or(class);
    let utility = utility.trim_start_matches('!');
    utility.starts_with("space-x-") || utility.starts_with("space-y-")
}

crate::ast_check! { on ["jsx_attribute"] => |node, source, ctx, diagnostics|
    if crate::rules::jsx::jsx_attribute_name(node, source) != Some("className") {
        return;
    }
    let Some(value) = crate::rules::jsx::jsx_attribute_string_value(node, source) else {
        return;
    };
    if value.split_ascii_whitespace().any(is_space_class) {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            super::META.id,
            "`space-x-*` / `space-y-*` are fragile — use `flex gap-*` (or `flex flex-col gap-*`) instead.".into(),
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
        crate::rules::test_helpers::run_rule(&Check, s, "t.tsx")
    }

    #[test]
    fn flags_space_x() {
        assert_eq!(
            run(r#"const x = <div className="space-x-2">x</div>;"#).len(),
            1
        );
    }

    #[test]
    fn flags_space_y_with_other_classes() {
        assert_eq!(
            run(r#"const x = <div className="p-4 space-y-4 items-start">x</div>;"#).len(),
            1
        );
    }

    #[test]
    fn allows_flex_gap() {
        assert!(run(r#"const x = <div className="flex gap-2">x</div>;"#).is_empty());
    }

    #[test]
    fn allows_no_classname() {
        assert!(run(r#"const x = <div>x</div>;"#).is_empty());
    }
}
