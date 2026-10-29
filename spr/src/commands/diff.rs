/*
 * Copyright (c) Radical HQ Limited
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::{collections::HashMap, iter::zip};

use crate::{
    config::StackStyle,
    error::{Error, Result, ResultExt, add_error},
    github::{
        GitHub, GitHubBranch, PullRequest, PullRequestRequestReviewers, PullRequestState,
        PullRequestUpdate,
    },
    message::MessageSectionsMap,
    message::{MessageSection, validate_commit_message},
    output::{output, write_commit_title},
    utils::{parse_name_list, remove_all_parens, run_command},
};
use git2::Oid;
use indoc::{formatdoc, indoc};

#[derive(Debug, clap::Parser)]
pub struct DiffOptions {
    /// Create/update pull requests for commits in range from base to revision
    #[clap(long, short = 'a')]
    all: bool,

    /// Update the pull request title and description on GitHub from the local
    /// commit message
    #[clap(long)]
    update_message: bool,

    /// Submit any new Pull Request as a draft
    #[clap(long)]
    draft: bool,

    /// Message to be used for commits updating existing pull requests (e.g.
    /// 'rebase' or 'review comments')
    #[clap(long, short = 'm')]
    message: Option<String>,

    /// Submit this commit as if it was cherry-picked on master. Do not base it
    /// on any intermediate changes between the master branch and this commit.
    #[clap(long)]
    cherry_pick: bool,

    /// Remove the "Cherry Pick:" marker from the commit description and ignore
    /// it for this invocation. Future `jj spr diff` invocations will behave
    /// as though --cherry-pick was not specified.
    #[clap(long, conflicts_with = "cherry_pick")]
    no_cherry_pick: bool,

    /// Base revision for --all mode (if not specified, uses trunk)
    #[clap(long)]
    base: Option<String>,

    /// Jujutsu revision(s) to operate on. Can be a single revision like '@' or a range like 'main..@' or 'a::c'.
    /// If a range is provided, behaves like --all mode. If not specified, uses '@-'.
    #[clap(short = 'r', long)]
    revision: Option<String>,

    /// Preview what would happen without pushing or creating PRs
    #[clap(long)]
    pub dry_run: bool,
}

/// Resolve the effective cherry-pick state from CLI flags and the "Cherry Pick:"
/// marker on the commit description.
///
/// When `--cherry-pick` is passed the marker is added (if absent); when
/// `--no-cherry-pick` is passed the marker is removed (if present). Returns
/// `(effective_cherry_pick, marker_was_changed)`.
fn resolve_cherry_pick(
    cherry_pick: bool,
    no_cherry_pick: bool,
    message: &mut crate::message::MessageSectionsMap,
) -> (bool, bool) {
    let stored = message
        .get(&MessageSection::CherryPick)
        .map(|s| s.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    if no_cherry_pick {
        if stored {
            message.remove(&MessageSection::CherryPick);
            return (false, true);
        }
        (false, false)
    } else if cherry_pick {
        if !stored {
            message.insert(MessageSection::CherryPick, "true".to_string());
            return (true, true);
        }
        (true, false)
    } else {
        (stored, false)
    }
}

pub async fn diff(
    opts: DiffOptions,
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

    // Get commits to process
    let mut prepared_commits = if use_range_mode {
        // Get range of commits from base to target
        jj.get_prepared_commits_from_to(config, &base_rev, &target_rev, is_inclusive)?
    } else {
        // Just get the single specified revision
        vec![jj.get_prepared_commit_for_revision(config, &target_rev)?]
    };

    // Determine the master base OID - this is the commit on master that the stack is based on
    let master_base_oid = if let Some(first_commit) = prepared_commits.first() {
        if use_range_mode {
            // For range mode, the parent of the first commit is the master base
            first_commit.parent_oid
        } else {
            // For single commit mode, find the actual merge base with master
            jj.get_master_base_for_commit(config, first_commit.oid)?
        }
    } else {
        output("👋", "No commits found - nothing to do. Good bye!")?;
        return result;
    };

    #[allow(clippy::needless_collect)]
    let pull_request_tasks: Vec<_> = prepared_commits
        .iter()
        .map(|pc: &crate::jj::PreparedCommit| {
            pc.pull_request_number
                .map(|number| tokio::spawn(gh.clone().get_pull_request(number)))
        })
        .collect();

    let mut message_on_prompt = "".to_string();

    // The Pull Request head branch and head commit for each local commit
    // processed so far, so that stacked Pull Requests can target them.
    let mut stack_heads: HashMap<Oid, (GitHubBranch, Oid)> = HashMap::new();

    for (prepared_commit, pull_request_task) in zip(prepared_commits.iter_mut(), pull_request_tasks)
    {
        if result.is_err() {
            break;
        }

        let pull_request = if let Some(task) = pull_request_task {
            Some(task.await??)
        } else {
            None
        };

        if !opts.dry_run {
            write_commit_title(prepared_commit)?;
        }

        // The further implementation of the diff command is in a separate function.
        // This makes it easier to run the code to update the local commit message
        // with all the changes that the implementation makes at the end, even if
        // the implementation encounters an error or exits early.
        result = diff_impl(
            &opts,
            &mut message_on_prompt,
            jj,
            gh,
            config,
            prepared_commit,
            master_base_oid,
            pull_request,
            &stack_heads,
        )
        .await
        .map(|head| {
            if let Some(head) = head {
                stack_heads.insert(prepared_commit.oid, head);
            }
        });
    }

    // This updates the commit message in the local Jujutsu repository (if it was
    // changed by the implementation)
    if !opts.dry_run {
        add_error(
            &mut result,
            jj.rewrite_commit_messages(prepared_commits.as_mut_slice()),
        );
    }

    if opts.dry_run {
        let actions: Vec<_> = prepared_commits
            .iter()
            .enumerate()
            .filter(|(_, c)| c.dry_run_action.is_some())
            .collect();

        output(
            "\n📋",
            &format!(
                "Dry run complete. Would process {} change(s):\n",
                actions.len()
            ),
        )?;

        for (idx, pc) in &actions {
            let title = pc
                .message
                .get(&crate::message::MessageSection::Title)
                .map(|t| &t[..])
                .unwrap_or("");
            let pos = idx + 1;

            let (action_label, base, head, reviewers_list) = match &pc.dry_run_action {
                Some(crate::jj::DryRunAction::Create {
                    base,
                    head,
                    draft,
                    reviewers,
                    ..
                }) => {
                    let label = if *draft {
                        "CREATE (draft)".to_string()
                    } else {
                        "CREATE".to_string()
                    };
                    (label, base.as_str(), head.as_str(), reviewers.clone())
                }
                Some(crate::jj::DryRunAction::Update {
                    pr_number,
                    base,
                    head,
                    ..
                }) => {
                    let label = format!("UPDATE PR #{pr_number}");
                    (label, base.as_str(), head.as_str(), vec![])
                }
                None => continue,
            };

            output(
                &format!("  #{pos}"),
                &format!("{action_label}  {}  \"{title}\"", pc.short_id),
            )?;
            output("     ", &format!("head: {head}"))?;
            output("     ", &format!("base: {base}"))?;
            if !reviewers_list.is_empty() {
                output(
                    "     ",
                    &format!("reviewers: {}", reviewers_list.join(", ")),
                )?;
            }
            output("", "")?;
        }
    }

    result
}

#[allow(clippy::too_many_arguments)]
async fn diff_impl(
    opts: &DiffOptions,
    message_on_prompt: &mut String,
    jj: &crate::jj::Jujutsu,
    gh: &mut crate::github::GitHub,
    config: &crate::config::Config,
    local_commit: &mut crate::jj::PreparedCommit,
    master_base_oid: Oid,
    pull_request: Option<PullRequest>,
    stack_heads: &HashMap<Oid, (GitHubBranch, Oid)>,
) -> Result<Option<(GitHubBranch, Oid)>> {
    // Parsed commit message of the local commit
    let message = &mut local_commit.message;

    let (effective_cherry_pick, marker_changed) =
        resolve_cherry_pick(opts.cherry_pick, opts.no_cherry_pick, message);
    if marker_changed {
        local_commit.message_changed = true;
    }

    // Check if the local commit is based directly on the master branch.
    let directly_based_on_master = local_commit.parent_oid == master_base_oid;

    // Determine the trees the Pull Request branch and the base branch should
    // have when we're done here.
    let (new_head_tree, new_base_tree) = if !effective_cherry_pick || directly_based_on_master {
        // Unless the user tells us to --cherry-pick, these should be the trees
        // of the current commit and its parent.
        // If the current commit is directly based on master (i.e.
        // directly_based_on_master is true), then we can do this here even when
        // the user tells us to --cherry-pick, because we would cherry pick the
        // current commit onto its parent, which gives us the same tree as the
        // current commit has, and the master base is the same as this commit's
        // parent.
        let head_tree = jj.get_tree_oid_for_commit(local_commit.oid)?;
        let base_tree = jj.get_tree_oid_for_commit(local_commit.parent_oid)?;

        (head_tree, base_tree)
    } else {
        // Cherry-pick the current commit onto master
        let index = jj.cherrypick(local_commit.oid, master_base_oid)?;

        if index.has_conflicts() {
            return Err(Error::new(formatdoc!(
                "This commit cannot be cherry-picked on {master}.",
                master = config.master_ref.branch_name(),
            )));
        }

        // This is the tree we are getting from cherrypicking the local commit
        // on master.
        let cherry_pick_tree = jj.write_index(index)?;
        let master_tree = jj.get_tree_oid_for_commit(master_base_oid)?;

        (cherry_pick_tree, master_tree)
    };

    if let Some(number) = local_commit.pull_request_number {
        output(
            "#️⃣ ",
            &format!(
                "Pull Request #{}: {}",
                number,
                config.pull_request_url(number)
            ),
        )?;
    }

    if local_commit.pull_request_number.is_none() || opts.update_message {
        validate_commit_message(message)?;
    }

    if let Some(ref pull_request) = pull_request {
        if pull_request.state == PullRequestState::Closed {
            return Err(Error::new(formatdoc!(
                "Pull request is closed. If you want to open a new one, \
                 remove the 'Pull Request' section from the commit message."
            )));
        }

        if !opts.update_message {
            let mut pull_request_updates: PullRequestUpdate = Default::default();
            pull_request_updates.update_message(pull_request, message);

            if !pull_request_updates.is_empty() {
                output(
                    "⚠️",
                    indoc!(
                        "The Pull Request's title/message differ from the \
                         local commit's message.
                         Use `spr diff --update-message` to overwrite the \
                         title and message on GitHub with the local message, \
                         or `spr amend` to go the other way (rewrite the local \
                         commit message with what is on GitHub)."
                    ),
                )?;
            }
        }
    }

    // Parse "Reviewers" section, if this is a new Pull Request
    let mut requested_reviewers = PullRequestRequestReviewers::default();

    if local_commit.pull_request_number.is_none()
        && let Some(reviewers) = message.get(&MessageSection::Reviewers)
    {
        let reviewers = parse_name_list(reviewers);
        let mut checked_reviewers = Vec::new();

        for reviewer in reviewers {
            // Teams are indicated with a leading #
            if let Some(slug) = reviewer.strip_prefix('#') {
                if let Ok(team) = GitHub::get_github_team((&config.owner).into(), slug.into()).await
                {
                    requested_reviewers
                        .team_reviewers
                        .push(team.slug.to_string());

                    checked_reviewers.push(reviewer);
                } else {
                    return Err(Error::new(format!(
                        "Reviewers field contains unknown team '{}'",
                        reviewer
                    )));
                }
            } else if let Ok(user) = GitHub::get_github_user(reviewer.clone()).await {
                requested_reviewers.reviewers.push(user.login);
                if let Some(name) = user.name {
                    checked_reviewers.push(format!(
                        "{} ({})",
                        reviewer.clone(),
                        remove_all_parens(&name)
                    ));
                } else {
                    checked_reviewers.push(reviewer);
                }
            } else {
                return Err(Error::new(format!(
                    "Reviewers field contains unknown user '{}'",
                    reviewer
                )));
            }
        }

        message.insert(MessageSection::Reviewers, checked_reviewers.join(", "));
        local_commit.message_changed = true;
    }

    // Get the name of the existing Pull Request branch, or constuct one if
    // there is none yet.

    let title = message
        .get(&MessageSection::Title)
        .map(|t| &t[..])
        .unwrap_or("");

    let pull_request_branch = match &pull_request {
        Some(pr) => pr.head.clone(),
        None => {
            config.new_github_branch(&config.get_new_branch_name(&jj.get_all_ref_names()?, title))
        }
    };

    // Work out the commits the new Pull Request commit is based on, and the
    // branch the Pull Request targets on GitHub:
    // `pr_head_oid` is the first parent of the new Pull Request commit (the
    // current head of the Pull Request, or the commit a new one starts from),
    // `pr_base_parent` an optional second parent merged into it,
    // `base_branch_commit` an optional commit to push to `base_branch`, and
    // `base_branch` the branch the Pull Request targets (`None` for master).
    let (pr_head_oid, pr_base_parent, base_branch_commit, base_branch, needs_merging_master) =
        if config.stack_style == StackStyle::Native {
            // With native stacking, the Pull Request targets master or the
            // head branch of the parent commit's Pull Request directly. Its
            // head branch must contain the tip of that branch, so that GitHub
            // shows exactly the changes of the local commit.
            let base = resolve_native_base(
                jj,
                gh,
                config,
                local_commit.parent_oid,
                master_base_oid,
                directly_based_on_master || effective_cherry_pick,
                stack_heads,
            )
            .await?;
            let base_branch = (!base.branch.is_master_branch()).then(|| base.branch.clone());

            if let Some(pr) = &pull_request {
                let contains_base = pr.head_oid == base.oid
                    || jj.git_repo.graph_descendant_of(pr.head_oid, base.oid)?;

                if contains_base && jj.get_tree_oid_for_commit(pr.head_oid)? == new_head_tree {
                    finish_without_new_commit(opts, gh, pr, message, Some(&base.branch)).await?;
                    return Ok(Some((pr.head.clone(), pr.head_oid)));
                }

                (
                    pr.head_oid,
                    (!contains_base).then_some(base.oid),
                    None,
                    base_branch,
                    !contains_base,
                )
            } else {
                (base.oid, None, None, base_branch, false)
            }
        } else {
            // Get the tree ids of the current head of the Pull Request, as well as the
            // base, and the commit id of the master commit this PR is currently based
            // on.
            // If there is no pre-existing Pull Request, we fill in the equivalent
            // values.
            let (pr_head_oid, pr_head_tree, pr_base_oid, pr_base_tree, pr_master_base) =
                if let Some(pr) = &pull_request {
                    let pr_head_tree = jj.get_tree_oid_for_commit(pr.head_oid)?;

                    let current_master_oid = jj.resolve_reference(config.master_ref.local())?;
                    // Use git for merge base calculation since jj doesn't expose this directly
                    let pr_base_oid = jj.git_repo.merge_base(pr.head_oid, pr.base_oid)?;
                    let pr_base_tree = jj.get_tree_oid_for_commit(pr_base_oid)?;

                    let pr_master_base = jj.git_repo.merge_base(pr.head_oid, current_master_oid)?;

                    (
                        pr.head_oid,
                        pr_head_tree,
                        pr_base_oid,
                        pr_base_tree,
                        pr_master_base,
                    )
                } else {
                    let master_base_tree = jj.get_tree_oid_for_commit(master_base_oid)?;
                    (
                        master_base_oid,
                        master_base_tree,
                        master_base_oid,
                        master_base_tree,
                        master_base_oid,
                    )
                };
            let needs_merging_master = pr_master_base != master_base_oid;

            // At this point we can check if we can exit early because no update to the
            // existing Pull Request is necessary
            if let Some(ref pull_request) = pull_request {
                // So there is an existing Pull Request...
                if !needs_merging_master
                    && pr_head_tree == new_head_tree
                    && pr_base_tree == new_base_tree
                {
                    // ...and it does not need a rebase, and the trees of both Pull
                    // Request branch and base are all the right ones.
                    finish_without_new_commit(opts, gh, pull_request, message, None).await?;
                    return Ok(Some((pull_request.head.clone(), pull_request.head_oid)));
                }
            }

            // Check if there is a base branch on GitHub already. That's the case when
            // there is an existing Pull Request, and its base is not the master branch.
            let base_branch = if let Some(ref pr) = pull_request {
                if pr.base.is_master_branch() {
                    None
                } else {
                    Some(pr.base.clone())
                }
            } else {
                None
            };

            // We are going to construct `pr_base_parent: Option<Oid>`.
            // The value will be the commit we have to merge into the new Pull Request
            // commit to reflect changes in the parent of the local commit (by rebasing
            // or changing commits between master and this one, although technically
            // that's also rebasing).
            // If it's `None`, then we will not merge anything into the new Pull Request
            // commit.
            // If we are updating an existing PR, then there are three cases here:
            // (1) the parent tree of this commit is unchanged and we do not need to
            //     merge in master, which means that the local commit was amended, but
            //     not rebased. We don't need to merge anything into the Pull Request
            //     branch.
            // (2) the parent tree has changed, but the parent of the local commit is on
            //     master (or we are cherry-picking) and we are not already using a base
            //     branch: in this case we can merge the master commit we are based on
            //     into the PR branch, without going via a base branch. Thus, we don't
            //     introduce a base branch here and the PR continues to target the
            //     master branch.
            // (3) the parent tree has changed, and we need to use a base branch (either
            //     because one was already created earlier, or we find that we are not
            //     directly based on master now): we need to construct a new commit for
            //     the base branch. That new commit's tree is always that of that local
            //     commit's parent (thus making sure that the difference between base
            //     branch and pull request branch are exactly the changes made by the
            //     local commit, thus the changes we want to have reviewed). The new
            //     commit may have one or two parents. The previous base is always a
            //     parent (that's either the current commit on an existing base branch,
            //     or the previous master commit the PR was based on if there isn't a
            //     base branch already). In addition, if the master commit this commit
            //     is based on has changed, (i.e. the local commit got rebased on newer
            //     master in the meantime) then we have to merge in that master commit,
            //     which will be the second parent.
            // If we are creating a new pull request then `pr_base_tree` (the current
            // base of the PR) was set above to be the tree of the master commit the
            // local commit is based one, whereas `new_base_tree` is the tree of the
            // parent of the local commit. So if the local commit for this new PR is on
            // master, those two are the same (and we want to apply case 1). If the
            // commit is not directly based on master, we have to create this new PR
            // with a base branch, so that is case 3.

            let (pr_base_parent, base_branch) = if pr_base_tree == new_base_tree
                && !needs_merging_master
            {
                // Case 1
                (None, base_branch)
            } else if base_branch.is_none() && (directly_based_on_master || effective_cherry_pick) {
                // Case 2
                (Some(master_base_oid), None)
            } else {
                // Case 3

                // We are constructing a base branch commit.
                // One parent of the new base branch commit will be the current base
                // commit, that could be either the top commit of an existing base
                // branch, or a commit on master.
                let mut parents = vec![pr_base_oid];

                // If we need to rebase on master, make the master commit also a
                // parent (except if the first parent is that same commit, we don't
                // want duplicates in `parents`).
                if needs_merging_master && pr_base_oid != master_base_oid {
                    parents.push(master_base_oid);
                }

                let new_base_branch_commit = if opts.dry_run {
                    // Use a placeholder OID — this won't be pushed
                    pr_base_oid
                } else {
                    jj.create_derived_commit(
                        local_commit.parent_oid,
                        &format!(
                            "[spr] {}\n\n[skip ci]",
                            if pull_request.is_some() {
                                "changes introduced through rebase".to_string()
                            } else {
                                format!(
                                    "changes to {} this commit is based on",
                                    config.master_ref.branch_name()
                                )
                            },
                        ),
                        new_base_tree,
                        &parents[..],
                    )?
                };

                // If `base_branch` is `None` (which means a base branch does not exist
                // yet), then make a `GitHubBranch` with a new name for a base branch
                let base_branch = if let Some(base_branch) = base_branch {
                    base_branch
                } else {
                    config.new_github_branch(
                        &config.get_base_branch_name(&jj.get_all_ref_names()?, title),
                    )
                };

                (Some(new_base_branch_commit), Some(base_branch))
            };

            (
                pr_head_oid,
                pr_base_parent,
                pr_base_parent,
                base_branch,
                needs_merging_master,
            )
        };

    let mut github_commit_message = opts.message.clone();
    if pull_request.is_some() && github_commit_message.is_none() && !opts.dry_run {
        let input = {
            let message_on_prompt = message_on_prompt.clone();

            tokio::task::spawn_blocking(move || {
                dialoguer::Input::<String>::new()
                    .with_prompt("Message (leave empty to abort)")
                    .with_initial_text(message_on_prompt)
                    .allow_empty(true)
                    .interact_text()
            })
            .await??
        };

        if input.is_empty() {
            return Err(Error::new("Aborted as per user request".to_string()));
        }

        *message_on_prompt = input.clone();
        github_commit_message = Some(input);
    }

    // Construct the new commit for the Pull Request branch. First parent is the
    // current head commit of the Pull Request (we set this to the master base
    // commit earlier if the Pull Request does not yet exist)
    let mut pr_commit_parents = vec![pr_head_oid];

    // If we prepared a commit earlier that needs merging into the Pull Request
    // branch, then that commit is a parent of the new Pull Request commit.
    if let Some(oid) = pr_base_parent {
        // ...unless if that's the same commit as the one we added to
        // pr_commit_parents first.
        if pr_commit_parents.first() != Some(&oid) {
            pr_commit_parents.push(oid);
        }
    }

    // Create the new commit
    let pr_commit = if opts.dry_run {
        // Use a placeholder OID — this won't be pushed
        pr_head_oid
    } else {
        jj.create_derived_commit(
            local_commit.oid,
            github_commit_message
                .as_ref()
                .map(|s| &s[..])
                .unwrap_or_else(|| title),
            new_head_tree,
            &pr_commit_parents[..],
        )?
    };

    if opts.dry_run {
        let base_ref = base_branch.as_ref().unwrap_or(&config.master_ref);
        let base_branch_name = base_ref.branch_name();
        let head_branch_name = pull_request_branch.branch_name();
        let is_stacked = !base_ref.is_master_branch();

        local_commit.dry_run_action = if let Some(ref pr) = pull_request {
            Some(crate::jj::DryRunAction::Update {
                pr_number: pr.number,
                base: base_branch_name.to_string(),
                head: head_branch_name.to_string(),
                is_stacked,
            })
        } else {
            let all_reviewers: Vec<String> = requested_reviewers
                .reviewers
                .iter()
                .chain(requested_reviewers.team_reviewers.iter())
                .cloned()
                .collect();
            Some(crate::jj::DryRunAction::Create {
                base: base_branch_name.to_string(),
                head: head_branch_name.to_string(),
                is_stacked,
                draft: opts.draft,
                reviewers: all_reviewers,
            })
        };
    } else {
        let mut cmd = jj.git_command();
        cmd.arg("push")
            .arg("--atomic")
            .arg("--no-verify")
            .arg("--")
            .arg(&config.remote_name)
            .arg(format!("{}:{}", pr_commit, pull_request_branch.on_github()));

        if let Some(pull_request) = pull_request {
            // We are updating an existing Pull Request

            if needs_merging_master {
                output(
                    "⚾",
                    &format!(
                        "Commit was rebased - updating Pull Request #{}",
                        pull_request.number
                    ),
                )?;
            } else {
                output(
                    "🔁",
                    &format!(
                        "Commit was changed - updating Pull Request #{}",
                        pull_request.number
                    ),
                )?;
            }

            // Things we want to update in the Pull Request on GitHub
            let mut pull_request_updates: PullRequestUpdate = Default::default();

            if opts.update_message {
                pull_request_updates.update_message(&pull_request, message);
            }

            if let Some(base_branch) = base_branch {
                // We are using a base branch.

                if let Some(base_branch_commit) = base_branch_commit {
                    // ...and we prepared a new commit for it, so we need to push an
                    // update of the base branch.
                    cmd.arg(format!(
                        "{}:{}",
                        base_branch_commit,
                        base_branch.on_github()
                    ));
                }

                // Push the new commit onto the Pull Request branch (and also the
                // new base commit, if we added that to cmd above).
                run_command(&mut cmd)
                    .await
                    .reword("git push failed".to_string())?;

                // If the Pull Request's base is not set to the base branch yet,
                // change that now.
                if pull_request.base.branch_name() != base_branch.branch_name() {
                    pull_request_updates.base = Some(base_branch.branch_name().to_string());
                }
            } else {
                // The Pull Request is against the master branch. In that case we
                // only need to push the update to the Pull Request branch.
                run_command(&mut cmd)
                    .await
                    .reword("git push failed".to_string())?;

                // With native stacking, a Pull Request that was stacked on
                // another one targets master once its parent has landed.
                if !pull_request.base.is_master_branch() {
                    pull_request_updates.base = Some(config.master_ref.branch_name().to_string());
                }
            }

            if !pull_request_updates.is_empty() {
                gh.update_pull_request(pull_request.number, pull_request_updates)
                    .await?;
            }
        } else {
            // We are creating a new Pull Request.

            // If there's a base branch, add it to the push
            if let (Some(base_branch), Some(base_branch_commit)) =
                (&base_branch, base_branch_commit)
            {
                cmd.arg(format!(
                    "{}:{}",
                    base_branch_commit,
                    base_branch.on_github()
                ));
            }
            // Push the pull request branch and the base branch if present
            run_command(&mut cmd)
                .await
                .reword("git push failed".to_string())?;

            // Then call GitHub to create the Pull Request.
            let pull_request_number = gh
                .create_pull_request(
                    message,
                    base_branch
                        .as_ref()
                        .unwrap_or(&config.master_ref)
                        .branch_name()
                        .to_string(),
                    pull_request_branch.branch_name().to_string(),
                    opts.draft,
                )
                .await?;

            let pull_request_url = config.pull_request_url(pull_request_number);

            output(
                "✨",
                &format!(
                    "Created new Pull Request #{}: {}",
                    pull_request_number, &pull_request_url,
                ),
            )?;

            message.insert(MessageSection::PullRequest, pull_request_url);
            local_commit.message_changed = true;

            let result = gh
                .request_reviewers(pull_request_number, requested_reviewers)
                .await;
            match result {
                Ok(()) => (),
                Err(error) => {
                    output("⚠️", "Requesting reviewers failed")?;
                    for message in error.messages() {
                        output("  ", message)?;
                    }
                }
            }
        }
    }

    Ok(Some((pull_request_branch, pr_commit)))
}

/// The branch a Pull Request targets with native stacking, and the commit at
/// its tip. That commit has to be an ancestor of the Pull Request head so
/// that GitHub shows only the changes of the local commit.
struct NativeBase {
    branch: GitHubBranch,
    oid: Oid,
}

/// Find what a Pull Request for a commit with the given parent should target
/// with native stacking: master if the commit is based on master, otherwise
/// the head branch of the Pull Request for the parent commit.
async fn resolve_native_base(
    jj: &crate::jj::Jujutsu,
    gh: &GitHub,
    config: &crate::config::Config,
    parent_oid: Oid,
    master_base_oid: Oid,
    based_on_master: bool,
    stack_heads: &HashMap<Oid, (GitHubBranch, Oid)>,
) -> Result<NativeBase> {
    if based_on_master {
        return Ok(NativeBase {
            branch: config.master_ref.clone(),
            oid: master_base_oid,
        });
    }

    // The parent was handled earlier in this run, so its Pull Request branch
    // is up to date.
    if let Some((branch, oid)) = stack_heads.get(&parent_oid) {
        return Ok(NativeBase {
            branch: branch.clone(),
            oid: *oid,
        });
    }

    let parent = jj.prepare_commit(config, parent_oid)?;
    let number = parent.pull_request_number.ok_or_else(|| {
        Error::new(formatdoc!(
            "The parent commit {parent} does not have a Pull Request yet. \
             Submit it first (for example with `jj spr diff --all`), or use \
             --cherry-pick to base this Pull Request on {master}.",
            parent = parent.short_id,
            master = config.master_ref.branch_name(),
        ))
    })?;

    let pull_request = gh.clone().get_pull_request(number).await?;

    if pull_request.state != PullRequestState::Open {
        return Err(Error::new(formatdoc!(
            "Pull Request #{number} for the parent commit {parent} is closed. \
             Rebase this commit onto {master} first.",
            parent = parent.short_id,
            master = config.master_ref.branch_name(),
        )));
    }

    if jj.get_tree_oid_for_commit(pull_request.head_oid)?
        != jj.get_tree_oid_for_commit(parent_oid)?
    {
        return Err(Error::new(formatdoc!(
            "Pull Request #{number} for the parent commit {parent} is out of \
             date. Update it first (for example with `jj spr diff --all`).",
            parent = parent.short_id,
        )));
    }

    Ok(NativeBase {
        branch: pull_request.head,
        oid: pull_request.head_oid,
    })
}

/// Handle a Pull Request whose head already has the right content: update
/// its title and description if requested, and its base branch if
/// `new_base` differs from the current one.
async fn finish_without_new_commit(
    opts: &DiffOptions,
    gh: &GitHub,
    pull_request: &PullRequest,
    message: &MessageSectionsMap,
    new_base: Option<&GitHubBranch>,
) -> Result<()> {
    let mut pull_request_updates: PullRequestUpdate = Default::default();

    if opts.update_message {
        // The user requested to update the commit message on GitHub
        pull_request_updates.update_message(pull_request, message);
    }
    let message_updated = !pull_request_updates.is_empty();

    let retarget = new_base.filter(|base| base.branch_name() != pull_request.base.branch_name());
    if let Some(base) = retarget {
        pull_request_updates.base = Some(base.branch_name().to_string());
    }

    if retarget.is_none() {
        output("✅", "No update necessary")?;
    }

    if pull_request_updates.is_empty() {
        return Ok(());
    }

    if opts.dry_run {
        output(
            "  ",
            &format!("Would update PR #{} on GitHub", pull_request.number),
        )?;
        return Ok(());
    }

    gh.update_pull_request(pull_request.number, pull_request_updates)
        .await?;

    if let Some(base) = retarget {
        output(
            "🎯",
            &format!("Changed base branch to {}", base.branch_name()),
        )?;
    }
    if message_updated {
        output("✍", "Updated commit message on GitHub")?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_config() -> crate::config::Config {
        crate::config::Config::new(
            "test_owner".into(),
            "test_repo".into(),
            "origin".into(),
            "main".into(),
            "spr/test/".into(),
            false,
        )
    }

    #[allow(dead_code)]
    fn create_test_git_repo() -> (TempDir, git2::Repository) {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let repo = git2::Repository::init(temp_dir.path()).expect("Failed to init git repo");

        // Create initial commit
        let signature = git2::Signature::now("Test User", "test@example.com")
            .expect("Failed to create signature");
        let tree_id = {
            let mut index = repo.index().expect("Failed to get index");
            index.write_tree().expect("Failed to write tree")
        };
        let tree = repo.find_tree(tree_id).expect("Failed to find tree");

        repo.commit(
            Some("HEAD"),
            &signature,
            &signature,
            "Initial commit",
            &tree,
            &[],
        )
        .expect("Failed to create initial commit");

        drop(tree); // Drop the tree reference before moving repo
        (temp_dir, repo)
    }

    #[allow(dead_code)]
    fn create_test_commit(repo: &git2::Repository, message: &str, content: &str) -> git2::Oid {
        let signature = git2::Signature::now("Test User", "test@example.com")
            .expect("Failed to create signature");

        // Write content to a test file
        let repo_path = repo.workdir().expect("Failed to get workdir");
        let file_path = repo_path.join("test.txt");
        fs::write(&file_path, content).expect("Failed to write test file");

        // Add file to index
        let mut index = repo.index().expect("Failed to get index");
        index
            .add_path(std::path::Path::new("test.txt"))
            .expect("Failed to add file to index");
        index.write().expect("Failed to write index");

        let tree_id = index.write_tree().expect("Failed to write tree");
        let tree = repo.find_tree(tree_id).expect("Failed to find tree");

        // Get HEAD commit as parent
        let parent_commit = repo
            .head()
            .expect("Failed to get HEAD")
            .peel_to_commit()
            .expect("Failed to peel to commit");

        repo.commit(
            Some("HEAD"),
            &signature,
            &signature,
            message,
            &tree,
            &[&parent_commit],
        )
        .expect("Failed to create commit")
    }

    #[test]
    fn test_diff_options_default_values() {
        let opts = DiffOptions {
            all: false,
            update_message: false,
            draft: false,
            message: None,
            cherry_pick: false,
            no_cherry_pick: false,
            base: None,
            revision: None,
            dry_run: false,
        };

        assert!(!opts.all);
        assert!(!opts.update_message);
        assert!(!opts.draft);
        assert!(!opts.cherry_pick);
        assert!(opts.message.is_none());
        assert!(opts.base.is_none());
    }

    #[test]
    fn test_diff_options_with_base() {
        let opts = DiffOptions {
            all: true,
            update_message: false,
            draft: false,
            message: None,
            cherry_pick: false,
            no_cherry_pick: false,
            base: Some("main".to_string()),
            revision: None,
            dry_run: false,
        };

        assert_eq!(opts.base, Some("main".to_string()));
        assert!(opts.all);
    }

    #[test]
    fn test_jujutsu_integration() {
        // Test configuration for jj-spr
        let config = create_test_config();
        assert_eq!(config.owner, "test_owner");
        assert_eq!(config.remote_name, "origin");
    }

    #[test]
    fn test_base_option_parsing() {
        // Test that the base option can be parsed correctly
        let opts_with_base = DiffOptions {
            all: true,
            update_message: false,
            draft: false,
            message: None,
            cherry_pick: false,
            no_cherry_pick: false,
            base: Some("main".to_string()),
            revision: None,
            dry_run: false,
        };

        assert_eq!(opts_with_base.base.as_deref(), Some("main"));
        assert!(opts_with_base.all);

        let opts_with_trunk = DiffOptions {
            all: true,
            update_message: false,
            draft: false,
            message: None,
            cherry_pick: false,
            no_cherry_pick: false,
            base: Some("trunk()".to_string()),
            revision: None,
            dry_run: false,
        };

        assert_eq!(opts_with_trunk.base.as_deref(), Some("trunk()"));
    }

    #[test]
    fn test_all_flag_behavior() {
        let opts_with_all = DiffOptions {
            all: true,
            update_message: false,
            draft: false,
            message: None,
            cherry_pick: false,
            no_cherry_pick: false,
            base: Some("trunk()".to_string()),
            revision: None,
            dry_run: false,
        };

        // When --all is specified, it should work with base revisions
        assert!(opts_with_all.all);
        assert!(opts_with_all.base.is_some());
    }

    #[test]
    fn test_diff_options_combinations() {
        // Test various valid combinations of options
        let opts = DiffOptions {
            all: true,
            update_message: true,
            draft: true,
            message: Some("Update message".to_string()),
            cherry_pick: false,
            no_cherry_pick: false,
            base: Some("trunk()".to_string()),
            revision: None,
            dry_run: false,
        };

        assert!(opts.all);
        assert!(opts.update_message);
        assert!(opts.draft);
        assert_eq!(opts.message.as_deref(), Some("Update message"));
        assert!(!opts.cherry_pick);
        assert_eq!(opts.base.as_deref(), Some("trunk()"));
    }

    #[test]
    fn test_diff_options_dry_run_flag() {
        let opts = DiffOptions {
            all: false,
            update_message: false,
            draft: false,
            message: None,
            cherry_pick: false,
            no_cherry_pick: false,
            base: None,
            revision: None,
            dry_run: true,
        };

        assert!(opts.dry_run);
        assert!(!opts.all);
    }

    // -------------------------------------------------------------------------
    // resolve_cherry_pick unit tests

    fn make_map_with_cherry_pick(value: &str) -> crate::message::MessageSectionsMap {
        [(
            crate::message::MessageSection::CherryPick,
            value.to_string(),
        )]
        .into()
    }

    #[test]
    fn test_resolve_cherry_pick_default_no_marker() {
        let mut map = crate::message::MessageSectionsMap::new();
        let (effective, changed) = resolve_cherry_pick(false, false, &mut map);
        assert!(!effective);
        assert!(!changed);
        assert!(!map.contains_key(&crate::message::MessageSection::CherryPick));
    }

    #[test]
    fn test_resolve_cherry_pick_default_with_marker() {
        let mut map = make_map_with_cherry_pick("true");
        let (effective, changed) = resolve_cherry_pick(false, false, &mut map);
        assert!(effective);
        assert!(!changed);
    }

    #[test]
    fn test_resolve_cherry_pick_marker_case_insensitive() {
        let mut map = make_map_with_cherry_pick("TRUE");
        let (effective, _) = resolve_cherry_pick(false, false, &mut map);
        assert!(effective);
    }

    #[test]
    fn test_resolve_cherry_pick_marker_other_value() {
        for value in &["false", "yes", "1", ""] {
            let mut map = make_map_with_cherry_pick(value);
            let (effective, _) = resolve_cherry_pick(false, false, &mut map);
            assert!(!effective, "Expected false for marker value {:?}", value);
        }
    }

    #[test]
    fn test_resolve_cherry_pick_flag_inserts_marker() {
        let mut map = crate::message::MessageSectionsMap::new();
        let (effective, changed) = resolve_cherry_pick(true, false, &mut map);
        assert!(effective);
        assert!(changed);
        assert_eq!(
            map.get(&crate::message::MessageSection::CherryPick)
                .map(String::as_str),
            Some("true")
        );
    }

    #[test]
    fn test_resolve_cherry_pick_flag_idempotent() {
        let mut map = make_map_with_cherry_pick("true");
        let (effective, changed) = resolve_cherry_pick(true, false, &mut map);
        assert!(effective);
        assert!(!changed);
    }

    #[test]
    fn test_resolve_cherry_pick_no_flag_removes_marker() {
        let mut map = make_map_with_cherry_pick("true");
        let (effective, changed) = resolve_cherry_pick(false, true, &mut map);
        assert!(!effective);
        assert!(changed);
        assert!(!map.contains_key(&crate::message::MessageSection::CherryPick));
    }

    #[test]
    fn test_resolve_cherry_pick_no_flag_when_absent() {
        let mut map = crate::message::MessageSectionsMap::new();
        let (effective, changed) = resolve_cherry_pick(false, true, &mut map);
        assert!(!effective);
        assert!(!changed);
    }

    #[test]
    fn test_diff_options_parse_no_cherry_pick() {
        use clap::Parser;
        let opts = DiffOptions::try_parse_from(["test", "--no-cherry-pick"]).unwrap();
        assert!(opts.no_cherry_pick);
        assert!(!opts.cherry_pick);
    }

    #[test]
    fn test_diff_options_cherry_pick_no_cherry_pick_conflict() {
        use clap::Parser;
        let result = DiffOptions::try_parse_from(["test", "--cherry-pick", "--no-cherry-pick"]);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.kind(), clap::error::ErrorKind::ArgumentConflict);
    }

    /// Run a `jj` command in `repo_path`, panicking if it fails.
    fn run_jj(repo_path: &std::path::Path, args: &[&str]) {
        let output = std::process::Command::new("jj")
            .args(args)
            .current_dir(repo_path)
            .output()
            .expect("Failed to run jj");
        assert!(
            output.status.success(),
            "jj {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    /// Create a jj repository with two commits, a base and a parent commit
    /// without a Pull Request, and return their ids.
    fn create_stacked_jj_repo() -> (TempDir, crate::jj::Jujutsu, Oid, Oid) {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let repo_path = temp_dir.path();

        run_jj(repo_path, &["git", "init", "--colocate"]);
        run_jj(
            repo_path,
            &["config", "set", "--repo", "user.name", "Test User"],
        );
        run_jj(
            repo_path,
            &["config", "set", "--repo", "user.email", "test@example.com"],
        );

        fs::write(repo_path.join("base.txt"), "base").unwrap();
        run_jj(repo_path, &["commit", "-m", "Base commit"]);
        fs::write(repo_path.join("parent.txt"), "parent").unwrap();
        run_jj(repo_path, &["commit", "-m", "Parent commit"]);

        let jj = crate::jj::Jujutsu::new(repo_path.to_path_buf()).expect("Failed to open repo");
        let (base_oid, parent_oid) = {
            // In a colocated repo, Git's HEAD is the parent of the working copy
            let parent = jj.git_repo.head().unwrap().peel_to_commit().unwrap();
            (parent.parent(0).unwrap().id(), parent.id())
        };

        (temp_dir, jj, base_oid, parent_oid)
    }

    fn create_test_github(config: &crate::config::Config, jj: &crate::jj::Jujutsu) -> GitHub {
        GitHub::new(
            config.clone(),
            jj.git_repo.path().to_owned(),
            reqwest::Client::new(),
        )
    }

    #[tokio::test]
    async fn test_native_base_based_on_master() {
        let (_temp_dir, jj, base_oid, parent_oid) = create_stacked_jj_repo();
        let config = create_test_config().with_stack_style(StackStyle::Native);
        let gh = create_test_github(&config, &jj);

        let base = resolve_native_base(
            &jj,
            &gh,
            &config,
            parent_oid,
            base_oid,
            true,
            &HashMap::new(),
        )
        .await
        .unwrap();

        assert!(base.branch.is_master_branch());
        assert_eq!(base.oid, base_oid);
    }

    #[tokio::test]
    async fn test_native_base_uses_parent_pushed_in_same_run() {
        let (_temp_dir, jj, base_oid, parent_oid) = create_stacked_jj_repo();
        let config = create_test_config().with_stack_style(StackStyle::Native);
        let gh = create_test_github(&config, &jj);

        let parent_branch = config.new_github_branch("spr/test/parent");
        let parent_head = Oid::from_str("1234567890123456789012345678901234567890").unwrap();
        let stack_heads = HashMap::from([(parent_oid, (parent_branch, parent_head))]);

        let base =
            resolve_native_base(&jj, &gh, &config, parent_oid, base_oid, false, &stack_heads)
                .await
                .unwrap();

        assert_eq!(base.branch.branch_name(), "spr/test/parent");
        assert!(!base.branch.is_master_branch());
        assert_eq!(base.oid, parent_head);
    }

    #[tokio::test]
    async fn test_native_base_requires_parent_pull_request() {
        let (_temp_dir, jj, base_oid, parent_oid) = create_stacked_jj_repo();
        let config = create_test_config().with_stack_style(StackStyle::Native);
        let gh = create_test_github(&config, &jj);

        let result = resolve_native_base(
            &jj,
            &gh,
            &config,
            parent_oid,
            base_oid,
            false,
            &HashMap::new(),
        )
        .await;

        let error = result.err().expect("expected an error");
        assert!(
            error
                .messages()
                .iter()
                .any(|m| m.contains("does not have a Pull Request yet")),
            "unexpected error: {:?}",
            error.messages()
        );
    }

    // Integration tests would require more complex setup with actual Git repositories
    // and proper mocking of GitHub API calls. The tests above focus on:
    // 1. Option parsing and validation
    // 2. Data structure correctness
    // 3. Basic logic flow verification
    //
    // For full integration testing, consider:
    // - Mocking GitHub API responses
    // - Creating test repositories with specific commit structures
    // - Testing the interaction between revision specification and commit preparation
}
