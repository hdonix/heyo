//! About: the version, the diagnostics to copy, and updating.

use super::{tile, Known, Page, Reaction};
use crate::i18n::{marked, t, tf};
use crate::slint_windows::answer::Answer;
use crate::slint_windows::host::Command;
use crate::slint_windows::ids::{Act, Id, PageId};
use crate::slint_windows::model::{ask, Entry, Line, Then};

pub(crate) const PAGE: &str = "about";

/// What the update button is doing.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) enum Update {
    /// Not asked yet.
    #[default]
    Unknown,
    Checking,
    /// This is the newest.
    Newest,
    /// A newer version, by its number.
    Newer(String),
    Installing(String),
}

#[derive(Debug, Default)]
pub(crate) struct About {
    pub update: Update,
}

impl Page for About {
    fn entries(&self, _: &Known) -> Vec<Entry> {
        vec![tile(PAGE, marked("About"), "info-01-16")]
    }

    fn shows(&self, at: &PageId) -> bool {
        *at == PageId::own(PAGE)
    }

    fn lines(&self, _: &PageId, known: &Known) -> Vec<Line> {
        let update = match &self.update {
            _ if !known.settings.can_update => String::new(),
            Update::Unknown => t("Check for Updates").into(),
            Update::Checking => t("Checking…").into(),
            Update::Newest => t("Heyo Is Up to Date").into(),
            Update::Newer(version) => tf("Install Heyo {0} and Restart", &[version]),
            Update::Installing(_) => t("Installing…").into(),
        };
        vec![Line {
            kind: "about",
            // Its two buttons: the diagnostics, and the update.
            id: Id::Act(Act::CopyDiagnostics).into(),
            more_id: Id::Act(Act::Update).into(),
            value: known.settings.version.clone(),
            description: t("A command palette launcher for Linux that runs Raycast extensions.").into(),
            placeholder: t("For a bug report: versions, the desktop session and the end of Heyo's log, with keys and tokens removed. Read it before you share it.").into(),
            more: update,
            busy: matches!(self.update, Update::Checking | Update::Installing(_)),
            on: known.copied,
            ..Line::default()
        }]
    }

    fn acted(&mut self, _: &PageId, _: &mut Known, id: &Id, _: &str) -> Option<Reaction> {
        let then = Then::Acted(id.clone());
        let asks = match id {
            Id::Act(Act::CopyDiagnostics) => vec![ask(Command::CopyDiagnostics, then)],
            Id::Act(Act::Update) => match self.update.clone() {
                Update::Newer(version) => {
                    self.update = Update::Installing(version);
                    vec![ask(Command::InstallUpdate, then)]
                }
                Update::Unknown | Update::Newest => {
                    self.update = Update::Checking;
                    vec![ask(Command::CheckForUpdate, then)]
                }
                Update::Checking | Update::Installing(_) => Vec::new(),
            },
            _ => return None,
        };
        Some(Reaction::asking(asks))
    }

    fn answered(&mut self, known: &mut Known, id: &Id, answer: &Result<Answer, String>) -> Reaction {
        match (id, answer) {
            (Id::Act(Act::CopyDiagnostics), Ok(_)) => known.copied = true,
            (Id::Act(Act::Update), Ok(Answer::Update(newer))) => self.update = newer.clone().map_or(Update::Newest, Update::Newer),
            // Looking failed: it can be looked for again; installing failed:
            // it can be installed again.
            (Id::Act(Act::Update), Err(_)) => {
                self.update = match &self.update {
                    Update::Checking => Update::Unknown,
                    Update::Installing(version) => Update::Newer(version.clone()),
                    other => other.clone(),
                }
            }
            _ => {}
        }
        Reaction::default()
    }
}
