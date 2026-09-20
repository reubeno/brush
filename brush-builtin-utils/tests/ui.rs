//! Compile-fail tests: misuse of the adapter macros must be rejected with a
//! message that names the accepted forms. The expected diagnostics are the
//! `.stderr` files beside each case; regenerate them with `TRYBUILD=overwrite`
//! after an intentional change.

#![cfg(test)]

#[test]
fn macro_misuse_reports_clear_errors() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/*.rs");
}
