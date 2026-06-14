//! react-hook-form-use-no-memo oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use oxc_ast::AstKind;
use oxc_ast::ast::Expression;
use oxc_semantic::Semantic;
use std::sync::Arc;

/// React-Compiler detection memoized per source file. The project-level answer
/// (`ProjectCtx::uses_react_compiler`) takes a per-directory `Mutex`; the
/// lock-free file slot caches it so this file pays the lock at most once.
fn project_uses_react_compiler(ctx: &CheckCtx) -> bool {
    crate::oxc_helpers::cached_file_bool(
        ctx.source,
        crate::oxc_helpers::SLOT_REACT_COMPILER,
        || ctx.project.uses_react_compiler(ctx.path),
    )
}

pub struct Check;

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["useForm"])
    }

    fn run_on_semantic<'a>(&self, semantic: &'a Semantic<'a>, ctx: &CheckCtx) -> Vec<Diagnostic> {
        // The `"use no memo"` directive only matters when the React Compiler is
        // transforming code. Without it the directive is a no-op, so a project
        // that hasn't opted into the compiler needs no diagnostic at all.
        if !project_uses_react_compiler(ctx) {
            return Vec::new();
        }

        // The proxy-memoization concern is React-Hook-Form-specific. A `useForm`
        // imported from another library (e.g. `@tanstack/react-form`) uses a
        // different state model, so this rule must not fire on it.
        if crate::oxc_helpers::local_binding_imported_from_foreign_package(semantic, "useForm") {
            return Vec::new();
        }

        // A `"use no memo"` directive anywhere — file-level or inside the
        // component body — satisfies the convention.
        let has_use_no_memo = semantic.nodes().iter().any(|n| {
            let directives = match n.kind() {
                AstKind::Program(p) => &p.directives,
                AstKind::Function(f) => match f.body.as_ref() {
                    Some(b) => &b.directives,
                    None => return false,
                },
                AstKind::ArrowFunctionExpression(a) => &a.body.directives,
                _ => return false,
            };
            directives.iter().any(|d| d.expression.value == "use no memo")
        });

        if has_use_no_memo {
            return Vec::new();
        }

        // Find the first bare `useForm(...)` call. Renamed/member forms
        // (`useFormContext`, `methods.useForm`) are intentionally not matched.
        let useform_span = semantic.nodes().iter().find_map(|n| {
            let AstKind::CallExpression(call) = n.kind() else { return None };
            let Expression::Identifier(id) = &call.callee else { return None };
            (id.name.as_str() == "useForm").then_some(call.span.start)
        });

        let Some(span_start) = useform_span else { return Vec::new() };

        let (line, column) = byte_offset_to_line_col(ctx.source, span_start as usize);
        vec![Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "This file calls `useForm` but has no `\"use no memo\"` directive. The \
                      React Compiler memoizes the form proxy incorrectly \u{2014} add \
                      `\"use no memo\"` to opt this file out."
                .into(),
            severity: Severity::Warning,
            span: None,
        }]
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
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::files::{Language, SourceFile};
    use crate::project::ProjectCtx;
    use oxc_allocator::Allocator;
    use oxc_parser::Parser as OxcParser;
    use oxc_semantic::SemanticBuilder;
    use oxc_span::SourceType;
    use std::fs;
    use tempfile::TempDir;

    /// Run the rule inside a real project rooted at `dir`, so React-Compiler
    /// detection (package.json / bundler config) reflects the fixture.
    fn run_in_project(dir: &std::path::Path, file_rel: &str, source: &str) -> Vec<Diagnostic> {
        let file_path = dir.join(file_rel);
        fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        fs::write(&file_path, source).unwrap();
        let source_file = SourceFile {
            path: file_path.clone(),
            language: Language::from_path(&file_path).unwrap(),
        };
        let refs = vec![&source_file];
        let config = Config::default();
        let project = ProjectCtx::load(&refs, &config);
        let canon = fs::canonicalize(&file_path).unwrap();

        let allocator = Allocator::default();
        let parse_ret = OxcParser::new(&allocator, source, SourceType::tsx()).parse();
        let semantic = SemanticBuilder::new().build(&parse_ret.program).semantic;
        let ctx = CheckCtx::for_test_with_project(&canon, source, &project);
        Check.run_on_semantic(&semantic, &ctx)
    }

    /// A fixture directory whose `package.json` declares the React Compiler, so
    /// the rule's compiler gate is open and the convention applies.
    fn compiler_project() -> TempDir {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{"devDependencies":{"babel-plugin-react-compiler":"^1.0.0"}}"#,
        )
        .unwrap();
        dir
    }

    #[test]
    fn flags_useform_without_directive() {
        let dir = compiler_project();
        let src = r#"
            export function EditForm() {
              const form = useForm();
              return <form />;
            }
        "#;
        assert_eq!(run_in_project(dir.path(), "src/edit-form.tsx", src).len(), 1);
    }

    #[test]
    fn allows_useform_with_file_directive() {
        let dir = compiler_project();
        let src = r#"
            "use no memo";
            export function EditForm() {
              const form = useForm();
              return <form />;
            }
        "#;
        assert!(run_in_project(dir.path(), "src/edit-form.tsx", src).is_empty());
    }

    #[test]
    fn allows_useform_with_body_directive() {
        let dir = compiler_project();
        let src = r#"
            export function EditForm() {
              "use no memo";
              const form = useForm();
              return <form />;
            }
        "#;
        assert!(run_in_project(dir.path(), "src/edit-form.tsx", src).is_empty());
    }

    #[test]
    fn ignores_use_form_context() {
        // `useFormContext` consumes an existing form; it needs no opt-out.
        let dir = compiler_project();
        let src = r#"
            export function Field() {
              const { register } = useFormContext();
              return <input {...register("x")} />;
            }
        "#;
        assert!(run_in_project(dir.path(), "src/field.tsx", src).is_empty());
    }

    #[test]
    fn ignores_file_without_useform() {
        let dir = compiler_project();
        let src = r#"export function Plain() { return <div />; }"#;
        assert!(run_in_project(dir.path(), "src/plain.tsx", src).is_empty());
    }

    #[test]
    fn regression_amadeo_use_form() {
        // amadeo runs the React Compiler; every `useForm` file carries
        // `"use no memo"`. A file missing it is a defect.
        let dir = compiler_project();
        let src = r#"
            export function CreateClientDialog() {
              const form = useForm({ resolver: zodResolver(schema) });
              return <form onSubmit={form.handleSubmit(onSubmit)} />;
            }
        "#;
        assert_eq!(run_in_project(dir.path(), "src/dialog.tsx", src).len(), 1);
    }

    #[test]
    fn ignores_tanstack_react_form_useform() {
        // Regression for rbaumier/comply#1594 — `@tanstack/react-form`'s
        // `useForm` uses a different state model and is unaffected by the React
        // Compiler proxy concern; this RHF rule must not fire on it.
        let dir = compiler_project();
        let src = r#"
            import { useForm } from '@tanstack/react-form';
            export default function App() {
              const form = useForm({
                defaultValues: { firstName: '', lastName: '' },
                onSubmit: async ({ value }) => { console.log(value); },
              });
              return <form />;
            }
        "#;
        assert!(run_in_project(dir.path(), "src/index.tsx", src).is_empty());
    }

    #[test]
    fn flags_react_hook_form_useform_with_import() {
        // Negative space: a genuine react-hook-form `useForm` under the compiler
        // still needs the directive.
        let dir = compiler_project();
        let src = r#"
            import { useForm } from 'react-hook-form';
            export function EditForm() {
              const form = useForm();
              return <form />;
            }
        "#;
        assert_eq!(run_in_project(dir.path(), "src/edit-form.tsx", src).len(), 1);
    }

    #[test]
    fn skips_when_project_has_no_react_compiler() {
        // Regression: issue #1761 — bulletproof-react calls `useForm` but has no
        // React Compiler dependency, so `"use no memo"` is a no-op and the rule
        // must stay silent.
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{"dependencies":{"react":"^19","react-hook-form":"^7.54.2"},"devDependencies":{"vite":"^6.1.1"}}"#,
        )
        .unwrap();
        let src = r#"
            const Form = ({ onSubmit, children, options, id, schema }) => {
              const form = useForm({ ...options, resolver: zodResolver(schema) });
              return (
                <FormProvider {...form}>
                  <form onSubmit={form.handleSubmit(onSubmit)} id={id}>
                    {children(form)}
                  </form>
                </FormProvider>
              );
            };
        "#;
        let d = run_in_project(dir.path(), "apps/react-vite/src/components/ui/form/form.tsx", src);
        assert!(d.is_empty(), "no react-compiler dep: rule must stay silent: {d:?}");
    }
}
