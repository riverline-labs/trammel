// Copyright (c) 2026 Riverline Labs LLC. Licensed under Apache 2.0.
//! `trammel` CLI: thin dispatch over the library API.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

use trammel::config::Config;

#[derive(Parser)]
#[command(
    name = "trammel",
    version,
    about = "Architectural conformance for Rust workspaces"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Cmd>,

    #[command(flatten)]
    check: CheckArgs,
}

#[derive(Subcommand)]
enum Cmd {
    /// Run conformance checks (default).
    Check(CheckArgs),
    /// Inspect rules from the loaded config.
    Rules(RulesCmd),
}

#[derive(clap::Args, Default)]
struct CheckArgs {
    /// Path to trammel.toml (default: ./trammel.toml).
    #[arg(long, value_name = "PATH")]
    config: Option<PathBuf>,
    /// Override `src_root` from the config.
    #[arg(long, value_name = "PATH")]
    src: Option<PathBuf>,
}

#[derive(clap::Args)]
struct RulesCmd {
    #[command(subcommand)]
    sub: Option<RulesSub>,
}

#[derive(Subcommand)]
enum RulesSub {
    /// List every active rule code (default).
    List,
    /// Print a rule's configured message and source location.
    Explain {
        /// The rule code (e.g. APP_NO_AXUM).
        rule: String,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command.unwrap_or(Cmd::Check(cli.check)) {
        Cmd::Check(args) => match cmd_check(args) {
            Ok(code) => code,
            Err(e) => {
                eprintln!("trammel: {e:#}");
                ExitCode::from(2)
            }
        },
        Cmd::Rules(rcmd) => match cmd_rules(rcmd) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("trammel: {e:#}");
                ExitCode::from(2)
            }
        },
    }
}

fn cmd_check(args: CheckArgs) -> Result<ExitCode> {
    let (mut cfg, project_root) = load_cfg(&args.config)?;
    if let Some(src) = args.src {
        cfg.src_root = src.to_string_lossy().into_owned();
    }
    let violations = trammel::run(&cfg, &project_root)?;
    let report = trammel::violations::render(&violations);
    print!("{report}");
    if violations.is_empty() {
        Ok(ExitCode::SUCCESS)
    } else {
        Ok(ExitCode::from(1))
    }
}

fn cmd_rules(rcmd: RulesCmd) -> Result<()> {
    let (cfg, _project_root) = load_cfg(&None)?;
    match rcmd.sub.unwrap_or(RulesSub::List) {
        RulesSub::List => print_rule_list(&cfg),
        RulesSub::Explain { rule } => print_rule_explain(&cfg, &rule)?,
    }
    Ok(())
}

fn print_rule_list(cfg: &Config) {
    for entry in collect_rules(cfg) {
        println!("{} ({})", entry.rule, entry.kind);
    }
}

fn print_rule_explain(cfg: &Config, rule: &str) -> Result<()> {
    let entries = collect_rules(cfg);
    let matches: Vec<_> = entries.iter().filter(|e| e.rule == rule).collect();
    if matches.is_empty() {
        anyhow::bail!("no rule named `{rule}` in config");
    }
    for e in matches {
        println!("rule: {}", e.rule);
        println!("kind: {}", e.kind);
        match &e.message {
            Some(m) => println!("message: {m}"),
            None => println!("message: (default)"),
        }
        println!();
    }
    Ok(())
}

struct RuleEntry<'a> {
    rule: &'a str,
    kind: &'static str,
    message: Option<&'a str>,
}

fn collect_rules(cfg: &Config) -> Vec<RuleEntry<'_>> {
    let mut out = Vec::new();
    for r in &cfg.forbidden_imports {
        out.push(RuleEntry {
            rule: &r.rule,
            kind: "forbidden_imports",
            message: r.message.as_deref(),
        });
    }
    for r in &cfg.forbidden_inline_paths {
        out.push(RuleEntry {
            rule: &r.rule,
            kind: "forbidden_inline_paths",
            message: r.message.as_deref(),
        });
    }
    for r in &cfg.forbidden_macros {
        out.push(RuleEntry {
            rule: &r.rule,
            kind: "forbidden_macros",
            message: r.message.as_deref(),
        });
    }
    for r in &cfg.forbidden_methods {
        out.push(RuleEntry {
            rule: &r.rule,
            kind: "forbidden_methods",
            message: r.message.as_deref(),
        });
    }
    for r in &cfg.required_struct_attrs {
        out.push(RuleEntry {
            rule: &r.rule,
            kind: "required_struct_attrs",
            message: r.message.as_deref(),
        });
    }
    for r in &cfg.fs_must_exist {
        out.push(RuleEntry {
            rule: &r.rule,
            kind: "fs_must_exist",
            message: r.message.as_deref(),
        });
    }
    for r in &cfg.fs_must_not_exist {
        out.push(RuleEntry {
            rule: &r.rule,
            kind: "fs_must_not_exist",
            message: r.message.as_deref(),
        });
    }
    for r in &cfg.file_content_scan {
        out.push(RuleEntry {
            rule: &r.rule,
            kind: "file_content_scan",
            message: r.message.as_deref(),
        });
    }
    if let Some(n) = &cfg.n_plus_one {
        out.push(RuleEntry {
            rule: &n.rule,
            kind: "n_plus_one",
            message: n.message.as_deref(),
        });
    }
    out
}

/// Resolve the config path (falling back to `./trammel.toml`) and the
/// project root (the config file's parent directory).
fn load_cfg(explicit: &Option<PathBuf>) -> Result<(Config, PathBuf)> {
    let path = explicit
        .clone()
        .unwrap_or_else(|| PathBuf::from("trammel.toml"));
    let cfg = trammel::config::load(&path)
        .with_context(|| format!("failed to load config `{}`", path.display()))?;
    let project_root = path
        .parent()
        .map(|p| {
            if p.as_os_str().is_empty() {
                PathBuf::from(".")
            } else {
                p.to_path_buf()
            }
        })
        .unwrap_or_else(|| PathBuf::from("."));
    Ok((cfg, project_root))
}
