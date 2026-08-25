//! Backend-parity tests: every backend must interpret a spec identically.

use super::{ArgKind, CommandSpecBuilder, Matches};

fn echo_spec() -> CommandSpecBuilder {
    CommandSpecBuilder::new()
        .arg("no_newline", &['n'], &[], ArgKind::Flag, None, "")
        .arg("delimiter", &['d'], &[], ArgKind::Value, Some("DELIM"), "")
        .positional_many("operands", "OPERANDS")
}

#[cfg(feature = "parser-bpaf")]
mod bpaf_impl {
    use super::*;
    use crate::argmodel::backend::ArgParserBackend as _;

    #[allow(clippy::panic)]
    fn run(argv: &[String]) -> Matches {
        let spec = echo_spec().build();
        super::super::bpaf_backend::BpafBackend
            .parse(&spec, "echo", argv)
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
        assert_eq!(m.values("operands"), ["a", "b"]);
    }

    #[test]
    fn strict_positionals_reject_flag_like_words() {
        let spec = echo_spec().build();
        assert!(
            super::super::bpaf_backend::BpafBackend
                .parse(&spec, "echo", &["-x".to_string()])
                .is_err()
        );
    }

    #[test]
    fn unknown_flag_errors() {
        let spec = echo_spec().build();
        let err = super::super::bpaf_backend::BpafBackend.parse(
            &spec,
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
    fn run(argv: &[String]) -> Matches {
        let spec = echo_spec().build();
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
