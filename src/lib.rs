use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, bail};
use serde::Serialize;

pub const DEFAULT_PREFIX: &str = "ThunderForgeVTT";

#[derive(Debug, Clone)]
pub struct WorkspaceSession {
    pub root: PathBuf,
    pub session_dir: PathBuf,
    pub linked_dirs: Vec<PathBuf>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkspaceInventory {
    pub base_dir: String,
    pub prefix: String,
    pub siblings: Vec<WorkspaceSibling>,
    pub links: Vec<WorkspaceLink>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkspaceSibling {
    pub name: String,
    pub path: String,
    pub has_git_dir: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkspaceLink {
    pub name: String,
    pub path: String,
    pub kind: String,
    pub target: Option<String>,
    pub resolved: Option<String>,
}

pub fn default_workspace_root() -> Result<PathBuf> {
    let home = std::env::var("HOME").context("HOME is not set")?;
    Ok(PathBuf::from(home).join(".workspaces"))
}

pub fn discover_sibling_dirs(cwd: &Path, prefix: &str) -> Result<Vec<PathBuf>> {
    let parent = cwd
        .parent()
        .context("cannot discover siblings from filesystem root")?;
    discover_sibling_dirs_in(parent, prefix)
}

fn discover_sibling_dirs_in(parent: &Path, prefix: &str) -> Result<Vec<PathBuf>> {
    let mut dirs: Vec<PathBuf> = fs::read_dir(parent)
        .with_context(|| format!("failed reading `{}`", parent.display()))?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| path.is_dir())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.starts_with(&format!("{prefix}-")))
                .unwrap_or(false)
        })
        .collect();
    dirs.sort();
    Ok(dirs)
}

pub fn inventory(
    cwd: &Path,
    prefix: &str,
    base_dir: Option<&Path>,
    inspect_links: &[String],
) -> Result<WorkspaceInventory> {
    let base = match base_dir {
        Some(path) if path.is_absolute() => path.to_path_buf(),
        Some(path) => cwd.join(path),
        None => cwd
            .parent()
            .context("cannot infer base directory from filesystem root")?
            .to_path_buf(),
    };

    let mut siblings = Vec::new();
    for dir in discover_sibling_dirs_in(&base, prefix)? {
        siblings.push(WorkspaceSibling {
            name: dir
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_owned(),
            path: dir.display().to_string(),
            has_git_dir: dir.join(".git").exists(),
        });
    }

    let mut links = Vec::new();
    for name in inspect_links {
        let path = cwd.join(name);
        match fs::symlink_metadata(&path) {
            Err(_) => links.push(WorkspaceLink {
                name: name.clone(),
                path: path.display().to_string(),
                kind: "missing".into(),
                target: None,
                resolved: None,
            }),
            Ok(meta) if meta.file_type().is_symlink() => links.push(WorkspaceLink {
                name: name.clone(),
                path: path.display().to_string(),
                kind: "symlink".into(),
                target: fs::read_link(&path).ok().map(|p| p.display().to_string()),
                resolved: fs::canonicalize(&path)
                    .ok()
                    .map(|p| p.display().to_string()),
            }),
            Ok(meta) if meta.is_dir() => links.push(WorkspaceLink {
                name: name.clone(),
                path: path.display().to_string(),
                kind: "directory".into(),
                target: None,
                resolved: fs::canonicalize(&path)
                    .ok()
                    .map(|p| p.display().to_string()),
            }),
            Ok(_) => links.push(WorkspaceLink {
                name: name.clone(),
                path: path.display().to_string(),
                kind: "file".into(),
                target: None,
                resolved: fs::canonicalize(&path)
                    .ok()
                    .map(|p| p.display().to_string()),
            }),
        }
    }

    Ok(WorkspaceInventory {
        base_dir: base.display().to_string(),
        prefix: prefix.to_owned(),
        siblings,
        links,
    })
}

pub fn create_workspace_session(root: &Path, dirs: &[PathBuf]) -> Result<WorkspaceSession> {
    if dirs.is_empty() {
        bail!("no directories were provided");
    }

    fs::create_dir_all(root)
        .with_context(|| format!("failed creating root `{}`", root.display()))?;

    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let session_dir = root.join(format!("session-{stamp}-{}", std::process::id()));
    fs::create_dir_all(&session_dir)
        .with_context(|| format!("failed creating `{}`", session_dir.display()))?;

    let mut linked_dirs = Vec::new();
    for dir in dirs {
        let canonical = fs::canonicalize(dir)
            .with_context(|| format!("failed resolving `{}`", dir.display()))?;
        if !canonical.is_dir() {
            bail!("`{}` is not a directory", canonical.display());
        }

        let link_name = canonical
            .file_name()
            .and_then(|n| n.to_str())
            .context("directory path has no terminal name")?;
        let link_path = session_dir.join(link_name);
        std::os::unix::fs::symlink(&canonical, &link_path).with_context(|| {
            format!(
                "failed creating symlink `{}` -> `{}`",
                link_path.display(),
                canonical.display()
            )
        })?;
        linked_dirs.push(canonical);
    }

    Ok(WorkspaceSession {
        root: root.to_path_buf(),
        session_dir,
        linked_dirs,
    })
}

pub fn cleanup_workspace_session(session_dir: &Path, root: &Path) -> Result<()> {
    let session_canon = fs::canonicalize(session_dir)
        .with_context(|| format!("failed resolving `{}`", session_dir.display()))?;
    let root_canon =
        fs::canonicalize(root).with_context(|| format!("failed resolving `{}`", root.display()))?;
    if !session_canon.starts_with(&root_canon) {
        bail!(
            "refusing to remove `{}` because it is outside root `{}`",
            session_canon.display(),
            root_canon.display()
        );
    }
    fs::remove_dir_all(&session_canon)
        .with_context(|| format!("failed removing `{}`", session_canon.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sibling_discovery_filters_by_prefix() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir(tmp.path().join("ThunderForgeVTT-levels")).unwrap();
        std::fs::create_dir(tmp.path().join("ThunderForgeVTT-ruler")).unwrap();
        std::fs::create_dir(tmp.path().join("ignore-me")).unwrap();
        std::fs::create_dir(tmp.path().join("repo")).unwrap();
        let dirs = discover_sibling_dirs(&tmp.path().join("repo"), DEFAULT_PREFIX).unwrap();
        let names: Vec<_> = dirs
            .into_iter()
            .map(|d| d.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        assert_eq!(
            names,
            vec!["ThunderForgeVTT-levels", "ThunderForgeVTT-ruler"]
        );
    }
}
