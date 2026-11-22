use super::*;

fn invalid() -> ProbeError {
    ProbeError::InvalidOutput
}

pub(super) fn branches(mut text: &str) -> Result<HashMap<String, TrackingBranch>, ProbeError> {
    let mut branches = HashMap::new();
    while !text.is_empty() {
        let mut field = || {
            let (value, rest) = text.split_once('\0').ok_or_else(invalid)?;
            text = rest;
            Ok::<_, ProbeError>(value)
        };
        let reference = field()?;
        let commit = field()?;
        let symbolic = field()?;
        let upstream = field()?;
        let remote = field()?;
        let track = field()?;
        text = text.strip_prefix('\n').ok_or_else(invalid)?;
        if !reference.starts_with("refs/heads/") || validate_object_id(commit).is_err() {
            return Err(invalid());
        }
        if !symbolic.is_empty() {
            continue;
        }
        let mut tracking = tracking(upstream, remote, track)?;
        tracking.observed_at_ms = None;
        if branches
            .insert(
                reference.to_owned(),
                TrackingBranch {
                    commit: commit.to_owned(),
                    tracking,
                },
            )
            .is_some()
        {
            return Err(invalid());
        }
    }
    Ok(branches)
}

fn tracking(upstream: &str, remote: &str, track: &str) -> Result<GitTracking, ProbeError> {
    if upstream.is_empty() {
        return if track.is_empty() && remote.is_empty() {
            Ok(GitTracking::without_counts(GitTrackingState::NoUpstream))
        } else {
            Err(invalid())
        };
    }
    if !upstream.starts_with("refs/") || upstream.chars().any(char::is_control) {
        return Err(invalid());
    }
    let mut result = GitTracking::unknown();
    result.upstream = Some(upstream.to_owned());
    result.remote = (!remote.is_empty() && remote != ".").then(|| remote.to_owned());
    if track == "gone" {
        result.state = GitTrackingState::UpstreamMissing;
        return Ok(result);
    }
    let (mut ahead, mut behind) = (None, None);
    for part in track.split(", ").filter(|part| !part.is_empty()) {
        let (kind, value) = part.split_once(' ').ok_or_else(invalid)?;
        let value = value.parse::<u64>().map_err(|_| invalid())?;
        let slot = match kind {
            "ahead" => &mut ahead,
            "behind" => &mut behind,
            _ => return Err(invalid()),
        };
        if slot.replace(value).is_some() {
            return Err(invalid());
        }
    }
    let (ahead, behind) = (ahead.unwrap_or(0), behind.unwrap_or(0));
    result.state = GitTracking::counts_state(ahead, behind);
    result.ahead_by = Some(ahead);
    result.behind_by = Some(behind);
    Ok(result)
}

pub(super) fn worktrees(text: &str) -> Result<HashMap<PathBuf, TrackingWorktree>, ProbeError> {
    let mut worktrees = HashMap::new();
    // Git's stable porcelain order lists the main worktree first, even when
    // bare or unborn; filtering those records must not promote a linked one.
    for (index, record) in text
        .split("\0\0")
        .filter(|record| !record.is_empty())
        .enumerate()
    {
        let (mut path, mut commit, mut branch, mut detached, mut bare) =
            (None, None, None, false, false);
        for field in record.split('\0').filter(|field| !field.is_empty()) {
            if let Some(value) = field.strip_prefix("worktree ") {
                if path.replace(PathBuf::from(value)).is_some() || !Path::new(value).is_absolute() {
                    return Err(invalid());
                }
            } else if let Some(value) = field.strip_prefix("HEAD ") {
                if commit.replace(value.to_owned()).is_some() || validate_object_id(value).is_err()
                {
                    return Err(invalid());
                }
            } else if let Some(value) = field.strip_prefix("branch ") {
                if branch.replace(value.to_owned()).is_some() || !value.starts_with("refs/heads/") {
                    return Err(invalid());
                }
            } else if field == "detached" {
                detached = true;
            } else if field == "bare" {
                bare = true;
            }
        }
        let path = path.ok_or_else(invalid)?;
        if bare
            || commit
                .as_deref()
                .is_some_and(|head| head.bytes().all(|byte| byte == b'0'))
        {
            continue;
        }
        if detached == branch.is_some() {
            return Err(invalid());
        }
        let worktree = TrackingWorktree {
            commit: commit.ok_or_else(invalid)?,
            branch,
            main: index == 0,
        };
        if worktrees.insert(path, worktree).is_some() {
            return Err(invalid());
        }
    }
    Ok(worktrees)
}
