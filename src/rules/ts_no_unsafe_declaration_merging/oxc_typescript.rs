//! ts-no-unsafe-declaration-merging OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use oxc_semantic::ScopeId;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [oxc_ast::AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        // (name, byte offset, lexical scope the declaration sits in). TypeScript
        // declaration merging only happens when both declarations live in the
        // *same* lexical scope, so the scope id is part of the match key.
        let mut class_decls: Vec<(&str, u32, ScopeId)> = Vec::new();
        let mut interface_decls: Vec<(&str, u32, ScopeId)> = Vec::new();

        for node in semantic.nodes().iter() {
            match node.kind() {
                AstKind::Class(class) => {
                    if let Some(id) = &class.id {
                        class_decls.push((id.name.as_str(), id.span.start, node.scope_id()));
                    }
                }
                AstKind::TSInterfaceDeclaration(decl) => {
                    // `declare interface Foo extends Bar {}` is an ambient,
                    // type-only augmentation with no runtime footprint, so it
                    // cannot conflict with a class's method implementations
                    // (the Stencil/Angular wrapper pattern). Skip it.
                    if decl.declare {
                        continue;
                    }
                    interface_decls.push((
                        decl.id.name.as_str(),
                        decl.id.span.start,
                        node.scope_id(),
                    ));
                }
                _ => {}
            }
        }

        let mut diagnostics = Vec::new();

        // Flag interfaces that merge with a same-named class in the same scope.
        for (iface_name, offset, iface_scope) in &interface_decls {
            if class_decls
                .iter()
                .any(|(c, _, c_scope)| c == iface_name && c_scope == iface_scope)
            {
                let (line, column) = byte_offset_to_line_col(ctx.source, *offset as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "Unsafe declaration merging — interface `{iface_name}` \
                         shares a name with a class."
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }

        // Flag classes that merge with a same-named interface in the same scope.
        for (class_name, offset, class_scope) in &class_decls {
            if interface_decls
                .iter()
                .any(|(i, _, i_scope)| i == class_name && i_scope == class_scope)
            {
                let (line, column) = byte_offset_to_line_col(ctx.source, *offset as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "Unsafe declaration merging — class `{class_name}` \
                         shares a name with an interface."
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
mod tests {
    fn run(source: &str) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_rule_by_id(
            "ts-no-unsafe-declaration-merging",
            source,
            "t.ts",
        )
    }

    #[test]
    fn flags_class_and_interface_same_name() {
        // One diagnostic for the interface, one for the class.
        assert_eq!(run("interface Foo {} class Foo {}").len(), 2);
    }

    #[test]
    fn allows_different_names() {
        assert!(run("interface Foo {} class Bar {}").is_empty());
    }

    // Regression for #1835: `declare interface` ambient augmentations merged
    // with a class (the Stencil/Angular wrapper pattern) are type-only and
    // carry no runtime footprint, so they must not be flagged.
    #[test]
    fn allows_declare_interface_merged_with_class() {
        let source = "\
class IonInputOtp extends ValueAccessor {}
export declare interface IonInputOtp extends Components.IonInputOtp {
  ionInput: EventEmitter<CustomEvent<InputInputEventDetail>>;
}";
        assert!(run(source).is_empty());
    }

    // Regression for #5291: a class and a same-named interface declared in
    // *different* function scopes (here, two separate test callbacks) cannot
    // merge at the TypeScript level, so neither must be flagged.
    #[test]
    fn allows_class_and_interface_in_different_function_scopes() {
        let source = "\
it('should support getters', () => {
  class A {
    get a() { return 'a' }
    get b() { return 'b' }
  }
  return A;
});
describe('lazy', () => {
  interface A {
    a: number
    b?: A
  }
});";
        assert!(run(source).is_empty());
    }

    // A class and an interface sharing a name inside the *same* block scope do
    // merge, so both stay flagged.
    #[test]
    fn flags_class_and_interface_in_same_block_scope() {
        let source = "\
function f() {
  class A {}
  interface A { a: number }
}";
        assert_eq!(run(source).len(), 2);
    }
}
