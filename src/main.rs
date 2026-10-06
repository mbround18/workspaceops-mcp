use std::{
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
#[command(name = "workspaceops", version)]
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
        eprintln!("workspaceops error: {err:#}");
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
