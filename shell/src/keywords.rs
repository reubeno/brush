use std::collections::HashSet;

use crate::Shell;

fn get_keywords(sh_mode_only: bool) -> HashSet<String> {
    let mut keywords = HashSet::new();
    keywords.insert(String::from("!"));
    keywords.insert(String::from("{"));
    keywords.insert(String::from("}"));
    keywords.insert(String::from("case"));
    keywords.insert(String::from("do"));
    keywords.insert(String::from("done"));
    keywords.insert(String::from("elif"));
    keywords.insert(String::from("else"));
    keywords.insert(String::from("esac"));
    keywords.insert(String::from("fi"));
    keywords.insert(String::from("for"));
    keywords.insert(String::from("if"));
    keywords.insert(String::from("in"));
    keywords.insert(String::from("then"));
    keywords.insert(String::from("until"));
    keywords.insert(String::from("while"));

    if !sh_mode_only {
        keywords.insert(String::from("[["));
        keywords.insert(String::from("]]"));
        keywords.insert(String::from("function"));
        keywords.insert(String::from("select"));
    }

    keywords
}

lazy_static::lazy_static! {
    pub(crate) static ref SH_MODE_KEYWORDS: HashSet<String> = get_keywords(true);
    pub(crate) static ref KEYWORDS: HashSet<String> = get_keywords(false);
}

pub fn is_keyword(shell: &Shell, name: &str) -> bool {
    if shell.options.sh_mode {
        SH_MODE_KEYWORDS.contains(name)
    } else {
        KEYWORDS.contains(name)
    }
}
