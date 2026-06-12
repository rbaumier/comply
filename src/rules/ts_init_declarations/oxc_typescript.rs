//! ts-init-declarations OXC backend — flag `let`/`var` declarations
//! without an initializer, skipping `declare`, `const`, and bindings that are
//! assigned later (deferred-assignment patterns such as try/catch or if/else,
//! which TypeScript's definite-assignment analysis already validates).

use std::sync::Arc;

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use oxc_ast::ast::VariableDeclarationKind;
use oxc_semantic::ReferenceFlags;

pub struct Check;

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for node in semantic.nodes().iter() {
            let AstKind::VariableDeclaration(decl) = node.kind() else {
                continue;
            };
            // Skip `const` — TS/JS already errors on uninitialized const.
            if decl.kind == VariableDeclarationKind::Const {
                continue;
            }
            // Skip `declare` contexts, including `var` inside `declare global`
            // / `declare module` blocks, which are ambient type-level bindings.
            if decl.declare
                || crate::oxc_helpers::is_in_ambient_declaration(node.id(), semantic)
            {
                continue;
            }
            // Skip test files — uninitialized fixtures at `describe` scope
            // assigned in beforeAll/beforeEach are idiomatic and unavoidable.
            if ctx.file.path_segments.in_test_dir {
                return Vec::new();
            }
            for declarator in &decl.declarations {
                if declarator.init.is_some() {
                    continue;
                }
                let name = match &declarator.id {
                    oxc_ast::ast::BindingPattern::BindingIdentifier(ident) => {
                        // Skip bindings assigned later (deferred-assignment):
                        // a write reference means the value is set in a
                        // subsequent statement — try/catch, if/else, switch —
                        // which TypeScript's definite-assignment analysis
                        // verifies on all paths before use. Only declarations
                        // that are never assigned remain a genuine smell.
                        if let Some(symbol_id) = ident.symbol_id.get()
                            && semantic.symbol_references(symbol_id).any(|reference| {
                                reference.flags().contains(ReferenceFlags::Write)
                            })
                        {
                            continue;
                        }
                        ident.name.as_str()
                    }
                    _ => "variable",
                };
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, declarator.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "`{name}` is declared without initialization — \
                         assign a value at declaration."
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        diagnostics
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
    use crate::rules::file_ctx::{FileCtx, PathSegments};

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.tsx")
    }

    fn run_in_test_file(src: &str) -> Vec<Diagnostic> {
        let file = FileCtx {
            path_segments: PathSegments { in_test_dir: true, ..PathSegments::default() },
            ..FileCtx::default()
        };
        crate::rules::test_helpers::run_rule_with_ctx(&Check, src, "t.tsx", crate::project::default_static_project_ctx(), &file)
    }

    #[test]
    fn no_fp_on_test_fixture_beforeall() {
        let src = r#"
describe('example', () => {
  let user: User;
  beforeAll(async () => { user = await createUser(); });
  test('should work', () => { expect(user.name).toBe('test'); });
});
"#;
        assert!(run_in_test_file(src).is_empty());
    }

    #[test]
    fn no_fp_on_var_in_declare_global() {
        // `var` inside `declare global` is an ambient type-level binding —
        // it never has an initializer and must not be flagged. (Closes #339)
        assert!(
            run("declare global {\n  var BASE_UI_ANIMATIONS_DISABLED: boolean;\n}\nexport {};")
                .is_empty()
        );
    }

    #[test]
    fn still_flags_uninitialized_let_at_runtime() {
        assert_eq!(run("let x: number;").len(), 1);
    }

    #[test]
    fn no_fp_on_try_catch_assignment() {
        // `let` declared uninitialized, then assigned in a `try` block — moving
        // the declaration inside `try` would scope it out of later use. (Closes #1107)
        let src = r#"
function f(tag: string, packageDir: string) {
  let modifiedFiles;
  try {
    modifiedFiles = getModifiedFilesSinceTag(tag, packageDir);
  } catch (err) {
    return;
  }
  return modifiedFiles;
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_on_conditional_branch_assignment() {
        // Assigned on all branches of an if/else — TS definite-assignment
        // analysis validates this; annotating `| undefined` would defeat it. (Closes #1107)
        let src = r#"
function f(storageAccount: string, containerName: string, credential: unknown) {
  let containerClient: ContainerClient;
  if (process.env.AZURE_CONTAINER_SAS_URL) {
    containerClient = new ContainerClient(process.env.AZURE_CONTAINER_SAS_URL);
  } else {
    containerClient = new ContainerClient(storageAccount, containerName, credential);
  }
  return containerClient;
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_let_never_assigned() {
        // Declared uninitialized and never written anywhere — genuine smell.
        let src = r#"
function g() {
  let neverAssigned: number;
  return 1;
}
"#;
        assert_eq!(run(src).len(), 1);
    }
}
