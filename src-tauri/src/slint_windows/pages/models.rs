//! Models: the providers, each on a page of its own, adding one, and the
//! speech models of this computer; and the pickers other pages choose a
//! provider's model with.

use std::collections::BTreeMap;

use super::{tile, Known, Page, Reaction, Sheet, To};
use crate::i18n::{marked, t, tf, translate};
use crate::models::kinds::{self, Source};
use crate::models::registry::{self, Entry as Field, ModelRef, Provider, Use};
use crate::slint_windows::answer::Answer;
use crate::slint_windows::host::Command;
use crate::slint_windows::ids::{self, Act, Id, PageId};
use crate::slint_windows::model::{ask, Ask, Entry, Line, Then};

pub(crate) const PAGE: &str = "models";

/// Checking a provider's connection.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Check {
    Checking,
    /// What it came to, in words.
    Done(Result<String, String>),
}

#[derive(Debug, Default)]
pub(crate) struct Models {
    /// What checking each provider came to, by its id.
    pub checks: BTreeMap<String, Check>,
    /// A provider being added, until it is; and the ids there were before.
    pub draft: Option<Provider>,
    adding: Vec<String>,
}

// --- What every page reads of the providers ----------------------------------

/// The providers, as the host told them (their secrets taken out).
/// A secret that is set stands as something in its field, so that
/// what needs it is not taken for missing.
pub(crate) fn providers(known: &Known) -> Vec<Provider> {
    let mut providers = known.settings.settings.providers.clone();
    for provider in &mut providers {
        for name in known.settings.provider_states.get(&provider.id).map_or(&[][..], |state| state.secrets_set.as_slice()) {
            provider.config.insert(name.clone(), "(set)".into());
        }
    }
    providers
}

fn provider(known: &Known, id: &str) -> Option<Provider> {
    providers(known).into_iter().find(|provider| provider.id == id)
}

fn secret_set(known: &Known, id: &str, field: &str) -> bool {
    known.settings.provider_states.get(id).is_some_and(|state| state.secrets_set.iter().any(|name| name == field))
}

/// The models each service listed, of those that answered.
fn listed_models(known: &Known) -> BTreeMap<String, Vec<String>> {
    known.listed.iter().filter_map(|(id, models)| Some((id.clone(), models.as_ref().ok()?.clone()))).collect()
}

/// Whether the speech model `id` is downloaded; for none, the one in use.
fn speech_installed(known: &Known, id: &str) -> bool {
    known.speech.iter().any(|entry| (if id.is_empty() { entry.in_use } else { entry.id == id }) && entry.installed)
}

/// What is known of a provider besides its settings, as its kind's
/// status is said from.
fn known_of(known: &Known, provider: &Provider) -> kinds::Known {
    let found = known.settings.provider_states.get(&provider.id).and_then(|state| state.cli_found.clone()).map(|found| found.map(|found| (found.version, found.path)));
    let source = provider.kind().source();
    let listed = if source == Source::Listed && provider.unset().is_none() { known.listed.get(&provider.id).cloned() } else { None };
    let downloaded = (source == Source::Catalogue && !known.speech.is_empty()).then(|| known.speech.iter().filter(|entry| entry.installed).count());
    kinds::Known { found, listed, downloaded }
}

/// What keeps a provider from being used as it is now, as far as is
/// known; none if nothing does.
fn trouble(known: &Known, provider: &Provider) -> Option<String> {
    if let Some(missing) = provider.unset() {
        return Some(translate(missing).into());
    }
    let found = known_of(known, provider);
    match (found.found, found.listed) {
        (Some(Err(why)), _) => Some(why),
        (_, Some(Err(e))) => Some(tf("Not reachable: {0}.", &[&e])),
        _ => None,
    }
}

/// The chips of what a provider can do.
fn abilities(provider: &Provider) -> Vec<String> {
    kinds::ABILITIES.iter().filter(|(can, _)| provider.can(*can)).map(|(_, word)| translate(word).to_string()).collect()
}

/// A picker of the model `need` refers to: each model of each provider
/// that can be used for it, as "provider · model", and `off` first where
/// using none is a choice. What it refers to that cannot be used, or is
/// gone, is said as a warning.
pub(crate) fn picker(known: &Known, need: Use, label: &str, description: &str, off: Option<&str>) -> Line {
    let providers = providers(known);
    let reference = need.reference(&known.settings.settings);
    let mut choices: Vec<(String, String)> = off.map(|off| (String::new(), off.to_string())).into_iter().collect();
    choices.extend(registry::offered(&providers, need, &listed_models(known)).into_iter().map(|(offered, title)| (ids::choice(&offered.provider, &offered.model), title)));
    let value = if reference.is_none() { String::new() } else { ids::choice(&reference.provider, &reference.model) };
    let problem = match registry::resolve(&providers, reference, need) {
        Err(registry::Unusable::NoneChosen) if off.is_some() => None,
        Err(why) => Some(crate::models::ModelError::Unusable(why).message()),
        Ok((provider, model)) => trouble(known, provider).or_else(|| (provider.kind().source() == Source::Catalogue && !known.speech.is_empty() && !speech_installed(known, model)).then(|| tf("{0} is not downloaded yet. Download it in Models.", &[&provider.model_title(model)]))),
    };
    if !choices.iter().any(|(choice, _)| *choice == value) {
        let name = providers.iter().find(|provider| provider.id == reference.provider).map_or(reference.provider.clone(), |provider| provider.name.clone());
        let title = if reference.is_none() { t("None").to_string() } else { format!("{name} · {}", if reference.model.is_empty() { t("Default") } else { &reference.model }) };
        choices.push((value.clone(), title));
    }
    let said = match &problem {
        Some(problem) if !description.is_empty() => format!("{problem} {description}"),
        Some(problem) => problem.clone(),
        None => description.to_string(),
    };
    let mut line = Line::row(Id::Pick(need), label).says(said).warns(problem.is_some());
    (line.control, line.value, line.choices) = ("select", value, choices);
    line
}

/// A picker's choice: what `need` is to refer to.
pub(crate) fn pick(known: &mut Known, need: Use, value: &str) -> Vec<Ask> {
    let (provider, model) = ids::chosen(value).unwrap_or_default();
    known.change(|view| *need.reference_mut(&mut view.settings) = ModelRef::to(provider, model))
}

/// The row that goes to the Models page, under the pickers.
pub(crate) fn to_models() -> Line {
    Line::row(Id::Act(Act::OpenModels), t("Providers")).says(t("Connect Claude, servers such as Ollama, Claude Code, Codex and speech services, and download speech models.")).button(t("Open Models"))
}

/// What is asked to know the models of every service that lists them
/// and is set up.
pub(crate) fn listings(known: &Known) -> Vec<Ask> {
    providers(known).into_iter().filter(|provider| provider.kind().source() == Source::Listed && provider.unset().is_none()).map(|provider| ask(Command::ProviderModels(provider.id.clone()), Then::Models(provider.id))).collect()
}

// --- The pages ---------------------------------------------------------------

impl Models {
    /// The list of the providers, and the way to add one.
    fn list(known: &Known) -> Vec<Line> {
        let mut page = Sheet::default();
        page.note(t("Providers run the models Heyo's features use. Add one, then choose its models where each feature is set up: AI in Advanced, speech recognition and polishing in Dictation."));
        let rows = providers(known)
            .iter()
            .map(|provider| {
                let kind = provider.kind();
                let status = kind.status(provider, &known_of(known, provider));
                let title = translate(kind.title());
                // Its kind, where its name does not say it already.
                let said = if provider.name == title { status.short } else { format!("{title} · {}", status.short) };
                Line::row(Id::Go(PageId::Provider(provider.id.clone())), provider.name.clone()).says(said).warns(status.warning).pictured(Some(kind.icon().into())).chips(abilities(provider)).link()
            })
            .collect();
        page.card(t("Providers"), rows);
        page.card("", vec![Line::row(Id::Go(PageId::AddProvider), t("Add Provider…")).says(t("Several of one kind can be added, two servers for instance.")).pictured(Some("plus".into())).link()]);
        page.done()
    }

    /// Choosing the kind of a new provider.
    fn adding() -> Vec<Line> {
        let mut page = Sheet::default();
        page.top(PageId::own(PAGE), t("Models"), "plus", t("Add Provider").into(), t("Choose what to connect.").into());
        let rows = kinds::addable().map(|kind| Line::row(Id::NewProvider(kind.word().into()), translate(kind.title())).says(translate(kind.about())).pictured(Some(kind.icon().into())).chips(abilities(&Provider::new("", kind.word()))).link()).collect();
        page.card("", rows);
        page.done()
    }

    /// The fields of a provider, to edit: its name (unless it is always
    /// there) and its kind's fields that apply. A new one's (`draft`) are
    /// kept here until it is added.
    fn form(known: &Known, provider: &Provider, draft: bool) -> Vec<Line> {
        let id = |field: &str| if draft { Id::DraftField(field.into()) } else { Id::ProviderField { provider: provider.id.clone(), field: field.into() } };
        let kind = provider.kind();
        let mut rows = Vec::new();
        if !kind.builtin() {
            rows.push(Line::row(id("name"), t("Name")).field(provider.name.clone(), translate(kind.title())));
        }
        for field in kind.fields().iter().filter(|field| provider.applies(field)) {
            let row = Line::row(id(field.name), translate(field.title)).says(translate(field.description));
            let set = if draft { !provider.get(field.name).is_empty() } else { secret_set(known, &provider.id, field.name) };
            rows.push(match field.entry {
                Field::Secret if set && draft => row.secret(t("Typed — type again to replace it")),
                Field::Secret if set => row.secret(t("Saved — type a new key to replace it")).more(t("Remove"), Id::ForgetSecret { provider: provider.id.clone(), field: field.name.into() }),
                Field::Secret => row.secret(field.placeholder),
                Field::Choice(choices) => {
                    let chosen = provider.get(field.name);
                    row.select(if chosen.is_empty() { choices[0].0 } else { chosen }, choices.iter().map(|(value, title)| (*value, translate(title))))
                }
                Field::Switch => row.switch(provider.get(field.name) == "true"),
                _ => row.field(provider.config.get(field.name).cloned().unwrap_or_default(), field.placeholder),
            });
        }
        rows
    }

    /// A new provider's form, until it is added or left.
    fn new_one(&self, known: &Known) -> Vec<Line> {
        let Some(draft) = &self.draft else { return Self::adding() };
        let kind = draft.kind();
        let mut page = Sheet::default();
        page.top(PageId::AddProvider, t("Add Provider"), kind.icon(), tf("New {0}", &[&translate(kind.title())]), translate(kind.about()).into());
        page.card(t("Settings"), Self::form(known, draft, true));
        page.card("", vec![Line::row(Id::Act(Act::SaveProvider), t("Add This Provider")).says(t("Its models can then be chosen where each feature is set up. It can be changed or removed here later.")).button(t("Add")).more(t("Cancel"), Id::Go(PageId::own(PAGE)))]);
        page.done()
    }

    /// Which uses refer to `model` of `provider`, in a word each.
    fn used_for(known: &Known, provider: &Provider, model: &str) -> Vec<String> {
        const USES: &[(Use, &str)] = &[(Use::Answers, marked("AI")), (Use::DictationRecognition, marked("Dictation")), (Use::DictationPolish, marked("Polish")), (Use::ClipboardCases, marked("Clipboard Search"))];
        USES.iter().filter(|(need, _)| need.suits(provider) && *need.reference(&known.settings.settings) == ModelRef::to(&provider.id, model)).map(|(_, word)| translate(word).to_string()).collect()
    }

    /// A provider's models, as its kind has them: a catalogue to download
    /// from, what the service lists, or what is known of it. Each says what
    /// uses it.
    fn model_rows(known: &Known, provider: &Provider) -> Vec<Line> {
        let kind = provider.kind();
        if kind.source() == Source::Catalogue {
            return Self::speech_rows(known);
        }
        let mut models = provider.models();
        let mut rows = Vec::new();
        if kind.source() == Source::Listed {
            match known.listed.get(&provider.id) {
                Some(Ok(listed)) => models.extend(listed.iter().filter(|name| !models.contains(name)).cloned().collect::<Vec<_>>()),
                Some(Err(e)) => rows.push(Line::row("", t("Not reachable")).says(e.clone()).warns(true)),
                None if provider.unset().is_none() => rows.push(Line::row("", t("Checking…"))),
                None => {}
            }
        }
        rows.extend(models.iter().map(|model| {
            let title = provider.model_title(model);
            let row = Line::row("", title.clone()).says(if title == *model || model.is_empty() { String::new() } else { model.clone() });
            let used = Self::used_for(known, provider, model);
            if used.is_empty() { row } else { row.badge(&tf("Used for {0}", &[&used.join(", ")])) }
        }));
        rows
    }

    /// A provider's page: how it is (and checking it), its settings, what
    /// it can do, its models, and removing it.
    fn provider_page(&self, known: &Known, provider: &Provider) -> Vec<Line> {
        let kind = provider.kind();
        let mut page = Sheet::default();
        // Its kind, where its name does not say it already.
        let title = translate(kind.title());
        page.top(PageId::own(PAGE), t("Models"), kind.icon(), provider.name.clone(), if provider.name == title { String::new() } else { title.into() });
        let status = kind.status(provider, &known_of(known, provider));
        let checking = self.checks.get(&provider.id) == Some(&Check::Checking);
        let mut status = Line::row(Id::CheckProvider(provider.id.clone()), t("Status")).says(status.long).warns(status.warning).button(if checking { t("Checking…") } else { t("Check Connection") });
        status.busy = checking;
        let mut rows = vec![status];
        if let Some(Check::Done(result)) = self.checks.get(&provider.id) {
            let (said, failed) = match result {
                Ok(said) => (said.clone(), false),
                Err(why) => (why.clone(), true),
            };
            rows.push(Line::row("", if failed { t("Connection Failed") } else { t("Connection Works") }).says(said).warns(failed));
        }
        rows.push(Line::row("", t("What It Can Do")).says(translate(kind.goes(provider))).chips(abilities(provider)));
        page.card("", rows);
        page.card(t("Settings"), Self::form(known, provider, false));
        page.card(t("Models"), Self::model_rows(known, provider));
        if !kind.builtin() {
            page.card("", vec![Line::row(Id::RemoveProvider(provider.id.clone()), t("Remove Provider")).says(t("What uses its models says so until another is chosen.")).confirm(t("Remove…"), t("Click Again to Remove"))]);
        }
        page.done()
    }

    /// The speech models of this computer: each with what it hears, how
    /// much it is, and what can be done with it (download it, or call
    /// the download off; use it, or delete it).
    fn speech_rows(known: &Known) -> Vec<Line> {
        known
            .speech
            .iter()
            .map(|entry| {
                let id = entry.id.clone();
                let size = tf("{0} MB", &[&entry.megabytes]);
                let row = Line::row(Id::SpeechDownload(id.clone()), entry.name.clone()).says(tf("{0} · {1}. {2}", &[&entry.languages, &size, &entry.note]));
                if let Some(going) = &entry.downloading {
                    let percent = if going.total > 0 { format!(" {}%", (going.received as f64 / going.total as f64 * 100.0).round()) } else { "…".into() };
                    row.text(format!("{}{percent}", crate::helper::step(&going.step))).more(t("Cancel"), Id::SpeechCancel(id))
                } else if entry.installed && entry.in_use {
                    row.badge(t("In Use"))
                } else if entry.installed {
                    Line { id: Id::SpeechDelete(id.clone()).into(), ..row }.confirm(t("Delete…"), t("Click Again to Delete")).more(t("Use"), Id::SpeechUse(id))
                } else {
                    row.button(t("Download"))
                }
            })
            .collect()
    }
}

impl Page for Models {
    fn entries(&self, _: &Known) -> Vec<Entry> {
        vec![tile(PAGE, marked("Models"), "computer-chip-16")]
    }

    fn shows(&self, at: &PageId) -> bool {
        matches!(at, PageId::AddProvider | PageId::NewProvider | PageId::Provider(_)) || *at == PageId::own(PAGE)
    }

    fn lines(&self, at: &PageId, known: &Known) -> Vec<Line> {
        match at {
            PageId::AddProvider => Self::adding(),
            PageId::NewProvider => self.new_one(known),
            PageId::Provider(id) => provider(known, id).map_or_else(|| Self::list(known), |provider| self.provider_page(known, &provider)),
            _ => Self::list(known),
        }
    }

    fn acted(&mut self, _: &PageId, known: &mut Known, id: &Id, value: &str) -> Option<Reaction> {
        let asks = match id {
            // A provider has commands of its own: the window never has its
            // secrets to send back with the rest of the settings.
            Id::ProviderField { provider: id, field } => {
                let Some(found) = provider(known, id) else { return Some(Reaction::default()) };
                let secret = found.kind().fields().iter().any(|known| known.name == field && known.entry == Field::Secret);
                let was = if field == "name" { found.name.as_str() } else { found.get(field) };
                if (secret && value.trim().is_empty()) || (!secret && was == value.trim()) {
                    return Some(Reaction::default());
                }
                // What a check said may no longer be so.
                self.checks.remove(id);
                let mut asks = vec![ask(Command::SetProviderField { provider: id.clone(), field: field.clone(), value: value.to_string() }, Then::Settings)];
                // Where the service is, or what it asks for, changed: asked again.
                if found.kind().source() == Source::Listed && found.kind().fields().iter().any(|known| known.name == field && matches!(known.entry, Field::Url | Field::Secret)) {
                    known.listed.remove(id);
                    asks.push(ask(Command::ProviderModels(id.clone()), Then::Models(id.clone())));
                }
                asks
            }
            Id::ForgetSecret { provider, field } => {
                self.checks.remove(provider);
                vec![ask(Command::SetProviderField { provider: provider.clone(), field: field.clone(), value: String::new() }, Then::Settings)]
            }
            Id::RemoveProvider(provider) => {
                known.listed.remove(provider);
                self.checks.remove(provider);
                return Some(Reaction { asks: vec![ask(Command::RemoveProvider(provider.clone()), Then::Settings)], to: Some(To::Move(PageId::own(PAGE))), said: false });
            }
            Id::NewProvider(kind) => {
                self.draft = Some(Provider::new("", kind));
                return Some(Reaction::showing(PageId::NewProvider));
            }
            // A new one's fields are kept here until it is added.
            Id::DraftField(field) => {
                if let Some(draft) = &mut self.draft {
                    let value = value.trim();
                    match field.as_str() {
                        "name" if !value.is_empty() => draft.name = value.into(),
                        "name" => {}
                        _ if value.is_empty() => {
                            draft.config.remove(field);
                        }
                        _ => {
                            draft.config.insert(field.clone(), value.into());
                        }
                    }
                }
                Vec::new()
            }
            Id::Act(Act::SaveProvider) => {
                let Some(draft) = self.draft.clone() else { return Some(Reaction::default()) };
                self.adding = providers(known).into_iter().map(|provider| provider.id).collect();
                vec![ask(Command::AddProvider { kind: draft.kind, name: draft.name, config: draft.config }, Then::Acted(id.clone()))]
            }
            Id::CheckProvider(provider) => {
                if self.checks.get(provider) == Some(&Check::Checking) {
                    return Some(Reaction::default());
                }
                self.checks.insert(provider.clone(), Check::Checking);
                vec![ask(Command::CheckProvider(provider.clone()), Then::Acted(id.clone()))]
            }
            // The speech models of this computer: each a command of the host's,
            // and then what they are is asked again.
            Id::SpeechDownload(model) => vec![ask(Command::DownloadSpeechModel(model.clone()), Then::Known), ask(Command::SpeechModels, Then::Known)],
            Id::SpeechCancel(model) => vec![ask(Command::CancelSpeechDownload(model.clone()), Then::Known), ask(Command::SpeechModels, Then::Known)],
            Id::SpeechDelete(model) => vec![ask(Command::DeleteSpeechModel(model.clone()), Then::Known), ask(Command::SpeechModels, Then::Known)],
            Id::SpeechUse(model) => {
                let mut asks = known.change(|view| view.settings.dictation_model = ModelRef::to(registry::LOCAL_SPEECH, model));
                asks.push(ask(Command::SpeechModels, Then::Known));
                asks
            }
            _ => return None,
        };
        Some(Reaction::asking(asks))
    }

    fn answered(&mut self, known: &mut Known, id: &Id, answer: &Result<Answer, String>) -> Reaction {
        match (id, answer) {
            (Id::CheckProvider(provider), Ok(answer)) => {
                let checked = if let Answer::Checked(checked) = answer { checked.clone() } else { kinds::Checked::default() };
                if let Some(models) = checked.models {
                    known.listed.insert(provider.clone(), Ok(models));
                }
                self.checks.insert(provider.clone(), Check::Done(Ok(checked.said)));
            }
            // Said on the provider's page.
            (Id::CheckProvider(provider), Err(message)) => {
                self.checks.insert(provider.clone(), Check::Done(Err(message.clone())));
                return Reaction { said: true, ..Reaction::default() };
            }
            // Added: its own page, where it can be checked.
            (Id::Act(Act::SaveProvider), Ok(_)) => {
                self.draft = None;
                let added = providers(known).into_iter().find(|provider| !self.adding.contains(&provider.id));
                return Reaction::showing(added.map_or(PageId::own(PAGE), |provider| PageId::Provider(provider.id)));
            }
            _ => {}
        }
        Reaction::default()
    }

    fn entered(&self, at: &PageId, known: &Known) -> Vec<Ask> {
        match at {
            PageId::Own(_) => super::dictation::needed(known),
            _ => [vec![ask(Command::SpeechModels, Then::Known)], listings(known)].concat(),
        }
    }
}
