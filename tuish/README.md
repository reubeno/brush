# tuish - TUI-based Interactive Shell

A creative take on an interactive shell using `ratatui` to provide a multi-pane terminal user interface.

## Features

- **Multi-pane UX**: Separate terminal output pane and command input pane
- **80/20 split layout**: Terminal output takes 80% of screen, command input takes 20%
- **PTY-backed terminal**: Uses `tui-term` widget to display command output in a pseudoterminal
- **Full brush shell integration**: Commands are parsed and executed through `brush-core::Shell`
- **Interactive programs supported**: Programs run directly in the PTY for proper interactive behavior

## Architecture

- **RatatuiInputBackend**: Implements `brush-interactive::InputBackend` trait
  - Manages ratatui terminal and event loop
  - Displays tui-term widget showing PTY output
  - Handles keyboard input in command pane
  - Returns commands to shell when Enter is pressed

- **Shell integration**: Uses `InteractiveShellExt::run_interactively()`
  - Reuses all existing brush shell logic
  - Session management, history, error handling
  - PROMPT_COMMAND execution
  - All shell features work transparently

## Building

```bash
cargo build --package tuish
```

## Running

```bash
cargo run --package tuish
```

Or after building:

```bash
./target/debug/tuish
```

## Controls

### Focus Switching

- **Tab**: Switch focus between Terminal pane and Command Input pane
  - When Terminal is focused: Can scroll/select (future enhancement)
  - When Command Input is focused: Can type and execute commands
  - The focused pane is indicated in the title bar

### Command Input (when Command Input pane is focused)

- **Type commands**: Enter text in the command input area
- **Enter**: Execute command
- **Arrow keys, Home, End**: Navigate within command input
- **Backspace, Delete**: Edit command text

### Global Controls

- **Ctrl+C**: Interrupt current command
- **Ctrl+D** (on empty line): Exit shell

## Future Enhancements

Potential areas for expansion:

- **Tab key for completion**: Once completion is implemented, Tab will be used for that in the command pane
- Terminal pane scrolling when focused
- Additional panes for shell state visibility (variables, jobs, history)
- Resizable and rearrangeable panes
- Command history navigation (up/down arrows)
- Multiple terminal output panes
- Per-command PTY spawning (vs single persistent PTY)
- Custom prompt rendering in command pane
