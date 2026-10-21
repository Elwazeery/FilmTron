//! Parser and lookup tables for translation catalogs (`*.tsv`, format documented in `es.tsv`).

use std::collections::HashMap;

/// A parsed catalog. Built once per language and kept for the life of the process.
#[derive(Debug, Default)]
pub struct Catalog {
    /// context-free strings: English source → translation (looked up every frame)
    plain: HashMap<String, String>,
    /// strings with a disambiguating context, keyed `context \u{1} source`
    contextual: HashMap<String, String>,
}

/// A catalog entry as read from the file: (context, source, translation).
pub type Entry = (String, String, String);

fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match it.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('\\') => out.push('\\'),
            Some(o) => {
                out.push('\\');
                out.push(o);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// Read the entries of a catalog file. Malformed lines are returned as errors (and skipped), so a
/// bad translation never breaks the UI; the tests insist the bundled catalogs have none.
pub fn parse_entries(text: &str) -> (Vec<Entry>, Vec<String>) {
    let mut entries = Vec::new();
    let mut errors = Vec::new();
    for (n, line) in text.lines().enumerate() {
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let mut cols = line.split('\t');
        match (cols.next(), cols.next(), cols.next(), cols.next()) {
            (Some(ctx), Some(src), Some(tr), None) if !src.is_empty() && !tr.is_empty() => {
                entries.push((unescape(ctx), unescape(src), unescape(tr)));
            }
            _ => errors.push(format!("line {}: expected `context<TAB>source<TAB>translation`", n + 1)),
        }
    }
    (entries, errors)
}

/// Legacy FilmCraft menu/dialog strings mapped to their FilmTron branded equivalents.
pub const BRAND_ALIASES: &[(&str, &str)] = &[
    ("Join the ArtCraft Discord…", "Original ArtCraft Discord…"),
    ("ArtCraft Website", "ArtCraft Upstream Website"),
    ("FilmCraft on getartcraft.com", "Original FilmCraft Website"),
    ("FilmCraft on GitHub", "FilmTron on GitHub"),
    ("About FilmCraft", "About FilmTron"),
    ("Hide FilmCraft", "Hide FilmTron"),
    ("Quit FilmCraft", "Quit FilmTron"),
    ("FilmCraft hit an error", "FilmTron hit an error"),
    (
        "Shows the version, community links and credits in a FilmCraft window.",
        "Shows the FilmTron version, upstream credits and community links.",
    ),
];

impl Catalog {
    pub fn parse(text: &str) -> Catalog {
        let mut c = Catalog::default();
        let entries = parse_entries(text).0;
        for (ctx, src, tr) in &entries {
            if ctx.is_empty() {
                c.plain.insert(src.clone(), tr.clone());
            } else {
                c.contextual.insert(format!("{ctx}\u{1}{src}"), tr.clone());
            }
        }
        for (ctx, src, tr) in &entries {
            if src.contains("FilmCraft") {
                let branded_src = src.replace("FilmCraft", "FilmTron");
                let branded_tr = tr.replace("FilmCraft", "FilmTron");
                if ctx.is_empty() {
                    c.plain.entry(branded_src).or_insert(branded_tr);
                } else {
                    c.contextual.entry(format!("{ctx}\u{1}{branded_src}")).or_insert(branded_tr);
                }
            }
        }
        for (old_src, new_src) in BRAND_ALIASES {
            if let Some(tr) = c.plain.get(*old_src).cloned() {
                c.plain.entry((*new_src).to_string()).or_insert_with(|| tr.replace("FilmCraft", "FilmTron"));
            }
        }
        let dark = c.plain.get("Dark").cloned().unwrap_or_else(|| "Dark".to_string());
        let light = c.plain.get("Light").cloned().unwrap_or_else(|| "Light".to_string());
        c.plain.entry("Inferno Pixel Glass (Dark)".to_string()).or_insert_with(|| format!("Inferno Pixel Glass ({dark})"));
        c.plain.entry("Inferno Pixel Glass (Light)".to_string()).or_insert_with(|| format!("Inferno Pixel Glass ({light})"));
        c
    }

    pub fn plain(&self, s: &str) -> Option<&str> {
        self.plain.get(s).map(String::as_str)
    }

    pub fn contextual(&self, ctx: &str, s: &str) -> Option<&str> {
        self.contextual.get(&format!("{ctx}\u{1}{s}")).map(String::as_str)
    }
}

/// `{name}` placeholders of a template, in order of appearance.
#[cfg(test)]
pub fn placeholders(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = s;
    while let Some(a) = rest.find('{') {
        let after = &rest[a + 1..];
        match after.find('}') {
            Some(b) => {
                out.push(&after[..b]);
                rest = &after[b + 1..];
            }
            None => break,
        }
    }
    out
}
