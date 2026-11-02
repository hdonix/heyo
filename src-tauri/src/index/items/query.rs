//! Understanding what is typed into a search of the connections' items:
//! when ("昨天下午", "下周三", "tomorrow"), from or to whom ("from:张三",
//! "张三发的", "from zhang"), what kind ("邮件", "meetings", "有附件"), and
//! the words to look for.
//!
//! By rule, as the clipboard's query is (`clipboard/query.rs`). A person is
//! read out of a query only when someone of that name or address is among
//! the items, unless the query says so outright ("from:"), so that "letter
//! from grandma" stays words; and a reading that finds nothing is taken back
//! (`Items::search`).

use std::sync::LazyLock;

use chrono::{DateTime, Local};
use regex::Regex;

use super::{ItemKind, PersonRole};
use crate::index::text::cjk;
use crate::index::when::{take_time_within, Reach, Span};

/// Someone an item is from or to.
#[derive(Debug, Clone, PartialEq)]
pub struct Someone {
    /// Part of a name, or the beginning of an address; lower case.
    pub text: String,
    /// The parts they may have had: "from" is the sender of a mail and the
    /// organizer of a meeting.
    pub roles: &'static [PersonRole],
}

const FROM: &[PersonRole] = &[PersonRole::From, PersonRole::Organizer];
const TO: &[PersonRole] = &[PersonRole::To, PersonRole::Cc, PersonRole::Attendee];
const ANY: &[PersonRole] = &[PersonRole::From, PersonRole::To, PersonRole::Cc, PersonRole::Organizer, PersonRole::Attendee];

/// What a query asks for.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Query {
    pub time: Option<Span>,
    pub someone: Option<Someone>,
    pub kind: Option<ItemKind>,
    /// Only items that have attachments.
    pub attachments: bool,
    /// The words to find, each of which must occur.
    pub terms: Vec<String>,
}

impl Query {
    /// Whether anything but words was read out of the query.
    pub fn filtered(&self) -> bool {
        self.time.is_some() || self.someone.is_some() || self.kind.is_some() || self.attachments
    }

    /// The query taken as nothing but words.
    pub fn literal(query: &str) -> Query {
        Query { terms: query.split_whitespace().map(str::to_owned).collect(), ..Query::default() }
    }
}

/// Takes "有附件", "with attachments" out of the query.
fn take_attachments(query: &mut String) -> bool {
    static PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)(?:有|带|含)附件的?|\bwith attachments?\b|\bhas:attachments?\b").unwrap());
    let Some(found) = PATTERN.find(query) else { return false };
    query.replace_range(found.range(), " ");
    true
}

/// Takes the person out of the query: "from:张三", "to:li@example.com",
/// "with:wang", "from zhang", "张三发的", "发给李四的", "来自张三的".
/// `known` says whether some item is from or to someone of a name or
/// address beginning so.
fn take_someone(query: &mut String, known: &dyn Fn(&str) -> bool) -> Option<Someone> {
    static OUTRIGHT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\b(from|to|with|cc):(\S+)").unwrap());
    // (pattern, roles): the name is the first group.
    static PATTERNS: LazyLock<Vec<(Regex, &'static [PersonRole])>> = LazyLock::new(|| {
        [
            (r"(?i)\bfrom ([\w.@+-]+)", FROM),
            (r"(?i)\b(?:sent to|invited) ([\w.@+-]+)", TO),
            (r"(?i)\bwith ([\w.@+-]+)", ANY),
            (r"发给\s*(\S+?)\s*的", TO),
            (r"来自\s*(\S+?)\s*的", FROM),
            (r"(\S+?)\s*发(?:来|过来)?的", FROM),
            (r"(?:和|跟|与)\s*(\S+?)\s*的", ANY),
        ]
        .into_iter()
        .map(|(pattern, roles)| (Regex::new(pattern).unwrap(), roles))
        .collect()
    });
    if let Some(captures) = OUTRIGHT.captures(query) {
        let roles = match captures[1].to_lowercase().as_str() {
            "from" => FROM,
            "to" | "cc" => TO,
            _ => ANY,
        };
        let text = captures[2].to_lowercase();
        query.replace_range(captures.get(0).unwrap().range(), " ");
        return Some(Someone { text, roles });
    }
    for (pattern, roles) in PATTERNS.iter() {
        let Some(captures) = pattern.captures(query) else { continue };
        let name = captures.get(1).unwrap();
        // Chinese runs on: of "关于预算张三发的", the name is the part at the
        // end that is someone's, the rest words.
        let starts: Vec<usize> = if name.as_str().chars().any(cjk) { name.as_str().char_indices().map(|(at, _)| name.start() + at).collect() } else { vec![name.start()] };
        for start in starts {
            let text = query[start..name.end()].to_lowercase();
            if text.chars().count() >= 2 && known(&text) {
                let whole = captures.get(0).unwrap();
                // What came before the name in the pattern goes too ("发给", "from").
                let from = if start == name.start() { whole.start() } else { start };
                query.replace_range(from..whole.end(), " ");
                return Some(Someone { text, roles });
            }
        }
    }
    None
}

/// The kind a word names. `alone`: whether it is all there is besides a
/// time and a person; a word in the singular ("meeting notes") is a kind
/// only then. Chinese words and English plurals are kinds anywhere.
fn kind_of(word: &str, alone: bool) -> Option<ItemKind> {
    let word = word.to_lowercase();
    let anywhere = match word.as_str() {
        "邮件" | "电邮" | "emails" | "mails" => Some(ItemKind::Email),
        "会议" | "日程" | "约会" | "meetings" | "events" | "appointments" => Some(ItemKind::Event),
        "消息" | "聊天" | "messages" | "chats" => Some(ItemKind::Message),
        "文档" | "documents" | "docs" => Some(ItemKind::Document),
        "任务" | "待办" | "tasks" | "todos" => Some(ItemKind::Task),
        _ => None,
    };
    let alone_only = || match word.as_str() {
        "email" | "mail" | "e-mail" | "inbox" => Some(ItemKind::Email),
        "meeting" | "event" | "calendar" | "appointment" => Some(ItemKind::Event),
        "message" | "chat" => Some(ItemKind::Message),
        "document" | "doc" => Some(ItemKind::Document),
        "task" | "todo" => Some(ItemKind::Task),
        _ => None,
    };
    anywhere.or_else(|| alone.then(alone_only).flatten())
}

/// Reads a query asked at `now`. `known`: whether some item is from or to
/// someone whose name has this in it, or whose address begins so.
pub fn parse(query: &str, now: DateTime<Local>, known: &dyn Fn(&str) -> bool) -> Query {
    let mut rest = query.to_owned();
    let time = take_time_within(&mut rest, now, Reach::Ahead);
    let attachments = take_attachments(&mut rest);
    let someone = take_someone(&mut rest, known);
    // What is left of "上周张三发的邮件" is "邮件"; of "上周的会议", "的会议".
    let mut terms: Vec<String> = rest
        .split_whitespace()
        .map(|term| term.strip_prefix('的').filter(|rest| !rest.is_empty()).unwrap_or(term))
        .filter(|term| *term != "的")
        .map(str::to_owned)
        .collect();
    if let Some(last) = terms.last_mut() {
        if let Some(stripped) = last.strip_suffix('的').filter(|rest| !rest.is_empty()) {
            *last = stripped.to_owned();
        }
    }
    let alone = terms.len() == 1;
    let kind = terms.iter().position(|term| kind_of(term, alone).is_some()).map(|at| kind_of(&terms.remove(at), alone).unwrap());
    let mut query = Query { time, someone, kind, attachments, terms };
    // "emails from zhang about the budget": the words are "budget".
    if query.filtered() {
        query.terms.retain(|term| !matches!(term.to_lowercase().as_str(), "about" | "关于" | "the" | "of" | "'s"));
        for term in &mut query.terms {
            if let Some(stripped) = term.strip_suffix("'s").filter(|rest| !rest.is_empty()) {
                *term = stripped.to_owned();
            }
        }
    }
    query
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    /// Wednesday 8 October 2025, 15:30.
    fn now() -> DateTime<Local> {
        Local.with_ymd_and_hms(2025, 10, 8, 15, 30, 0).unwrap()
    }

    fn read(query: &str) -> Query {
        let people = ["张三", "李四", "zhang san", "zhang.san@example.com", "li@example.com"];
        parse(query, now(), &|text| people.iter().any(|known| known.contains(text) || known.starts_with(text)))
    }

    fn someone(text: &str, roles: &'static [PersonRole]) -> Option<Someone> {
        Some(Someone { text: text.into(), roles })
    }

    fn terms(query: &Query) -> Vec<&str> {
        query.terms.iter().map(String::as_str).collect()
    }

    #[test]
    fn people_in_chinese_and_english() {
        assert_eq!(read("from:张三").someone, someone("张三", FROM));
        assert_eq!(read("from:Nobody@x.org 预算").someone, someone("nobody@x.org", FROM));
        assert_eq!(terms(&read("from:Nobody@x.org 预算")), ["预算"]);
        assert_eq!(read("to:li").someone, someone("li", TO));
        assert_eq!(read("张三发的").someone, someone("张三", FROM));
        assert_eq!(read("张三发来的合同").someone, someone("张三", FROM));
        assert_eq!(terms(&read("张三发来的合同")), ["合同"]);
        assert_eq!(read("发给李四的报价").someone, someone("李四", TO));
        assert_eq!(terms(&read("发给李四的报价")), ["报价"]);
        assert_eq!(read("和张三的会议").someone, someone("张三", ANY));
        assert_eq!(read("和张三的会议").kind, Some(ItemKind::Event));
        // The name at the end of a run of characters, the rest words.
        let query = read("预算张三发的");
        assert_eq!((query.someone.clone(), terms(&query)), (someone("张三", FROM), vec!["预算"]));
        assert_eq!(read("budget from zhang").someone, someone("zhang", FROM));
        assert_eq!(terms(&read("budget from zhang")), ["budget"]);
        assert_eq!(read("from zhang.san@").someone, someone("zhang.san@", FROM));
        // Nobody of that name: words.
        assert_eq!(read("letter from grandma"), Query::literal("letter from grandma"));
        assert_eq!(read("王五发的").someone, None);
        assert_eq!(read("with sugar").someone, None);
    }

    #[test]
    fn kinds_and_attachments() {
        assert_eq!(read("邮件").kind, Some(ItemKind::Email));
        assert_eq!(read("mail").kind, Some(ItemKind::Email));
        assert_eq!(read("上周的会议").kind, Some(ItemKind::Event));
        assert_eq!(read("上周的会议").terms, Vec::<String>::new());
        assert_eq!(read("日程").kind, Some(ItemKind::Event));
        assert_eq!(read("meetings tomorrow").kind, Some(ItemKind::Event));
        assert!(read("meetings tomorrow").time.is_some());
        let query = read("张三发的邮件 预算");
        assert_eq!((query.kind, query.someone.is_some(), terms(&query)), (Some(ItemKind::Email), true, vec!["预算"]));
        assert_eq!(read("emails from zhang about budget").kind, Some(ItemKind::Email));
        assert_eq!(terms(&read("emails from zhang about budget")), ["budget"]);
        // A singular word with others is a word.
        assert_eq!(read("meeting notes"), Query::literal("meeting notes"));
        assert_eq!(read("会议纪要"), Query::literal("会议纪要"));

        let query = read("有附件的邮件");
        assert!(query.attachments);
        assert_eq!((query.kind, query.terms.len()), (Some(ItemKind::Email), 0));
        assert!(read("invoice with attachments").attachments);
        assert_eq!(terms(&read("invoice with attachments")), ["invoice"]);
        assert!(read("带附件").attachments);
    }

    #[test]
    fn times_ahead_and_behind() {
        let query = read("明天的会议");
        assert_eq!((query.kind, query.time.unwrap().text), (Some(ItemKind::Event), "明天".to_owned()));
        assert_eq!(read("昨天下午张三发的邮件").someone, someone("张三", FROM));
        assert_eq!(read("昨天下午张三发的邮件").time.unwrap().text, "昨天下午");
        assert!(read("next week").time.is_some());
        assert!(!read("发票").filtered());
    }
}
