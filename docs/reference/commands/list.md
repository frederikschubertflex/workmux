---
description: List all git worktrees with their tmux window and merge status
---

# list

Lists git worktrees with their tmux window status. Alias: `ls`. Defaults to all worktrees.

```bash
workmux list [flags]
```

## Options

| Flag        | Description                                                                                         |
| ----------- | --------------------------------------------------------------------------------------------------- |
| `--pr`      | Show GitHub PR status for each worktree. Requires the `gh` CLI to be installed and authenticated.  |
| `--all`     | Show all worktrees (active and inactive) (default).                                                |
| `--active`  | Show only active worktrees.                                                                        |

## Examples

```bash
# List all worktrees (default)
workmux list

# List with PR status
workmux list --pr

# List only active worktrees
workmux list --active
```

## Example output

```
REPO    HANDLE      BRANCH      STATE     TMUX    PATH
----    ------      ------      -----     ----    ----
project project     main        inactive  0       ~/project
project user-auth   user-auth   active    1       ~/project__worktrees/user-auth
project bug-fix     bug-fix     active    1       ~/project__worktrees/bug-fix
```

## Key

- `STATE=active` means a tmux window exists for this worktree.
- `TMUX=1` means a tmux window exists, `TMUX=0` means none.

### Multi-repo

Set `repo_paths` in `~/.config/workmux/config.yaml` to list across multiple repositories.
# List all worktrees across configured repos
workmux list --all
