use crate::diagnostic::{Diagnostic, Severity};

const PREFIXES: &[&str] = &["@-webkit-", "@-moz-", "@-ms-", "@-o-"];

crate::ast_check! { on ["at_rule", "keyframes_statement"] prefilter = ["@-webkit-", "@-moz-", "@-ms-", "@-o-"] => |node, source, ctx, diagnostics|
    let mut c = node.walk();
    let Some(kw) = node.children(&mut c).find(|n| n.kind() == "at_keyword") else { return; };
    let text = kw.utf8_text(source).unwrap_or_default();
    if !PREFIXES.iter().any(|p| text.starts_with(p)) { return; }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &kw,
        super::META.id,
        format!("Vendor-prefixed at-rule `{text}`; remove the prefix and rely on autoprefixer."),
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
    fn flags_webkit_keyframes() {
        let css = "@-webkit-keyframes slide { from { left: 0; } to { left: 100px; } }";
        assert_eq!(run(css).len(), 1);
    }

    #[test]
    fn flags_moz_document() {
        assert_eq!(
            run("@-moz-document url-prefix() { .a { color: red; } }").len(),
            1
        );
    }

    #[test]
    fn allows_unprefixed_keyframes() {
        let css = "@keyframes slide { from { left: 0; } to { left: 100px; } }";
        assert!(run(css).is_empty());
    }
}
