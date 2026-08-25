//! Backend-parity tests: every backend must interpret a spec identically.

use super::{ArgSpec, CommandSpec, ParsedValues, PositionalSpec};

const ECHO_SPEC: CommandSpec = CommandSpec {
    args: &[
        ArgSpec::flag("no_newline", &['n'], &[], ""),
        ArgSpec::value("delimiter", &['d'], &[], "DELIM", ""),
    ],
    positionals: &[PositionalSpec::many("operands", "OPERANDS")],
};

#[cfg(feature = "parser-bpaf")]
mod bpaf_impl {
    use super::*;
    use crate::argmodel::backend::ArgParserBackend as _;

    #[allow(clippy::panic)]
    fn run(argv: &[String]) -> ParsedValues {
        super::super::bpaf_backend::BpafBackend
            .parse(&ECHO_SPEC, "echo", argv)
            .unwrap_or_else(|e| panic!("bpaf parse failed: {e}"))
    }

    #[test]
    fn flag_and_value() {
        let m = run(&["-d".to_string(), ":".to_string(), "-n".to_string()]);
        assert!(m.flag("no_newline"));
        assert_eq!(m.value("delimiter"), Some(":"));
    }

    #[test]
    fn plain_operands_bind_to_positionals() {
        let m = run(&["a", "b"].iter().map(|s| s.to_string()).collect::<Vec<_>>());
        assert_eq!(m.positional_values("operands"), ["a", "b"]);
    }

    #[test]
    fn strict_positionals_reject_flag_like_words() {
        assert!(
            super::super::bpaf_backend::BpafBackend
                .parse(&ECHO_SPEC, "echo", &["-x".to_string()])
                .is_err()
        );
    }

    #[test]
    fn unknown_flag_errors() {
        let err = super::super::bpaf_backend::BpafBackend.parse(
            &ECHO_SPEC,
            "echo",
            &["--frobnicate".to_string()],
        );
        assert!(err.is_err());
    }
}

#[cfg(feature = "parser-clap")]
mod clap_impl {
    use super::*;

    #[allow(clippy::panic)]
    fn run(argv: &[String]) -> ParsedValues {
        let spec = echo_spec();
        super::super::clap_backend::ClapBackend
            .parse(&spec, "echo", argv)
            .unwrap_or_else(|e| panic!("clap parse failed: {e}"))
    }

    #[test]
    fn flag_and_value() {
        let m = run(&["-d".to_string(), ":".to_string(), "-n".to_string()]);
        assert!(m.flag("no_newline"));
        assert_eq!(m.value("delimiter"), Some(":"));
    }
}
