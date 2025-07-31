#![allow(dead_code)]

use memmap2::Mmap;
use std::fs::File;
use std::io;
use std::path::Path;
use log::{debug};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchDirection {
    Forward,
    Backward,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Line<'a> {
    pub line_number: usize,
    pub data: &'a str,
}

impl<'a> Line<'a> {
    pub fn new(line_number: usize, data: &'a str) -> Self {
        Line { line_number, data }
    }
}

pub struct LogFile {
    mmap: Mmap,
    line_starts: Vec<usize>,
    line_lengths: Vec<usize>,
    line_visibility: Vec<bool>,
    backup_visibility: Option<Vec<bool>>,
    total_lines: usize,
}

impl LogFile {
    /// Create a new LogFile from a file path
    pub fn new<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let file = File::open(path)?;
        let mmap = unsafe { Mmap::map(&file)? };

        if mmap.is_empty() {
            return Ok(LogFile {
                mmap,
                line_starts: vec![0],
                line_lengths: vec![0],
                line_visibility: vec![true],
                backup_visibility: None,
                total_lines: 1,
            });
        }

        // Build line index by scanning for newlines
        let mut line_starts = vec![0]; // First line starts at 0
        let mut line_lengths = Vec::new();

        for (pos, &byte) in mmap.iter().enumerate() {
            if byte == b'\n' {
                let mut len = pos - line_starts.last().unwrap();
                if len > 0 && mmap[pos - 1] == b'\r' {
                    len -= 1; // Adjust for CRLF
                }
                line_lengths.push(len);
                line_starts.push(pos + 1); // Start of next line is after the newline
            }
        }

        // Push the length of the last line if the file doesn't end with a newline
        if let Some(&last_start) = line_starts.last() {
            if last_start < mmap.len() {
                line_lengths.push(mmap.len() - last_start);
            }
        }

        // Remove the last entry if it points past the end of file
        if line_starts.last() == Some(&mmap.len()) {
            line_starts.pop();
        }

        let total_lines = line_starts.len();
        let line_visibility = vec![true; total_lines];

        Ok(LogFile {
            mmap,
            line_starts,
            line_lengths,
            line_visibility,
            backup_visibility: None,
            total_lines,
        })
    }

    /// Get the total number of lines in the file
    pub fn total_lines(&self) -> usize {
        self.total_lines
    }

    /// Get the number of currently visible lines
    pub fn visible_lines(&self) -> usize {
        self.line_visibility
            .iter()
            .filter(|&&visible| visible)
            .count()
    }

    /// Get a line by index (0-based)
    pub fn get_line(&self, line_idx: usize) -> Option<&str> {
        if line_idx >= self.total_lines {
            return None;
        }

        let start = self.line_starts[line_idx];
        let mut end = if line_idx + 1 < self.total_lines {
            // Not the last line - go up to the next line start minus 1
            self.line_starts[line_idx + 1].saturating_sub(1)
        } else {
            // Last line - go to end of file
            self.mmap.len()
        };

        if start > end {
            return None;
        }

        // Remove trailing newlines from the line slice
        while end > start && (self.mmap[end - 1] == b'\n' || self.mmap[end - 1] == b'\r') {
            end -= 1;
        }
        // Convert bytes to string, handling potential UTF-8 issues gracefully
        std::str::from_utf8(&self.mmap[start..end]).ok()
    }

    /// Check if a line is visible
    pub fn is_line_visible(&self, line_idx: usize) -> bool {
        self.line_visibility.get(line_idx).copied().unwrap_or(false)
    }

    /// Hide a line
    pub fn hide_line(&mut self, line_idx: usize) {
        if line_idx < self.total_lines {
            self.line_visibility[line_idx] = false;
        }
    }

    /// Show a line
    pub fn show_line(&mut self, line_idx: usize) {
        if line_idx < self.total_lines {
            self.line_visibility[line_idx] = true;
        }
    }

    /// Hide all lines
    pub fn hide_all(&mut self) {
        self.line_visibility.fill(false);
    }

    /// Show all lines
    pub fn show_all(&mut self) {
        self.line_visibility.fill(true);
    }

    pub fn show_single_line(&mut self, line_idx: usize) {
        if line_idx < self.total_lines {
            self.backup_visibility = Some(self.line_visibility.clone());
            self.line_visibility.fill(false);
            self.line_visibility[line_idx] = true;
        }
    }

    /// Hide lines matching a predicate
    pub fn hide_lines_matching<F>(&mut self, predicate: F)
    where
        F: Fn(&str) -> bool,
    {
        for i in 0..self.total_lines {
            if let Some(line) = self.get_line(i) {
                if predicate(line) {
                    self.line_visibility[i] = false;
                }
            }
        }
    }

    /// Show lines matching a predicate
    pub fn show_lines_matching<F>(&mut self, predicate: F)
    where
        F: Fn(&str) -> bool,
    {
        for i in 0..self.total_lines {
            if let Some(line) = self.get_line(i) {
                if predicate(line) {
                    self.line_visibility[i] = true;
                } else {
                    self.line_visibility[i] = false;
                }
            }
        }
    }

    /// Get a range of visible lines for display
    pub fn get_visible_lines(&self, start_indx: usize, count: usize) -> Vec<Line> {
        let mut result = Vec::new();
        let mut visible_count = 0;


        let indx = start_indx.min(self.total_lines);

        for i in indx..self.total_lines {
            if !self.is_line_visible(i) {
                continue; // Skip hidden lines
            }

            if let Some(line) = self.get_line(i) {
                result.push(Line::new(i, line));
                visible_count += 1;
                if visible_count >= count {
                    break;
                }
            }
        }

        // If we didn't find enough visible lines, try to find the last visible line before start_indx
        if result.is_empty() {
            for i in (0..start_indx).rev() {
                if self.is_line_visible(i) {
                    if let Some(line) = self.get_line(i) {
                        result.push(Line::new(i, line));
                        break;
                    }
                }
            }
        } 

        if result.is_empty() {
            result.push(Line::new(0, "No visible lines"));
        }

        result
    }

    pub fn num_lines_to_print(line_len: usize, cols: usize, max_lines:usize, rows_left: usize) -> usize {
        let num_lines_to_print = if line_len == 0 {
            1
        } else {
            line_len / cols + if line_len % cols > 0 { 1 } else { 0 }
        };
        num_lines_to_print.min(max_lines).min(rows_left)
    }

    pub fn get_end_of_file(&self, rows: usize, cols: usize, max_row_per_line: usize) -> (usize, usize) {
        self.get_pos_from_end_line(self.total_lines, rows, cols, max_row_per_line)
    }

    pub fn get_pos_from_end_line(&self, end_pos: usize, rows: usize, cols: usize, max_row_per_line: usize) -> (usize, usize) {

        let mut start_line = None;
        let mut end_line = None;
        for i in (0..end_pos).rev() {
            if self.is_line_visible(i) {
                end_line = Some(i);
                break;
            }
        }

        if let Some(end_line) = end_line {
            let mut row_count = Self::num_lines_to_print(
                self.line_lengths[end_line],
                cols,
                max_row_per_line,
                rows,
            );

            for i in (0..end_line).rev() {
                if self.is_line_visible(i) {
                    row_count += Self::num_lines_to_print(
                        self.line_lengths[i],
                        cols,
                        max_row_per_line,
                        max_row_per_line,
                    );
                }

		debug!("EOF row_count {}  rows{}", row_count, rows);
		if row_count <= rows {
		    start_line = Some(i);
		}

		if row_count >= rows {
                    break;
                }
            }

            if let Some(start_line) = start_line {
	        debug!("Get end of file {} {}", start_line, end_line);
                return (start_line, end_line);
            } else {
                return (end_line, end_line); 
            }

        } else {
            (0, 0) // No visible lines
        }
    }

    pub fn search(
        &self,
        pattern: &str,
        line_num: usize,
        search_current_line: bool,
        direction: SearchDirection,
    ) -> Option<usize> {

        let offset = if search_current_line {
            0
        } else {
            1 // Start searching from the next line
        };


        match direction {
            SearchDirection::Forward => {
                for i in (line_num + offset)..self.total_lines {
                    
                    if !self.is_line_visible(i) {
                        continue; // Skip hidden lines
                    }

                    if let Some(line) = self.get_line(i) {
                        debug!("Checking line {}: {}", i, line);
                        if line.contains(pattern) {
                            debug!("Found pattern '{}' in line {}", pattern, i);
                            return Some(i);
                        }
                    }
                }
            }
            SearchDirection::Backward => {
                for i in (0..=(line_num - offset)).rev() {
                    if !self.is_line_visible(i) {
                        continue; // Skip hidden lines
                    }

                    if let Some(line) = self.get_line(i) {
                        if line.contains(pattern) {
                            return Some(i);
                        }
                    }
                }
            }
        }
        None
    }

    /// Get file size in bytes
    pub fn file_size(&self) -> usize {
        self.mmap.len()
    }

    /// Get the raw bytes for a line (useful for binary data or non-UTF8)
    pub fn get_line_bytes(&self, line_idx: usize) -> Option<&[u8]> {
        if line_idx >= self.total_lines {
            return None;
        }

        let start = self.line_starts[line_idx];
        let end = if line_idx + 1 < self.total_lines {
            self.line_starts[line_idx + 1].saturating_sub(1)
        } else {
            self.mmap.len()
        };

        if start > end {
            return None;
        }

        Some(&self.mmap[start..end])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn create_test_file(content: &str) -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();
        file.flush().unwrap();
        file
    }

    #[test]
    fn test_basic_functionality() {
        let test_content = "Line 1\nLine 2\nLine 3\n";
        let file = create_test_file(test_content);

        let viewer = LogFile::new(file.path()).unwrap();

        assert_eq!(viewer.total_lines(), 3);
        assert_eq!(viewer.get_line(0), Some("Line 1"));
        assert_eq!(viewer.get_line(1), Some("Line 2"));
        assert_eq!(viewer.get_line(2), Some("Line 3"));
        assert_eq!(viewer.get_line(3), None);
    }

    #[test]
    fn test_hide_show_lines() {
        let test_content = "Line 1\nLine 2\nLine 3\n";
        let file = create_test_file(test_content);

        let mut viewer = LogFile::new(file.path()).unwrap();

        assert_eq!(viewer.visible_lines(), 3);

        viewer.hide_line(1);
        assert_eq!(viewer.visible_lines(), 2);
        assert!(!viewer.is_line_visible(1));

        viewer.show_line(1);
        assert_eq!(viewer.visible_lines(), 3);
        assert!(viewer.is_line_visible(1));
    }

    #[test]
    fn test_get_visible_lines() {
        let test_content = "Line 1\nLine 2\nLine 3\nLine 4\n";
        let file = create_test_file(test_content);

        let mut viewer = LogFile::new(file.path()).unwrap();
        viewer.hide_line(1); // Hide "Line 2"
        let visible = viewer.get_visible_lines(0, 10);
        assert_eq!(visible.len(), 3);
        assert_eq!(visible[0], Line::new(0, "Line 1"));
        assert_eq!(visible[1], Line::new(2, "Line 3"));
        assert_eq!(visible[2], Line::new(3, "Line 4"));
    }

    #[test]
    fn test_search() {
        let test_content = "Error: something bad
Info: all good
Error: another issue
";
        let file = create_test_file(test_content);

        let viewer = LogFile::new(file.path()).unwrap();

        let error_lines = viewer.search("Error", 0, true, SearchDirection::Forward);
        assert_eq!(error_lines, Some(0));


        let info_lines = viewer.search("Info", 0, true, SearchDirection::Forward);
        assert_eq!(info_lines, Some(1));
    }

    #[test]
    fn test_line_lengths() {
        // Test with \n line endings
        let test_content_lf = "line 1\nline 22\nline 333\n";
        let file_lf = create_test_file(test_content_lf);
        let viewer_lf = LogFile::new(file_lf.path()).unwrap();
        assert_eq!(viewer_lf.line_lengths, vec![6, 7, 8]);

        // Test with \r\n line endings
        let test_content_crlf = "line 1\r\nline 22\r\nline 333\r\n";
        let file_crlf = create_test_file(test_content_crlf);
        let viewer_crlf = LogFile::new(file_crlf.path()).unwrap();
        assert_eq!(viewer_crlf.line_lengths, vec![6, 7, 8]);

        // Test with mixed line endings
        let test_content_mixed = "line 1\nline 22\r\nline 333\n";
        let file_mixed = create_test_file(test_content_mixed);
        let viewer_mixed = LogFile::new(file_mixed.path()).unwrap();
        assert_eq!(viewer_mixed.line_lengths, vec![6, 7, 8]);

        // Test with no trailing newline
        let test_content_no_newline = "line 1\nline 22";
        let file_no_newline = create_test_file(test_content_no_newline);
        let viewer_no_newline = LogFile::new(file_no_newline.path()).unwrap();
        assert_eq!(viewer_no_newline.line_lengths, vec![6, 7]);

        // Test with empty file
        let test_content_empty = "";
        let file_empty = create_test_file(test_content_empty);
        let viewer_empty = LogFile::new(file_empty.path()).unwrap();
        assert_eq!(viewer_empty.line_lengths, vec![0]);
    }

}
