//! General: opening at login, Heyo's hotkey, how it looks, its window,
//! live activities; and GNOME Shell's extension where it is wanted.

use super::{from_word, on, tile, word, Known, Page, Reaction, Sheet};
use crate::commands::settings::HotkeyName;
use crate::i18n::{self, marked, t};
use crate::slint_windows::host::Command;
use crate::slint_windows::ids::{Act, Bound, Id, PageId, Setting};
use crate::slint_windows::model::{ask, Entry, Line, Then};

pub(crate) const PAGE: &str = "general";

pub(crate) struct General;

impl Page for General {
    fn entries(&self, _: &Known) -> Vec<Entry> {
        vec![tile(PAGE, marked("General"), "cog-16")]
    }

    fn shows(&self, at: &PageId) -> bool {
        *at == PageId::own(PAGE)
    }

    fn lines(&self, _: &PageId, known: &Known) -> Vec<Line> {
        let (view, settings) = (&known.settings, &known.settings.settings);
        let mut page = Sheet::default();
        // GNOME's Shell extension, where there is a use for it and it is
        // not running as it should.
        let extension = match known.capabilities.as_ref().map(|capabilities| capabilities.extension.as_ref()) {
            Some("missing") => Some((t("On Wayland, switching and arranging windows, pasting, clipboard history, screenshots and the hold-to-talk key go through a small GNOME Shell extension that only Heyo can use."), t("Install"))),
            Some("outdated") => Some((t("This version of Heyo comes with a newer extension than the one installed. Update it, then log out and back in. An extension that is already running keeps working until then, without what the new one adds."), t("Update"))),
            Some("pending") => Some((t("In place. Log out and back in to finish: GNOME loads a new or updated extension only when a session starts."), "")),
            _ => None,
        };
        if let Some((description, action)) = extension {
            let row = Line::row(Id::Act(Act::InstallShellExtension), t("GNOME Shell Extension")).says(description);
            page.card(t("Wayland"), vec![if action.is_empty() { row } else { row.button(action) }]);
        }
        let hotkey = Line::row("", t("Heyo Hotkey"));
        page.card(
            "",
            vec![
                Line::row(Id::Set(Setting::LaunchAtLogin), t("Open at Login")).switch(view.launch_at_login),
                if known.hotkeys_supported() { hotkey.hotkey(&Bound::Own(HotkeyName::Toggle).to_string(), known.hotkeys.toggle.as_deref()) } else { hotkey.says(t("Bind a desktop keyboard shortcut to heyo.")) },
            ],
        );
        // Each language by its own name, as `i18n::languages` has them,
        // English too: whoever cannot read the one in use finds theirs.
        let languages = [(i18n::SYSTEM, t("System")), (i18n::SOURCE, "English")].into_iter().chain(i18n::languages());
        page.card(
            t("Appearance"),
            vec![
                Line::row(Id::Set(Setting::Appearance), t("Theme")).says(t("Light, dark, or whichever the system is using")).select(&word(&settings.appearance), [("system", t("System")), ("light", t("Light")), ("dark", t("Dark"))]),
                Line::row(Id::Set(Setting::Language), t("Language")).says(t("What Heyo itself says. Extensions speak their own.")).select(&settings.language, languages),
            ],
        );
        page.card(
            t("Window"),
            vec![
                Line::row(Id::Set(Setting::CompactMode), t("Compact Mode")).says(t("Show only the search field until you type")).switch(settings.compact_mode),
                Line::row(Id::Set(Setting::HideOnBlur), t("Hide When Focus Is Lost")).says(t("Close the window when you click elsewhere")).switch(settings.hide_on_blur),
                Line::row(Id::Set(Setting::KeepCommand), t("Keep the Running Command")).says(t("Reopening Heyo returns to the command you were using instead of the root search")).switch(settings.keep_command),
            ],
        );
        page.card(t("Live Activities"), vec![Line::row(Id::Set(Setting::LiveActivities), t("Show What Is in Progress")).says(t("Timers, downloads and what extensions report as under way, at the top of the root search, with a notification when one ends or needs you")).switch(settings.live_activities)]);
        page.done()
    }

    fn acted(&mut self, _: &PageId, known: &mut Known, id: &Id, value: &str) -> Option<Reaction> {
        let asks = match id {
            Id::Set(Setting::LaunchAtLogin) => known.change(|view| view.launch_at_login = on(value)),
            Id::Set(Setting::Appearance) => match from_word(value) {
                Some(appearance) => known.change(|view| view.settings.appearance = appearance),
                None => Vec::new(),
            },
            Id::Set(Setting::Language) => known.change(|view| view.settings.language = value.to_string()),
            Id::Set(Setting::CompactMode) => known.change(|view| view.settings.compact_mode = on(value)),
            Id::Set(Setting::HideOnBlur) => known.change(|view| view.settings.hide_on_blur = on(value)),
            Id::Set(Setting::KeepCommand) => known.change(|view| view.settings.keep_command = on(value)),
            Id::Set(Setting::LiveActivities) => known.change(|view| view.settings.live_activities = on(value)),
            Id::Act(Act::InstallShellExtension) => vec![ask(Command::InstallShellExtension, Then::Known)],
            _ => return None,
        };
        Some(Reaction::asking(asks))
    }
}
