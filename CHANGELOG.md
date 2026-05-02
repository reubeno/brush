# Changelog

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

All notable changes to this project will be documented in this file.

## [0.4.0] - 2026-05-02

### 🚀 Features

- *(parser)* Add official `serde` feature to `brush-parser` for AST serialization ([#783](https://github.com/reubeno/brush/pull/783))
- *(parser)* [**breaking**] Replace ParsingNearToken with ParsingNear ([#1022](https://github.com/reubeno/brush/pull/1022))
- Introduce notion of marking errors as fatal ([#770](https://github.com/reubeno/brush/pull/770))
- Expose key bindings for hint acceptance ([#802](https://github.com/reubeno/brush/pull/802))
- Add 'serde' feature to `brush-core` ([#831](https://github.com/reubeno/brush/pull/831))
- Enable using word expander without command substs ([#877](https://github.com/reubeno/brush/pull/877))
- Enable session leading config ([#904](https://github.com/reubeno/brush/pull/904))
- Implement \l sequence in prompt expansion ([#913](https://github.com/reubeno/brush/pull/913))
- *(windows)* Various path fixes for windows platform ([#1075](https://github.com/reubeno/brush/pull/1075))
- Implement fatal error propagation ([#773](https://github.com/reubeno/brush/pull/773))
- Implement `set -u` semantics ([#774](https://github.com/reubeno/brush/pull/774))
- Source tracking improvements ([#805](https://github.com/reubeno/brush/pull/805))
- Support asynchronous builtin execution ([#810](https://github.com/reubeno/brush/pull/810))
- *(builtins)* Implement caller builtin ([#812](https://github.com/reubeno/brush/pull/812))
- Implement "compgen -A binding" ([#814](https://github.com/reubeno/brush/pull/814))
- Emulate `exec` in subshell ([#823](https://github.com/reubeno/brush/pull/823))
- Experimental builtin for serializing full shell state ([#835](https://github.com/reubeno/brush/pull/835))
- Expand tilde expansion support ([#842](https://github.com/reubeno/brush/pull/842))
- Implement `set -e` (a.k.a. `errexit`) + `pipefail` semantics ([#852](https://github.com/reubeno/brush/pull/852))
- Experimental shell/terminal integration ([#872](https://github.com/reubeno/brush/pull/872))
- *(hooks)* Experimental `zsh`-style preexec/precmd hooks ([#652](https://github.com/reubeno/brush/pull/652))
- Rudimentary readline macro support ([#880](https://github.com/reubeno/brush/pull/880))
- Optional TOML-based config file for brush-shell ([#895](https://github.com/reubeno/brush/pull/895))
- *(read)* Fill out more of `read` builtin ([#914](https://github.com/reubeno/brush/pull/914))
- Add convenience cmd line option to set up xtrace ([#915](https://github.com/reubeno/brush/pull/915))
- Scaffolding for winnow parser ([#974](https://github.com/reubeno/brush/pull/974))
- *(completion)* Implement COMP_KEY and COMP_TYPE ([#1008](https://github.com/reubeno/brush/pull/1008))
- *(options)* Implement failglob option ([#1011](https://github.com/reubeno/brush/pull/1011))
- *(well-known-vars)* Implement BASH_ARGC and BASH_ARGV variables ([#1013](https://github.com/reubeno/brush/pull/1013))
- *(getopts)* Implement OPTERR semantics ([#1048](https://github.com/reubeno/brush/pull/1048))
- *(mapfile)* Add `-O` flag ([#558](https://github.com/reubeno/brush/pull/558))
- Add coproc AST and placeholder execution ([#1029](https://github.com/reubeno/brush/pull/1029))
- *(trap)* Implement ERR trap ([#1020](https://github.com/reubeno/brush/pull/1020))
- *(windows)* Add compat /dev/null handling ([#1044](https://github.com/reubeno/brush/pull/1044))
- *(coproc)* Implement coprocs ([#1068](https://github.com/reubeno/brush/pull/1068))
- Implement $_ well-known variable for last command argument ([#1030](https://github.com/reubeno/brush/pull/1030))
- Add optional bundling of coreutils builtins behind experimental feature flag ([#1031](https://github.com/reubeno/brush/pull/1031))

### 🐛 Bug Fixes

- Fill in more SourceLocation implementations ([#804](https://github.com/reubeno/brush/pull/804))
- Correct parsing of escaped backslash in ANSI-C quotes ([#850](https://github.com/reubeno/brush/pull/850))
- Bang formatted without space leads to invalid command ([#1109](https://github.com/reubeno/brush/pull/1109))
- Avoid allocating in `pre_exec` closure ([#777](https://github.com/reubeno/brush/pull/777))
- Improve "compgen -A {user,group}" ([#815](https://github.com/reubeno/brush/pull/815))
- Correct completion word tokenizing ([#816](https://github.com/reubeno/brush/pull/816))
- Honor mark-directories option ([#817](https://github.com/reubeno/brush/pull/817))
- Correct scope mismatch on error ([#821](https://github.com/reubeno/brush/pull/821))
- Suppress color when help is redirected ([#822](https://github.com/reubeno/brush/pull/822))
- History import robustness ([#878](https://github.com/reubeno/brush/pull/878))
- Avoid some allocation in brace-expansion ([#884](https://github.com/reubeno/brush/pull/884))
- Eliminate unneeded clone ([#886](https://github.com/reubeno/brush/pull/886))
- More direct Intos ([#946](https://github.com/reubeno/brush/pull/946))
- Remove ref-options ([#952](https://github.com/reubeno/brush/pull/952))
- Don't clobber builtin usage error if error display fails ([#965](https://github.com/reubeno/brush/pull/965))
- Use std::env::{split_paths, join_paths} for `PATH` splitting/combining ([#968](https://github.com/reubeno/brush/pull/968))
- Handle more readline macro forms ([#967](https://github.com/reubeno/brush/pull/967))
- Build for freebsd ([#980](https://github.com/reubeno/brush/pull/980))
- *(commands)* Use async pipe reads for command substitution
- Do not panic on file clone failure ([#1051](https://github.com/reubeno/brush/pull/1051))
- *(wasm)* Avoid panic in split_paths on wasm ([#1064](https://github.com/reubeno/brush/pull/1064))
- *(android)* Address android and 32-bit build issues ([#1070](https://github.com/reubeno/brush/pull/1070))
- Address hang on macos when used as login shell ([#1095](https://github.com/reubeno/brush/pull/1095))
- *(windows)* Fix /dev/null handling ([#1104](https://github.com/reubeno/brush/pull/1104))
- Expose -c flag via $- ([#767](https://github.com/reubeno/brush/pull/767))
- Don't allow builtin to invoke disabled builtin ([#719](https://github.com/reubeno/brush/pull/719))
- Getopts with no provided args ([#796](https://github.com/reubeno/brush/pull/796))
- Preserve $? during interactive shell prompt expansion ([#798](https://github.com/reubeno/brush/pull/798))
- Correct initialization of SHELL var ([#800](https://github.com/reubeno/brush/pull/800))
- Correct exit status handling in assignment-only cmds ([#801](https://github.com/reubeno/brush/pull/801))
- Extended test redirection ([#811](https://github.com/reubeno/brush/pull/811))
- Negative substring offsets ([#818](https://github.com/reubeno/brush/pull/818))
- Do not tilde-expand prompts ([#819](https://github.com/reubeno/brush/pull/819))
- Escaping in pattern character sets ([#824](https://github.com/reubeno/brush/pull/824))
- Special-casing for OLDPWD ([#825](https://github.com/reubeno/brush/pull/825))
- Omit unset vars from `set` builtin output ([#826](https://github.com/reubeno/brush/pull/826))
- Getopts short options ([#827](https://github.com/reubeno/brush/pull/827))
- Update default of COMP_WORDBREAKS ([#828](https://github.com/reubeno/brush/pull/828))
- Disable reedline for cmd mode / script exec ([#845](https://github.com/reubeno/brush/pull/845))
- Correct trace depth computation ([#847](https://github.com/reubeno/brush/pull/847))
- Assorted ANSI-C quote compat fixes ([#851](https://github.com/reubeno/brush/pull/851))
- True and false must ignore all args ([#865](https://github.com/reubeno/brush/pull/865))
- Gracefully handle closed stdout ([#873](https://github.com/reubeno/brush/pull/873))
- Gracefully handle closed stderr ([#875](https://github.com/reubeno/brush/pull/875))
- Delay profile/rc loading until after input backend attached ([#879](https://github.com/reubeno/brush/pull/879))
- *(completion)* Properly escape filenames with spaces or special chars ([#870](https://github.com/reubeno/brush/pull/870))
- *(unset)* Allow unsetting non-standard names ([#893](https://github.com/reubeno/brush/pull/893))
- *(test)* Correct handling of -- and invalid test exprs ([#894](https://github.com/reubeno/brush/pull/894))
- Reenable a few tests ([#896](https://github.com/reubeno/brush/pull/896))
- Handle redir form: `>&` PATH ([#908](https://github.com/reubeno/brush/pull/908))
- Compat fixes for \[ and \] in prompt expansion ([#909](https://github.com/reubeno/brush/pull/909))
- Parsing array accesses in array indices ([#910](https://github.com/reubeno/brush/pull/910))
- Add missing assignment arithmetic operators (|=, ^=) ([#911](https://github.com/reubeno/brush/pull/911))
- Correct overzealous regex escaping ([#912](https://github.com/reubeno/brush/pull/912))
- Systematic removal of problematic .unwrap() calls ([#921](https://github.com/reubeno/brush/pull/921))
- *(builtins)* Shadowing special builtins with funcs ([#922](https://github.com/reubeno/brush/pull/922))
- *(hash)* Accept names with slashes without lookup or error ([#924](https://github.com/reubeno/brush/pull/924))
- Ignore errors in -x tracing ([#928](https://github.com/reubeno/brush/pull/928))
- Trim space in test comparisons ([#950](https://github.com/reubeno/brush/pull/950))
- Exit status in command arg expansion failure ([#951](https://github.com/reubeno/brush/pull/951))
- Correctly handle pipe errors in builtins ([#953](https://github.com/reubeno/brush/pull/953))
- *(completion)* Provide default fallback completion for shell vars ([#954](https://github.com/reubeno/brush/pull/954))
- *(extglob)* Handle escaped parens ([#960](https://github.com/reubeno/brush/pull/960))
- *(patterns)* Correct backslash preservation ([#963](https://github.com/reubeno/brush/pull/963))
- *(return)* Exit code when stderr is not writable ([#975](https://github.com/reubeno/brush/pull/975))
- Check func names against reserved words ([#978](https://github.com/reubeno/brush/pull/978))
- *(test)* Resolve bash to absolute path before running compat tests ([#994](https://github.com/reubeno/brush/pull/994))
- *(pathsearch)* Include symlink'd executables in search ([#991](https://github.com/reubeno/brush/pull/991)) ([#992](https://github.com/reubeno/brush/pull/992))
- Readonly should operate on global scope ([#1003](https://github.com/reubeno/brush/pull/1003))
- *(peg)* Support legacy $[expr] arithmetic syntax ([#1004](https://github.com/reubeno/brush/pull/1004))
- *(arrays)* Add set + unset support for negative array indices ([#1005](https://github.com/reubeno/brush/pull/1005))
- *(expansion)* Allow ${#arr[i]} on unset variables with set -u ([#1007](https://github.com/reubeno/brush/pull/1007))
- *(bind)* Silently succeed when key bindings unavailable ([#1009](https://github.com/reubeno/brush/pull/1009))
- *(compgen)* Disable pathname expansion for -W word list ([#1010](https://github.com/reubeno/brush/pull/1010))
- *(completion)* Only deduplicate completions in interactive presentation ([#1012](https://github.com/reubeno/brush/pull/1012))
- *(trap)* - with multiple signals should clear all handlers ([#1015](https://github.com/reubeno/brush/pull/1015))
- *(heredoc)* Preserve quotes in heredoc expansion (rebased) ([#1014](https://github.com/reubeno/brush/pull/1014))
- *(arithmetic)* Allow space around prefix arithmetic operators ([#1016](https://github.com/reubeno/brush/pull/1016))
- *(arithmetic)* More faithful overflow/underflow handling ([#1017](https://github.com/reubeno/brush/pull/1017))
- *(arithmetic-for)* Correct parsing + eval issues ([#1018](https://github.com/reubeno/brush/pull/1018))
- *(arithmetic)* Implement high-radix literals ([#1019](https://github.com/reubeno/brush/pull/1019))
- *(printf)* Quote empty strings as '' in printf %q ([#1026](https://github.com/reubeno/brush/pull/1026))
- *(expansion)* Correct descending brace sequence expansion with step ([#1025](https://github.com/reubeno/brush/pull/1025))
- *(builtins)* Add builtin commands to `compgen -A command`([#997](https://github.com/reubeno/brush/pull/997)) ([#1027](https://github.com/reubeno/brush/pull/1027))
- *(expansion)* Preserve quoted empty strings with nullglob enabled ([#1035](https://github.com/reubeno/brush/pull/1035))
- *(command_subst)* Strip NULL bytes from command substitutions ([#1049](https://github.com/reubeno/brush/pull/1049))
- Return 2 for unknown cmdline options ([#1050](https://github.com/reubeno/brush/pull/1050))
- *(complete)* Complete current and parent directories. ([#887](https://github.com/reubeno/brush/pull/887))
- *(arithmetic)* Detect infinite arithmetic var reference recursion ([#1021](https://github.com/reubeno/brush/pull/1021))
- *(tokenizer)* Prioritize heredoc body over terminating char in `next_token_until` ([#1055](https://github.com/reubeno/brush/pull/1055))
- *(tokenizer)* Unquote heredoc end tag in `delimit_current_token` ([#1056](https://github.com/reubeno/brush/pull/1056)) ([#1057](https://github.com/reubeno/brush/pull/1057))
- Use i64 for exit ([#1065](https://github.com/reubeno/brush/pull/1065))
- *(heredoc)* Handle unbalanced single quotes and backticks in heredoc bodies inside `$(…)`  ([#1067](https://github.com/reubeno/brush/pull/1067))
- *(cmdline)* Handle `--` as option terminator for `-c` flag ([#1076](https://github.com/reubeno/brush/pull/1076))
- *(login)* Correctly reflect login option via shopt ([#1078](https://github.com/reubeno/brush/pull/1078))
- *(-c)* Call EXIT traps on exit; add tests ([#1080](https://github.com/reubeno/brush/pull/1080))
- *(builtins)* Add reserved keywords to `compgen -A command` ([#1047](https://github.com/reubeno/brush/pull/1047))
- *(-f)* Implement -f command line option ([#1091](https://github.com/reubeno/brush/pull/1091))
- *(parser)* Preserve whitespace in regex operands of extended tests ([#1096](https://github.com/reubeno/brush/pull/1096))
- *(wellknownvars)* Set OSTYPE on macOS ([#1097](https://github.com/reubeno/brush/pull/1097))

### 🚜 Refactor

- Reexport parser types ([#784](https://github.com/reubeno/brush/pull/784))
- Reorganize existing PEG parser ([#899](https://github.com/reubeno/brush/pull/899))
- Split shell.rs => shell/*.rs ([#945](https://github.com/reubeno/brush/pull/945))
- Use let chains ([#982](https://github.com/reubeno/brush/pull/982))
- Make `try_iter_open_fds` fully iterator-based ([#1001](https://github.com/reubeno/brush/pull/1001))
- Rename TokenLocation -> SourceSpan ([#788](https://github.com/reubeno/brush/pull/788))
- Merge func call and script stacks ([#789](https://github.com/reubeno/brush/pull/789))
- Merge brushinfo and brushctl ([#813](https://github.com/reubeno/brush/pull/813))
- Shell / interactive-shell layering ([#832](https://github.com/reubeno/brush/pull/832))
- Make Shell fields private ([#900](https://github.com/reubeno/brush/pull/900))
- Extract build/test/validate commands into `xtask` ([#898](https://github.com/reubeno/brush/pull/898))
- Reuse test harness for non-oracle tests ([#916](https://github.com/reubeno/brush/pull/916))
- [**breaking**] Make `Shell` generic over `ShellExtensions` type param ([#941](https://github.com/reubeno/brush/pull/941))

### 📚 Documentation

- Update todo comments ([#787](https://github.com/reubeno/brush/pull/787))
- Update readme ([#853](https://github.com/reubeno/brush/pull/853))
- Update readme ([#976](https://github.com/reubeno/brush/pull/976))
- Minor updates ([#1061](https://github.com/reubeno/brush/pull/1061))
- Update compat tables ([#1062](https://github.com/reubeno/brush/pull/1062))
- Update header image on readme ([#1113](https://github.com/reubeno/brush/pull/1113))
- Update reference docs ([#1114](https://github.com/reubeno/brush/pull/1114))

### ⚡ Performance

- Assorted performance changes ([#936](https://github.com/reubeno/brush/pull/936))
- Prealloc capacity for some vectors, remove unnecessary Strings ([#940](https://github.com/reubeno/brush/pull/940))
- Address regex cache contention ([#1043](https://github.com/reubeno/brush/pull/1043))

### 🧪 Testing

- *(parser)* Per syntax element tests ([#938](https://github.com/reubeno/brush/pull/938))
- Add error handling tests, resolve nextest issue ([#768](https://github.com/reubeno/brush/pull/768))
- Extend IFS tests ([#775](https://github.com/reubeno/brush/pull/775))
- Add not-yet-passing call-stack/`caller` tests ([#785](https://github.com/reubeno/brush/pull/785))
- Skip confused test ([#820](https://github.com/reubeno/brush/pull/820))
- Refactor + augment expansion test cases ([#846](https://github.com/reubeno/brush/pull/846))
- More tests for traps and related options ([#947](https://github.com/reubeno/brush/pull/947))
- Add mkosi config ([#1093](https://github.com/reubeno/brush/pull/1093))
- Add basic wasm32-wasip2 tests to CI ([#1098](https://github.com/reubeno/brush/pull/1098))
- *(nameref)* Add large set of nameref-focused tests ([#1102](https://github.com/reubeno/brush/pull/1102))
- Temporarily disable flaky test ([#1103](https://github.com/reubeno/brush/pull/1103))

### ⚙️ Miscellaneous Tasks

- Rename feature: fuzz-testing => arbitrary ([#844](https://github.com/reubeno/brush/pull/844))
- Switch non-Unix plats to use std::env::home_dir ([#830](https://github.com/reubeno/brush/pull/830))
- Update dependencies ([#985](https://github.com/reubeno/brush/pull/985))
- *(clippy)* Fix nightly lints ([#998](https://github.com/reubeno/brush/pull/998))
- Enable windows tests ([#1083](https://github.com/reubeno/brush/pull/1083))
- Replace git expectrl dependency with v0.8.0 release ([#779](https://github.com/reubeno/brush/pull/779))
- Workflow sync + cleanup ([#790](https://github.com/reubeno/brush/pull/790))
- Upgrade several dependencies ([#843](https://github.com/reubeno/brush/pull/843))
- Add placeholders for new named options (not yet supported) ([#848](https://github.com/reubeno/brush/pull/848))
- Remove pprof dependency ([#891](https://github.com/reubeno/brush/pull/891))
- Update dependencies ([#892](https://github.com/reubeno/brush/pull/892))
- Update dependencies ([#923](https://github.com/reubeno/brush/pull/923))
- Upgrade deps ([#942](https://github.com/reubeno/brush/pull/942))
- Update dependencies ([#966](https://github.com/reubeno/brush/pull/966))
- Update dependencies ([#977](https://github.com/reubeno/brush/pull/977))
- Upgrade MSRV to 1.88 and update deps ([#981](https://github.com/reubeno/brush/pull/981))
- Upgrade dependencies ([#1063](https://github.com/reubeno/brush/pull/1063))
- Address clippy error in xtask ([#1108](https://github.com/reubeno/brush/pull/1108))

### Build

- *(deps)* Bump utf8-chars from 3.0.5 to 3.0.6 in the cargo group ([#781](https://github.com/reubeno/brush/pull/781))
- *(deps)* Bump the cargo group with 2 updates ([#807](https://github.com/reubeno/brush/pull/807))
- *(deps)* Bump the cargo group with 3 updates ([#836](https://github.com/reubeno/brush/pull/836))
- *(deps)* Bump uuid from 1.18.1 to 1.19.0 in the cargo group ([#840](https://github.com/reubeno/brush/pull/840))
- *(deps)* Bump the cargo group with 3 updates ([#882](https://github.com/reubeno/brush/pull/882))
- *(deps)* Bump the cargo group with 3 updates ([#925](https://github.com/reubeno/brush/pull/925))
- *(deps)* Bump thiserror from 2.0.17 to 2.0.18 in the cargo group ([#961](https://github.com/reubeno/brush/pull/961))
- *(deps)* Bump the cargo group with 8 updates ([#1024](https://github.com/reubeno/brush/pull/1024))
- *(deps)* Bump the cargo group with 4 updates ([#1032](https://github.com/reubeno/brush/pull/1032))
- *(deps)* Bump the cargo group with 6 updates ([#1041](https://github.com/reubeno/brush/pull/1041))
- *(deps)* Bump the cargo group with 7 updates ([#1059](https://github.com/reubeno/brush/pull/1059))
- *(deps)* Bump the cargo group with 4 updates ([#1081](https://github.com/reubeno/brush/pull/1081))
- *(deps)* Bump the cargo group with 4 updates ([#1088](https://github.com/reubeno/brush/pull/1088))
- *(deps)* Bump the cargo group with 5 updates ([#1112](https://github.com/reubeno/brush/pull/1112))

<!-- generated by git-cliff -->

## [0.3.0] - 2025-11-17

### 🚀 Features

- Add ShellBuilder, ParserBuilder ([#651](https://github.com/reubeno/brush/pull/651))
- [**breaking**] Refactor SourcePosition to use Arc ([#727](https://github.com/reubeno/brush/pull/727))
- *(bind)* Extend key binding support ([#740](https://github.com/reubeno/brush/pull/740))
- Enable custom error formatting ([#722](https://github.com/reubeno/brush/pull/722))
- [**breaking**] Introduce SourceLocation ([#728](https://github.com/reubeno/brush/pull/728))
- Implement fc builtin ([#739](https://github.com/reubeno/brush/pull/739))
- Implement alternate arithmetic for syntax ([#744](https://github.com/reubeno/brush/pull/744))
- Implement BASH_XTRACEFD ([#747](https://github.com/reubeno/brush/pull/747))
- Revisit how ExecutionParameters layer open files atop the Shell ([#749](https://github.com/reubeno/brush/pull/749))
- Basic support for trap EXIT ([#750](https://github.com/reubeno/brush/pull/750))
- Implement opt-in fd inheritance from host env ([#753](https://github.com/reubeno/brush/pull/753))

### 🐛 Bug Fixes

- Workaround error on nightly ([#711](https://github.com/reubeno/brush/pull/711))
- Comment unsafe blocks + better harden 1 block ([#733](https://github.com/reubeno/brush/pull/733))
- Don't fail importing unreadable history lines ([#710](https://github.com/reubeno/brush/pull/710))
- Tokenizer handling of here docs in quoted command substitutions ([#716](https://github.com/reubeno/brush/pull/716))
- Address lint errors from stable + nightly ([#723](https://github.com/reubeno/brush/pull/723))
- *(builtins)* Correct `read` var update on empty input ([#729](https://github.com/reubeno/brush/pull/729))
- Address race conditions in basic input tests ([#730](https://github.com/reubeno/brush/pull/730))
- Do not pass along exported-but-unset vars ([#732](https://github.com/reubeno/brush/pull/732))
- *(builtins)* Suppress error when type -p sees no command ([#745](https://github.com/reubeno/brush/pull/745))
- Command substitutions with large output ([#748](https://github.com/reubeno/brush/pull/748))
- Expansion in select double-quoted parameter exprs ([#751](https://github.com/reubeno/brush/pull/751))
- Correct expansion behavior in prompts ([#756](https://github.com/reubeno/brush/pull/756))
- Escaping and pipeline parse issues ([#762](https://github.com/reubeno/brush/pull/762))

### 🚜 Refactor

- Use `Shell` builder pattern in more code ([#688](https://github.com/reubeno/brush/pull/688))
- Os_pipe::pipe() -> std::io::pipe() ([#695](https://github.com/reubeno/brush/pull/695))
- Extract script + function call stacks to their own modules ([#709](https://github.com/reubeno/brush/pull/709))
- Update Shell::new() to take creation options as owned ([#689](https://github.com/reubeno/brush/pull/689))
- Move builtins into their own crate ([#690](https://github.com/reubeno/brush/pull/690))
- Shell struct API improvements ([#692](https://github.com/reubeno/brush/pull/692))
- Error/result type overhaul ([#720](https://github.com/reubeno/brush/pull/720))
- Move more platform-specific code under sys ([#735](https://github.com/reubeno/brush/pull/735))

### 📚 Documentation

- Update readme ([#742](https://github.com/reubeno/brush/pull/742))

### 🧪 Testing

- Add not-yet-passing tests for set -u and set -e ([#736](https://github.com/reubeno/brush/pull/736))
- Add new command substitution test case ([#752](https://github.com/reubeno/brush/pull/752))

### ⚙️ Miscellaneous Tasks

- Run static code checks on linux + macOS too ([#678](https://github.com/reubeno/brush/pull/678))
- Fix build error with cargo nightly ([#687](https://github.com/reubeno/brush/pull/687))
- *(msrv)* [**breaking**] Upgrade MSRV to 1.87.0 ([#693](https://github.com/reubeno/brush/pull/693))
- Fix benchmark execution ([#691](https://github.com/reubeno/brush/pull/691))
- Update dependencies ([#696](https://github.com/reubeno/brush/pull/696))
- Update dependencies ([#757](https://github.com/reubeno/brush/pull/757))
- Add brush crate + docs publishing ([#760](https://github.com/reubeno/brush/pull/760))

### Build

- *(deps)* Bump procfs from 0.17.0 to 0.18.0 in the cargo group across 1 directory ([#671](https://github.com/reubeno/brush/pull/671))
- *(deps)* Bump bon from 3.7.2 to 3.8.0 in the cargo group ([#698](https://github.com/reubeno/brush/pull/698))
- *(deps)* Bump the cargo group with 4 updates ([#676](https://github.com/reubeno/brush/pull/676))
- *(deps)* Bump serde from 1.0.221 to 1.0.223 in the cargo group ([#680](https://github.com/reubeno/brush/pull/680))
- *(deps)* Bump the cargo group with 5 updates ([#684](https://github.com/reubeno/brush/pull/684))
- *(deps)* Bump the cargo group with 3 updates ([#686](https://github.com/reubeno/brush/pull/686))
- *(deps)* Bump the cargo group with 5 updates ([#708](https://github.com/reubeno/brush/pull/708))
- *(deps)* Bump the cargo group with 8 updates ([#713](https://github.com/reubeno/brush/pull/713))
- *(deps)* Bump the cargo group with 3 updates ([#755](https://github.com/reubeno/brush/pull/755))

<!-- generated by git-cliff -->

## [0.2.23] - 2025-08-30

### 🐛 Bug Fixes

- *(cmdline)* Correct exit code for `--version` + `--help` ([#667](https://github.com/reubeno/brush/pull/667))

<!-- generated by git-cliff -->
## [0.2.22] - 2025-08-29

### 🚀 Features

- *(diag)* Add minimal miette support to parser ([#648](https://github.com/reubeno/brush/pull/648))

### 🐛 Bug Fixes

- Exclude bind-bound commands from history ([#650](https://github.com/reubeno/brush/pull/650))
- *(cmdline)* Improve error handling for unknown cmdline options ([#656](https://github.com/reubeno/brush/pull/656))

### 📚 Documentation

- Update readme ([#657](https://github.com/reubeno/brush/pull/657))

### ⚙️ Miscellaneous Tasks

- Additional clippy fixes ([#661](https://github.com/reubeno/brush/pull/661))
- Downgrade homedir ([#662](https://github.com/reubeno/brush/pull/662))
- Address warnings on windows targets ([#663](https://github.com/reubeno/brush/pull/663))

### Build

- *(deps)* Bump the cargo group with 4 updates ([#654](https://github.com/reubeno/brush/pull/654))
- *(deps)* Bump the cargo group with 4 updates ([#659](https://github.com/reubeno/brush/pull/659))
- *(deps)* Bump tracing-subscriber from 0.3.19 to 0.3.20 in the cargo group ([#664](https://github.com/reubeno/brush/pull/664))

<!-- generated by git-cliff -->
## [0.2.21] - 2025-08-13

### 🚀 Features

- *(history)* Implement history builtin ([#599](https://github.com/reubeno/brush/pull/599))

### 🐛 Bug Fixes

- *(parser)* Resolve issue with parser confusing subshell for arith expr ([#624](https://github.com/reubeno/brush/pull/624))
- *(expansion)* Support broader set of nested brace expansions ([#625](https://github.com/reubeno/brush/pull/625))
- Correct obvious string indexing errors ([#641](https://github.com/reubeno/brush/pull/641))
- Fixes for preexec-style bash extensions ([#643](https://github.com/reubeno/brush/pull/643))
- Prepare tests to run against bash-5.3 ([#610](https://github.com/reubeno/brush/pull/610))
- *(unset)* Correct unset of associative array element ([#626](https://github.com/reubeno/brush/pull/626))
- *(declare)* Refine varname validation ([#629](https://github.com/reubeno/brush/pull/629))
- Hyphenated script args ([#630](https://github.com/reubeno/brush/pull/630))
- Special case for command subst ([#632](https://github.com/reubeno/brush/pull/632))

### 📚 Documentation

- *(readme)* Update Arch Linux install instructions ([#604](https://github.com/reubeno/brush/pull/604))
- Adds homebrew section to README installation instructions ([#638](https://github.com/reubeno/brush/pull/638))

### ⚙️ Miscellaneous Tasks

- Publish license notices in CD flow ([#622](https://github.com/reubeno/brush/pull/622))
- Update dependencies ([#623](https://github.com/reubeno/brush/pull/623))
- Cleanup allow attributes, switch to expect where possible ([#642](https://github.com/reubeno/brush/pull/642))

### Build

- *(deps)* Bump indenter from 0.3.3 to 0.3.4 in the cargo group ([#627](https://github.com/reubeno/brush/pull/627))
- *(deps)* Bump the cargo group with 3 updates ([#612](https://github.com/reubeno/brush/pull/612))
- *(deps)* Bump the cargo group with 2 updates ([#613](https://github.com/reubeno/brush/pull/613))
- *(deps)* Bump the cargo group with 2 updates ([#619](https://github.com/reubeno/brush/pull/619))
- *(deps)* Bump the cargo group across 1 directory with 5 updates ([#609](https://github.com/reubeno/brush/pull/609))
- *(deps)* Bump tokio from 1.46.1 to 1.47.0 in the cargo group ([#616](https://github.com/reubeno/brush/pull/616))
- *(deps)* Bump the cargo group with 2 updates ([#621](https://github.com/reubeno/brush/pull/621))

<!-- generated by git-cliff -->
## [0.2.20] - 2025-07-04

### 🚀 Features

- *(api)* API usability improvements for `Shell::invoke_function` (#596)
- Enable -o/+o on brush command line (#590)

### 🐛 Bug Fixes

- *(dot)* Only shadow args when some provided to `source` (#582)

### 🚜 Refactor

- *(ShellValue)* Take in an `Into` (#598)

### 📚 Documentation

- README.md installation updates (#580)
- Update README.md badges (#588)

### 🧪 Testing

- Tag test binary dependencies (#585)
- Add test cases for open issues (#587)
- Add not-yet-passing history tests (#591)

### ⚙️ Miscellaneous Tasks

- Update dependencies + deny policy (#586)
- Remove unneeded dev deps from 'test-with' (#594)
- Update dependencies (#601)

### Build

- *(deps)* Bump test-with from 0.15.1 to 0.15.2 in the cargo group (#593)

<!-- generated by git-cliff -->
## [0.2.19] - 2025-06-25

### 🚀 Features

- *(AndOrList)* Add iteration abilities (#512)
- Generic Shell functions to parse a script from bytes (#509)
- Ability to clear all functions from shell environment (#546)
- *(vars)* Implement correct updating for -u/-c/-l vars (#529)
- *(env)* Introduce BRUSH_VERSION variable (#531)
- Enable cargo-binstall to work with brush (#536)
- *(parser)* Add gettext enabled quotes (#446)
- *(printf)* Replace printf impl with uucore wrapper (#552)
- *(args)* Add --rcfile command-line option (#568)

### 🐛 Bug Fixes

- *(vars)* Ensure effective GID is first in GROUPS (#526)
- *(traps)* Add stub definition for RETURN trap (#559)
- *(typeset)* Mark typeset as a declaration builtin (#517)
- *(tokenizer)* Correctly treat $( (...) ) as a cmd substitution (#521)
- *(backquote)* Correctly handle trailing backslash in backquoted command (#524)
- *(test)* Reuse -ef, -nt, -ot support (#525)
- *(arithmetic)* Permit space after unary operators (#527)
- *(tokenizer)* Correctly handle here docs terminated by EOF (#551)
- *(interactive)* Fix behavior of cmds piped to stdin (#539)
- *(functions)* Allow func names to contain slashes (#560)
- *(tokenizer)* Handle escaped single-quote in ANSI-C quoted string (#561)
- *(arithmetic)* Correct left shift handling (#562)
- *(local)* Enable use of local to detect function (#565)
- *(expansion)* Handle signed numbers in brace-expansion ranges (#566)
- *(redirection)* Assorted fixes to redirection (#567)
- *(prompt)* Implement `\A` (#569)
- *(expansion)* Correct ${!PARAM@...} (#570)
- *(expansion)* Fix parsing escaped single-quotes in ANSI-C strs (#571)
- *(for/case)* Allow reserved words in for word lists (#578)

### 🚜 Refactor

- *(parser)* Abstract parse errors (#574)

### ⚡ Performance

- Remove redundant lookups in path searching (#573)

### 🧪 Testing

- *(parser)* Enable serde::Serialize on AST et al. for test targets (#544)
- *(tokenizer)* Adopt insta for tokenizer tests (#550)
- *(parser)* Start using insta crate for snapshot-testing parser (#545)

### ⚙️ Miscellaneous Tasks

- Upgrade MSRV to 1.85.0 (#553)
- Upgrade crates to Rust 2024 edition (#554)
- Enable more lints + fixes (#555)
- Upgrade dependencies (#556)

### Build

- *(deps)* Bump pprof from 0.14.0 to 0.15.0 in the cargo group (#542)
- *(deps)* Bump the cargo group with 3 updates (#522)

<!-- generated by git-cliff -->
## [0.2.18] - 2025-05-22

### 🚀 Features

- *(apply_unary_predicate_to_str)* Implement `-R` operand
- *(apply_binary_predicate)* Implement `-ef` operand
- *(apply_binary_predicate)* Implement `-nt` operand
- *(ShellValue)* Add `is_set` method
- *(apply_binary_predicate)* Implement `-ot` operand
- *(Shell)* Add methods to add environmental variables and builtins ([#447](https://github.com/reubeno/brush/pull/447))
- Implement {cd,pwd} -{L,P} ([#458](https://github.com/reubeno/brush/pull/458))
- *(builtins)* Implement initial (partial) `bind -x` support ([#478](https://github.com/reubeno/brush/pull/478))
- *(mapfile)* Register also as readarray ([#486](https://github.com/reubeno/brush/pull/486))
- *(funcs)* Implement function exporting/importing ([#492](https://github.com/reubeno/brush/pull/492))
- *(mapfile)* Implement `-n`, `-d`, `-s` ([#490](https://github.com/reubeno/brush/pull/490))
- *(ulimit)* Implement ulimit builtin ([#482](https://github.com/reubeno/brush/pull/482))

### 🐛 Bug Fixes

- *(break,continue)* Use `default_value_t` for flag values ([#493](https://github.com/reubeno/brush/pull/493))
- *(help)* Better compact argument help ([#499](https://github.com/reubeno/brush/pull/499))
- *(tests)* Wrong boolean operator
- *(trap)* Handle '-' unregistration syntax ([#452](https://github.com/reubeno/brush/pull/452))
- *(complete)* Fixes + tests for "declare -r" ([#462](https://github.com/reubeno/brush/pull/462))
- Enable unnameable_types lint and mitigate errors ([#459](https://github.com/reubeno/brush/pull/459))
- *(builtin)* Fix issues with 'builtin' invoking declaration builtins ([#466](https://github.com/reubeno/brush/pull/466))
- *(declare)* Suppress errors for non-existent functions in declare -f / -F ([#467](https://github.com/reubeno/brush/pull/467))
- *(expansion)* Allow negative subscripts with indexed arrays + slices ([#468](https://github.com/reubeno/brush/pull/468))
- Allow "bind" usage in .bashrc ([#485](https://github.com/reubeno/brush/pull/485))
- *(expansion)* Fix issues parsing backquoted commands nested in single/double-quoted strings ([#491](https://github.com/reubeno/brush/pull/491))
- Fix typos + add spell-checking PR check ([#501](https://github.com/reubeno/brush/pull/501))

### 📚 Documentation

- Update readme ([#481](https://github.com/reubeno/brush/pull/481))
- Add discord invite to readme ([#494](https://github.com/reubeno/brush/pull/494))

### 🧪 Testing

- *(extended-tests)* Explicitly set modified date on test files ([#453](https://github.com/reubeno/brush/pull/453))
- Add compat test for $_ ([#480](https://github.com/reubeno/brush/pull/480))
- Add known-failing tests to reproduce reported issues ([#483](https://github.com/reubeno/brush/pull/483))

### ⚙️ Miscellaneous Tasks

- Enable (and fix) current checks across all workspace crates ([#457](https://github.com/reubeno/brush/pull/457))
- *(extended_tests)* Add `-ef`, `-nt`, and `-ot`
- Upgrade dependencies: reedline, nix, clap, thiserror, etc. ([#456](https://github.com/reubeno/brush/pull/456))
- Better log + connect unimpl functionality with GH issues ([#476](https://github.com/reubeno/brush/pull/476))

### Build

- *(deps)* Bump the cargo group with 3 updates ([#461](https://github.com/reubeno/brush/pull/461))
- *(deps)* Bump the cargo group with 2 updates ([#487](https://github.com/reubeno/brush/pull/487))

### Ref

- *(Shell)* Turn `&Path` parameters into `AsRef<Path>` ([#448](https://github.com/reubeno/brush/pull/448))

<!-- generated by git-cliff -->
## [0.2.17] - 2025-04-24

### 🚀 Features

- *(Shell)* Add `get_env_var` method for more generic variable returns (#438)

### 🐛 Bug Fixes

- Honor COMP_WORDBREAKS in completion tokenization (#407)
- Handle complete builtin run without options (#435)

### 📚 Documentation

- Add instructions for installing from the AUR (#433)

### 🧪 Testing

- Implement --skip in brush-compat-tests harness (#432)

### ⚙️ Miscellaneous Tasks

- *(Shell)* Use relaxed typing for string input (#437)
- Enable building for wasm32-unknown-unknown (#425)

### Build

- *(deps)* Bump rand from 0.9.0 to 0.9.1 in the cargo group (#431)
- *(deps)* Bump the cargo group with 3 updates (#426)
- *(deps)* Bump the cargo group with 2 updates (#427)

<!-- generated by git-cliff -->
## [0.2.16] - 2025-03-25

### 🚀 Features

- *(arithmetic)* Support explicit base#literal in arithmetic (#388)
- Implement `command -p` (#402)

### 🐛 Bug Fixes

- Default PS1 and PS2 in interactive mode (#390)
- *(builtins)* Implement command-less exec semantics with open fds (#384)
- *(builtins)* Correct read handling of IFS/space (#385)
- *(extglob)* Correct handling of extglobs with empty branches (#386)
- Correct path tests on empty strings (#391)
- Allow newline in empty array assignment (#405)
- Improve panic handling output (#409)
- *(regex)* Enable multiline mode for extended test regexes (#416)
- Parse '#' as char only if inside a variable expansion (#418)

### 📚 Documentation

- Symlink licenses under crate dirs (#400)

### 🧪 Testing

- Add more linux distros to test matrix (#412)
- Enable testing on nixos/nix container image (#413)

### ⚙️ Miscellaneous Tasks

- Upgrade cached crate (#398)
- Rewrite dir diffing test code to avoid deps

### Build

- *(deps)* Bump the cargo group with 2 updates (#393)
- *(deps)* Bump whoami from 1.5.2 to 1.6.0 in the cargo group (#423)
- *(deps)* Bump the cargo group with 2 updates (#389)
- *(deps)* Bump serde from 1.0.218 to 1.0.219 in the cargo group (#401)
- *(deps)* Bump the cargo group with 2 updates (#410)

<!-- generated by git-cliff -->
## [0.2.15] - 2025-02-03

### 🚀 Features

- *(continue)* Implement continue n for n >= 2 ([#326](https://github.com/reubeno/brush/pull/326))
- *(options)* Implement dotglob semantics ([#332](https://github.com/reubeno/brush/pull/332))
- *(options)* Implement "set -t" ([#333](https://github.com/reubeno/brush/pull/333))
- *(options)* Implement "set -a" ([#336](https://github.com/reubeno/brush/pull/336))
- *(env)* Introduce dynamic variables ([#360](https://github.com/reubeno/brush/pull/360))

### 🐛 Bug Fixes

- *(builtins)* Skip unenumerable vars in set builtin ([#322](https://github.com/reubeno/brush/pull/322))
- *(expansion)* Handle negative substring offset ([#372](https://github.com/reubeno/brush/pull/372))
- *(completion)* Better handle native errors in completion funcs ([#373](https://github.com/reubeno/brush/pull/373))
- *(builtins)* Correct parsing of bind positional arg ([#381](https://github.com/reubeno/brush/pull/381))
- *(patterns)* Fix incorrect parse of char ranges ([#323](https://github.com/reubeno/brush/pull/323))
- *(exit)* Correct exit semantics in various compund statements ([#347](https://github.com/reubeno/brush/pull/347))
- *(for)* Correct semantics for "for" without "in" ([#348](https://github.com/reubeno/brush/pull/348))
- Correct semantics of = in non-extended test commands ([#349](https://github.com/reubeno/brush/pull/349))
- *(return)* Error if return used outside sourced script or function ([#350](https://github.com/reubeno/brush/pull/350))
- *(arithmetic)* Recursively evaluate var references ([#351](https://github.com/reubeno/brush/pull/351))
- *(arithmetic)* Fixes for nested parenthesis parsing in arithmetic ([#353](https://github.com/reubeno/brush/pull/353))
- *(builtins)* Fix set builtin handling of - and -- ([#354](https://github.com/reubeno/brush/pull/354))
- *(builtins)* Do not interpret --help in command builtin command args ([#355](https://github.com/reubeno/brush/pull/355))
- *(builtins)* Correct more 'set' argument parsing ([#356](https://github.com/reubeno/brush/pull/356))
- *(variables)* More correct handling of integer variables ([#357](https://github.com/reubeno/brush/pull/357))
- *(redirection)* Make sure redirection fd + operator are contiguous ([#359](https://github.com/reubeno/brush/pull/359))
- Better error when cwd is gone ([#370](https://github.com/reubeno/brush/pull/370))
- *(builtins)* Fix read builtin ignoring tab chars ([#371](https://github.com/reubeno/brush/pull/371))
- Propagate execution parameters more thoroughly ([#374](https://github.com/reubeno/brush/pull/374))
- *(redirection)* Allow continuing past redir errors ([#375](https://github.com/reubeno/brush/pull/375))

### ⚡ Performance

- Remove unneeded string cloning for arithmetic eval ([#324](https://github.com/reubeno/brush/pull/324))
- Simplify export enumeration ([#363](https://github.com/reubeno/brush/pull/363))
- Skip word parsing if no expansion required ([#365](https://github.com/reubeno/brush/pull/365))
- Minor optimizations for shell create + command run ([#362](https://github.com/reubeno/brush/pull/362))

### 🧪 Testing

- *(perf)* Update tokenizer/parser benchmarks ([#321](https://github.com/reubeno/brush/pull/321))
- Resolve false errors about side effects in bash-completion tests ([#379](https://github.com/reubeno/brush/pull/379))

### ⚙️ Miscellaneous Tasks

- Remove some unneeded `pub(crate)` visibility annotations ([#346](https://github.com/reubeno/brush/pull/346))
- Remove unneeded result wrappings ([#367](https://github.com/reubeno/brush/pull/367))
- Remove a few object clones ([#368](https://github.com/reubeno/brush/pull/368))
- Update readme ([#331](https://github.com/reubeno/brush/pull/331))
- Add pattern and expansion tests to track newly filed issues ([#330](https://github.com/reubeno/brush/pull/330))
- Minor cleanups and test additions ([#364](https://github.com/reubeno/brush/pull/364))
- Fix rng warnings ([#378](https://github.com/reubeno/brush/pull/378))

### Build

- *(deps)* Bump indexmap from 2.7.0 to 2.7.1 in the cargo group ([#334](https://github.com/reubeno/brush/pull/334))
- *(deps)* Bump the cargo group across 1 directory with 4 updates ([#320](https://github.com/reubeno/brush/pull/320))
- *(deps)* Bump the cargo group with 3 updates ([#376](https://github.com/reubeno/brush/pull/376))

<!-- generated by git-cliff -->
## [0.2.14] - 2025-01-10

### 🚀 Features

- *(prompts)* Enable PS0, custom right-side prompts, more ([#278](https://github.com/reubeno/brush/pull/278))
- *(completion)* Programmable completion support for filters + commands
- *(non-posix)* Implement `time` keyword ([#310](https://github.com/reubeno/brush/pull/310))
- *(builtins)* Implement suspend ([#311](https://github.com/reubeno/brush/pull/311))
- *(set)* Implement nullglob option ([#279](https://github.com/reubeno/brush/pull/279))
- *(set)* Implement nocaseglob + nocasematch options ([#282](https://github.com/reubeno/brush/pull/282))
- *(builtins)* Add minimal mapfile + bind impls
- *(debug)* Improved function tracing capabilities
- *(options)* Implement lastpipe option
- Implement brace expansion ([#290](https://github.com/reubeno/brush/pull/290))
- *(options)* Implement noclobber option (a.k.a. -C) ([#291](https://github.com/reubeno/brush/pull/291))
- *(builtins)* Implement more of kill builtin ([#305](https://github.com/reubeno/brush/pull/305))
- *(builtins)* Implement times builtin ([#309](https://github.com/reubeno/brush/pull/309))

### 🐛 Bug Fixes

- Correct sh mode vs posix mode confusion for syntax extensions
- Assorted non-fatal clippy warnings ([#274](https://github.com/reubeno/brush/pull/274))
- *(builtins)* Correct behavior of set builtin with no args
- More consistently honor shell options when invoking the tokenizer
- Update COMP_WORDBREAKS default value
- Honor extglob for expansion transformations
- Sync PWD with actual workdir on launch
- *(jobs)* Only report job status when job control option is enabled ([#306](https://github.com/reubeno/brush/pull/306))
- Stop incorrectly parsing assignment as function def ([#273](https://github.com/reubeno/brush/pull/273))
- Multiple issues blocking docker cmd completion ([#275](https://github.com/reubeno/brush/pull/275))
- Improve substring ops with multi-byte chars ([#280](https://github.com/reubeno/brush/pull/280))
- Better handle escape chars in pattern bracket exprs ([#281](https://github.com/reubeno/brush/pull/281))
- *(completion)* Multiple fixes for compgen builtin usage
- *(regex)* Work around incompatibilities between shell + rust regexes
- *(extendedtests)* Add missing arithmetic eval in extended tests
- *(command)* Handle sending basic command errors to redirected stderr
- Improve accuracy of negative extglobs
- Implement date and time in prompts ([#298](https://github.com/reubeno/brush/pull/298))
- *(completion)* Handle -o {default,dirnames,plusdirs} ([#300](https://github.com/reubeno/brush/pull/300))
- *(expansion)* Correct length for 1-element arrays ([#316](https://github.com/reubeno/brush/pull/316))
- Correct issues with `!` extglobs and compgen -X ([#317](https://github.com/reubeno/brush/pull/317))

### 📚 Documentation

- Update README to reflect test expansion

### ⚡ Performance

- Cache parsing for arithmetic expressions ([#301](https://github.com/reubeno/brush/pull/301))
- Remove unneeded async from arithmetic eval ([#312](https://github.com/reubeno/brush/pull/312))
- Remove setup operations from microbenchmarks ([#307](https://github.com/reubeno/brush/pull/307))
- Reimplement colon command as a "simple builtin" ([#315](https://github.com/reubeno/brush/pull/315))

### 🧪 Testing

- *(completion)* Add another completion test
- *(completion)* Enable use of pexpect et al. with basic input backend

### ⚙️ Miscellaneous Tasks

- Update comments
- Improve tracing for completion function invocation
- Remove unneeded helper code
- Address warnings ([#313](https://github.com/reubeno/brush/pull/313))

### Build

- *(deps)* Bump the cargo group with 3 updates ([#285](https://github.com/reubeno/brush/pull/285))
- *(deps)* Bump the cargo group with 4 updates ([#289](https://github.com/reubeno/brush/pull/289))
- *(deps)* Bump the cargo group with 3 updates ([#294](https://github.com/reubeno/brush/pull/294))
- *(deps)* Bump anyhow from 1.0.94 to 1.0.95 in the cargo group ([#297](https://github.com/reubeno/brush/pull/297))
- *(deps)* Bump the cargo group with 2 updates ([#299](https://github.com/reubeno/brush/pull/299))
- *(deps)* Bump the cargo group with 2 updates ([#304](https://github.com/reubeno/brush/pull/304))

<!-- generated by git-cliff -->
## [0.2.13] - 2024-11-26

### 🚀 Features

- *(ast)* Derive `PartialEq` and `Eq` for testing ([#259](https://github.com/reubeno/brush/pull/259))

### 🐛 Bug Fixes

- Correct parsing of parens in arithmetic command ([#270](https://github.com/reubeno/brush/pull/270))

### ⚙️ Miscellaneous Tasks

- Upgrade dependencies ([#271](https://github.com/reubeno/brush/pull/271))

### Build

- *(deps)* Bump the cargo group with 3 updates ([#258](https://github.com/reubeno/brush/pull/258))
- *(deps)* Bump the cargo group with 7 updates ([#267](https://github.com/reubeno/brush/pull/267))

<!-- generated by git-cliff -->
## [0.2.12] - 2024-11-03

### 🚀 Features

- Implement support for ;;& and ;& in case items ([#223](https://github.com/reubeno/brush/pull/223))
- Implement `|&` extension ([#240](https://github.com/reubeno/brush/pull/240))
- Implement `kill -l` ([#221](https://github.com/reubeno/brush/pull/221))
- Implement `|&` for function declarations ([#244](https://github.com/reubeno/brush/pull/244))

### 🐛 Bug Fixes

- Omit dirs from executable searches ([#236](https://github.com/reubeno/brush/pull/236))
- Handle PS2 prompts that require prompt-expansion ([#239](https://github.com/reubeno/brush/pull/239))
- Allow usually-operator chars in regex parens ([#224](https://github.com/reubeno/brush/pull/224))
- Assorted correctness issues in getopts builtin ([#225](https://github.com/reubeno/brush/pull/225))
- Assorted completion-related issues ([#226](https://github.com/reubeno/brush/pull/226))
- String replacement with slashes ([#231](https://github.com/reubeno/brush/pull/231))
- Correct pattern removal expansions on arrays ([#232](https://github.com/reubeno/brush/pull/232))
- *(completion)* Fix -- handling in getopts ([#235](https://github.com/reubeno/brush/pull/235))
- *(completion)* Correct behavior of slice past end of array ([#237](https://github.com/reubeno/brush/pull/237))
- Support here documents in command substitutions ([#255](https://github.com/reubeno/brush/pull/255))

### 🧪 Testing

- Run completion tests using bash-completion 2.14.0 ([#238](https://github.com/reubeno/brush/pull/238))
- Add os-targeted integration tests ([#241](https://github.com/reubeno/brush/pull/241))

### ⚙️ Miscellaneous Tasks

- Upgrade crate dependencies ([#247](https://github.com/reubeno/brush/pull/247))

### Build

- *(deps)* Bump the cargo group with 2 updates ([#220](https://github.com/reubeno/brush/pull/220))

<!-- generated by git-cliff -->
## [0.2.11] - 2024-10-18

### 🚀 Features

- Experimentally enable reedline as an input backend ([#186](https://github.com/reubeno/brush/pull/186))
- Default to reedline and add syntax highlighting support ([#187](https://github.com/reubeno/brush/pull/187))
- Add a panic handler via human-panic ([#191](https://github.com/reubeno/brush/pull/191))
- Several fixes for bash-completion + tests ([#192](https://github.com/reubeno/brush/pull/192))
- Implement `cd -` ([#201](https://github.com/reubeno/brush/pull/201))
- Implement command hashing ([#206](https://github.com/reubeno/brush/pull/206))

### 🐛 Bug Fixes

- Deduplicate completion candidates ([#189](https://github.com/reubeno/brush/pull/189))
- Cleanup transient completion variables ([#213](https://github.com/reubeno/brush/pull/213))
- Allow newlines in extended test exprs ([#188](https://github.com/reubeno/brush/pull/188))
- Fixes for short-circuit precedence + parameter expr replacement ([#193](https://github.com/reubeno/brush/pull/193))
- Workarounds for edge word parsing cases ([#194](https://github.com/reubeno/brush/pull/194))
- Assorted completion issues with ~ and vars ([#199](https://github.com/reubeno/brush/pull/199))
- Slight compat improvements to set -x ([#205](https://github.com/reubeno/brush/pull/205))
- Matching newline chars in glob patterns ([#207](https://github.com/reubeno/brush/pull/207))
- Honor IFS in read builtin ([#208](https://github.com/reubeno/brush/pull/208))
- Correct behavior of break in arithmetic for loop ([#210](https://github.com/reubeno/brush/pull/210))
- Address issues with array unset ([#211](https://github.com/reubeno/brush/pull/211))
- Handle expansion in here documents ([#212](https://github.com/reubeno/brush/pull/212))

### 📚 Documentation

- Update readme ([#182](https://github.com/reubeno/brush/pull/182))
- Update readme with new links ([#204](https://github.com/reubeno/brush/pull/204))

### 🧪 Testing

- Enable setting min oracle version on tests ([#184](https://github.com/reubeno/brush/pull/184))

### ⚙️ Miscellaneous Tasks

- Where possible replace `async-trait` with native async trait support in 1.75+ ([#197](https://github.com/reubeno/brush/pull/197))

### Build

- *(deps)* Bump futures from 0.3.30 to 0.3.31 in the cargo group ([#190](https://github.com/reubeno/brush/pull/190))
- Leave rustyline disabled by default ([#196](https://github.com/reubeno/brush/pull/196))
- *(deps)* Bump the cargo group with 4 updates ([#203](https://github.com/reubeno/brush/pull/203))
- Remove rustyline support ([#216](https://github.com/reubeno/brush/pull/216))

<!-- generated by git-cliff -->
## [0.2.10] - 2024-09-30

### 🐛 Bug Fixes

- Allow source to be used with process substitution ([#175](https://github.com/reubeno/brush/pull/175))
- Address multiple issues with foreground controls for pipeline commands ([#180](https://github.com/reubeno/brush/pull/180))

### 🧪 Testing

- Move to cargo nextest ([#176](https://github.com/reubeno/brush/pull/176))
- Correctly report skipped tests for nextest ([#178](https://github.com/reubeno/brush/pull/178))
- Convert more test skips to known failures ([#179](https://github.com/reubeno/brush/pull/179))

### Build

- *(deps)* Bump the cargo group with 2 updates ([#177](https://github.com/reubeno/brush/pull/177))

<!-- generated by git-cliff -->
## [0.2.9] - 2024-09-26

### 🚀 Features

- Launch processes in their own process groups ([#166](https://github.com/reubeno/brush/pull/166))

### 🐛 Bug Fixes

- Posix compliant argument parsing for `-c` mode ([#147](https://github.com/reubeno/brush/pull/147))

### 🧪 Testing

- Add more basic interactive tests ([#168](https://github.com/reubeno/brush/pull/168))
- Bring up macos testing ([#172](https://github.com/reubeno/brush/pull/172))

### Build

- *(deps)* Bump thiserror from 1.0.63 to 1.0.64 in the cargo group ([#167](https://github.com/reubeno/brush/pull/167))
- Temporarily disable failing test ([#170](https://github.com/reubeno/brush/pull/170))
- Refactor PR workflow to better support multi-platform build + test ([#169](https://github.com/reubeno/brush/pull/169))

<!-- generated by git-cliff -->
## [0.2.8] - 2024-09-17

### 🐛 Bug Fixes

- Implement ~USER syntax ([#160](https://github.com/reubeno/brush/pull/160))
- Compgen needs to expand target arg ([#162](https://github.com/reubeno/brush/pull/162))
- Do not invoke debug traps during completion funcs ([#163](https://github.com/reubeno/brush/pull/163))
- Disable flaky test until it can be root-caused

### 📚 Documentation

- Generate man page via xtask ([#157](https://github.com/reubeno/brush/pull/157))

### ⚡ Performance

- Short-term optimization for common-case printf

### ⚙️ Miscellaneous Tasks

- Extract InteractiveShell as trait + refactor ([#159](https://github.com/reubeno/brush/pull/159))

### Build

- *(deps)* Bump tokio from 1.39.3 to 1.40.0 in the cargo group ([#156](https://github.com/reubeno/brush/pull/156))
- *(deps)* Bump the cargo group with 6 updates ([#158](https://github.com/reubeno/brush/pull/158))
- *(deps)* Bump the cargo group with 2 updates ([#161](https://github.com/reubeno/brush/pull/161))

<!-- generated by git-cliff -->
## [0.2.7] - 2024-09-01

### 🚀 Features

- Move MSRV up to 1.75.0 ([#139](https://github.com/reubeno/brush/pull/139))

### 🐛 Bug Fixes

- Correct echo -e escape expansion for \x sequences ([#143](https://github.com/reubeno/brush/pull/143))
- Disable displaying tracing target ([#140](https://github.com/reubeno/brush/pull/140))
- Correct multiple issues with process substitution + redirection ([#145](https://github.com/reubeno/brush/pull/145))

### Build

- *(deps)* Bump tokio from 1.39.1 to 1.39.2 in the cargo group ([#141](https://github.com/reubeno/brush/pull/141))
- *(deps)* Bump the cargo group with 3 updates ([#148](https://github.com/reubeno/brush/pull/148))
- *(deps)* Bump serde from 1.0.204 to 1.0.206 in the cargo group ([#150](https://github.com/reubeno/brush/pull/150))
- *(deps)* Bump the cargo group with 2 updates ([#152](https://github.com/reubeno/brush/pull/152))
- *(deps)* Bump serde from 1.0.208 to 1.0.209 in the cargo group ([#154](https://github.com/reubeno/brush/pull/154))

<!-- generated by git-cliff -->
## [0.2.6] - 2024-07-23

### 🐛 Bug Fixes

- Correct relative path resolution cases
- Relative path completion fixes ([#137](https://github.com/reubeno/brush/pull/137))

<!-- generated by git-cliff -->
## [0.2.5] - 2024-07-23

### 🐛 Bug Fixes

- Build error outside git ([#134](https://github.com/reubeno/brush/pull/134))

### Build

- *(deps)* Bump the cargo group with 2 updates ([#133](https://github.com/reubeno/brush/pull/133))

<!-- generated by git-cliff -->
## [0.2.4] - 2024-07-19

### 🚀 Features

- Initial support for non-linux
- Enable simpler builtins implemented outside brush ([#130](https://github.com/reubeno/brush/pull/130))
- Get building on windows and wasm-wasip1 targets ([#116](https://github.com/reubeno/brush/pull/116))
- Add brushctl builtin, seed with event toggling support

### 🐛 Bug Fixes

- Absorb breaking change in homedir crate
- Clippy and check warnings ([#123](https://github.com/reubeno/brush/pull/123))
- Correct completion fallback logic when spec matches but 0 results ([#125](https://github.com/reubeno/brush/pull/125))
- Various build warnings on windows build ([#126](https://github.com/reubeno/brush/pull/126))
- Exclude tags from git version info ([#115](https://github.com/reubeno/brush/pull/115))

### 📚 Documentation

- Update readme ([#127](https://github.com/reubeno/brush/pull/127))

### ⚙️ Miscellaneous Tasks

- Merge builtin and builtins modules
- Update comments ([#129](https://github.com/reubeno/brush/pull/129))

### Build

- *(deps)* Bump the cargo group across 1 directory with 5 updates

<!-- generated by git-cliff -->
## [0.2.3] - 2024-07-03

### 🚀 Features

- Enable -O and +O on command line (#105)
- Start using cargo-fuzz for testing (#106)
- Enable fuzz-testing arithmetic eval (#108)
- Include more details in version info (#112)

### 🐛 Bug Fixes

- Correct expansion when PWD is / (#96)
- Ensure parser error actually impls Error (#98)
- Realign newline parsing with spec (#99)
- Correct handling of unterminated expansions (#101)
- Add &>> implementation (#103)
- Correct metadata for fuzz crate (#107)
- Resolve assorted arithmetic eval issues (#110)
- Correct ** overflow behavior (#111)

### ⚙️ Miscellaneous Tasks

- Update Cargo.lock (#113)
- Release

### Build

- Take targeted dependency updates (#93)
- Update config (#97)

## [0.2.2] - 2024-06-19

### 🚀 Features

- Implement 'command' builtin (#77)
- Add stubs for help man page generation
- Fill out read builtin impl
- Rework here doc files (#85)
- Set + validate intentional MSRV (1.72.0) (#86)
- Add basic changelog
- Add basic changelog (#87)

### 🐛 Bug Fixes

- Compgen -W expansion (#78)
- Don't split completions that aren't file paths (#79)
- Allow interrupting read builtin, run pipeline cmds in subshell (#81)
- Add missing flush calls
- Tweak manifests to work with release flow (#89)
- Ensure brush-core builds outside workspace (#90)

### 📚 Documentation

- Add crate shields to readme (#74)
- Add missing code documentation

### ⚙️ Miscellaneous Tasks

- *(release)* Bump version to 0.2.0 (#88)

### Build

- Update dependencies
- Adjust clippy warnings

## [0.1.0] - 2024-06-11

### Build

- Prepare for initial release (#68)
- Enable publishing (#71)

<!-- generated by git-cliff -->
