use crate::log_file::{LogFile, SearchDirection};
use crate::log_viewer::LogViewer;

use anyhow::Result;
use log::debug;
use shlex;

pub fn handle_command(
    input: &str,
    line_num: usize,
    lf: &mut LogFile,
    lv: &mut LogViewer,
) -> Result<Option<usize>> {
    let trimmed_input = input.trim();
    if trimmed_input.is_empty() {
        return Ok(None);
    }

    let first_char = trimmed_input.chars().next().unwrap();

    if first_char == '/' || first_char == '?' {
        let parts = shlex::split(&trimmed_input[1..])
            .ok_or_else(|| anyhow::anyhow!("Failed to parse command"))?;

        let ret = search(
            if parts.is_empty() { "" } else { &parts[0] },
            line_num,
            lf,
            lv,
            if first_char == '/' {
                SearchDirection::Forward
            } else {
                SearchDirection::Backward
            },
        );
        debug!(
            "Search command executed with pattern: '{}', result: {:?}",
            &trimmed_input[1..],
            ret
        );
        return Ok(ret);

        // Call search function with pattern
    } else {
        let parts = shlex::split(trimmed_input)
            .ok_or_else(|| anyhow::anyhow!("Failed to parse command"))?;
        let command = &parts[0];
        let args = &parts[1..];

        debug!("Raw input: {:?}", trimmed_input);
        debug!("Parsed parts: {:?}", parts);

        debug!("Command: '{}', Args: {:?}", command, args);

        match command.as_str() {
            "hl" | "highlight" => {
                // Highlight
                if args.is_empty() {
                    return Ok(None); // No pattern provided
                }
                let _color = lv.set_highlight(
                    args[0].clone(),
                    if args.len() > 1 {
                        Some(args[1].to_string())
                    } else {
                        None
                    },
                );
            }
            "hd" | "hide"=> {
                // Hide
                if args.is_empty() {
                    return Ok(None); // No pattern provided
                }

                lf.hide_lines_matching(|line| line.contains(&args[0]));

            }
            "sh" | "show" => {
                // Hide
                if args.is_empty() {
                    return Ok(None); // No pattern provided
                }

                lf.show_lines_matching(|line| line.contains(&args[0]));

            }
            "set" => {
                // Set search pattern
                if args.len() < 2 {
                    return Ok(None); // No pattern provided
                }

                match args[0].as_str() {
                    "search_color" => {
                        lv.set_search_color(args[1].as_str());
                    }
                    _ => {
                        debug!("Unknown set command: {}", args[0]);
                    }
                }
            }
            _ => {
                // Unknown command
            }
        }
    }

    Ok(None)
}

pub fn search(
    pattern: &str,
    line_num: usize,
    lf: &LogFile,
    lv: &mut LogViewer,
    direction: SearchDirection,
) -> Option<usize> {
    let mut search_current_line = true;
    let mut pattern = pattern.to_string();
    if pattern.is_empty() {
        if let Some(val) = &lv.search_pattern {
            pattern = val.clone();
            search_current_line = false;
        } else {
            return None; // No search pattern to clear
        }
    }

    lv.search_pattern = Some(pattern.to_string());
    lf.search(&pattern, line_num, search_current_line, direction)
}
