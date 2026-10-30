//! Taking the terminal.

#![cfg(test)]
#![cfg(unix)]

/// Without a terminal on standard input, taking it changes nothing: the process stays in its
/// process group.
#[test]
fn without_a_terminal_nothing_is_taken() {
    let null = std::fs::File::open("/dev/null").unwrap();
    nix::unistd::dup2_stdin(&null).unwrap();
    let group = nix::unistd::getpgrp();

    let _terminal = brush_hosting::terminal::ProcessTerminal::acquire().unwrap();

    assert_eq!(nix::unistd::getpgrp(), group);
}
