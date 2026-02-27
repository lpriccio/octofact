---
name: bugfix
description: Pick the next open bug from BUGS.md, fix it, test it, and commit after user approval.
allowed-tools: Read, Edit, Write, Bash(*), Grep, Glob, AskUserQuestion, Task(*)
argument-hint: "[optional bug number or keyword]"
---

# Octofact Bugfix Workflow

Fix the next open bug from the bug tracker. Follow every step in order. Do not skip steps or combine them.

## Step 1: Read the bug list and codebase context

Read these files to understand the full context:

- `BUGS.md` — the bug tracker (numbered, checkboxed list)
- `CLAUDE.md` — build instructions, architecture, conventions

Identify all **unchecked** (`- [ ]`) bugs. These are open bugs awaiting a fix.

If `$ARGUMENTS` was provided, prefer a bug matching that number or keyword. Otherwise pick the first open bug in the list.

Announce which bug you've selected and briefly explain your understanding of the problem.

## Step 2: Investigate

Before writing any fix, understand the bug:

- Read the relevant source files mentioned in the bug description or that you identify through searching.
- Reproduce the issue mentally by tracing the code path.
- If the bug description is ambiguous or you need clarification on expected behavior, ask the user.
- Identify the root cause. Do not guess — trace the logic.

Explain to the user what you've found: the root cause, which files are involved, and your proposed fix.

**Wait for the user to confirm the approach** before writing code. If the fix is straightforward and obvious, you may propose and proceed in one message, but still clearly state what you plan to change.

## Step 3: Fix

Write the fix. Follow these rules:

- Read existing files before modifying them.
- Follow the patterns and conventions already in the codebase.
- Keep changes minimal and focused on the bug. Do not refactor unrelated code.
- Do not introduce new features — only fix the bug.
- Use `PATH="$HOME/.cargo/bin:$PATH" cargo build --release` and `PATH="$HOME/.cargo/bin:$PATH" cargo clippy --release` to validate.
- Run `PATH="$HOME/.cargo/bin:$PATH" cargo test --release` if there are tests.
- Fix all compiler errors and clippy warnings before proceeding.

## Step 4: Test

Verify the fix works:

- `PATH="$HOME/.cargo/bin:$PATH" cargo test --release` — all tests must pass.
- `PATH="$HOME/.cargo/bin:$PATH" cargo build --release` — must compile clean.
- `PATH="$HOME/.cargo/bin:$PATH" cargo clippy --release` — no warnings.
- If the bug has specific reproduction steps, verify the fix addresses them.

## Step 5: User review

Ask the user to verify the bug is fixed. Be specific:

- Tell them how to run the program.
- Tell them exactly what behavior was broken and what it should look like now.
- Tell them what to do to trigger the previously-buggy code path.

**Wait for the user's response.** Do not proceed until they confirm or give feedback.

## Step 6: Iterate on feedback

If the user reports the fix is incomplete or introduced a regression:

- Investigate further.
- Fix what they ask for.
- Rebuild and retest.
- Ask them to check again.
- Repeat until they're satisfied.

## Step 7: Update BUGS.md

Check off the fixed bug by changing `- [ ]` to `- [x]`.

If during the fix you discovered related bugs or new issues, **ask the user for permission** before adding new items to `BUGS.md`. Propose the exact text and wait for approval.

## Step 8: Commit and push

Only after the user has confirmed the bug is fixed:

1. Stage the changed files (be specific — do not `git add -A`).
2. Commit with a message describing the fix (reference the bug number from BUGS.md).
3. Push to the remote.

If there is no git remote configured, just commit locally and tell the user.
