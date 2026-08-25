//! Backend-parity tests: every backend must interpret a spec identically.

#[cfg_attr(not(feature = "parser-bpaf"), allow(unused_imports))]
use super::ParsedValues;
#[cfg_attr(not(feature = "parser-usage"), allow(unused_imports))]
#[cfg_attr(not(feature = "parser-bpaf"), allow(unused_imports))]
use super::{ArgKind, ArgSpec, CommandSpec};
#[cfg(feature = "parser-usage")]
use super::{CommandSpec as UsageCommandSpec, PositionalSpec};

#[cfg(feature = "parser-bpaf")]
use super::PositionalSpec;
#[cfg(feature = "parser-bpaf")]
const ECHO_SPEC: CommandSpec = CommandSpec {
    args: &[
        ArgSpec::flag("no_newline", &['n'], &[], ""),
        ArgSpec::value("delimiter", &['d'], &[], "DELIM", ""),
    ],
    positionals: &[PositionalSpec::many("operands", "OPERANDS")],
};

#[cfg(feature = "parser-bpaf")]
#[cfg(feature = "parser-bpaf")]
mod bpaf_impl {
    use super::*;
    #[cfg_attr(not(feature = "parser-usage"), allow(unused_imports))]
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

#[cfg(feature = "parser-usage")]
mod usage_impl {
    use super::UsageCommandSpec as CommandSpec;
    use super::*;
    use crate::argmodel::backend::ArgParserBackend as _;

    const SHIFT_SPEC: CommandSpec = CommandSpec {
        args: &[],
        positionals: &[PositionalSpec::one("n", "N")],
    };

    #[test]
    fn single_positional_binds_value() {
        let values = super::super::usage_backend::UsageBackend
            .parse(&SHIFT_SPEC, "shift", &["2".to_string()])
            .unwrap_or_else(|e| panic!("usage parse failed: {e}"));
        assert_eq!(values.value_of_positional("n"), Some("2"));
    }
}

#[cfg(feature = "parser-usage")]
mod usage_cache {
    use super::*;
    use crate::argmodel::backend::ArgParserBackend as _;

    const SHIFT_SPEC: CommandSpec = CommandSpec {
        args: &[],
        positionals: &[PositionalSpec::one("n", "N")],
    };

    #[test]
    fn repeated_parses_reuse_interned_graph() {
        // N.B. First parse interns the graph; later parses must reuse it.
        // Assert via address stability of the engine command graph.
        let a = super::super::usage_backend::build_command(&SHIFT_SPEC, "shift");
        let b = super::super::usage_backend::build_command(&SHIFT_SPEC, "shift");
        assert!(std::ptr::eq(a, b));

        let first = super::super::usage_backend::UsageBackend
            .parse(&SHIFT_SPEC, "shift", &["2".to_string()])
            .unwrap();
        assert_eq!(first.value_of_positional("n"), Some("2"));

        let second = super::super::usage_backend::UsageBackend
            .parse(&SHIFT_SPEC, "shift", &["3".to_string()])
            .unwrap();
        assert_eq!(second.value_of_positional("n"), Some("3"));
    }
}
