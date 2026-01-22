use anyhow::{Result, anyhow};
use std::path::Path;

use crate::{config, git, github, spinner, tmux};

use super::types::WorktreeInfo;

/// List all worktrees with their status
pub fn list(config: &config::Config, fetch_pr_status: bool) -> Result<Vec<WorktreeInfo>> {
    let repo_root = git::get_repo_root()?;
    list_in_repo(&repo_root, config, fetch_pr_status)
}

pub fn list_in_repo(
    repo_root: &Path,
    config: &config::Config,
    fetch_pr_status: bool,
) -> Result<Vec<WorktreeInfo>> {
    if !git::is_git_repo_in(repo_root)? {
        return Err(anyhow!(
            "Not in a git repository: {}",
            repo_root.display()
        ));
    }

    let worktrees_data = git::list_worktrees_in(repo_root)?;

    if worktrees_data.is_empty() {
        return Ok(Vec::new());
    }

    // Check tmux status and get all windows once to avoid repeated process calls
    let tmux_windows: std::collections::HashSet<String> = if tmux::is_running().unwrap_or(false) {
        tmux::get_all_window_names().unwrap_or_default()
    } else {
        std::collections::HashSet::new()
    };

    // Get the main branch for unmerged checks
    let main_branch = git::get_default_branch_in(Some(repo_root)).ok();

    // Get all unmerged branches in one go for efficiency
    // Prefer checking against remote tracking branch for more accurate results
    let unmerged_branches = main_branch
        .as_deref()
        .and_then(|main| git::get_merge_base_in(main, Some(repo_root)).ok())
        .and_then(|base| git::get_unmerged_branches_in(&base, Some(repo_root)).ok())
        .unwrap_or_default(); // Use an empty set on failure

    // Batch fetch all PRs if requested (single API call)
    let pr_map = if fetch_pr_status {
        spinner::with_spinner("Fetching PR status", || {
            Ok(github::list_prs_in(Some(repo_root)).unwrap_or_default())
        })?
    } else {
        std::collections::HashMap::new()
    };

    let prefix = config.window_prefix();
    let worktrees: Vec<WorktreeInfo> = worktrees_data
        .into_iter()
        .map(|(path, branch)| {
            // Extract handle from worktree path basename (the source of truth)
            let handle = path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or(&branch)
                .to_string();

            // Use handle for tmux window check, not branch name
            let prefixed_window_name = tmux::prefixed(prefix, &handle);
            let has_tmux = tmux_windows
                .iter()
                .any(|name| tmux::window_matches_handle(name, &handle, &prefixed_window_name));

            // Check for unmerged commits, but only if this isn't the main branch
            let has_unmerged = if let Some(ref main) = main_branch {
                if branch == *main || branch == "(detached)" {
                    false
                } else {
                    unmerged_branches.contains(&branch)
                }
            } else {
                false
            };

            // Lookup PR info from batch fetch
            let pr_info = pr_map.get(&branch).cloned();

            WorktreeInfo {
                branch,
                handle,
                path,
                has_tmux,
                has_unmerged,
                pr_info,
            }
        })
        .collect();

    Ok(worktrees)
}
