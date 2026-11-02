//! Understanding what is typed into the clipboard search: when ("昨天下午",
//! "last week"), from where ("from firefox", "终端里的"), what kind ("链接"),
//! and the words to look for. The time is read as `index::when` reads it;
//! the words are looked up as `index::text` says.
//!
//! All by rule, so it is instant and predictable. A reading that finds
//! nothing is dropped in favour of taking the whole query as words (see
//! `Store::search`), so a query that merely looks like a time is not lost.

use std::sync::LazyLock;

use chrono::{DateTime, Local};
use regex::Regex;
use serde::Serialize;

pub use crate::index::when::Span;
use crate::index::when::take_time;

use super::Kind;

/// What a query asks for.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Query {
    pub time: Option<Span>,
    /// Part of the name of the application copied from.
    pub app: Option<String>,
    pub kind: Option<Kind>,
    /// The words to find, each of which must occur.
    pub terms: Vec<String>,
}

impl Query {
    /// Whether anything but words was read out of the query.
    pub fn filtered(&self) -> bool {
        self.time.is_some() || self.app.is_some() || self.kind.is_some()
    }

    /// The query taken as nothing but words.
    pub fn literal(query: &str) -> Query {
        Query { terms: query.split_whitespace().map(str::to_owned).collect(), ..Query::default() }
    }
}

type Local_ = DateTime<Local>;

/// Takes "from firefox", "来自终端", "终端里的", "app:code" out of the query,
/// if what is named is an application something was copied from.
fn take_app(query: &mut String, apps: &[String]) -> Option<String> {
    static PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
        [r"(?i)\bapp:(\S+)", r"(?i)\bfrom ([\w.-]+)", r"(?:来自|在)\s*([A-Za-z0-9_.-]+)\s*(?:里|中|上)?(?:复制)?的?", r"([A-Za-z0-9_.-]+)\s*(?:里|中)(?:复制)?的?"]
            .into_iter()
            .map(|pattern| Regex::new(pattern).unwrap())
            .collect()
    });
    for (index, pattern) in PATTERNS.iter().enumerate() {
        let Some(captures) = pattern.captures(query) else { continue };
        let name = captures[1].to_lowercase();
        // "app:" says so outright; the others must name a known application.
        let known = name.chars().count() >= 2 && apps.iter().any(|app| app.to_lowercase().contains(&name));
        if index == 0 || known {
            let range = captures.get(0).unwrap().range();
            query.replace_range(range, " ");
            return Some(name);
        }
    }
    None
}

fn kind_of(word: &str) -> Option<Kind> {
    Some(match word.to_lowercase().as_str() {
        "链接" | "网址" | "link" | "links" | "url" | "urls" => Kind::Link,
        "图片" | "图" | "截图" | "image" | "images" | "picture" | "pictures" | "screenshot" | "screenshots" => Kind::Image,
        "颜色" | "color" | "colors" | "colour" | "colours" => Kind::Color,
        "文件" | "file" | "files" => Kind::File,
        "文字" | "文本" | "text" => Kind::Text,
        _ => return None,
    })
}

/// Reads a query. `apps` are the applications there are copies from.
pub fn parse(query: &str, now: Local_, apps: &[String]) -> Query {
    let mut rest = query.to_owned();
    let time = take_time(&mut rest, now);
    let app = take_app(&mut rest, apps);
    // What is left of "昨天复制的地址" is "复制的地址"; of "上周的链接", "的链接".
    let mut rest = rest.trim();
    for filler in ["复制的", "拷贝的", "的", "copied"] {
        rest = rest.strip_prefix(filler).unwrap_or(rest).trim_start();
        rest = rest.strip_suffix(filler).unwrap_or(rest).trim_end();
    }
    // A kind is a filter only when nothing else is asked for: "config file"
    // is about the words, "昨天的链接" about links.
    if let Some(kind) = kind_of(rest) {
        return Query { time, app, kind: Some(kind), terms: Vec::new() };
    }
    Query { time, app, kind: None, terms: rest.split_whitespace().map(str::to_owned).collect() }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Wednesday 8 October 2025, 15:30.
    fn now() -> Local_ {
        use chrono::TimeZone;
        Local.with_ymd_and_hms(2025, 10, 8, 15, 30, 0).unwrap()
    }

    fn read(query: &str) -> Query {
        parse(query, now(), &["firefox".into(), "org.gnome.Terminal".into(), "Code".into()])
    }

    #[test]
    fn what_is_left_are_the_words() {
        assert_eq!(read("昨天复制的地址").terms, ["地址"]);
        assert_eq!(read("上周的会议纪要").terms, ["会议纪要"]);
        assert_eq!(read("server address yesterday").terms, ["server", "address"]);
        assert_eq!(read("昨天下午").terms, Vec::<String>::new());
        assert_eq!(read("昨天下午").time.unwrap().text, "昨天下午");
        assert_eq!(read("yesterday afternoon").time.unwrap().text, "yesterday afternoon");
        // No time in these.
        assert_eq!(read("会议纪要"), Query { terms: vec!["会议纪要".into()], ..Query::default() });
        assert_eq!(read("monday.com login").time, None);
        assert_eq!(read("todays").time, None);
    }

    #[test]
    fn sources_and_kinds() {
        assert_eq!(read("token from firefox"), Query { app: Some("firefox".into()), terms: vec!["token".into()], ..Query::default() });
        assert_eq!(read("来自firefox的密钥").app.as_deref(), Some("firefox"));
        assert_eq!(read("terminal里的命令").app.as_deref(), Some("terminal"));
        assert_eq!(read("terminal里的命令").terms, ["命令"]);
        assert_eq!(read("app:code useEffect").app.as_deref(), Some("code"));
        // Not an application anything was copied from: words.
        assert_eq!(read("letter from grandma").app, None);
        assert_eq!(read("心里的话").app, None);

        assert_eq!(read("昨天的链接").kind, Some(Kind::Link));
        assert_eq!(read("images").kind, Some(Kind::Image));
        assert_eq!(read("上周复制的图片").kind, Some(Kind::Image));
        // With other words, a kind is a word too.
        assert_eq!(read("config file"), Query { terms: vec!["config".into(), "file".into()], ..Query::default() });
    }
}
