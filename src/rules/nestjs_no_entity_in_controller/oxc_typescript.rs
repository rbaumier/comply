use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

fn is_nestjs_controller_file(source: &str) -> bool {
    crate::oxc_helpers::source_contains(source, "@Controller")
}

fn is_entity_import(line: &str) -> bool {
    let trimmed = line.trim_start();
    if !trimmed.starts_with("import ") {
        return false;
    }
    if trimmed.contains(".entity'")
        || trimmed.contains(".entity\"")
        || trimmed.contains("/entities/")
        || trimmed.contains("/entity/")
    {
        return true;
    }
    let Some(open) = trimmed.find('{') else {
        return false;
    };
    let Some(close) = trimmed[open..].find('}') else {
        return false;
    };
    let names = &trimmed[open + 1..open + close];
    names
        .split(',')
        .map(|n| n.trim().trim_start_matches("type ").trim())
        .any(|name| {
            let imported = name.split_whitespace().next().unwrap_or("");
            imported.ends_with("Entity") && imported != "Entity"
        })
}

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if !is_nestjs_controller_file(ctx.source) {
            return Vec::new();
        }
        ctx.source
            .lines()
            .enumerate()
            .filter(|(_, line)| is_entity_import(line))
            .map(|(idx, _)| Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line: idx + 1,
                column: 1,
                rule_id: super::META.id.into(),
                message: "Controller imports an ORM entity — return a DTO from the service \
                          instead of leaking the persistence model into the HTTP layer."
                    .into(),
                severity: Severity::Warning,
                span: None,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_named_entity_import() {
        let src = "import { UserEntity } from './user.entity';\n@Controller() class C {}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_entity_module_path() {
        let src = "import { User } from './entities/user';\n@Controller() class C {}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_dto_import() {
        let src =
            "import { CreateUserDto } from './dto/create-user.dto';\n@Controller() class C {}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_non_controller_files() {
        let src = "import { UserEntity } from './user.entity';\nclass Service {}";
        assert!(run(src).is_empty());
    }
}
