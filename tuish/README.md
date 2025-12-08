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

- **Type commands**: Enter text in the bottom command pane
- **Enter**: Execute command
- **Ctrl+C**: Interrupt current command
- **Ctrl+D** (on empty line): Exit shell
- **Arrow keys, Home, End**: Navigate within command input
- **Backspace, Delete**: Edit command text

## Future Enhancements

Potential areas for expansion:

- Additional panes for shell state visibility (variables, jobs, history)
- Resizable and rearrangeable panes
- Command completion support
- Command history navigation (up/down arrows)
- Multiple terminal output panes
- Per-command PTY spawning (vs single persistent PTY)
- Custom prompt rendering in command pane
