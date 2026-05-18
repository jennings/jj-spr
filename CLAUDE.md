# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Super Pull Requests (SPR) is a Rust-based CLI tool (`jj-spr`) that bridges Jujutsu's change-based workflow with GitHub's pull request model. It's the power tool for Jujutsu + GitHub workflows, enabling amend-friendly single PRs and effortless stacked PRs.

**Key Value Propositions:**
- **Amend-friendly workflow**: Users can amend freely locally using Jujutsu's natural workflow while maintaining clean, incremental diffs for reviewers on GitHub
- **Effortless stacking**: Supports both independent and dependent PR stacks with automatic rebase handling and flexible landing order

**Key Architecture Concepts:**
- **Change ID-based**: Uses Jujutsu's stable change IDs instead of Git commit hashes
- **Stacking support**: Creates dependent PRs with proper base branch handling
- **Colocated repositories**: Requires both `.jj` and `.git` directories (Jujutsu colocated with Git)
- **GitHub API integration**: Uses both REST API (via `octocrab`) and GraphQL API for PR management

## Development Commands

### Building and Testing
```bash
# Build the project
cargo build

# Build release version
cargo build --release

# Run all tests (requires jj and git installed)
cargo test

# Run specific test
cargo test test_name

# Run integration tests
cargo test --test '*'
```

### Code Quality
```bash
# Format code
cargo fmt

# Check formatting without modifying
cargo fmt --all -- --check

# Run clippy (treat warnings as errors in CI)
cargo clippy --all-features --all-targets -- -D warnings

# Run clippy for development (warnings allowed)
cargo clippy --all-features --all-targets
```

### Nix Development (if using Nix)
```bash
# Enter development shell with all dependencies
nix develop

# Build with Nix
nix build

# Check flake
nix flake check

# Update flake dependencies
nix flake update
```

## Core Architecture

### Module Structure

The codebase follows a library + binary structure:

- **`spr/src/lib.rs`**: Public module declarations
- **`spr/src/main.rs`**: CLI entrypoint using `clap`
- **`spr/src/commands/`**: Individual command implementations (diff, land, list, amend, close, etc.)
- **`spr/src/jj.rs`**: Jujutsu integration layer (executes `jj` commands, manages change IDs)
- **`spr/src/git.rs`**: Git operations via `git2` crate (branch management, commit creation)
- **`spr/src/github.rs`**: GitHub API client (PR creation/updates, GraphQL queries)
- **`spr/src/config.rs`**: Configuration management (reads from git config)
- **`spr/src/message.rs`**: Commit message parsing and formatting (Summary, Reviewers, etc.)
- **`spr/src/revision_utils.rs`**: Revision parsing and resolution
- **`spr/src/output.rs`**: Terminal output formatting
- **`spr/src/error.rs`**: Error types and handling

### Key Design Patterns

1. **Change-based workflow**: Each Jujutsu change (identified by a change ID) maps to one GitHub PR. The PR branch name includes the change ID to maintain the link even after rebases.

2. **Stacked PR handling**: When creating stacked PRs, the tool creates intermediate branches for each change in the stack. The base branch of PR #2 points to the PR branch of PR #1, allowing independent reviews and landing.

3. **Commit message structure**: Commit messages are parsed into sections (Summary, Reviewers, etc.) which are stored in both the local Jujutsu change and the GitHub PR. The tool maintains bidirectional sync.

4. **Git/Jujutsu bridge**: While Jujutsu is the primary interface, the tool uses Git operations under the hood for branch creation and pushing to GitHub, since GitHub only understands Git.

## Important Constraints

- **Requires colocated repository**: Must have both `.jj/` and `.git/` directories
- **GitHub-only**: Only works with GitHub (not GitLab, Bitbucket, etc.)
- **Stable change IDs**: Relies on Jujutsu's change IDs remaining stable across rebases
- **Async runtime**: Uses Tokio for async operations (GitHub API calls)
- **Configuration via git config**: Settings are stored in `.git/config` using `spr.*` keys

## Dependencies

### Runtime Requirements
- `jj` (Jujutsu CLI) must be in PATH
- `git` must be in PATH
- GitHub Personal Access Token for API access

### Key Rust Dependencies
- `clap` - CLI argument parsing with derive macros
- `git2` - Git operations via libgit2
- `octocrab` - GitHub REST API client
- `graphql_client` - GitHub GraphQL API client
- `tokio` - Async runtime
- `dialoguer` - Interactive prompts

## Common Workflows

### Creating/Updating PRs
1. User runs `jj spr diff -r <revision>`
2. Tool resolves revision to change ID and Git commit hash
3. Parses commit message into sections
4. Creates/updates Git branch named with change ID
5. Pushes branch to GitHub
6. Creates/updates PR via GitHub API
7. Updates local commit message with PR number

### Landing PRs
1. User runs `jj spr land -r <revision>`
2. Tool finds associated PR by change ID
3. Verifies PR is approved (if configured)
4. Squash-merges PR on GitHub
5. Cleans up remote branch

### Stacked PRs
1. User creates multiple changes with parent-child relationships
2. Runs `jj spr diff --all` or `jj spr diff -r main..@`
3. Tool creates PRs in order, each with base pointing to previous PR's branch
4. When landing, only the bottom PR is merged; others remain valid

## Testing Strategy

- **Integration tests** in `spr/tests/` verify end-to-end workflows using temporary Git/Jujutsu repos
- **Unit tests** are colocated with implementation code
- Tests require both `jj` and `git` binaries available in PATH
- CI runs on Ubuntu with all dependencies installed

## Configuration Options

Stored in git config under `spr.*` namespace:
- `spr.githubRepository` - Owner/repo name
- `spr.branchPrefix` - Prefix for generated branches (default: `spr/`)
- `spr.requireApproval` - Require PR approval before landing
- `spr.githubHost` - Custom GitHub Enterprise host
- `spr.githubToken` - GitHub API token (typically stored via `jj spr init`)
