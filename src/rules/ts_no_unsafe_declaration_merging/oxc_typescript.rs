//! ts-no-unsafe-declaration-merging OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
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
        let mut class_names: Vec<(&str, u32)> = Vec::new();
        let mut interface_names: Vec<(&str, u32)> = Vec::new();

        for node in semantic.nodes().iter() {
            match node.kind() {
                AstKind::Class(class) => {
                    if let Some(id) = &class.id {
                        class_names.push((id.name.as_str(), id.span.start));
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
                    interface_names.push((decl.id.name.as_str(), decl.id.span.start));
                }
                _ => {}
            }
        }

        let mut diagnostics = Vec::new();

        // Flag interfaces that share a name with a class
        for (iface_name, offset) in &interface_names {
            if class_names.iter().any(|(c, _)| c == iface_name) {
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

        // Flag classes that share a name with an interface
        for (class_name, offset) in &class_names {
            if interface_names.iter().any(|(i, _)| i == class_name) {
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
}
