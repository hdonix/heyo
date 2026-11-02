//! What the settings window shows and what its controls do, worked out
//! from what the host's commands answer.
//!
//! The window knows nothing of settings: it draws lines (`Line`: a
//! heading, a row with a control, a row of a table) and says which control
//! was used. This is everything in between. Each page is a module of its
//! own (`pages`): its lines, made from what the host answered (`Known`,
//! typed as the host's commands answer it, `answer`), what its controls
//! ask of the host (`Ask`), and what it makes of the answers. The model
//! here routes: it keeps which page is shown, the sidebar and its search,
//! and puts each answer where it is known. None of it needs a window, so
//! all of it is tested with answers written out.
//!
//! What the host decides is not decided here: every change goes through a
//! command, and is shown as the host then says it is.

use super::answer::Answer;
use super::host::Command;
use super::ids::{Bound, Id, PageId};
use super::keys;
use super::pages::{self, Known, Page, Reaction, To};
use crate::i18n::{marked, t};

/// The languages dictation can be told to hear.
pub(crate) use crate::dictation::LANGUAGES as DICTATION_LANGUAGES;

/// One line of a page, as `ui/settings.slint` draws it (see `Line` there).
/// `icon` is a file, or the name of a symbol.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Line {
    pub kind: &'static str,
    pub id: String,
    pub first: bool,
    pub last: bool,
    pub space: f32,
    pub indent: bool,
    pub label: String,
    pub description: String,
    pub danger: bool,
    pub icon: Option<String>,
    pub control: &'static str,
    pub on: bool,
    pub mixed: bool,
    pub value: String,
    pub placeholder: String,
    pub choices: Vec<(String, String)>,
    pub hotkey: String,
    pub keys: Vec<String>,
    pub more: String,
    /// What its further button acts on, its box to tick, and the field
    /// of its alias: a line with more than one control has an id for each.
    pub more_id: String,
    pub alias: String,
    pub alias_id: String,
    pub checkable: bool,
    pub check_id: String,
    pub open: bool,
    pub busy: bool,
    /// Words shown as chips before its control: what a provider can do.
    pub chips: Vec<String>,
}

impl Line {
    pub(crate) fn row(id: impl Into<String>, label: impl Into<String>) -> Line {
        Line { kind: "row", id: id.into(), label: label.into(), ..Line::default() }
    }

    pub(crate) fn says(mut self, description: impl Into<String>) -> Line {
        self.description = description.into();
        self
    }

    pub(crate) fn warns(mut self, danger: bool) -> Line {
        self.danger = danger;
        self
    }

    pub(crate) fn pictured(mut self, icon: Option<String>) -> Line {
        self.icon = icon;
        self
    }

    pub(crate) fn switch(mut self, on: bool) -> Line {
        (self.control, self.on) = ("switch", on);
        self
    }

    pub(crate) fn select<'a>(mut self, value: &str, choices: impl IntoIterator<Item = (&'a str, &'a str)>) -> Line {
        (self.control, self.value) = ("select", value.to_string());
        self.choices = choices.into_iter().map(|(value, title)| (value.to_string(), title.to_string())).collect();
        self
    }

    pub(crate) fn field(mut self, value: impl Into<String>, placeholder: &str) -> Line {
        (self.control, self.value, self.placeholder) = ("field", value.into(), placeholder.to_string());
        self
    }

    /// A field for something that is never shown again once it is said:
    /// what was typed is taken out of it. (A "password" field is another
    /// thing: it shows, as dots, what is stored.)
    pub(crate) fn secret(mut self, placeholder: &str) -> Line {
        (self.control, self.placeholder) = ("secret", placeholder.to_string());
        self
    }

    pub(crate) fn button(mut self, words: &str) -> Line {
        (self.control, self.value) = ("button", words.to_string());
        self
    }

    /// A button that asks for a second click, saying `again` in between.
    pub(crate) fn confirm(mut self, words: &str, again: &str) -> Line {
        (self.control, self.value, self.placeholder) = ("confirm", words.to_string(), again.to_string());
        self
    }

    /// The whole row is pressed to go where its id says, which a chevron shows.
    pub(crate) fn link(mut self) -> Line {
        self.control = "link";
        self
    }

    pub(crate) fn chips(mut self, chips: Vec<String>) -> Line {
        self.chips = chips;
        self
    }

    pub(crate) fn badge(mut self, words: &str) -> Line {
        (self.control, self.value) = ("badge", words.to_string());
        self
    }

    pub(crate) fn text(mut self, words: impl Into<String>) -> Line {
        (self.control, self.value) = ("text", words.into());
        self
    }

    /// A further button, and what it acts on.
    pub(crate) fn more(mut self, words: &str, id: Id) -> Line {
        (self.more, self.more_id) = (words.to_string(), id.into());
        self
    }

    /// A field that records the hotkey `name`, showing the one bound.
    pub(crate) fn hotkey(mut self, name: &str, accelerator: Option<&str>) -> Line {
        self.hotkey = name.to_string();
        self.keys = accelerator.filter(|a| !a.is_empty()).map(keys::parts).unwrap_or_default();
        self
    }
}

/// What is different between the lines a page showed and the lines it
/// is to show, as Slint's list is told it.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Changes {
    /// How many lines at the beginning stay.
    pub head: usize,
    /// After them: how many of the old go, and how many new ones come in
    /// their place.
    pub out: usize,
    pub come: usize,
    /// Which of the lines that stay are not as they were, by their place
    /// among the new.
    pub changed: Vec<usize>,
}

/// What Slint's list is told when a page's lines become others: the
/// lines that begin and end both stay (changed, where they changed), and
/// what is between them is taken out and put in. A line stays if it is
/// the same line of the same thing, whatever it now says (`kind`, `id`
/// and `label`).
///
/// So a control that is being used stays the control it was (a switch
/// moves, a field keeps what is typed into it), a list that is opened
/// further down stays where it was scrolled to, and another page is not
/// left scrolled to where the last one was: Slint's list keeps its place
/// by the number of its first row, and forgets it only when that row is
/// taken out.
pub(crate) fn changes(was: &[Line], is: &[Line]) -> Changes {
    let alike = |was: &Line, is: &Line| was.kind == is.kind && was.id == is.id && was.label == is.label;
    let head = was.iter().zip(is).take_while(|(was, is)| alike(was, is)).count();
    let most = was.len().min(is.len()) - head;
    let tail = was.iter().rev().zip(is.iter().rev()).take(most).take_while(|(was, is)| alike(was, is)).count();
    let at_head = (0..head).filter(|at| was[*at] != is[*at]);
    let at_tail = (0..tail).filter(|back| was[was.len() - 1 - back] != is[is.len() - 1 - back]).map(|back| is.len() - 1 - back);
    Changes { head, out: was.len() - head - tail, come: is.len() - head - tail, changed: at_head.chain(at_tail).collect() }
}

/// One entry of the sidebar. `icon` is a file or the name of a symbol.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Entry {
    pub id: String,
    pub title: String,
    pub icon: Option<String>,
    pub extension: bool,
    /// What the sidebar's search finds it by.
    pub found_by: Vec<String>,
}

/// Something to ask of the host, and what its answer is for.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Ask {
    pub command: Command,
    pub then: Then,
}

/// What an answer is for: where `Model::answered` puts it. What the host
/// answers says by its type what it is (`Answer`); this says what only the
/// asker knows.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Then {
    /// The settings as they now are; refused, they are read again.
    Settings,
    /// Put where it is known, whatever it is.
    Known,
    /// What the extension with this id may ask Heyo for.
    Permissions(String),
    /// The models the service that is this provider lists, by its id; or
    /// why it could not be asked, which is said where they would be.
    Models(String),
    /// The item's hotkey was set to this accelerator.
    Bound(String, String),
    /// For the page whose control this was (`Page::answered`).
    Acted(Id),
}

pub(crate) fn ask(command: Command, then: Then) -> Ask {
    Ask { command, then }
}

/// Everything the window was told, and where it is.
pub(crate) struct Model {
    pub known: Known,
    pub page: PageId,
    pub nav_search: String,
    pub error: String,
    pages: Vec<Box<dyn Page>>,
}

impl Model {
    pub(crate) fn new() -> Model {
        Model { known: Known::default(), page: PageId::own(pages::general::PAGE), nav_search: String::new(), error: String::new(), pages: pages::all() }
    }

    pub(crate) fn hotkeys_supported(&self) -> bool {
        self.known.hotkeys_supported()
    }

    /// The page module that shows the page that is shown.
    fn current(&self) -> Option<&dyn Page> {
        self.pages.iter().find(|page| page.shows(&self.page)).map(|page| page.as_ref())
    }

    /// What the sidebar lists: the pages, then the extensions, each only
    /// if the search finds it; and how many of them are pages.
    pub(crate) fn nav(&self) -> (Vec<Entry>, usize) {
        let query = self.nav_search.to_lowercase();
        let found = |entry: &Entry| query.is_empty() || entry.found_by.iter().any(|title| title.to_lowercase().contains(&query));
        let entries: Vec<Entry> = self.pages.iter().flat_map(|page| page.entries(&self.known)).filter(found).collect();
        let pages = entries.iter().filter(|entry| !entry.extension).count();
        (entries, pages)
    }

    /// The entry of the sidebar the page shown belongs to: its own, or
    /// its module's only one (a provider's page belongs to Models).
    fn entry(&self) -> Option<Entry> {
        let entries = self.current()?.entries(&self.known);
        let at = self.page.to_string();
        let only = entries.len() == 1;
        entries.into_iter().find(|entry| only || entry.id == at)
    }

    /// The id of the sidebar's entry the page shown belongs to.
    pub(crate) fn section(&self) -> String {
        self.entry().map_or_else(|| self.page.to_string(), |entry| entry.id)
    }

    pub(crate) fn title(&self) -> String {
        self.entry().map_or_else(|| t("Settings").into(), |entry| entry.title)
    }

    /// The words in the field above the page's lines, if it has one.
    pub(crate) fn find_placeholder(&self) -> &'static str {
        self.current().map_or("", |page| page.header(&self.page).placeholder)
    }

    /// The menu beside that field: what its button says, how many things
    /// are ticked that narrow the page, and its entries (id, title,
    /// whether it is a heading, whether it is ticked).
    pub(crate) fn filter(&self) -> Option<(String, usize, Vec<(String, String, bool, bool)>)> {
        self.current().and_then(|page| page.header(&self.page).filter)
    }

    /// The control `id` of the page shown was used, saying `value`.
    fn act(&mut self, id: &Id, value: &str) -> Option<Reaction> {
        let Model { known, pages, page: at, .. } = self;
        pages.iter_mut().find(|page| page.shows(at)).and_then(|page| page.acted(at, known, id, value))
    }

    /// An entry of that menu was ticked.
    pub(crate) fn pick(&mut self, id: &str) {
        self.act(&Id::parse(id), "");
    }

    /// The lines of the page that is shown.
    pub(crate) fn lines(&self) -> Vec<Line> {
        self.current().map(|page| page.lines(&self.page, &self.known)).unwrap_or_default()
    }

    /// What a page's control, or an answer, came to.
    fn react(&mut self, reaction: Reaction) -> Vec<Ask> {
        let mut asks = reaction.asks;
        match reaction.to {
            Some(To::Show(page)) => asks.extend(self.show_page(page)),
            Some(To::Move(page)) => self.page = page,
            None => {}
        }
        asks
    }

    /// The title of the item `id`, which binding a hotkey to it names.
    fn item_title(&self, id: &str) -> Option<String> {
        match id {
            "heyo:screenshot" => return Some(t("Capture Screenshot").into()),
            "heyo:screenshot-text" => return Some(t("Capture Text from Screen").into()),
            _ => {}
        }
        let applications = self.known.applications.iter().map(|item| (&item.id, &item.title));
        let commands = self.known.extensions.iter().flat_map(|extension| extension.commands.iter().map(|command| (&command.id, &command.title)));
        applications.chain(commands).find(|(item, _)| *item == id).map(|(_, title)| title.clone())
    }

    /// Binds `accelerator` (none, when it is empty) to one of Heyo's own
    /// hotkeys or to an item.
    pub(crate) fn bind(&mut self, id: &str, accelerator: &str) -> Vec<Ask> {
        self.error.clear();
        match Bound::parse(id) {
            Bound::Own(name) => vec![ask(Command::SetHotkey { name, accelerator: accelerator.to_string() }, Then::Known)],
            Bound::Item(id) => {
                let Some(title) = self.item_title(&id) else { return Vec::new() };
                vec![ask(Command::SetItemHotkey { id: id.clone(), title, accelerator: accelerator.to_string() }, Then::Bound(id, accelerator.to_string()))]
            }
        }
    }

    /// The control `id` was used and now says `value`: what to ask of the
    /// host. What only changes what is shown is changed here.
    pub(crate) fn acted(&mut self, id: &str, value: &str) -> Vec<Ask> {
        // What went wrong before stays said until something else is done:
        // not until the host next answers, which it also does unasked (a
        // download that failed is followed by a look at what is installed).
        self.error.clear();
        match Id::parse(id) {
            Id::Go(page) => self.show_page(page),
            id => self.act(&id, value).map(|reaction| self.react(reaction)).unwrap_or_default(),
        }
    }

    // --- What is asked without a control -------------------------------------

    /// What is read when the window is shown: it may have changed while
    /// the window was away.
    pub(crate) fn opened(&self) -> Vec<Ask> {
        let mut asks = vec![ask(Command::GetSettings, Then::Settings), ask(Command::GetHotkeys, Then::Known), ask(Command::GetCapabilities, Then::Known)];
        asks.extend(pages::reload());
        asks.extend(self.entered());
        asks
    }

    /// What the page that is shown needs to know besides.
    pub(crate) fn entered(&self) -> Vec<Ask> {
        self.current().map(|page| page.entered(&self.page, &self.known)).unwrap_or_default()
    }

    /// What is asked again every two seconds.
    pub(crate) fn poll(&self) -> Vec<Ask> {
        self.current().map(|page| page.poll(&self.page, &self.known)).unwrap_or_default()
    }

    /// Another page is shown, as the sidebar names it.
    pub(crate) fn show(&mut self, page: &str) -> Vec<Ask> {
        self.show_page(PageId::parse(page))
    }

    fn show_page(&mut self, page: PageId) -> Vec<Ask> {
        if self.page == page {
            return Vec::new();
        }
        self.page = page;
        self.known.find.clear();
        self.error.clear();
        self.entered()
    }

    /// The host answered: what is known changes, and more may be asked.
    pub(crate) fn answered(&mut self, then: Then, answer: Result<Answer, String>) -> Vec<Ask> {
        if let Then::Models(provider) = then {
            let models = answer.map(|answer| if let Answer::Models(models) = answer { models } else { Vec::new() });
            self.known.listed.insert(provider, models);
            return Vec::new();
        }
        let (error, mut asks) = (self.error.clone(), Vec::new());
        match &answer {
            Ok(told) => match &then {
                Then::Permissions(extension) => {
                    if let Answer::Permissions(rows) = told {
                        self.known.permissions = (extension.clone(), rows.clone());
                    }
                }
                Then::Bound(id, accelerator) => {
                    if accelerator.is_empty() {
                        self.known.item_hotkeys.remove(id);
                    } else {
                        self.known.item_hotkeys.insert(id.clone(), accelerator.clone());
                    }
                }
                _ => self.known.learn(told.clone()),
            },
            Err(message) => {
                self.error = message.clone();
                // What was shown before the host answered may not be so.
                if then == Then::Settings {
                    asks.push(ask(Command::GetSettings, Then::Known));
                }
            }
        }
        if let Then::Acted(id) = &then {
            let Model { known, pages, .. } = self;
            let reactions: Vec<Reaction> = pages.iter_mut().map(|page| page.answered(known, id, &answer)).collect();
            for reaction in reactions {
                if reaction.said {
                    self.error = error.clone();
                }
                asks.extend(self.react(reaction));
            }
        }
        asks
    }

    /// The host emitted something to its windows.
    pub(crate) fn heard(&mut self, event: &str, told: &serde_json::Value) -> Vec<Ask> {
        use crate::wire::event;
        // A download's progress, or its end: true when it is over.
        let over = told["type"] != "progress";
        if over && told["type"] == "error" && event != event::SETTINGS_CHANGED && event != event::INDEX_READY {
            self.error = told["message"].as_str().unwrap_or_default().to_string();
        }
        match event {
            // Favorites and aliases also change from the palette. The event
            // leaves out what only this window may know: ask for the whole.
            event::SETTINGS_CHANGED => vec![ask(Command::GetSettings, Then::Settings)],
            event::INDEX_READY => pages::reload(),
            // How far each download is, the host keeps: asked for again.
            event::DICTATION_INSTALL if over => vec![ask(Command::DictationStatus, Then::Known), ask(Command::SpeechModels, Then::Known)],
            event::DICTATION_INSTALL => vec![ask(Command::SpeechModels, Then::Known)],
            event::OCR_INSTALL => {
                self.known.ocr_installing = !over;
                if over { vec![ask(Command::OcrStatus, Then::Known)] } else { Vec::new() }
            }
            event::CLIPBOARD_MEANING_INSTALL => {
                self.known.meaning_installing = !over;
                if over { vec![ask(Command::ClipboardMeaningStatus, Then::Known)] } else { Vec::new() }
            }
            _ => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests;
