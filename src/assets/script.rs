//! Weapon scripts: the KeyValues files the game reads a weapon's settings from,
//! `platform/scripts/weapons/<name>.txt`. A `#base` file is read first and the file's own keys
//! override it. The top-level keys of the `WeaponData` block are kept, lowercased, and the
//! `RUI_CrosshairData` block is read on its own (`crosshairs`).

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

/// A weapon's crosshairs (`RUI_CrosshairData`): arguments every crosshair takes, then each
/// `Crosshair_N` by number with its RUI and its own arguments. An argument is bound to a value
/// the engine names (`weapon_is_firing`, `player_zoomFrac`, ...).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Crosshairs {
    pub default_args: Vec<(String, String)>,
    pub crosshairs: Vec<Crosshair>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Crosshair {
    pub number: u32,
    pub ui: String,
    pub args: Vec<(String, String)>,
}

fn set(list: &mut Vec<(String, String)>, k: &str, v: &str) {
    match list.iter_mut().find(|(key, _)| key.eq_ignore_ascii_case(k)) {
        Some(e) => e.1 = v.to_string(),
        None => list.push((k.to_string(), v.to_string())),
    }
}

/// The pairs of a block starting at `toks[i]` (an `Open`), and the index past its `Close`;
/// nested blocks are handed to `nested` by name.
fn block(toks: &[Tok], mut i: usize, mut pair: impl FnMut(&str, &str), mut nested: impl FnMut(&str, usize) -> usize) -> usize {
    i += 1;
    while i < toks.len() {
        match (&toks[i], toks.get(i + 1)) {
            (Tok::Close, _) => return i + 1,
            (Tok::Str(k), Some(Tok::Open)) => i = nested(k, i + 1),
            (Tok::Str(k), Some(Tok::Str(v))) => {
                pair(k, v);
                i += 2;
            }
            _ => i += 1,
        }
    }
    i
}

fn skip(toks: &[Tok], i: usize) -> usize {
    block(toks, i, |_, _| {}, |_, j| skip(toks, j))
}

fn read_crosshairs(dir: &Path, file: &str, out: &mut Crosshairs, depth: u32) -> io::Result<()> {
    let text = std::fs::read_to_string(dir.join(file))?;
    for line in text.lines() {
        if let Some(base) = line.trim().strip_prefix("#base").map(|b| b.trim().trim_matches('"')).filter(|_| depth < 8) {
            read_crosshairs(dir, base, out, depth + 1)?;
        }
    }
    let toks = tokens(&text);
    let mut i = 0;
    while i < toks.len() {
        if matches!((&toks[i], toks.get(i + 1)), (Tok::Str(k), Some(Tok::Open)) if k.eq_ignore_ascii_case("RUI_CrosshairData")) {
            let mut defaults = std::mem::take(&mut out.default_args);
            let mut found = std::mem::take(&mut out.crosshairs);
            i = block(&toks, i + 1, |_, _| {}, |name, j| {
                if name.eq_ignore_ascii_case("DefaultArgs") {
                    return block(&toks, j, |k, v| set(&mut defaults, k, v), |_, j| skip(&toks, j));
                }
                let Some(n) = name.strip_prefix("Crosshair_").and_then(|n| n.parse::<u32>().ok()) else { return skip(&toks, j) };
                let at = found.iter().position(|c| c.number == n).unwrap_or_else(|| {
                    found.push(Crosshair { number: n, ..Default::default() });
                    found.len() - 1
                });
                let mut c = std::mem::take(&mut found[at]);
                let end = block(
                    &toks,
                    j,
                    |k, v| {
                        if k.eq_ignore_ascii_case("ui") {
                            c.ui = v.to_string();
                        }
                    },
                    |inner, j| if inner.eq_ignore_ascii_case("Args") { block(&toks, j, |k, v| set(&mut c.args, k, v), |_, j| skip(&toks, j)) } else { skip(&toks, j) },
                );
                found[at] = c;
                end
            });
            out.default_args = defaults;
            out.crosshairs = found;
            continue;
        }
        i += 1;
    }
    out.crosshairs.sort_by_key(|c| c.number);
    Ok(())
}

/// A weapon's `RUI_CrosshairData`, its `#base` files' first, then its own over them.
pub fn crosshairs(install: &Path, name: &str) -> io::Result<Crosshairs> {
    let mut out = Crosshairs::default();
    read_crosshairs(&install.join("platform/scripts/weapons"), &format!("{name}.txt"), &mut out, 0)?;
    Ok(out)
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

    #[test]
    fn crosshairs_merge_over_their_base() {
        let dir = std::env::temp_dir().join(format!("lurch-crosshair-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("_b.txt"), "WeaponData\n{\n\tRUI_CrosshairData\n\t{\n\t\tDefaultArgs\n\t\t{\n\t\t\tadsFrac player_zoomFrac\n\t\t}\n\t\tCrosshair_1\n\t\t{\n\t\t\t\"ui\" \"ui/a\"\n\t\t}\n\t}\n}\n").unwrap();
        std::fs::write(
            dir.join("c.txt"),
            "#base \"_b.txt\"\nWeaponData\n{\n\tRUI_CrosshairData\n\t{\n\t\tDefaultArgs\n\t\t{\n\t\t\tteamColor crosshair_team_color\n\t\t}\n\t\tCrosshair_2\n\t\t{\n\t\t\t\"ui\" \"ui/b\"\n\t\t\tArgs\n\t\t\t{\n\t\t\t\tammoFrac \"progress_weapon_clip_ammo_frac\"\n\t\t\t}\n\t\t}\n\t}\n}\n",
        )
        .unwrap();
        let mut out = Crosshairs::default();
        read_crosshairs(&dir, "c.txt", &mut out, 0).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(out.default_args, [("adsFrac".into(), "player_zoomFrac".into()), ("teamColor".into(), "crosshair_team_color".into())]);
        assert_eq!(out.crosshairs.len(), 2);
        assert_eq!(out.crosshairs[0].ui, "ui/a");
        assert_eq!(out.crosshairs[1], Crosshair { number: 2, ui: "ui/b".into(), args: vec![("ammoFrac".into(), "progress_weapon_clip_ammo_frac".into())] });
    }
}
