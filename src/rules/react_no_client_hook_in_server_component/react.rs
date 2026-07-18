//! react-no-client-hook-in-server-component backend.
//!
//! Flags `useFoo(...)` style call sites inside files classified as
//! `RscContext::ServerComponent`. React's `use()` primitive (lowercase, no
//! capital follow-up) is server-safe and slips through the `use[A-Z]` gate
//! intentionally.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::file_ctx::RscContext;

fn is_hook_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next() == Some('u')
        && chars.next() == Some('s')
        && chars.next() == Some('e')
        && chars.next().is_some_and(|c| c.is_ascii_uppercase())
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if ctx.file.rsc_context != RscContext::ServerComponent {
        return;
    }
    let Some(func) = node.child_by_field_name("function") else { return };
    if func.kind() != "identifier" {
        return;
    }
    let Ok(name) = func.utf8_text(source) else { return };
    if !is_hook_name(name) {
        return;
    }

    let pos = func.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "react-no-client-hook-in-server-component".into(),
        message: format!(
            "`{name}()` is a React hook and can't run in a server component. \
             Mark the file with `\"use client\"` or extract this into a \
             client component."
        ),
        severity: Severity::Error,
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
    use crate::rules::file_ctx::FileCtx;

    fn server_ctx() -> FileCtx {
        FileCtx {
            rsc_context: RscContext::ServerComponent,
            ..Default::default()
        }
    }

    fn client_ctx() -> FileCtx {
        FileCtx {
            rsc_context: RscContext::ClientComponent,
            ..Default::default()
        }
    }

    fn run(source: &str, file: &FileCtx) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule_with_ctx(&Check, source, "t.tsx", crate::project::default_static_project_ctx(), file)
    }

    #[test]
    fn flags_use_state_in_server_component() {
        let src = r#"
export default function Page() {
    const [count, setCount] = useState(0);
    return <div>{count}</div>;
}
"#;
        assert_eq!(run(src, &server_ctx()).len(), 1);
    }

    #[test]
    fn flags_use_router_in_server_component() {
        let src = r#"
export default function Page() {
    const router = useRouter();
    return <div />;
}
"#;
        assert_eq!(run(src, &server_ctx()).len(), 1);
    }

    #[test]
    fn allows_use_state_in_client_component() {
        let src = r#"
export default function Page() {
    const [count, setCount] = useState(0);
    return <div>{count}</div>;
}
"#;
        assert!(run(src, &client_ctx()).is_empty());
    }

    #[test]
    fn allows_plain_use_primitive_in_server_component() {
        // React 19 `use()` is server-safe — lowercase letter after "use"
        // means it's not a hook.
        let src = r#"
export default function Page() {
    const data = use(fetchData());
    return <div>{data}</div>;
}
"#;
        assert!(run(src, &server_ctx()).is_empty());
    }

    #[test]
    fn allows_non_hook_named_function_in_server_component() {
        // `user` and `useless` don't match the `use[A-Z]` pattern.
        let src = r#"
export default function Page() {
    const u = user();
    const v = useless();
    return <div />;
}
"#;
        assert!(run(src, &server_ctx()).is_empty());
    }

    #[test]
    fn ignores_unknown_rsc_context() {
        let src = r#"
export default function Page() {
    const [count, setCount] = useState(0);
    return <div>{count}</div>;
}
"#;
        assert!(run(src, &FileCtx::default()).is_empty());
    }

    /// End-to-end: build a real Next.js app-router project on disk, let
    /// `ProjectCtx::load` + `classify_rsc` decide the RSC context from the
    /// import graph, then run the rule against `target_rel`.
    fn run_in_next_app(files: &[(&str, &str)], target_rel: &str) -> Vec<Diagnostic> {
        use crate::config::Config;
        use crate::files::{Language, SourceFile};
        use crate::project::ProjectCtx;
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name":"t","dependencies":{"next":"14.0.0"}}"#,
        )
        .unwrap();
        let mut source_files: Vec<SourceFile> = Vec::new();
        for (rel, content) in files {
            let p = dir.path().join(rel);
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(&p, content).unwrap();
            let language = Language::from_path(&p).unwrap();
            source_files.push(SourceFile { path: p, language });
        }
        let refs: Vec<&SourceFile> = source_files.iter().collect();
        let project = ProjectCtx::load(&refs, &Config::default());
        let target = dir.path().join(target_rel);
        let source = std::fs::read_to_string(&target).unwrap();
        let language = Language::from_path(&target).unwrap();
        let file_ctx = FileCtx::build(&target, &source, language, &project);
        crate::rules::test_helpers::run_ast_check(&Check, &source, &target, &project, &file_ctx)
    }

    #[test]
    fn no_flag_use_state_in_hook_imported_by_client_component() {
        let diags = run_in_next_app(
            &[
                (
                    "app/hooks/use-x.ts",
                    "import { useState } from \"react\";\n\
                     export function useX() { const [s, setS] = useState(0); return s; }\n",
                ),
                (
                    "app/comp.tsx",
                    "\"use client\";\nimport { useX } from \"./hooks/use-x\";\n\
                     export function Comp() { return null; }\n",
                ),
            ],
            "app/hooks/use-x.ts",
        );
        assert!(diags.is_empty(), "hook below a client boundary must not be flagged: {diags:?}");
    }

    #[test]
    fn still_flags_use_state_in_true_server_component() {
        let diags = run_in_next_app(
            &[
                (
                    "app/page.tsx",
                    "import { useState } from \"react\";\n\
                     export default function Page() { const [s, setS] = useState(0); return <div>{s}</div>; }\n",
                ),
                (
                    "app/widget.tsx",
                    "\"use client\";\nexport function Widget() { return null; }\n",
                ),
            ],
            "app/page.tsx",
        );
        assert_eq!(diags.len(), 1, "true server entrypoint must still be flagged: {diags:?}");
    }
}
