// The design tokens of Heyo's interface, read from the web view's scales and
// written as a Slint global for the settings window to be drawn from.
#[path = "src/slint_windows/tokens.rs"]
mod tokens;

fn main() {
        slint_windows();
    tauri_build::build()
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
    let config = slint_build::CompilerConfiguration::new().with_include_paths(vec![written]).with_style("fluent".into());
    slint_build::compile_with_config("src/slint_windows/ui/settings.slint", config).unwrap_or_else(|e| panic!("the Slint settings window: {e}"));
}
