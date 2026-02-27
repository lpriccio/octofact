---
name: milestone
description: Pick the next unblocked Octofact milestone, implement it, test it, and commit after user approval.
allowed-tools: Read, Edit, Write, Bash(*), Grep, Glob, AskUserQuestion, Task(*), EnterWorktree
argument-hint: "[--worktree] [--progress FILE] [optional phase or milestone hint]"
---

# Octofact Milestone Workflow

Work through the next unblocked milestone from the game plan. Follow every step in order. Do not skip steps or combine them.

## Step 0: Set up worktree (if requested)

If `$ARGUMENTS` contains `--worktree`, use `EnterWorktree` to create an isolated worktree **before doing anything else**. Give it a name based on the milestone hint if one was provided (e.g., `milestone-belt-sim`), otherwise let it auto-generate a name.

This ensures all work happens on a separate branch in an isolated copy of the repo, so multiple Claudes can work on different milestones in parallel without conflicts.

If `--worktree` is not present, skip this step and work in the current directory as usual.

## Step 1: Read the plan and progress

**Parse arguments:** Extract flags from `$ARGUMENTS`:

- `--worktree` — already handled in Step 0
- `--progress <FILE>` — use that file as the progress tracker instead of the default `PROGRESS.md`

The remaining arguments (after extracting flags) are the milestone hint. Examples:

- `/milestone --worktree --progress GUI_PROGRESS.md phase 3` → worktree, progress file `GUI_PROGRESS.md`, hint `phase 3`
- `/milestone --worktree` → worktree, default progress file, no hint
- `/milestone --progress GUI_PROGRESS.md` → no worktree, progress file `GUI_PROGRESS.md`, no hint
- `/milestone phase 3` → no worktree, default progress file, hint `phase 3`

Read these files to understand the full context:

- `GAME_PLAN.md` — the master architecture blueprint
- The progress file (default `PROGRESS.md`, or the file specified via `--progress`) — checkboxed milestone tracker
- `CLAUDE.md` — build instructions, architecture, conventions

Identify all **unchecked** (`- [ ]`) milestones. Determine which are **unblocked** — meaning all milestones they depend on (earlier items in the same phase, or earlier phases where noted) are already checked.

If a milestone hint was provided, prefer a milestone matching that hint. Otherwise pick the first unblocked item in phase order.

Announce which milestone you've selected and briefly explain why it's next.

## Step 2: Interview the user (if needed)

Before writing any code, evaluate whether this milestone involves:

- **Aesthetic choices** (colors, layout, visual style, animation feel)
- **Human-facing UI decisions** (control scheme, menu structure, what information to show)
- **Gameplay feel** (speeds, timings, how something should "feel")
- **Ambiguous design** (the plan says "or:" or lists alternatives, or an Open Question in GAME_PLAN.md applies)

If **any** of those apply, interview the user first. Ask specific, concrete questions with options where possible. Do not ask vague questions — propose defaults and ask if they're acceptable.

If the milestone is purely technical (internal data structures, algorithms, plumbing), skip the interview and proceed.

## Step 3: Implement

Write the code. Follow these rules:

- Read existing files before modifying them.
- Follow the patterns and conventions already in the codebase.
- Use `PATH="$HOME/.cargo/bin:$PATH" cargo build --release` and `PATH="$HOME/.cargo/bin:$PATH" cargo clippy --release` to validate.
- Run `PATH="$HOME/.cargo/bin:$PATH" cargo test --release` if there are tests.
- Fix all compiler errors and clippy warnings before proceeding.
- Keep changes minimal and focused on the milestone. Do not refactor unrelated code.

## Step 4: Test

Run the project and verify the milestone works:

- `PATH="$HOME/.cargo/bin:$PATH" cargo test --release` — all tests must pass.
- `PATH="$HOME/.cargo/bin:$PATH" cargo build --release` — must compile clean.
- `PATH="$HOME/.cargo/bin:$PATH" cargo clippy --release` — no warnings.
- If the milestone has a **Validation** section in GAME_PLAN.md, verify those criteria.

## Step 5: User review

Ask the user to check that the milestone is working as planned. Be specific about what to test:

- Tell them how to run the program
- Tell them exactly what behavior to look for
- Tell them what keys to press or actions to take

**Wait for the user's response.** Do not proceed until they confirm or give feedback.

## Step 6: Iterate on feedback

If the user reports issues or wants changes:

- Fix what they ask for.
- Rebuild and retest.
- Ask them to check again.
- Repeat until they're satisfied.

## Step 7: Update progress file

Check off the completed milestone in the progress file (the one determined in Step 1) by changing `- [ ]` to `- [x]`.

If during implementation you discovered work that should be tracked for a future session, **ask the user for permission** before adding new items to the progress file. Propose the exact text of any new items and wait for approval.

## Step 8: Commit and push

Only after the user has confirmed the milestone works:

1. Stage the changed files (be specific — do not `git add -A`).
2. Commit with a message describing what was implemented.
3. Push to the remote.

If there is no git remote configured, just commit locally and tell the user.

**Worktree mode:** If running in a worktree, remind the user that the work is on a separate branch and they can merge it when ready (e.g., `git merge <branch-name>` from their main working branch, or create a PR).
