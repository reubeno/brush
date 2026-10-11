//! Compile-fail tests: misuse of the adapter macros must be rejected with a
//! message that names the accepted forms. The expected diagnostics are the
//! `.stderr` files beside each case; regenerate them with `TRYBUILD=overwrite`
//! after an intentional change.

#![cfg(test)]

#[test]
#[cfg_attr(
    not(target_os = "linux"),
    ignore = "the diagnostics are platform-independent; one platform is enough for a slow build"
)]
fn macro_misuse_reports_clear_errors() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/*.rs");
}
