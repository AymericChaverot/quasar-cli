//! Editing an environment as the dashboard stores it: dotenv text whose
//! comments, blank lines and order belong to whoever wrote them, and which a
//! change to one variable must leave alone.
//!
//! Quasar hands each `KEY=value` line to the container as it is, quotes and
//! all, so values are written raw here too, exactly as they would be typed
//! into the dashboard's editor.

use std::collections::BTreeMap;

use anyhow::{Result, bail};

/// The variables the text defines, in the way Quasar reads them: trimmed
/// lines holding an `=`, comments skipped, and the last of a repeated name
/// winning.
pub fn parse(content: &str) -> BTreeMap<String, String> {
    content
        .lines()
        .filter_map(assignment)
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

/// A line's name and value, when the line assigns one.
fn assignment(line: &str) -> Option<(&str, &str)> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let (k, v) = line.split_once('=')?;
    Some((k.trim(), v))
}

/// Checks a `KEY=value` argument and splits it.
pub fn parse_pair(arg: &str) -> Result<(&str, &str)> {
    let Some((key, value)) = arg.split_once('=') else {
        bail!("{arg} is not KEY=value");
    };
    check_key(key)?;
    if value.contains('\n') || value.contains('\r') {
        bail!("the value of {key} spans several lines, which an environment cannot hold");
    }
    Ok((key, value))
}

/// A variable name as a shell would accept it.
pub fn check_key(key: &str) -> Result<()> {
    let mut chars = key.chars();
    let ok = matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_');
    if !ok {
        bail!("{key:?} is not a variable name (letters, digits and _, not starting with a digit)");
    }
    Ok(())
}

/// Sets a variable: its first line takes the new value where it stands, any
/// later lines for the same name go (they would override it), and a name
/// not there yet is added at the end.
pub fn set(content: &str, key: &str, value: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut done = false;
    for line in content.lines() {
        match assignment(line) {
            Some((k, _)) if k == key => {
                if !done {
                    out.push(format!("{key}={value}"));
                    done = true;
                }
            }
            _ => out.push(line.to_string()),
        }
    }
    if !done {
        out.push(format!("{key}={value}"));
    }
    join(out)
}

/// Removes every line defining the variable, and says whether there was one.
pub fn unset(content: &str, key: &str) -> (String, bool) {
    let mut found = false;
    let out: Vec<String> = content
        .lines()
        .filter(|line| match assignment(line) {
            Some((k, _)) if k == key => {
                found = true;
                false
            }
            _ => true,
        })
        .map(str::to_string)
        .collect();
    (join(out), found)
}

fn join(lines: Vec<String>) -> String {
    let mut s = lines.join("\n");
    if !s.is_empty() {
        s.push('\n');
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    const ENV: &str = "# the database\nDATABASE_URL=postgres://u:p@db/x\n\nDEBUG=0\n";

    #[test]
    fn parse_reads_like_quasar() {
        let vars = parse("# c\n A = 1 \nB=x=y\nnoequals\nA=2\n");
        assert_eq!(vars.get("A").map(String::as_str), Some("2"));
        assert_eq!(vars.get("B").map(String::as_str), Some("x=y"));
        assert_eq!(vars.len(), 2);
    }

    #[test]
    fn set_replaces_in_place_and_keeps_the_rest() {
        assert_eq!(
            set(ENV, "DEBUG", "1"),
            "# the database\nDATABASE_URL=postgres://u:p@db/x\n\nDEBUG=1\n"
        );
    }

    #[test]
    fn set_appends_a_new_name() {
        assert_eq!(set(ENV, "PORT", "8080"), format!("{ENV}PORT=8080\n"));
        assert_eq!(set("", "A", "1"), "A=1\n");
        assert_eq!(set("A=1", "B", "2"), "A=1\nB=2\n");
    }

    #[test]
    fn set_drops_later_duplicates() {
        assert_eq!(set("A=1\nB=2\nA=3\n", "A", "9"), "A=9\nB=2\n");
    }

    #[test]
    fn set_ignores_a_commented_line_of_the_same_name() {
        assert_eq!(set("# A=old\n", "A", "1"), "# A=old\nA=1\n");
    }

    #[test]
    fn unset_removes_every_definition() {
        let (out, found) = unset("A=1\n# A=c\nB=2\nA=3\n", "A");
        assert!(found);
        assert_eq!(out, "# A=c\nB=2\n");
        assert!(!unset(ENV, "NOPE").1);
    }

    #[test]
    fn pairs_and_keys_are_checked() {
        assert_eq!(parse_pair("A=b=c").unwrap(), ("A", "b=c"));
        assert_eq!(parse_pair("EMPTY=").unwrap(), ("EMPTY", ""));
        assert!(parse_pair("novalue").is_err());
        assert!(parse_pair("1A=x").is_err());
        assert!(parse_pair("A B=x").is_err());
        assert!(parse_pair("A=line\nbreak").is_err());
        assert!(check_key("_private2").is_ok());
    }
}
