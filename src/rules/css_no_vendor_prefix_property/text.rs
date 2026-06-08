use crate::diagnostic::{Diagnostic, Severity};

const PREFIXES: &[&str] = &["-webkit-", "-moz-", "-ms-", "-o-"];

crate::ast_check! { on ["declaration"] prefilter = ["-webkit-", "-moz-", "-ms-", "-o-"] => |node, source, ctx, diagnostics|
    let mut c = node.walk();
    let Some(prop) = node.children(&mut c).find(|n| n.kind() == "property_name") else { return; };
    let name = prop.utf8_text(source).unwrap_or_default();
    if !PREFIXES.iter().any(|p| name.starts_with(p)) { return; }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &prop,
        super::META.id,
        format!("Vendor-prefixed property `{name}`; remove the prefix and rely on autoprefixer."),
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
    fn flags_webkit_transform() {
        assert_eq!(run(".a { -webkit-transform: rotate(45deg); }").len(), 1);
    }

    #[test]
    fn flags_moz_user_select() {
        assert_eq!(run(".a { -moz-user-select: none; }").len(), 1);
    }

    #[test]
    fn allows_unprefixed_transform() {
        assert!(run(".a { transform: rotate(45deg); }").is_empty());
    }
}
