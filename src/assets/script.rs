//! Weapon scripts: the KeyValues files the game reads a weapon's settings from,
//! `platform/scripts/weapons/<name>.txt`. A `#base` file is read first and the file's own keys
//! override it. Only the top-level keys of the `WeaponData` block are kept, lowercased.

use std::collections::HashMap;
use std::io;
use std::path::Path;

#[derive(Debug, PartialEq)]
enum Tok {
    Str(String),
    Open,
    Close,
}

fn tokens(text: &str) -> Vec<Tok> {
    let mut out = Vec::new();
    let mut it = text.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '"' => out.push(Tok::Str(it.by_ref().take_while(|&c| c != '"').collect())),
            '{' => out.push(Tok::Open),
            '}' => out.push(Tok::Close),
            '/' if it.peek() == Some(&'/') => while it.next_if(|&c| c != '\n').is_some() {},
            '#' => while it.next_if(|&c| c != '\n').is_some() {},
            c if c.is_whitespace() => {}
            c => {
                let mut word = c.to_string();
                while let Some(c) = it.next_if(|c| !c.is_whitespace() && !matches!(c, '"' | '{' | '}')) {
                    word.push(c);
                }
                out.push(Tok::Str(word));
            }
        }
    }
    out
}

fn read(dir: &Path, file: &str, out: &mut HashMap<String, String>, depth: u32) -> io::Result<()> {
    let text = std::fs::read_to_string(dir.join(file))?;
    for line in text.lines() {
        if let Some(base) = line.trim().strip_prefix("#base").map(|b| b.trim().trim_matches('"')).filter(|_| depth < 8) {
            read(dir, base, out, depth + 1)?;
        }
    }
    let toks = tokens(&text);
    let mut level = 0;
    let mut i = 0;
    while i < toks.len() {
        match (&toks[i], toks.get(i + 1)) {
            (Tok::Open, _) => level += 1,
            (Tok::Close, _) => level -= 1,
            (Tok::Str(k), Some(Tok::Str(v))) if level == 1 => {
                out.insert(k.to_ascii_lowercase(), v.clone());
                i += 1;
            }
            _ => {}
        }
        i += 1;
    }
    Ok(())
}

pub fn weapon(install: &Path, name: &str) -> io::Result<HashMap<String, String>> {
    let mut out = HashMap::new();
    read(&install.join("platform/scripts/weapons"), &format!("{name}.txt"), &mut out, 0)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_base_is_overridden_and_nested_blocks_skipped() {
        let dir = std::env::temp_dir().join(format!("lurch-script-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("_base.txt"), "WeaponData\n{\n\t\"deploy_time\" \"0.5\"\n\t\"raise_time\" \"0.3\"\n}\n").unwrap();
        std::fs::write(
            dir.join("w.txt"),
            "#base \"_base.txt\"\nWeaponData\n{\n\t// \"raise_time\" \"9\"\n\t\"Deploy_Time\"\t\"0.25\"\n\tMods\n\t{\n\t\tx\n\t\t{\n\t\t\t\"raise_time\" \"7\"\n\t\t}\n\t}\n\tactive_crosshair_count\t\"1\"\n}\n",
        )
        .unwrap();
        let mut out = HashMap::new();
        read(&dir, "w.txt", &mut out, 0).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(out.get("deploy_time").map(String::as_str), Some("0.25"));
        assert_eq!(out.get("raise_time").map(String::as_str), Some("0.3"));
        assert_eq!(out.get("active_crosshair_count").map(String::as_str), Some("1"));
    }
}
