use std::io::Write;

use crate::{InteractivePrompt, ReadResult, ShellError};

const BACKSPACE: char = 8u8 as char;

// TODO: This function is desperately calling out to be turned into a small module and cleaned up,
// but we intentionally want to stop short of adding all the bells and whistles. We just want enough
// here that we can use it in the basic shell for (p)expect/pty-style testing of completion, and
// without using VT100-style escape sequences for cursor movement and display.
#[allow(clippy::unwrap_in_result)]
#[allow(clippy::too_many_lines)]
#[allow(clippy::cast_possible_truncation)]
pub(crate) fn read_input_line_from_terminal(
    prompt: &InteractivePrompt,
    mut completion_handler: impl FnMut(
        &str,
        usize,
    ) -> Result<brush_core::completion::Completions, ShellError>,
) -> Result<ReadResult, ShellError> {
    use crossterm::ExecutableCommand;

    let raw_mode = RawModeToggle::new()?;
    let display_prompt = || -> Result<(), ShellError> {
        raw_mode.disable()?;
        eprint!("{}", prompt.prompt.as_str());
        raw_mode.enable()?;
        std::io::stderr().flush()?;
        Ok(())
    };

    let mut line = String::new();
    let mut cursor = 0;

    loop {
        raw_mode.enable()?;
        if let crossterm::event::Event::Key(event) = crossterm::event::read()? {
            match (event.modifiers, event.code) {
                (_, crossterm::event::KeyCode::Enter)
                | (crossterm::event::KeyModifiers::CONTROL, crossterm::event::KeyCode::Char('j')) =>
                {
                    raw_mode.disable()?;
                    eprintln!();
                    raw_mode.enable()?;
                    std::io::stderr().flush()?;
                    return Ok(ReadResult::Input(line));
                }
                (
                    crossterm::event::KeyModifiers::SHIFT | crossterm::event::KeyModifiers::NONE,
                    crossterm::event::KeyCode::Char(c),
                ) => {
                    line.insert(cursor, c);
                    cursor += 1;
                    eprint!("{c}");
                    std::io::stderr().flush()?;
                }
                (crossterm::event::KeyModifiers::CONTROL, crossterm::event::KeyCode::Char('c')) => {
                    raw_mode.disable()?;
                    eprintln!("^C");
                    return Ok(ReadResult::Interrupted);
                }
                (crossterm::event::KeyModifiers::CONTROL, crossterm::event::KeyCode::Char('d')) => {
                    if line.is_empty() {
                        raw_mode.disable()?;
                        eprintln!();
                        return Ok(ReadResult::Eof);
                    }
                }
                (crossterm::event::KeyModifiers::CONTROL, crossterm::event::KeyCode::Char('l')) => {
                    std::io::stderr()
                        .execute(crossterm::terminal::Clear(
                            crossterm::terminal::ClearType::All,
                        ))?
                        .execute(crossterm::cursor::MoveTo(0, 0))?;

                    display_prompt()?;
                    eprint!("{}", line.as_str());
                    std::io::stderr().flush()?;
                }
                (_, crossterm::event::KeyCode::Backspace) => {
                    if cursor > 0 {
                        cursor -= 1;
                        line.remove(cursor);
                        raw_mode.disable()?;
                        eprint!("{BACKSPACE}");
                        eprint!("{} ", &line[cursor..]);
                        eprint!("{}", repeated_char_str(BACKSPACE, line.len() + 1 - cursor));
                        raw_mode.enable()?;
                        std::io::stderr().flush()?;
                    }
                }
                (_, crossterm::event::KeyCode::Left) => {
                    if cursor > 0 {
                        cursor -= 1;
                        raw_mode.disable()?;
                        eprint!("{BACKSPACE}");
                        raw_mode.enable()?;
                        std::io::stderr().flush()?;
                    }
                }
                (_, crossterm::event::KeyCode::Tab) => {
                    let completions = completion_handler(line.as_str(), cursor)?;
                    if completions.candidates.is_empty() {
                        // Do nothing
                    } else if completions.candidates.len() == 1 {
                        // Apply replacement directly.
                        let candidate = completions.candidates.iter().next().unwrap();
                        if completions.insertion_index + completions.delete_count == cursor {
                            let mut delete_count = completions.delete_count;
                            let mut redisplay_offset = completions.insertion_index;

                            // Don't bother erasing and re-writing the portion of the
                            // completion's prefix that
                            // is identical to what we already had in the token-being-completed.
                            if delete_count > 0
                                && candidate.starts_with(
                                    &line[redisplay_offset..redisplay_offset + delete_count],
                                )
                            {
                                redisplay_offset += delete_count;
                                delete_count = 0;
                            }

                            let mut updated_line = line.clone();
                            updated_line.truncate(completions.insertion_index);
                            updated_line.push_str(candidate);
                            updated_line.push_str(&line[cursor..]);
                            line = updated_line;

                            cursor = completions.insertion_index + candidate.len();

                            let move_left = repeated_char_str(BACKSPACE, delete_count);
                            raw_mode.disable()?;
                            eprint!("{move_left}{}", &line[redisplay_offset..]);
                            // TODO: Remove trailing chars if completion is shorter?
                            eprint!("{}", repeated_char_str(BACKSPACE, line.len() - cursor));
                            raw_mode.enable()?;
                            std::io::stderr().flush()?;
                        }
                    } else {
                        // Display replacements.
                        raw_mode.disable()?;
                        eprintln!();
                        for candidate in &completions.candidates {
                            eprintln!("{candidate}");
                        }
                        raw_mode.enable()?;
                        std::io::stderr().flush()?;
                        // Re-display prompt.
                        display_prompt()?;
                        // Re-display line so far.
                        raw_mode.disable()?;
                        eprint!(
                            "{line}{}",
                            repeated_char_str(BACKSPACE, line.len() - cursor)
                        );
                        raw_mode.enable()?;
                        std::io::stderr().flush()?;
                    }
                }
                _ => (),
            }
        }
    }
}

struct RawModeToggle {
    initial: bool,
}

impl RawModeToggle {
    fn new() -> Result<Self, ShellError> {
        let initial = crossterm::terminal::is_raw_mode_enabled()?;
        Ok(Self { initial })
    }

    #[allow(clippy::unused_self)]
    pub fn enable(&self) -> Result<(), ShellError> {
        crossterm::terminal::enable_raw_mode()?;
        Ok(())
    }

    #[allow(clippy::unused_self)]
    pub fn disable(&self) -> Result<(), ShellError> {
        crossterm::terminal::disable_raw_mode()?;
        Ok(())
    }
}

impl Drop for RawModeToggle {
    fn drop(&mut self) {
        let _ = if self.initial {
            crossterm::terminal::enable_raw_mode()
        } else {
            crossterm::terminal::disable_raw_mode()
        };
    }
}

fn repeated_char_str(c: char, count: usize) -> String {
    (0..count).map(|_| c).collect()
}
