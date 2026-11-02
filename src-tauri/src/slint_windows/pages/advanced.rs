//! Advanced: Node.js for extensions, how the web view draws, the models
//! of AI, pasting into terminals, and where the settings are.

use super::models::{listings, pick, picker, to_models};
use super::{from_word, on, tile, word, Known, Page, Reaction, Sheet};
use crate::i18n::{marked, t, tf, translate};
use crate::models::registry::Use;
use crate::slint_windows::host::Command;
use crate::slint_windows::ids::{Act, Id, PageId, Setting};
use crate::slint_windows::model::{ask, Ask, Entry, Line, Then};

pub(crate) const PAGE: &str = "advanced";

const RENDERERS: &[(&str, &str)] = &[("auto", marked("Automatic")), ("cpu", "CPU"), ("gpu-shared-memory", marked("GPU, shared memory")), ("gpu-dma-buf", marked("GPU, DMA-BUF (WebKit's default)"))];

pub(crate) struct Advanced;

impl Page for Advanced {
    fn entries(&self, _: &Known) -> Vec<Entry> {
        vec![tile(PAGE, marked("Advanced"), "wrench-screwdriver-16")]
    }

    fn shows(&self, at: &PageId) -> bool {
        *at == PageId::own(PAGE)
    }

    fn lines(&self, _: &PageId, known: &Known) -> Vec<Line> {
        let (view, settings) = (&known.settings, &known.settings.settings);
        let mut page = Sheet::default();
        let version = match &view.node_version {
            Ok(version) => tf("Using {0}.", &[&version]),
            Err(why) => why.clone(),
        };
        page.card(
            t("Extensions"),
            vec![
                Line::row(Id::Set(Setting::NodePath), t("Node.js")).says(format!("{version}{} {}", if view.node_overridden { format!(" {}", t("HEYO_NODE overrides this setting.")) } else { String::new() }, t("Extensions need Node.js 22 or newer."))).warns(view.node_version.is_err()).field(settings.node_path.clone(), "node (from PATH)"),
                Line::row(Id::Act(Act::OpenExtensions), t("Extensions Folder")).says(view.extensions_dir.clone()).button(t("Open")),
            ],
        );
        page.card(
            t("Rendering"),
            vec![Line::row(Id::Set(Setting::Renderer), t("Renderer")).says(t("How the palette and what else the web view draws is drawn. Automatic keeps to the CPU on an NVIDIA driver without modesetting (the GPU cannot share frames there) and lets WebKit choose elsewhere; GPU with shared memory draws on the GPU and copies each frame, which is faster at 200% scale and keeps every CSS effect. Takes effect when Heyo is next started.")).select(&word(&settings.renderer), RENDERERS.iter().map(|(value, title)| (*value, translate(title))))],
        );
        page.card(
            "AI",
            vec![
                picker(known, Use::Answers, t("Model"), t("Answers Ask AI, AI Chat, Selected Text Actions and extensions."), None),
                picker(known, Use::ClipboardCases, t("Clipboard Search Cases"), t("Writes the cases the clipboard search is measured by, from what you copied: only a model that keeps it on this computer is offered."), Some(t("None"))),
                to_models(),
            ],
        );
        page.card(t("Pasting"), vec![Line::row(Id::Set(Setting::ConfirmTerminalPaste), t("Ask Before Pasting Several Lines into a Terminal")).says(t("A terminal runs each pasted line as a command. Dictation never asks: there, line breaks become spaces.")).switch(settings.confirm_terminal_paste)]);
        page.card(t("Data"), vec![Line::row("", t("Settings File")).says(view.settings_path.clone())]);
        page.done()
    }

    fn acted(&mut self, _: &PageId, known: &mut Known, id: &Id, value: &str) -> Option<Reaction> {
        let asks = match id {
            Id::Set(Setting::NodePath) => known.change(|view| view.settings.node_path = value.to_string()),
            Id::Set(Setting::Renderer) => match from_word(value) {
                Some(renderer) => known.change(|view| view.settings.renderer = renderer),
                None => Vec::new(),
            },
            Id::Set(Setting::ConfirmTerminalPaste) => known.change(|view| view.settings.confirm_terminal_paste = on(value)),
            Id::Pick(need) => pick(known, *need, value),
            Id::Act(Act::OpenExtensions) => vec![ask(Command::OpenExtensionsFolder, Then::Known)],
            Id::Act(Act::OpenModels) => return Some(Reaction::showing(PageId::own(super::models::PAGE))),
            _ => return None,
        };
        Some(Reaction::asking(asks))
    }

    fn entered(&self, _: &PageId, known: &Known) -> Vec<Ask> {
        listings(known)
    }
}
