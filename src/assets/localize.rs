//! The game's localized text: each file `platform/resource/localization_files.txt` and then
//! `dev_localization_files.txt` list, with `_english.txt` appended, UTF-16 with a byte order
//! mark, its `"Tokens"` block of
//! `"NAME" "text"` pairs. The engine's table (`CLocalize`, vtable `0x1357e60`) finds a name
//! without its leading `#`, comparing ASCII letters without case (`0x3a57c0`, `0x466d30`).
//!
//! R5Reloaded's SDK then adds each enabled mod's (`mods/mods.vdf` by the `id` in each
//! `mods/*/mod.vdf`): the files its `LocalizationFiles` names, `%language%` replaced.
//!
//! Not checked against the engine's loader: which of two entries of one name it keeps (here
//! the first), the escapes (`\n`, `\r`, `\t`, `\\`, `\"`), and the conditions after an entry
//! (`[$PC]`, `[!$NX&&!$PS4]`), read here as on a PC. Not checked against the SDK's: the
//! order the mods are read in (here the directories' names).

use std::collections::HashMap;
use std::path::Path;

/// The localized texts by name, lower-cased, in UTF-16.
pub type Table = HashMap<String, Vec<u16>>;

const PC: [&str; 3] = ["$PC", "$WIN32", "$WINDOWS"];

/// The lists of files, read one after the other (`0x3a3d10`): the second one, of files "not
/// shipped", is read in every build all the same.
const LISTS: [&str; 2] = ["localization_files.txt", "dev_localization_files.txt"];

pub fn load(install: &Path, language: &str) -> Table {
    let dir = install.join("platform/resource");
    let mut table = Table::new();
    for list in LISTS {
        let Ok(list) = std::fs::read_to_string(dir.join(list)) else { continue };
        for line in list.lines() {
            let name = line.split("//").next().unwrap_or_default().trim();
            if name.is_empty() {
                continue;
            }
            read(&dir.join(format!("{name}_{language}.txt")), &mut table);
        }
    }
    for file in mod_files(install, language) {
        read(&file, &mut table);
    }
    table
}

fn read(path: &Path, table: &mut Table) {
    let Ok(bytes) = std::fs::read(path) else { return };
    let units: Vec<u16> = bytes.as_chunks::<2>().0.iter().map(|&c| u16::from_le_bytes(c)).skip_while(|&u| u == 0xfeff).collect();
    parse(&String::from_utf16_lossy(&units), table);
}

/// The enabled mods' localization files.
fn mod_files(install: &Path, language: &str) -> Vec<std::path::PathBuf> {
    use super::keyvalues;
    let mods = install.join("mods");
    let list = std::fs::read(mods.join("mods.vdf")).map(|b| keyvalues::parse(&b)).unwrap_or_default();
    let Some(list) = list.first() else { return Vec::new() };
    let Ok(dirs) = std::fs::read_dir(&mods) else { return Vec::new() };
    let mut dirs: Vec<_> = dirs.flatten().map(|d| d.path()).filter(|d| d.join("mod.vdf").is_file()).collect();
    dirs.sort();
    let mut out = Vec::new();
    for dir in dirs {
        let vdf = std::fs::read(dir.join("mod.vdf")).map(|b| keyvalues::parse(&b)).unwrap_or_default();
        let Some(m) = vdf.first() else { continue };
        if m.str("id").and_then(|id| list.str(id)) != Some("1") {
            continue;
        }
        for k in m.find("LocalizationFiles").map(|k| k.keys()).unwrap_or_default() {
            out.push(dir.join(k.name.replace("%language%", language)));
        }
    }
    out
}

#[derive(Debug, PartialEq)]
enum Word {
    Str(String),
    Open,
    Close,
    Cond(String),
}

fn words(text: &str) -> Vec<Word> {
    let mut out = Vec::new();
    let mut it = text.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '/' if it.peek() == Some(&'/') => {
                it.by_ref().find(|&c| c == '\n');
            }
            '{' => out.push(Word::Open),
            '}' => out.push(Word::Close),
            '[' => out.push(Word::Cond(it.by_ref().take_while(|&c| c != ']').collect())),
            '"' => {
                let mut s = String::new();
                while let Some(c) = it.next() {
                    match c {
                        '"' => break,
                        '\\' => match it.next() {
                            Some('n') => s.push('\n'),
                            Some('r') => s.push('\r'),
                            Some('t') => s.push('\t'),
                            Some(e @ ('\\' | '"')) => s.push(e),
                            Some(e) => s.extend(['\\', e]),
                            None => {}
                        },
                        c => s.push(c),
                    }
                }
                out.push(Word::Str(s));
            }
            c if c.is_whitespace() => {}
            c => {
                let mut s = c.to_string();
                while let Some(&n) = it.peek().filter(|&&n| !n.is_whitespace() && !"{}\"[".contains(n)) {
                    s.push(n);
                    it.next();
                }
                out.push(Word::Str(s));
            }
        }
    }
    out
}

fn on_pc(cond: &str) -> bool {
    cond.split("||").any(|all| {
        all.split("&&").all(|term| {
            let term = term.trim();
            match term.strip_prefix('!') {
                Some(t) => !PC.contains(&t),
                None => PC.contains(&term),
            }
        })
    })
}

fn parse(text: &str, table: &mut Table) {
    let words = words(text);
    let Some(start) = words.windows(2).position(|w| matches!(&w[0], Word::Str(s) if s.eq_ignore_ascii_case("tokens")) && w[1] == Word::Open) else { return };
    let mut it = words[start + 2..].iter().peekable();
    while let Some(Word::Str(name)) = it.next() {
        let Some(Word::Str(text)) = it.next() else { return };
        if let Some(Word::Cond(c)) = it.peek() {
            it.next();
            if !on_pc(c) {
                continue;
            }
        }
        table.entry(name.to_ascii_lowercase()).or_insert_with(|| text.encode_utf16().collect());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_keep_the_pc_entry_and_their_escapes() {
        let mut t = Table::new();
        let file = "\"lang\"\n{\n\"Language\" \"english\"\n// a comment\n\"Tokens\"\n{\n\"A\" \"one\\ntwo \\\"q\\\"\"\n\"B\" \"Console\"[$NX]\n\"B\" \"PC\"[!$NX&&!$PS4]\n\"Ride\"\t\"Ride\"\n}\n}\n";
        parse(file, &mut t);
        let get = |k: &str| t.get(k).map(|u| String::from_utf16_lossy(u));
        assert_eq!(get("a").as_deref(), Some("one\ntwo \"q\""));
        assert_eq!(get("b").as_deref(), Some("PC"));
        assert_eq!(get("ride").as_deref(), Some("Ride"));
        assert_eq!(get("language"), None);
    }
}
