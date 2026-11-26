//! Mandatory external toolchains.
//!
//! comply delegates part of its rule set to programs it does not own: clippy,
//! cargo-shear, oxlint, and the typescript-go checker behind the
//! type-aware sidecar. None is optional. Skipping one would silently
//! under-report, so comply refuses to run rather than print a false "clean".
//!
//! A refusal is a diagnosis, not a crash. This type is what every refusal
//! raises, so `main` can answer with the remedy instead of a bug-report banner.

/// A toolchain comply requires but cannot use.
#[derive(Debug, thiserror::Error)]
pub enum ToolchainError {
    /// Absent, and one command installs it.
    #[error("{what}.\nInstall it with: {install_cmd}")]
    Missing { what: String, install_cmd: String },

    /// A gap comply can name but no single command closes.
    /// An interpreter with no one-line install, a checker API that won't start.
    #[error("{what}.")]
    Unusable { what: String },
}

/// What becomes of files a pass could not analyze.
#[derive(Debug, Clone, Copy)]
pub enum Unanalyzed {
    /// A git file scope refuses them, with exit code 2.
    Refuse,
    /// A full scan notes them on stderr.
    Note,
}

/// How many unanalyzed files a message names.
const NAMED_FILES: usize = 20;

/// Names the `files` a pass did not analyze, and why.
pub fn report_unanalyzed(
    why: &str,
    files: &[&crate::files::SourceFile],
    unanalyzed: Unanalyzed,
) -> anyhow::Result<()> {
    let names: Vec<String> = files
        .iter()
        .take(NAMED_FILES)
        .map(|file| file.path.display().to_string())
        .collect();
    let more = files.len().saturating_sub(NAMED_FILES);
    let rest = if more == 0 {
        String::new()
    } else {
        format!(" and {more} more")
    };
    let what = format!(
        "{why}. These file(s) were not analyzed: {}{rest}",
        names.join(", ")
    );
    match unanalyzed {
        Unanalyzed::Refuse => Err(ToolchainError::Unusable { what }.into()),
        Unanalyzed::Note => {
            eprintln!("comply: {what}");
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_renders_the_install_command_on_its_own_line() {
        let err = ToolchainError::Missing {
            what: "oxlint is required but was not found".to_owned(),
            install_cmd: "npm install -g oxlint".to_owned(),
        };
        assert_eq!(
            err.to_string(),
            "oxlint is required but was not found.\nInstall it with: npm install -g oxlint"
        );
    }

    #[test]
    fn unusable_renders_a_single_sentence() {
        let err = ToolchainError::Unusable {
            what: "`node` was not found".to_owned(),
        };
        assert_eq!(err.to_string(), "`node` was not found.");
    }
}
