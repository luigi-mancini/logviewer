use crate::command_handler::handle_command;
use crate::log_file;
use crate::log_viewer;
use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use std::io::Write;
use tempfile::NamedTempFile;

use log::debug;

enum ViewMode {
    Normal,
    Expanded,
}

struct ViewState {
    start_line: usize,
    end_line: usize,
    cursor: (u16, u16),
}

pub struct Controller {
    log_file: log_file::LogFile,
    log_viewer: log_viewer::LogViewer,
    running: bool,
    start_line: usize,
    end_line: usize,
    rows: usize,
    cols: usize,
    cursor: (u16, u16),
    line_numbers: Vec<usize>,
    temp_file: Option<NamedTempFile>,
    expanded_log_file: Option<log_file::LogFile>,
    mode: ViewMode,
    normal_view_state: ViewState,
}

impl Controller {
    pub fn new(log_file_path: &str) -> anyhow::Result<Self> {
        let log_file = log_file::LogFile::new(log_file_path)?;
        let log_viewer = log_viewer::LogViewer::new();
        let (rows, cols) = log_viewer.get_row_cols()?;

        Ok(Controller {
            log_file,
            log_viewer,
            running: true,
            start_line: 0,
            end_line: 0,
            rows,
            cols,
            cursor: (0, 0),
            line_numbers: Vec::new(),
            temp_file: None,
            expanded_log_file: None,
            mode: ViewMode::Normal,
            normal_view_state: ViewState {
                start_line: 0,
                end_line: 0,
                cursor: (0, 0),
            },
        })
    }

    pub fn run(&mut self) -> anyhow::Result<()> {
        let (rows, _) = self.log_viewer.get_row_cols()?;
        self.start_line = 0;
        self.end_line = rows;

        self.draw()?;
        enable_raw_mode()?;

        while self.running {
            let mut redraw = false;

            // Check for events with timeout
            if event::poll(std::time::Duration::from_millis(100))? {
                match event::read()? {
                    Event::Key(key) => {
                        redraw = self.handle_key_event(key)?;
                    }
                    Event::Resize(width, height) => {
                        self.handle_resize(width, height)?;
                        redraw = true; // Redraw on resize
                    }
                    _ => {}
                }

                // Redraw after handling event
                if redraw && self.running {
                    self.draw()?;
                }
            }
        }

        disable_raw_mode()?;

        Ok(())
    }

    fn get_active_log_file(&self) -> &log_file::LogFile {
        match self.mode {
            ViewMode::Normal => &self.log_file,
            ViewMode::Expanded => self.expanded_log_file.as_ref().unwrap(),
        }
    }

    fn get_current_line_number(&self) -> usize {
        if self.cursor.1 as usize >= self.line_numbers.len() {
            return 0; // Default to 0 if cursor is out of bounds
        }
        self.line_numbers[self.cursor.1 as usize]
    }

    fn switch_to_expanded_mode(&mut self) -> Result<()> {
        // Save current state
        self.normal_view_state = ViewState {
            start_line: self.start_line,
            end_line: self.end_line,
            cursor: self.cursor,
        };

        // Create temp file and write the line to it
        let mut temp_file = NamedTempFile::new()?;
        let line_content = self
            .log_file
            .get_line(self.get_current_line_number())
            .unwrap_or_default();

        let bytes = line_content.as_bytes();

        for chunk in bytes.chunks(self.cols) {
            temp_file.write_all(chunk)?;
            temp_file.write_all(b"\n")?; // Add newline after each chunk
        }

        let temp_path = temp_file.path().to_str().unwrap().to_string();

        debug!(
            "Switching to expanded mode. Temp file created at: {}",
            temp_path
        );

        self.temp_file = Some(temp_file);

        // Create new LogFile for the temp file
        let new_log_file = log_file::LogFile::new(&temp_path)?;
        self.expanded_log_file = Some(new_log_file);

        // Switch mode and reset view
        self.mode = ViewMode::Expanded;
        self.start_line = 0;
        self.end_line = self.rows;
        self.cursor = (0, 0);

        Ok(())
    }

    fn switch_to_normal_mode(&mut self) {
        // Restore state
        self.start_line = self.normal_view_state.start_line;
        self.end_line = self.normal_view_state.end_line;
        self.cursor = self.normal_view_state.cursor;

        // Switch mode and clear expanded view resources
        self.mode = ViewMode::Normal;
        self.expanded_log_file = None;
        self.temp_file = None;
    }

    fn command_mode(&mut self, key: Option<char>) -> Result<Option<usize>> {
        self.log_viewer.set_cursor_to_command_line()?;
        
        let mut input = String::new();

        if let Some(c) = key {
            input.push(c);
            print!("{}", c);
            std::io::stdout().flush()?;
        }

        loop {
            if let Event::Key(KeyEvent { code, .. }) = event::read()? {
                match code {
                    KeyCode::Char(c) => {
                        input.push(c);
                        print!("{}", c);
                        std::io::stdout().flush()?;
                    }
                    KeyCode::Backspace => {
                        if !input.is_empty() {
                            input.pop();
                            print!(" ");
                            std::io::stdout().flush()?;
                        }
                    }
                    KeyCode::Enter => {
                        self.log_viewer.clear_command_line()?;
                        return handle_command(
                            &input,
                            self.get_current_line_number(),
                            &mut self.log_file,
                            &mut self.log_viewer,
                        );
                    }
                    KeyCode::Esc => {
                        self.log_viewer.clear_command_line()?;
                        break;
                    }
                    _ => {}
                }
            }
        }
        Ok(None)
    }

    fn handle_key_event(&mut self, key: KeyEvent) -> Result<bool> {
        let mut redraw = true;

        let key_char = match key.code {
            KeyCode::Char(c) => Some(c),
            _ => None, // Not a printable char
        };

        match key.code {
            KeyCode::Esc | KeyCode::Char('/') | KeyCode::Char('?') => {
                
                if let Some(val) = self.command_mode(key_char)? {
                    debug!("Command mode returned with value: {}", val);

                    self.start_line = val;
                    self.end_line =
                        (self.start_line + self.rows).min(self.get_active_log_file().total_lines());
                    self.cursor = (0, 0);
                    self.log_viewer.set_cursor(self.cursor.0, self.cursor.0)?;
                } else {
                    debug!("Exiting command mode");
                    self.log_viewer.set_cursor(self.cursor.0, self.cursor.1)?;
                }
            }
            KeyCode::Char('q') => {
                self.log_viewer.clear()?;
                self.log_viewer.set_cursor(0, 0)?;
                self.running = false; // Exit on 'q'
            }
            KeyCode::Char('j') | KeyCode::Down => {
                redraw = self.move_cursor(0, 1)?; // Move cursor down
            }
            KeyCode::Char('k') | KeyCode::Up => {
                redraw = self.move_cursor(0, -1)?; // Move cursor up
            }
            KeyCode::Char('h') | KeyCode::Left => {
                redraw = self.move_cursor(-1, 0)?; // Move cursor left
            }
            KeyCode::Char('l') | KeyCode::Right => {
                redraw = self.move_cursor(1, 0)?; // Move cursor right
            }
            KeyCode::Char(' ') if key.modifiers.contains(event::KeyModifiers::CONTROL) => {
                self.page_up();
            }
            KeyCode::Char('b') | KeyCode::PageUp => {
                self.page_up();
            }
            KeyCode::Char(' ') | KeyCode::Char('f') | KeyCode::PageDown => {
                self.page_down();
            }
            KeyCode::Char('e') => {
                match self.mode {
                    ViewMode::Normal => self.switch_to_expanded_mode()?,
                    ViewMode::Expanded => self.switch_to_normal_mode(),
                }
                self.cursor = (0, 0);
                self.log_viewer.set_cursor(0, 0)?;
            }
            KeyCode::Char('g') | KeyCode::Char('<') => {
                // Go to the first line
                self.start_line = 0;
                self.end_line = self.rows;
                self.cursor = (0, 0);
                self.log_viewer.set_cursor(0, 0)?;
            }
            KeyCode::Char('G') | KeyCode::Char('>')=> {
                // Go to the last line
                (self.start_line, self.end_line) = self.log_file.get_end_of_file(self.rows, self.cols, 3);
                /*let total_lines = self.get_active_log_file().total_lines();
                self.start_line = total_lines.saturating_sub(self.rows);
                self.end_line = total_lines; */
                self.cursor = (0, 0);
                self.log_viewer.set_cursor(0, 0)?;
            }
            KeyCode::Char('x') => {
                self.log_file.hide_line(self.get_current_line_number());
            }
            _ => {}
        }
        Ok(redraw)
    }

    fn move_cursor(&mut self, x: i16, y: i16) -> Result<bool> {
        let new_x = self.cursor.0 as i16 + x;
        let new_y = self.cursor.1 as i16 + y;

        if new_y < 0 {
            // Prevent moving cursor above the first line
            self.start_line = self.start_line.saturating_sub(1);
            self.end_line = self.start_line + self.rows;
            return Ok(true);
        } else if new_y > self.line_numbers.len() as i16 - 1 {
            // Prevent moving cursor below the last line in the file
            let total_lines = self.get_active_log_file().total_lines();
            debug!("End line {} Total lines: {}", self.end_line, total_lines);
            if self.end_line + 1 >= total_lines {
                return Ok(false);
            }

            self.start_line = (self.start_line + 1).min(self.get_active_log_file().total_lines());
            self.end_line = (self.end_line + 1).min(self.get_active_log_file().total_lines());
            return Ok(true); // Prevent moving cursor below the last line
        }

        // Ensure cursor position is within bounds
        let x = if new_x < 0 {
            0
        } else if new_x >= self.cols as i16 {
            self.cols as u16 - 1
        } else {
            new_x as u16
        };
        let y = if new_y < 0 {
            0
        } else if new_y >= self.rows as i16 {
            self.rows as u16 - 1
        } else {
            new_y as u16
        };

        // Update cursor position
        self.cursor = (x, y);
        self.log_viewer.set_cursor(x, y)?;
        Ok(false)
    }

    fn page_up(&mut self) {
        debug!("Page up called");

        if self.start_line > 0 {
            debug!(
                "Page up called. start{} end{} rows{}",
                self.start_line, self.end_line, self.rows
            );
            self.start_line = self.start_line.saturating_sub(self.rows);
            self.end_line =
                (self.start_line + self.rows).min(self.get_active_log_file().total_lines());
        }
    }

    fn page_down(&mut self) {
        debug!("Page down called {}", self.end_line);
        if self.end_line + 1 < self.get_active_log_file().total_lines() {
            self.start_line = self.end_line + 1;
            self.end_line =
                (self.start_line + self.rows).min(self.get_active_log_file().total_lines());
            debug!(
                "Page down called. start{} end{} rows{}",
                self.start_line, self.end_line, self.rows
            );
        }
    }

    fn handle_resize(&mut self, _width: u16, _height: u16) -> Result<()> {
        // Handle terminal resize if needed
        debug!("Terminal resized to width: {}, height: {}", _width, _height);
        let (rows, cols) = self.log_viewer.get_row_cols()?;
        self.rows = rows;
        self.cols = cols;
        self.log_viewer.set_cursor(0, 0)?;
        //self.log_viewer.clear()?;
        //self.log_viewer.clear()?;
        //self.log_viewer.print_screen(&self.log_file.get_visible_lines(0, height as usize - 1))?;
        Ok(())
    }

    fn draw(&mut self) -> Result<()> {
        // Destructure self so that we can borrow log_viewer and log_file independently.
        let Controller {
            log_file,
            log_viewer,
            start_line,
            end_line,
            rows,
            cursor,
            line_numbers,
            expanded_log_file,
            mode,
            .. // Ignore other fields for now
        } = self;

        log_viewer.clear()?;

        let active_log_file = match mode {
            ViewMode::Normal => log_file,
            ViewMode::Expanded => expanded_log_file.as_ref().unwrap(),
        };

        debug!("Drawing lines from {} rows {}", *start_line, *rows);
        let visible_lines = active_log_file.get_visible_lines(*start_line, *rows);
        *line_numbers = log_viewer.print_screen(&visible_lines)?;
        debug!("Line numbers: {:?}", line_numbers);

        *start_line = line_numbers.first().cloned().unwrap_or(0);
        *end_line = line_numbers.last().cloned().unwrap_or(0);

        if cursor.1 > line_numbers.len() as u16 - 1 {
            // If cursor is at the bottom, move it to the last line
            cursor.1 = (line_numbers.len()).saturating_sub(1) as u16;
            log_viewer.set_cursor(cursor.0, cursor.1)?;
        }

        debug!("Drawing lines from {} to {}", *start_line,*end_line);
        Ok(())
    }
}
