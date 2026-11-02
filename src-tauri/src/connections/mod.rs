//! Connections: accounts of services (mail, calendars, MCP servers) whose
//! items a connector, a command of an extension or of Heyo's own, syncs
//! into a store on this computer, to be found from the palette.
//!
//! - `accounts.rs`: the accounts (settings), the connectors they can be of
//!   (manifests), and what is remembered of their syncing.
//! - `scheduler.rs`: when each account is synced and watched, as plain
//!   data with the time passed in.
//! - `shell.rs`: what carries that out: the connections runtime's process
//!   (`runtime/src/connections/`, its conversation typed in `wire.rs`),
//!   the store, the timeouts.
//! - `items.rs`: the items, and the store they are kept in.
//!
//! This file is the shell as the running application has it
//! ([`Connections`], from `Live`), and what the settings and the palette
//! ask of it.

pub mod accounts;
pub mod items;
pub mod scheduler;
pub mod shell;

use std::sync::{Arc, OnceLock};

use tauri::{AppHandle, Manager};

use self::accounts::Kind;
use self::shell::{Place, Setup, Shell};
use crate::secrets::Owner;
use crate::wire::{ConnectorAccount, ConnectorCommand};

/// The connections of the running application: started with it, once the
/// settings are read.
#[derive(Default)]
pub struct Connections(OnceLock<Arc<Shell>>);

impl Connections {
    pub fn shell(&self) -> Option<&Arc<Shell>> {
        self.0.get()
    }
}

/// Heyo itself, as the shell asks it.
struct Live(AppHandle);

impl Live {
    fn settings(&self) -> crate::settings::Settings {
        self.0.state::<crate::SettingsState>().0.read().unwrap().clone()
    }
}

/// The connector an account names, among those there are.
pub fn kind_of(connector: &str) -> Option<Kind> {
    accounts::kinds().into_iter().find(|kind| kind.id == connector)
}

/// The account as its connector is given it.
pub fn given(account: &accounts::Account, kind: &Kind) -> ConnectorAccount {
    ConnectorAccount {
        id: account.id.clone(),
        name: account.name.clone(),
        connector: ConnectorCommand { extension: kind.extension.clone(), command: kind.command.clone() },
        config: account.config.clone(),
    }
}

impl Place for Live {
    fn account(&self, id: &str) -> Option<(ConnectorAccount, Kind)> {
        let account = self.settings().connections.into_iter().find(|account| account.id == id)?;
        let kind = kind_of(&account.connector)?;
        Some((given(&account, &kind), kind))
    }

    fn secret(&self, account: &str, name: &str) -> Option<String> {
        self.settings().secrets.read(Owner::Connection(account), name).map(str::to_owned)
    }

    fn runtime(&self) -> Result<std::process::Command, String> {
        let mut command = crate::helper::node(&self.0, "heyo-connections.cjs", "HEYO_CONNECTIONS")?;
        command.envs(crate::paths::for_runtime());
        Ok(command)
    }
}

/// Starts the connections: the accounts there are are scheduled from where
/// they were left. Nothing runs until there is an account.
pub fn start(app: &AppHandle) {
    let items = match items::Items::open(&crate::paths::data().join("connections.db")) {
        Ok(items) => Arc::new(items),
        Err(e) => return eprintln!("heyo: connections are off: the store could not be opened: {e}"),
    };
    let setup = Setup { kept: Some(crate::paths::data().join("connections.json")), ..Setup::default() };
    let shell = Shell::start(Live(app.clone()), items, setup);
    let kinds = accounts::kinds();
    let settings = app.state::<crate::SettingsState>().0.read().unwrap().connections.clone();
    let scheduled: Vec<_> = settings
        .iter()
        .map(|account| (account.id.clone(), kinds.iter().find(|kind| kind.id == account.connector).map(Kind::plan).unwrap_or_default()))
        .collect();
    shell.begin(&scheduled);
    let _ = app.state::<Connections>().0.set(shell);
}
