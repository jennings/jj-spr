# Create and Land a Simple PR

This section details the process of putting a single commit up for review, and landing it (pushing it upstream). It assumes you don't have multiple reviews in flight at the same time. That situation is covered in [another guide](./stack.md), but you should be familiar with this single-review workflow before reading that one.

## Understanding @ and @-

Before diving into the workflow, it's crucial to understand Jujutsu's revision symbols:

- **`@`** = your **working copy** (the current state where you make edits)
- **`@-`** = the **parent** of your working copy (typically your last committed change)

**After running `jj commit`:**
- Your committed change moves to `@-`
- Your working copy `@` becomes empty (ready for new work)

**Why this matters for jj-spr:**
- `jj spr diff` defaults to operating on **`@-`** (your completed change)
- `jj spr land` defaults to operating on **`@`** (your working copy)

This means after `jj commit`, you'll typically:
- Run `jj spr diff` (no args needed - creates PR for `@-`)
- Run `jj spr land -r @-` (must specify `@-` since land defaults to `@`)

**When in doubt:** Run `jj log` to see your revision history and where you are.

```
Example jj log output:
@  qpvuntsm you@example.com 2024-01-15 12:00:00 (empty)
│  (no description set)
○  kmkuslkw you@example.com 2024-01-15 11:30:00
│  Add authentication feature
◆  main@origin
```

In this example:
- `@` is empty (ready for new work)
- `@-` is the "Add authentication feature" commit (what you'd create a PR for)

## Basic Workflow with Jujutsu

1. Fetch the latest changes from upstream:
   ```shell
   jj git fetch
   ```

2. Create a new change for your work:
   ```shell
   jj new main@origin
   ```

3. Make your changes and describe them:
   ```shell
   # Edit your files...
   # ... make your changes ...

   # Describe the change
   jj describe -m "Add user authentication

   This implements basic user authentication using JWT tokens."
   ```

   See [this guide](./commit-message.md) for what to put in your commit message.

4. Create a new empty change on top, so your PR change moves to `@-`:
   ```shell
   jj new
   ```

   This creates an empty working copy at `@`, with your PR change now at `@-`. This is the recommended workflow because `jj spr diff` defaults to operating on `@-`.

5. Run `jj spr diff` to create a PR for your change:
   ```shell
   jj spr diff
   ```

   > **Note:** By default, `diff` operates on `@-` (the parent of your working copy). Since we just ran `jj new`, your completed change is now at `@-`, so `jj spr diff` will correctly target it.

6. Wait for reviewers to approve. If you need to make changes:

   1. Make your edits in your working copy (`@`). Jujutsu automatically tracks the changes.
   2. When ready, squash the changes into your PR change at `@-`:
      ```shell
      jj squash
      ```
   3. Update the description if needed:
      ```shell
      jj describe -r @-
      ```
   4. Run `jj spr diff` to update the PR. If you changed the commit message, add the `--update-message` flag:
      ```shell
      jj spr diff --update-message
      ```

      This will update the PR with the new version of your change. jj-spr will prompt you for a short message that describes what you changed. You can also pass the update message on the command line using the `--message`/`-m` flag.

7. Once your PR is approved, land it:
   ```shell
   jj spr land -r @-
   ```

   > **Note:** By default, `land` operates on `@` (your working copy). Since your PR change is at `@-`, you must specify `-r @-`.

8. **🚨 CRITICAL: After landing, you MUST manually rebase your working copy:**
   ```shell
   jj git fetch
   jj rebase -r @ -d main@origin
   ```

   > ⚠️ **IMPORTANT - DO NOT SKIP THIS STEP:** jj-spr currently requires manual rebasing after **every** `jj spr land`. The `land` command merges your PR on GitHub but does **not** automatically update your local Jujutsu state.
   >
   > **What happens if you skip this:**
   > - Your working copy will still be based on old `main`
   > - Future changes will be based on outdated code
   > - You'll face conflicts and confusion later
   > - Your stack will be out of sync with GitHub
   >
   > **This is a known limitation** that may be automated in future versions. For now, always run the rebase commands immediately after landing.
   >
   > **Pro tip:** Create a shell alias (see [Troubleshooting](#troubleshooting)) to combine landing and rebasing into one command.

## Working with Change IDs

In Jujutsu, every change has a stable change ID (like `qpvuntsm`). You can use these IDs to refer to specific changes:

```shell
# Create a PR for a specific change
jj spr diff -r qpvuntsm

# Land a specific change
jj spr land -r qpvuntsm
```

## When you update

When you run `jj spr diff` to update an existing PR, your update will be added to the PR as a new commit, so that reviewers can see exactly what changed. The new commit's message will be what you entered when prompted.

The individual commits that you see in the PR are solely for the benefit of reviewers; they will not be reflected in the commit history when the PR is landed. The commit that eventually lands on upstream `main` will always be a single commit, whose message is the title and description from the PR.

## Updating before landing

Unlike Git, Jujutsu automatically maintains your change's identity even when rebasing. However, you must still run `jj spr diff` to update the PR before landing if you've rebased onto new upstream changes, or else `jj spr land` will fail.

This is because `jj spr land` checks to make sure that the PR content matches what will be landed.

## Conflicts on landing

`jj spr land` may fail with conflicts if there have been new changes pushed to upstream `main` since you last fetched. In this case:

1. Fetch and rebase your change onto latest upstream `main`:
   ```shell
   jj git fetch
   jj rebase -r @ -d main@origin
   ```

2. Resolve any conflicts:
   ```shell
   # Jujutsu will mark conflicts in the files
   # Edit the files to resolve conflicts
   jj resolve
   ```

3. Run `jj spr diff` to update the PR:
   ```shell
   jj spr diff
   ```

4. Run `jj spr land` again:
   ```shell
   jj spr land
   ```

Note that even if your change is not based on the latest upstream `main`, landing will still succeed as long as there are no conflicts with the actual latest upstream `main`.

## Common Scenarios

### Landing without specifying revision

```shell
# These commands operate on different revisions by default:
jj spr diff       # operates on @- (parent of working copy)
jj spr land       # operates on @ (working copy)
```

Following the recommended workflow where you keep an empty working copy at `@`, your PR change will be at `@-`, so you must specify it when landing:

```shell
jj new main@origin
# ... make changes ...
jj describe -m "My change"
jj new  # Moves PR change to @-
jj spr diff
jj spr land -r @-  # Must specify @- since land defaults to @
```

### Quick workflow

```shell
# 1. Create a new change and make your edits
jj new main@origin
# ... make changes ...

# 2. Describe your change
jj describe -m "Add feature"

# 3. Create a new empty change on top (moves your PR change to @-)
jj new

# 4. Create PR (operates on @-, your completed change)
jj spr diff

# 5. Make updates if needed
# ... edit files ...
jj squash  # Squash changes into @-
jj spr diff  # Update the PR

# 6. After approval, land it
jj spr land -r @-

# 7. Rebase your working copy
jj git fetch
jj rebase -r @ -d main@origin
```

## Troubleshooting

### "I ran `jj spr diff` but nothing happened" or "No changes to diff"

**Cause:** Your change might be at a different revision than `@-` (the default).

**Solutions:**
1. Check where your changes are:
   ```shell
   jj log
   ```

2. If your change is at `@` (working copy is not empty):
   ```shell
   jj spr diff -r @
   ```

3. If you forgot to run `jj new` after describing your change:
   ```shell
   jj new  # Creates empty change on top, moving your PR change to @-
   jj spr diff  # Now operates on @- (your completed change)
   ```

### "I forgot to rebase after landing"

**Problem:** After `jj spr land`, you must manually rebase your working copy onto the updated `main@origin`.

**Solution:**
```shell
jj git fetch
jj rebase -r @ -d main@origin
```

**Why this matters:** If you don't rebase, your future changes might be based on outdated code, leading to conflicts later.

### "I landed the wrong change"

**Problem:** You landed a change you didn't mean to land.

**Solution:** Unfortunately, once landed, the PR is merged on GitHub. You'll need to:
1. Revert the change on GitHub or locally
2. Create a new PR with the revert

**Prevention:** Always double-check which change you're landing:
```shell
jj log -r @-  # Check what you're about to land
jj spr land -r @-
```

### "`jj spr land` failed with conflicts"

**Cause:** Upstream `main` has changed since your last fetch, causing conflicts.

**Solution:**
1. Fetch and rebase onto latest main:
   ```shell
   jj git fetch
   jj rebase -r @- -d main@origin
   ```

2. Resolve any conflicts (Jujutsu will mark them in files):
   ```shell
   # Edit conflicted files
   # Jujutsu automatically tracks resolution
   ```

3. Update the PR with resolved conflicts:
   ```shell
   jj spr diff
   ```

4. Try landing again:
   ```shell
   jj spr land -r @-
   ```

### "The PR content doesn't match what will be landed"

**Cause:** You've made local changes (like rebasing) without updating the PR.

**Solution:**
```shell
jj spr diff  # Update the PR to match local state
jj spr land -r @-  # Now landing will work
```

### "I can't find my change ID"

**Problem:** The docs reference change IDs like `qpvuntsm` but you don't know yours.

**Solution:**
```shell
# Show recent changes with their IDs
jj log

# Show only your changes since main
jj log -r 'main@origin..'

# The first column shows the change ID
# Example output:
#   @  qpvuntsm you@example.com 2024-01-15
#   │  Add feature
```
