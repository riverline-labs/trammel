// Copyright (c) 2026 Riverline Labs LLC. Licensed under Apache 2.0.
//! `trammel` CLI: thin dispatch over the library API.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use globset::GlobSet;

use trammel::config::{Config, Layer};
use trammel::layers::LayerSet;
use trammel::rules::{scope_applies, CompiledRules};

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
    /// Show the layer a file classifies into and which rules apply to it.
    Inspect(InspectArgs),
}

#[derive(clap::Args)]
struct InspectArgs {
    /// File path (absolute, or relative to project root or src_root).
    file: PathBuf,
    /// Path to trammel.toml (default: ./trammel.toml).
    #[arg(long, value_name = "PATH")]
    config: Option<PathBuf>,
}

#[derive(clap::Args, Default)]
struct CheckArgs {
    /// Path to trammel.toml (default: ./trammel.toml).
    #[arg(long, value_name = "PATH")]
    config: Option<PathBuf>,
    /// Override `src_root` from the config.
    #[arg(long, value_name = "PATH")]
    src: Option<PathBuf>,
    /// Emit a JSON array of violations to stdout instead of the human-readable report.
    #[arg(long)]
    json: bool,
}

#[derive(clap::Args)]
struct RulesCmd {
    #[command(subcommand)]
    sub: Option<RulesSub>,
    /// Path to trammel.toml (default: ./trammel.toml).
    #[arg(long, value_name = "PATH", global = true)]
    config: Option<PathBuf>,
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
        Cmd::Inspect(args) => match cmd_inspect(args) {
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
    if args.json {
        let json = trammel::violations::render_json(&violations)
            .context("failed to render violations as JSON")?;
        println!("{json}");
    } else {
        let report = trammel::violations::render(&violations);
        print!("{report}");
    }
    if violations.is_empty() {
        Ok(ExitCode::SUCCESS)
    } else {
        Ok(ExitCode::from(1))
    }
}

fn cmd_rules(rcmd: RulesCmd) -> Result<()> {
    let (cfg, _project_root) = load_cfg(&rcmd.config)?;
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
    for r in &cfg.forbidden_constructors {
        out.push(RuleEntry {
            rule: &r.rule,
            kind: "forbidden_constructors",
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

fn cmd_inspect(args: InspectArgs) -> Result<()> {
    let (cfg, project_root) = load_cfg(&args.config)?;
    let layer_set = LayerSet::build(&cfg).context("failed to build layer set")?;
    let compiled = CompiledRules::build(&cfg).context("failed to compile rule scopes")?;

    let rel = resolve_rel_to_src_root(&args.file, &project_root, &cfg.src_root)?;
    println!("file: {rel}");
    let Some(layer) = layer_set.classify(&rel) else {
        println!("layer: (unclassified — no [[layers]] entry matches; rules will be skipped)");
        return Ok(());
    };
    println!("layer: {}", layer.name);
    let exempt = layer_set.is_exempt(layer, &rel);
    println!(
        "exempt: {}",
        if exempt {
            "yes (every layer-scoped rule skipped for this file)"
        } else {
            "no"
        }
    );
    println!(
        "test_context: {}",
        if layer.implicit_test_context {
            "yes (layer has implicit_test_context = true)"
        } else {
            "no (per-fn `#[test]` / `#[cfg(test)]` may still apply)"
        }
    );
    println!();

    if exempt {
        println!("(no further rules listed because this file is layer-exempt)");
        return Ok(());
    }

    let mut applies: Vec<String> = Vec::new();
    let mut skipped: Vec<String> = Vec::new();

    record(
        &mut applies,
        &mut skipped,
        "forbidden_imports",
        cfg.forbidden_imports
            .iter()
            .map(|r| (&r.rule, &r.in_layers, &r.in_layers_except)),
        &compiled.forbidden_imports,
        layer,
        &rel,
    );
    record(
        &mut applies,
        &mut skipped,
        "forbidden_inline_paths",
        cfg.forbidden_inline_paths
            .iter()
            .map(|r| (&r.rule, &r.in_layers, &r.in_layers_except)),
        &compiled.forbidden_inline_paths,
        layer,
        &rel,
    );
    record(
        &mut applies,
        &mut skipped,
        "forbidden_constructors",
        cfg.forbidden_constructors
            .iter()
            .map(|r| (&r.rule, &r.in_layers, &r.in_layers_except)),
        &compiled.forbidden_constructors,
        layer,
        &rel,
    );
    record(
        &mut applies,
        &mut skipped,
        "forbidden_macros",
        cfg.forbidden_macros
            .iter()
            .map(|r| (&r.rule, &r.in_layers, &r.in_layers_except)),
        &compiled.forbidden_macros,
        layer,
        &rel,
    );
    record(
        &mut applies,
        &mut skipped,
        "forbidden_methods",
        cfg.forbidden_methods
            .iter()
            .map(|r| (&r.rule, &r.in_layers, &r.in_layers_except)),
        &compiled.forbidden_methods,
        layer,
        &rel,
    );
    record(
        &mut applies,
        &mut skipped,
        "required_struct_attrs",
        cfg.required_struct_attrs
            .iter()
            .map(|r| (&r.rule, &r.in_layers, &r.in_layers_except)),
        &compiled.required_struct_attrs,
        layer,
        &rel,
    );

    if let Some(n) = &cfg.n_plus_one {
        let in_scope = n.in_layers.iter().any(|l| l == &layer.name);
        if in_scope {
            applies.push(format!("n_plus_one/{}", n.rule));
        } else {
            skipped.push(format!(
                "n_plus_one/{} (layer `{}` not in n_plus_one.in_layers)",
                n.rule, layer.name
            ));
        }
    }

    println!("rules that apply ({}):", applies.len());
    if applies.is_empty() {
        println!("  (none)");
    }
    for entry in &applies {
        println!("  {entry}");
    }
    println!();
    println!("rules that do NOT apply ({}):", skipped.len());
    if skipped.is_empty() {
        println!("  (none)");
    }
    for entry in &skipped {
        println!("  {entry}");
    }

    Ok(())
}

fn record<'a>(
    applies: &mut Vec<String>,
    skipped: &mut Vec<String>,
    kind: &str,
    rules: impl Iterator<Item = (&'a String, &'a Vec<String>, &'a Vec<String>)>,
    compiled: &[GlobSet],
    layer: &Layer,
    rel_path: &str,
) {
    for ((rule, in_layers, in_layers_except), files) in rules.zip(compiled.iter()) {
        let label = format!("{kind}/{rule}");
        if scope_applies(in_layers, in_layers_except, files, layer, rel_path) {
            applies.push(label);
        } else {
            skipped.push(label);
        }
    }
}

/// Normalize a user-given file path into one relative to `src_root` (the
/// representation the engine uses for layer classification).
///
/// Accepts: absolute paths under src_root, project-rooted paths starting
/// with `<src_root>/`, and paths already relative to src_root. Existence is
/// not checked — `inspect` answers "what *would* this file classify as?".
fn resolve_rel_to_src_root(file: &Path, project_root: &Path, src_root: &str) -> Result<String> {
    let s = file.to_string_lossy().replace('\\', "/");

    if file.is_absolute() {
        let src_root_abs = project_root.join(src_root);
        let canon_src = src_root_abs.canonicalize().unwrap_or(src_root_abs);
        let canon_file = file.canonicalize().unwrap_or_else(|_| file.to_path_buf());
        let rel = canon_file.strip_prefix(&canon_src).with_context(|| {
            format!(
                "absolute path `{}` is not under src_root `{}`",
                canon_file.display(),
                canon_src.display()
            )
        })?;
        return Ok(rel.to_string_lossy().replace('\\', "/"));
    }

    let src_prefix = format!("{}/", src_root.trim_end_matches('/'));
    Ok(s.strip_prefix(&src_prefix).unwrap_or(&s).to_string())
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
