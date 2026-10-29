/*
 * Copyright (c) Radical HQ Limited
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::collections::HashMap;

use crate::config::get_config_value;
use crate::error::Error;
use crate::error::Result;
use graphql_client::{GraphQLQuery, Response};
use reqwest;
use tabled::Table;
use tabled::Tabled;
use tabled::settings::Style;

/// Revset searched for changes belonging to open Pull Requests when neither
/// `--revision` nor `spr.listRevset` is given.
const DEFAULT_REVSET: &str = "reachable(@, mutable())";

/// Marks a change matched through its spr "Pull Request" trailer.
const SPR_MARKER: &str = "🥞";
/// Marks a change matched through a tracked jj bookmark.
const BOOKMARK_MARKER: &str = "🔖";

#[derive(Debug, clap::Parser)]
pub struct ListOptions {
    /// Jujutsu revset to search for changes belonging to the listed Pull
    /// Requests (if not given taken from config spr.listRevset, defaulting
    /// to 'reachable(@, mutable())')
    #[clap(short = 'r', long)]
    revision: Option<String>,
}

#[allow(clippy::upper_case_acronyms)]
type URI = String;
#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "src/gql/schema.docs.graphql",
    query_path = "src/gql/open_reviews.graphql",
    response_derives = "Debug"
)]
pub struct SearchQuery;

/// Local changes that open Pull Requests can be matched to.
#[derive(Default)]
struct LocalChanges {
    /// Change IDs of changes with a "Pull Request" trailer, by PR number.
    by_pull_request: HashMap<u64, Vec<String>>,
    /// Change IDs of changes with a local bookmark that tracks a bookmark
    /// on the remote, by bookmark name.
    by_branch: HashMap<String, Vec<String>>,
}

impl LocalChanges {
    fn load(jj: &crate::jj::Jujutsu, config: &crate::config::Config, revset: &str) -> Result<Self> {
        let tracked = jj.get_tracked_bookmarks(&config.remote_name)?;
        let mut local = Self::default();

        for change in jj.get_changes_in_revset(config, revset)? {
            // Changes submitted with spr carry a "Pull Request" trailer.
            if let Some(number) = change.pull_request_number {
                local
                    .by_pull_request
                    .entry(number)
                    .or_default()
                    .push(change.change_id.clone());
            }
            // Other Pull Requests are matched through a local bookmark that
            // tracks the Pull Request's branch on the remote.
            for bookmark in change.local_bookmarks {
                if tracked.contains(&bookmark) {
                    local
                        .by_branch
                        .entry(bookmark)
                        .or_default()
                        .push(change.change_id.clone());
                }
            }
        }

        Ok(local)
    }

    /// What to show for a Pull Request in the "Local" column: change IDs,
    /// marked by whether they were matched through the spr trailer or a
    /// tracked bookmark.
    fn local_refs_for(
        &self,
        pr: &search_query::SearchQuerySearchNodesOnPullRequest,
    ) -> Vec<String> {
        if let Some(ids) = self.by_pull_request.get(&(pr.number as u64)) {
            return ids.iter().map(|id| format!("{SPR_MARKER} {id}")).collect();
        }
        // A same-named branch in a fork is unrelated to our bookmark.
        if !pr.is_cross_repository
            && let Some(ids) = self.by_branch.get(&pr.head_ref_name)
        {
            return ids
                .iter()
                .map(|id| format!("{BOOKMARK_MARKER} {id}"))
                .collect();
        }
        Vec::new()
    }
}

pub async fn list(
    opts: ListOptions,
    jj: &crate::jj::Jujutsu,
    graphql_client: reqwest::Client,
    config: &crate::config::Config,
) -> Result<()> {
    let revset = match opts.revision {
        Some(revision) => revision,
        None => get_config_value("spr.listRevset", &jj.git_repo.config()?)
            .unwrap_or_else(|| DEFAULT_REVSET.to_string()),
    };
    let local = LocalChanges::load(jj, config, &revset)?;

    let variables = search_query::Variables {
        query: format!(
            "repo:{}/{} is:open is:pr author:@me archived:false",
            config.owner, config.repo
        ),
    };
    let request_body = SearchQuery::build_query(variables);
    let res = graphql_client
        .post("https://api.github.com/graphql")
        .json(&request_body)
        .send()
        .await?;
    let response_body: Response<search_query::ResponseData> = res.json().await?;

    print_pr_info(response_body, &local).ok_or_else(|| Error::new("unexpected error"))
}

#[derive(Tabled)]
struct Row {
    #[tabled(rename = "Local")]
    local: String,
    #[tabled(rename = "Reviews")]
    review_status: String,
    #[tabled(rename = "Description")]
    description: String,
}

fn print_pr_info(
    response_body: Response<search_query::ResponseData>,
    local: &LocalChanges,
) -> Option<()> {
    let mut rows: Vec<Row> = Vec::new();

    for pr in response_body.data?.search.nodes? {
        let pr = match pr {
            Some(crate::commands::list::search_query::SearchQuerySearchNodes::PullRequest(pr)) => {
                pr
            }
            _ => continue,
        };

        let local_refs = local.local_refs_for(&pr);
        let local_column = if local_refs.is_empty() {
            console::style("-").dim().to_string()
        } else {
            local_refs.join("\n")
        };

        let review_status = match pr.review_decision {
            Some(search_query::PullRequestReviewDecision::APPROVED) => {
                console::style("Accepted").green().to_string()
            }
            Some(search_query::PullRequestReviewDecision::CHANGES_REQUESTED) => {
                console::style("Changes Requested").red().to_string()
            }
            None | Some(search_query::PullRequestReviewDecision::REVIEW_REQUIRED) => {
                "Pending".to_string()
            }
            Some(search_query::PullRequestReviewDecision::Other(d)) => d,
        };

        let description = format!(
            "{}\n{}",
            console::style(&pr.title).bold(),
            console::style(&pr.url).dim(),
        );

        rows.push(Row {
            local: local_column,
            review_status,
            description,
        });
    }

    if rows.is_empty() {
        return Some(());
    }

    let mut table = Table::new(rows);
    table.with(Style::sharp());

    let term = console::Term::stdout();
    term.write_line(&table.to_string()).ok()?;

    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pr(
        number: i64,
        head: &str,
        cross: bool,
    ) -> search_query::SearchQuerySearchNodesOnPullRequest {
        search_query::SearchQuerySearchNodesOnPullRequest {
            number,
            title: String::new(),
            url: String::new(),
            review_decision: None,
            head_ref_name: head.to_string(),
            is_cross_repository: cross,
        }
    }

    #[test]
    fn test_local_refs_for() {
        let mut local = LocalChanges::default();
        local.by_pull_request.insert(1, vec!["aaaa".into()]);
        local
            .by_branch
            .insert("spr/me/one".into(), vec!["aaaa".into()]);
        local
            .by_branch
            .insert("feature".into(), vec!["bbbb".into()]);

        // spr trailer wins over a bookmark on the same change.
        assert_eq!(
            local.local_refs_for(&pr(1, "spr/me/one", false)),
            vec!["🥞 aaaa"]
        );
        // Non-spr PR matched by tracked bookmark.
        assert_eq!(
            local.local_refs_for(&pr(2, "feature", false)),
            vec!["🔖 bbbb"]
        );
        // Same branch name in a fork does not match.
        assert!(local.local_refs_for(&pr(3, "feature", true)).is_empty());
        // No local change.
        assert!(local.local_refs_for(&pr(4, "other", false)).is_empty());
    }
}
