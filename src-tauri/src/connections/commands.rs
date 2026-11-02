//! What the settings window and the palette ask of the connections: the
//! accounts with how their syncing goes, the connectors there are, adding,
//! changing, removing, checking and syncing an account, and searching
//! what was synced. The settings window asks through its `Command`s
//! (`slint_windows/host.rs`); the palette searches with the
//! `search_connections` command, and shows what it found with
//! `preview_connection`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use tauri::{AppHandle, Manager};
use ts_rs::TS;

use super::accounts::{self, Kind};
use super::shell::{FoundAccount, Searched, State};
use super::Connections;
use crate::i18n::tf;
use crate::permissions::{self, Need, Permission};

/// An account as a window is told of it: without its secrets, saying
/// which are set.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../shared/bindings/", rename = "ConnectionView")]
pub struct View {
    pub id: String,
    pub connector: String,
    pub name: String,
    #[ts(type = "Record<string, unknown>")]
    pub config: Map<String, Value>,
    pub secrets_set: Vec<String>,
    /// Its connector is not installed (any more).
    pub orphaned: bool,
    pub state: State,
}

/// How many items a search answers at most.
const FOUND: usize = 20;

fn shell(app: &AppHandle) -> Result<std::sync::Arc<super::shell::Shell>, String> {
    app.state::<Connections>().shell().cloned().ok_or_else(|| "connections are not running".to_owned())
}

/// The accounts, in order, with how their syncing goes.
pub(crate) fn list(app: &AppHandle) -> Vec<View> {
    let settings = app.state::<crate::SettingsState>().0.read().unwrap().clone();
    let kinds = accounts::kinds();
    let shell = app.state::<Connections>().shell().cloned();
    settings
        .connections
        .into_iter()
        .map(|account| View {
            secrets_set: accounts::secrets_set(&settings.secrets, &account.id),
            orphaned: !kinds.iter().any(|kind| kind.id == account.connector),
            state: shell.as_ref().map(|shell| shell.state(&account.id)).unwrap_or_else(|| State {
                phase: "idle".into(),
                since: None,
                next: None,
                last_sync: None,
                error: None,
                items: 0,
                progress: None,
            }),
            id: account.id,
            connector: account.connector,
            name: account.name,
            config: account.config,
        })
        .collect()
}

/// The connectors accounts can be added of: Heyo's own, and those of
/// extensions that list the `connector` permission.
pub(crate) fn kinds() -> Vec<Kind> {
    let declared = |kind: &Kind| kind.heyo.as_ref().is_some_and(|heyo| heyo.permissions.iter().any(|name| name == Permission::Connector.name()));
    accounts::kinds().into_iter().filter(|kind| kind.builtin() || declared(kind)).collect()
}

fn kind(connector: &str) -> Result<Kind, String> {
    super::kind_of(connector).ok_or_else(|| tf("{0} is not a connector that is installed.", &[&connector]))
}

/// The fields and the secrets as one set of values, as `accounts` takes them.
fn values(mut config: Map<String, Value>, secrets: BTreeMap<String, String>) -> Map<String, Value> {
    config.extend(secrets.into_iter().map(|(name, value)| (name, Value::String(value))));
    config
}

/// The extension's permission to sync accounts, asked in the palette the
/// first time one of its connectors is given an account.
fn allowed(app: &AppHandle, kind: &Kind) -> Result<(), String> {
    let caller = permissions::Caller {
        name: kind.extension_name.clone(),
        title: kind.extension_title.clone(),
        heyo: kind.heyo.clone(),
        builtin: kind.builtin(),
        background: false,
        route: crate::wire::Route::Heyo,
        location: kind.extension.clone(),
    };
    let need = Need { permission: Permission::Connector, unasked: false, member: "connector" };
    permissions::GRANTED.check(&caller, need, false, &|consent| permissions::ask(app, consent)).map_err(|refused| refused.message)
}

/// Adds an account of the connector `connector` (`<extension>:<command>`),
/// named `name`, with the values of its fields and its secrets; it is
/// synced at once.
pub(crate) fn add(app: &AppHandle, connector: String, name: String, config: Map<String, Value>, secrets: BTreeMap<String, String>) -> Result<Vec<View>, String> {
    let kind = kind(&connector)?;
    allowed(app, &kind)?;
    let values = values(config, secrets);
    let id = crate::commands::settings::change_with(app, |settings| accounts::add(settings, &kind, &name, &values))?;
    shell(app)?.added(&id, kind.plan());
    Ok(list(app))
}

/// Changes an account: its name if given, and the fields and secrets
/// given (an empty secret is one removed). It syncs everything again.
pub(crate) fn edit(app: &AppHandle, id: String, name: Option<String>, config: Map<String, Value>, secrets: BTreeMap<String, String>) -> Result<Vec<View>, String> {
    let connector = app.state::<crate::SettingsState>().0.read().unwrap().connections.iter().find(|account| account.id == id).map(|account| account.connector.clone());
    let kind = kind(&connector.ok_or_else(|| tf("There is no connection {0}.", &[&id]))?)?;
    let values = values(config, secrets);
    crate::commands::settings::change_with(app, |settings| accounts::edit(settings, &kind, &id, name.as_deref(), &values))?;
    shell(app)?.edited(&id, kind.plan());
    Ok(list(app))
}

/// Removes an account, its secrets and its items.
pub(crate) fn remove(app: &AppHandle, id: String) -> Result<Vec<View>, String> {
    crate::commands::settings::change_with(app, |settings| accounts::remove(settings, &id))?;
    shell(app)?.removed(&id);
    Ok(list(app))
}

/// Whether the account works as it is set up, as its connector says; what
/// went wrong in its words. Blocks, up to a minute.
pub(crate) fn check(app: &AppHandle, id: String) -> Result<(), String> {
    shell(app)?.check(&id).map_err(|error| error.message)
}

/// Syncs the account now, or as soon as the sync running ends.
pub(crate) fn sync_now(app: &AppHandle, id: String) -> Result<(), String> {
    if !app.state::<crate::SettingsState>().0.read().unwrap().connections.iter().any(|account| account.id == id) {
        return Err(tf("There is no connection {0}.", &[&id]));
    }
    shell(app)?.sync_now(&id);
    Ok(())
}

/// Whether an account of `connector`, set up with these fields and secrets,
/// would work, before it is added; or, `of` an account there is, as it
/// would be changed (a secret not given is the account's). Blocks, up to a
/// minute.
pub(crate) fn try_out(app: &AppHandle, connector: String, of: Option<String>, config: Map<String, Value>, secrets: BTreeMap<String, String>) -> Result<(), String> {
    let kind = kind(&connector)?;
    let saved = of.as_ref().and_then(|id| app.state::<crate::SettingsState>().0.read().unwrap().connections.iter().find(|account| &account.id == id).cloned());
    // A change is tried on what the account has, with what changes.
    let mut values = saved.as_ref().map(|account| account.config.clone()).unwrap_or_default();
    values.extend(config);
    let (config, tried) = accounts::trial(&kind, &self::values(values, secrets))?;
    let name = saved.as_ref().map_or_else(|| kind.title.clone(), |account| account.name.clone());
    let account = super::given(&accounts::Account { id: String::new(), connector: kind.id.clone(), name, config }, &kind);
    shell(app)?.try_out(account, kind, tried, saved.map(|account| account.id)).map_err(|error| error.message)
}

/// What `query` finds among the synced items, within 50 ms, with the
/// accounts they are of.
pub(crate) fn search(app: &AppHandle, query: &str) -> Searched {
    let Some(shell) = app.state::<Connections>().shell().cloned() else { return Searched::nothing() };
    let mut searched = shell.search(query, FOUND);
    let settings = app.state::<crate::SettingsState>().0.read().unwrap().connections.clone();
    let kinds = accounts::kinds();
    for found in &searched.found {
        let Some(account) = settings.iter().find(|account| account.id == found.account) else { continue };
        let web = kinds.iter().find(|kind| kind.id == account.connector).and_then(|kind| kind.web(&account.config)).map(str::to_owned);
        searched.accounts.entry(account.id.clone()).or_insert_with(|| FoundAccount { name: account.name.clone(), web });
    }
    searched
}

/// The palette's search of what connectors synced.
#[tauri::command(async)]
pub(crate) fn search_connections(query: String, app: AppHandle) -> Searched {
    search(&app, &query)
}

/// Shows an item that was found in the palette, by Heyo's own command
/// that previews one (`connection-item`, which the root search does not
/// list): it is given the item, and the name of its account.
#[tauri::command]
pub(crate) fn preview_connection(account: String, id: String, app: AppHandle) -> Result<(), String> {
    let item = shell(&app)?.item(&account, &id).ok_or_else(|| tf("There is no item {0}.", &[&id]))?;
    let name = app.state::<crate::SettingsState>().0.read().unwrap().connections.iter().find(|known| known.id == account).map_or(account.clone(), |known| known.name.clone());
    let arguments = serde_json::json!({ "item": serde_json::to_string(&item).expect("plain data"), "account": name });
    let launch = crate::wire::HostMessage::Launch { extension: crate::extensions::BUILTIN.into(), command: PREVIEW.into(), fallback_text: None, arguments: Some(arguments), launch_context: None };
    app.state::<crate::Runtime>().send(&app, &launch.json())
}

/// The command of Heyo's own that previews an item (`runtime/src/builtins`).
pub(crate) const PREVIEW: &str = "connection-item";
