//! Screenshots: their hotkeys, and the engine that reads text in them.

use super::{tile, Known, Page, Reaction, Sheet};
use crate::i18n::{marked, t};
use crate::slint_windows::answer::Answer;
use crate::slint_windows::host::Command;
use crate::slint_windows::ids::{Act, Id, PageId};
use crate::slint_windows::model::{ask, Ask, Entry, Line, Then};

pub(crate) const PAGE: &str = "screenshots";

pub(crate) struct Screenshots;

impl Page for Screenshots {
    fn entries(&self, _: &Known) -> Vec<Entry> {
        vec![tile(PAGE, marked("Screenshots"), "camera-16")]
    }

    fn shows(&self, at: &PageId) -> bool {
        *at == PageId::own(PAGE)
    }

    fn lines(&self, _: &PageId, known: &Known) -> Vec<Line> {
        let mut page = Sheet::default();
        page.note(t("Drag to select part of the screen, or click a window to select exactly that. Mark it up with rectangles, arrows or the pen, then copy it, save it to Pictures/Screenshots, pin it on top of other windows, or copy the text in it."));
        let hotkey = |id: &str, title: &str, description: &str| {
            let row = Line::row("", title).says(description);
            if known.hotkeys_supported() { row.hotkey(id, known.item_hotkeys.get(id).map(String::as_str)) } else { row }
        };
        page.card(t("Hotkeys"), vec![hotkey("heyo:screenshot", t("Capture Screenshot"), t("Enter copies, Ctrl+S saves, Esc cancels")), hotkey("heyo:screenshot-text", t("Capture Text from Screen"), t("Drag over text and it is copied"))]);
        let installed = known.ocr.installed;
        let engine = Line::row(Id::Act(Act::InstallOcr), t("Recognition Engine")).says(if installed { t("PaddleOCR is installed. Chinese and English; images never leave this computer.") } else { t("A one-time download of about 340 MB (PaddleOCR, Chinese and English).") });
        page.card(t("Text Recognition"), vec![if installed { engine.badge(t("Installed")) } else if known.ocr_installing || known.ocr.installing { engine.text("Downloading…") } else { engine.button(t("Download")) }]);
        page.done()
    }

    fn acted(&mut self, _: &PageId, known: &mut Known, id: &Id, _: &str) -> Option<Reaction> {
        match id {
            Id::Act(Act::InstallOcr) => {
                known.ocr_installing = true;
                // A download was started; an answer only if it could not be.
                Some(Reaction::asking(vec![ask(Command::OcrInstall, Then::Acted(id.clone()))]))
            }
            _ => None,
        }
    }

    fn answered(&mut self, known: &mut Known, id: &Id, answer: &Result<Answer, String>) -> Reaction {
        if *id == Id::Act(Act::InstallOcr) && answer.is_err() {
            known.ocr_installing = false;
        }
        Reaction::default()
    }

    fn entered(&self, _: &PageId, _: &Known) -> Vec<Ask> {
        vec![ask(Command::OcrStatus, Then::Known)]
    }
}
