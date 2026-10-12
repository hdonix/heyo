//! The design tokens of the web interface, for a window that is not drawn
//! by it.
//!
//! `src/styles/abstracts/_scales.scss` is where a colour, a size or a
//! duration is written, once. The web interface has them from there as
//! custom properties (`src/styles/base/_tokens.scss`); this reads the same
//! file and writes them as a Slint global, `Tokens`, when Heyo is built
//! (`build.rs`). Nothing is copied by hand, so nothing can drift: a token
//! the Slint side uses and the scales no longer have fails the build.
//!
//! This file is compiled twice: into the build script, which calls
//! [`slint_global`], and into the crate, for its tests.

use std::collections::BTreeMap;

/// One Sass map: its entries in the order written, each a name and the
/// values given for it (one, or the items of a list in parentheses).
pub type Map = Vec<(String, Vec<String>)>;

/// What `_scales.scss` declares: its maps and its plain variables.
#[derive(Debug, Default, PartialEq)]
pub struct Scales {
    pub maps: BTreeMap<String, Map>,
    pub variables: BTreeMap<String, String>,
}

/// The text without its `//` comments.
fn uncommented(source: &str) -> String {
    source.lines().map(|line| line.split("//").next().unwrap_or("")).collect::<Vec<_>>().join("\n")
}

/// Splits at the commas that are not inside parentheses.
fn items(list: &str) -> Vec<String> {
    let (mut found, mut depth, mut item) = (Vec::new(), 0, String::new());
    for c in list.chars() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                found.push(std::mem::take(&mut item));
                continue;
            }
            _ => {}
        }
        item.push(c);
    }
    found.push(item);
    found.into_iter().map(|item| item.trim().to_string()).filter(|item| !item.is_empty()).collect()
}

/// Reads the maps (`$name: ( … );`) and variables (`$name: value;`) of a
/// Sass file that is written as `_scales.scss` is: no nesting of maps, no
/// expressions.
pub fn parse(source: &str) -> Result<Scales, String> {
    let source = uncommented(source);
    let mut scales = Scales::default();
    let mut from = 0;
    while let Some(found) = source[from..].find('$') {
        let at = from + found;
        from = at + 1;
        // Only a `$` that begins its line declares: `$unit` inside a
        // function is a use.
        let line_start = source[..at].rfind('\n').map_or(0, |n| n + 1);
        if !source[line_start..at].trim().is_empty() {
            continue;
        }
        let Some(colon) = source[from..].find(':').map(|n| from + n) else { break };
        let name = source[from..colon].trim().to_string();
        if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
            continue;
        }
        let value_at = colon + 1 + (source[colon + 1..].len() - source[colon + 1..].trim_start().len());
        if source[value_at..].starts_with('(') {
            let body = &source[value_at + 1..];
            let end = closing(body).ok_or_else(|| format!("${name}: its map is not closed"))?;
            let mut map = Map::new();
            for entry in items(&body[..end]) {
                let (key, values) = entry.split_once(':').ok_or_else(|| format!("${name}: `{entry}` is not `name: value`"))?;
                let values = values.trim();
                let values = match values.strip_prefix('(').and_then(|inner| inner.strip_suffix(')')) {
                    Some(inner) => items(inner),
                    None => vec![values.to_string()],
                };
                map.push((key.trim().trim_matches('"').to_string(), values));
            }
            scales.maps.insert(name, map);
            from = value_at + 1 + end;
        } else {
            let end = source[value_at..].find(';').ok_or_else(|| format!("${name}: no `;` after its value"))?;
            scales.variables.insert(name, source[value_at..value_at + end].trim().to_string());
            from = value_at + end;
        }
    }
    Ok(scales)
}

/// Where the parenthesis that was opened just before `body` closes.
fn closing(body: &str) -> Option<usize> {
    let mut depth = 1;
    for (at, c) in body.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(at);
                }
            }
            _ => {}
        }
    }
    None
}

/// A CSS colour as Slint writes one: `#rrggbbaa`.
pub fn colour(css: &str) -> Result<String, String> {
    let css = css.trim();
    if let Some(hex) = css.strip_prefix('#') {
        let wide: String = match hex.len() {
            3 => hex.chars().flat_map(|c| [c, c]).collect(),
            6 | 8 => hex.to_string(),
            _ => return Err(format!("{css}: not a colour")),
        };
        if !wide.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(format!("{css}: not a colour"));
        }
        return Ok(format!("#{}{}", wide.to_lowercase(), if wide.len() == 6 { "ff" } else { "" }));
    }
    let inner = css.strip_prefix("rgba(").or_else(|| css.strip_prefix("rgb(")).and_then(|rest| rest.strip_suffix(')')).ok_or_else(|| format!("{css}: not a colour"))?;
    let parts: Vec<f64> = inner.split(',').map(|part| part.trim().parse::<f64>().map_err(|_| format!("{css}: not a colour"))).collect::<Result<_, _>>()?;
    let (channels, alpha) = match parts.as_slice() {
        [r, g, b] => ([*r, *g, *b], 1.0),
        [r, g, b, a] => ([*r, *g, *b], *a),
        _ => return Err(format!("{css}: not a colour")),
    };
    let byte = |value: f64| value.round().clamp(0.0, 255.0) as u8;
    Ok(format!("#{:02x}{:02x}{:02x}{:02x}", byte(channels[0]), byte(channels[1]), byte(channels[2]), byte(alpha * 255.0)))
}

/// A length in `px`, as a number.
fn pixels(css: &str) -> Result<f64, String> {
    css.trim().strip_suffix("px").and_then(|n| n.parse().ok()).ok_or_else(|| format!("{css}: not a length in px"))
}

/// The maps of pairs (light, dark), and what their properties are named
/// before the entry's own name: as the custom properties of the web
/// interface are, without `--` (and without `color-`, which Slint's type
/// says already).
const PAIRS: &[(&str, &str)] = &[("colors", ""), ("fills", "fill-"), ("shadows", "shadow-"), ("raycast", "raycast-")];

/// The maps of lengths, and what their properties are named before the
/// entry's own name.
const LENGTHS: &[(&str, &str)] = &[("radii", "radius-"), ("heights", "height-"), ("icons", "icon-")];

/// The scales as the source of a Slint global named `Tokens`. Colours
/// follow its `dark` property, which the window sets.
pub fn slint_global(scales: &Scales) -> Result<String, String> {
    let map = |name: &str| scales.maps.get(name).ok_or_else(|| format!("${name} is not in the scales"));
    let mut out = String::from("// Written by build.rs from src/styles/abstracts/_scales.scss: change it there.\n\nexport global Tokens {\n    in-out property <bool> dark;\n");
    for (name, prefix) in PAIRS {
        out.push('\n');
        for (key, values) in map(name)? {
            let [light, dark] = values.as_slice() else { return Err(format!("${name}: {key} is not a pair")) };
            out.push_str(&format!("    out property <color> {prefix}{key}: self.dark ? {} : {};\n", colour(dark)?, colour(light)?));
        }
    }
    out.push('\n');
    for (key, values) in map("type")? {
        let [size, line, tracking] = values.as_slice() else { return Err(format!("$type: {key} is not (size, line height, tracking)")) };
        // Tracking is written in em, which Slint has not: here it is the
        // length it comes to at this size.
        let em: f64 = tracking.strip_suffix("em").unwrap_or(tracking).parse().map_err(|_| format!("$type: {key}: {tracking} is not a tracking"))?;
        out.push_str(&format!("    out property <length> text-{key}-size: {}px;\n", pixels(size)?));
        out.push_str(&format!("    out property <length> text-{key}-line: {}px;\n", pixels(line)?));
        out.push_str(&format!("    out property <length> text-{key}-tracking: {}px;\n", (em * pixels(size)? * 10000.0).round() / 10000.0));
    }
    out.push('\n');
    for (key, values) in map("weights")? {
        let weight: u16 = values[0].parse().map_err(|_| format!("$weights: {key} is not a number"))?;
        out.push_str(&format!("    out property <int> weight-{key}: {weight};\n"));
    }
    for (name, prefix) in LENGTHS {
        out.push('\n');
        for (key, values) in map(name)? {
            out.push_str(&format!("    out property <length> {prefix}{key}: {}px;\n", pixels(&values[0])?));
        }
    }
    out.push('\n');
    for (key, values) in map("durations")? {
        let ms = values[0].strip_suffix("ms").and_then(|n| n.parse::<u32>().ok()).ok_or_else(|| format!("$durations: {key} is not in ms"))?;
        out.push_str(&format!("    out property <duration> duration-{key}: {ms}ms;\n"));
    }
    out.push('\n');
    for (key, values) in map("easings")? {
        if !values[0].starts_with("cubic-bezier(") {
            return Err(format!("$easings: {key} is not a cubic-bezier"));
        }
        out.push_str(&format!("    out property <easing> ease-{key}: {};\n", values[0]));
    }
    out.push('\n');
    for name in ["unit", "window-margin"] {
        let value = scales.variables.get(name).ok_or_else(|| format!("${name} is not in the scales"))?;
        out.push_str(&format!("    out property <length> {name}: {}px;\n", pixels(value)?));
    }
    out.push_str("}\n");
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCALES: &str = include_str!("../../../src/styles/abstracts/_scales.scss");

    #[test]
    fn reads_maps_pairs_and_variables() {
        let scales = parse("// A comment, with (a parenthesis.\n$colors: (\n  \"text\": (#1a1918, #f4f3f2),\n  // between\n  \"bg\": (rgba(245, 242, 241, 0.985), rgba(30, 28, 28, 0.975)),\n);\n$unit: 4px;\n@function space($steps) {\n  @return $steps * $unit;\n}\n$radii: (\n  \"md\": 8px,\n);\n").unwrap();
        assert_eq!(scales.maps["colors"], vec![("text".to_string(), vec!["#1a1918".to_string(), "#f4f3f2".to_string()]), ("bg".to_string(), vec!["rgba(245, 242, 241, 0.985)".to_string(), "rgba(30, 28, 28, 0.975)".to_string()])]);
        assert_eq!(scales.maps["radii"], vec![("md".to_string(), vec!["8px".to_string()])]);
        assert_eq!(scales.variables["unit"], "4px");
        assert_eq!(scales.variables.len(), 1, "a use of a variable was read as its declaration");
    }

    #[test]
    fn writes_colours_as_slint_does() {
        assert_eq!(colour("#000").unwrap(), "#000000ff");
        assert_eq!(colour("#D9471F").unwrap(), "#d9471fff");
        assert_eq!(colour("rgba(0, 0, 0, 0.08)").unwrap(), "#00000014");
        assert_eq!(colour("rgba(245, 242, 241, 0.985)").unwrap(), "#f5f2f1fb");
        assert!(colour("red").is_err());
    }

    /// The scales as they are in the repository are read whole, and what
    /// the window is drawn with is among them.
    #[test]
    fn the_scales_of_the_web_interface_become_the_global() {
        let scales = parse(SCALES).unwrap();
        for map in ["colors", "fills", "shadows", "raycast", "type", "weights", "radii", "heights", "icons", "durations", "easings"] {
            assert!(scales.maps.contains_key(map), "${map} was not read");
        }
        let global = slint_global(&scales).unwrap();
        for line in [
            "out property <color> text: self.dark ? #f4f3f2ff : #1a1918ff;",
            "out property <color> accent: self.dark ? #ff6b3dff : #d9471fff;",
            "out property <color> fill-2: self.dark ? #fffaf511 : #1e140f0e;",
            "out property <color> raycast-blue: self.dark ? #56c2ffff : #0a7fd6ff;",
            "out property <length> text-row-size: 15px;",
            "out property <length> text-row-line: 20px;",
            "out property <length> text-row-tracking: -0.135px;",
            "out property <int> weight-semibold: 600;",
            "out property <length> radius-xl: 16px;",
            "out property <length> height-lg: 32px;",
            "out property <duration> duration-base: 160ms;",
            "out property <easing> ease-out: cubic-bezier(0.2, 0, 0, 1);",
            "out property <length> window-margin: 32px;",
        ] {
            assert!(global.contains(line), "the global lacks `{line}`:\n{global}");
        }
    }

    /// Every entry of the scales is in the global: none is left behind
    /// because its map was not known.
    #[test]
    fn every_map_of_the_scales_is_written() {
        let scales = parse(SCALES).unwrap();
        let global = slint_global(&scales).unwrap();
        let written = global.lines().filter(|line| line.starts_with("    out property")).count();
        let entries: usize = scales.maps.iter().map(|(name, map)| if name == "type" { 3 * map.len() } else { map.len() }).sum();
        assert_eq!(written, entries + 2, "a map of the scales is not written into the global (or one is written twice)");
    }
}
