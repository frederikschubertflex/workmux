---
description: Capture output from an agent pane
---

# capture

Capture recent output from the agent pane for a worktree.

```bash
workmux capture [flags]
```

## Options

| Flag         | Description                                               |
| ------------ | --------------------------------------------------------- |
| `--handle`   | Worktree handle (defaults to current worktree if omitted) |
| `--pane-id`  | Target pane ID (required if multiple agent panes exist)   |
| `--lines`    | Number of lines to capture (default: 800)                 |
| `--ansi`     | Preserve ANSI colors in output                            |

## Examples

```bash
# Capture last 800 lines from current worktree's agent pane
workmux capture

# Capture 200 lines from a specific worktree
workmux capture --handle feature-login --lines 200

# Capture output with ANSI colors
workmux capture --handle feature-login --ansi
```
