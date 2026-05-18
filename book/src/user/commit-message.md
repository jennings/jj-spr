# Format and Update Commit Messages

In Jujutsu, commit messages (called "descriptions") follow a similar format to traditional Git commits, but with some jj-specific considerations.

## Message Format

You should format your change descriptions like this:

```
One-line title

Then a description, which may be multiple lines long.
This describes the change you are making with this commit.

Reviewers: github-username-a, github-username-b
```

The first line will be the title of the PR created by `jj spr diff`, and the rest of the lines except for the `Reviewers` line will be the PR description (i.e. the content of the first comment). The GitHub users named on the `Reviewers` line will be added to the PR as reviewers.

## Working with Jujutsu Descriptions

Set or update a change description:
```shell
# Interactive editor (recommended for multi-line descriptions)
jj describe

# Or set directly from command line
jj describe -m "Add feature

This is a really cool feature!"
```

View the current description:
```shell
jj log --no-graph -r @
```

## Updating the PR Title and Description

When you create a PR with `jj spr diff`, **the PR becomes the source of truth** for the title and description. When you land a change with `jj spr land`, its description will be updated to match the PR's title and description.

If you want to update the title or description, there are two ways:

1. **Modify the PR through GitHub's UI** (simplest method)

2. **Update locally and push**:
   ```shell
   # Edit the description
   jj describe
   
   # Push the update to the PR
   jj spr diff --update-message
   ```
   
   _Note: This does not update reviewers; that must be done in the GitHub UI._

If you want to sync your local description with the PR's current title and description:
```shell
jj spr amend
```

## Fields Added by jj spr

At various stages, `jj spr` will add metadata to your change description:

1. **After creating a PR**, `jj spr diff` adds:
   ```
   Pull Request: https://github.com/example/project/pull/123
   ```
   This line tells `jj spr` that a PR exists for this change.

2. **After landing**, `jj spr land` adds:
   ```
   Reviewed By: github-username-a
   ```
   This lists the GitHub users who approved the PR.

## Example Lifecycle

### Initial description:
```
Add user authentication

Implements JWT-based authentication for the API.

Reviewers: alice, bob
```

### After `jj spr diff`:
```
Add user authentication

Implements JWT-based authentication for the API.

Reviewers: alice, bob

Pull Request: https://github.com/example/api/pull/456
```

### After `jj spr land` (with bob's approval):
```
Add user authentication

Implements JWT-based authentication for the API.

Reviewers: alice, bob

Reviewed By: bob

Pull Request: https://github.com/example/api/pull/456
```

## Jujutsu-Specific Tips

1. **Change IDs are stable**: Unlike Git commit hashes, Jujutsu change IDs remain the same even when you modify the description.

2. **Description templates**: You can set up a description template:
   ```toml
   # In .jj/repo/config.toml
   [templates]
   draft_commit_description = '''
   

   Reviewers:
   '''
   ```

3. **Bulk operations**: Update multiple descriptions at once:
   ```shell
   # Reword multiple changes interactively
   jj reword -r 'mine() & ~main@origin'
   ```

## Reformatting

`jj spr format` reformats your current change's description to match the canonical format:

```shell
jj spr format
```

This is purely local and doesn't touch GitHub. It's useful for cleaning up formatting before running `jj spr diff`.