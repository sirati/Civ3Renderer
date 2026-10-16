//! The INI dialect of Civilization III's unit and animation descriptions
//! (`Art/Units/<Unit>/<Unit>.ini`): `[Section]` headers, `KEY=value` lines,
//! `;` comments, keys case-insensitive.

use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Ini {
    /// Lower-cased section name -> lower-cased key -> raw value.
    sections: BTreeMap<String, BTreeMap<String, String>>,
}

impl Ini {
    pub fn parse(text: &str) -> Self {
        let mut ini = Ini::default();
        let mut section = String::new();
        for line in text.lines() {
            let line = line.split(';').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                section = name.trim().to_lowercase();
                ini.sections.entry(section.clone()).or_default();
            } else if let Some((k, v)) = line.split_once('=') {
                ini.sections.entry(section.clone()).or_default().insert(k.trim().to_lowercase(), v.trim().to_owned());
            }
        }
        ini
    }

    /// Civilization III's INI files are Windows-1252; decode losslessly.
    pub fn parse_bytes(bytes: &[u8]) -> Self {
        Self::parse(&bytes.iter().map(|&b| b as char).collect::<String>())
    }

    pub fn get(&self, section: &str, key: &str) -> Option<&str> {
        self.sections.get(&section.to_lowercase())?.get(&key.to_lowercase()).map(String::as_str)
    }

    pub fn section(&self, section: &str) -> impl Iterator<Item = (&str, &str)> {
        self.sections
            .get(&section.to_lowercase())
            .into_iter()
            .flat_map(|s| s.iter().map(|(k, v)| (k.as_str(), v.as_str())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_unit_ini_style() {
        let ini = Ini::parse("[Animations]\nBLANK=\nDEFAULT=WarriorDefault.flc ; idle\nRUN=WarriorRun.flc\n\n[Timing]\nDEFAULT=0.500000\n");
        assert_eq!(ini.get("animations", "default"), Some("WarriorDefault.flc"));
        assert_eq!(ini.get("ANIMATIONS", "Blank"), Some(""));
        assert_eq!(ini.get("Timing", "default"), Some("0.500000"));
        assert_eq!(ini.section("animations").count(), 3);
    }
}
