//! OxcCheck backend for rn-flashlist-over-flatlist — flag `FlatList` from 'react-native'.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ImportDeclaration]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ImportDeclaration(import) = node.kind() else { return };

        if import.source.value.as_str() != "react-native" {
            return;
        }

        let Some(ref specifiers) = import.specifiers else { return };
        let has_flatlist = specifiers.iter().any(|s| {
            if let oxc_ast::ast::ImportDeclarationSpecifier::ImportSpecifier(named) = s {
                named.local.name.as_str() == "FlatList"
            } else {
                false
            }
        });

        if !has_flatlist {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, import.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`FlatList` from 'react-native' is slow — import `FlashList` from '@shopify/flash-list'.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;


    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx(s, &Check)
    }


    #[test]
    fn flags_flatlist_import() {
        let src = "import { FlatList } from 'react-native';";
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn flags_flatlist_among_others() {
        let src = "import { View, FlatList, Text } from 'react-native';";
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn allows_flashlist() {
        let src = "import { FlashList } from '@shopify/flash-list';";
        assert!(run(src).is_empty());
    }


    #[test]
    fn allows_other_rn_imports() {
        let src = "import { View, Text } from 'react-native';";
        assert!(run(src).is_empty());
    }
}
