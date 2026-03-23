/*
 * Copyright (c) Radical HQ Limited
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::collections::HashSet;
use std::process::Stdio;

use crate::{error::Result, output::output};

#[derive(Debug, clap::Parser)]
pub struct CleanupOptions {
    /// Actually delete the orphan branches (default is list-only)
    #[clap(long)]
    confirm: bool,
}

/// Extract branch names from refs that match the SPR remote prefix.
///
/// Given refs like `refs/remotes/origin/spr/user/my-feature`, extracts
/// `spr/user/my-feature`.
fn extract_spr_branch_names(
    all_refs: &HashSet<String>,
    remote_name: &str,
    branch_prefix: &str,
) -> Vec<String> {
    let remote_prefix = format!("refs/remotes/{}/{}", remote_name, branch_prefix);
    let strip_len = "refs/remotes/".len() + remote_name.len() + 1;

    all_refs
        .iter()
        .filter(|r| r.starts_with(&remote_prefix))
        .map(|r| r[strip_len..].to_string())
        .collect()
}

/// Find SPR branches that are not referenced by any open PR.
fn find_orphan_branches<'a>(
    spr_branches: &'a [String],
    open_pr_branches: &HashSet<String>,
) -> Vec<&'a String> {
    spr_branches
        .iter()
        .filter(|b| !open_pr_branches.contains(*b))
        .collect()
}

pub async fn cleanup(
    opts: CleanupOptions,
    jj: &crate::jj::Jujutsu,
    gh: &crate::github::GitHub,
    config: &crate::config::Config,
) -> Result<()> {
    output("🔍", "Finding orphan SPR branches...")?;

    let all_refs = jj.get_all_ref_names()?;
    let spr_branches =
        extract_spr_branch_names(&all_refs, &config.remote_name, &config.branch_prefix);

    if spr_branches.is_empty() {
        output("✨", "No SPR branches found. Nothing to clean up.")?;
        return Ok(());
    }

    let open_pr_branches = gh.get_open_pr_branch_names().await?;
    let orphan_branches = find_orphan_branches(&spr_branches, &open_pr_branches);

    if orphan_branches.is_empty() {
        output(
            "✨",
            &format!(
                "All {} SPR branch(es) belong to open PRs. Nothing to clean up.",
                spr_branches.len()
            ),
        )?;
        return Ok(());
    }

    output(
        "🗑️",
        &format!(
            "Found {} orphan SPR branch(es) (out of {} total):",
            orphan_branches.len(),
            spr_branches.len()
        ),
    )?;

    let term = console::Term::stdout();
    for branch in &orphan_branches {
        term.write_line(&format!("     {}", console::style(*branch).dim()))?;
    }

    if !opts.confirm {
        output("💡", "Run with --confirm to delete these branches.")?;
        return Ok(());
    }

    output("🧹", "Deleting orphan branches...")?;

    for branch in &orphan_branches {
        let result = tokio::process::Command::new("git")
            .current_dir(jj.git_workdir())
            .arg("push")
            .arg("--no-verify")
            .arg("--delete")
            .arg("--")
            .arg(&config.remote_name)
            .arg(format!("refs/heads/{}", branch))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .await;

        match result {
            Ok(status) if status.status.success() => {
                output("✅", &format!("Deleted {}", branch))?;
            }
            _ => {
                output(
                    "⚠️",
                    &format!("Failed to delete {} (may already be gone)", branch),
                )?;
            }
        }
    }

    output("✨", "Cleanup complete.")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_spr_branch_names_filters_by_prefix() {
        let refs: HashSet<String> = [
            "refs/remotes/origin/spr/user/my-feature",
            "refs/remotes/origin/spr/user/main.my-feature",
            "refs/remotes/origin/main",
            "refs/remotes/origin/other-branch",
            "refs/heads/local-branch",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();

        let mut branches = extract_spr_branch_names(&refs, "origin", "spr/user/");
        branches.sort();

        assert_eq!(
            branches,
            vec!["spr/user/main.my-feature", "spr/user/my-feature"]
        );
    }

    #[test]
    fn test_extract_spr_branch_names_empty_when_no_match() {
        let refs: HashSet<String> = ["refs/remotes/origin/main", "refs/remotes/origin/feature"]
            .iter()
            .map(|s| s.to_string())
            .collect();

        let branches = extract_spr_branch_names(&refs, "origin", "spr/user/");
        assert!(branches.is_empty());
    }

    #[test]
    fn test_extract_spr_branch_names_respects_remote_name() {
        let refs: HashSet<String> = [
            "refs/remotes/origin/spr/user/feat",
            "refs/remotes/upstream/spr/user/feat",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();

        let branches = extract_spr_branch_names(&refs, "upstream", "spr/user/");
        assert_eq!(branches, vec!["spr/user/feat"]);
    }

    #[test]
    fn test_find_orphan_branches_identifies_orphans() {
        let spr_branches: Vec<String> = vec![
            "spr/user/feat-a".into(),
            "spr/user/main.feat-a".into(),
            "spr/user/feat-b".into(),
            "spr/user/feat-c".into(),
        ];

        let open_pr_branches: HashSet<String> = ["spr/user/feat-a", "spr/user/main.feat-a"]
            .iter()
            .map(|s| s.to_string())
            .collect();

        let mut orphans: Vec<&str> = find_orphan_branches(&spr_branches, &open_pr_branches)
            .into_iter()
            .map(|s| s.as_str())
            .collect();
        orphans.sort();

        assert_eq!(orphans, vec!["spr/user/feat-b", "spr/user/feat-c"]);
    }

    #[test]
    fn test_find_orphan_branches_none_when_all_active() {
        let spr_branches: Vec<String> =
            vec!["spr/user/feat-a".into(), "spr/user/main.feat-a".into()];

        let open_pr_branches: HashSet<String> = ["spr/user/feat-a", "spr/user/main.feat-a"]
            .iter()
            .map(|s| s.to_string())
            .collect();

        let orphans = find_orphan_branches(&spr_branches, &open_pr_branches);
        assert!(orphans.is_empty());
    }

    #[test]
    fn test_find_orphan_branches_all_orphans_when_no_open_prs() {
        let spr_branches: Vec<String> = vec!["spr/user/feat-a".into(), "spr/user/feat-b".into()];

        let open_pr_branches: HashSet<String> = HashSet::new();

        let orphans = find_orphan_branches(&spr_branches, &open_pr_branches);
        assert_eq!(orphans.len(), 2);
    }
}
