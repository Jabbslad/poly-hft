# Agent Instructions

This project runs **multiple AI agents simultaneously** using git worktrees for code isolation and **bd** (beads) for coordinated task management.

**You are one of several agents.** Other agents are working in parallel. Follow these rules to avoid conflicts.

## Environment Setup

Each agent runs in its own git worktree. **Disable the beads daemon**:

```bash
export BEADS_NO_DAEMON=1
```

## Claiming Work (MANDATORY)

**NEVER start work without claiming the task first.** Other agents are working concurrently.

```bash
# 1. Sync to see latest state
bd sync

# 2. Find unclaimed work
bd ready

# 3. Check what others are working on
bd list --status in_progress

# 4. Claim the task with YOUR agent name
bd update <id> --status in_progress --assignee "<your-agent-name>"

# 5. Verify your claim before starting
bd show <id>
```

**If a task is already claimed by another agent, pick a different task.**

## Quick Reference

```bash
bd ready                          # Find available work
bd list --status in_progress      # See what's being worked on
bd update <id> --status in_progress --assignee "agent-1"  # Claim work
bd show <id>                      # View issue details
bd comments add <id> "message"    # Coordinate with other agents
bd close <id> --reason "..."      # Complete work
bd sync                           # Sync with git (do this often!)
```

## During Work

- **Commit frequently** - Smaller commits reduce merge conflicts
- **Sync often** - Run `bd sync` periodically to stay current with other agents
- **Stay in your lane** - Only work on files related to your claimed task
- **Communicate via comments** - If you need to touch shared files:
  ```bash
  bd comments add <id> "Modifying src/config.rs - other agents avoid"
  ```

## Landing the Plane (Session Completion)

**When ending a work session**, you MUST complete ALL steps below. Work is NOT complete until `git push` succeeds.

**MANDATORY WORKFLOW:**

1. **File issues for remaining work** - Create issues for anything that needs follow-up
2. **Run quality gates** (if code changed) - Tests, linters, builds
3. **Update issue status** - Close finished work, update in-progress items
4. **SYNC AND PUSH TO REMOTE** - This is MANDATORY:
   ```bash
   bd sync                    # Export your task updates
   git pull --rebase          # Get others' changes
   git push
   git status                 # MUST show "up to date with origin"
   ```
5. **Clean up** - Clear stashes, prune remote branches
6. **Verify** - All changes committed AND pushed

**CRITICAL RULES:**
- Work is NOT complete until `git push` succeeds
- NEVER stop before pushing - that leaves work stranded locally
- NEVER say "ready to push when you are" - YOU must push
- If push fails, resolve conflicts and retry until it succeeds
- Always `bd sync` before pushing to share your task state

## Conflict Prevention

| Conflict Type | Prevention |
|---------------|------------|
| Task conflicts | Always claim with `--assignee` before starting |
| Code conflicts | Work on different files; commit & push frequently |
| Merge conflicts | `git pull --rebase` often; keep changes small |
| Beads conflicts | `bd sync` frequently; hash-based IDs prevent ID collisions |

## Detailed Guide

For comprehensive multi-agent patterns, see [docs/MULTI_AGENT_WORKFLOW.md](docs/MULTI_AGENT_WORKFLOW.md).
