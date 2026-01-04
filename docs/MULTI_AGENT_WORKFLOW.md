# Multi-Agent Workflow Guide

Coordinate multiple Claude Code agents working concurrently on the same codebase using beads for task management and git worktrees for code isolation.

## Overview

**Pattern**: Each agent operates in its own git worktree while sharing a common beads task database. Agents claim tasks via the `assignee` field to prevent duplicate work.

**Key Benefits**:
- Code isolation: Each agent has its own working directory
- Shared visibility: All agents see the same task list
- No ID collisions: Beads uses hash-based IDs (`bd-a1b2`)

## Prerequisites

1. Beads initialized: `bd init`
2. Git repository with worktree support
3. Understanding of your project's module boundaries (for task assignment)

## Setup

### 1. Create Worktrees for Each Agent

```bash
# From main repository
git worktree add ../agent-1-worktree -b feature/agent-1-work
git worktree add ../agent-2-worktree -b feature/agent-2-work
```

### 2. Disable Daemon in Worktrees

The beads daemon has compatibility issues with shared databases across worktrees. Disable it:

```bash
# Option A: Environment variable (recommended)
export BEADS_NO_DAEMON=1

# Option B: Per-command
bd --no-daemon <command>
```

Add to each agent's shell config or session startup.

### 3. Optional: Configure Sync Branch

If your `main` branch is protected:

```bash
bd config set sync-branch beads-sync
```

This commits beads data to a separate branch, keeping it out of code PRs.

## Agent Workflow

### Starting a Session

```bash
# 1. Navigate to your worktree
cd ../agent-1-worktree

# 2. Pull latest and sync beads
git pull origin main
bd sync

# 3. Find available tasks
bd ready --json
```

### Claiming a Task

**Always claim before working** to prevent duplicate effort:

```bash
# Claim task with your agent name
bd update bd-42 --status in_progress --assignee "agent-1"

# Verify your claim
bd show bd-42
```

### During Work

```bash
# Commit frequently (every logical change)
git add . && git commit -m "bd-42: Implement feature X"

# Check what others are working on
bd list --status in_progress
```

### Completing a Task

```bash
# 1. Run tests/linting if code changed
cargo test && cargo clippy

# 2. Close the task
bd close bd-42 --reason "Implemented feature X with tests"

# 3. Sync beads to git
bd sync

# 4. Push your branch
git push -u origin feature/agent-1-work

# 5. Create PR or merge as appropriate
```

## Preventing Conflicts

### Task Conflicts
- **Always claim with assignee** before starting work
- Check `bd list --status in_progress` to see active work
- Use labels to segment task types to specific agents

### Code Conflicts
- **Assign tasks that touch different files/modules**
- Keep tasks small and focused
- Merge to main frequently via PRs
- If overlap is unavoidable, coordinate via task comments:
  ```bash
  bd comments add bd-42 "Working on src/model/ - agent-1"
  ```

### Beads Data Conflicts
- Hash-based IDs eliminate ID collisions
- `bd sync` frequently to stay current
- Last-write-wins for same-issue edits (rare if claiming properly)

## Quick Reference

| Action | Command |
|--------|---------|
| Find available tasks | `bd ready` |
| Claim a task | `bd update <id> --status in_progress --assignee "agent-1"` |
| See my tasks | `bd list --assignee "agent-1"` |
| See all active work | `bd list --status in_progress` |
| Add task comment | `bd comments add <id> "message"` |
| Complete task | `bd close <id> --reason "..."` |
| Sync to git | `bd sync` |
| Create subtask | `bd create "Subtask" --discovered-from <parent-id>` |

## Troubleshooting

### "Database locked" errors
Ensure daemon is disabled in worktrees:
```bash
export BEADS_NO_DAEMON=1
```

### Missing issues after pull
Run sync to import from JSONL:
```bash
bd sync
```

### Branch conflicts with beads-sync
If using sync branch and it conflicts:
```bash
git checkout beads-sync
git pull --rebase origin beads-sync
git push
```

## References

- [Beads Documentation](https://github.com/steveyegge/beads)
- [AGENTS.md](../AGENTS.md) - Session completion rules
- [CLAUDE.md](../CLAUDE.md) - Project-specific instructions
