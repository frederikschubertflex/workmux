---
description: Send a message to an agent pane
---

# send

Send a message to the agent pane for a worktree.

```bash
workmux send [flags]
```

## Options

| Flag         | Description                                               |
| ------------ | --------------------------------------------------------- |
| `--handle`   | Worktree handle (defaults to current worktree if omitted) |
| `--pane-id`  | Target pane ID (required if multiple agent panes exist)   |
| `--message`  | Message to send (reads from stdin if omitted)             |
| `--command`  | Send as a shell command (single-line only)                |

## Examples

```bash
# Send a message to the agent pane for the current worktree
workmux send --message "Review the failing tests"

# Send a message to a specific worktree
workmux send --handle feature-login --message "Continue with auth flow"

# Send a shell command (single line)
workmux send --handle feature-login --command --message "!git status"

# Send multiline input from stdin
cat task.md | workmux send --handle feature-login
```
