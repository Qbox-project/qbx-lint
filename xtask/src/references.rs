use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io::Read;
use std::path::PathBuf;

use serde_json::{json, Value};

type Result<T> = std::result::Result<T, String>;

const MAX_DOWNLOAD: u64 = 2 * 1024 * 1024;

struct Source {
    key: &'static str,
    repository: &'static str,
    path: &'static str,
    url: &'static str,
}

const CONTROLS: Source = Source {
    key: "controls",
    repository: "citizenfx/fivem-docs",
    path: "content/docs/game-references/controls.md",
    url: "https://docs.fivem.net/docs/game-references/controls/",
};

const PED_FLAGS: Source = Source {
    key: "ped_config_flags",
    repository: "citizenfx/natives",
    path: "PED/SetPedConfigFlag.md",
    url: "https://docs.fivem.net/natives/?_0x1913FE4CBF41C463",
};

#[derive(Debug, PartialEq, Eq)]
struct Control {
    id: u32,
    name: String,
    keyboard: String,
    controller: String,
}

#[derive(Debug, PartialEq, Eq)]
struct PedFlag {
    id: u32,
    name: String,
}

pub fn generate(args: impl Iterator<Item = String>) -> Result<()> {
    let args: Vec<_> = args.collect();
    let pinned = match args.as_slice() {
        [] => false,
        [flag] if flag == "--pinned" => true,
        _ => return Err("usage: cargo xtask references [--pinned]".into()),
    };
    let data_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../crates/qbx_fivem_data/data");
    let previous_sources: Value = if pinned {
        let text = std::fs::read_to_string(data_dir.join("reference_sources.json")).map_err(|e| e.to_string())?;
        serde_json::from_str(&text).map_err(|e| e.to_string())?
    } else {
        Value::Null
    };
    let (controls_text, mut controls_source) =
        fetch_source(&CONTROLS, pinned.then(|| &previous_sources[CONTROLS.key]))?;
    let (flags_text, mut flags_source) = fetch_source(&PED_FLAGS, pinned.then(|| &previous_sources[PED_FLAGS.key]))?;
    let controls = parse_controls(&controls_text)?;
    let flags = parse_ped_flags(&flags_text)?;

    // Validate both complete downloads before replacing any checked-in file.
    let previous_count =
        |filename| std::fs::read_to_string(data_dir.join(filename)).map_or(0, |text| text.lines().count());
    validate_complete("controls", controls.iter().map(|row| row.id), 361, previous_count("controls.tsv"))?;
    validate_complete("ped flags", flags.iter().map(|row| row.id), 464, previous_count("ped_config_flags.tsv"))?;

    let mut controls_tsv = String::new();
    for row in &controls {
        writeln!(controls_tsv, "{}\t{}\t{}\t{}", row.id, row.name, row.keyboard, row.controller).unwrap();
    }
    let mut flags_tsv = String::new();
    for row in &flags {
        // The source names flags but supplies no per-flag prose. Leave description empty.
        writeln!(flags_tsv, "{}\t{}\t", row.id, row.name).unwrap();
    }
    controls_source["count"] = json!(controls.len());
    flags_source["count"] = json!(flags.len());
    let sources = json!({ "format_version": 1, "controls": controls_source, "ped_config_flags": flags_source });
    let sources_json = format!("{}\n", serde_json::to_string_pretty(&sources).map_err(|e| e.to_string())?);
    for (filename, content) in
        [("controls.tsv", controls_tsv), ("ped_config_flags.tsv", flags_tsv), ("reference_sources.json", sources_json)]
    {
        std::fs::write(data_dir.join(filename), content).map_err(|e| format!("writing {filename}: {e}"))?;
    }
    eprintln!("wrote {} controls and {} ped configuration flags", controls.len(), flags.len());
    Ok(())
}

fn fetch_source(source: &Source, pinned: Option<&Value>) -> Result<(String, Value)> {
    let (revision, committed_at) = if let Some(pinned) = pinned {
        if pinned["repository"] != source.repository
            || pinned["path"] != source.path
            || pinned["source_url"] != source.url
        {
            return Err(format!("pinned {} source does not match the configured official source", source.key));
        }
        (string_field(pinned, "revision")?, string_field(pinned, "committed_at")?)
    } else {
        let url = format!("https://api.github.com/repos/{}/commits?path={}&per_page=1", source.repository, source.path);
        let json: Value =
            serde_json::from_str(&download(&url)?).map_err(|e| format!("invalid commit metadata: {e}"))?;
        (string_field(&json[0], "sha")?, string_field(&json[0]["commit"]["committer"], "date")?)
    };
    if revision.len() != 40 || !revision.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(format!("invalid {} commit revision", source.key));
    }
    if committed_at.len() != 20 || !committed_at.ends_with('Z') || !committed_at.contains('T') {
        return Err(format!("invalid {} commit date", source.key));
    }
    let raw_url = format!("https://raw.githubusercontent.com/{}/{revision}/{}", source.repository, source.path);
    let body = download(&raw_url)?;
    let metadata = json!({
        "repository": source.repository, "path": source.path, "source_url": source.url,
        "revision": revision, "committed_at": committed_at, "raw_url": raw_url,
    });
    Ok((body, metadata))
}

fn string_field(json: &Value, key: &str) -> Result<String> {
    json[key].as_str().filter(|value| !value.is_empty()).map(str::to_owned).ok_or_else(|| format!("missing {key}"))
}

fn download(url: &str) -> Result<String> {
    eprintln!("fetching {url}");
    let response = ureq::get(url)
        .set("User-Agent", "qbx-lint-reference-generator")
        .timeout(std::time::Duration::from_secs(30))
        .call()
        .map_err(|e| format!("{url}: {e}"))?;
    let mut body = String::new();
    response.into_reader().take(MAX_DOWNLOAD + 1).read_to_string(&mut body).map_err(|e| e.to_string())?;
    if body.len() as u64 > MAX_DOWNLOAD {
        return Err(format!("{url}: download exceeds size limit"));
    }
    Ok(body)
}

fn parse_controls(text: &str) -> Result<Vec<Control>> {
    let mut lines = text.lines();
    let found = lines
        .any(|line| columns(line).is_some_and(|cells| cells == ["Index", "Name", "Default QWERTY", "Xbox Controller"]));
    if !found {
        return Err("missing official controls table header".into());
    }
    let separator =
        columns(lines.next().ok_or("missing controls table separator")?).ok_or("invalid table separator")?;
    if separator.len() != 4
        || separator.iter().any(|cell| !cell.contains('-') || !cell.chars().all(|c| c == '-' || c == ':'))
    {
        return Err("invalid controls table separator".into());
    }
    let mut controls = BTreeMap::new();
    for line in lines {
        if !line.trim_start().starts_with('|') {
            break;
        }
        let cells = columns(line).ok_or("malformed controls row")?;
        if cells.len() != 4 {
            return Err(format!("expected four control columns: {line}"));
        }
        let id = cells[0].parse::<u32>().map_err(|_| format!("invalid control ID: {}", cells[0]))?;
        let name = plain_label(cells[1])?;
        if !name.starts_with("INPUT_") || !identifier(&name) {
            return Err(format!("invalid control name: {name}"));
        }
        let row = Control { id, name, keyboard: plain_label(cells[2])?, controller: plain_label(cells[3])? };
        if controls.insert(id, row).is_some() {
            return Err(format!("duplicate control ID: {id}"));
        }
    }
    if controls.is_empty() {
        return Err("empty controls table".into());
    }
    Ok(controls.into_values().collect())
}

fn columns(line: &str) -> Option<Vec<&str>> {
    let row = line.trim().strip_prefix('|')?.strip_suffix('|')?;
    Some(row.split('|').map(str::trim).collect())
}

fn parse_ped_flags(text: &str) -> Result<Vec<PedFlag>> {
    let (_, body) = text.split_once("enum ePedConfigFlags {").ok_or("missing ePedConfigFlags enum")?;
    let (body, _) = body.split_once('}').ok_or("unterminated ePedConfigFlags enum")?;
    let mut flags = BTreeMap::new();
    for line in body.lines() {
        let line = line.split_once("//").map_or(line, |(code, _)| code).trim();
        if line.is_empty() {
            continue;
        }
        let (name, value) = line.split_once('=').ok_or_else(|| format!("malformed ped flag: {line}"))?;
        let name = name.trim();
        let is_hash =
            name.strip_prefix("_0x").is_some_and(|hash| hash.len() == 8 && hash.bytes().all(|b| b.is_ascii_hexdigit()));
        if !identifier(name) || !(name.starts_with("CPED_CONFIG_FLAG_") || is_hash) {
            return Err(format!("invalid ped flag name: {name}"));
        }
        let id = value
            .trim()
            .trim_end_matches(',')
            .trim()
            .parse::<u32>()
            .map_err(|_| format!("invalid ped flag ID: {value}"))?;
        if flags.insert(id, PedFlag { id, name: name.to_owned() }).is_some() {
            return Err(format!("duplicate ped flag ID: {id}"));
        }
    }
    if flags.is_empty() {
        return Err("empty ped flags enum".into());
    }
    Ok(flags.into_values().collect())
}

fn identifier(name: &str) -> bool {
    !name.is_empty() && name.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

/// Flatten display markup rather than shipping HTML fragments into hover labels.
fn plain_label(text: &str) -> Result<String> {
    let mut plain = String::new();
    let mut rest = text;
    while let Some(start) = rest.find('<') {
        plain.push_str(&rest[..start]);
        let end = rest[start..].find('>').ok_or("unterminated HTML in reference label")? + start;
        let tag = rest[start + 1..end].trim().trim_matches('/').trim().to_ascii_lowercase();
        match tag.as_str() {
            "br" => plain.push_str(" / "),
            "b" | "strong" | "kbd" | "code" | "em" | "i" | "span" | "sub" | "sup" => {}
            _ => return Err(format!("unsupported HTML in reference label: <{tag}>")),
        }
        rest = &rest[end + 1..];
    }
    plain.push_str(rest);
    if let Some(inline_code) = plain.strip_prefix('`').and_then(|inside| inside.strip_suffix('`')) {
        plain = inline_code.to_owned();
    }
    let mut unescaped = String::new();
    let mut chars = plain.chars().peekable();
    while let Some(character) = chars.next() {
        if character == '\\' && chars.peek().is_some_and(char::is_ascii_punctuation) {
            unescaped.push(chars.next().unwrap());
        } else {
            unescaped.push(character);
        }
    }
    plain = unescaped;
    for (entity, replacement) in [
        ("&nbsp;", " "),
        ("&quot;", "\""),
        ("&#39;", "'"),
        ("&apos;", "'"),
        ("&lt;", "<"),
        ("&gt;", ">"),
        ("&amp;", "&"),
    ] {
        plain = plain.replace(entity, replacement);
    }
    if plain.contains('<') || plain.contains('>') || plain.contains(';') && plain.contains('&') {
        return Err("unhandled markup in reference label".into());
    }
    Ok(plain.split_whitespace().collect::<Vec<_>>().join(" "))
}

fn validate_complete(label: &str, ids: impl Iterator<Item = u32>, minimum: usize, previous: usize) -> Result<()> {
    let ids: Vec<_> = ids.collect();
    if ids.len() < minimum.max(previous) {
        return Err(format!(
            "refusing incomplete {label}: {} entries, expected at least {}",
            ids.len(),
            minimum.max(previous)
        ));
    }
    if !ids.iter().enumerate().all(|(expected, &id)| id as usize == expected) {
        return Err(format!("refusing incomplete {label}: IDs must be unique and contiguous from zero"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEADER: &str = "| Index | Name | Default QWERTY | Xbox Controller |\n| --- | --- | --- | --- |\n";

    #[test]
    fn controls_ignore_control_types_and_normalize_markup() {
        let text = format!("| Index | Name |\n| --- | --- |\n| 2 | FRONTEND_CONTROL |\n\n{HEADER}| 1 | INPUT\\_SECOND | | (NONE) |\n| 0 | INPUT\\_FIRST | <kbd>E</kbd><br />F &amp; G | `A` |\n");
        let rows = parse_controls(&text).unwrap();
        assert_eq!(
            rows[0],
            Control { id: 0, name: "INPUT_FIRST".into(), keyboard: "E / F & G".into(), controller: "A".into() }
        );
        assert_eq!(rows[1].keyboard, "");
        assert_eq!(rows[1].controller, "(NONE)");
        assert_eq!(plain_label("\\[").unwrap(), "[");
        assert_eq!(plain_label("~ / \\`").unwrap(), "~ / `");
        assert_eq!(plain_label("\\").unwrap(), "\\");
    }

    #[test]
    fn malformed_or_duplicate_controls_fail() {
        for rows in [
            "| 0 | INPUT_A | E | A |\n| 0 | INPUT_B | F | B |",
            "| 0 | INPUT_A | E |",
            "| zero | INPUT_A | E | A |",
            "| 0 | OTHER_A | E | A |",
        ] {
            assert!(parse_controls(&format!("{HEADER}{rows}")).is_err());
        }
        assert!(parse_controls("<html>bad gateway</html>").is_err());
        assert!(parse_controls(HEADER).is_err());
        assert!(plain_label("<script>alert(1)</script>").is_err());
        assert!(plain_label("<kbd>unfinished").is_ok());
        assert!(plain_label("<kbd unfinished").is_err());
        assert!(plain_label("&unknown;").is_err());
    }

    #[test]
    fn ped_flags_preserve_source_spelling_and_unknown_hashes() {
        let flags = parse_ped_flags("enum ePedConfigFlags {\n CPED_CONFIG_FLAG_DissableAutoFallOffTests = 0, // potential alias\n _0xDB115BFA = 1,\n}\n").unwrap();
        assert_eq!(flags[0].name, "CPED_CONFIG_FLAG_DissableAutoFallOffTests");
        assert_eq!(flags[1].name, "_0xDB115BFA");
    }

    #[test]
    fn malformed_duplicate_or_truncated_ped_flags_fail() {
        for body in
            ["CPED_CONFIG_FLAG_A = 0,\nCPED_CONFIG_FLAG_B = 0,", "CPED_CONFIG_FLAG_A = unknown,", "not an entry", ""]
        {
            assert!(parse_ped_flags(&format!("enum ePedConfigFlags {{\n{body}\n}}")).is_err());
        }
        assert!(parse_ped_flags("enum ePedConfigFlags {\nCPED_CONFIG_FLAG_A = 0,").is_err());
        assert!(parse_ped_flags("upstream unavailable").is_err());
    }

    #[test]
    fn incomplete_downloads_and_regressions_are_rejected() {
        assert!(validate_complete("fixture", [0, 1].into_iter(), 2, 2).is_ok());
        assert!(validate_complete("fixture", [0].into_iter(), 2, 0).is_err());
        assert!(validate_complete("fixture", [0, 1].into_iter(), 2, 3).is_err());
        assert!(validate_complete("fixture", [0, 2].into_iter(), 2, 2).is_err());
        assert!(validate_complete("fixture", [0, 0].into_iter(), 2, 2).is_err());
    }
}
