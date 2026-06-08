//! no-unsupported-node-builtins oxc backend — compare each Node.js API usage
//! against the minimum Node version declared in the nearest `package.json`'s
//! `engines.node` field.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

use super::{lookup_global, lookup_instance_method, lookup_static_method, min_node_major};

pub struct Check;

/// True if the identifier is in a declaration position (variable name, param
/// name, function/class name). Prevents "shim" declarations from tripping
/// the rule on themselves.
fn is_declaration_name<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let parent_id = semantic.nodes().parent_id(node.id());
    if node.id() == parent_id {
        return false;
    }
    let parent = semantic.nodes().get_node(parent_id);
    matches!(
        parent.kind(),
        AstKind::VariableDeclarator(_)
            | AstKind::Function(_)
            | AstKind::Class(_)
            | AstKind::MethodDefinition(_)
            | AstKind::FormalParameter(_)
            | AstKind::FormalParameters(_)
            | AstKind::ArrowFunctionExpression(_)
            | AstKind::BindingRestElement(_)
            | AstKind::LabeledStatement(_)
    )
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let Some(min_version) = min_node_major(ctx) else {
            return Vec::new();
        };

        let mut diagnostics = Vec::new();

        for node in semantic.nodes().iter() {
            match node.kind() {
                AstKind::IdentifierReference(ident) => {
                    if is_declaration_name(node, semantic) {
                        continue;
                    }
                    let text = ident.name.as_str();
                    if let Some(required) = lookup_global(text).filter(|&r| r > min_version) {
                        let (line, column) =
                            byte_offset_to_line_col(ctx.source, ident.span.start as usize);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: super::META.id.into(),
                            message: format!(
                                "`{text}` is not available in Node.js {min_version}; requires Node.js {required} or later."
                            ),
                            severity: Severity::Warning,
                            span: None,
                        });
                    }
                }
                AstKind::StaticMemberExpression(mem) => {
                    let prop_text = mem.property.name.as_str();

                    // Static method on `Object` / `Array`.
                    if let oxc_ast::ast::Expression::Identifier(obj) = &mem.object {
                        let obj_text = obj.name.as_str();
                        if let Some(required) =
                            lookup_static_method(obj_text, prop_text).filter(|&r| r > min_version)
                        {
                            let (line, column) =
                                byte_offset_to_line_col(ctx.source, mem.span.start as usize);
                            diagnostics.push(Diagnostic {
                                path: Arc::clone(&ctx.path_arc),
                                line,
                                column,
                                rule_id: super::META.id.into(),
                                message: format!(
                                    "`{obj_text}.{prop_text}` is not available in Node.js {min_version}; requires Node.js {required} or later."
                                ),
                                severity: Severity::Warning,
                                span: None,
                            });
                            continue;
                        }
                    }

                    // Instance method — flagged regardless of receiver shape.
                    if let Some(required) =
                        lookup_instance_method(prop_text).filter(|&r| r > min_version)
                    {
                        let (line, column) =
                            byte_offset_to_line_col(ctx.source, mem.property.span.start as usize);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: super::META.id.into(),
                            message: format!(
                                "`.{prop_text}()` is not available in Node.js {min_version}; requires Node.js {required} or later."
                            ),
                            severity: Severity::Warning,
                            span: None,
                        });
                    }
                }
                _ => {}
            }
        }

        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::diagnostic::Diagnostic;
    use crate::files::{Language, SourceFile};
    use crate::project::ProjectCtx;
    use crate::rules::test_helpers::run_oxc_ts_with_project;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;



    fn setup_with_engine(node_version: &str, source: &str) -> Vec<Diagnostic> {
        let dir = TempDir::new().unwrap();
        let pkg =
            format!(r#"{{"name":"t","version":"0.0.0","engines":{{"node":"{node_version}"}}}}"#);
        fs::write(dir.path().join("package.json"), pkg).unwrap();
        let src_path = dir.path().join("app.ts");
        fs::write(&src_path, source).unwrap();
        let src_path = fs::canonicalize(&src_path).unwrap();

        let source_file = SourceFile {
            path: src_path.clone(),
            language: Language::TypeScript,
        };
        let refs: Vec<&SourceFile> = vec![&source_file];
        let config = Config::default();
        let project = ProjectCtx::load(&refs, &config);

        run_oxc_ts_with_project(source, &Check, &project)
    }


    fn setup_without_engine(source: &str) -> Vec<Diagnostic> {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{"name":"t","version":"0.0.0"}"#,
        )
        .unwrap();
        let src_path = dir.path().join("app.ts");
        fs::write(&src_path, source).unwrap();
        let src_path = fs::canonicalize(&src_path).unwrap();

        let source_file = SourceFile {
            path: src_path.clone(),
            language: Language::TypeScript,
        };
        let refs: Vec<&SourceFile> = vec![&source_file];
        let config = Config::default();
        let project = ProjectCtx::load(&refs, &config);

        run_oxc_ts_with_project(source, &Check, &project)
    }


    fn setup_without_package_json(source: &str) -> Vec<Diagnostic> {
        let dir = TempDir::new().unwrap();
        let src_path = dir.path().join("app.ts");
        fs::write(&src_path, source).unwrap();
        let src_path: PathBuf = fs::canonicalize(&src_path).unwrap();

        let source_file = SourceFile {
            path: src_path.clone(),
            language: Language::TypeScript,
        };
        let refs: Vec<&SourceFile> = vec![&source_file];
        let config = Config::default();
        let project = ProjectCtx::load(&refs, &config);

        run_oxc_ts_with_project(source, &Check, &project)
    }


    #[test]
    fn allows_fetch_at_18() {
        let d = setup_with_engine(">=18", "const res = fetch('http://example.com');");
        assert!(d.is_empty());
    }


    #[test]
    fn allows_structured_clone_at_17() {
        let d = setup_with_engine(">=17", "const copy = structuredClone(obj);");
        assert!(d.is_empty());
    }


    #[test]
    fn skips_no_engine_field() {
        let d = setup_without_engine("const copy = structuredClone(obj);");
        assert!(d.is_empty());
    }


    #[test]
    fn skips_no_package_json() {
        let d = setup_without_package_json("const copy = structuredClone(obj);");
        assert!(d.is_empty());
    }


    #[test]
    fn allows_object_group_by_at_21() {
        let d = setup_with_engine(">=21", "Object.groupBy(arr, fn);");
        assert!(d.is_empty());
    }


    #[test]
    fn allows_older_apis() {
        let d = setup_with_engine(">=16", "setTimeout(() => {}, 1000); arr.map(x => x);");
        assert!(d.is_empty());
    }


    #[test]
    fn does_not_flag_member_property_named_fetch() {
        let d = setup_with_engine(">=16", "obj.fetch();");
        assert!(d.is_empty());
    }


    #[test]
    fn parses_caret_range() {
        let d = setup_with_engine("^18.0.0", "const res = fetch('u');");
        assert!(d.is_empty());
    }
}
