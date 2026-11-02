//! When a query asks about: "昨天下午", "last week", "3天前", "周三", and,
//! for what has days still to come (a calendar), "明天", "next week". Taken
//! out of the query by rule, so it is instant and predictable.

use std::sync::LazyLock;

use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, NaiveTime, TimeZone};
use regex::Regex;
use serde::Serialize;

/// A span of time the query names.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Span {
    /// Milliseconds since the epoch; the end is exclusive.
    pub start: i64,
    pub end: i64,
    /// The words that named it.
    pub text: String,
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

type Rule = (Regex, fn(&regex::Captures, Local_) -> Option<(Local_, Local_)>);

fn rule(pattern: &str, f: fn(&regex::Captures, Local_) -> Option<(Local_, Local_)>) -> Rule {
    (Regex::new(&format!("(?i){pattern}")).unwrap(), f)
}

/// Whether a query may name days still to come: a copy is never made
/// tomorrow, a meeting may be.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Reach {
    Past,
    Ahead,
}

/// The days ahead a query names, before the rules for days gone by.
static AHEAD: LazyLock<Vec<Rule>> = LazyLock::new(|| {
    vec![
        rule(r"大后天", |_, now| Some(days(now.date_naive() + Duration::days(3), 1))),
        rule(r"后天|\bthe day after tomorrow\b", |_, now| Some(days(now.date_naive() + Duration::days(2), 1))),
        rule(r"明[天日早晚]|\btomorrow\b", |_, now| Some(days(now.date_naive() + Duration::days(1), 1))),
        rule(&format!(r"{N}天后|\bin (\d{{1,3}}) days?\b"), |c, now| {
            let n = number(c.get(1).or(c.get(2))?.as_str())?;
            Some(days(now.date_naive() + Duration::days(n), 1))
        }),
        rule(&format!(r"(?:未来|接下来){N}天|\b(?:next|coming) (\d{{1,3}}) days\b"), |c, now| {
            let n = number(c.get(1).or(c.get(2))?.as_str())?;
            Some((now - Duration::seconds(1), midnight(now.date_naive() + Duration::days(n + 1))))
        }),
        // "下周三", "next Tuesday": that day of next week. "下周": all of it.
        rule(r"(下下|下)(?:周|星期|礼拜)([一二三四五六日天])?|\bnext (week|mon|tue|wed|thu|fri|sat|sun)[a-z]*\b", |c, now| {
            let monday = now.date_naive() - Duration::days(now.weekday().num_days_from_monday() as i64);
            if let Some(english) = c.get(3) {
                if english.as_str().eq_ignore_ascii_case("week") {
                    return Some(days(monday + Duration::days(7), 7));
                }
                return Some(days(monday + Duration::days(7 + weekday(english.as_str())?), 1));
            }
            let weeks = if &c[1] == "下下" { 2 } else { 1 };
            let start = monday + Duration::days(7 * weeks);
            match c.get(2) {
                Some(day) => Some(days(start + Duration::days(weekday(day.as_str())?), 1)),
                None => Some(days(start, 7)),
            }
        }),
        rule(r"下个?月|\bnext month\b", |_, now| {
            let next = (now.date_naive().with_day(1)? + Duration::days(32)).with_day(1)?;
            let after = (next + Duration::days(32)).with_day(1)?;
            Some((midnight(next), midnight(after)))
        }),
        // A date of this year, whether gone by or to come.
        rule(&format!(r"{N}月{N}[日号]"), |c, now| Some(days(NaiveDate::from_ymd_opt(now.year(), number(&c[1])? as u32, number(&c[2])? as u32)?, 1))),
    ]
});

/// The day or days a query names, with the words that named them.
fn day_span(query: &str, now: Local_, reach: Reach) -> Option<(std::ops::Range<usize>, Local_, Local_)> {
    static RULES: LazyLock<Vec<Rule>> = LazyLock::new(|| {
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
    let ahead = if reach == Reach::Ahead { &AHEAD[..] } else { &[] };
    ahead.iter().chain(RULES.iter()).find_map(|(pattern, range)| {
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

/// Takes the time expression out of the query, of days gone by.
pub fn take_time(query: &mut String, now: Local_) -> Option<Span> {
    take_time_within(query, now, Reach::Past)
}

/// Takes the time expression out of the query, of days gone by and, if
/// `reach` says so, of days to come.
pub fn take_time_within(query: &mut String, now: Local_, reach: Reach) -> Option<Span> {
    let part = part_of_day(query);
    let day = day_span(query, now, reach);
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

    /// The time in a query, and what is left of the query.
    fn read(query: &str) -> (Option<Span>, String) {
        let mut rest = query.to_owned();
        (take_time(&mut rest, now()), rest)
    }

    fn span(query: &str) -> (i64, i64) {
        let time = read(query).0.unwrap_or_else(|| panic!("no time in {query:?}"));
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
        assert_eq!(read("12月25日").0.unwrap().start, Local.with_ymd_and_hms(2024, 12, 25, 0, 0, 0).unwrap().timestamp_millis());
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
        let recent = read("刚才复制的").0.unwrap();
        assert_eq!(recent.end - recent.start, 15 * 60_000 + 1000);
    }

    #[test]
    fn days_to_come_where_there_are_any() {
        let ahead = |query: &str| {
            let time = take_time_within(&mut query.to_owned(), now(), Reach::Ahead).unwrap_or_else(|| panic!("no time in {query:?}"));
            (time.start, time.end)
        };
        assert_eq!(ahead("明天的会议"), (at(10, 9, 0), at(10, 10, 0)));
        assert_eq!(ahead("tomorrow"), (at(10, 9, 0), at(10, 10, 0)));
        assert_eq!(ahead("明天下午"), (at(10, 9, 12), at(10, 9, 18)));
        assert_eq!(ahead("后天"), (at(10, 10, 0), at(10, 11, 0)));
        assert_eq!(ahead("下周"), (at(10, 13, 0), at(10, 20, 0)));
        assert_eq!(ahead("next week"), (at(10, 13, 0), at(10, 20, 0)));
        assert_eq!(ahead("下周三"), (at(10, 15, 0), at(10, 16, 0)));
        assert_eq!(ahead("next friday"), (at(10, 17, 0), at(10, 18, 0)));
        assert_eq!(ahead("下个月"), (at(11, 1, 0), at(12, 1, 0)));
        assert_eq!(ahead("12月25日"), (at(12, 25, 0), at(12, 26, 0)));
        // Days gone by are read as ever.
        assert_eq!(ahead("昨天"), (at(10, 7, 0), at(10, 8, 0)));
        assert_eq!(ahead("last week"), (at(9, 29, 0), at(10, 6, 0)));
        // Looking back only, there is no tomorrow.
        assert_eq!(read("明天").0, None);
        assert_eq!(read("12月25日").0.unwrap().start, Local.with_ymd_and_hms(2024, 12, 25, 0, 0, 0).unwrap().timestamp_millis());
    }

    #[test]
    fn what_is_left() {
        assert_eq!(read("昨天下午").0.unwrap().text, "昨天下午");
        assert_eq!(read("yesterday afternoon").0.unwrap().text, "yesterday afternoon");
        assert_eq!(read("server address yesterday").1.split_whitespace().collect::<Vec<_>>(), ["server", "address"]);
        // No time in these.
        assert_eq!(read("会议纪要").0, None);
        assert_eq!(read("monday.com login").0, None);
        assert_eq!(read("todays").0, None);
    }
}
