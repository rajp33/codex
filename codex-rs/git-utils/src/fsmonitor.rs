//! Policy for preserving Git's built-in filesystem monitor.
//!
//! Codex overrides `core.fsmonitor` so repository configuration cannot select
//! an executable helper. Preserve the built-in daemon only when the effective
//! value is canonical boolean `true` and Git advertises daemon support.

use std::future::Future;

const CONFIG_PROBE_ARGS: &[&str] = &["config", "--null", "--get", "core.fsmonitor"];
const CAPABILITY_PROBE_ARGS: &[&str] = &["version", "--build-options"];
const PROBE_ENV: &[(&str, &str)] = &[("GIT_OPTIONAL_LOCKS", "0"), ("LC_ALL", "C")];

/// The safe `core.fsmonitor` override for an internal Git command.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FsmonitorOverride {
    /// Disable repository-selected filesystem monitor helpers.
    Disabled,
    /// Preserve Git's built-in filesystem monitor daemon.
    BuiltIn,
}

impl FsmonitorOverride {
    /// Returns the complete Git configuration override.
    pub const fn git_config_arg(self) -> &'static str {
        match self {
            Self::Disabled => "core.fsmonitor=false",
            Self::BuiltIn => "core.fsmonitor=true",
        }
    }
}

/// Executes the Git commands required by [`detect_fsmonitor_override`].
///
/// Implementations must return stdout only when Git exits successfully.
/// Timeouts, spawn or transport failures, signal termination, and nonzero exit
/// statuses must return `None`.
pub trait FsmonitorProbeRunner: Send {
    /// Runs one bounded probe in the target repository and environment.
    fn run_probe(
        &mut self,
        args: &'static [&'static str],
        env: &'static [(&'static str, &'static str)],
    ) -> impl Future<Output = Option<Vec<u8>>> + Send;
}

/// Returns the safe filesystem monitor override for the target repository.
///
/// This intentionally probes every time. Effective Git configuration is
/// layered, may use conditional includes, and can change while Codex is
/// running:
/// https://git-scm.com/docs/git-config#SCOPES
/// https://git-scm.com/docs/git-config#_conditional_includes
pub async fn detect_fsmonitor_override(
    runner: &mut impl FsmonitorProbeRunner,
) -> FsmonitorOverride {
    // A typed query converts every matching value before `--get` selects the
    // effective one. A shadowed helper path can therefore make a repository-
    // local true fail conversion. Query the raw value and accept only the exact
    // spelling `true` followed by the NUL terminator requested here.
    // https://github.com/git/git/blob/94f057755b7941b321fd11fec1b2e3ca5313a4e0/builtin/config.c#L482-L514
    // https://github.com/git/git/blob/94f057755b7941b321fd11fec1b2e3ca5313a4e0/builtin/config.c#L611-L614
    let Some(config) = runner.run_probe(CONFIG_PROBE_ARGS, PROBE_ENV).await else {
        return FsmonitorOverride::Disabled;
    };
    if config != b"true\0" {
        return FsmonitorOverride::Disabled;
    }

    // Git 2.35.1 and older interpret "true" as a hook pathname. Before Git
    // 2.26, a successful empty hook response can hide tracked changes. Require
    // the feature line Git added specifically for capability checks.
    // https://github.com/git/git/blob/94f057755b7941b321fd11fec1b2e3ca5313a4e0/Documentation/config/core.adoc#L90-L99
    // https://github.com/git/git/commit/dd77cf61a1a2fbf52c94d0cd986d555ad2ba8a4b
    let Some(build_options) = runner.run_probe(CAPABILITY_PROBE_ARGS, PROBE_ENV).await else {
        return FsmonitorOverride::Disabled;
    };
    if build_options
        .split(|byte| *byte == b'\n')
        .any(|line| line.trim_ascii() == b"feature: fsmonitor--daemon")
    {
        FsmonitorOverride::BuiltIn
    } else {
        FsmonitorOverride::Disabled
    }
}

#[cfg(test)]
#[path = "fsmonitor_tests.rs"]
mod tests;
