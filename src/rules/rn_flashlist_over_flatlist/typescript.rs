//! Flags `import { FlatList, ... } from 'react-native'`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "import_statement" { return; }
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
mod tests {
    use super::*;
    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(s, &Check)
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
