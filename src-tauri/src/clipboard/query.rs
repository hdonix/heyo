//! Understanding what is typed into the clipboard search: when ("昨天下午",
//! "last week"), from where ("from firefox", "终端里的"), what kind ("链接"),
//! and the words to look for, turned into full-text queries.
//!
//! All by rule, so it is instant and predictable. A reading that finds
//! nothing is dropped in favour of taking the whole query as words (see
//! `Store::search`), so a query that merely looks like a time is not lost.

use std::sync::LazyLock;

use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, NaiveTime, TimeZone};
use regex::Regex;
use serde::Serialize;

use super::Kind;

/// A span of time the query names.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Span {
    /// Milliseconds since the epoch; the end is exclusive.
    pub start: i64,
    pub end: i64,
    /// The words that named it.
    pub text: String,
}

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

fn cjk(c: char) -> bool {
    matches!(c as u32, 0x3040..=0x30ff | 0x3400..=0x4dbf | 0x4e00..=0x9fff | 0xac00..=0xd7af | 0xf900..=0xfaff | 0x20000..=0x2a6df)
}

/// Text as it is indexed for word search: every Chinese, Japanese or Korean
/// character a word of its own (those languages do not put spaces between
/// words, and SQLite's tokenizer would take a whole sentence for one), the
/// rest as it is.
pub fn words(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + text.len() / 2);
    for c in text.chars() {
        if cjk(c) {
            out.push(' ');
            out.push(c);
            out.push(' ');
        } else {
            out.push(c);
        }
    }
    out
}

/// How one term is looked up.
#[derive(Debug, PartialEq)]
pub enum Lookup {
    /// A phrase in the word index (the last word may be only begun).
    Words(String),
    /// A substring, through the trigram index: for URLs, paths, identifiers.
    Substring(String),
    /// Too short and too odd for either index (`=>`): a plain scan.
    Scan(String),
}

pub fn lookup(term: &str) -> Lookup {
    let has_word = |s: &str| s.chars().any(char::is_alphanumeric);
    if term.chars().any(cjk) {
        // A run of characters is a phrase of one-character words, which finds
        // any substring of any length; a trigram index finds nothing shorter
        // than three, and most Chinese words are two.
        Lookup::Words(format!("\"{}\" *", words(term).replace('"', " ")))
    } else if term.chars().count() >= 3 {
        Lookup::Substring(format!("\"{}\"", term.replace('"', "\"\"")))
    } else if has_word(term) {
        Lookup::Words(format!("\"{}\" *", term.replace('"', " ")))
    } else {
        Lookup::Scan(term.to_owned())
    }
}

type Local_ = DateTime<Local>;

fn midnight(date: NaiveDate) -> Local_ {
    // The first valid instant of the day, where a clock change skips midnight.
    Local.from_local_datetime(&date.and_time(NaiveTime::MIN)).earliest().unwrap_or_else(|| Local.from_utc_datetime(&date.and_time(NaiveTime::MIN)))
}

fn days(from: NaiveDate, count: i64) -> (Local_, Local_) {
    (midnight(from), midnight(from + Duration::days(count)))
}

fn number(text: &str) -> Option<i64> {
    if let Ok(n) = text.parse() {
        return Some(n);
    }
    let digit = |c| "零一二三四五六七八九".chars().position(|d| d == c).map(|p| p as i64);
    match text.chars().collect::<Vec<_>>()[..] {
        ['两'] => Some(2),
        ['十'] => Some(10),
        [a] => digit(a),
        ['十', b] => Some(10 + digit(b)?),
        [a, '十'] => Some(digit(a)? * 10),
        [a, '十', b] => Some(digit(a)? * 10 + digit(b)?),
        _ => None,
    }
}

/// Monday = 0.
fn weekday(text: &str) -> Option<i64> {
    let text = text.to_lowercase();
    let english = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"];
    if let Some(day) = english.iter().position(|d| text.starts_with(d)) {
        return Some(day as i64);
    }
    "一二三四五六日".chars().position(|c| text.ends_with(c)).map(|p| p as i64).or_else(|| text.ends_with('天').then_some(6))
}

const N: &str = r"(\d{1,3}|[零一二两三四五六七八九十]{1,3})";

/// The day or days a query names, with the words that named them.
fn day_span(query: &str, now: Local_) -> Option<(std::ops::Range<usize>, Local_, Local_)> {
    type Rule = (Regex, fn(&regex::Captures, Local_) -> Option<(Local_, Local_)>);
    static RULES: LazyLock<Vec<Rule>> = LazyLock::new(|| {
        let rule = |pattern: &str, f: fn(&regex::Captures, Local_) -> Option<(Local_, Local_)>| (Regex::new(&format!("(?i){pattern}")).unwrap(), f);
        vec![
            rule(r"刚才|刚刚|\bjust now\b", |_, now| Some((now - Duration::minutes(15), now + Duration::seconds(1)))),
            rule(&format!(r"(?:最近|过去|近|这){N}个?小时|\b(?:last|past) (\d{{1,3}}) hours?\b"), |c, now| {
                let n = number(c.get(1).or(c.get(2))?.as_str())?;
                Some((now - Duration::hours(n), now + Duration::seconds(1)))
            }),
            rule(&format!(r"{N}个?小时前|\b(\d{{1,3}}) hours? ago\b"), |c, now| {
                let n = number(c.get(1).or(c.get(2))?.as_str())?;
                Some((now - Duration::hours(n + 1), now - Duration::hours((n - 1).max(0))))
            }),
            rule(&format!(r"(?:最近|过去|近|这){N}天|\b(?:last|past) (\d{{1,3}}) days?\b"), |c, now| {
                let n = number(c.get(1).or(c.get(2))?.as_str())?;
                Some((midnight(now.date_naive() - Duration::days(n - 1)), now + Duration::seconds(1)))
            }),
            rule(&format!(r"{N}天前|\b(\d{{1,3}}) days? ago\b"), |c, now| {
                let n = number(c.get(1).or(c.get(2))?.as_str())?;
                Some(days(now.date_naive() - Duration::days(n), 1))
            }),
            rule(r"(\d{4})[-/年](\d{1,2})[-/月](\d{1,2})[日号]?", |c, _| {
                let date = NaiveDate::from_ymd_opt(c[1].parse().ok()?, c[2].parse().ok()?, c[3].parse().ok()?)?;
                Some(days(date, 1))
            }),
            rule(&format!(r"{N}月{N}[日号]"), |c, now| {
                let (month, day) = (number(&c[1])? as u32, number(&c[2])? as u32);
                let date = NaiveDate::from_ymd_opt(now.year(), month, day)?;
                // A date still to come this year means last year's.
                let date = if date > now.date_naive() { NaiveDate::from_ymd_opt(now.year() - 1, month, day)? } else { date };
                Some(days(date, 1))
            }),
            rule(r"大前天", |_, now| Some(days(now.date_naive() - Duration::days(3), 1))),
            rule(r"前天|\bthe day before yesterday\b", |_, now| Some(days(now.date_naive() - Duration::days(2), 1))),
            rule(r"昨[天日晚]|\byesterday\b|\blast night\b", |_, now| Some(days(now.date_naive() - Duration::days(1), 1))),
            rule(r"今[天日早晚]|\btoday\b|\btonight\b|\bthis (?:morning|afternoon|evening)\b", |_, now| Some(days(now.date_naive(), 1))),
            // "上周三", "last Tuesday": that day of last week. "上周": all of it.
            rule(r"(上上|上)(?:周|星期|礼拜)([一二三四五六日天])?|\blast (week|mon|tue|wed|thu|fri|sat|sun)[a-z]*\b", |c, now| {
                let monday = now.date_naive() - Duration::days(now.weekday().num_days_from_monday() as i64);
                if let Some(english) = c.get(3) {
                    if english.as_str().eq_ignore_ascii_case("week") {
                        return Some(days(monday - Duration::days(7), 7));
                    }
                    // In English "last Tuesday" is the most recent one gone by.
                    let day = weekday(english.as_str())?;
                    let back = (now.weekday().num_days_from_monday() as i64 - day + 7 - 1) % 7 + 1;
                    return Some(days(now.date_naive() - Duration::days(back), 1));
                }
                let weeks = if &c[1] == "上上" { 2 } else { 1 };
                let start = monday - Duration::days(7 * weeks);
                match c.get(2) {
                    Some(day) => Some(days(start + Duration::days(weekday(day.as_str())?), 1)),
                    None => Some(days(start, 7)),
                }
            }),
            rule(r"(?:本|这个?)(?:周|星期|礼拜)|\bthis week\b", |_, now| {
                let monday = now.date_naive() - Duration::days(now.weekday().num_days_from_monday() as i64);
                Some(days(monday, 7))
            }),
            // "周三", "Tuesday": the most recent one, today included.
            rule(r"(?:周|星期|礼拜)([一二三四五六日天])|\b(?:on )?(monday|tuesday|wednesday|thursday|friday|saturday|sunday)\b", |c, now| {
                let day = weekday(c.get(1).or(c.get(2))?.as_str())?;
                let back = (now.weekday().num_days_from_monday() as i64 - day + 7) % 7;
                Some(days(now.date_naive() - Duration::days(back), 1))
            }),
            rule(r"上个?月|\blast month\b", |_, now| {
                let first = now.date_naive().with_day(1)?;
                let previous = (first - Duration::days(1)).with_day(1)?;
                Some((midnight(previous), midnight(first)))
            }),
            rule(r"(?:本|这个?)月|\bthis month\b", |_, now| {
                let first = now.date_naive().with_day(1)?;
                let next = (first + Duration::days(32)).with_day(1)?;
                Some((midnight(first), midnight(next)))
            }),
            rule(r"最近|近期|\brecent(?:ly)?\b", |_, now| Some((midnight(now.date_naive() - Duration::days(6)), now + Duration::seconds(1)))),
        ]
    });
    RULES.iter().find_map(|(pattern, range)| {
        let captures = pattern.captures(query)?;
        let found = captures.get(0)?.range();
        if !stands_alone(query, &found) {
            return None;
        }
        let (start, end) = range(&captures, now)?;
        Some((found, start, end))
    })
}

/// Whether what was found is an expression of its own and not a piece of
/// something else: the "monday" of "monday.com", the date in a file name.
/// (Chinese needs no such care: it has no spaces to tell by.)
fn stands_alone(query: &str, found: &std::ops::Range<usize>) -> bool {
    if !query[found.clone()].is_ascii() {
        return true;
    }
    let before = query[..found.start].chars().next_back();
    let after = query[found.end..].chars().next();
    [before, after].into_iter().flatten().all(|c| c.is_whitespace() || !c.is_ascii())
}

/// The hours a part of the day covers.
fn part_of_day(query: &str) -> Option<(std::ops::Range<usize>, u32, u32)> {
    static PARTS: LazyLock<Vec<(Regex, u32, u32)>> = LazyLock::new(|| {
        [
            (r"凌晨", 0, 6),
            (r"早上|上午|今早|\b(?:this |in the )?morning\b", 5, 12),
            (r"中午|\b(?:at )?noon\b", 11, 14),
            (r"下午|\b(?:this |in the )?afternoon\b", 12, 18),
            (r"晚上|今晚|昨晚|\b(?:this |in the )?evening\b|\btonight\b|\blast night\b", 18, 24),
        ]
        .into_iter()
        .map(|(pattern, from, to)| (Regex::new(&format!("(?i){pattern}")).unwrap(), from, to))
        .collect()
    });
    PARTS.iter().find_map(|(pattern, from, to)| {
        let found = pattern.find(query)?.range();
        stands_alone(query, &found).then_some((found, *from, *to))
    })
}

/// Takes the time expression out of the query.
fn take_time(query: &mut String, now: Local_) -> Option<Span> {
    let part = part_of_day(query);
    let day = day_span(query, now);
    let (mut start, mut end) = match (&day, &part) {
        (Some((_, start, end)), _) => (*start, *end),
        // "下午" alone: this afternoon.
        (None, Some(_)) => days(now.date_naive(), 1),
        (None, None) => return None,
    };
    let mut cut: Vec<std::ops::Range<usize>> = day.iter().map(|d| d.0.clone()).collect();
    // A part of the day narrows a single day; "上周下午" means nothing.
    if let Some((range, from, to)) = part {
        if (Duration::hours(23)..=Duration::hours(25)).contains(&(end - start)) {
            let date = start.date_naive();
            start = midnight(date) + Duration::hours(from as i64);
            end = midnight(date) + Duration::hours(to as i64);
            cut.push(range);
        }
    }
    // One span of the query, unless the two are apart ("昨天 的 下午" is not
    // worth the trouble): "今晚" is both a day and a part of it.
    cut.sort_by_key(|range| range.start);
    let mut merged: Vec<std::ops::Range<usize>> = Vec::new();
    for range in cut {
        match merged.last_mut() {
            Some(last) if query[last.end.min(range.start)..range.start].trim().is_empty() => last.end = last.end.max(range.end),
            _ => merged.push(range),
        }
    }
    let text = merged.iter().map(|range| query[range.clone()].trim()).collect::<Vec<_>>().join(" ");
    for range in merged.iter().rev() {
        query.replace_range(range.clone(), " ");
    }
    Some(Span { start: start.timestamp_millis(), end: end.timestamp_millis(), text })
}

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
        Local.with_ymd_and_hms(2025, 10, 8, 15, 30, 0).unwrap()
    }

    fn at(month: u32, day: u32, hour: u32) -> i64 {
        Local.with_ymd_and_hms(2025, month, day, 0, 0, 0).unwrap().timestamp_millis() + hour as i64 * 3_600_000
    }

    fn read(query: &str) -> Query {
        parse(query, now(), &["firefox".into(), "org.gnome.Terminal".into(), "Code".into()])
    }

    fn span(query: &str) -> (i64, i64) {
        let time = read(query).time.unwrap_or_else(|| panic!("no time in {query:?}"));
        (time.start, time.end)
    }

    #[test]
    fn days_in_chinese_and_english() {
        assert_eq!(span("昨天的地址"), (at(10, 7, 0), at(10, 8, 0)));
        assert_eq!(span("yesterday address"), (at(10, 7, 0), at(10, 8, 0)));
        assert_eq!(span("今天"), (at(10, 8, 0), at(10, 9, 0)));
        assert_eq!(span("前天"), (at(10, 6, 0), at(10, 7, 0)));
        assert_eq!(span("3天前"), (at(10, 5, 0), at(10, 6, 0)));
        assert_eq!(span("三天前"), (at(10, 5, 0), at(10, 6, 0)));
        assert_eq!(span("2 days ago"), (at(10, 6, 0), at(10, 7, 0)));
        assert_eq!(span("10月1号"), (at(10, 1, 0), at(10, 2, 0)));
        assert_eq!(span("2025-10-03 invoice"), (at(10, 3, 0), at(10, 4, 0)));
        // Not yet come this year: last year's.
        assert_eq!(read("12月25日").time.unwrap().start, Local.with_ymd_and_hms(2024, 12, 25, 0, 0, 0).unwrap().timestamp_millis());
    }

    #[test]
    fn weeks_and_months() {
        // The week of Monday 29 September.
        assert_eq!(span("上周"), (at(9, 29, 0), at(10, 6, 0)));
        assert_eq!(span("last week"), (at(9, 29, 0), at(10, 6, 0)));
        assert_eq!(span("上周三"), (at(10, 1, 0), at(10, 2, 0)));
        assert_eq!(span("上上周"), (at(9, 22, 0), at(9, 29, 0)));
        assert_eq!(span("本周"), (at(10, 6, 0), at(10, 13, 0)));
        assert_eq!(span("周一"), (at(10, 6, 0), at(10, 7, 0)));
        // Today is a Wednesday: "周三" is today, "last Wednesday" a week ago.
        assert_eq!(span("星期三"), (at(10, 8, 0), at(10, 9, 0)));
        assert_eq!(span("last wednesday"), (at(10, 1, 0), at(10, 2, 0)));
        assert_eq!(span("last tuesday"), (at(10, 7, 0), at(10, 8, 0)));
        assert_eq!(span("上个月"), (at(9, 1, 0), at(10, 1, 0)));
        assert_eq!(span("this month"), (at(10, 1, 0), at(11, 1, 0)));
        assert_eq!(span("最近三天").0, at(10, 6, 0));
        assert_eq!(span("last 7 days").0, at(10, 2, 0));
    }

    #[test]
    fn parts_of_the_day() {
        assert_eq!(span("昨天下午"), (at(10, 7, 12), at(10, 7, 18)));
        assert_eq!(span("yesterday afternoon"), (at(10, 7, 12), at(10, 7, 18)));
        assert_eq!(span("下午"), (at(10, 8, 12), at(10, 8, 18)));
        assert_eq!(span("昨晚"), (at(10, 7, 18), at(10, 8, 0)));
        assert_eq!(span("今早"), (at(10, 8, 5), at(10, 8, 12)));
        // A part of the day does not narrow a week.
        assert_eq!(span("上周"), (at(9, 29, 0), at(10, 6, 0)));
        let recent = read("刚才复制的").time.unwrap();
        assert_eq!(recent.end - recent.start, 15 * 60_000 + 1000);
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

    #[test]
    fn lookups() {
        assert_eq!(words("发票abc 2024"), " 发  票 abc 2024");
        assert_eq!(lookup("发票"), Lookup::Words("\" 发  票 \" *".into()));
        assert_eq!(lookup("github.com/fla"), Lookup::Substring("\"github.com/fla\"".into()));
        assert_eq!(lookup("ab"), Lookup::Words("\"ab\" *".into()));
        assert_eq!(lookup("=>"), Lookup::Scan("=>".into()));
        assert_eq!(lookup("say\"hi"), Lookup::Substring("\"say\"\"hi\"".into()));
    }
}
