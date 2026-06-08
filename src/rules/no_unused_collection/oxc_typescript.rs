use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use oxc_ast::ast::{BindingPattern, Expression, VariableDeclarationKind};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

const COLLECTION_CTORS: &[&str] = &["Map", "Set", "Array", "WeakMap", "WeakSet"];
const WRITE_METHODS: &[&str] = &["push", "add", "set", "unshift", "splice"];

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let nodes = semantic.nodes();

        // Pass 1: collect collection declarations (name, declaration span start).
        let mut collections: Vec<(&str, u32)> = Vec::new();

        for node in nodes.iter() {
            let AstKind::VariableDeclarator(decl) = node.kind() else {
                continue;
            };
            let parent_id = nodes.parent_id(node.id());
            let AstKind::VariableDeclaration(var_decl) = nodes.kind(parent_id) else {
                continue;
            };
            if var_decl.kind != VariableDeclarationKind::Const {
                continue;
            }
            let BindingPattern::BindingIdentifier(id) = &decl.id else {
                continue;
            };
            let Some(init) = &decl.init else { continue };
            let is_collection = match init.without_parentheses() {
                Expression::ArrayExpression(_) => true,
                Expression::NewExpression(new_expr) => {
                    if let Expression::Identifier(ctor) = &new_expr.callee {
                        COLLECTION_CTORS.contains(&ctor.name.as_str())
                    } else {
                        false
                    }
                }
                _ => false,
            };
            if is_collection {
                collections.push((id.name.as_str(), id.span.start));
            }
        }

        if collections.is_empty() {
            return diagnostics;
        }

        // Pass 2: for each collection, classify all identifier references as
        // write (mutation method call) or read (anything else).
        for &(name, decl_start) in &collections {
            let mut is_written = false;
            let mut is_read = false;

            for node in nodes.iter() {
                let AstKind::IdentifierReference(id_ref) = node.kind() else {
                    continue;
                };
                if id_ref.name.as_str() != name {
                    continue;
                }
                if id_ref.span.start == decl_start {
                    continue;
                }
                // Check if this is a write: parent is StaticMemberExpression
                // that is callee of a CallExpression with a write method.
                let parent_id = nodes.parent_id(node.id());
                let is_write =
                    if let AstKind::StaticMemberExpression(member) = nodes.kind(parent_id) {
                        // Identifier must be the object, not the property.
                        let is_object = member.object.span().start == id_ref.span.start;
                        if is_object && WRITE_METHODS.contains(&member.property.name.as_str()) {
                            let grandparent_id = nodes.parent_id(parent_id);
                            matches!(nodes.kind(grandparent_id), AstKind::CallExpression(_))
                        } else {
                            false
                        }
                    } else {
                        false
                    };

                if is_write {
                    is_written = true;
                } else {
                    is_read = true;
                }
            }

            if is_written && !is_read {
                let (line, column) = byte_offset_to_line_col(ctx.source, decl_start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!("Collection `{name}` is populated but never read."),
                    severity: super::META.severity,
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_pushed_but_never_read() {
        let src = r#"
const items = [];
items.push(1);
items.push(2);
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_set_add_but_never_read() {
        let src = r#"
const seen = new Set();
seen.add("a");
seen.add("b");
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_pushed_and_iterated() {
        let src = r#"
const items = [];
items.push(1);
items.forEach(x => console.log(x));
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_pushed_and_returned() {
        let src = r#"
const items = [];
items.push(1);
return items;
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_collection_passed_as_arg() {
        let src = r#"
const items = [];
items.push(1);
doSomething(items);
"#;
        assert!(run_on(src).is_empty());
    }
}
