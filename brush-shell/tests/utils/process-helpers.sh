function get-proc-stat-value() {
    cat /proc/self/stat | cut -d ' ' --output-delimiter=, -f$1
}

function get-pid() {
    get-proc-stat-value 1
}

function get-ppid() {
    get-proc-stat-value 4
}

function get-pgrp() {
    get-proc-stat-value 5
}

function get-session-id() {
    get-proc-stat-value 6
}

function get-tty-nr() {
    get-proc-stat-value 7
}

function get-term-pgid() {
    get-proc-stat-value 8
}
