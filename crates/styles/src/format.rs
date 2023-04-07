use crate::color;
use once_cell::sync::Lazy;
use regex::{Captures, Regex};
use std::path::Path;
use std::sync::Mutex;

static STYLE_TOKEN: Lazy<Mutex<Regex>> =
    Lazy::new(|| Mutex::new(Regex::new(r#"<(\w+)>([^<>]+)</(\w+)>"#).unwrap()));

#[inline]
pub fn format_style_tags<T: AsRef<str>>(value: T) -> String {
    String::from(
        STYLE_TOKEN
            .lock()
            .unwrap()
            .replace_all(value.as_ref(), |caps: &Captures| {
                let token = caps.get(1).map_or("", |m| m.as_str());
                let inner = caps.get(2).map_or("", |m| m.as_str());

                match token {
                    "accent" => color::muted(inner),
                    "failure" => color::failure(inner),
                    "file" => color::file(inner),
                    "hash" => color::hash(inner),
                    "id" => color::id(inner),
                    "invalid" => color::invalid(inner),
                    "label" | "target" => color::label(inner),
                    "muted" => color::muted_light(inner),
                    "path" => color::path(Path::new(inner)),
                    "shell" => color::shell(inner),
                    "success" => color::success(inner),
                    "symbol" => color::symbol(inner),
                    "url" => color::url(inner),
                    _ => String::from(inner),
                }
            }),
    )
}
