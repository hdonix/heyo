// The design tokens of Heyo's interface, read from the web view's scales and
// written as a Slint global for the settings window to be drawn from.
#[path = "src/slint_windows/tokens.rs"]
mod tokens;

/// The translations: a directory for each language, in gettext's layout.
const LOCALES: &str = "../locales";

fn main() {
    locales();
    slint_windows();
    sherpa_onnx();
    tauri_build::build()
}

/// Checks the prebuilt sherpa-onnx (and ONNX Runtime) that the build script
/// of `sherpa-onnx-sys`, which runs before this one, fetched from the
/// release of its version: a release's file can be replaced under the same
/// address, and what is linked into Heyo must be what was chosen
/// (`sherpa-onnx.sha256`, the digests GitHub lists for the release's
/// files). Where the libraries were given (`SHERPA_ONNX_LIB_DIR`) nothing
/// was fetched.
fn sherpa_onnx() {
    use sha2::{Digest, Sha256};
    println!("cargo:rerun-if-changed=sherpa-onnx.sha256");
    println!("cargo:rerun-if-env-changed=SHERPA_ONNX_LIB_DIR");
    if std::env::var_os("SHERPA_ONNX_LIB_DIR").is_some() {
        return println!("cargo:warning=sherpa-onnx comes from SHERPA_ONNX_LIB_DIR, unchecked");
    }
    let var = |name: &str| std::env::var(name).unwrap_or_default();
    // Named as the `-sys` crate names them: linked in, but on Windows in
    // DLLs (Cargo.toml says why).
    let system = match (var("CARGO_CFG_TARGET_OS").as_str(), var("CARGO_CFG_TARGET_ARCH").as_str()) {
        ("linux", "x86_64") => "linux-x64-static-lib",
        ("linux", "aarch64") => "linux-aarch64-static-lib",
        ("macos", "x86_64") => "osx-x64-static-lib",
        ("macos", "aarch64") => "osx-arm64-static-lib",
        ("windows", "x86_64") => "win-x64-shared-MT-Release-lib",
        ("windows", "aarch64") => "win-arm64-shared-MT-Release-lib",
        (os, arch) => panic!("no prebuilt sherpa-onnx is pinned for {os} on {arch}"),
    };
    let pins = std::fs::read_to_string("sherpa-onnx.sha256").expect("sherpa-onnx.sha256");
    let (expected, name) = pins
        .lines()
        .filter_map(|line| line.split_once("  "))
        .find(|(_, name)| name.contains(&format!("-{system}.tar")))
        .unwrap_or_else(|| panic!("sherpa-onnx.sha256 pins nothing for {system}"));
    // Where the `-sys` crate keeps what it fetched: in the target directory.
    let out = std::path::PathBuf::from(var("OUT_DIR"));
    let target = std::env::var_os("CARGO_TARGET_DIR").map(std::path::PathBuf::from).or_else(|| out.ancestors().find(|dir| dir.file_name().is_some_and(|name| name == "target")).map(Into::into)).unwrap_or(out);
    let archive = target.join("sherpa-onnx-prebuilt").join(name);
    println!("cargo:rerun-if-changed={}", archive.display());
    // The `-sys` crate unpacks the archive beside it and, finding it
    // unpacked, fetches nothing again. A cache that keeps the unpacked
    // libraries and not the archive (CI's) leaves nothing to check: they
    // were checked when they were fetched, by the build that unpacked them.
    let unpacked = archive.with_file_name(name.trim_end_matches(".tar.bz2")).join("lib");
    if !archive.is_file() && unpacked.is_dir() {
        return println!("cargo:warning=sherpa-onnx comes from {} as an earlier build unpacked it; its archive is gone, unchecked", unpacked.display());
    }
    let bytes = std::fs::read(&archive).unwrap_or_else(|e| panic!("{}: {e} (fetched by sherpa-onnx-sys's build script, which should have run first)", archive.display()));
    let got: String = Sha256::digest(&bytes).iter().map(|byte| format!("{byte:02x}")).collect();
    if got != expected {
        panic!("{} is not the pinned file (SHA-256 {got}, expected {expected}): remove it and build again, or find out who changed it", archive.display());
    }
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
