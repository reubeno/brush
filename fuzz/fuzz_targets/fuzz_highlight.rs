#![no_main]
#![allow(missing_docs)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::panic)]

use libfuzzer_sys::fuzz_target;
use std::sync::LazyLock;

use brush_interactive::highlighting::{Highlighted, highlight_command};

static TOKIO_RT: LazyLock<tokio::runtime::Runtime> =
    LazyLock::new(|| tokio::runtime::Runtime::new().unwrap());

static SHELL_TEMPLATE: LazyLock<brush_core::Shell> = LazyLock::new(|| {
    TOKIO_RT
        .block_on(
            brush_core::Shell::builder()
                .profile(brush_core::ProfileLoadBehavior::Skip)
                .rc(brush_core::RcLoadBehavior::Skip)
                .build(),
        )
        .unwrap()
});

/// Clamps `raw` into `[0, line.len()]` and down to a UTF-8 char boundary.
const fn snap_cursor(line: &str, raw: u32) -> usize {
    let upper = line.len();
    if upper == 0 {
        return 0;
    }
    let mut pos = (raw as usize) % (upper + 1);
    while pos > 0 && !line.is_char_boundary(pos) {
        pos -= 1;
    }
    pos
}

fn assert_invariants(highlighted: &Highlighted<'_>) {
    let line = highlighted.line();

    // 1. Every span lands on UTF-8 char boundaries and the range is valid.
    for span in highlighted.spans() {
        assert!(
            span.range.start <= span.range.end,
            "span has start > end: {span:?} (line={line:?})",
        );
        assert!(
            span.range.end <= line.len(),
            "span end exceeds line length: {span:?} (line.len()={}, line={line:?})",
            line.len(),
        );
        assert!(
            line.is_char_boundary(span.range.start),
            "span start not on char boundary: {span:?} (line={line:?})",
        );
        assert!(
            line.is_char_boundary(span.range.end),
            "span end not on char boundary: {span:?} (line={line:?})",
        );
    }

    // 2. Spans are ordered and contiguous, covering the entire input.
    let mut next_expected_start = 0usize;
    for span in highlighted.spans() {
        assert_eq!(
            span.range.start, next_expected_start,
            "spans are not contiguous: gap or overlap before {span:?} \
             (expected start={next_expected_start}, line={line:?})",
        );
        next_expected_start = span.range.end;
    }
    assert_eq!(
        next_expected_start,
        line.len(),
        "spans do not cover entire input (covered {next_expected_start} of {}, line={line:?})",
        line.len(),
    );

    // 3. Resolving the text of each span must not panic.
    for (_, _) in highlighted.iter() {}
}

// `raw_cursor` is clamped into the line and snapped to a char boundary below.
fuzz_target!(|input: (String, u32)| {
    let (line, raw_cursor) = input;
    let cursor = snap_cursor(&line, raw_cursor);

    let shell = SHELL_TEMPLATE.clone();
    let highlighted = highlight_command(&shell, &line, cursor);

    assert_invariants(&highlighted);
});
