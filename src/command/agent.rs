use anyhow::{Result, anyhow};
use std::path::PathBuf;

use crate::{config, git, tmux};

pub struct AgentPaneTarget {
    pub pane_id: String,
    pub agent: Option<String>,
}

struct Candidate {
    pane_id: String,
    session: String,
    window_name: String,
    current_command: String,
    status: Option<String>,
    pane_role: Option<String>,
    agent: Option<String>,
}

pub fn resolve_agent_pane(handle: &str, pane_id: Option<&str>) -> Result<AgentPaneTarget> {
    let base_config = config::Config::load(None)?;
    let repo_roots = resolve_repo_roots(&base_config)?;
    let panes = tmux::list_panes()?;

    if panes.is_empty() {
        return Err(anyhow!("No tmux panes found. Is tmux running?"));
    }

    let mut candidates: Vec<Candidate> = Vec::new();
    let mut seen_panes: std::collections::HashSet<String> = std::collections::HashSet::new();

    for repo_root in repo_roots {
        let repo_config = config::Config::load_for_repo_root(&repo_root, None)?;
        let prefixed_window_name = tmux::prefixed(repo_config.window_prefix(), handle);

        for pane in panes.iter().filter(|p| {
            tmux::window_matches_handle(&p.window_name, handle, &prefixed_window_name)
        }) {
            if !seen_panes.insert(pane.pane_id.clone()) {
                continue;
            }
            candidates.push(Candidate {
                pane_id: pane.pane_id.clone(),
                session: pane.session.clone(),
                window_name: pane.window_name.clone(),
                current_command: pane.current_command.clone(),
                status: pane.status.clone(),
                pane_role: pane.pane_role.clone(),
                agent: repo_config.agent.clone(),
            });
        }
    }

    if candidates.is_empty() {
        return Err(anyhow!(
            "No agent panes found for handle '{}'. Use `workmux list --all` to check handles.",
            handle
        ));
    }

    if let Some(requested) = pane_id {
        let matching = candidates
            .into_iter()
            .find(|candidate| candidate.pane_id == requested);

        let Some(candidate) = matching else {
            return Err(anyhow!(
                "Pane id '{}' not found for handle '{}'",
                requested,
                handle
            ));
        };

        return Ok(AgentPaneTarget {
            pane_id: candidate.pane_id,
            agent: candidate.agent,
        });
    }

    let mut agent_candidates: Vec<Candidate> =
        candidates.into_iter().filter(is_agent_candidate).collect();

    if agent_candidates.is_empty() {
        return Err(anyhow!(
            "No agent panes found for handle '{}'. Use `workmux list --all` to check handles.",
            handle
        ));
    }

    if agent_candidates.len() > 1 {
        let mut message = format!(
            "Multiple agent panes found for handle '{}'. Re-run with --pane-id.\n",
            handle
        );
        for candidate in agent_candidates {
            let status = candidate
                .status
                .as_deref()
                .unwrap_or("-");
            message.push_str(&format!(
                "  pane_id={} session={} window={} status={} cmd={}\n",
                candidate.pane_id,
                candidate.session,
                candidate.window_name,
                status,
                candidate.current_command
            ));
        }
        return Err(anyhow!(message));
    }

    let candidate = agent_candidates
        .pop()
        .ok_or_else(|| anyhow!("No agent panes found for handle '{}'", handle))?;

    Ok(AgentPaneTarget {
        pane_id: candidate.pane_id,
        agent: candidate.agent,
    })
}

fn is_agent_candidate(candidate: &Candidate) -> bool {
    candidate
        .pane_role
        .as_deref()
        .is_some_and(|role| role == "agent")
        || candidate.status.is_some()
        || candidate
            .agent
            .as_deref()
            .is_some_and(|agent| config::is_agent_command(&candidate.current_command, agent))
}

fn resolve_repo_roots(config: &config::Config) -> Result<Vec<PathBuf>> {
    if let Some(repo_patterns) = config.repo_paths.as_ref() {
        let expanded = config::expand_repo_paths(repo_patterns)?;
        for pattern in expanded.unmatched_patterns {
            eprintln!("workmux: repo_paths pattern '{}' did not match any paths", pattern);
        }

        if expanded.paths.is_empty() {
            return Err(anyhow!(
                "repo_paths is set but no repositories matched the configured patterns"
            ));
        }

        let mut roots = Vec::new();
        for repo_root in expanded.paths {
            if !repo_root.exists() {
                eprintln!(
                    "workmux: repo_paths entry '{}' does not exist; skipping",
                    repo_root.display()
                );
                continue;
            }
            if !repo_root.is_dir() {
                eprintln!(
                    "workmux: repo_paths entry '{}' is not a directory; skipping",
                    repo_root.display()
                );
                continue;
            }
            if !git::is_git_repo_in(&repo_root)? {
                eprintln!(
                    "workmux: repo_paths entry '{}' is not a git repository; skipping",
                    repo_root.display()
                );
                continue;
            }
            roots.push(repo_root);
        }

        if roots.is_empty() {
            return Err(anyhow!(
                "repo_paths did not yield any valid git repositories"
            ));
        }

        Ok(roots)
    } else {
        Ok(vec![git::get_repo_root()?])
    }
}
