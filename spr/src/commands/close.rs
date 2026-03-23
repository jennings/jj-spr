/*
 * Copyright (c) Radical HQ Limited
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::process::Stdio;

use indoc::formatdoc;

use crate::{
    error::{Error, Result, add_error},
    github::{PullRequestState, PullRequestUpdate},
    jj::PreparedCommit,
    message::MessageSection,
    output::{output, write_commit_title},
};

#[derive(Debug, clap::Parser)]
pub struct CloseOptions {
    /// Close Pull Requests for commits in range from base to revision
    #[clap(long, short = 'a')]
    all: bool,

    /// Base revision for --all mode (if not specified, uses trunk)
    #[clap(long)]
    base: Option<String>,

    /// Jujutsu revision(s) to operate on. Can be a single revision like '@' or a range like 'main..@' or 'a::c'.
    /// If a range is provided, behaves like --all mode. If not specified, uses '@-'.
    #[clap(short = 'r', long)]
    revision: Option<String>,
}

pub async fn close(
    opts: CloseOptions,
    jj: &crate::jj::Jujutsu,
    gh: &mut crate::github::GitHub,
    config: &crate::config::Config,
) -> Result<()> {
    let mut result = Ok(());

    // Determine revision and whether to use range mode
    let (use_range_mode, base_rev, target_rev, is_inclusive) =
        crate::revision_utils::parse_revision_and_range(
            opts.revision.as_deref(),
            opts.all,
            opts.base.as_deref(),
        )?;

    let mut prepared_commits = if use_range_mode {
        jj.get_prepared_commits_from_to(config, &base_rev, &target_rev, is_inclusive)?
    } else {
        vec![jj.get_prepared_commit_for_revision(config, &target_rev)?]
    };

    if prepared_commits.is_empty() {
        output("👋", "No commits found - nothing to do. Good bye!")?;
        return result;
    }

    for prepared_commit in prepared_commits.iter_mut() {
        if result.is_err() {
            break;
        }

        write_commit_title(prepared_commit)?;

        // The further implementation of the close command is in a separate function.
        // This makes it easier to run the code to update the local commit message
        // with all the changes that the implementation makes at the end, even if
        // the implementation encounters an error or exits early.
        result = close_impl(jj, gh, config, prepared_commit).await;
    }

    // This updates the commit message in the local Jujutsu repository (if it was
    // changed by the implementation)
    add_error(
        &mut result,
        jj.rewrite_commit_messages(&mut prepared_commits),
    );

    result
}

async fn close_impl(
    jj: &crate::jj::Jujutsu,
    gh: &mut crate::github::GitHub,
    config: &crate::config::Config,
    prepared_commit: &mut PreparedCommit,
) -> Result<()> {
    let pull_request_number = if let Some(number) = prepared_commit.pull_request_number {
        output("#️⃣ ", &format!("Pull Request #{}", number))?;
        number
    } else {
        return Err(Error::new("This commit does not refer to a Pull Request."));
    };

    // Load Pull Request information
    let pull_request = gh.clone().get_pull_request(pull_request_number).await?;

    if pull_request.state != PullRequestState::Open {
        return Err(Error::new(formatdoc!(
            "This Pull Request is already closed!",
        )));
    }

    output("📖", "Getting started...")?;

    let base_is_master = pull_request.base.is_master_branch();

    let result = gh
        .update_pull_request(
            pull_request_number,
            PullRequestUpdate {
                state: Some(PullRequestState::Closed),
                ..Default::default()
            },
        )
        .await;

    match result {
        Ok(()) => (),
        Err(error) => {
            output("❌", "GitHub Pull Request close failed")?;

            return Err(error);
        }
    };

    output("📕", "Closed!")?;

    // Remove sections from commit that are not relevant after closing.
    prepared_commit.message.remove(&MessageSection::PullRequest);
    prepared_commit.message.remove(&MessageSection::ReviewedBy);
    prepared_commit.message_changed = true;

    let mut remove_old_branch_child_process = tokio::process::Command::new("git")
        .current_dir(jj.git_workdir())
        .arg("push")
        .arg("--no-verify")
        .arg("--delete")
        .arg("--")
        .arg(&config.remote_name)
        .arg(pull_request.head.on_github())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    let remove_old_base_branch_child_process = if base_is_master {
        None
    } else {
        Some(
            tokio::process::Command::new("git")
                .current_dir(jj.git_workdir())
                .arg("push")
                .arg("--no-verify")
                .arg("--delete")
                .arg("--")
                .arg(&config.remote_name)
                .arg(pull_request.base.on_github())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?,
        )
    };

    // Wait for the "git push" to delete the old Pull Request branch to finish,
    // but ignore the result.
    // GitHub may be configured to delete the branch automatically,
    // in which case it's gone already and this command fails.
    remove_old_branch_child_process.wait().await?;
    if let Some(mut proc) = remove_old_base_branch_child_process {
        proc.wait().await?;
    }

    Ok(())
}
