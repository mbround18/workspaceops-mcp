use std::{
    fs,
    path::PathBuf,
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use anyhow::{Context, Result, bail};
use clap::Parser;
use workspaceops::{
    DEFAULT_PREFIX, WorkspaceSession, cleanup_workspace_session, create_workspace_session,
    default_workspace_root, discover_sibling_dirs,
};

#[derive(Debug, Parser)]
#[command(name = "woops", version)]
struct Cli {
    /// Directories to link into the workspace session.
    #[arg(value_name = "DIR", num_args = 0..)]
    dirs: Vec<PathBuf>,

    /// Agent CLI command to launch (for example: `claude`, `copilot`, `gemini`).
    #[arg(long, default_value = "copilot", value_name = "CLI")]
    agent: String,

    /// Sibling discovery prefix when DIR args are omitted.
    #[arg(long, default_value = DEFAULT_PREFIX)]
    prefix: String,

    /// Root directory for session folders.
    #[arg(long)]
    root: Option<PathBuf>,

    /// Prepare links but do not launch an agent process.
    #[arg(long, default_value_t = false)]
    no_launch: bool,

    /// Keep session directory instead of auto-cleaning on exit.
    #[arg(long, default_value_t = false)]
    keep_on_exit: bool,
}

fn main() {
    if let Err(err) = run() {
        eprintln!("woops error: {err:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    println!("woops lol");

    let cwd = std::env::current_dir().context("failed reading current directory")?;
    let dirs = if cli.dirs.is_empty() {
        discover_sibling_dirs(&cwd, &cli.prefix)?
    } else {
        cli.dirs
    };
    if dirs.is_empty() {
        bail!(
            "no directories to link (pass DIR args or ensure sibling directories matching `{}-*`)",
            cli.prefix
        );
    }

    let root = cli.root.unwrap_or(default_workspace_root()?);
    let session = create_workspace_session(&root, &dirs)?;
    write_agents_md(&session)?;
    println!("workspace: {}", session.session_dir.display());
    for linked in &session.linked_dirs {
        println!("linked: {}", linked.display());
    }

    let outcome = if cli.no_launch {
        Ok(())
    } else {
        run_agent_until_exit(&session, &cli.agent)
    };

    if cli.keep_on_exit {
        println!("kept: {}", session.session_dir.display());
    } else {
        cleanup_workspace_session(&session.session_dir, &session.root)?;
        println!("cleaned: {}", session.session_dir.display());
    }

    outcome
}

fn write_agents_md(session: &WorkspaceSession) -> Result<()> {
    let mut body = String::new();
    body.push_str("# AGENTS.md\n\n");
    body.push_str("This is an ephemeral workspace created by `woops`.\n\n");
    body.push_str("- This session folder will be removed when the session ends unless `--keep-on-exit` is used.\n");
    body.push_str("- Do not store important files here.\n");
    body.push_str("- Use this directory as quick access to relevant project material.\n");
    body.push_str(
        "- Tools like graphify are fine to run, but treat this workspace as disposable.\n\n",
    );
    body.push_str("## Linked items and guidance file hints\n\n");
    if session.linked_dirs.is_empty() {
        body.push_str("_No linked items._\n");
    } else {
        for linked in &session.linked_dirs {
            let name = linked
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("(unknown)");
            body.push_str(&format!("- `{name}`\n"));
            let llm = linked.join("llm.txt");
            if llm.exists() {
                body.push_str(&format!("  - `llm.txt`: `{}`\n", llm.display()));
            } else if let Some(fallback) = first_existing(
                linked,
                &[
                    "AGENTS.md",
                    "agents.md",
                    "CLAUDE.md",
                    "claude.md",
                    "README.md",
                    "readme.md",
                ],
            ) {
                body.push_str(&format!(
                    "  - `llm.txt`: _(not found)_\n  - fallback: `{}`\n",
                    fallback.display()
                ));
            } else {
                body.push_str(&format!(
                    "  - `llm.txt`: _(not found at `{}`)_\n  - fallback: _(none found: AGENTS.md / CLAUDE.md / README.md)_\n",
                    llm.display()
                ));
            }
        }
    }
    fs::write(session.session_dir.join("AGENTS.md"), body)
        .context("failed writing AGENTS.md into workspace session")?;
    Ok(())
}

fn first_existing(root: &std::path::Path, candidates: &[&str]) -> Option<PathBuf> {
    candidates
        .iter()
        .map(|name| root.join(name))
        .find(|path| path.exists())
}

fn run_agent_until_exit(session: &WorkspaceSession, agent: &str) -> Result<()> {
    let stop = Arc::new(AtomicBool::new(false));
    let stop_flag = Arc::clone(&stop);
    ctrlc::set_handler(move || {
        stop_flag.store(true, Ordering::SeqCst);
    })
    .context("failed installing Ctrl+C handler")?;

    let mut child = Command::new(agent)
        .current_dir(&session.session_dir)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .with_context(|| format!("failed launching agent `{agent}`"))?;
    println!("launched-agent: {agent}");

    loop {
        if stop.load(Ordering::SeqCst) {
            let _ = child.kill();
            let _ = child.wait();
            break;
        }

        if let Some(status) = child
            .try_wait()
            .context("failed waiting for launched agent process")?
        {
            if !status.success() {
                bail!("agent process `{agent}` exited non-zero: {status}");
            }
            break;
        }
        std::thread::sleep(Duration::from_millis(120));
    }

    Ok(())
}
