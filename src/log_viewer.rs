use anyhow::Result;
use crossterm::{
    cursor,
    style::{Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor, Stylize},
    terminal::*,
    QueueableCommand,
};
use log::debug;
use std::io::{stdout, Write};

use crate::log_file::Line;

pub struct LogViewer {
    stdout: std::io::Stdout,
    cursor_position: (u16, u16),
    pub search_pattern: Option<String>,
    search_color: Color,
    unused_colors: Vec<Color>,
    highlight: Vec<(String, Color)>,
}

impl LogViewer {
    pub fn new() -> Self {
        let unused_colors = vec![
            Color::Red,
            Color::Green,
            Color::Yellow,
            Color::Blue,
            Color::Magenta,
            Color::Cyan,
            Color::DarkYellow,   
            Color::DarkCyan,
            Color::DarkGreen
        ];

        LogViewer {
            stdout: stdout(),
            cursor_position: (0, 0),
            search_pattern: None,
            search_color: Color::Red,
            unused_colors,
            highlight: Vec::new(),
        }
    }

    pub fn clear(&mut self) -> Result<()> {
        self.stdout.queue(Clear(ClearType::All))?;
        Ok(())
    }

    pub fn get_row_cols(&self) -> Result<(usize, usize)> {
        let size = window_size()?;
        // Save 1 row for the input bar
        Ok((size.rows as usize - 1, size.columns as usize))
    }

    pub fn set_search_color(&mut self, color: &str) {
        if let Ok(color) = Color::try_from(color) {
            self.search_color = color;
        } else {
            debug!("Invalid color string: {}", color);
        }
    }

    pub fn set_cursor(&mut self, x: u16, y: u16) -> Result<()> {
        let (rows, cols) = self.get_row_cols()?;

        // Ensure cursor position is within bounds
        let x = if x >= cols as u16 { cols as u16 - 1 } else { x };
        let y = if y >= rows as u16 { rows as u16 - 1 } else { y };

        // Update cursor position
        self.cursor_position = (x, y);
        self.stdout.queue(cursor::MoveTo(x, y))?;
        self.stdout.flush()?;
        Ok(())
    }

    pub fn set_cursor_to_command_line(&mut self) -> Result<()> {
        let (rows, _) = self.get_row_cols()?;

        debug!("Setting cursor to command line at row: {}", rows);

        self.stdout.queue(cursor::MoveTo(0, rows as u16))?;
        self.stdout.flush()?;
        Ok(())
    }

    pub fn clear_command_line(&mut self) -> Result<()> {
        let (rows, _) = self.get_row_cols()?;
        self.stdout.queue(cursor::MoveTo(0, rows as u16))?;
        self.stdout.queue(Clear(ClearType::CurrentLine))?;
        self.stdout.flush()?;
        Ok(())
    }

    pub fn print_line_with_highlight(&mut self, line_str: &str) -> Result<()> {
        // Collect foreground matches
        let mut fg_matches = Vec::new();
        for (pattern, color) in &self.highlight {
            let mut search_start = 0;
            while let Some(start) = line_str[search_start..].find(pattern) {
                let abs_start = search_start + start;
                let abs_end = abs_start + pattern.len();
                fg_matches.push((abs_start, abs_end, *color));
                search_start = abs_end;
            }
        }

        // Collect background matches (search pattern)
        let mut bg_matches = Vec::new();
        if let Some(pattern) = &self.search_pattern {
            let mut search_start = 0;
            while let Some(start) = line_str[search_start..].find(pattern) {
                let abs_start = search_start + start;
                let abs_end = abs_start + pattern.len();
                bg_matches.push((abs_start, abs_end));
                search_start = abs_end;
            }
        }

        // Sort both by position
        fg_matches.sort_by_key(|(start, _, _)| *start);
        bg_matches.sort_by_key(|(start, _)| *start);

        // Create a list of all position changes (start/end of any match)
        let mut positions = std::collections::BTreeSet::new();
        positions.insert(0); // Always start at beginning
        positions.insert(line_str.len()); // Always end at the end

        for (start, end, _) in &fg_matches {
            positions.insert(*start);
            positions.insert(*end);
        }
        for (start, end) in &bg_matches {
            positions.insert(*start);
            positions.insert(*end);
        }

        // Convert to sorted vector for easier iteration
        let positions: Vec<usize> = positions.into_iter().collect();

        // Process each segment between position changes
        for i in 0..positions.len() - 1 {
            let start_pos = positions[i];
            let end_pos = positions[i + 1];

            if start_pos >= end_pos {
                continue; // Skip invalid ranges
            }

            // Determine current styling for this segment
            let current_bg = bg_matches
                .iter()
                .find(|(start, end)| start_pos >= *start && start_pos < *end);
            let current_fg = fg_matches
                .iter()
                .find(|(start, end, _)| start_pos >= *start && start_pos < *end);

            // Apply styling
            if let Some(_) = current_bg {
                self.stdout.queue(SetBackgroundColor(Color::Red))?;
            }
            if let Some((_, _, color)) = current_fg {
                self.stdout.queue(SetForegroundColor(*color))?;
            }

            // Print the text segment
            self.stdout.queue(Print(&line_str[start_pos..end_pos]))?;

            // Reset colors if any were applied
            if current_bg.is_some() || current_fg.is_some() {
                self.stdout.queue(ResetColor)?;
            }
        }

        Ok(())
    }

    pub fn print_screen(&mut self, lines: &[Line]) -> Result<Vec<usize>> {
        let (mut rows, cols) = self.get_row_cols()?;
        self.stdout.queue(cursor::MoveTo(0, 0))?;

        let mut line_numbers: Vec<usize> = Vec::new();

        for line in lines.iter() {
            let line_len = line.data.len();

            let mut num_lines_to_print = if line_len == 0 {
	        1
	    } else {
	        line_len / cols + if line_len % cols > 0 { 1 } else { 0 }
            };
            num_lines_to_print = num_lines_to_print.min(3).min(rows);
	    
            if line_len > num_lines_to_print * cols {
                // Truncate long lines
                let end_pos = num_lines_to_print * cols - 5; // Reserve space for "..."
                self.print_line_with_highlight(&line.data[..end_pos])?;
                //self.stdout.queue(Print(&line.data[..end_pos]))?;
                self.stdout.queue(Print("[...]\r\n".red()))?;
            } else {
                self.print_line_with_highlight(&line.data)?;
                self.stdout.queue(Print("\r\n"))?;
            }

            for _ in 0..num_lines_to_print {
                line_numbers.push(line.line_number);
            }

            if num_lines_to_print >= rows {
                break;
            }

            rows -= num_lines_to_print;
        }

        self.stdout.queue(cursor::MoveTo(
            self.cursor_position.0,
            self.cursor_position.1,
        ))?;
        self.stdout.flush()?;

        Ok(line_numbers)
    }

    pub fn set_highlight(&mut self, pattern: String, color_str: Option<String>) -> Result<()> {
        if let Some(color_str) = color_str {
            if let Ok(color) = Color::try_from(color_str.as_str()) {
                self.highlight.push((pattern, color));
                self.unused_colors.retain(|c| *c != color);
                Ok(())
            } else {
                Err(anyhow::anyhow!("Invalid color string: {}", color_str))
            }
        } else {
            if let Some(color) = self.unused_colors.pop() {
                self.highlight.push((pattern, color));
                Ok(())
            } else {
                Err(anyhow::anyhow!("No unused colors available"))
            }
        }
    }
}
