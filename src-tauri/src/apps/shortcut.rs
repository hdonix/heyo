//! What Windows' lists of applications say, read without asking Windows:
//! the Start menu's shortcut files (`.lnk`, a binary format Microsoft
//! documents as MS-SHLLINK) and the list of Store applications as
//! PowerShell prints it. Kept apart from the discovery itself so that it
//! is compiled, and tested, wherever Heyo is built.

/// What a shortcut says about the application it starts.
#[derive(Debug, Default, PartialEq)]
pub struct Shortcut {
    /// The file it opens, when it names one. (Some name only an entry in
    /// the installer's database, or a place that is not a file.)
    pub target: Option<String>,
    pub arguments: Option<String>,
    pub description: Option<String>,
    /// The file its icon is in, when not the target itself.
    pub icon_file: Option<String>,
    /// Which of that file's icons: its place, or (negative) its resource number.
    pub icon_index: i32,
}

/// The bytes of a file, read forward; every read says if there were too few.
struct Bytes<'a> {
    all: &'a [u8],
    at: usize,
}

impl<'a> Bytes<'a> {
    fn take(&mut self, length: usize) -> Option<&'a [u8]> {
        let piece = self.all.get(self.at..self.at.checked_add(length)?)?;
        self.at += length;
        Some(piece)
    }

    fn u16(&mut self) -> Option<u16> {
        self.take(2).map(|b| u16::from_le_bytes([b[0], b[1]]))
    }

    fn u32(&mut self) -> Option<u32> {
        self.take(4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
}

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    bytes.get(at..at.checked_add(4)?).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn utf16(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes.chunks_exact(2).map(|pair| u16::from_le_bytes([pair[0], pair[1]])).take_while(|unit| *unit != 0).collect();
    String::from_utf16_lossy(&units)
}

/// Text in the system's old one-byte encoding. Which one that was is not
/// recorded; anything beyond ASCII is read as Latin-1, and shortcuts with
/// such names carry the name in UTF-16 as well, which is preferred.
fn ansi(bytes: &[u8]) -> String {
    bytes.iter().take_while(|byte| **byte != 0).map(|byte| *byte as char).collect()
}

/// The path in a shortcut's "link info": a base on a local drive and
/// sometimes a remainder.
fn local_path(info: &[u8]) -> Option<String> {
    const HAS_LOCAL_PATH: u32 = 1;
    let (header, flags) = (u32_at(info, 4)? as usize, u32_at(info, 8)?);
    if flags & HAS_LOCAL_PATH == 0 {
        return None;
    }
    let text = |offset: Option<u32>, wide: bool| {
        let offset = offset.filter(|offset| *offset != 0)? as usize;
        info.get(offset..).map(|bytes| if wide { utf16(bytes) } else { ansi(bytes) })
    };
    // Newer shortcuts say both again in UTF-16.
    let (base, rest) = if header >= 0x24 && u32_at(info, 28).is_some_and(|offset| offset != 0) {
        (text(u32_at(info, 28), true)?, text(u32_at(info, 32), true).unwrap_or_default())
    } else {
        (text(u32_at(info, 16), false)?, text(u32_at(info, 24), false).unwrap_or_default())
    };
    Some(format!("{base}{rest}")).filter(|path| !path.is_empty())
}

/// Reads a shortcut file. `None`: it is not one, or is cut short.
pub fn parse(file: &[u8]) -> Option<Shortcut> {
    const SIGNATURE: [u8; 20] = [0x4C, 0, 0, 0, 0x01, 0x14, 0x02, 0, 0, 0, 0, 0, 0xC0, 0, 0, 0, 0, 0, 0, 0x46];
    const HAS_ID_LIST: u32 = 0x01;
    const HAS_LINK_INFO: u32 = 0x02;
    const HAS_NAME: u32 = 0x04;
    const HAS_RELATIVE_PATH: u32 = 0x08;
    const HAS_WORKING_DIR: u32 = 0x10;
    const HAS_ARGUMENTS: u32 = 0x20;
    const HAS_ICON_LOCATION: u32 = 0x40;
    const IS_UNICODE: u32 = 0x80;
    // Blocks after the strings: paths with %VARIABLES% in them, in a
    // fixed-size one-byte text followed by a fixed-size UTF-16 one.
    const VARIABLE_TARGET: u32 = 0xA000_0001;
    const VARIABLE_ICON: u32 = 0xA000_0007;
    const ONE_BYTE_TEXT: usize = 260;

    if file.get(..20)? != SIGNATURE {
        return None;
    }
    let flags = u32_at(file, 20)?;
    let mut shortcut = Shortcut { icon_index: u32_at(file, 56)? as i32, ..Shortcut::default() };
    let mut bytes = Bytes { all: file, at: 76 };
    if flags & HAS_ID_LIST != 0 {
        let length = bytes.u16()? as usize;
        bytes.take(length)?;
    }
    if flags & HAS_LINK_INFO != 0 {
        let length = u32_at(file, bytes.at)? as usize;
        shortcut.target = local_path(bytes.take(length)?);
    }
    let mut icon_file = None;
    for part in [HAS_NAME, HAS_RELATIVE_PATH, HAS_WORKING_DIR, HAS_ARGUMENTS, HAS_ICON_LOCATION] {
        if flags & part == 0 {
            continue;
        }
        let characters = bytes.u16()? as usize;
        let text = if flags & IS_UNICODE != 0 { utf16(bytes.take(characters * 2)?) } else { ansi(bytes.take(characters)?) };
        let text = Some(text).filter(|text| !text.is_empty());
        match part {
            HAS_NAME => shortcut.description = text,
            HAS_ARGUMENTS => shortcut.arguments = text,
            HAS_ICON_LOCATION => icon_file = text,
            _ => {}
        }
    }
    // The blocks are optional, and so is their being whole.
    let (mut variable_target, mut variable_icon) = (None, None);
    while let Some(length) = bytes.u32().map(|length| length as usize) {
        if length < 8 {
            break;
        }
        let Some(kind) = bytes.u32() else { break };
        let Some(block) = bytes.take(length - 8) else { break };
        let path = block.get(ONE_BYTE_TEXT..).map(utf16).filter(|path| !path.is_empty()).or_else(|| Some(ansi(block)).filter(|path| !path.is_empty()));
        match kind {
            VARIABLE_TARGET => variable_target = path,
            VARIABLE_ICON => variable_icon = path,
            _ => {}
        }
    }
    shortcut.target = shortcut.target.or(variable_target);
    shortcut.icon_file = variable_icon.or(icon_file);
    Some(shortcut)
}

/// Puts the values of `%VARIABLES%` into a path. What is not a known
/// variable stays as written.
pub fn expand(path: &str, value: impl Fn(&str) -> Option<String>) -> String {
    let mut out = String::new();
    let mut rest = path;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('%').and_then(|end| value(&after[..end]).map(|value| (end, value))) {
            Some((end, value)) => {
                out.push_str(&value);
                rest = &after[end + 1..];
            }
            None => {
                out.push('%');
                rest = after;
            }
        }
    }
    out + rest
}

/// Whether a Start menu entry is something installers add beside the
/// program itself.
pub fn beside_the_program(name: &str) -> bool {
    let name = name.to_lowercase();
    ["uninstall", "readme", "release notes", "license", "documentation", "website", "help"].iter().any(|word| name.contains(word))
}

/// The program whose windows are the application's: by its file's name,
/// lowercase and without ".exe", as windows are told apart on Windows.
/// `None` where the shortcut starts something whose windows could be
/// anyone's (a script host, a console, a browser showing a web page as an
/// application) or belong to a program it only passes through.
pub fn window_program(target: &str, arguments: Option<&str>) -> Option<String> {
    const ANYONES: &[&str] = &[
        "cmd", "powershell", "pwsh", "wt", "conhost", "explorer", "rundll32", "mmc", "msiexec", "control", "wscript", "cscript", "mshta", "java", "javaw",
        "python", "pythonw", "node", "chrome_proxy", "msedge_proxy", "brave_proxy",
    ];
    let name = target.rsplit(['\\', '/']).next().unwrap_or_default().to_lowercase();
    let program = name.strip_suffix(".exe")?;
    // Applications that update themselves are started through their
    // updater: `Update.exe --processStart "Slack.exe"`.
    if program == "update" {
        let named = arguments?.split("--processStart").nth(1)?.trim().trim_start_matches('=').trim();
        let named = named.strip_prefix('"').and_then(|rest| rest.split('"').next()).or_else(|| named.split_whitespace().next())?;
        return window_program(named, None).filter(|program| program != "update");
    }
    (!program.is_empty() && !ANYONES.contains(&program)).then(|| program.to_owned())
}

/// The Store's (and other packaged) applications from what PowerShell's
/// `Get-StartApps | ConvertTo-Json` prints: name and id. Its other entries
/// are the Start menu's shortcuts over again, which are read directly.
pub fn packaged(json: &str) -> Vec<(String, String)> {
    let listed = match serde_json::from_str::<serde_json::Value>(json.trim().trim_start_matches('\u{feff}')) {
        Ok(serde_json::Value::Array(listed)) => listed,
        // A list of one is printed as the one.
        Ok(one @ serde_json::Value::Object(_)) => vec![one],
        _ => return Vec::new(),
    };
    listed
        .iter()
        .filter_map(|entry| Some((entry.get("Name")?.as_str()?.trim().to_owned(), entry.get("AppID")?.as_str()?.to_owned())))
        // `Family!Application`: only packaged applications are named so.
        .filter(|(name, id)| !name.is_empty() && id.contains('!') && !id.contains(['\\', '/', '"', '\n', '\r']))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wide(text: &str) -> Vec<u8> {
        text.encode_utf16().flat_map(u16::to_le_bytes).collect()
    }

    /// A shortcut file with these parts, as Windows writes them.
    fn file(flags: u32, icon_index: i32, link_info: Option<Vec<u8>>, strings: &[&str], blocks: &[(u32, &str)]) -> Vec<u8> {
        let mut out = vec![0x4C, 0, 0, 0, 0x01, 0x14, 0x02, 0, 0, 0, 0, 0, 0xC0, 0, 0, 0, 0, 0, 0, 0x46];
        out.extend(flags.to_le_bytes());
        out.resize(56, 0);
        out.extend(icon_index.to_le_bytes());
        out.resize(76, 0);
        if flags & 0x01 != 0 {
            // An item list of ten bytes that says nothing readable.
            out.extend(10u16.to_le_bytes());
            out.extend([0xAA; 10]);
        }
        out.extend(link_info.unwrap_or_default());
        for text in strings {
            out.extend((text.encode_utf16().count() as u16).to_le_bytes());
            out.extend(if flags & 0x80 != 0 { wide(text) } else { text.bytes().collect() });
        }
        for (kind, path) in blocks {
            out.extend(0x314u32.to_le_bytes());
            out.extend(kind.to_le_bytes());
            let mut one_byte: Vec<u8> = path.bytes().collect();
            one_byte.resize(260, 0);
            let mut two_byte = wide(path);
            two_byte.resize(520, 0);
            out.extend(one_byte);
            out.extend(two_byte);
        }
        out.extend(0u32.to_le_bytes());
        out
    }

    /// Link info naming a local path, with or without the UTF-16 copies.
    fn link_info(base: &str, rest: &str, unicode: bool) -> Vec<u8> {
        let header = if unicode { 0x24u32 } else { 0x1C };
        let mut body: Vec<u8> = Vec::new();
        let mut place = |bytes: Vec<u8>| {
            let at = header + body.len() as u32;
            body.extend(bytes);
            at
        };
        let base_at = place(base.bytes().chain([0]).collect());
        let rest_at = place(rest.bytes().chain([0]).collect());
        let wide_base_at = if unicode { place(wide(&format!("{base}\0"))) } else { 0 };
        let wide_rest_at = if unicode { place(wide(&format!("{rest}\0"))) } else { 0 };
        let mut out: Vec<u8> = Vec::new();
        for value in [header + body.len() as u32, header, 1, 0, base_at, 0, rest_at] {
            out.extend(value.to_le_bytes());
        }
        if unicode {
            out.extend(wide_base_at.to_le_bytes());
            out.extend(wide_rest_at.to_le_bytes());
        }
        out.extend(body);
        out
    }

    #[test]
    fn a_shortcut_names_its_program_arguments_and_icon() {
        let target = r"C:\Program Files\Mozilla Firefox\firefox.exe";
        // Item list, link info, description, relative path, working directory, arguments, icon; UTF-16.
        let bytes = file(
            0xFF,
            -3,
            Some(link_info(target, "", false)),
            &["Browse the web", r"..\..\firefox.exe", r"C:\Program Files\Mozilla Firefox", "-private-window", r"%ProgramFiles%\Mozilla Firefox\icons.dll"],
            &[],
        );
        let shortcut = parse(&bytes).unwrap();
        assert_eq!(shortcut.target.as_deref(), Some(target));
        assert_eq!(shortcut.arguments.as_deref(), Some("-private-window"));
        assert_eq!(shortcut.description.as_deref(), Some("Browse the web"));
        assert_eq!(shortcut.icon_file.as_deref(), Some(r"%ProgramFiles%\Mozilla Firefox\icons.dll"));
        assert_eq!(shortcut.icon_index, -3);
    }

    #[test]
    fn the_utf16_path_is_preferred_and_the_remainder_added() {
        let bytes = file(0x02, 0, Some(link_info(r"C:\Users\Zoë\", r"Apps\editor.exe", true)), &[], &[]);
        assert_eq!(parse(&bytes).unwrap().target.as_deref(), Some(r"C:\Users\Zoë\Apps\editor.exe"));
        // One-byte strings, and no link info at all.
        let bytes = file(0x04, 0, None, &["Notes"], &[]);
        assert_eq!(parse(&bytes).unwrap(), Shortcut { description: Some("Notes".into()), ..Shortcut::default() });
    }

    #[test]
    fn a_path_with_variables_stands_in_where_no_other_is_given() {
        let bytes = file(0x80 | 0x40, 0, None, &["shell32.dll"], &[(0xA000_0001, r"%windir%\system32\notepad.exe"), (0xA000_0007, r"%windir%\system32\imageres.dll")]);
        let shortcut = parse(&bytes).unwrap();
        assert_eq!(shortcut.target.as_deref(), Some(r"%windir%\system32\notepad.exe"));
        assert_eq!(shortcut.icon_file.as_deref(), Some(r"%windir%\system32\imageres.dll"));
        let windows = |name: &str| name.eq_ignore_ascii_case("WINDIR").then(|| r"C:\Windows".to_owned());
        assert_eq!(expand(shortcut.target.as_deref().unwrap(), windows), r"C:\Windows\system32\notepad.exe");
        assert_eq!(expand("100% of %NOTHING% and %windir%", windows), r"100% of %NOTHING% and C:\Windows");
        assert_eq!(expand("plain", windows), "plain");
    }

    #[test]
    fn what_is_not_a_whole_shortcut_is_refused_or_read_as_far_as_it_goes() {
        assert_eq!(parse(b"not a shortcut at all, though long enough to hold the header of one, which is 76 bytes"), None);
        assert_eq!(parse(&[]), None);
        let whole = file(0xFF, 0, Some(link_info(r"C:\a.exe", "", false)), &["a", "b", "c", "d", "e"], &[(0xA000_0001, "x")]);
        // Cut anywhere, it never panics; cut inside what must be there, it is refused.
        for length in 0..whole.len() {
            let _ = parse(&whole[..length]);
        }
        assert_eq!(parse(&whole[..90]), None);
        // Sizes that point beyond the file.
        let mut lying = file(0x01, 0, None, &[], &[]);
        lying[76..78].copy_from_slice(&u16::MAX.to_le_bytes());
        assert_eq!(parse(&lying), None);
    }

    #[test]
    fn installers_extras_are_told_from_programs() {
        assert!(beside_the_program("Uninstall Foo"));
        assert!(beside_the_program("Foo Help"));
        assert!(!beside_the_program("Firefox"));
    }

    #[test]
    fn an_applications_windows_are_its_programs() {
        assert_eq!(window_program(r"C:\Program Files\Mozilla Firefox\firefox.exe", None).as_deref(), Some("firefox"));
        assert_eq!(window_program(r"C:\Program Files\Microsoft VS Code\Code.exe", Some("--new-window")).as_deref(), Some("code"));
        // Started through its updater.
        let slack = r"C:\Users\me\AppData\Local\slack\Update.exe";
        assert_eq!(window_program(slack, Some(r#"--processStart "slack.exe""#)).as_deref(), Some("slack"));
        assert_eq!(window_program(slack, Some("--processStart Discord.exe --process-start-args x")).as_deref(), Some("discord"));
        assert_eq!(window_program(slack, None), None);
        // A console, a web page in a browser's frame, a document.
        assert_eq!(window_program(r"C:\Windows\System32\cmd.exe", Some("/k vcvars.bat")), None);
        assert_eq!(window_program(r"C:\Program Files\Google\Chrome\Application\chrome_proxy.exe", Some("--app-id=abc")), None);
        assert_eq!(window_program(r"C:\Windows\System32\services.msc", None), None);
    }

    #[test]
    fn packaged_applications_are_read_from_powershells_list() {
        let json = "\u{feff}[{\"Name\":\"Calculator\",\"AppID\":\"Microsoft.WindowsCalculator_8wekyb3d8bbwe!App\"},\
            {\"Name\":\"Notepad++\",\"AppID\":\"{6D809377-6AF0-444B-8957-A3773F02200E}\\\\Notepad++\\\\notepad++.exe\"},\
            {\"Name\":\"Firefox\",\"AppID\":\"308046B0AF4A39CB\"},\
            {\"Name\":\"Terminal\",\"AppID\":\"Microsoft.WindowsTerminal_8wekyb3d8bbwe!App\"}]\r\n";
        assert_eq!(
            packaged(json),
            [
                ("Calculator".to_owned(), "Microsoft.WindowsCalculator_8wekyb3d8bbwe!App".to_owned()),
                ("Terminal".to_owned(), "Microsoft.WindowsTerminal_8wekyb3d8bbwe!App".to_owned())
            ]
        );
        assert_eq!(packaged(r#"{"Name":"Only","AppID":"Family_abc!Only"}"#), [("Only".to_owned(), "Family_abc!Only".to_owned())]);
        assert!(packaged("").is_empty());
        assert!(packaged("Get-StartApps : The term is not recognized").is_empty());
    }
}
