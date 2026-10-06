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
