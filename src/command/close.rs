use crate::{config, git, tmux, verbosity};
use anyhow::{Context, Result, anyhow};
use std::path::{Path, PathBuf};

pub fn run(name: Option<&str>, repo: Option<&str>) -> Result<()> {
    let config = config::Config::load(None)?;

    // When no name is provided, prefer the current tmux window name
    // This handles duplicate windows (e.g., wm:feature-2) correctly
    let (full_window_name, is_current_window) = match name {
        Some(handle) => {
            let target = resolve_worktree_target(handle, repo, &config)?;
            let prefixed = tmux::prefixed(target.prefix.as_str(), handle);
            let window_name = resolve_window_name(handle, &prefixed)?;
            let current_window = tmux::current_window_name()?;
            let is_current = current_window.as_deref() == Some(&window_name);
            (window_name, is_current)
        }
        None => {
            let prefix = config.window_prefix();
            // No name provided - check if we're in a workmux window
            if let Some(current) = tmux::current_window_name()? {
                if current.starts_with(prefix) {
                    // We're in a workmux window, use it directly
                    (current.clone(), true)
                } else {
                    // Not in a workmux window, fall back to directory name
                    let handle = super::resolve_name(None)?;
                    (tmux::prefixed(prefix, &handle), false)
                }
            } else {
                // Not in tmux, use directory name
                let handle = super::resolve_name(None)?;
                (tmux::prefixed(prefix, &handle), false)
            }
        }
    };

    // Check if the tmux window exists
    if !tmux::window_exists_by_full_name(&full_window_name)? {
        return Err(anyhow!(
            "No active tmux window found for '{}'. The worktree exists but has no open window.",
            full_window_name
        ));
    }

    if is_current_window {
        // Schedule the window close with a small delay so the command can complete
        tmux::schedule_window_close_by_full_name(
            &full_window_name,
            std::time::Duration::from_millis(100),
        )?;
    } else {
        // Kill the window directly
        tmux::kill_window_by_full_name(&full_window_name).context("Failed to close tmux window")?;
        println!("✓ Closed window '{}' (worktree kept)", full_window_name);
    }

    Ok(())
}

struct CloseTarget {
    repo_root: PathBuf,
    prefix: String,
}

fn resolve_worktree_target(
    handle: &str,
    repo_filter: Option<&str>,
    config: &config::Config,
) -> Result<CloseTarget> {
    let repo_roots = resolve_repo_roots(config, repo_filter)?;
    let mut matches = Vec::new();

    for repo_root in repo_roots {
        let repo_config = config::Config::load_for_repo_root(&repo_root, None)?;
        let worktrees = git::list_worktrees_in(&repo_root)?;
        let mut found = false;
        for (path, _branch) in worktrees {
            if let Some(name) = path.file_name().and_then(|n| n.to_str())
                && name == handle
            {
                found = true;
                break;
            }
        }
        if found {
            matches.push(CloseTarget {
                repo_root,
                prefix: repo_config.window_prefix().to_string(),
            });
        }
    }

    if matches.is_empty() {
        return Err(anyhow!(
            "No worktree found with name '{}'. Use 'workmux list' to see available worktrees.",
            handle
        ));
    }

    if matches.len() > 1 {
        let mut message = format!(
            "Multiple worktrees named '{}'. Re-run with --repo.\n",
            handle
        );
        for target in matches {
            let label = format_repo_label(&target.repo_root);
            message.push_str(&format!(
                "  repo={} path={}\n",
                label,
                target.repo_root.display()
            ));
        }
        return Err(anyhow!(message));
    }

    Ok(matches.remove(0))
}

fn resolve_repo_roots(config: &config::Config, repo_filter: Option<&str>) -> Result<Vec<PathBuf>> {
    let roots = if let Some(repo_patterns) = config.repo_paths.as_ref() {
        let expanded = config::expand_repo_paths(repo_patterns)?;
        for pattern in expanded.unmatched_patterns {
            if verbosity::is_verbose() {
                eprintln!(
                    "workmux: repo_paths pattern '{}' did not match any paths",
                    pattern
                );
            }
        }
        expanded.paths
    } else {
        if repo_filter.is_some() {
            return Err(anyhow!(
                "--repo requires repo_paths to be configured in ~/.config/workmux/config.yaml"
            ));
        }
        vec![git::get_repo_root()?]
    };

    let mut filtered = Vec::new();
    let mut has_repo = false;
    for repo_root in roots {
        if !repo_root.exists() {
            if verbosity::is_verbose() {
                eprintln!(
                    "workmux: repo_paths entry '{}' does not exist; skipping",
                    repo_root.display()
                );
            }
            continue;
        }
        if !repo_root.is_dir() {
            if verbosity::is_verbose() {
                eprintln!(
                    "workmux: repo_paths entry '{}' is not a directory; skipping",
                    repo_root.display()
                );
            }
            continue;
        }
        if !git::is_git_repo_in(&repo_root)? {
            if verbosity::is_verbose() {
                eprintln!(
                    "workmux: repo_paths entry '{}' is not a git repository; skipping",
                    repo_root.display()
                );
            }
            continue;
        }
        if let Some(filter) = repo_filter {
            if !repo_matches_filter(&repo_root, filter) {
                continue;
            }
        }
        has_repo = true;
        filtered.push(repo_root);
    }

    if !has_repo {
        return Err(anyhow!(
            "repo_paths did not yield any valid git repositories"
        ));
    }

    if filtered.is_empty() {
        return Err(anyhow!(
            "No repositories matched --repo '{}'",
            repo_filter.unwrap_or("")
        ));
    }

    Ok(filtered)
}

fn repo_matches_filter(repo_root: &Path, filter: &str) -> bool {
    let label = format_repo_label(repo_root);
    if filter == label {
        return true;
    }
    filter == repo_root.display().to_string()
}

fn format_repo_label(repo_root: &Path) -> String {
    repo_root
        .file_name()
        .and_then(|n| n.to_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| repo_root.display().to_string())
}

fn resolve_window_name(handle: &str, prefixed: &str) -> Result<String> {
    let windows = tmux::get_all_window_names()?;
    let mut matches: Vec<String> = windows
        .into_iter()
        .filter(|name| tmux::window_matches_handle(name, handle, prefixed))
        .collect();

    if matches.is_empty() {
        return Err(anyhow!(
            "No active tmux window found for '{}'. The worktree exists but has no open window.",
            handle
        ));
    }

    if matches.len() > 1 {
        let mut message = format!(
            "Multiple tmux windows matched '{}'. Re-run with --repo or close manually.\n",
            handle
        );
        for name in matches.iter() {
            message.push_str(&format!("  window={}\n", name));
        }
        return Err(anyhow!(message));
    }

    Ok(matches.remove(0))
}
