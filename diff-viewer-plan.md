# Diff Viewer — See What Claude Changed in Container Sessions

## Context

When Claude runs autonomously inside a Docker container, repos are cloned (not mounted) — fully isolated from the host. Changes only reach the host via PR. But before merging, you want to see exactly what Claude did — a diff viewer that shows container-side changes in real time or on demand.

## Options for Accessing Container Changes

### Option 1: Output-only volume mount

Mount a read-only output directory where Claude writes diffs/summaries. Repos stay cloned inside the container, but a shared volume exposes change summaries.

- **Pros**: Real-time visibility, no git round-trip
- **Cons**: Need to define what gets written (raw diffs? summaries? both?)
- **Implementation**: Add `-v /tmp/claude-output:/output` to docker-compose, have Claude write diffs to `/output/`

### Option 2: docker cp

Pull files out of a running container on demand: `docker cp container:/workspace/repo/file.txt .`

- **Pros**: Simple, no config needed
- **Cons**: Manual, no real-time view, need to know which files changed

### Option 3: docker exec

Shell into the running container to browse: `docker exec -it container bash`

- **Pros**: Full access, can run git diff inside
- **Cons**: Manual, breaks the "hands-off autonomous" model

### Option 4: Git-based (recommended)

Claude pushes a branch from inside the container. Host-side diff viewer does `git fetch && git diff main..claude-branch`.

- **Pros**: Cleanest — you see exactly what would go into the PR. Works with existing Graphite workflow. No volume mounts needed.
- **Cons**: Requires Claude to push before you can see anything (no real-time mid-work visibility)
- **Implementation**: Cove could auto-fetch the branch and render a TUI diff view

## Recommended Approach

Option 4 (git-based) as the primary path, with Option 1 (output volume) as a supplement for real-time progress updates.

**Flow:**

1. Claude works autonomously in container
2. Claude pushes branch via `gt submit`
3. Cove detects the push (via hook or polling)
4. Cove fetches the branch on the host
5. Cove renders a TUI diff view (ratatui) showing the changes

## Progress

- [ ] Design TUI diff view layout (ratatui)
- [ ] Implement git fetch + diff for container branches
- [ ] Add push detection (hook or polling)
- [ ] Integrate with cove sidebar
- [ ] Optional: add output volume for real-time progress
