use futures_util::{StreamExt, stream};
use tokio::io::AsyncReadExt;

use super::*;

/// Verify only Git administrative links, never working files or the index.
pub(super) async fn verified(
    worktrees: HashMap<PathBuf, TrackingWorktree>,
    common: &Path,
) -> HashMap<PathBuf, TrackingWorktree> {
    stream::iter(worktrees)
        .map(|(path, worktree)| async move {
            registered(&path, common, worktree.main)
                .await
                .then_some((path, worktree))
        })
        .buffer_unordered(4)
        .filter_map(async move |item| item)
        .collect()
        .await
}

pub(super) async fn registered(path: &Path, common: &Path, main: bool) -> bool {
    if tokio::fs::canonicalize(path).await.ok().as_deref() != Some(path) {
        return false;
    }
    let marker = path.join(".git");
    let Ok(metadata) = tokio::fs::symlink_metadata(&marker).await else {
        return false;
    };
    if metadata.is_dir() {
        return main && tokio::fs::canonicalize(&marker).await.ok().as_deref() == Some(common);
    }
    if !metadata.is_file() || metadata.len() > 8192 {
        return false;
    }
    let Some(value) = metadata_line(&marker).await else {
        return false;
    };
    let Some(directory) = value.strip_prefix("gitdir: ") else {
        return false;
    };
    let Ok(directory) = tokio::fs::canonicalize(path.join(directory)).await else {
        return false;
    };
    // Only the main worktree may use the common directory directly, including
    // submodules and --separate-git-dir roots. Linked entries need a backlink.
    if main {
        return directory == common;
    }
    if directory.parent() != Some(common.join("worktrees").as_path()) {
        return false;
    }
    let Some(common_directory) = metadata_line(&directory.join("commondir")).await else {
        return false;
    };
    if tokio::fs::canonicalize(directory.join(common_directory))
        .await
        .ok()
        .as_deref()
        != Some(common)
    {
        return false;
    }
    let Some(backlink) = metadata_line(&directory.join("gitdir")).await else {
        return false;
    };
    tokio::fs::canonicalize(directory.join(backlink))
        .await
        .ok()
        .as_deref()
        == Some(marker.as_path())
}

async fn metadata_line(path: &Path) -> Option<String> {
    let mut bytes = Vec::new();
    tokio::fs::File::open(path)
        .await
        .ok()?
        .take(8193)
        .read_to_end(&mut bytes)
        .await
        .ok()?;
    if bytes.len() > 8192 {
        return None;
    }
    let text = String::from_utf8(bytes).ok()?;
    let line = text.trim_end_matches(['\n', '\r']);
    (!line.is_empty() && !line.contains(['\0', '\n', '\r'])).then(|| line.to_owned())
}
