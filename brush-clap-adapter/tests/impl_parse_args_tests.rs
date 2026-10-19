//! Tests of what `impl_parse_args!` implements, in each of its forms, as a
//! crate that uses it sees it.

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use brush_core::CommandArg;
use brush_core::builtins::{
    ContentOptions, ContentType, DeclarationRole, Invocation, ParseArgs, UsageError,
};
use brush_core::parser::{SourceSpan, ast};
use clap::Parser;

/// Parses `args`, as the builtin `T` invoked under `name`.
fn parse<T: ParseArgs>(
    name: &str,
    args: impl IntoIterator<Item = CommandArg>,
) -> Result<T, UsageError> {
    T::parse(Invocation::new(name, args))
}

/// Returns words as arguments.
fn words<const N: usize>(words: [&str; N]) -> Vec<CommandArg> {
    words.into_iter().map(CommandArg::from).collect()
}

/// Returns an assignment argument, as the shell passes one to a declaration
/// utility.
fn assignment(name: &str, value: &str) -> CommandArg {
    CommandArg::Assignment(ast::Assignment {
        name: ast::AssignmentName::VariableName(name.to_owned()),
        value: ast::AssignmentValue::Scalar(ast::Word::new(value)),
        append: false,
        loc: SourceSpan::default(),
    })
}

/// Describes arguments by kind and text, e.g. `["a=1" (assignment), "b"]`.
fn describe(args: &[CommandArg]) -> Vec<String> {
    args.iter()
        .map(|arg| match arg {
            CommandArg::String(s) => s.clone(),
            CommandArg::Assignment(a) => format!("{a} (assignment)"),
        })
        .collect()
}

fn content<T: ParseArgs>(name: &str, content_type: ContentType, colorized: bool) -> String {
    T::content(name, content_type, &ContentOptions { colorized }).unwrap()
}

//
// impl_parse_args!(Type)
//

#[derive(Parser, Debug)]
#[command(about = "Does plain things")]
struct Plain {
    #[arg(short = 'v')]
    verbose: bool,
    #[arg(short = 'n')]
    count: Option<u32>,
    args: Vec<String>,
}

brush_clap_adapter::impl_parse_args!(Plain);

#[test]
fn plain_form_is_not_a_declaration_utility() {
    assert_eq!(Plain::DECLARATION_ROLE, DeclarationRole::None);
}

#[test]
fn plain_form_parses_every_argument_with_clap() {
    let plain: Plain = parse("plain", words(["-v", "-n", "3", "a", "b"])).unwrap();
    assert!(plain.verbose);
    assert_eq!(plain.count, Some(3));
    assert_eq!(plain.args, ["a", "b"]);
}

#[test]
fn plain_form_flattens_an_assignment_to_its_text() {
    let plain: Plain = parse("plain", [assignment("a", "1"), "b".into()]).unwrap();
    assert_eq!(plain.args, ["a=1", "b"]);
}

#[test]
fn plain_form_rejects_what_clap_rejects() {
    let err = parse::<Plain>("plain", words(["-z"])).unwrap_err();
    assert!(err.to_string().contains("'-z'"), "{err}");
    let err = parse::<Plain>("plain", words(["-n", "x"])).unwrap_err();
    assert!(err.to_string().contains("'x'"), "{err}");
}

#[test]
fn plain_form_turns_help_but_not_h_into_a_help_request() {
    let err = parse::<Plain>("plain", words(["-v", "--help"])).unwrap_err();
    assert!(err.is_help());
    for flag in ["-h", "-z"] {
        let err = parse::<Plain>("plain", words([flag])).unwrap_err();
        assert!(!err.is_help(), "{flag}");
        assert!(err.to_string().contains(&format!("'{flag}'")), "{err}");
    }
}

#[test]
fn every_form_turns_clap_s_help_flag_into_a_help_request() {
    assert!(
        parse::<PlusOptions>("p", words(["--help"]))
            .unwrap_err()
            .is_help()
    );
    assert!(
        parse::<KeepDoubleDash>("k", words(["--help", "--"]))
            .unwrap_err()
            .is_help()
    );
    assert!(
        parse::<DeclarationUtility>("d", words(["--help"]))
            .unwrap_err()
            .is_help()
    );
    // After a double dash, `--help` is an operand.
    assert!(parse::<KeepDoubleDash>("k", words(["--", "--help"])).is_ok());
}

#[test]
fn plain_form_takes_a_double_dash_as_clap_does() {
    let plain: Plain = parse("plain", words(["--", "-v"])).unwrap();
    assert!(!plain.verbose);
    assert_eq!(plain.args, ["-v"]);
}

//
// impl_parse_args!(Type, plus_options)
//

#[derive(Parser, Debug)]
struct PlusOptions {
    #[arg(short = 'x')]
    x: bool,
    #[arg(long = "+x")]
    plus_x: bool,
    #[arg(short = 'y')]
    y: bool,
    #[arg(long = "+y")]
    plus_y: bool,
    args: Vec<String>,
}

brush_clap_adapter::impl_parse_args!(PlusOptions, plus_options,);

#[test]
fn plus_options_form_is_not_a_declaration_utility() {
    assert_eq!(PlusOptions::DECLARATION_ROLE, DeclarationRole::None);
}

#[test]
fn plus_options_form_takes_options_with_either_sign() {
    let opts: PlusOptions = parse("p", words(["+x", "-y"])).unwrap();
    assert!(opts.plus_x && opts.y && !opts.x && !opts.plus_y);
}

#[test]
fn plus_options_form_splits_combined_plus_options() {
    let opts: PlusOptions = parse("p", words(["+xy"])).unwrap();
    assert!(opts.plus_x && opts.plus_y && !opts.x && !opts.y);
}

#[test]
fn plus_options_form_leaves_words_after_double_dash_alone() {
    let opts: PlusOptions = parse("p", words(["+x", "--", "+y", "-x"])).unwrap();
    assert!(opts.plus_x && !opts.plus_y && !opts.x);
    assert_eq!(opts.args, ["+y", "-x"]);
}

#[test]
fn plus_options_form_rejects_an_unknown_plus_option() {
    assert!(parse::<PlusOptions>("p", words(["+z"])).is_err());
}

//
// impl_parse_args!(Type, keep_double_dash = field)
//

#[derive(Parser, Debug)]
struct KeepDoubleDash {
    #[arg(short = 'n')]
    n: bool,
    args: Vec<String>,
}

brush_clap_adapter::impl_parse_args!(KeepDoubleDash, keep_double_dash = args);

#[test]
fn keep_double_dash_form_is_not_a_declaration_utility() {
    assert_eq!(KeepDoubleDash::DECLARATION_ROLE, DeclarationRole::None);
}

#[test]
fn keep_double_dash_form_appends_the_double_dash_and_what_follows() {
    let parsed: KeepDoubleDash = parse("k", words(["-n", "a", "--", "-n", "--", "b"])).unwrap();
    assert!(parsed.n);
    assert_eq!(parsed.args, ["a", "--", "-n", "--", "b"]);
}

#[test]
fn keep_double_dash_form_without_a_double_dash_parses_as_usual() {
    let parsed: KeepDoubleDash = parse("k", words(["-n", "a"])).unwrap();
    assert!(parsed.n);
    assert_eq!(parsed.args, ["a"]);
}

#[test]
fn keep_double_dash_form_with_a_leading_double_dash_parses_no_options() {
    let parsed: KeepDoubleDash = parse("k", words(["--", "-n"])).unwrap();
    assert!(!parsed.n);
    assert_eq!(parsed.args, ["--", "-n"]);
}

#[test]
fn keep_double_dash_form_rejects_a_bad_option_before_the_double_dash_only() {
    assert!(parse::<KeepDoubleDash>("k", words(["-z", "--"])).is_err());
    assert!(parse::<KeepDoubleDash>("k", words(["--", "-z"])).is_ok());
}

//
// impl_parse_args!(Type, declarations = field[, plus_options])
//

#[derive(Parser, Debug)]
struct DeclarationUtility {
    #[arg(short = 'i')]
    integer: bool,
    #[arg(long = "+i")]
    plus_integer: bool,
    #[arg(skip)]
    declarations: Vec<CommandArg>,
}

brush_clap_adapter::impl_parse_args!(DeclarationUtility, declarations = declarations);

#[derive(Parser, Debug)]
struct PlusDeclarationUtility {
    #[arg(short = 'i')]
    integer: bool,
    #[arg(long = "+i")]
    plus_integer: bool,
    #[arg(skip)]
    declarations: Vec<CommandArg>,
}

brush_clap_adapter::impl_parse_args!(
    PlusDeclarationUtility,
    declarations = declarations,
    plus_options,
);

#[test]
fn declarations_forms_are_declaration_utilities() {
    assert_eq!(
        DeclarationUtility::DECLARATION_ROLE,
        DeclarationRole::Utility
    );
    assert_eq!(
        PlusDeclarationUtility::DECLARATION_ROLE,
        DeclarationRole::Utility
    );
}

#[test]
fn declarations_form_keeps_each_declaration_as_it_arrived() {
    let parsed: DeclarationUtility = parse(
        "d",
        [
            "-i".into(),
            assignment("a", "1 2"),
            "b".into(),
            "-".into(),
            "+".into(),
        ],
    )
    .unwrap();
    assert!(parsed.integer);
    assert_eq!(
        describe(&parsed.declarations),
        ["a=1 2 (assignment)", "b", "-", "+"]
    );
}

#[test]
fn declarations_form_takes_options_only_before_the_first_declaration() {
    let parsed: DeclarationUtility = parse("d", words(["a", "-i", "b"])).unwrap();
    assert!(!parsed.integer);
    assert_eq!(describe(&parsed.declarations), ["a", "-i", "b"]);
}

#[test]
fn declarations_form_takes_what_follows_a_double_dash_as_declarations() {
    let parsed: DeclarationUtility = parse("d", words(["-i", "--", "-i"])).unwrap();
    assert!(parsed.integer);
    assert_eq!(describe(&parsed.declarations), ["-i"]);
}

#[test]
fn declarations_form_without_plus_options_rejects_a_plus_option() {
    assert!(parse::<DeclarationUtility>("d", words(["+i"])).is_err());
}

#[test]
fn declarations_form_rejects_an_unknown_option() {
    assert!(parse::<DeclarationUtility>("d", words(["-z", "a"])).is_err());
}

#[test]
fn declarations_form_with_plus_options_takes_them() {
    let parsed: PlusDeclarationUtility = parse("d", words(["+i", "a"])).unwrap();
    assert!(parsed.plus_integer && !parsed.integer);
    assert_eq!(describe(&parsed.declarations), ["a"]);
}

//
// impl_parse_args!(Type, parse = parser[, declaration_utility])
//

/// Parses by hand.
#[derive(Parser, Debug)]
#[command(about = "Parses by hand")]
struct Custom {
    #[arg(skip)]
    args: Vec<CommandArg>,
}

impl Custom {
    fn parse_args(invocation: Invocation<'_>) -> Result<Self, UsageError> {
        let name = invocation.name().to_owned();
        let args = invocation.into_args();
        if args
            .iter()
            .any(|arg| matches!(arg, CommandArg::String(s) if s == "bad"))
        {
            return Err(UsageError::new(format!("{name}: bad argument")));
        }
        Ok(Self { args })
    }
}

brush_clap_adapter::impl_parse_args!(Custom, parse = Custom::parse_args);

/// Parses by hand, as a declaration utility.
#[derive(Parser, Debug)]
struct CustomDeclarations {
    #[arg(skip)]
    args: Vec<CommandArg>,
}

brush_clap_adapter::impl_parse_args!(
    CustomDeclarations,
    parse = |invocation: Invocation<'_>| Ok(Self {
        args: invocation.into_args()
    }),
    declaration_utility,
);

#[test]
fn parse_form_role_follows_declaration_utility() {
    assert_eq!(Custom::DECLARATION_ROLE, DeclarationRole::None);
    assert_eq!(
        CustomDeclarations::DECLARATION_ROLE,
        DeclarationRole::Utility
    );
}

#[test]
fn parse_form_hands_the_whole_invocation_to_the_parser() {
    let parsed: Custom = parse("c", [CommandArg::from("-z"), assignment("a", "1")]).unwrap();
    assert_eq!(describe(&parsed.args), ["-z", "a=1 (assignment)"]);

    let parsed: CustomDeclarations = parse("c", words(["--help", "--"])).unwrap();
    assert_eq!(describe(&parsed.args), ["--help", "--"]);
}

#[test]
fn parse_form_returns_the_parser_s_error() {
    let err = parse::<Custom>("custom", words(["ok", "bad"])).unwrap_err();
    assert!(!err.is_help());
    assert_eq!(err.to_string(), "custom: bad argument");
}

#[test]
fn parse_form_takes_its_help_from_clap() {
    assert_eq!(
        content::<Custom>("custom", ContentType::ShortDescription, false),
        "custom - Parses by hand\n"
    );
}

//
// Help content, the same in every form.
//

/// Exercises the short usage line.
#[derive(Parser, Debug)]
#[command(about = "Shows its usage")]
struct Usage {
    #[arg(short = 'a')]
    all: bool,
    #[arg(short = 'b', value_name = "B")]
    bee: Option<String>,
    #[arg(short = 'r', required = true)]
    required: Option<String>,
    #[arg(short = 's', hide = true)]
    secret: bool,
    #[arg(long = "long-only")]
    long_only: bool,
    #[arg(value_name = "FILE")]
    file: String,
    #[arg(hide = true)]
    hidden: Option<String>,
    #[arg(value_name = "REST")]
    rest: Vec<String>,
}

brush_clap_adapter::impl_parse_args!(Usage);

// Has no options, operands, or description (which clap would take from a
// doc comment).
#[derive(Parser, Debug)]
struct Bare {}

brush_clap_adapter::impl_parse_args!(Bare);

#[test]
fn short_usage_lists_optional_then_required_short_options_then_operands() {
    assert_eq!(
        content::<Usage>("usage", ContentType::ShortUsage, false),
        "usage: usage [-ab] -r FILE [REST]\n"
    );
}

#[test]
fn short_usage_is_given_under_the_invoked_name() {
    assert_eq!(
        content::<Usage>("alias", ContentType::ShortUsage, false),
        "alias: alias [-ab] -r FILE [REST]\n"
    );
}

/// Takes operands only.
#[derive(Parser, Debug)]
struct OperandsOnly {
    #[arg(value_name = "OPTIONAL")]
    optional: Option<String>,
    #[arg(value_name = "NEXT")]
    next: Option<String>,
}

brush_clap_adapter::impl_parse_args!(OperandsOnly);

/// Takes a required option only.
#[derive(Parser, Debug)]
struct RequiredOnly {
    #[arg(short = 'r', required = true)]
    required: Option<String>,
    #[arg(short = 'q', required = true)]
    quiet: Option<String>,
}

brush_clap_adapter::impl_parse_args!(RequiredOnly);

#[test]
fn short_usage_of_operands_alone_brackets_each_optional_one() {
    assert_eq!(
        content::<OperandsOnly>("ops", ContentType::ShortUsage, false),
        "ops: ops [OPTIONAL] [NEXT]\n"
    );
}

#[test]
fn short_usage_of_required_options_alone_groups_them() {
    assert_eq!(
        content::<RequiredOnly>("req", ContentType::ShortUsage, false),
        "req: req -rq\n"
    );
}

#[test]
fn short_description_is_the_name_and_clap_s_about() {
    assert_eq!(
        content::<Usage>("alias", ContentType::ShortDescription, false),
        "alias - Shows its usage\n"
    );
    assert_eq!(
        content::<Bare>("bare", ContentType::ShortDescription, false),
        "bare - \n"
    );
}

#[test]
fn detailed_help_is_clap_s_rendering_under_the_invoked_name() {
    let help = content::<Usage>("alias", ContentType::DetailedHelp, false);
    assert!(help.starts_with("Shows its usage\n"), "{help}");
    assert!(help.contains("Usage: alias "), "{help}");
    assert!(help.contains("--long-only"), "{help}");
    assert!(!help.contains("-s"), "a hidden option is listed: {help}");
    assert!(!help.contains('\x1b'), "uncolored help has escapes: {help}");
}

#[test]
fn detailed_help_is_colored_on_request() {
    use clap::builder::styling::{AnsiColor, Effects};

    let colored = content::<Usage>("alias", ContentType::DetailedHelp, true);
    let header = AnsiColor::Yellow.on_default() | Effects::BOLD | Effects::UNDERLINE;
    let usage = AnsiColor::Green.on_default() | Effects::BOLD;
    let literal = AnsiColor::Magenta.on_default() | Effects::BOLD;
    for expected in [
        format!("{header}Options:"),
        format!("{usage}Usage:"),
        format!("{literal}--long-only"),
    ] {
        assert!(
            colored.contains(&expected),
            "{expected:?} not in {colored:?}"
        );
    }
}

#[test]
fn a_man_page_is_not_implemented() {
    assert!(Usage::content("usage", ContentType::ManPage, &ContentOptions::default()).is_err());
}
