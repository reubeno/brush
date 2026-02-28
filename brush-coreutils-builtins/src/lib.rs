//! Optional coreutils builtins for brush, powered by uutils/coreutils.
//!
//! Each utility is feature-gated (e.g., `coreutils.cat`, `coreutils.ls`) and
//! can be individually enabled or disabled. The `coreutils.all` feature enables
//! all cross-platform utilities.

use std::collections::HashMap;
use std::ffi::OsString;

use brush_core::builtins::{self, ContentType, ContentOptions};

/// Generates a [`builtins::SimpleCommand`] wrapper around a coreutils `uumain` function.
macro_rules! coreutils_builtin {
    ($struct_name:ident, $util_crate:ident, $desc:literal) => {
        /// Coreutils builtin wrapping the corresponding uutils implementation.
        pub(crate) struct $struct_name;

        impl builtins::SimpleCommand for $struct_name {
            fn get_content(
                _name: &str,
                content_type: ContentType,
                _options: &ContentOptions,
            ) -> Result<String, brush_core::Error> {
                match content_type {
                    ContentType::DetailedHelp | ContentType::ShortDescription => {
                        Ok($desc.into())
                    }
                    ContentType::ShortUsage => Ok(String::new()),
                    ContentType::ManPage => {
                        brush_core::error::unimp("man page not available for coreutils builtin")
                    }
                }
            }

            #[expect(clippy::cast_sign_loss)]
            fn execute<
                SE: brush_core::ShellExtensions,
                I: Iterator<Item = S>,
                S: AsRef<str>,
            >(
                _context: brush_core::ExecutionContext<'_, SE>,
                args: I,
            ) -> Result<brush_core::ExecutionResult, brush_core::Error> {
                let os_args: Vec<OsString> =
                    args.map(|a| OsString::from(a.as_ref())).collect();

                let code = $util_crate::uumain(os_args.into_iter());
                Ok(brush_core::ExecutionResult::new((code & 0xFF) as u8))
            }
        }
    };
}

/// Registers a coreutils builtin in the map if its feature is enabled.
macro_rules! register_coreutils_builtin {
    ($map:expr, $feature:literal, $name:literal, $struct_name:ident) => {
        #[cfg(feature = $feature)]
        $map.insert(
            $name.into(),
            builtins::simple_builtin::<$struct_name, SE>(),
        );
    };
}

// ── Builtin struct declarations ─────────────────────────────────────────────

#[cfg(feature = "coreutils.arch")]
coreutils_builtin!(ArchCommand, uu_arch, "print machine hardware name");

#[cfg(feature = "coreutils.base32")]
coreutils_builtin!(Base32Command, uu_base32, "base32 encode/decode data");

#[cfg(feature = "coreutils.base64")]
coreutils_builtin!(Base64Command, uu_base64, "base64 encode/decode data");

#[cfg(feature = "coreutils.basename")]
coreutils_builtin!(BasenameCommand, uu_basename, "strip directory and suffix from filenames");

#[cfg(feature = "coreutils.basenc")]
coreutils_builtin!(BasencCommand, uu_basenc, "encode/decode data and print to stdout");

#[cfg(feature = "coreutils.cat")]
coreutils_builtin!(CatCommand, uu_cat, "concatenate files and print on the standard output");

#[cfg(feature = "coreutils.cksum")]
coreutils_builtin!(CksumCommand, uu_cksum, "print CRC checksum and byte counts");

#[cfg(feature = "coreutils.b2sum")]
coreutils_builtin!(B2sumCommand, uu_b2sum, "print or check BLAKE2 message digests");

#[cfg(feature = "coreutils.md5sum")]
coreutils_builtin!(Md5sumCommand, uu_md5sum, "print or check MD5 message digests");

#[cfg(feature = "coreutils.sha1sum")]
coreutils_builtin!(Sha1sumCommand, uu_sha1sum, "print or check SHA1 message digests");

#[cfg(feature = "coreutils.sha224sum")]
coreutils_builtin!(Sha224sumCommand, uu_sha224sum, "print or check SHA224 message digests");

#[cfg(feature = "coreutils.sha256sum")]
coreutils_builtin!(Sha256sumCommand, uu_sha256sum, "print or check SHA256 message digests");

#[cfg(feature = "coreutils.sha384sum")]
coreutils_builtin!(Sha384sumCommand, uu_sha384sum, "print or check SHA384 message digests");

#[cfg(feature = "coreutils.sha512sum")]
coreutils_builtin!(Sha512sumCommand, uu_sha512sum, "print or check SHA512 message digests");

#[cfg(feature = "coreutils.comm")]
coreutils_builtin!(CommCommand, uu_comm, "compare two sorted files line by line");

#[cfg(feature = "coreutils.cp")]
coreutils_builtin!(CpCommand, uu_cp, "copy files and directories");

#[cfg(feature = "coreutils.csplit")]
coreutils_builtin!(CsplitCommand, uu_csplit, "split a file into sections");

#[cfg(feature = "coreutils.cut")]
coreutils_builtin!(CutCommand, uu_cut, "remove sections from each line of files");

#[cfg(feature = "coreutils.date")]
coreutils_builtin!(DateCommand, uu_date, "print or set the system date and time");

#[cfg(feature = "coreutils.dd")]
coreutils_builtin!(DdCommand, uu_dd, "convert and copy a file");

#[cfg(feature = "coreutils.df")]
coreutils_builtin!(DfCommand, uu_df, "report file system disk space usage");

#[cfg(feature = "coreutils.dir")]
coreutils_builtin!(DirCommand, uu_dir, "list directory contents");

#[cfg(feature = "coreutils.dircolors")]
coreutils_builtin!(DircolorsCommand, uu_dircolors, "color setup for ls");

#[cfg(feature = "coreutils.dirname")]
coreutils_builtin!(DirnameCommand, uu_dirname, "strip last component from file name");

#[cfg(feature = "coreutils.du")]
coreutils_builtin!(DuCommand, uu_du, "estimate file space usage");

#[cfg(feature = "coreutils.echo")]
coreutils_builtin!(EchoCommand, uu_echo, "display a line of text (coreutils)");

#[cfg(feature = "coreutils.env")]
coreutils_builtin!(EnvCommand, uu_env, "run a program in a modified environment");

#[cfg(feature = "coreutils.expand")]
coreutils_builtin!(ExpandCommand, uu_expand, "convert tabs to spaces");

#[cfg(feature = "coreutils.expr")]
coreutils_builtin!(ExprCommand, uu_expr, "evaluate expressions");

#[cfg(feature = "coreutils.factor")]
coreutils_builtin!(FactorCommand, uu_factor, "factor numbers");

#[cfg(feature = "coreutils.false")]
coreutils_builtin!(FalseCommand, uu_false, "do nothing, unsuccessfully");

#[cfg(feature = "coreutils.fmt")]
coreutils_builtin!(FmtCommand, uu_fmt, "simple optimal text formatter");

#[cfg(feature = "coreutils.fold")]
coreutils_builtin!(FoldCommand, uu_fold, "wrap each input line to fit in specified width");

#[cfg(feature = "coreutils.head")]
coreutils_builtin!(HeadCommand, uu_head, "output the first part of files");

#[cfg(feature = "coreutils.hostname")]
coreutils_builtin!(HostnameCommand, uu_hostname, "print the system hostname");

#[cfg(feature = "coreutils.join")]
coreutils_builtin!(JoinCommand, uu_join, "join lines of two files on a common field");

#[cfg(feature = "coreutils.link")]
coreutils_builtin!(LinkCommand, uu_link, "create a link to a file");

#[cfg(feature = "coreutils.ln")]
coreutils_builtin!(LnCommand, uu_ln, "make links between files");

#[cfg(feature = "coreutils.ls")]
coreutils_builtin!(LsCommand, uu_ls, "list directory contents");

#[cfg(feature = "coreutils.mkdir")]
coreutils_builtin!(MkdirCommand, uu_mkdir, "make directories");

#[cfg(feature = "coreutils.mktemp")]
coreutils_builtin!(MktempCommand, uu_mktemp, "create a temporary file or directory");

#[cfg(feature = "coreutils.more")]
coreutils_builtin!(MoreCommand, uu_more, "file perusal filter for crt viewing");

#[cfg(feature = "coreutils.mv")]
coreutils_builtin!(MvCommand, uu_mv, "move (rename) files");

#[cfg(feature = "coreutils.nl")]
coreutils_builtin!(NlCommand, uu_nl, "number lines of files");

#[cfg(feature = "coreutils.nproc")]
coreutils_builtin!(NprocCommand, uu_nproc, "print the number of processing units");

#[cfg(feature = "coreutils.numfmt")]
coreutils_builtin!(NumfmtCommand, uu_numfmt, "convert numbers from/to human-readable strings");

#[cfg(feature = "coreutils.od")]
coreutils_builtin!(OdCommand, uu_od, "dump files in octal and other formats");

#[cfg(feature = "coreutils.paste")]
coreutils_builtin!(PasteCommand, uu_paste, "merge lines of files");

#[cfg(feature = "coreutils.pr")]
coreutils_builtin!(PrCommand, uu_pr, "paginate or columnate files for printing");

#[cfg(feature = "coreutils.printenv")]
coreutils_builtin!(PrintenvCommand, uu_printenv, "print all or part of environment");

#[cfg(feature = "coreutils.printf")]
coreutils_builtin!(PrintfCommand, uu_printf, "format and print data (coreutils)");

#[cfg(feature = "coreutils.ptx")]
coreutils_builtin!(PtxCommand, uu_ptx, "produce a permuted index of file contents");

#[cfg(feature = "coreutils.pwd")]
coreutils_builtin!(PwdCommand, uu_pwd, "print name of current/working directory (coreutils)");

#[cfg(feature = "coreutils.readlink")]
coreutils_builtin!(ReadlinkCommand, uu_readlink, "print resolved symbolic links or canonical file names");

#[cfg(feature = "coreutils.realpath")]
coreutils_builtin!(RealpathCommand, uu_realpath, "print the resolved path");

#[cfg(feature = "coreutils.rm")]
coreutils_builtin!(RmCommand, uu_rm, "remove files or directories");

#[cfg(feature = "coreutils.rmdir")]
coreutils_builtin!(RmdirCommand, uu_rmdir, "remove empty directories");

#[cfg(feature = "coreutils.seq")]
coreutils_builtin!(SeqCommand, uu_seq, "print a sequence of numbers");

#[cfg(feature = "coreutils.shred")]
coreutils_builtin!(ShredCommand, uu_shred, "overwrite a file to hide its contents");

#[cfg(feature = "coreutils.shuf")]
coreutils_builtin!(ShufCommand, uu_shuf, "generate random permutations");

#[cfg(feature = "coreutils.sleep")]
coreutils_builtin!(SleepCommand, uu_sleep, "delay for a specified amount of time");

#[cfg(feature = "coreutils.sort")]
coreutils_builtin!(SortCommand, uu_sort, "sort lines of text files");

#[cfg(feature = "coreutils.split")]
coreutils_builtin!(SplitCommand, uu_split, "split a file into pieces");

#[cfg(feature = "coreutils.sum")]
coreutils_builtin!(SumCommand, uu_sum, "checksum and count the blocks in a file");

#[cfg(feature = "coreutils.sync")]
coreutils_builtin!(SyncCommand, uu_sync, "synchronize cached writes to persistent storage");

#[cfg(feature = "coreutils.tac")]
coreutils_builtin!(TacCommand, uu_tac, "concatenate and print files in reverse");

#[cfg(feature = "coreutils.tail")]
coreutils_builtin!(TailCommand, uu_tail, "output the last part of files");

#[cfg(feature = "coreutils.tee")]
coreutils_builtin!(TeeCommand, uu_tee, "read from stdin and write to stdout and files");

#[cfg(feature = "coreutils.test")]
coreutils_builtin!(TestCommand, uu_test, "check file types and compare values (coreutils)");

#[cfg(feature = "coreutils.touch")]
coreutils_builtin!(TouchCommand, uu_touch, "change file timestamps");

#[cfg(feature = "coreutils.tr")]
coreutils_builtin!(TrCommand, uu_tr, "translate or delete characters");

#[cfg(feature = "coreutils.true")]
coreutils_builtin!(TrueCommand, uu_true, "do nothing, successfully");

#[cfg(feature = "coreutils.truncate")]
coreutils_builtin!(TruncateCommand, uu_truncate, "shrink or extend the size of a file");

#[cfg(feature = "coreutils.tsort")]
coreutils_builtin!(TsortCommand, uu_tsort, "perform topological sort");

#[cfg(feature = "coreutils.uname")]
coreutils_builtin!(UnameCommand, uu_uname, "print system information");

#[cfg(feature = "coreutils.unexpand")]
coreutils_builtin!(UnexpandCommand, uu_unexpand, "convert spaces to tabs");

#[cfg(feature = "coreutils.uniq")]
coreutils_builtin!(UniqCommand, uu_uniq, "report or omit repeated lines");

#[cfg(feature = "coreutils.unlink")]
coreutils_builtin!(UnlinkCommand, uu_unlink, "remove a file by calling unlink");

#[cfg(feature = "coreutils.vdir")]
coreutils_builtin!(VdirCommand, uu_vdir, "list directory contents (verbose)");

#[cfg(feature = "coreutils.wc")]
coreutils_builtin!(WcCommand, uu_wc, "print newline, word, and byte counts");

#[cfg(feature = "coreutils.whoami")]
coreutils_builtin!(WhoamiCommand, uu_whoami, "print effective userid");

#[cfg(feature = "coreutils.yes")]
coreutils_builtin!(YesCommand, uu_yes, "output a string repeatedly until killed");

// ── Public API ──────────────────────────────────────────────────────────────

/// Returns the set of coreutils built-in commands enabled by feature flags.
#[allow(clippy::too_many_lines)]
pub fn coreutils_builtins<SE: brush_core::extensions::ShellExtensions>()
-> HashMap<String, builtins::Registration<SE>> {
    let mut m = HashMap::<String, builtins::Registration<SE>>::new();

    register_coreutils_builtin!(m, "coreutils.arch", "arch", ArchCommand);
    register_coreutils_builtin!(m, "coreutils.base32", "base32", Base32Command);
    register_coreutils_builtin!(m, "coreutils.base64", "base64", Base64Command);
    register_coreutils_builtin!(m, "coreutils.basename", "basename", BasenameCommand);
    register_coreutils_builtin!(m, "coreutils.basenc", "basenc", BasencCommand);
    register_coreutils_builtin!(m, "coreutils.cat", "cat", CatCommand);
    register_coreutils_builtin!(m, "coreutils.cksum", "cksum", CksumCommand);
    register_coreutils_builtin!(m, "coreutils.b2sum", "b2sum", B2sumCommand);
    register_coreutils_builtin!(m, "coreutils.md5sum", "md5sum", Md5sumCommand);
    register_coreutils_builtin!(m, "coreutils.sha1sum", "sha1sum", Sha1sumCommand);
    register_coreutils_builtin!(m, "coreutils.sha224sum", "sha224sum", Sha224sumCommand);
    register_coreutils_builtin!(m, "coreutils.sha256sum", "sha256sum", Sha256sumCommand);
    register_coreutils_builtin!(m, "coreutils.sha384sum", "sha384sum", Sha384sumCommand);
    register_coreutils_builtin!(m, "coreutils.sha512sum", "sha512sum", Sha512sumCommand);
    register_coreutils_builtin!(m, "coreutils.comm", "comm", CommCommand);
    register_coreutils_builtin!(m, "coreutils.cp", "cp", CpCommand);
    register_coreutils_builtin!(m, "coreutils.csplit", "csplit", CsplitCommand);
    register_coreutils_builtin!(m, "coreutils.cut", "cut", CutCommand);
    register_coreutils_builtin!(m, "coreutils.date", "date", DateCommand);
    register_coreutils_builtin!(m, "coreutils.dd", "dd", DdCommand);
    register_coreutils_builtin!(m, "coreutils.df", "df", DfCommand);
    register_coreutils_builtin!(m, "coreutils.dir", "dir", DirCommand);
    register_coreutils_builtin!(m, "coreutils.dircolors", "dircolors", DircolorsCommand);
    register_coreutils_builtin!(m, "coreutils.dirname", "dirname", DirnameCommand);
    register_coreutils_builtin!(m, "coreutils.du", "du", DuCommand);
    register_coreutils_builtin!(m, "coreutils.echo", "uecho", EchoCommand);
    register_coreutils_builtin!(m, "coreutils.env", "env", EnvCommand);
    register_coreutils_builtin!(m, "coreutils.expand", "expand", ExpandCommand);
    register_coreutils_builtin!(m, "coreutils.expr", "expr", ExprCommand);
    register_coreutils_builtin!(m, "coreutils.factor", "factor", FactorCommand);
    register_coreutils_builtin!(m, "coreutils.false", "ufalse", FalseCommand);
    register_coreutils_builtin!(m, "coreutils.fmt", "fmt", FmtCommand);
    register_coreutils_builtin!(m, "coreutils.fold", "fold", FoldCommand);
    register_coreutils_builtin!(m, "coreutils.head", "head", HeadCommand);
    register_coreutils_builtin!(m, "coreutils.hostname", "hostname", HostnameCommand);
    register_coreutils_builtin!(m, "coreutils.join", "join", JoinCommand);
    register_coreutils_builtin!(m, "coreutils.link", "link", LinkCommand);
    register_coreutils_builtin!(m, "coreutils.ln", "ln", LnCommand);
    register_coreutils_builtin!(m, "coreutils.ls", "ls", LsCommand);
    register_coreutils_builtin!(m, "coreutils.mkdir", "mkdir", MkdirCommand);
    register_coreutils_builtin!(m, "coreutils.mktemp", "mktemp", MktempCommand);
    register_coreutils_builtin!(m, "coreutils.more", "more", MoreCommand);
    register_coreutils_builtin!(m, "coreutils.mv", "mv", MvCommand);
    register_coreutils_builtin!(m, "coreutils.nl", "nl", NlCommand);
    register_coreutils_builtin!(m, "coreutils.nproc", "nproc", NprocCommand);
    register_coreutils_builtin!(m, "coreutils.numfmt", "numfmt", NumfmtCommand);
    register_coreutils_builtin!(m, "coreutils.od", "od", OdCommand);
    register_coreutils_builtin!(m, "coreutils.paste", "paste", PasteCommand);
    register_coreutils_builtin!(m, "coreutils.pr", "pr", PrCommand);
    register_coreutils_builtin!(m, "coreutils.printenv", "printenv", PrintenvCommand);
    register_coreutils_builtin!(m, "coreutils.printf", "uprintf", PrintfCommand);
    register_coreutils_builtin!(m, "coreutils.ptx", "ptx", PtxCommand);
    register_coreutils_builtin!(m, "coreutils.pwd", "upwd", PwdCommand);
    register_coreutils_builtin!(m, "coreutils.readlink", "readlink", ReadlinkCommand);
    register_coreutils_builtin!(m, "coreutils.realpath", "realpath", RealpathCommand);
    register_coreutils_builtin!(m, "coreutils.rm", "rm", RmCommand);
    register_coreutils_builtin!(m, "coreutils.rmdir", "rmdir", RmdirCommand);
    register_coreutils_builtin!(m, "coreutils.seq", "seq", SeqCommand);
    register_coreutils_builtin!(m, "coreutils.shred", "shred", ShredCommand);
    register_coreutils_builtin!(m, "coreutils.shuf", "shuf", ShufCommand);
    register_coreutils_builtin!(m, "coreutils.sleep", "sleep", SleepCommand);
    register_coreutils_builtin!(m, "coreutils.sort", "sort", SortCommand);
    register_coreutils_builtin!(m, "coreutils.split", "split", SplitCommand);
    register_coreutils_builtin!(m, "coreutils.sum", "sum", SumCommand);
    register_coreutils_builtin!(m, "coreutils.sync", "sync", SyncCommand);
    register_coreutils_builtin!(m, "coreutils.tac", "tac", TacCommand);
    register_coreutils_builtin!(m, "coreutils.tail", "tail", TailCommand);
    register_coreutils_builtin!(m, "coreutils.tee", "tee", TeeCommand);
    register_coreutils_builtin!(m, "coreutils.test", "utest", TestCommand);
    register_coreutils_builtin!(m, "coreutils.touch", "touch", TouchCommand);
    register_coreutils_builtin!(m, "coreutils.tr", "tr", TrCommand);
    register_coreutils_builtin!(m, "coreutils.true", "utrue", TrueCommand);
    register_coreutils_builtin!(m, "coreutils.truncate", "truncate", TruncateCommand);
    register_coreutils_builtin!(m, "coreutils.tsort", "tsort", TsortCommand);
    register_coreutils_builtin!(m, "coreutils.uname", "uname", UnameCommand);
    register_coreutils_builtin!(m, "coreutils.unexpand", "unexpand", UnexpandCommand);
    register_coreutils_builtin!(m, "coreutils.uniq", "uniq", UniqCommand);
    register_coreutils_builtin!(m, "coreutils.unlink", "unlink", UnlinkCommand);
    register_coreutils_builtin!(m, "coreutils.vdir", "vdir", VdirCommand);
    register_coreutils_builtin!(m, "coreutils.wc", "wc", WcCommand);
    register_coreutils_builtin!(m, "coreutils.whoami", "whoami", WhoamiCommand);
    register_coreutils_builtin!(m, "coreutils.yes", "yes", YesCommand);

    m
}

/// Extension trait that simplifies adding coreutils builtins to a shell builder.
pub trait ShellBuilderExt {
    /// Add coreutils builtins to the shell being built.
    #[must_use]
    fn coreutils_builtins(self) -> Self;
}

impl<SE: brush_core::extensions::ShellExtensions, S: brush_core::ShellBuilderState> ShellBuilderExt
    for brush_core::ShellBuilder<SE, S>
{
    fn coreutils_builtins(self) -> Self {
        self.builtins(crate::coreutils_builtins())
    }
}
