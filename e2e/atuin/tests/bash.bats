#!/usr/bin/env bats
# atuin shell integration: bash

load helpers/shell

@test "init binds ctrl-r and up arrow" {
    start_shell bash
    run_line "bind -s > $BATS_TEST_TMPDIR/macros"
    wait_for_prompt
    grep -qF '"\C-r"' "$BATS_TEST_TMPDIR/macros"
    grep -qF '"\e[A"' "$BATS_TEST_TMPDIR/macros"
}

@test "commands are recorded with their exit status" {
    start_shell bash
    run_line 'echo recorded-one'
    wait_for '^recorded-one$'
    run_line 'false'
    wait_for_prompt
    flush_history
    wait_for_history '^0 echo recorded-one$'
    wait_for_history '^1 false$'
}

@test "ctrl-r search runs the selected command" {
    start_shell bash
    run_line 'echo needle-in-history'
    wait_for '^needle-in-history$'
    flush_history
    wait_for_history '^0 echo needle-in-history$'
    send_keys C-r
    wait_for 'Atuin v'
    send_keys -l 'needle-in'
    wait_for '^\s*>.*echo needle-in-history'
    send_keys Enter
    wait_for_prompt
    # The command was run again: two outputs on screen.
    wait_for_screen_count '^needle-in-history$' 2
}

@test "escaping ctrl-r search keeps the current line" {
    start_shell bash
    send_keys -l 'echo keep-me'
    send_keys C-r
    wait_for 'Atuin v'
    send_keys Escape
    wait_for '^atuin-test> echo keep-me$'
}

@test "up arrow opens search" {
    start_shell bash
    run_line 'echo up-arrow-entry'
    wait_for '^up-arrow-entry$'
    send_keys Up
    wait_for 'Atuin v'
    send_keys Escape
    wait_for_prompt
}
