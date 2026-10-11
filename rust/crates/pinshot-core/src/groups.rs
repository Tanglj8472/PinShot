//! Pin groups: Default is protected; names are case-insensitive unique.

use crate::i18n::tr;

pub const DEFAULT: &str = "Default";
pub const DEFAULT_GROUP: &str = DEFAULT;

pub fn dedupe(names: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for n in names {
        let n = n.trim();
        if n.is_empty() {
            continue;
        }
        let key = n.to_ascii_lowercase();
        if seen.insert(key) {
            out.push(n.to_string());
        }
    }
    if !out.iter().any(|g| g.eq_ignore_ascii_case(DEFAULT)) {
        out.insert(0, DEFAULT.into());
    }
    if out.is_empty() {
        out.push(DEFAULT.into());
    }
    out
}

pub fn find<'a>(names: &'a [String], name: &str) -> Option<&'a str> {
    let want = name.trim().to_ascii_lowercase();
    names
        .iter()
        .find(|g| g.to_ascii_lowercase() == want)
        .map(|s| s.as_str())
}

pub fn ensure(names: &mut Vec<String>, name: &str) -> String {
    let name = name.trim();
    if name.is_empty() {
        return DEFAULT.into();
    }
    if let Some(e) = find(names, name) {
        return e.to_string();
    }
    names.push(name.to_string());
    *names = dedupe(names);
    find(names, name).unwrap_or(DEFAULT).to_string()
}

pub fn rename(names: &mut Vec<String>, old: &str, new: &str) -> Result<String, String> {
    if old.eq_ignore_ascii_case(DEFAULT) {
        return Err("The Default group cannot be renamed.".into());
    }
    let new = new.trim();
    if new.is_empty() {
        return Err("empty".into());
    }
    if let Some(clash) = find(names, new) {
        if !clash.eq_ignore_ascii_case(old) {
            return Err("duplicate".into());
        }
    }
    let Some(i) = names.iter().position(|g| g.eq_ignore_ascii_case(old)) else {
        return Err("missing".into());
    };
    names[i] = new.to_string();
    *names = dedupe(names);
    Ok(new.to_string())
}

pub fn remove(names: &mut Vec<String>, name: &str) -> Result<(), String> {
    if name.eq_ignore_ascii_case(DEFAULT) {
        return Err("The Default group cannot be removed.".into());
    }
    names.retain(|g| !g.eq_ignore_ascii_case(name));
    *names = dedupe(names);
    Ok(())
}

/// Display label: built-in Default / "Group N" are translated.
pub fn label(id: &str) -> String {
    if id.eq_ignore_ascii_case(DEFAULT) {
        return tr("Default");
    }
    if let Some(rest) = id.strip_prefix("Group ") {
        if rest.chars().all(|c| c.is_ascii_digit()) {
            return tr(&format!("Group {rest}"));
        }
    }
    id.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dedupe_inserts_default() {
        let g = dedupe(&["Work".into(), "work".into()]);
        assert!(g.iter().any(|x| x == DEFAULT));
        assert_eq!(g.iter().filter(|x| x.eq_ignore_ascii_case("work")).count(), 1);
    }

    #[test]
    fn cannot_remove_default() {
        let mut g = vec![DEFAULT.into(), "A".into()];
        assert!(remove(&mut g, "Default").is_err());
        assert!(g.iter().any(|x| x == DEFAULT));
    }

    #[test]
    fn rename_and_ensure() {
        let mut g = vec![DEFAULT.into(), "A".into()];
        assert_eq!(rename(&mut g, "A", "B").unwrap(), "B");
        assert_eq!(ensure(&mut g, "b"), "B");
        assert!(rename(&mut g, DEFAULT, "X").is_err());
    }
}
