/*
 * Copyright (c) Radical HQ Limited
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use crate::{
    error::{Error, Result},
    output::output,
};

pub type MessageSectionsMap = std::collections::BTreeMap<MessageSection, String>;

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Debug)]
pub enum MessageSection {
    Title,
    Summary,
    Reviewers,
    ReviewedBy,
    PullRequest,
    CherryPick,
}

pub fn message_section_label(section: &MessageSection) -> &'static str {
    use MessageSection::*;

    match section {
        Title => "Title",
        Summary => "Summary",
        Reviewers => "Reviewers",
        ReviewedBy => "Reviewed By",
        PullRequest => "Pull Request",
        CherryPick => "Cherry Pick",
    }
}

pub fn message_section_by_label(label: &str) -> Option<MessageSection> {
    use MessageSection::*;

    match &label.to_ascii_lowercase()[..] {
        "title" => Some(Title),
        "summary" => Some(Summary),
        "reviewer" => Some(Reviewers),
        "reviewers" => Some(Reviewers),
        "reviewed by" => Some(ReviewedBy),
        "pull request" => Some(PullRequest),
        "cherry pick" => Some(CherryPick),
        _ => None,
    }
}

pub fn parse_message(msg: &str, top_section: MessageSection) -> MessageSectionsMap {
    let regex = lazy_regex::regex!(r#"^\s*([\w\s]+?)\s*:\s*(.*)$"#);

    let mut section = top_section;
    let mut lines_in_section = Vec::<&str>::new();
    let mut sections = std::collections::BTreeMap::<MessageSection, String>::new();

    for (lineno, line) in msg
        .trim()
        .split('\n')
        .map(|line| line.trim_end())
        .enumerate()
    {
        if let Some(caps) = regex.captures(line) {
            let label = caps.get(1).unwrap().as_str();
            let payload = caps.get(2).unwrap().as_str();

            if let Some(new_section) = message_section_by_label(label) {
                append_to_message_section(
                    sections.entry(section),
                    lines_in_section.join("\n").trim(),
                );
                section = new_section;
                lines_in_section = vec![payload];
                continue;
            }
        }

        if lineno == 0 && top_section == MessageSection::Title {
            sections.insert(top_section, line.to_string());
            section = MessageSection::Summary;
        } else {
            lines_in_section.push(line);
        }
    }

    if !lines_in_section.is_empty() {
        append_to_message_section(sections.entry(section), lines_in_section.join("\n").trim());
    }

    sections
}

fn append_to_message_section(
    entry: std::collections::btree_map::Entry<MessageSection, String>,
    text: &str,
) {
    if !text.is_empty() {
        entry
            .and_modify(|value| {
                if value.is_empty() {
                    *value = text.to_string();
                } else {
                    *value = format!("{}\n\n{}", value, text);
                }
            })
            .or_insert_with(|| text.to_string());
    } else {
        entry.or_default();
    }
}

pub fn build_message(section_texts: &MessageSectionsMap, sections: &[MessageSection]) -> String {
    let mut result = String::new();
    let mut display_label = false;

    for section in sections {
        let value = section_texts.get(section);
        if let Some(text) = value {
            if !result.is_empty() {
                result.push('\n');
            }

            if section != &MessageSection::Title && section != &MessageSection::Summary {
                // Once we encounter a section that's neither Title nor Summary,
                // we start displaying the labels.
                display_label = true;
            }

            if display_label {
                let label = message_section_label(section);
                result.push_str(label);
                result.push_str(if label.len() + text.len() > 76 || text.contains('\n') {
                    ":\n"
                } else {
                    ": "
                });
            }

            result.push_str(text);
            result.push('\n');
        }
    }

    result.trim().to_owned()
}

pub fn build_commit_message(section_texts: &MessageSectionsMap) -> String {
    build_message(
        section_texts,
        &[
            MessageSection::Title,
            MessageSection::Summary,
            MessageSection::Reviewers,
            MessageSection::ReviewedBy,
            MessageSection::PullRequest,
            MessageSection::CherryPick,
        ],
    )
}

pub fn build_github_body(section_texts: &MessageSectionsMap) -> String {
    build_message(section_texts, &[MessageSection::Summary])
}

pub fn build_github_body_for_merging(section_texts: &MessageSectionsMap) -> String {
    build_message(
        section_texts,
        &[
            MessageSection::Summary,
            MessageSection::Reviewers,
            MessageSection::ReviewedBy,
            MessageSection::PullRequest,
        ],
    )
}

pub fn validate_commit_message(message: &MessageSectionsMap) -> Result<()> {
    let title_missing_or_empty = match message.get(&MessageSection::Title) {
        None => true,
        Some(title) => title.is_empty(),
    };
    if title_missing_or_empty {
        output("💔", "Commit message does not have a title!")?;
        return Err(Error::empty());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    // Note this useful idiom: importing names from outer (for mod tests) scope.
    use super::*;

    #[test]
    fn test_parse_empty() {
        assert_eq!(
            parse_message("", MessageSection::Title),
            [(MessageSection::Title, "".to_string())].into()
        );
    }

    #[test]
    fn test_parse_title() {
        assert_eq!(
            parse_message("Hello", MessageSection::Title),
            [(MessageSection::Title, "Hello".to_string())].into()
        );
        assert_eq!(
            parse_message("Hello\n", MessageSection::Title),
            [(MessageSection::Title, "Hello".to_string())].into()
        );
        assert_eq!(
            parse_message("\n\nHello\n\n", MessageSection::Title),
            [(MessageSection::Title, "Hello".to_string())].into()
        );
    }

    #[test]
    fn test_parse_title_and_summary() {
        assert_eq!(
            parse_message("Hello\nFoo Bar", MessageSection::Title),
            [
                (MessageSection::Title, "Hello".to_string()),
                (MessageSection::Summary, "Foo Bar".to_string())
            ]
            .into()
        );
        assert_eq!(
            parse_message("Hello\n\nFoo Bar", MessageSection::Title),
            [
                (MessageSection::Title, "Hello".to_string()),
                (MessageSection::Summary, "Foo Bar".to_string())
            ]
            .into()
        );
        assert_eq!(
            parse_message("Hello\n\n\nFoo Bar", MessageSection::Title),
            [
                (MessageSection::Title, "Hello".to_string()),
                (MessageSection::Summary, "Foo Bar".to_string())
            ]
            .into()
        );
        assert_eq!(
            parse_message("Hello\n\nSummary:\nFoo Bar", MessageSection::Title),
            [
                (MessageSection::Title, "Hello".to_string()),
                (MessageSection::Summary, "Foo Bar".to_string())
            ]
            .into()
        );
    }

    #[test]
    fn test_parse_sections() {
        assert_eq!(
            parse_message(
                r#"Hello

Summary:
here is
the
summary

Reviewer:    a, b, c"#,
                MessageSection::Title
            ),
            [
                (MessageSection::Title, "Hello".to_string()),
                (MessageSection::Summary, "here is\nthe\nsummary".to_string()),
                (MessageSection::Reviewers, "a, b, c".to_string()),
            ]
            .into()
        );
    }

    #[test]
    fn test_parse_cherry_pick() {
        let map = parse_message(
            "My title\n\nPull Request: https://github.com/x/y/pull/1\nCherry Pick: true",
            MessageSection::Title,
        );
        assert_eq!(
            map.get(&MessageSection::Title).map(String::as_str),
            Some("My title")
        );
        assert_eq!(
            map.get(&MessageSection::PullRequest).map(String::as_str),
            Some("https://github.com/x/y/pull/1")
        );
        assert_eq!(
            map.get(&MessageSection::CherryPick).map(String::as_str),
            Some("true")
        );
    }

    #[test]
    fn test_build_commit_message_with_cherry_pick() {
        let map: MessageSectionsMap = [
            (MessageSection::Title, "My title".to_string()),
            (
                MessageSection::PullRequest,
                "https://github.com/x/y/pull/1".to_string(),
            ),
            (MessageSection::CherryPick, "true".to_string()),
        ]
        .into();
        let output = build_commit_message(&map);
        // Cherry Pick must appear after Pull Request
        let pr_pos = output.find("Pull Request:").unwrap();
        let cp_pos = output.find("Cherry Pick:").unwrap();
        assert!(
            cp_pos > pr_pos,
            "Cherry Pick should appear below Pull Request"
        );
        assert!(output.ends_with("Cherry Pick: true"));
    }

    #[test]
    fn test_roundtrip_cherry_pick() {
        let original = "My title\n\nPull Request: https://github.com/x/y/pull/1\nCherry Pick: true";
        let map = parse_message(original, MessageSection::Title);
        let rebuilt = build_commit_message(&map);
        let reparsed = parse_message(&rebuilt, MessageSection::Title);
        assert_eq!(
            reparsed
                .get(&MessageSection::CherryPick)
                .map(String::as_str),
            Some("true")
        );
        assert_eq!(
            reparsed
                .get(&MessageSection::PullRequest)
                .map(String::as_str),
            Some("https://github.com/x/y/pull/1")
        );
    }

    #[test]
    fn test_build_github_body_for_merging_omits_cherry_pick() {
        let map: MessageSectionsMap = [
            (MessageSection::Title, "My title".to_string()),
            (MessageSection::Summary, "Some summary".to_string()),
            (
                MessageSection::PullRequest,
                "https://github.com/x/y/pull/1".to_string(),
            ),
            (MessageSection::CherryPick, "true".to_string()),
        ]
        .into();
        let body = build_github_body_for_merging(&map);
        assert!(
            !body.contains("Cherry Pick"),
            "GitHub body should not contain Cherry Pick marker"
        );
    }

    #[test]
    fn test_build_message_trims() {
        assert_eq!(
            build_message(
                &[
                    (MessageSection::Title, "  Hello".to_string()),
                    (MessageSection::Summary, "Foo Bar  ".to_string())
                ]
                .into(),
                &[MessageSection::Title, MessageSection::Summary]
            ),
            "Hello\n\nFoo Bar"
        );
        assert_eq!(
            build_message(
                &[
                    (MessageSection::Title, "only title".to_string()),
                    (MessageSection::Summary, "\n".to_string())
                ]
                .into(),
                &[MessageSection::Summary]
            ),
            ""
        );
    }
}
