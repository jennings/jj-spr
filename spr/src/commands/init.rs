/*
 * Copyright (c) Radical HQ Limited
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use indoc::formatdoc;
use lazy_regex::regex;

use crate::{
    config::{AuthTokenSource, get_auth_token_with_source, set_jj_config},
    error::{Error, Result, ResultExt},
    output::output,
};

pub async fn init() -> Result<()> {
    output("👋", "Welcome to spr!")?;

    let path = std::env::current_dir()?;
    let repo = crate::jj::Jujutsu::new(path.clone()).reword(formatdoc!(
        "Could not find Git backend for Jujutsu repository in {:?}.",
        &path
    ))?;
    let config = repo.git_repo.config()?;

    // GitHub Personal Access Token

    console::Term::stdout().write_line("")?;

    output(
        "🔑",
        "Okay, let's get started. First we need to authenticate to GitHub.",
    )?;

    let github_auth_token = get_auth_token_with_source(&config).and_then(|value| {
        if value.token().is_empty() {
            None
        } else {
            Some(value)
        }
    });

    let reuse_token = match github_auth_token {
        None => false,
        Some(AuthTokenSource::GitHubCLI(_)) => dialoguer::Confirm::new()
            .with_prompt("Use the GitHub CLI to authenticate?")
            .default(true)
            .interact()?,
        Some(AuthTokenSource::Config(_)) => dialoguer::Confirm::new()
            .with_prompt("A personal access token is already configured. Use it?")
            .default(true)
            .interact()?,
    };

    let pat = if reuse_token {
        github_auth_token.unwrap().token().to_owned()
    } else {
        output(
            "  ",
            &formatdoc!(
                "We need a 'Personal Access Token' from GitHub. This will \
             authorise spr to open/update/merge Pull Requests etc. on behalf of \
             your GitHub user.
             You can get one by going to https://github.com/settings/tokens \
             and clicking on 'Generate new token'. The token needs the 'repo', \
             'user' and 'read:org' permissions, so please tick those three boxes \
             in the 'Select scopes' section.
             You might want to set the 'Expiration' to 'No expiration', as \
             otherwise you will have to repeat this procedure soon. Even \
             if the token does not expire, you can always revoke it in case \
             you fear someone got hold of it."
            ),
        )?;

        let pat = dialoguer::Password::new()
            .with_prompt("GitHub Personal Access Token")
            .allow_empty_password(github_auth_token.is_some())
            .interact()?;

        if pat.is_empty() {
            return Err(Error::new("Cannot continue without an access token."));
        }
        pat
    };

    let octocrab = octocrab::OctocrabBuilder::default()
        .personal_token(pat.clone())
        .build()?;
    let github_user = octocrab.current().user().await?;

    output("👋", &formatdoc!("Hello {}!", github_user.login))?;

    if !reuse_token {
        set_jj_config("spr.githubAuthToken", pat.as_str(), &path)?;
    }

    // Name of remote

    console::Term::stdout().write_line("")?;

    output(
        "❓",
        &formatdoc!(
            "What's the name of the Git remote pointing to GitHub? Usually it's
             'origin'."
        ),
    )?;

    let remote = dialoguer::Input::<String>::new()
        .with_prompt("Name of remote for GitHub")
        .with_initial_text(
            config
                .get_string("spr.githubRemoteName")
                .ok()
                .unwrap_or_else(|| "origin".to_string()),
        )
        .interact_text()?;
    set_jj_config("spr.githubRemoteName", &remote, &path)?;

    // Name of the GitHub repo

    console::Term::stdout().write_line("")?;

    output(
        "❓",
        &formatdoc!(
            "What's the name of the GitHub repository. Please enter \
             'OWNER/REPOSITORY' (basically the bit that follow \
             'github.com/' in the address.)"
        ),
    )?;

    let url = repo.git_repo.find_remote(&remote)?.url().map(String::from);
    let regex = lazy_regex::regex!(r#"github\.com[/:]([\w\-\.]+/[\w\-\.]+?)(.git)?$"#);
    let github_repo = config
        .get_string("spr.githubRepository")
        .ok()
        .and_then(|value| if value.is_empty() { None } else { Some(value) })
        .or_else(|| {
            url.as_ref()
                .and_then(|url| regex.captures(url))
                .and_then(|caps| caps.get(1))
                .map(|m| m.as_str().to_string())
        })
        .unwrap_or_default();

    let github_repo = dialoguer::Input::<String>::new()
        .with_prompt("GitHub repository")
        .with_initial_text(github_repo)
        .interact_text()?;
    set_jj_config("spr.githubRepository", &github_repo, &path)?;

    // Master branch name (just query GitHub)

    let github_repo_info = octocrab
        .get::<octocrab::models::Repository, _, _>(format!("/repos/{}", &github_repo), None::<&()>)
        .await?;

    set_jj_config(
        "spr.githubMasterBranch",
        github_repo_info
            .default_branch
            .as_ref()
            .map(|s| &s[..])
            .unwrap_or("master"),
        &path,
    )?;

    // Pull Request branch prefix

    console::Term::stdout().write_line("")?;

    let branch_prefix = config
        .get_string("spr.branchPrefix")
        .ok()
        .and_then(|value| if value.is_empty() { None } else { Some(value) })
        .unwrap_or_else(|| format!("spr/{}/", &github_user.login));

    output(
        "❓",
        &formatdoc!(
            "What prefix should be used when naming Pull Request branches?
             Good practice is to begin with 'spr/' as a general namespace \
             for spr-managed Pull Request branches. Continuing with the \
             GitHub user name is a good idea, so there is no danger of names \
             clashing with those of other users.
             The prefix should end with a good separator character (like '/' \
             or '-'), since commit titles will be appended to this prefix."
        ),
    )?;

    let branch_prefix = dialoguer::Input::<String>::new()
        .with_prompt("Branch prefix")
        .with_initial_text(branch_prefix)
        .validate_with(|input: &String| -> Result<()> { validate_branch_prefix(input) })
        .interact_text()?;

    set_jj_config("spr.branchPrefix", &branch_prefix, &path)?;

    Ok(())
}

fn validate_branch_prefix(branch_prefix: &str) -> Result<()> {
    // They can include slash / for hierarchical (directory) grouping, but no slash-separated component can begin with a dot . or end with the sequence .lock.
    if branch_prefix.contains("/.")
        || branch_prefix.contains(".lock/")
        || branch_prefix.ends_with(".lock")
        || branch_prefix.starts_with('.')
    {
        return Err(Error::new(
            "Branch prefix cannot have slash-separated component beginning with a dot . or ending with the sequence .lock",
        ));
    }

    if branch_prefix.contains("..") {
        return Err(Error::new(
            "Branch prefix cannot contain two consecutive dots anywhere.",
        ));
    }

    if branch_prefix.chars().any(|c| c.is_ascii_control()) {
        return Err(Error::new(
            "Branch prefix cannot contain ASCII control sequence",
        ));
    }

    let forbidden_chars_re = regex!(r"[ \~\^:?*\[\\]");
    if forbidden_chars_re.is_match(branch_prefix) {
        return Err(Error::new(
            "Branch prefix contains one or more forbidden characters.",
        ));
    }

    if branch_prefix.contains("//") || branch_prefix.starts_with('/') {
        return Err(Error::new(
            "Branch prefix contains multiple consecutive slashes or starts with slash.",
        ));
    }

    if branch_prefix.contains("@{") {
        return Err(Error::new("Branch prefix cannot contain the sequence @{"));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validate_branch_prefix;

    #[test]
    fn test_branch_prefix_rules() {
        // Rules taken from https://git-scm.com/docs/git-check-ref-format
        // Note: Some rules don't need to be checked because the prefix is
        // always embedded into a larger context. For example, rule 9 in the
        // reference states that a _refname_ cannot be the single character @.
        // This rule is impossible to break purely via the branch prefix.
        let bad_prefixes: Vec<(&str, &str)> = vec![
            (
                "spr/.bad",
                "Cannot start slash-separated component with dot",
            ),
            (".bad", "Cannot start slash-separated component with dot"),
            ("spr/bad.lock", "Cannot end with .lock"),
            (
                "spr/bad.lock/some_more",
                "Cannot end slash-separated component with .lock",
            ),
            (
                "spr/b..ad/bla",
                "They cannot contain two consecutive dots anywhere",
            ),
            ("spr/bad//bla", "They cannot contain consecutive slashes"),
            ("/bad", "Prefix should not start with slash"),
            ("/bad@{stuff", "Prefix cannot contain sequence @{"),
        ];

        for (branch_prefix, reason) in bad_prefixes {
            assert!(validate_branch_prefix(branch_prefix).is_err(), "{}", reason);
        }

        let ok_prefix = "spr/some.lockprefix/with-stuff/foo";
        assert!(validate_branch_prefix(ok_prefix).is_ok());
    }

    #[test]
    fn test_branch_prefix_rejects_forbidden_characters() {
        // Here I'm mostly concerned about escaping / not escaping in the regex :p
        assert!(validate_branch_prefix("bad\x1F").is_err());
        assert!(validate_branch_prefix("notbad!").is_ok());
        assert!(
            validate_branch_prefix("bad /space").is_err(),
            "Reject space in prefix"
        );
        assert!(validate_branch_prefix("bad~").is_err(), "Reject tilde");
        assert!(validate_branch_prefix("bad^").is_err(), "Reject caret");
        assert!(validate_branch_prefix("bad:").is_err(), "Reject colon");
        assert!(validate_branch_prefix("bad?").is_err(), "Reject ?");
        assert!(validate_branch_prefix("bad*").is_err(), "Reject *");
        assert!(validate_branch_prefix("bad[").is_err(), "Reject [");
        assert!(validate_branch_prefix(r"bad\").is_err(), "Reject \\");
    }
}
