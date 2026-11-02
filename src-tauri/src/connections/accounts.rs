//! The accounts the user has connected, the connectors they can be of, and
//! what is remembered of each account's syncing.
//!
//! An account is a setting (`Settings::connections`): its connector, its
//! name, and the values of its connector's fields; the values of the fields
//! that are secrets are kept with the secrets (`secrets.rs`) as
//! `connection:<id>/<field>`, never among the rest. A connector is a
//! command of mode `connector` (docs/extension-api.md, 7.11), and says in
//! its manifest what its accounts have to be given (`heyo.fields`), ways
//! of filling them in (`heyo.presets`), what it brings (`heyo.items`) and
//! how it is synced (`heyo.sync`).
//!
//! How far each account's syncing got (its cursor, when it last synced,
//! what went wrong) is not a setting: it is kept in `connections.json` in
//! the data directory ([`Kept`]), written after the items it speaks of.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use chrono::TimeDelta;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use ts_rs::TS;

use super::items::ItemKind;
use super::scheduler::{ErrorKind, Plan, Past, Time};
use crate::extensions::Choice;
use crate::i18n::{t, tf};
use crate::secrets::{Owner, Secrets};
use crate::settings::Settings;

/// A connected account, as it is saved.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../shared/bindings/", rename = "Connection")]
pub struct Account {
    /// Its own, never reused while it is there.
    pub id: String,
    /// Its connector: `<extension name>:<command>`, `@heyo:<command>` for
    /// Heyo's own.
    pub connector: String,
    /// As the user calls it.
    pub name: String,
    /// The values of its connector's fields that are not secrets, by name.
    #[serde(default)]
    #[ts(type = "Record<string, unknown>")]
    pub config: Map<String, Value>,
}

/// How a field of a connector is filled in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../shared/bindings/", rename = "ConnectorFieldType")]
pub enum FieldType {
    Text,
    Number,
    /// Several pieces of text.
    List,
    /// One of `choices`.
    Choice,
    /// On or off.
    Switch,
    /// Kept with the secrets, and never shown again once saved.
    Secret,
}

/// A field an account of the connector is given.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../shared/bindings/", rename = "ConnectorField")]
pub struct Field {
    pub name: String,
    pub title: String,
    #[serde(rename = "type")]
    #[ts(rename = "type")]
    pub kind: FieldType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "unknown")]
    pub default: Option<Value>,
    #[serde(default)]
    pub required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub placeholder: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub description: Option<String>,
    /// What a `choice` is chosen from.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(optional, as = "Option<Vec<Choice>>")]
    pub choices: Vec<Choice>,
}

/// A way of filling the fields in, for a service the connector knows
/// (`Gmail`, with its server's address).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../shared/bindings/", rename = "ConnectorPreset")]
pub struct Preset {
    pub title: String,
    #[ts(type = "Record<string, unknown>")]
    pub values: Map<String, Value>,
}

/// How its accounts are synced (`heyo.sync`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../shared/bindings/", rename = "ConnectorSync")]
pub struct Sync {
    /// How often, as `30m` or `1h`; every 15 minutes when it does not say.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub every: Option<String>,
    /// It watches its accounts for changes.
    #[serde(default)]
    pub watch: bool,
}

/// A connector: a command of mode `connector`, as its manifest declares it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../shared/bindings/", rename = "ConnectorKind")]
pub struct Kind {
    /// What an account names it by: `<extension name>:<command>`.
    pub id: String,
    /// Where the extension is (`@heyo` for Heyo's own): what the runtime loads.
    pub extension: String,
    pub command: String,
    pub title: String,
    pub extension_title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub icon: Option<String>,
    /// What it brings.
    pub items: Vec<ItemKind>,
    pub fields: Vec<Field>,
    pub presets: Vec<Preset>,
    pub sync: Sync,
    /// The extension's manifest says what it is, and lists the `connector`
    /// permission; Heyo's own need not.
    #[serde(skip)]
    #[ts(skip)]
    pub heyo: Option<crate::extensions::Heyo>,
    #[serde(skip)]
    #[ts(skip)]
    pub extension_name: String,
}

impl Kind {
    /// How its accounts are scheduled.
    pub fn plan(&self) -> Plan {
        Plan { every: self.sync.every.as_deref().and_then(every), watch: self.sync.watch }
    }

    /// Whether `name` is one of its secret fields.
    pub fn secret(&self, name: &str) -> bool {
        self.fields.iter().any(|field| field.name == name && field.kind == FieldType::Secret)
    }

    pub fn builtin(&self) -> bool {
        self.extension == crate::extensions::BUILTIN
    }
}

/// `30s`, `15m`, `1h`, `1d`; a minute at the least.
fn every(text: &str) -> Option<TimeDelta> {
    let text = text.trim();
    let split = text.find(|c: char| !c.is_ascii_digit())?;
    let amount: i64 = text[..split].parse().ok()?;
    let unit = match &text[split..] {
        "s" => TimeDelta::seconds(1),
        "m" => TimeDelta::minutes(1),
        "h" => TimeDelta::hours(1),
        "d" => TimeDelta::days(1),
        _ => return None,
    };
    Some((unit * amount as i32).max(TimeDelta::minutes(1)))
}

/// The `heyo` field of a connector command, as its manifest writes it.
#[derive(Debug, Default, Deserialize)]
struct Declared {
    #[serde(default)]
    items: Vec<ItemKind>,
    #[serde(default)]
    fields: Vec<Field>,
    #[serde(default)]
    presets: Vec<Preset>,
    #[serde(default)]
    sync: Sync,
}

/// The connectors of the extension in `dir`, from its manifest. A command
/// whose `heyo` field cannot be read is no connector.
pub fn kinds_in(dir: &Path) -> Vec<Kind> {
    let Ok(text) = std::fs::read_to_string(dir.join("package.json")) else { return Vec::new() };
    let Ok(manifest) = serde_json::from_str::<Value>(&text) else { return Vec::new() };
    let Some(name) = manifest["name"].as_str() else { return Vec::new() };
    let heyo: Option<crate::extensions::Heyo> = manifest.get("heyo").map(|heyo| serde_json::from_value(heyo.clone()).unwrap_or_default());
    let extension_title = manifest["title"].as_str().unwrap_or(name);
    let commands = manifest["commands"].as_array().cloned().unwrap_or_default();
    commands
        .iter()
        .filter(|command| command["mode"] == "connector")
        .filter_map(|command| {
            let command_name = command["name"].as_str()?;
            if !dir.join(format!("{command_name}.js")).is_file() {
                return None;
            }
            let declared: Declared = match command.get("heyo") {
                Some(declared) => serde_json::from_value(declared.clone()).map_err(|e| eprintln!("heyo: {}: connector {command_name}: {e}", dir.display())).ok()?,
                None => Declared::default(),
            };
            Some(Kind {
                id: format!("{name}:{command_name}"),
                extension: dir.to_string_lossy().into_owned(),
                command: command_name.into(),
                title: command["title"].as_str().unwrap_or(command_name).into(),
                extension_title: extension_title.into(),
                icon: command["icon"].as_str().or(manifest["icon"].as_str()).map(|icon| dir.join("assets").join(icon).to_string_lossy().into_owned()),
                items: declared.items,
                fields: declared.fields,
                presets: declared.presets,
                sync: declared.sync,
                heyo: heyo.clone(),
                extension_name: name.into(),
            })
        })
        .collect()
}

/// Every connector there is: Heyo's own (none yet), then those of the
/// installed extensions and of those being developed.
pub fn kinds() -> Vec<Kind> {
    crate::extensions::extension_dirs().iter().flat_map(|dir| kinds_in(dir)).collect()
}

/// The values of `kind`'s fields `given` names, checked against what the
/// field takes, split into what is kept with the settings and what with the
/// secrets. A field left out takes its default; a required one with
/// neither is refused, unless `partial` (an edit, which changes only what
/// it names).
fn checked(kind: &Kind, given: &Map<String, Value>, partial: bool) -> Result<(Map<String, Value>, BTreeMap<String, String>), String> {
    let mut config = Map::new();
    let mut secrets = BTreeMap::new();
    for name in given.keys() {
        if !kind.fields.iter().any(|field| &field.name == name) {
            return Err(tf("{0} has no setting {1}.", &[&kind.title, name]));
        }
    }
    for field in &kind.fields {
        let value = match given.get(&field.name) {
            Some(value) => value.clone(),
            None if partial => continue,
            None => match &field.default {
                Some(default) => default.clone(),
                None if field.required && field.kind != FieldType::Secret => return Err(tf("{0} needs {1}.", &[&kind.title, &field.title])),
                None => continue,
            },
        };
        let wrong = || tf("{0} is not a value {1} takes.", &[&value.to_string(), &field.title]);
        match field.kind {
            FieldType::Secret => {
                let text = value.as_str().ok_or_else(wrong)?;
                secrets.insert(field.name.clone(), text.to_owned());
                continue;
            }
            FieldType::Text if !value.is_string() => return Err(wrong()),
            FieldType::Number if !value.is_number() => return Err(wrong()),
            FieldType::Switch if !value.is_boolean() => return Err(wrong()),
            FieldType::List if !value.as_array().is_some_and(|all| all.iter().all(Value::is_string)) => return Err(wrong()),
            FieldType::Choice if !field.choices.iter().any(|choice| value.as_str() == Some(choice.value.as_str())) => return Err(wrong()),
            _ => {}
        }
        if field.required && (value.as_str().is_some_and(|text| text.trim().is_empty()) || value.is_null()) {
            return Err(tf("{0} needs {1}.", &[&kind.title, &field.title]));
        }
        config.insert(field.name.clone(), value);
    }
    Ok((config, secrets))
}

/// An id for a new account of the connector `command`: the command's
/// name, numbered after the first.
fn new_id(accounts: &[Account], command: &str) -> String {
    let taken = |id: &str| accounts.iter().any(|account| account.id == id);
    if !taken(command) {
        return command.into();
    }
    (2..).map(|n| format!("{command}-{n}")).find(|id| !taken(id)).expect("there is always another number")
}

/// Adds an account of `kind` to the settings, set up with `values` (its
/// fields by name, secrets among them); says its id.
pub fn add(settings: &mut Settings, kind: &Kind, name: &str, values: &Map<String, Value>) -> Result<String, String> {
    let (config, secrets) = checked(kind, values, false)?;
    let id = new_id(&settings.connections, &kind.command);
    let name = match name.trim() {
        "" => kind.title.clone(),
        name => name.to_owned(),
    };
    for (field, value) in &secrets {
        settings.secrets.store(Owner::Connection(&id), field, value);
    }
    settings.connections.push(Account { id: id.clone(), connector: kind.id.clone(), name, config });
    Ok(id)
}

/// Changes an account: its name when given, and the fields `values` names
/// (an empty secret is one removed).
pub fn edit(settings: &mut Settings, kind: &Kind, id: &str, name: Option<&str>, values: &Map<String, Value>) -> Result<(), String> {
    let (config, secrets) = checked(kind, values, true)?;
    let account = settings.connections.iter_mut().find(|account| account.id == id).ok_or_else(|| tf("There is no connection {0}.", &[&id]))?;
    if let Some(name) = name {
        if name.trim().is_empty() {
            return Err(t("A connection needs a name.").into());
        }
        account.name = name.trim().into();
    }
    account.config.extend(config);
    for (field, value) in &secrets {
        settings.secrets.store(Owner::Connection(id), field, value);
    }
    Ok(())
}

/// Takes an account away, and its secrets with it.
pub fn remove(settings: &mut Settings, id: &str) -> Result<(), String> {
    let at = settings.connections.iter().position(|account| account.id == id).ok_or_else(|| tf("There is no connection {0}.", &[&id]))?;
    settings.connections.remove(at);
    settings.secrets.forget(Owner::Connection(id));
    Ok(())
}

/// The names of the account's secrets that are set.
pub fn secrets_set(secrets: &Secrets, id: &str) -> Vec<String> {
    secrets.names(Owner::Connection(id))
}

// --- What is remembered of the syncing ---------------------------------------

/// What went wrong the last time, in the connector's words.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../shared/bindings/", rename = "ConnectionError")]
pub struct Trouble {
    pub kind: ErrorKind,
    pub message: String,
}

/// An account's syncing, as far as it got.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Synced {
    /// Where the connector goes on from: what its last sync answered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    /// When the last sync began.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_attempt: Option<Time>,
    /// When the last sync that finished did.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_sync: Option<Time>,
    #[serde(default)]
    pub failures: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<Trouble>,
}

impl Synced {
    /// What the scheduler is told of it when Heyo starts.
    pub fn past(&self) -> Past {
        Past { last_attempt: self.last_attempt, failures: self.failures, error: self.error.as_ref().map(|error| error.kind) }
    }
}

/// Every account's syncing, by id, and where it is kept.
#[derive(Debug, Default)]
pub struct Kept {
    path: Option<PathBuf>,
    pub accounts: BTreeMap<String, Synced>,
}

impl Kept {
    /// What is kept at `path`; nothing, if it cannot be read. `None` keeps
    /// nothing anywhere (a test's).
    pub fn load(path: Option<PathBuf>) -> Kept {
        let accounts = path.as_ref().and_then(|path| std::fs::read_to_string(path).ok()).and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default();
        Kept { path, accounts }
    }

    /// Writes it whole or not at all, readable only by the user.
    pub fn save(&self) {
        let Some(path) = &self.path else { return };
        let write = || -> std::io::Result<()> {
            use std::io::Write;
            std::fs::create_dir_all(path.parent().unwrap_or(Path::new(".")))?;
            let temporary = path.with_extension("json.tmp");
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create(true).truncate(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options.open(&temporary)?;
            file.write_all(serde_json::to_string_pretty(&self.accounts).expect("plain data").as_bytes())?;
            file.sync_all()?;
            std::fs::rename(&temporary, path)
        };
        if let Err(e) = write() {
            eprintln!("heyo: could not save {}: {e}", path.display());
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use serde_json::json;

    /// A connector of a mail service, as a manifest would declare it.
    pub(crate) fn mail() -> Kind {
        let fields = json!([
            { "name": "host", "title": "Server", "type": "text", "required": true },
            { "name": "port", "title": "Port", "type": "number", "default": 993 },
            { "name": "folders", "title": "Folders", "type": "list", "default": ["INBOX"] },
            { "name": "security", "title": "Security", "type": "choice", "default": "tls", "choices": [{ "title": "TLS", "value": "tls" }, { "title": "None", "value": "none" }] },
            { "name": "unread", "title": "Unread Only", "type": "switch" },
            { "name": "password", "title": "Password", "type": "secret", "required": true },
        ]);
        Kind {
            id: "mail:imap".into(),
            extension: "/x/mail".into(),
            command: "imap".into(),
            title: "IMAP".into(),
            extension_title: "Mail".into(),
            icon: None,
            items: vec![ItemKind::Email],
            fields: serde_json::from_value(fields).unwrap(),
            presets: Vec::new(),
            sync: Sync { every: Some("30m".into()), watch: true },
            heyo: None,
            extension_name: "mail".into(),
        }
    }

    fn values(value: Value) -> Map<String, Value> {
        value.as_object().unwrap().clone()
    }

    #[test]
    fn an_account_is_added_with_its_defaults_and_its_secrets_apart() {
        let mut settings = Settings::default();
        let id = add(&mut settings, &mail(), "", &values(json!({ "host": "imap.example.com", "password": "hunter2" }))).unwrap();
        assert_eq!(id, "imap");
        let account = &settings.connections[0];
        assert_eq!((account.name.as_str(), account.connector.as_str()), ("IMAP", "mail:imap"));
        assert_eq!(Value::Object(account.config.clone()), json!({ "host": "imap.example.com", "port": 993, "folders": ["INBOX"], "security": "tls" }));
        assert_eq!(settings.secrets.read(Owner::Connection("imap"), "password"), Some("hunter2"));
        assert!(!serde_json::to_string(&settings.connections).unwrap().contains("hunter2"));
        // A second of the same connector has an id of its own.
        assert_eq!(add(&mut settings, &mail(), "Home", &values(json!({ "host": "h" }))).unwrap(), "imap-2");
    }

    #[test]
    fn what_a_field_does_not_take_is_refused() {
        let mut settings = Settings::default();
        for wrong in [
            json!({ "password": "x" }),
            json!({ "host": "" }),
            json!({ "host": "h", "port": "993" }),
            json!({ "host": "h", "folders": "INBOX" }),
            json!({ "host": "h", "security": "ssl" }),
            json!({ "host": "h", "unread": "yes" }),
            json!({ "host": "h", "password": 12 }),
            json!({ "host": "h", "colour": "red" }),
        ] {
            assert!(add(&mut settings, &mail(), "", &values(wrong.clone())).is_err(), "{wrong}");
        }
        assert!(settings.connections.is_empty() && settings.secrets.is_empty());
    }

    #[test]
    fn an_account_is_changed_by_what_is_named_and_removed_with_its_secrets() {
        let mut settings = Settings::default();
        let id = add(&mut settings, &mail(), "Work", &values(json!({ "host": "a", "password": "one" }))).unwrap();
        edit(&mut settings, &mail(), &id, Some(" Office "), &values(json!({ "port": 143, "password": "two" }))).unwrap();
        let account = &settings.connections[0];
        assert_eq!((account.name.as_str(), &account.config["host"], &account.config["port"]), ("Office", &json!("a"), &json!(143)));
        assert_eq!(secrets_set(&settings.secrets, &id), ["password"]);
        assert_eq!(settings.secrets.read(Owner::Connection(&id), "password"), Some("two"));
        // An empty secret is removed.
        edit(&mut settings, &mail(), &id, None, &values(json!({ "password": "" }))).unwrap();
        assert!(secrets_set(&settings.secrets, &id).is_empty());
        assert!(edit(&mut settings, &mail(), &id, Some(" "), &Map::new()).is_err());
        assert!(edit(&mut settings, &mail(), "gone", None, &Map::new()).is_err());
        edit(&mut settings, &mail(), &id, None, &values(json!({ "password": "three" }))).unwrap();
        remove(&mut settings, &id).unwrap();
        assert!(settings.connections.is_empty() && settings.secrets.is_empty());
        assert!(remove(&mut settings, &id).is_err());
    }

    #[test]
    fn a_connector_is_read_from_its_manifest() {
        let dir = std::env::temp_dir().join(format!("heyo-connectors-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let manifest = json!({
            "name": "mail", "title": "Mail", "icon": "mail.png",
            "heyo": { "permissions": ["connector"] },
            "commands": [
                { "name": "imap", "title": "IMAP", "mode": "connector", "heyo": {
                    "items": ["email"], "fields": [{ "name": "host", "title": "Server", "type": "text" }],
                    "presets": [{ "title": "Gmail", "values": { "host": "imap.gmail.com" } }], "sync": { "every": "30m", "watch": true } } },
                { "name": "unbuilt", "title": "Unbuilt", "mode": "connector" },
                { "name": "odd", "title": "Odd", "mode": "connector", "heyo": { "fields": [{ "name": "x", "title": "X", "type": "colour" }] } },
                { "name": "inbox", "title": "Inbox", "mode": "view" },
            ],
        });
        std::fs::write(dir.join("package.json"), manifest.to_string()).unwrap();
        for file in ["imap.js", "odd.js", "inbox.js"] {
            std::fs::write(dir.join(file), "").unwrap();
        }
        let kinds = kinds_in(&dir);
        assert_eq!(kinds.len(), 1, "{kinds:?}");
        let kind = &kinds[0];
        assert_eq!((kind.id.as_str(), kind.extension_title.as_str(), kind.presets[0].title.as_str()), ("mail:imap", "Mail", "Gmail"));
        assert_eq!(kind.plan(), Plan { every: Some(TimeDelta::minutes(30)), watch: true });
        assert!(kind.icon.as_deref().unwrap().ends_with("assets/mail.png"));
        assert!(kind.heyo.as_ref().unwrap().permissions.contains(&"connector".to_string()));
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(every("10s"), Some(TimeDelta::minutes(1)));
        assert_eq!(every("2h"), Some(TimeDelta::hours(2)));
        assert_eq!(every("soon"), None);
    }

    #[test]
    fn what_is_remembered_survives_a_restart() {
        let path = std::env::temp_dir().join(format!("heyo-connections-kept-{}.json", std::process::id()));
        let mut kept = Kept::load(Some(path.clone()));
        let at = chrono::Utc::now();
        kept.accounts.insert("imap".into(), Synced { cursor: Some("42".into()), last_attempt: Some(at), last_sync: Some(at), failures: 2, error: Some(Trouble { kind: ErrorKind::Auth, message: "No".into() }) });
        kept.save();
        let again = Kept::load(Some(path.clone()));
        assert_eq!(again.accounts, kept.accounts);
        assert_eq!(again.accounts["imap"].past(), Past { last_attempt: Some(at), failures: 2, error: Some(ErrorKind::Auth) });
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        }
        let _ = std::fs::remove_file(&path);
    }
}
