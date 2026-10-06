use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use rmcp::{
    ErrorData, ServerHandler, ServiceExt,
    handler::server::wrapper::Parameters,
    model::{CallToolResult, ContentBlock, Implementation, ServerCapabilities, ServerConfig},
    tool, tool_router,
    transport::stdio,
};
use serde::Deserialize;
use tracing_subscriber::EnvFilter;
use workspaceops::{create_workspace_session, default_workspace_root, inventory};

#[derive(Debug, Parser)]
#[command(name = "workspaceops-mcp", version)]
struct Cli {
    #[arg(long, env = "WORKSPACEOPS_MCP_LOG", default_value = "info")]
    log: String,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
struct InventoryParams {
    #[serde(default)]
    prefix: Option<String>,
    #[serde(default)]
    base_dir: Option<String>,
    #[serde(default)]
    inspect_links: Vec<String>,
    #[serde(default)]
    cwd: Option<String>,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
struct PrepareParams {
    #[serde(default)]
    dirs: Vec<String>,
    #[serde(default)]
    prefix: Option<String>,
    #[serde(default)]
    root: Option<String>,
    #[serde(default)]
    cwd: Option<String>,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
struct CleanupParams {
    session_dir: String,
    #[serde(default)]
    root: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
struct PrepareOutcome {
    root: String,
    session_dir: String,
    linked_dirs: Vec<String>,
}

#[derive(Default, Clone)]
struct WorkspaceOpsServer;

#[tool_router]
impl WorkspaceOpsServer {
    #[tool(
        name = "workspace_inventory",
        description = "Read-only sibling workspace and local symlink inventory (`<prefix>-*`) for multi-checkout setups."
    )]
    fn workspace_inventory(
        &self,
        Parameters(params): Parameters<InventoryParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let cwd = params
            .cwd
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        let prefix = params.prefix.unwrap_or_else(|| "ThunderForgeVTT".into());
        match inventory(
            &cwd,
            &prefix,
            params.base_dir.as_deref().map(PathBuf::from).as_deref(),
            &params.inspect_links,
        ) {
            Ok(outcome) => {
                let mut text = format!("base: {}\nprefix: {}\n", outcome.base_dir, outcome.prefix);
                if outcome.siblings.is_empty() {
                    text.push_str("\nsiblings: none\n");
                } else {
                    text.push_str("\nsiblings:\n");
                    for s in &outcome.siblings {
                        text.push_str(&format!(
                            "- {} ({}){}\n",
                            s.name,
                            s.path,
                            if s.has_git_dir { " [.git]" } else { "" }
                        ));
                    }
                }
                if !outcome.links.is_empty() {
                    text.push_str("\nlinks:\n");
                    for l in &outcome.links {
                        text.push_str(&format!("- {}: {}", l.name, l.kind));
                        if let Some(target) = &l.target {
                            text.push_str(&format!(" -> {target}"));
                        }
                        if let Some(resolved) = &l.resolved {
                            text.push_str(&format!(" (resolves to {resolved})"));
                        }
                        text.push('\n');
                    }
                }
                Ok(with_structured(text, &outcome))
            }
            Err(err) => Ok(CallToolResult::error(vec![ContentBlock::text(
                err.to_string(),
            )])),
        }
    }

    #[tool(
        name = "workspace_prepare",
        description = "Create a managed workspace session under `~/.workspaces` (or `root`) with symlinks to provided dirs; if dirs are empty, discovers `<prefix>-*` siblings."
    )]
    fn workspace_prepare(
        &self,
        Parameters(params): Parameters<PrepareParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let cwd = params
            .cwd
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        let prefix = params.prefix.unwrap_or_else(|| "ThunderForgeVTT".into());
        let dirs: Vec<PathBuf> = if params.dirs.is_empty() {
            workspaceops::discover_sibling_dirs(&cwd, &prefix)
                .map_err(|e| ErrorData::internal_error(e.to_string(), None))?
        } else {
            params.dirs.iter().map(PathBuf::from).collect()
        };
        let root = match params.root {
            Some(r) => PathBuf::from(r),
            None => default_workspace_root()
                .map_err(|e| ErrorData::internal_error(e.to_string(), None))?,
        };
        let session = create_workspace_session(&root, &dirs)
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        let outcome = PrepareOutcome {
            root: session.root.display().to_string(),
            session_dir: session.session_dir.display().to_string(),
            linked_dirs: session
                .linked_dirs
                .iter()
                .map(|p| p.display().to_string())
                .collect(),
        };
        let mut text = format!("session: {}\n", outcome.session_dir);
        for dir in &outcome.linked_dirs {
            text.push_str(&format!("linked: {dir}\n"));
        }
        Ok(with_structured(text, &outcome))
    }

    #[tool(
        name = "workspace_cleanup",
        description = "Remove a managed workspace session directory, but only when it is under the configured root."
    )]
    fn workspace_cleanup(
        &self,
        Parameters(params): Parameters<CleanupParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let root = match params.root {
            Some(r) => PathBuf::from(r),
            None => default_workspace_root()
                .map_err(|e| ErrorData::internal_error(e.to_string(), None))?,
        };
        match workspaceops::cleanup_workspace_session(&PathBuf::from(&params.session_dir), &root) {
            Ok(()) => Ok(with_structured(
                format!("cleaned: {}\n", params.session_dir),
                &serde_json::json!({"cleaned": params.session_dir, "root": root}),
            )),
            Err(err) => Ok(CallToolResult::error(vec![ContentBlock::text(
                err.to_string(),
            )])),
        }
    }
}

#[rmcp::tool_handler]
impl ServerHandler for WorkspaceOpsServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new(
                env!("CARGO_PKG_NAME"),
                env!("CARGO_PKG_VERSION"),
            ))
            .with_instructions(
                "Managed workspace sessions and read-only sibling inventory for multi-checkout workflows.",
            )
    }
}

fn with_structured<T: serde::Serialize>(text: String, value: &T) -> CallToolResult {
    let mut result = CallToolResult::success(vec![ContentBlock::text(text)]);
    result.structured_content = serde_json::to_value(value).ok();
    result
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_new(&cli.log).unwrap_or_else(|_| EnvFilter::new("info")))
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();

    let service = WorkspaceOpsServer.serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
