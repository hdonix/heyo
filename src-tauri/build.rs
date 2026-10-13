// The design tokens of Heyo's interface, read from the web view's scales and
// written as a Slint global for the settings window to be drawn from.
#[path = "src/slint_windows/tokens.rs"]
mod tokens;

/// The translations: a directory for each language, in gettext's layout.
const LOCALES: &str = "../locales";

fn main() {
    locales();
    slint_windows();
    tauri_build::build()
}

/// Reads every language's `heyo.po` into the table `src/i18n.rs` finds a
/// text's translation in, written as `locales.rs`: the languages in the
/// order of their directories' names, and every message one of them
/// translates.
fn locales() {
    use std::collections::BTreeMap;
    use rspolib::TranslatedEntry;
    use std::fmt::Write;
    println!("cargo:rerun-if-changed={LOCALES}");
    let mut files: Vec<_> = std::fs::read_dir(LOCALES).unwrap_or_else(|e| panic!("{LOCALES}: {e}")).filter_map(|entry| Some(entry.ok()?.path().join("LC_MESSAGES/heyo.po"))).filter(|file| file.exists()).collect();
    files.sort();
    // Code, its own name for itself, and its plural rule.
    let mut languages: Vec<(String, String, String)> = Vec::new();
    // (context, English) → each language's forms.
    let mut messages: BTreeMap<(String, String), Vec<Vec<String>>> = BTreeMap::new();
    for (place, file) in files.iter().enumerate() {
        let po = rspolib::pofile(file.as_path()).unwrap_or_else(|e| panic!("{}: {e}", file.display()));
        let field = |name: &str| po.metadata.get(name).unwrap_or_else(|| panic!("{}: its header has no {name}", file.display())).clone();
        let rule = field("Plural-Forms").split(';').find_map(|part| part.trim().strip_prefix("plural=").map(|rule| rule.trim().to_string())).unwrap_or_else(|| panic!("{}: its Plural-Forms has no plural=", file.display()));
        languages.push((field("Language"), field("X-Language-Name"), rule));
        for entry in po.entries.iter().filter(|entry| entry.translated()) {
            let forms = if entry.msgid_plural.is_some() { entry.msgstr_plural.clone() } else { entry.msgstr.clone().into_iter().collect() };
            let key = (entry.msgctxt.clone().unwrap_or_default(), entry.msgid.clone());
            messages.entry(key).or_insert_with(|| vec![Vec::new(); files.len()])[place] = forms;
        }
    }
    let mut table = format!("const LANGUAGES: &[(&str, &str, &str)] = &{languages:?};\nconst MESSAGES: &[(&str, &str, &[&[&str]])] = &[\n");
    for ((context, text), forms) in &messages {
        let forms: Vec<String> = forms.iter().map(|forms| format!("&{forms:?}")).collect();
        writeln!(table, "    ({context:?}, {text:?}, &[{}]),", forms.join(", ")).expect("writing to a string");
    }
    table.push_str("];\n");
    let written = std::path::PathBuf::from(std::env::var_os("OUT_DIR").expect("cargo sets OUT_DIR")).join("locales.rs");
    std::fs::write(&written, table).unwrap_or_else(|e| panic!("{}: {e}", written.display()));
}

/// Compiles the settings window (`src/slint_windows/ui/`), with the
/// tokens beside it as `tokens.slint`.
fn slint_windows() {
    const SCALES: &str = "../src/styles/abstracts/_scales.scss";
    println!("cargo:rerun-if-changed={SCALES}");
    println!("cargo:rerun-if-changed=src/slint_windows/tokens.rs");
    let written = std::path::PathBuf::from(std::env::var_os("OUT_DIR").expect("cargo sets OUT_DIR")).join("slint-tokens");
    let scales = std::fs::read_to_string(SCALES).unwrap_or_else(|e| panic!("{SCALES}: {e}"));
    let global = tokens::parse(&scales).and_then(|scales| tokens::slint_global(&scales)).unwrap_or_else(|e| panic!("{SCALES}: {e}"));
    std::fs::create_dir_all(&written).and_then(|()| std::fs::write(written.join("tokens.slint"), global)).unwrap_or_else(|e| panic!("{}: {e}", written.display()));
    // The style is only that of Slint's `ListView`, whose scroll bar is not shown.
    // What the markup says (`@tr`) is translated from the same files as
    // the rest, which go into the program. The English alone is the key,
    // as it is everywhere else: not the name of the component it is in.
    let config = slint_build::CompilerConfiguration::new().with_include_paths(vec![written]).with_style("fluent".into()).with_bundled_translations(LOCALES).with_default_translation_context(slint_build::DefaultTranslationContext::None);
    slint_build::compile_with_config("src/slint_windows/ui/settings.slint", config).unwrap_or_else(|e| panic!("the Slint settings window: {e}"));
}
