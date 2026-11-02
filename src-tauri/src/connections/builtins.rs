//! Heyo's own connectors, declared as an extension's manifest would
//! declare them: mail over IMAP, CalDAV calendars, calendar subscriptions
//! (ICS) and MCP servers. What syncs them is in the connections runtime
//! (`runtime/src/connections/builtin/`), by the same command names.
//!
//! Their titles, descriptions and the notes of their presets are said in
//! the language Heyo speaks, so they are made each time they are asked for.

use serde_json::{json, Map, Value};

use super::accounts::{Field, FieldType, Kind, Preset, Sync};
use crate::extensions::{Choice, BUILTIN};
use crate::i18n::t;
use crate::index::items::ItemKind;

fn field(name: &str, title: &str, kind: FieldType) -> Field {
    Field { name: name.into(), title: title.into(), kind, default: None, required: false, placeholder: None, description: None, choices: Vec::new() }
}

impl Field {
    fn required(mut self) -> Field {
        self.required = true;
        self
    }
    fn default(mut self, value: Value) -> Field {
        self.default = Some(value);
        self
    }
    fn placeholder(mut self, text: &str) -> Field {
        self.placeholder = Some(text.into());
        self
    }
    fn description(mut self, text: &str) -> Field {
        self.description = Some(text.into());
        self
    }
    fn choices(mut self, choices: &[(&str, &str)]) -> Field {
        self.choices = choices.iter().map(|(value, title)| Choice { title: (*title).into(), value: (*value).into() }).collect();
        self
    }
}

fn preset(title: &str, values: Value, description: &str, web: Option<&str>) -> Preset {
    let values: Map<String, Value> = values.as_object().cloned().unwrap_or_default();
    Preset { title: title.into(), values, description: Some(description.into()), web: web.map(Into::into) }
}

fn kind(command: &str, title: &str, icon: &str, items: Vec<ItemKind>, fields: Vec<Field>, presets: Vec<Preset>, sync: Sync) -> Kind {
    Kind {
        id: format!("{BUILTIN}:{command}"),
        extension: BUILTIN.into(),
        command: command.into(),
        title: title.into(),
        extension_title: "Heyo".into(),
        icon: Some(icon.into()),
        items,
        fields,
        presets,
        sync,
        heyo: None,
        extension_name: BUILTIN.into(),
    }
}

/// The password field of a service that wants one made for Heyo.
fn password() -> Field {
    field("password", t("Password"), FieldType::Secret)
        .required()
        .description(t("Most mail and calendar services want an app password or authorization code here, made for Heyo in the service's settings, not the password you sign in with."))
}

fn imap() -> Kind {
    let server = |title: &str, host: &str, note: &str, web: &str| preset(title, json!({ "host": host, "port": 993, "security": "tls" }), note, Some(web));
    let netease = t("Turn on IMAP in the mailbox's settings on the web, and use the authorization code it gives as the password.");
    kind(
        "imap",
        t("IMAP Mail"),
        "envelope-16",
        vec![ItemKind::Email],
        vec![
            field("host", t("Server"), FieldType::Text).required().placeholder("imap.example.com"),
            field("port", t("Port"), FieldType::Number).default(json!(993)),
            field("security", t("Security"), FieldType::Choice).default(json!("tls")).choices(&[("tls", "TLS"), ("starttls", "STARTTLS"), ("none", t("None"))]),
            field("username", t("User Name"), FieldType::Text).required().placeholder("you@example.com"),
            password(),
            field("folders", t("Folders"), FieldType::List).default(json!(["INBOX"])),
            field("days", t("Days to Bring First"), FieldType::Number).default(json!(90)).description(t("How far back the first sync of a folder goes; later syncs bring everything new.")),
        ],
        vec![
            server(t("QQ Mail"), "imap.qq.com", t("Turn on IMAP in QQ Mail's settings on the web, and use the authorization code it gives as the password, not the QQ password."), "https://mail.qq.com"),
            server(t("NetEase 163 Mail"), "imap.163.com", netease, "https://mail.163.com"),
            server(t("NetEase 126 Mail"), "imap.126.com", netease, "https://mail.126.com"),
            server(t("Alibaba Enterprise Mail"), "imap.qiye.aliyun.com", t("Use the whole address as the user name; with secure login on, a client password made in the mailbox's settings."), "https://qiye.aliyun.com"),
            server(t("Tencent Enterprise Mail"), "imap.exmail.qq.com", t("Use the whole address as the user name; with WeChat secure login on, a client password made in the mailbox's settings."), "https://exmail.qq.com"),
            server("Gmail", "imap.gmail.com", t("Needs 2-Step Verification on the Google account, and an app password made for Heyo."), "https://mail.google.com"),
            server("iCloud", "imap.mail.me.com", t("Use the name before @icloud.com as the user name, and an app-specific password made in the Apple Account's settings."), "https://www.icloud.com/mail"),
            server("Outlook", "outlook.office365.com", t("Microsoft wants most accounts to sign in through its own page (OAuth), which Heyo cannot do yet: an app password works only where the account still allows one."), "https://outlook.live.com/mail"),
        ],
        Sync { every: Some("15m".into()), watch: true },
    )
}

fn caldav() -> Kind {
    let server = |title: &str, url: &str, note: &str, web: Option<&str>| preset(title, json!({ "url": url }), note, web);
    kind(
        "caldav",
        t("CalDAV Calendar"),
        "calendar-16",
        vec![ItemKind::Event],
        vec![
            field("url", t("Server"), FieldType::Text).required().placeholder("https://caldav.example.com/"),
            field("username", t("User Name"), FieldType::Text).required(),
            password(),
            field("calendars", t("Calendars"), FieldType::List).description(t("The names of the calendars to bring; all of them when left empty.")),
        ],
        vec![
            server("iCloud", "https://caldav.icloud.com/", t("Use the Apple Account's address as the user name, and an app-specific password made in its settings."), Some("https://www.icloud.com/calendar")),
            server("Fastmail", "https://caldav.fastmail.com/", t("Use the whole address as the user name, and an app password with calendar access."), Some("https://app.fastmail.com/calendar")),
            server(t("Feishu"), "https://caldav.feishu.cn/", t("In Feishu's calendar settings, turn on CalDAV sync: it shows the user name and password to use."), None),
        ],
        Sync { every: Some("30m".into()), watch: false },
    )
}

fn ics() -> Kind {
    kind(
        "ics",
        t("Calendar Subscription"),
        "calendar-16",
        vec![ItemKind::Event],
        vec![field("url", t("Address"), FieldType::Text)
            .required()
            .placeholder("https://example.com/calendar.ics")
            .description(t("A calendar's address in iCalendar format (.ics), such as Google Calendar's secret address in iCal format."))],
        Vec::new(),
        Sync { every: Some("30m".into()), watch: false },
    )
}

/// Heyo's own connectors.
pub fn kinds() -> Vec<Kind> {
    vec![imap(), caldav(), ics()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_is_heyos_own_and_its_presets_fill_in_only_its_fields() {
        for kind in kinds() {
            assert!(kind.builtin(), "{}", kind.id);
            assert_eq!(kind.id, format!("@heyo:{}", kind.command));
            for preset in &kind.presets {
                for name in preset.values.keys() {
                    let field = kind.fields.iter().find(|field| &field.name == name).unwrap_or_else(|| panic!("{} has no field {name}", kind.id));
                    assert_ne!(field.kind, FieldType::Secret, "a preset never holds a secret");
                }
                assert!(preset.description.is_some());
            }
            for field in &kind.fields {
                if field.kind == FieldType::Choice {
                    let default = field.default.as_ref().and_then(Value::as_str).unwrap();
                    assert!(field.choices.iter().any(|choice| choice.value == default), "{}: {}", kind.id, field.name);
                }
            }
        }
    }

    #[test]
    fn the_connections_runtime_has_each_of_them() {
        let registry = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../runtime/src/connections/builtins.ts")).unwrap();
        let listed = registry.split("builtinConnectors").nth(1).and_then(|rest| rest.split('{').nth(1)).and_then(|rest| rest.split('}').next()).unwrap();
        let names: Vec<&str> = listed.split(',').map(str::trim).filter(|name| !name.is_empty()).collect();
        let commands: Vec<String> = kinds().into_iter().map(|kind| kind.command).collect();
        assert_eq!(names, commands);
    }

    #[test]
    fn an_account_is_added_from_a_preset() {
        let mut settings = crate::settings::Settings::default();
        let imap = imap();
        let mut values = imap.presets[0].values.clone();
        values.insert("username".into(), json!("me@qq.com"));
        values.insert("password".into(), json!("code"));
        let id = super::super::accounts::add(&mut settings, &imap, "", &values).unwrap();
        let account = &settings.connections[0];
        assert_eq!((id.as_str(), account.connector.as_str()), ("imap", "@heyo:imap"));
        assert_eq!(account.config["host"], "imap.qq.com");
        assert!(imap.presets.iter().all(|preset| preset.web.as_deref().is_some_and(|web| web.starts_with("https://"))));
        assert_eq!(account.config["folders"], json!(["INBOX"]));
        assert!(account.config.get("password").is_none());
    }
}
