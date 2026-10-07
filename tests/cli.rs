use std::process::{Command, Stdio};

#[test]
fn help_prints_usage_and_agent_flag() {
    let output = Command::new(env!("CARGO_BIN_EXE_woops"))
        .arg("--help")
        .stdin(Stdio::null())
        .output()
        .expect("spawn woops");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Usage: woops"), "{stdout}");
    assert!(stdout.contains("--agent"), "{stdout}");
}

#[test]
fn no_launch_mode_creates_then_cleans_workspace() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("workspaces");

    let linked_dir = tmp.path().join("ThunderForgeVTT-levels");
    std::fs::create_dir(&linked_dir).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_woops"))
        .arg("--no-launch")
        .arg("--agent")
        .arg("claude")
        .arg("--root")
        .arg(&root)
        .arg(&linked_dir)
        .current_dir(tmp.path())
        .stdin(Stdio::null())
        .output()
        .expect("spawn woops");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("woops lol"), "{stdout}");
    assert!(stdout.contains("cleaned:"), "{stdout}");

    let entries = std::fs::read_dir(&root).unwrap().count();
    assert_eq!(
        entries, 0,
        "workspace root should have no session directories"
    );
}

#[test]
fn keep_on_exit_writes_agents_md_with_linked_llm_hints() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("workspaces");

    let linked_dir = tmp.path().join("ThunderForgeVTT-levels");
    std::fs::create_dir(&linked_dir).unwrap();
    std::fs::write(linked_dir.join("llm.txt"), "model guidance\n").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_woops"))
        .arg("--no-launch")
        .arg("--keep-on-exit")
        .arg("--root")
        .arg(&root)
        .arg(&linked_dir)
        .current_dir(tmp.path())
        .stdin(Stdio::null())
        .output()
        .expect("spawn woops");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let kept_line = stdout
        .lines()
        .find(|line| line.starts_with("kept: "))
        .expect("expected kept line in output");
    let kept_path = kept_line.trim_start_matches("kept: ").trim();
    let agents = std::fs::read_to_string(format!("{kept_path}/AGENTS.md")).unwrap();
    assert!(agents.contains("ephemeral workspace"), "{agents}");
    assert!(agents.contains("will be removed"), "{agents}");
    assert!(agents.contains("ThunderForgeVTT-levels"), "{agents}");
    assert!(agents.contains("llm.txt"), "{agents}");
}

#[test]
fn missing_llm_txt_falls_back_to_claude_or_readme_style_files() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("workspaces");

    let linked_dir = tmp.path().join("ThunderForgeVTT-gitops");
    std::fs::create_dir(&linked_dir).unwrap();
    std::fs::write(linked_dir.join("CLAUDE.md"), "project guidance\n").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_woops"))
        .arg("--no-launch")
        .arg("--keep-on-exit")
        .arg("--root")
        .arg(&root)
        .arg(&linked_dir)
        .current_dir(tmp.path())
        .stdin(Stdio::null())
        .output()
        .expect("spawn woops");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let kept_line = stdout
        .lines()
        .find(|line| line.starts_with("kept: "))
        .expect("expected kept line in output");
    let kept_path = kept_line.trim_start_matches("kept: ").trim();
    let agents = std::fs::read_to_string(format!("{kept_path}/AGENTS.md")).unwrap();
    assert!(agents.contains("`llm.txt`: _(not found)_"), "{agents}");
    assert!(agents.contains("fallback:"), "{agents}");
    assert!(agents.contains("CLAUDE.md"), "{agents}");
}
