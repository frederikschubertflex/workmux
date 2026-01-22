use crate::{config, git, verbosity, workflow};
use anyhow::{Result, anyhow};
use std::path::Path;
use tabled::{
    Table, Tabled,
    settings::{Padding, Style, disable::Remove, object::Columns},
};

#[derive(Tabled)]
struct WorktreeRow {
    #[tabled(rename = "REPO")]
    repo: String,
    #[tabled(rename = "HANDLE")]
    handle: String,
    #[tabled(rename = "BRANCH")]
    branch: String,
    #[tabled(rename = "STATE")]
    state: String,
    #[tabled(rename = "PR")]
    pr_status: String,
    #[tabled(rename = "TMUX")]
    tmux_status: String,
    #[tabled(rename = "PATH")]
    path_str: String,
}

fn format_pr_status(pr_info: Option<crate::github::PrSummary>) -> String {
    pr_info
        .map(|pr| {
            let label = match pr.state.as_str() {
                "OPEN" if pr.is_draft => "draft",
                "OPEN" => "open",
                "MERGED" => "merged",
                "CLOSED" => "closed",
                _ => "unknown",
            };
            format!("#{} {}", pr.number, label)
        })
        .unwrap_or_else(|| "-".to_string())
}

pub fn run(show_pr: bool, show_all: bool) -> Result<()> {
    let config = config::Config::load(None)?;
    let mut rows: Vec<WorktreeRow> = Vec::new();

    if let Some(repo_patterns) = config.repo_paths.as_ref() {
        let expanded = config::expand_repo_paths(repo_patterns)?;
        for pattern in expanded.unmatched_patterns {
            if verbosity::is_verbose() {
                eprintln!(
                    "workmux: repo_paths pattern '{}' did not match any paths",
                    pattern
                );
            }
        }

        if expanded.paths.is_empty() {
            return Err(anyhow!(
                "repo_paths is set but no repositories matched the configured patterns"
            ));
        }

        let mut has_repo = false;
        for repo_root in expanded.paths {
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
            has_repo = true;
            let repo_config = config::Config::load_for_repo_root(&repo_root, None)?;
            let worktrees = workflow::list_in_repo(&repo_root, &repo_config, show_pr)?;
            rows.extend(build_rows(
                &repo_root,
                worktrees,
                show_all,
                show_pr,
            ));
        }

        if !has_repo {
            return Err(anyhow!(
                "repo_paths did not yield any valid git repositories"
            ));
        }
    } else {
        let repo_root = git::get_repo_root()?;
        let worktrees = workflow::list(&config, show_pr)?;
        rows.extend(build_rows(
            &repo_root,
            worktrees,
            show_all,
            show_pr,
        ));
    }

    if rows.is_empty() {
        if show_all {
            println!("No worktrees found");
        } else {
            println!("No active worktrees found");
        }
        return Ok(());
    }

    let mut table = Table::new(rows);
    table
        .with(Style::blank())
        .modify(Columns::new(0..7), Padding::new(0, 1, 0, 0));

    // Hide PR column if --pr flag not used
    if !show_pr {
        table.with(Remove::column(Columns::new(4..5)));
    }

    println!("{table}");

    Ok(())
}

fn build_rows(
    repo_root: &Path,
    worktrees: Vec<workflow::types::WorktreeInfo>,
    show_all: bool,
    show_pr: bool,
) -> Vec<WorktreeRow> {
    let repo_label = format_repo_label(repo_root);
    worktrees
        .into_iter()
        .filter(|wt| show_all || wt.has_tmux)
        .map(|wt| WorktreeRow {
            repo: repo_label.clone(),
            handle: wt.handle,
            branch: wt.branch,
            state: if wt.has_tmux {
                "active".to_string()
            } else {
                "inactive".to_string()
            },
            pr_status: if show_pr {
                format_pr_status(wt.pr_info)
            } else {
                String::new()
            },
            tmux_status: if wt.has_tmux { "1".to_string() } else { "0".to_string() },
            path_str: format_path(&wt.path),
        })
        .collect()
}

fn format_repo_label(repo_root: &Path) -> String {
    repo_root
        .file_name()
        .and_then(|n| n.to_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| repo_root.display().to_string())
}

fn format_path(path: &Path) -> String {
    if let Some(home_dir) = home::home_dir()
        && let Ok(stripped) = path.strip_prefix(&home_dir)
    {
        if stripped.as_os_str().is_empty() {
            return "~".to_string();
        }
        return format!("~/{}", stripped.display());
    }
    path.display().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::github::PrSummary;
    use std::path::PathBuf;

    fn pr(state: &str, is_draft: bool) -> PrSummary {
        PrSummary {
            number: 42,
            title: String::new(),
            state: state.to_string(),
            is_draft,
        }
    }

    #[test]
    fn test_format_pr_status_variants() {
        assert_eq!(format_pr_status(Some(pr("OPEN", true))), "#42 draft");
        assert_eq!(format_pr_status(Some(pr("OPEN", false))), "#42 open");
        assert_eq!(format_pr_status(Some(pr("MERGED", false))), "#42 merged");
        assert_eq!(format_pr_status(Some(pr("CLOSED", false))), "#42 closed");
        assert_eq!(format_pr_status(Some(pr("OTHER", false))), "#42 unknown");
        assert_eq!(format_pr_status(None), "-");
    }

    #[test]
    fn test_build_rows_filters_active() {
        let repo_root = Path::new("/tmp/repo");
        let active = workflow::types::WorktreeInfo {
            branch: "main".to_string(),
            handle: "active".to_string(),
            path: repo_root.join("active"),
            has_tmux: true,
            has_unmerged: false,
            pr_info: None,
        };
        let inactive = workflow::types::WorktreeInfo {
            branch: "dev".to_string(),
            handle: "inactive".to_string(),
            path: repo_root.join("inactive"),
            has_tmux: false,
            has_unmerged: false,
            pr_info: None,
        };

        let rows = build_rows(repo_root, vec![active, inactive], false, false);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].handle, "active");
        assert_eq!(rows[0].state, "active");
        assert_eq!(rows[0].tmux_status, "1");
    }

    #[test]
    fn test_format_path_home() {
        let Some(home_dir) = home::home_dir() else {
            return;
        };
        let path = home_dir.join("repos").join("workmux");
        assert_eq!(format_path(&path), "~/repos/workmux");
    }

    #[test]
    fn test_format_path_absolute() {
        let path = PathBuf::from("/tmp/workmux");
        assert_eq!(format_path(&path), "/tmp/workmux");
    }
}
