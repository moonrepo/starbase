use crate::color::{Style, paint_style};
use std::collections::HashMap;
use std::sync::LazyLock;

static TAGS_MAP: LazyLock<HashMap<String, Style>> = LazyLock::new(|| {
    HashMap::from_iter(
        [
            Style::Caution,
            Style::Failure,
            Style::File,
            Style::Hash,
            Style::Id,
            Style::Invalid,
            Style::Label,
            Style::Muted,
            Style::MutedLight,
            Style::Path,
            Style::Property,
            Style::Shell,
            Style::Success,
            Style::Symbol,
            Style::Url,
        ]
        .into_iter()
        .map(|style| (format!("{:?}", style).to_lowercase(), style)),
    )
});

/// Parses a string with HTML-like tags into a list of tagged pieces.
/// For example: `<file>starbase.json</file>`
pub fn parse_tags<T: AsRef<str>>(value: T, panic: bool) -> Vec<(String, Option<String>)> {
    let message = value.as_ref().to_owned();

    if !message.contains('<') {
        return vec![(message, None)];
    }

    let mut results: Vec<(String, Option<String>)> = vec![];

    let mut add_result = |text: &str, tag: Option<String>| {
        if let Some(last) = results.last_mut() {
            if last.1 == tag {
                last.0.push_str(text);
                return;
            }
        }

        results.push((text.to_owned(), tag));
    };

    let mut text = message.as_str();
    let mut tag_stack = vec![];
    let mut tag_count = 0;

    while let Some(open_index) = text.find('<') {
        if let Some(close_index) = text.find('>') {
            let mut tag = text.get(open_index + 1..close_index).unwrap_or_default();

            // Definitely not a tag
            if tag.is_empty() || tag.contains(' ') {
                add_result(text.get(..=open_index).unwrap(), None);

                text = text.get(open_index + 1..).unwrap();
                continue;
            }

            let prev_text = text.get(..open_index).unwrap();

            // Close tag, extract with style
            if tag.starts_with('/') {
                tag = tag.strip_prefix('/').unwrap();

                if tag_stack.is_empty() && panic {
                    panic!("Close tag `{}` found without an open tag", tag);
                }

                let in_tag = tag_stack.last();

                if in_tag.is_some_and(|inner| tag != inner) && panic {
                    panic!(
                        "Close tag `{}` does not much the open tag `{}`",
                        tag,
                        in_tag.as_ref().unwrap()
                    );
                }

                add_result(prev_text, in_tag.map(|_| tag.to_owned()));

                tag_stack.pop();
            }
            // Open tag, preserve the current tag
            else {
                add_result(prev_text, tag_stack.last().cloned());

                tag_stack.push(tag.to_owned());
                tag_count += 1;
            }

            text = text.get(close_index + 1..).unwrap();
        } else {
            add_result(text.get(..=open_index).unwrap(), None);

            text = text.get(open_index + 1..).unwrap();
        }
    }

    // If stack is the same length as the count, then we have a
    // bunch of open tags without closing tags. Let's assume these
    // aren't meant to be style tags...
    if tag_count > 0 && tag_stack.len() == tag_count {
        return vec![(message, None)];
    }

    if !text.is_empty() {
        add_result(text, None);
    }

    results
        .into_iter()
        .filter(|item| !item.0.is_empty())
        .collect()
}

/// Parses a string with HTML-like tags into a list of styled pieces.
/// For example: `<file>starbase.json</file>`
pub fn parse_style_tags<T: AsRef<str>>(value: T) -> Vec<(String, Option<Style>)> {
    let message = value.as_ref();

    if !message.contains('<') {
        return vec![(message.to_owned(), None)];
    }

    parse_tags(message, false)
        .into_iter()
        .map(|(text, tag)| (text, tag.and_then(|tag| TAGS_MAP.get(&tag).cloned())))
        .collect()
}

/// Apply styles to a string by replacing style specific tags.
/// For example: `<file>starbase.json</file>`
pub fn apply_style_tags<T: AsRef<str>>(value: T) -> String {
    let mut result = vec![];

    for (text, style) in parse_style_tags(value) {
        result.push(match style {
            Some(with) => paint_style(with, text),
            None => text,
        });
    }

    result.join("")
}

/// Remove style and tag specific markup from a string.
pub fn remove_style_tags<T: AsRef<str>>(value: T) -> String {
    let mut result = vec![];

    for (text, _) in parse_style_tags(value) {
        result.push(text);
    }

    result.join("")
}
