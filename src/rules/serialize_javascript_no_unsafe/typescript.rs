//! serialize-javascript-no-unsafe backend.
//!
//! Walks every `call_expression`. If the callee is `serialize` and the
//! second argument is an object literal containing `unsafe: true`, flag
//! it — the `unsafe` option disables HTML escaping in
//! `serialize-javascript` and exposes consumers to XSS when the
//! serialized value is embedded in HTML.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] prefilter = ["serialize"] => |node, source, ctx, diagnostics|
    let Some(name) = crate::rules::call_expression::call_function_name(node, source) else {
        return;
    };
    if name != "serialize" { return; }

    let Some(args) = node.child_by_field_name("arguments") else { return; };
    if args.named_child_count() < 2 { return; }
    let Some(options) = args.named_child(1) else { return; };
    if options.kind() != "object" { return; }

    for i in 0..options.named_child_count() {
        let Some(pair) = options.named_child(i) else { continue; };
        if pair.kind() != "pair" { continue; }

        let Some(key) = pair.child_by_field_name("key") else { continue; };
        let key_text = key
            .utf8_text(source)
            .unwrap_or("")
            .trim_matches(|c: char| c == '\'' || c == '"');
        if key_text != "unsafe" { continue; }

        let Some(value) = pair.child_by_field_name("value") else { continue; };
        if value.utf8_text(source).unwrap_or("").trim() != "true" { continue; }

        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &pair,
            super::META.id,
            "`serialize(..., { unsafe: true })` disables HTML escaping — remove the `unsafe` option.".into(),
            Severity::Error,
        ));
        return;
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
    use super::Check;
    use crate::diagnostic::Diagnostic;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.ts")
    }

    #[test]
    fn flags_unsafe_true() {
        assert_eq!(run("serialize(data, { unsafe: true })").len(), 1);
    }

    #[test]
    fn flags_unsafe_true_quoted_key() {
        assert_eq!(run(r#"serialize(data, { "unsafe": true })"#).len(), 1);
    }

    #[test]
    fn allows_unsafe_false() {
        assert!(run("serialize(data, { unsafe: false })").is_empty());
    }

    #[test]
    fn allows_no_options() {
        assert!(run("serialize(data)").is_empty());
    }

    #[test]
    fn allows_other_options() {
        assert!(run("serialize(data, { isJSON: true })").is_empty());
    }

    #[test]
    fn ignores_non_serialize_call() {
        assert!(run("stringify(data, { unsafe: true })").is_empty());
    }
}
