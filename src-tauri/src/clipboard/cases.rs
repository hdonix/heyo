//! Writing search cases to measure the clipboard search by (`eval.rs`,
//! `--generate`): for an entry of the history, the queries a person might
//! type some days later to find it again. A model writes them, and only one
//! on this computer: an entry is whatever was copied.
//!
//! What a model writes for this is too long, too tidy and too close to the
//! words of the entry, where real queries are a word or two. So the prompt
//! asks for short ones of named types, and what comes back is checked
//! against the entry rather than trusted.

use std::collections::HashSet;
use std::sync::LazyLock;

use regex::Regex;
use serde_json::Value;

use crate::models::registry::{Unusable, Use};
use crate::models::{Model, Question};
use crate::settings::Settings;

/// An entry to write queries for.
pub(crate) struct CaseEntry {
    pub text: String,
    /// The application it was copied in, if known.
    pub app: Option<String>,
    /// When, as a person would say it at the time of asking: "yesterday", "3 days ago".
    pub copied: String,
}

/// The kinds of query, which the measurements are reported by.
pub(crate) const TYPES: &[(&str, &str)] = &[
    ("keyword", "one or two words (or two to four Chinese characters) that occur in the text"),
    ("paraphrase", "the same thing said in other words, using none of the text's own distinctive words"),
    ("description", "what kind of thing it is rather than what it says, as in “the ssh command for the test server”"),
    ("cross-language", "what it is about, in Chinese if the text is English and in English if the text is Chinese"),
    ("partial", "a vague memory of one part of it, a few words, perhaps slightly wrong"),
    ("exact", "a fragment of an address, a number or an identifier in the text, copied exactly (only if the text has one)"),
    ("hinted", "a word of the text together with when it was copied or which application it came from"),
];

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Case {
    /// One of `TYPES`.
    pub kind: &'static str,
    pub query: String,
}

/// Longer than this and it is not something typed into a search field.
const MAX_WORDS: usize = 7;
const MAX_CHARACTERS: usize = 40;
/// A model that reads this much has read enough to describe it.
const MAX_TEXT: usize = 1500;

fn system() -> String {
    let types: Vec<String> = TYPES.iter().map(|(kind, what)| format!("- {kind}: {what}")).collect();
    format!(
        "You help test a search over a person's clipboard history. Given something they copied, write the searches they might type a few days later to find it again. People type very little: one to four words, lower case, no punctuation, no quotes, never a whole sentence.\n\nWrite one search of each of these types that makes sense for the text, as JSON lines of the form {{\"type\": \"...\", \"query\": \"...\"}} and nothing else:\n{}",
        types.join("\n")
    )
}

pub(crate) fn prompt(entry: &CaseEntry) -> String {
    let source = entry.app.as_deref().map(|app| format!(" in {app}")).unwrap_or_default();
    let text: String = entry.text.chars().take(MAX_TEXT).collect();
    format!("(Copied {}{source}.)\n{text}", entry.copied)
}

static HAN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\p{Han}").unwrap());

/// The words of a text that say what it is about: Latin words of four
/// letters or more, and every pair of neighbouring Chinese characters.
fn distinctive(text: &str) -> HashSet<String> {
    static WORDS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[\p{L}\p{N}]+").unwrap());
    let mut found = HashSet::new();
    for word in WORDS.find_iter(&text.to_lowercase()) {
        let word = word.as_str();
        if !HAN.is_match(word) {
            if word.chars().count() >= 4 {
                found.insert(word.to_owned());
            }
            continue;
        }
        let characters: Vec<char> = word.chars().collect();
        found.extend(characters.windows(2).map(|pair| pair.iter().collect::<String>()));
    }
    found
}

/// Whether a query the model wrote is one of its type for this text. A
/// paraphrase that repeats the text's words measures the keyword search
/// again, and an "exact" fragment that is not in the text finds nothing by
/// design: both would make the numbers lie.
pub(crate) fn acceptable(case: &Case, text: &str) -> bool {
    let query = case.query.as_str();
    if query.is_empty() || query.chars().count() > MAX_CHARACTERS || query.split_whitespace().count() > MAX_WORDS || query.contains('\n') {
        return false;
    }
    let lower = text.to_lowercase();
    match case.kind {
        "exact" => lower.contains(&query.to_lowercase()),
        "keyword" => query.to_lowercase().split_whitespace().all(|word| lower.contains(word)),
        "paraphrase" | "description" | "cross-language" => {
            let own = distinctive(text);
            !distinctive(query).iter().any(|word| own.contains(word))
        }
        _ => true,
    }
}

/// The cases in a model's answer: the lines that are what was asked for,
/// one of a type, whatever else it wrote around them.
pub(crate) fn parse(answer: &str, text: &str) -> Vec<Case> {
    static OBJECT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\{.*\}").unwrap());
    let quotes: &[char] = &['"', '“', '”', '\''];
    let mut cases: Vec<Case> = Vec::new();
    for line in answer.lines() {
        let Some(json) = OBJECT.find(line) else { continue };
        let Ok(candidate) = serde_json::from_str::<Value>(json.as_str()) else { continue };
        let (Some(kind), Some(query)) = (candidate["type"].as_str(), candidate["query"].as_str()) else { continue };
        let Some(&(kind, _)) = TYPES.iter().find(|(known, _)| *known == kind) else { continue };
        let case = Case { kind, query: query.trim().trim_matches(quotes).to_owned() };
        if cases.iter().any(|other| other.kind == case.kind || other.query == case.query) {
            continue;
        }
        if acceptable(&case, text) {
            cases.push(case);
        }
    }
    cases
}

/// The model the settings name for writing cases (`casesModel`): only one
/// that keeps what it is given on this computer, which the reference is
/// held to however it was set (`registry::Use::ClipboardCases`).
pub(crate) fn local_only(settings: &Settings) -> Result<Model, String> {
    Model::for_use(settings, Use::ClipboardCases).map_err(|why| match why {
        crate::models::ModelError::Unusable(Unusable::Cannot(name)) => format!("Cases are written from what you copied, so only a model on this computer may write them, and {name} is not one. Choose one in Heyo Settings → Advanced."),
        other => other.message(),
    })
}

/// The cases `model` (one `local_only` gave) writes for an entry.
pub(crate) async fn cases_for(entry: &CaseEntry, model: &Model) -> Result<Vec<Case>, String> {
    let answer = model.answer(&Question::material(system(), prompt(entry)), &mut |_| {}).await.map_err(|e| e.message())?;
    Ok(parse(&answer, &entry.text))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::mock::{block_on, Server};
    use crate::models::registry::{self, Kind, ModelRef, Provider};
    use serde_json::json;

    fn entry() -> CaseEntry {
        CaseEntry { text: "ssh deploy@10.0.3.17 -p 2222".into(), app: Some("org.gnome.Terminal".into()), copied: "3 days ago".into() }
    }

    fn case(kind: &'static str, query: &str) -> Case {
        Case { kind, query: query.into() }
    }

    fn server(id: &str, url: &str) -> Provider {
        let mut provider = Provider::new(id, Kind::OpenaiCompatible);
        provider.config.insert(registry::URL.into(), url.into());
        provider
    }

    /// The settings with these servers besides the usual providers, and
    /// cases written by `cases`.
    fn with(servers: Vec<Provider>, cases: ModelRef) -> Settings {
        let mut providers = registry::defaults();
        providers[1].config.insert(registry::KEY.into(), "sk-secret".into());
        providers.extend(servers);
        providers.push(Provider::new("cc", Kind::ClaudeCode));
        Settings { providers, cases_model: cases, ..Settings::default() }
    }

    #[test]
    fn only_a_model_on_this_computer_is_ever_asked() {
        for url in ["https://api.example.com/v1", "http://192.168.1.20:8000/v1", "not a url"] {
            let refused = local_only(&with(vec![server("far", url)], ModelRef::to("far", "tiny"))).unwrap_err();
            assert!(refused.contains("only a model on this computer"), "{refused}");
        }
        for url in ["http://127.0.0.1:8000/v1", "http://localhost:11434/v1"] {
            assert_eq!(local_only(&with(vec![server("near", url)], ModelRef::to("near", "tiny"))), Ok(Model::local(url, "tiny")));
        }
        // Claude's API, and a command-line tool, which runs here but answers
        // from its maker's service: never the one asked.
        for other in [registry::CLAUDE, "cc"] {
            assert!(local_only(&with(vec![], ModelRef::to(other, ""))).unwrap_err().contains("only a model on this computer"));
        }
        // None chosen: said so.
        assert!(local_only(&with(vec![], ModelRef::none())).unwrap_err().starts_with("No model is chosen"));
    }

    #[test]
    fn they_come_from_the_local_server_whichever_provider_the_settings_name() {
        let lines = [
            json!({ "type": "keyword", "query": "ssh deploy" }),
            json!({ "type": "description", "query": "server login command" }),
            json!({ "type": "cross-language", "query": "服务器登录命令" }),
            json!({ "type": "exact", "query": "10.0.3.17" }),
            json!({ "type": "hinted", "query": "ssh from terminal" }),
        ];
        let answer: String = lines.iter().map(|line| format!("{line}\n")).collect();
        let characters: Vec<char> = answer.chars().collect();
        let stream: String = characters.chunks(20).map(|piece| format!("data: {}\n\n", json!({ "choices": [{ "delta": { "content": piece.iter().collect::<String>() } }] }))).collect();
        let server = Server::start(move |_| (200, format!("{stream}data: [DONE]\n\n")));
        let settings = Settings { ai_model: ModelRef::to(registry::CLAUDE, ""), ..with(vec![self::server("near", &server.url("/v1"))], ModelRef::to("near", "tiny")) };
        let cases = block_on(cases_for(&entry(), &local_only(&settings).unwrap())).unwrap();
        assert_eq!(cases.iter().map(|c| c.kind).collect::<Vec<_>>(), ["keyword", "description", "cross-language", "exact", "hinted"]);
        let asked = server.asked();
        assert_eq!((asked.len(), asked[0].path.as_str()), (1, "/v1/chat/completions"));
        // The entry is given with when and where it was copied, and the key goes nowhere.
        assert!(asked[0].body.contains("ssh deploy@10.0.3.17 -p 2222") && asked[0].body.contains("Copied 3 days ago in org.gnome.Terminal"));
        assert!(!asked[0].body.contains("sk-secret") && asked[0].header("x-api-key").is_none());
    }

    #[test]
    fn a_long_entry_is_given_only_as_far_as_the_model_needs() {
        assert!(prompt(&CaseEntry { text: "x".repeat(5000), app: None, copied: "today".into() }).chars().count() < 1600);
        assert_eq!(prompt(&CaseEntry { text: "note".into(), app: None, copied: "today".into() }), "(Copied today.)\nnote");
    }

    const TEXT: &str = "The quarterly report is due on Friday; please send me your numbers by Wednesday noon.";

    #[test]
    fn what_a_model_wrote_is_read_line_by_line_whatever_is_around_it() {
        let answer = [
            "Sure! Here are the searches:",
            "```json",
            r#"{"type": "keyword", "query": "quarterly report"}"#,
            r#"- {"type": "paraphrase", "query": "three-month summary deadline"}"#,
            r#"{"type": "cross-language", "query": "“季度报告截止”"}"#,
            r#"{"type": "keyword", "query": "report"}"#,
            r#"{"type": "poem", "query": "o report"}"#,
            r#"{"type": "partial", "query": 5}"#,
            "{not json}",
            "```",
        ]
        .join("\n");
        assert_eq!(parse(&answer, TEXT), [case("keyword", "quarterly report"), case("paraphrase", "three-month summary deadline"), case("cross-language", "季度报告截止")]);
    }

    #[test]
    fn what_a_model_wrote_is_checked_against_the_entry() {
        // A paraphrase that repeats the entry's words measures the word search again.
        assert!(!acceptable(&case("paraphrase", "quarterly numbers due"), TEXT));
        assert!(acceptable(&case("paraphrase", "three-month summary deadline"), TEXT));
        assert!(!acceptable(&case("description", "那个发票的事"), "上个月的发票已经寄出，请查收"));
        assert!(acceptable(&case("description", "账单邮寄了"), "上个月的发票已经寄出，请查收"));
        // A fragment that is not in the entry finds nothing by design.
        let text = entry().text;
        assert!(acceptable(&case("exact", "10.0.3.17"), &text));
        assert!(!acceptable(&case("exact", "10.0.3.71"), &text));
        assert!(acceptable(&case("keyword", "SSH deploy"), &text));
        assert!(!acceptable(&case("keyword", "ssh server"), &text));
        // Nobody types a sentence into a search field.
        assert!(!acceptable(&case("partial", "please send me all of your numbers by Wednesday at noon"), TEXT));
        assert!(!acceptable(&case("hinted", ""), TEXT));
    }
}
