//! How results reach the terminal: aligned tables and coloured states for
//! people, JSON for scripts.

use std::io::{self, IsTerminal, Write};
use std::sync::OnceLock;

use anyhow::Result;
use serde::Serialize;

/// Writes `value` as indented JSON followed by a newline.
pub fn json<T: Serialize + ?Sized>(value: &T) -> Result<()> {
    let mut out = io::stdout().lock();
    serde_json::to_writer_pretty(&mut out, value)?;
    writeln!(out)?;
    Ok(())
}

/// Writes rows under a header, each column as wide as its widest cell.
pub fn table(headers: &[&str], rows: &[Vec<String>]) -> Result<()> {
    table_styled(headers, rows, |_, cell| cell.to_string())
}

/// Like [`table`], with `style(column, cell)` deciding how each body cell is
/// drawn. Widths are measured on the plain text, so colour codes never
/// misalign a column.
pub fn table_styled(
    headers: &[&str],
    rows: &[Vec<String>],
    style: impl Fn(usize, &str) -> String,
) -> Result<()> {
    let mut out = io::stdout().lock();
    out.write_all(render_table(headers, rows, style).as_bytes())?;
    Ok(())
}

fn render_table(
    headers: &[&str],
    rows: &[Vec<String>],
    style: impl Fn(usize, &str) -> String,
) -> String {
    let mut widths: Vec<usize> = headers.iter().map(|h| h.chars().count()).collect();
    for row in rows {
        for (w, cell) in widths.iter_mut().zip(row) {
            *w = (*w).max(cell.chars().count());
        }
    }
    let mut s = String::new();
    let mut line = |cells: Vec<(&str, String)>| {
        let last = cells.len().saturating_sub(1);
        for (i, (plain, drawn)) in cells.iter().enumerate() {
            s.push_str(drawn);
            // The last column is not padded, so lines carry no trailing spaces.
            if i != last {
                s.push_str(&" ".repeat(widths[i] - plain.chars().count() + 2));
            }
        }
        s.push('\n');
    };
    line(headers.iter().map(|h| (*h, h.to_string())).collect());
    for row in rows {
        line(
            row.iter()
                .enumerate()
                .map(|(i, c)| (c.as_str(), style(i, c)))
                .collect(),
        );
    }
    s
}

/// Writes labelled values one per line, labels aligned, indented under a
/// title the caller has printed.
pub fn fields(pairs: &[(&str, String)]) -> Result<()> {
    let width = pairs.iter().map(|(k, _)| k.len()).max().unwrap_or(0);
    let mut out = io::stdout().lock();
    for (k, v) in pairs {
        writeln!(out, "  {}  {v}", dim(&format!("{k:<width$}")))?;
    }
    Ok(())
}

/// Whether stdout takes colour: a terminal, and NO_COLOR unset.
fn colour() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| {
        io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none_or(|v| v.is_empty())
    })
}

fn paint(code: &str, text: &str) -> String {
    if colour() {
        format!("\x1b[{code}m{text}\x1b[0m")
    } else {
        text.to_string()
    }
}

pub fn bold(text: &str) -> String {
    paint("1", text)
}

pub fn dim(text: &str) -> String {
    paint("2", text)
}

/// An application's state, coloured by how worried to be about it.
pub fn state(state: &str) -> String {
    let code = match state {
        "running" => "32",
        "error" => "31",
        "stopped" | "not deployed" => "2",
        _ => "33",
    };
    paint(code, state)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn columns_align_without_trailing_spaces() {
        let rows = vec![
            vec!["blog".into(), "blog.example.com".into(), "running".into()],
            vec!["grafana-long".into(), "g.io".into(), "stopped".into()],
        ];
        let got = render_table(&["NAME", "HOST", "STATE"], &rows, |_, c| c.to_string());
        assert_eq!(
            got,
            "NAME          HOST              STATE\n\
             blog          blog.example.com  running\n\
             grafana-long  g.io              stopped\n"
        );
    }

    #[test]
    fn styling_does_not_shift_columns() {
        let rows = vec![vec!["a".into(), "x".into()], vec!["bbb".into(), "y".into()]];
        let got = render_table(&["N", "S"], &rows, |col, c| {
            if col == 0 {
                format!("<{c}>")
            } else {
                c.to_string()
            }
        });
        assert_eq!(got, "N    S\n<a>    x\n<bbb>  y\n");
    }
}
