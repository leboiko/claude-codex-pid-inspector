//! Command-line interface definition for `agentop`.
//!
//! Parsed once at startup by [`parse`]; the resulting [`Cli`] struct drives
//! which execution mode is used (interactive TUI, `--list`, `--json`, or
//! shell-completion output).

use clap::{ArgGroup, Parser, ValueEnum};

/// agentop — a TUI process inspector for Claude Code and OpenAI Codex CLI.
#[derive(Debug, Parser)]
#[command(
    name = "agentop",
    version,
    about = "A TUI process inspector for Claude Code and OpenAI Codex CLI",
    long_about = None,
    group(
        ArgGroup::new("output_mode")
            .args(["json", "list"])
            .multiple(false)
    )
)]
pub struct Cli {
    /// Print a JSON snapshot to stdout and exit.
    ///
    /// Outputs a single-line JSON object (NDJSON-compatible) unless
    /// `--pretty` is also given. Use `--pretty` for human-readable indented
    /// output.
    #[arg(long, group = "output_mode")]
    pub json: bool,

    /// Indent the `--json` output (4 spaces). Ignored without `--json`.
    #[arg(long, requires = "json")]
    pub pretty: bool,

    /// Print a plain-text process table to stdout and exit.
    #[arg(long, group = "output_mode")]
    pub list: bool,

    /// Print a shell-completion script to stdout and exit.
    ///
    /// Example: `agentop --generate-completions zsh > ~/.zsh/completions/_agentop`
    #[arg(long, value_name = "SHELL")]
    pub generate_completions: Option<Shell>,
}

/// Supported shells for completion-script generation.
///
/// Mapped to [`clap_complete::Shell`] values when generating scripts.
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum Shell {
    /// Bash shell completions.
    Bash,
    /// Zsh shell completions.
    Zsh,
    /// Fish shell completions.
    Fish,
    /// PowerShell completions.
    Powershell,
    /// Elvish shell completions.
    Elvish,
}

impl From<Shell> for clap_complete::Shell {
    fn from(s: Shell) -> Self {
        match s {
            Shell::Bash => clap_complete::Shell::Bash,
            Shell::Zsh => clap_complete::Shell::Zsh,
            Shell::Fish => clap_complete::Shell::Fish,
            Shell::Powershell => clap_complete::Shell::PowerShell,
            Shell::Elvish => clap_complete::Shell::Elvish,
        }
    }
}

/// Parse command-line arguments from `std::env::args_os`.
///
/// Exits the process with a formatted error message if the arguments are
/// invalid; this is standard `clap` behaviour.
pub fn parse() -> Cli {
    Cli::parse()
}
