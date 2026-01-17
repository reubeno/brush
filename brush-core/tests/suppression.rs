//! Tests for error suppression behavior used by extensions.
//!
//! These ensure that errors marked as already displayed by extensions are
//! not printed again by the core.

use brush_core::error;

#[test]
fn suppressed_error_is_not_displayed() {
    let err = error::Error::from(error::ErrorKind::CommandNotFound("curl".to_string())).mark_displayed();

    let shell = brush_core::Shell::default();
    let mut out = Vec::new();

    // display_error should be a no-op for suppressed errors
    shell.display_error(&mut out, &err).unwrap();
    assert!(out.is_empty(), "suppressed error should not produce output");
}

#[test]
fn mark_displayed_sets_flag() {
    let err = error::Error::from(error::ErrorKind::CommandNotFound("git".to_string()));
    assert!(!err.is_suppressed());

    let err2 = err.mark_displayed();
    assert!(err2.is_suppressed());
}
