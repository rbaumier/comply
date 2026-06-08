//! OxcCheck backend for no-static-only-class.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::ClassElement;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::Class]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::Class(class) = node.kind() else { return };

        // Skip classes that extend a superclass
        if class.super_class.is_some() {
            return;
        }

        let mut member_count = 0u32;
        let mut all_static = true;

        for element in &class.body.body {
            match element {
                ClassElement::MethodDefinition(m) => {
                    member_count += 1;
                    if !m.r#static {
                        all_static = false;
                        break;
                    }
                }
                ClassElement::PropertyDefinition(p) => {
                    member_count += 1;
                    if !p.r#static {
                        all_static = false;
                        break;
                    }
                }
                _ => continue,
            }
        }

        if member_count == 0 || !all_static {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, class.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Use an object or plain functions instead of a class with only static members."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_static_only_methods() {
        let d = run_on("class Foo { static bar() {} static baz() {} }");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "no-static-only-class");
    }


    #[test]
    fn flags_static_only_fields() {
        let d = run_on("class Foo { static x = 1; static y = 2; }");
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn flags_mixed_static_methods_and_fields() {
        let d = run_on("class Foo { static x = 1; static bar() {} }");
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn allows_class_with_instance_member() {
        assert!(run_on("class Foo { static bar() {} baz() {} }").is_empty());
    }


    #[test]
    fn allows_class_with_only_instance_methods() {
        assert!(run_on("class Foo { bar() {} }").is_empty());
    }


    #[test]
    fn allows_class_extending_superclass() {
        assert!(run_on("class Foo extends Base { static bar() {} }").is_empty());
    }


    #[test]
    fn allows_empty_class() {
        assert!(run_on("class Foo {}").is_empty());
    }


    #[test]
    fn flags_class_expression() {
        let d = run_on("const Foo = class { static bar() {} };");
        assert_eq!(d.len(), 1);
    }
}
