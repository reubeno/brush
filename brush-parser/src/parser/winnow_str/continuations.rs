//! Line continuations, removed before parsing, as bash removes them while it reads input.
//!
//! A backslash followed by a newline is dropped wherever bash drops it: outside quotes, in
//! double quotes, in backquotes, in `$(...)`, `${...}` and `$((...))`, and in the body of a
//! here-document whose tag is not quoted. It stays where the backslash is taken literally: after
//! an escaping backslash, in single quotes and `$'...'`, in a comment, and in the body of a
//! here-document whose tag is quoted. In an unquoted here-document's body the lines are joined
//! before they are compared with the tag, so `E\` and `OF` end `<<EOF`.
//!
//! A here-document opened in a command substitution that closes before its body, as in
//! `$(cat <<EOF)`, has its body on the lines after the current one. As bash does, the body is
//! read from there, ahead of any other here-document pending by then, and it becomes part of
//! the substitution: it is moved in before the `)`, and the lines it was on are skipped.

#![allow(
    clippy::string_slice,
    reason = "every offset is found by scanning for an ASCII byte, so it is a char boundary"
)]

/// The input as bash reads it: without the line continuations it removes, and with the bodies
/// of here-documents left open in command substitutions moved into them. With it, the anchors
/// that map it back to the input: `(offset in the text, offset in the input)` at each place the
/// two stop running in step. `None` when there is nothing to change.
///
/// # Errors
///
/// When a here-document left open in a command substitution has no body before the end of
/// the input: the input is incomplete, as the tokenizer reports it.
pub(super) fn remove(input: &str) -> Result<Option<(String, Anchors)>, Incomplete> {
    let has_continuation = input.contains("\\\n");
    let has_left_open_body = input.contains("$(") && input.contains("<<");
    if !has_continuation && !has_left_open_body {
        return Ok(None);
    }
    let mut scanner = Scanner {
        src: input.as_bytes(),
        at: 0,
        out: Vec::with_capacity(input.len()),
        anchors: Vec::new(),
        pending: Vec::new(),
        skips: Vec::new(),
        incomplete: false,
    };
    scanner.shell(None);
    if scanner.incomplete {
        return Err(Incomplete);
    }
    if scanner.anchors.is_empty() {
        return Ok(None);
    }
    // Only ASCII bytes were dropped, and whole lines moved, so the text is still UTF-8.
    Ok(String::from_utf8(scanner.out)
        .ok()
        .map(|text| (text, scanner.anchors)))
}

/// `(offset in the text, offset in the input)` at each place the two part.
pub(super) type Anchors = Vec<(usize, usize)>;

/// The input ends before the body of a here-document opened in a command substitution.
#[derive(Debug)]
pub(super) struct Incomplete;

/// A here-document's tag at the start of `text`: how many bytes it takes, the text a body line
/// must equal to end it (quotes removed), and whether any of it was quoted.
///
/// As in bash: in single quotes everything is literal; in double quotes a backslash escapes
/// only `$`, `` ` ``, `"` and `\`; outside quotes it escapes any character. A `$(...)`,
/// `$((...))`, `$[...]` or `${...}` is part of the tag, as written.
pub(super) fn here_tag(text: &str) -> (usize, String, bool) {
    let bytes = text.as_bytes();
    let mut tag = String::new();
    let mut quoted = false;
    let mut i = 0;
    let push = |tag: &mut String, from: usize, to: usize| tag.push_str(&text[from..to]);
    while i < bytes.len() {
        let b = bytes[i];
        match b {
            b' ' | b'\t' | b'\n' | b';' | b'&' | b'|' | b'<' | b'>' | b'(' | b')' => break,
            b'\'' => {
                quoted = true;
                let end = text[i + 1..].find('\'').map_or(bytes.len(), |n| i + 1 + n);
                push(&mut tag, i + 1, end);
                i = (end + 1).min(bytes.len());
            }
            b'"' => {
                quoted = true;
                i += 1;
                while i < bytes.len() && bytes[i] != b'"' {
                    if bytes[i] == b'\\'
                        && matches!(bytes.get(i + 1), Some(b'$' | b'`' | b'"' | b'\\'))
                    {
                        i += 1;
                    }
                    let next = text[i..].chars().next().map_or(1, char::len_utf8);
                    push(&mut tag, i, i + next);
                    i += next;
                }
                i = (i + 1).min(bytes.len());
            }
            b'\\' => {
                quoted = true;
                if i + 1 < bytes.len() {
                    let next = text[i + 1..].chars().next().map_or(1, char::len_utf8);
                    push(&mut tag, i + 1, i + 1 + next);
                    i += 1 + next;
                } else {
                    i += 1;
                }
            }
            b'$' if matches!(bytes.get(i + 1), Some(b'(' | b'[' | b'{')) => {
                let (open, close) = match bytes[i + 1] {
                    b'(' => (b'(', b')'),
                    b'[' => (b'[', b']'),
                    _ => (b'{', b'}'),
                };
                let mut depth = 0usize;
                let mut j = i + 1;
                while j < bytes.len() {
                    if bytes[j] == open {
                        depth += 1;
                    } else if bytes[j] == close {
                        depth -= 1;
                        if depth == 0 {
                            j += 1;
                            break;
                        }
                    }
                    j += 1;
                }
                push(&mut tag, i, j);
                i = j;
            }
            _ => {
                let next = text[i..].chars().next().map_or(1, char::len_utf8);
                push(&mut tag, i, i + next);
                i += next;
            }
        }
    }
    (i, tag, quoted)
}

/// Where the line at the start of `text` ends: the offset of the first newline that is not
/// inside quotes, backquotes or an expansion, or the length of `text`. A here-document's body
/// starts on the line after it: `cat <<A; echo "$(echo x` and `echo y)"` are one line.
pub(super) fn line_end(text: &str) -> usize {
    let bytes = text.as_bytes();
    let mut i = 0;
    let mut word_start = true;
    while i < bytes.len() {
        let b = bytes[i];
        let start = i;
        match b {
            b'\n' => return i,
            b'\\' => i += 2,
            b'\'' => i = text[i + 1..].find('\'').map_or(bytes.len(), |n| i + n + 2),
            b'`' => i = skip_backquotes(bytes, i + 1),
            b'"' => i = skip_double(text, i + 1),
            b'$' => i = skip_dollar(text, i),
            b'#' if word_start => {
                return text[i..].find('\n').map_or(bytes.len(), |n| i + n);
            }
            _ => i += 1,
        }
        word_start = start + 1 == i && (is_blank(b) || is_operator(b));
    }
    bytes.len()
}

/// Past the backquote that closes the text starting at `from`.
const fn skip_backquotes(bytes: &[u8], mut from: usize) -> usize {
    while from < bytes.len() {
        match bytes[from] {
            b'\\' => from += 2,
            b'`' => return from + 1,
            _ => from += 1,
        }
    }
    bytes.len()
}

/// Past the double quote that closes the text starting at `from`.
fn skip_double(text: &str, mut from: usize) -> usize {
    let bytes = text.as_bytes();
    while from < bytes.len() {
        match bytes[from] {
            b'\\' => from += 2,
            b'"' => return from + 1,
            b'`' => from = skip_backquotes(bytes, from + 1),
            b'$' => from = skip_dollar(text, from),
            _ => from += 1,
        }
    }
    bytes.len()
}

/// Past the expansion starting with the `$` at `at`: the tokenizer finds the end of a
/// `$(...)`; brackets are counted for `$((...))`, `$[...]` and `${...}`.
fn skip_dollar(text: &str, at: usize) -> usize {
    let bytes = text.as_bytes();
    let (open, close) = match bytes.get(at + 1) {
        Some(b'(') if bytes.get(at + 2) != Some(&b'(') => {
            return crate::tokenizer::command_substitution_body(
                &text[at + 2..],
                &crate::tokenizer::TokenizerOptions::default(),
            )
            .map_or(bytes.len(), |body| at + 2 + body.len() + 1);
        }
        Some(b'(') => (b'(', b')'),
        Some(b'[') => (b'[', b']'),
        Some(b'{') => (b'{', b'}'),
        _ => return at + 1,
    };
    let mut depth = 0usize;
    let mut i = at + 1;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 1,
            b'\'' if open == b'{' => {
                i = text[i + 1..].find('\'').map_or(bytes.len(), |n| i + n + 1);
            }
            b'"' => {
                i = skip_double(text, i + 1);
                continue;
            }
            b if b == open => depth += 1,
            b if b == close => {
                depth -= 1;
                if depth == 0 {
                    return i + 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    bytes.len()
}

/// A here-document whose operator has been read and whose body has not.
struct PendingHereDoc {
    tag: Vec<u8>,
    strip_tabs: bool,
    quoted: bool,
}

struct Scanner<'a> {
    src: &'a [u8],
    at: usize,
    out: Vec<u8>,
    anchors: Vec<(usize, usize)>,
    pending: Vec<PendingHereDoc>,
    /// Lines of the input already read as bodies of here-documents moved into a command
    /// substitution, as `(start, end)`: skipped when the scanner reaches them.
    skips: Vec<(usize, usize)>,
    /// A here-document moved into a command substitution has no body before the end.
    incomplete: bool,
}

const fn is_blank(b: u8) -> bool {
    b == b' ' || b == b'\t'
}

const fn is_operator(b: u8) -> bool {
    matches!(b, b';' | b'&' | b'|' | b'<' | b'>' | b'(' | b')')
}

impl Scanner<'_> {
    fn peek(&self, ahead: usize) -> Option<u8> {
        self.src.get(self.at + ahead).copied()
    }

    fn copy(&mut self, n: usize) {
        let end = (self.at + n).min(self.src.len());
        self.out.extend_from_slice(&self.src[self.at..end]);
        self.at = end;
        // Lines already read as a moved body are passed over, several in a row too.
        while let Some(&(start, skip_end)) = self.skips.first()
            && self.at == start
        {
            self.skips.remove(0);
            self.jump(skip_end);
        }
    }

    /// Goes on reading the input at `to`, noting where the text and the input part.
    fn jump(&mut self, to: usize) {
        self.at = to;
        self.anchors.push((self.out.len(), to));
    }

    /// At a backslash: drops it with the newline after it, or copies the escaped pair.
    fn backslash(&mut self) {
        if self.peek(1) == Some(b'\n') {
            self.jump(self.at + 2);
        } else {
            self.copy(2);
        }
    }

    /// Text read as the shell's own grammar: the input, or a `$(...)` when `closer` is `)`.
    fn shell(&mut self, closer: Option<u8>) {
        let pending_before = self.pending.len();
        let mut depth = 0usize;
        let mut word_start = true;
        while let Some(b) = self.peek(0) {
            match b {
                b'\\' => self.backslash(),
                b'\'' => self.single(),
                b'"' => {
                    self.copy(1);
                    self.double();
                }
                b'`' => {
                    self.copy(1);
                    self.backtick();
                }
                b'$' => self.dollar(),
                b'#' if word_start => {
                    while let Some(c) = self.peek(0) {
                        if c == b'\n' {
                            break;
                        }
                        self.copy(1);
                    }
                }
                b'<' => {
                    // `<<` as read, so a continuation may split it; `<<<` is a here-string.
                    self.copy(1);
                    if self.out.ends_with(b"<<")
                        && !self.out.ends_with(b"<<<")
                        && self.peek(0) != Some(b'<')
                        && !(self.peek(0) == Some(b'\\')
                            && self.peek(1) == Some(b'\n')
                            && self.peek(2) == Some(b'<'))
                    {
                        self.here_doc_tag();
                    }
                }
                b'(' if word_start && self.peek(1) == Some(b'(') => {
                    // An arithmetic command, where `<<` is a shift.
                    self.copy(2);
                    self.arithmetic(2);
                }
                b'(' => {
                    depth += 1;
                    self.copy(1);
                }
                b')' if closer == Some(b')') && depth == 0 => {
                    if self.pending.len() > pending_before {
                        self.move_bodies_in(pending_before);
                    }
                    self.copy(1);
                    return;
                }
                b')' => {
                    depth = depth.saturating_sub(1);
                    self.copy(1);
                }
                b'\n' => {
                    self.copy(1);
                    if !self.pending.is_empty() {
                        // An unterminated body here is the parser's to report.
                        self.here_doc_bodies();
                    }
                }
                _ => self.copy(1),
            }
            word_start = self
                .out
                .last()
                .is_none_or(|&c| is_blank(c) || c == b'\n' || is_operator(c));
        }
    }

    /// Single-quoted text, copied as it is.
    fn single(&mut self) {
        self.copy(1);
        while let Some(b) = self.peek(0) {
            self.copy(1);
            if b == b'\'' {
                return;
            }
        }
    }

    /// The text after an opening double quote, through the closing one.
    fn double(&mut self) {
        while let Some(b) = self.peek(0) {
            match b {
                b'\\' => self.backslash(),
                b'"' => {
                    self.copy(1);
                    return;
                }
                b'`' => {
                    self.copy(1);
                    self.backtick();
                }
                b'$' => self.dollar(),
                _ => self.copy(1),
            }
        }
    }

    /// The text after an opening backquote, through the closing one. Quotes are not
    /// recognized while it is read, so a continuation goes even between single quotes.
    fn backtick(&mut self) {
        while let Some(b) = self.peek(0) {
            match b {
                b'\\' => self.backslash(),
                b'`' => {
                    self.copy(1);
                    return;
                }
                _ => self.copy(1),
            }
        }
    }

    /// At a `$`: an expansion, `$'...'`, or a plain `$`.
    fn dollar(&mut self) {
        match self.peek(1) {
            Some(b'\'') => {
                // `$'...'`: backslashes escape, and a continuation stays.
                self.copy(2);
                while let Some(b) = self.peek(0) {
                    if b == b'\\' {
                        self.copy(2);
                    } else {
                        self.copy(1);
                        if b == b'\'' {
                            return;
                        }
                    }
                }
            }
            Some(b'(') if self.peek(2) == Some(b'(') => {
                self.copy(3);
                self.arithmetic(2);
            }
            Some(b'(') => {
                self.copy(2);
                self.shell(Some(b')'));
            }
            Some(b'[') => {
                self.copy(2);
                self.brackets(b'[', b']');
            }
            Some(b'{') => {
                self.copy(2);
                self.brackets(b'{', b'}');
            }
            _ => self.copy(1),
        }
    }

    /// The contents of `$((...))` or `((...))` after the opening parens, through the closing
    /// ones: `open` parens are open.
    fn arithmetic(&mut self, open: usize) {
        let mut depth = open;
        while let Some(b) = self.peek(0) {
            match b {
                b'\\' => self.backslash(),
                b'(' => {
                    depth += 1;
                    self.copy(1);
                }
                b')' => {
                    depth -= 1;
                    self.copy(1);
                    if depth == 0 {
                        return;
                    }
                }
                b'$' => self.dollar(),
                b'`' => {
                    self.copy(1);
                    self.backtick();
                }
                b'"' => {
                    self.copy(1);
                    self.double();
                }
                _ => self.copy(1),
            }
        }
    }

    /// The contents of `${...}` or `$[...]` after the opener, through the matching closer.
    fn brackets(&mut self, open: u8, close: u8) {
        let mut depth = 0usize;
        while let Some(b) = self.peek(0) {
            match b {
                b'\\' => self.backslash(),
                b'\'' => self.single(),
                b'"' => {
                    self.copy(1);
                    self.double();
                }
                b'`' => {
                    self.copy(1);
                    self.backtick();
                }
                b'$' => self.dollar(),
                _ if b == open => {
                    depth += 1;
                    self.copy(1);
                }
                _ if b == close => {
                    self.copy(1);
                    if depth == 0 {
                        return;
                    }
                    depth -= 1;
                }
                _ => self.copy(1),
            }
        }
    }

    /// After `<<`: the tag, queued for the body after the line.
    fn here_doc_tag(&mut self) {
        let strip_tabs = self.peek(0) == Some(b'-');
        if strip_tabs {
            self.copy(1);
        }
        // Blanks, and continuations: the tag may be on the next line.
        loop {
            match self.peek(0) {
                Some(b) if is_blank(b) => self.copy(1),
                Some(b'\\') if self.peek(1) == Some(b'\n') => self.backslash(),
                _ => break,
            }
        }
        let rest = std::str::from_utf8(&self.src[self.at..]).unwrap_or_default();
        let (len, tag, quoted) = here_tag(rest);
        let tag = tag.into_bytes();
        self.copy(len);
        if !tag.is_empty() || quoted {
            self.pending.push(PendingHereDoc {
                tag,
                strip_tabs,
                quoted,
            });
        }
    }

    /// At the `)` of a command substitution that closes before the bodies of the
    /// here-documents it opened: reads them from the lines after the current one and puts
    /// them in before the `)`, as bash does. Those lines are skipped when reached.
    fn move_bodies_in(&mut self, from: usize) {
        let close = self.at;
        let line_end = self.src[close..]
            .iter()
            .position(|&b| b == b'\n')
            .map_or(self.src.len(), |n| close + n);
        if line_end >= self.src.len() {
            // No lines after this one: more input may bring the bodies.
            self.incomplete = true;
            return;
        }
        // The newline that ends the line comes first, then the bodies.
        self.anchors.push((self.out.len(), line_end));
        self.out.push(b'\n');
        let outer = self.pending.drain(..from).collect::<Vec<_>>();
        // Past the lines an earlier substitution on this line already took its bodies from.
        let mut body_start = line_end + 1;
        while let Some(&(_, end)) = self
            .skips
            .iter()
            .find(|&&(start, end)| start == body_start && end > start)
        {
            body_start = end;
        }
        self.jump(body_start);
        if !self.here_doc_bodies() {
            self.incomplete = true;
        }
        self.pending = outer;
        let body_end = self.at;
        if body_end > body_start {
            self.skips.push((body_start, body_end));
            self.skips.sort_unstable();
        }
        self.jump(close);
    }

    /// The bodies of the here-documents queued on the line just ended, in order; false when
    /// the input ends before one of them does.
    fn here_doc_bodies(&mut self) -> bool {
        let pending = std::mem::take(&mut self.pending);
        let mut all_ended = true;
        for doc in pending {
            let mut found = false;
            while self.at < self.src.len() {
                let line_start = self.out.len();
                let ended = if doc.quoted {
                    self.raw_line()
                } else {
                    self.joined_line()
                };
                let mut line = &self.out[line_start..];
                if line.last() == Some(&b'\n') {
                    line = &line[..line.len() - 1];
                }
                if doc.strip_tabs {
                    let tabs = line.iter().take_while(|&&c| c == b'\t').count();
                    line = &line[tabs..];
                }
                if line == doc.tag.as_slice() {
                    found = true;
                    break;
                }
                if !ended {
                    break;
                }
            }
            all_ended &= found;
        }
        all_ended
    }

    /// Copies a line as it is, its newline included; false at the end of input.
    fn raw_line(&mut self) -> bool {
        while let Some(b) = self.peek(0) {
            self.copy(1);
            if b == b'\n' {
                return true;
            }
        }
        false
    }

    /// Copies a line with its continuations removed, so it goes on with the next one; its
    /// newline included. False at the end of input.
    fn joined_line(&mut self) -> bool {
        while let Some(b) = self.peek(0) {
            match b {
                b'\\' => self.backslash(),
                b'\n' => {
                    self.copy(1);
                    return true;
                }
                _ => self.copy(1),
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::remove;

    fn cleaned(input: &str) -> String {
        remove(input)
            .unwrap()
            .map_or_else(|| input.to_owned(), |(text, _)| text)
    }

    #[test]
    fn continuations_go_where_bash_drops_them() {
        assert_eq!(cleaned("echo a\\\nb"), "echo ab");
        assert_eq!(cleaned("echo \"x\\\ny\""), "echo \"xy\"");
        assert_eq!(cleaned("echo `echo 'a\\\nb'`"), "echo `echo 'ab'`");
        assert_eq!(cleaned("true &\\\n& echo"), "true && echo");
        assert_eq!(cleaned("echo $\\\n(x) ${\\\ny}"), "echo $(x) ${y}");
    }

    #[test]
    fn continuations_stay_where_the_backslash_is_literal() {
        for input in [
            "echo 'a\\\nb'",
            "echo $'a\\\nb'",
            "# comment \\\necho",
            "echo $(echo 'a\\\nb')",
            "echo a\\\\\nb",
        ] {
            assert_eq!(cleaned(input), input, "{input:?}");
        }
    }

    #[test]
    fn here_document_bodies_follow_their_tag() {
        assert_eq!(cleaned("cat <<E\nab\\\ncd\nE\n"), "cat <<E\nabcd\nE\n");
        assert_eq!(
            cleaned("cat <<'E'\nab\\\ncd\nE\nx\\\ny"),
            "cat <<'E'\nab\\\ncd\nE\nxy"
        );
        // The joined line is what is compared with the tag.
        assert_eq!(cleaned("cat <<EOF\nE\\\nOF\nx\\\ny"), "cat <<EOF\nEOF\nxy");
        assert_eq!(
            cleaned("cat <<-E\n\ta\\\n\tb\n\tE\n"),
            "cat <<-E\n\ta\tb\n\tE\n"
        );
        assert_eq!(cleaned("cat <\\\n<E\nbody\nE\n"), "cat <<E\nbody\nE\n");
    }

    #[test]
    fn a_line_goes_on_over_newlines_in_quotes_and_expansions() {
        use super::line_end;
        assert_eq!(line_end("a b\nc"), 3);
        assert_eq!(line_end("echo \"$(echo x\necho y)\"\na"), 23);
        assert_eq!(line_end("echo \"${x:-p\nq}\"\nb"), 16);
        assert_eq!(line_end("echo $[ 1 +\n2 ]\nc"), 15);
        assert_eq!(line_end("echo 'a\nb' # c \" d\ne"), 18);
    }

    #[test]
    fn a_tag_loses_its_quotes_as_in_bash() {
        use super::here_tag;
        for (text, tag, quoted) in [
            ("EOF rest", "EOF", false),
            ("'a\\b'", "a\\b", true),
            ("\"a\\b\"", "a\\b", true),
            ("\"a\\$b\"", "a$b", true),
            ("\"x'y\"", "x'y", true),
            ("a\\'b", "a'b", true),
            ("'\"x\"'", "\"x\"", true),
            ("$(x) y", "$(x)", false),
            ("$((1 + 2))\n", "$((1 + 2))", false),
            ("$[1];", "$[1]", false),
        ] {
            let (_, got, q) = here_tag(text);
            assert_eq!((got.as_str(), q), (tag, quoted), "{text:?}");
        }
    }

    #[test]
    fn anchors_map_the_text_back_to_the_input() {
        let (text, anchors) = remove("a\\\nb\\\nc").unwrap().unwrap();
        assert_eq!(text, "abc");
        assert_eq!(anchors, [(1, 3), (2, 6)]);
    }

    #[test]
    fn a_body_after_a_command_substitution_moves_into_it() {
        assert_eq!(
            cleaned("echo $(cat <<EOF) more\nbody\nEOF\necho after\n"),
            "echo $(cat <<EOF\nbody\nEOF\n) more\necho after\n"
        );
        assert_eq!(
            cleaned("x=$(echo $(cat <<EOF))\nbody\nEOF\n"),
            "x=$(echo $(cat <<EOF\nbody\nEOF\n))\n"
        );
        // Two on one line take their bodies in turn.
        assert_eq!(
            cleaned("echo $(cat <<C) $(cat <<D)\nc\nC\nd\nD\n"),
            "echo $(cat <<C\nc\nC\n) $(cat <<D\nd\nD\n)\n"
        );
        // Two pending at the end of input: incomplete, and no endless loop.
        assert!(remove("cat <<A; echo \"[$(cat <<B)]\" \"[$(cat <<C)]\"\n").is_err());
        // Without the body yet, the input is incomplete.
        assert!(remove("echo $(cat <<EOF)\n").is_err());
        assert!(remove("echo $(cat <<EOF)\nbody\n").is_err());
        // In an arithmetic command, in a substitution too, `<<` is a shift.
        assert!(
            remove("x=$( (( ((1)) << 2 )); echo ok )\n")
                .unwrap()
                .is_none()
        );
    }
}
