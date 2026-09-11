//! Go's `text/tabwriter`.
//!
//! Reproduced rather than mapped onto a Rust table crate because the exact
//! column widths are pinned by `testdata/default_table.txt`, which is compared
//! byte-for-byte.
//!
//! Cells are separated by tabs and lines by newlines. A column is formed only
//! from cells that are *not* the last cell on their line, so the text after the
//! final tab is never padded. Each column is as wide as its widest cell plus
//! the padding, floored at the minimum width.

use std::io::{self, Write};

/// A buffering writer that aligns tab-separated cells into columns.
pub struct TabWriter<W: Write> {
    inner: W,
    buf: String,
    minwidth: usize,
    padding: usize,
    padchar: char,
}

impl<W: Write> TabWriter<W> {
    /// Mirrors Go's `tabwriter.NewWriter(out, minwidth, tabwidth, padding, padchar, 0)`.
    ///
    /// `tabwidth` only affects the `TabIndent` mode, which `envconfig` does not
    /// use, so it is not represented here.
    pub fn new(inner: W, minwidth: usize, padding: usize, padchar: char) -> Self {
        Self {
            inner,
            buf: String::new(),
            minwidth,
            padding,
            padchar,
        }
    }

    /// Formats everything buffered so far and writes it to the inner writer.
    pub fn flush(&mut self) -> io::Result<()> {
        let text = std::mem::take(&mut self.buf);
        if text.is_empty() {
            return self.inner.flush();
        }
        let lines: Vec<Vec<&str>> = text.split('\n').map(|l| l.split('\t').collect()).collect();
        let mut out = String::with_capacity(text.len());
        let mut widths: Vec<usize> = Vec::new();
        self.format(&lines, &mut widths, 0, lines.len(), &mut out);
        self.inner.write_all(out.as_bytes())?;
        self.inner.flush()
    }

    /// Go's `Writer.format`: finds column blocks, computes each column's width,
    /// then recurses into the columns to its right.
    fn format(
        &self,
        lines: &[Vec<&str>],
        widths: &mut Vec<usize>,
        line0: usize,
        line1: usize,
        out: &mut String,
    ) {
        let column = widths.len();
        let mut l0 = line0;
        let mut this = line0;
        while this < line1 {
            if column + 1 < lines[this].len() {
                // Lines before the block are written with the columns known so far.
                self.write_lines(lines, widths, l0, this, out);
                l0 = this;

                let mut width = self.minwidth;
                while this < line1 && column + 1 < lines[this].len() {
                    let w = cell_width(lines[this][column]) + self.padding;
                    if w > width {
                        width = w;
                    }
                    this += 1;
                }

                widths.push(width);
                self.format(lines, widths, l0, this, out);
                widths.pop();
                l0 = this;
            }
            this += 1;
        }
        self.write_lines(lines, widths, l0, line1, out);
    }

    /// Go's `Writer.writeLines`: pads every cell that belongs to a known
    /// column, and terminates every line but the last buffered one.
    fn write_lines(
        &self,
        lines: &[Vec<&str>],
        widths: &[usize],
        line0: usize,
        line1: usize,
        out: &mut String,
    ) {
        for (i, line) in lines.iter().enumerate().take(line1).skip(line0) {
            for (j, cell) in line.iter().enumerate() {
                out.push_str(cell);
                if let Some(&w) = widths.get(j) {
                    for _ in cell_width(cell)..w {
                        out.push(self.padchar);
                    }
                }
            }
            if i != lines.len() - 1 {
                out.push('\n');
            }
        }
    }
}

fn cell_width(cell: &str) -> usize {
    cell.chars().count()
}

impl<W: Write> Write for TabWriter<W> {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        let s =
            std::str::from_utf8(data).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        self.buf.push_str(s);
        Ok(data.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        TabWriter::flush(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(input: &str) -> String {
        let mut out: Vec<u8> = Vec::new();
        {
            let mut w = TabWriter::new(&mut out, 1, 4, ' ');
            w.write_all(input.as_bytes()).unwrap();
            TabWriter::flush(&mut w).unwrap();
        }
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn pads_columns_to_widest_cell_plus_padding() {
        // Columns: "a"/"cccc" -> 4+4 = 8; trailing cells are never padded.
        assert_eq!(render("a\tb\ncccc\td\n"), "a       b\ncccc    d\n");
    }

    #[test]
    fn trailing_cell_is_not_padded() {
        let out = render("KEY\tTYPE\n");
        assert_eq!(out, "KEY    TYPE\n");
        assert!(!out.ends_with(' '));
    }

    #[test]
    fn lines_without_tabs_pass_through_unpadded() {
        assert_eq!(render("header\n\na\tb\n"), "header\n\na    b\n");
    }

    #[test]
    fn a_line_with_fewer_cells_breaks_the_column_block() {
        // "zz" has no second cell, so it does not widen column 0, and the
        // block below it is measured independently.
        assert_eq!(render("a\tb\nzz\nccc\td\n"), "a    b\nzz\nccc    d\n");
    }

    #[test]
    fn minimum_width_is_respected() {
        let mut out: Vec<u8> = Vec::new();
        {
            let mut w = TabWriter::new(&mut out, 10, 0, ' ');
            w.write_all(b"a\tb\n").unwrap();
            TabWriter::flush(&mut w).unwrap();
        }
        assert_eq!(String::from_utf8(out).unwrap(), "a         b\n");
    }

    #[test]
    fn no_trailing_newline_is_added() {
        // The final buffered line is still padded, but gets no newline.
        assert_eq!(render("a\tb"), "a    b");
    }
    #[test]
    fn flushing_an_empty_writer_writes_nothing() {
        assert_eq!(render(""), "");
    }

    #[test]
    fn write_trait_flush_formats_too() {
        let mut out: Vec<u8> = Vec::new();
        {
            let mut w = TabWriter::new(&mut out, 1, 4, ' ');
            w.write_all(b"a\tb\n").unwrap();
            // Go through the io::Write::flush path rather than the inherent one.
            Write::flush(&mut w).unwrap();
        }
        assert_eq!(String::from_utf8(out).unwrap(), "a    b\n");
    }

    #[test]
    fn a_different_pad_character_is_used() {
        let mut out: Vec<u8> = Vec::new();
        {
            let mut w = TabWriter::new(&mut out, 1, 2, '.');
            w.write_all(b"a\tb\n").unwrap();
            TabWriter::flush(&mut w).unwrap();
        }
        assert_eq!(String::from_utf8(out).unwrap(), "a..b\n");
    }

    #[test]
    fn three_columns_align_independently() {
        assert_eq!(
            render("a\tbbbb\tc\nddd\te\tf\n"),
            "a      bbbb    c\nddd    e       f\n"
        );
    }
}
