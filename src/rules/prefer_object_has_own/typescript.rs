use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] prefilter = ["hasOwnProperty"] => |node, source, ctx, diagnostics|
    let Some(func) = node.child_by_field_name("function") else { return; };
    if func.kind() != "member_expression" { return; }

    let Some(prop) = func.child_by_field_name("property") else { return; };
    let prop_name = prop.utf8_text(source).unwrap_or("");

    if prop_name != "hasOwnProperty" { return; }

    // Check it's not already Object.prototype.hasOwnProperty.call (allowed pattern)
    let Some(obj) = func.child_by_field_name("object") else { return; };
    let obj_text = obj.utf8_text(source).unwrap_or("");
    if obj_text == "Object.prototype" { return; }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "prefer-object-has-own".into(),
        message: "Use `Object.hasOwn(obj, key)` instead of `obj.hasOwnProperty(key)` (ES2022).".into(),
        severity: Severity::Warning,
        span: None,
    });
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
    fn run(code: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, code, "t.ts")
    }

    #[test]
    fn flags_has_own_property() {
        assert_eq!(run("obj.hasOwnProperty('key')").len(), 1);
    }

    #[test]
    fn flags_this_has_own_property() {
        assert_eq!(run("this.hasOwnProperty('key')").len(), 1);
    }

    #[test]
    fn allows_object_has_own() {
        assert!(run("Object.hasOwn(obj, 'key')").is_empty());
    }

    #[test]
    fn allows_prototype_call() {
        assert!(run("Object.prototype.hasOwnProperty.call(obj, 'key')").is_empty());
    }

    #[test]
    fn skips_protobuf_generated_file_issue1144() {
        // Issue #1144: Protobuf-compiled files cannot be hand-edited and emit
        // `.hasOwnProperty()` deliberately, disabling `no-prototype-builtins`
        // themselves. The generated-file detector must classify both the
        // `generated/` directory and the Protobuf eslint-disable header. The
        // engine skips every rule on generated files (engine/mod.rs), so a true
        // `is_generated` flag is what suppresses these findings — and the file
        // still produces `.hasOwnProperty()` diagnostics when run ungated,
        // proving the skip (not the check) is doing the work.
        use crate::files::Language;
        use crate::project::ProjectCtx;
        use crate::rules::file_ctx::FileCtx;
        let src = "/*eslint-disable block-scoped-var, id-length, no-control-regex, no-magic-numbers, no-prototype-builtins, no-redeclare, no-shadow, no-var, sort-vars*/\nimport * as $protobuf from \"protobufjs/minimal\";\nfunction f(o) { return o.hasOwnProperty('k'); }\n";
        let path = std::path::Path::new(
            "sdk/web-pubsub/web-pubsub-client-protobuf/src/generated/clientProto.js",
        );
        let file = FileCtx::build(path, src, Language::JavaScript, &ProjectCtx::empty());
        assert!(file.is_generated, "Protobuf generated file must be detected as generated");
        assert_eq!(
            crate::rules::test_helpers::run_rule(&Check, src, "clientProto.js").len(),
            1,
            "the check itself still fires; only the is_generated gate suppresses it"
        );
    }

    #[test]
    fn still_flags_hand_written_file_issue1144() {
        // A non-generated file using `.hasOwnProperty()` still flags.
        use crate::files::Language;
        use crate::project::ProjectCtx;
        use crate::rules::file_ctx::FileCtx;
        let src = "function f(o) { return o.hasOwnProperty('k'); }\n";
        let path = std::path::Path::new("src/utils.js");
        let file = FileCtx::build(path, src, Language::JavaScript, &ProjectCtx::empty());
        assert!(!file.is_generated, "a hand-written file must not be detected as generated");
        assert_eq!(crate::rules::test_helpers::run_rule(&Check, src, "src/utils.js").len(), 1);
    }
}
