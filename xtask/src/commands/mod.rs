//! Command registry and dispatch for `cargo xtask`.
//!
//! # Extension pattern for later, independently owned modules
//!
//! `xtask` is the *root-owned* dispatcher (#152/#163). Later, independently
//! owned work adds its own command group as a new module -- for example
//! #141 (assets) or #144 (browser-smoke) -- without editing any existing
//! feature module (`test_cmd.rs`, `check_cmd.rs`, `pre_push.rs`):
//!
//! 1. Add a new file under `xtask/src/commands/`, e.g. `assets_cmd.rs`.
//! 2. In it, expose the same three items every group exposes: a `pub const
//!    ABOUT: &str`, a `pub const SUBCOMMANDS: &[(&str, &str)]` (name +
//!    one-line help), and `pub fn run(sub: &str) -> Result<(), StepError>`.
//!    Build each subcommand's `Command` and run it through
//!    [`crate::process::run_step`], exactly like `test_cmd`/`check_cmd` do.
//! 3. Declare the module (`pub mod assets_cmd;`) and add exactly one entry
//!    to the `GROUPS` slice below. That one line is the only change outside
//!    the new module's own file -- no other command group is touched.
//!
//! A standalone root command with no subcommands (like `pre-push`) follows
//! the same shape minus the subcommand list: a `pub const ABOUT`, a `pub fn
//! run() -> Result<(), StepError>`, and one entry in `ROOT_COMMANDS`.

pub mod check_cmd;
pub mod pre_push;
pub mod test_cmd;

use crate::process::StepError;

/// A subcommand family reachable as `cargo xtask <name> <subcommand>`.
struct Group {
    name: &'static str,
    about: &'static str,
    subcommands: &'static [(&'static str, &'static str)],
    run: fn(&str) -> Result<(), StepError>,
}

/// A standalone command reachable as `cargo xtask <name>` (no subcommand).
struct RootCommand {
    name: &'static str,
    about: &'static str,
    run: fn() -> Result<(), StepError>,
}

const GROUPS: &[Group] = &[
    Group {
        name: "test",
        about: test_cmd::ABOUT,
        subcommands: test_cmd::SUBCOMMANDS,
        run: test_cmd::run,
    },
    Group {
        name: "check",
        about: check_cmd::ABOUT,
        subcommands: check_cmd::SUBCOMMANDS,
        run: check_cmd::run,
    },
];

const ROOT_COMMANDS: &[RootCommand] = &[RootCommand {
    name: "pre-push",
    about: pre_push::ABOUT,
    run: pre_push::run,
}];

/// Everything that can go wrong dispatching a command line, distinct from a
/// step actually failing (see [`StepError`]).
pub enum DispatchError {
    /// The arguments didn't match a known command; `message` explains why
    /// and how to fix it.
    Usage(String),
    /// A known command ran and one of its steps failed.
    Step(StepError),
}

impl From<StepError> for DispatchError {
    fn from(err: StepError) -> Self {
        DispatchError::Step(err)
    }
}

/// Parses and runs `args` (the process args after `xtask` itself, i.e. after
/// cargo's own `run --package xtask --`). Printing `--help`/no-args counts
/// as success.
pub fn dispatch(args: &[String]) -> Result<(), DispatchError> {
    match args.first().map(String::as_str) {
        None | Some("-h") | Some("--help") => {
            print_help();
            Ok(())
        }
        Some(name) => {
            if let Some(root) = ROOT_COMMANDS.iter().find(|c| c.name == name) {
                if let Some(extra) = args.get(1) {
                    return Err(DispatchError::Usage(format!(
                        "`cargo xtask {name}` takes no arguments (got `{extra}`)"
                    )));
                }
                return (root.run)().map_err(DispatchError::from);
            }

            if let Some(group) = GROUPS.iter().find(|g| g.name == name) {
                let Some(sub) = args.get(1) else {
                    return Err(DispatchError::Usage(usage_for_group(group)));
                };
                if !group.subcommands.iter().any(|(n, _)| *n == sub) {
                    return Err(DispatchError::Usage(format!(
                        "unknown `cargo xtask {name}` subcommand: `{sub}`\n\n{}",
                        usage_for_group(group)
                    )));
                }
                return (group.run)(sub).map_err(DispatchError::from);
            }

            Err(DispatchError::Usage(format!(
                "unknown command: `{name}`\n\n{}",
                help_text()
            )))
        }
    }
}

fn usage_for_group(group: &Group) -> String {
    let names: Vec<&str> = group.subcommands.iter().map(|(n, _)| *n).collect();
    format!("usage: cargo xtask {} <{}>", group.name, names.join("|"))
}

fn print_help() {
    print!("{}", help_text());
}

fn help_text() -> String {
    let mut text = String::new();
    text.push_str("cargo xtask -- root verification dispatcher for romanian-folk-fight\n\n");
    text.push_str("USAGE:\n    cargo xtask <command> [subcommand]\n\n");
    text.push_str("COMMANDS:\n");
    for group in GROUPS {
        text.push_str(&format!("  {} -- {}\n", group.name, group.about));
        for (sub, about) in group.subcommands {
            text.push_str(&format!("    {} {sub:<12} {about}\n", group.name));
        }
    }
    for root in ROOT_COMMANDS {
        text.push_str(&format!("  {:<17} -- {}\n", root.name, root.about));
    }
    text.push_str(
        "\nSee xtask/README.md for the artifact-directory convention and the\n\
         extension pattern for adding new command groups.\n",
    );
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn help_lists_only_root_owned_commands() {
        let text = help_text();
        assert!(text.contains("test logic"));
        assert!(text.contains("test journey"));
        assert!(text.contains("check build-matrix"));
        assert!(text.contains("pre-push"));
        // Must never grow asset/browser-smoke placeholders (#141/#144 own those).
        assert!(!text.to_lowercase().contains("asset"));
        assert!(!text.to_lowercase().contains("browser"));
    }

    #[test]
    fn unknown_command_is_a_usage_error_not_a_panic() {
        let result = dispatch(&["nope".to_string()]);
        assert!(matches!(result, Err(DispatchError::Usage(_))));
    }

    #[test]
    fn a_group_without_a_subcommand_is_a_usage_error() {
        let result = dispatch(&["test".to_string()]);
        assert!(matches!(result, Err(DispatchError::Usage(_))));
    }

    #[test]
    fn an_unknown_subcommand_is_a_usage_error() {
        let result = dispatch(&["test".to_string(), "nope".to_string()]);
        assert!(matches!(result, Err(DispatchError::Usage(_))));
    }
}
