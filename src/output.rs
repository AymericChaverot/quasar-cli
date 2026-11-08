//! How results reach the terminal: aligned tables for people, JSON for
//! scripts.

use std::io::{self, Write};

use anyhow::Result;
use serde::Serialize;

/// Writes `value` as indented JSON followed by a newline.
pub fn json<T: Serialize + ?Sized>(value: &T) -> Result<()> {
    let mut out = io::stdout().lock();
    serde_json::to_writer_pretty(&mut out, value)?;
    writeln!(out)?;
    Ok(())
}

/// Writes rows under a header, each column as wide as its widest cell. The
/// last column is not padded, so lines carry no trailing spaces.
pub fn table(headers: &[&str], rows: &[Vec<String>]) -> Result<()> {
    let mut out = io::stdout().lock();
    out.write_all(render_table(headers, rows).as_bytes())?;
    Ok(())
}

fn render_table(headers: &[&str], rows: &[Vec<String>]) -> String {
    let mut widths: Vec<usize> = headers.iter().map(|h| h.chars().count()).collect();
    for row in rows {
        for (w, cell) in widths.iter_mut().zip(row) {
            *w = (*w).max(cell.chars().count());
        }
    }
    let mut s = String::new();
    let mut line = |cells: &mut dyn Iterator<Item = &str>| {
        let cells: Vec<&str> = cells.collect();
        for (i, cell) in cells.iter().enumerate() {
            if i + 1 == cells.len() {
                s.push_str(cell);
            } else {
                s.push_str(cell);
                s.push_str(&" ".repeat(widths[i] - cell.chars().count() + 2));
            }
        }
        s.push('\n');
    };
    line(&mut headers.iter().copied());
    for row in rows {
        line(&mut row.iter().map(String::as_str));
    }
    s
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
        let got = render_table(&["NAME", "HOST", "STATE"], &rows);
        assert_eq!(
            got,
            "NAME          HOST              STATE\n\
             blog          blog.example.com  running\n\
             grafana-long  g.io              stopped\n"
        );
    }
}
