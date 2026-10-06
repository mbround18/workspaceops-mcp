# workspaceops

`woops` creates a managed workspace session under `~/.workspaces/*`, symlinks target directories into it, launches your chosen coding agent CLI, and cleans up the session directory automatically when you exit (including Ctrl+C).
The repository also ships `workspaceops-mcp`, an MCP server exposing workspace inventory/prepare/cleanup tools over stdio.

## Install

```bash
cargo install --path .
```

To run the MCP server binary directly:

```bash
cargo run --bin workspaceops-mcp
```

## Usage

```bash
woops [DIR ...] [--agent claude]
```

### Behavior

- Prints `woops lol` on startup.
- If `DIR ...` is omitted, discovers sibling directories matching `--prefix` (default `ThunderForgeVTT`).
- Creates links in a new session directory under `--root` (default `~/.workspaces`).
- Launches the single agent command from `--agent`.
- On Ctrl+C, terminates the launched agent process and removes the session directory.

### Helpful flags

- `--agent <cli>`: one agent command, e.g. `claude`, `copilot`, `gemini`, `whateverthecliis`
- `--no-launch`: prepare and clean a session without launching an agent (useful for testing)
- `--keep-on-exit`: skip cleanup and preserve the session directory
- `--root <path>`: override workspace root
- `--prefix <name>`: sibling discovery prefix for no-arg mode
