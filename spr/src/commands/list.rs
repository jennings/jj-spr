/*
 * Copyright (c) Radical HQ Limited
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use crate::error::Error;
use crate::error::Result;
use graphql_client::{GraphQLQuery, Response};
use reqwest;
use tabled::Table;
use tabled::Tabled;
use tabled::settings::Style;

#[allow(clippy::upper_case_acronyms)]
type URI = String;
#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "src/gql/schema.docs.graphql",
    query_path = "src/gql/open_reviews.graphql",
    response_derives = "Debug"
)]
pub struct SearchQuery;

pub async fn list(graphql_client: reqwest::Client, config: &crate::config::Config) -> Result<()> {
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

    print_pr_info(response_body).ok_or_else(|| Error::new("unexpected error"))
}

#[derive(Tabled)]
struct Row {
    #[tabled(rename = "Reviews")]
    review_status: String,
    #[tabled(rename = "Description")]
    description: String,
}

fn print_pr_info(response_body: Response<search_query::ResponseData>) -> Option<()> {
    let mut rows: Vec<Row> = Vec::new();

    for pr in response_body.data?.search.nodes? {
        let pr = match pr {
            Some(crate::commands::list::search_query::SearchQuerySearchNodes::PullRequest(pr)) => {
                pr
            }
            _ => continue,
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
