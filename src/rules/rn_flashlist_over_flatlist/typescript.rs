//! Flags `import { FlatList, ... } from 'react-native'`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["import_statement"] => |node, source, ctx, diagnostics|
    let Some(src_node) = node.child_by_field_name("source") else { return };
    let Ok(raw) = src_node.utf8_text(source) else { return };
    let spec = raw.trim_matches(|c| c == '"' || c == '\'');
    if spec != "react-native" { return; }

    // Walk the import clause looking for a named import of `FlatList`.
    let Ok(full) = node.utf8_text(source) else { return };
    // Simple substring check bounded by the import keyword — sufficient since
    // the entire import_statement text starts with `import ... from 'react-native'`.
    let mut has_flatlist = false;
    for token in full.split(|c: char| !c.is_alphanumeric() && c != '_') {
        if token == "FlatList" {
            has_flatlist = true;
            break;
        }
    }
    if !has_flatlist { return; }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "`FlatList` from 'react-native' is slow — import `FlashList` from '@shopify/flash-list'.".into(),
        Severity::Warning,
    ));
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
    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.tsx")
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
