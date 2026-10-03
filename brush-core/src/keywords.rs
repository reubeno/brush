/// The shell's reserved words, in the order bash lists them (e.g. for `compgen -k`), each
/// with whether it's reserved in sh mode too.
const KEYWORDS: [(&str, bool); 22] = [
    ("if", true),
    ("then", true),
    ("else", true),
    ("elif", true),
    ("fi", true),
    ("case", true),
    ("esac", true),
    ("for", true),
    ("select", false),
    ("while", true),
    ("until", true),
    ("do", true),
    ("done", true),
    ("in", true),
    ("function", false),
    ("time", false),
    ("{", true),
    ("}", true),
    ("!", true),
    ("[[", false),
    ("]]", false),
    ("coproc", false),
];

/// Returns the reserved words, in bash's order; in sh mode, only those reserved there.
pub(crate) fn keywords(sh_mode: bool) -> impl Iterator<Item = &'static str> {
    KEYWORDS
        .iter()
        .filter(move |(_, in_sh_mode)| *in_sh_mode || !sh_mode)
        .map(|(keyword, _)| *keyword)
}
