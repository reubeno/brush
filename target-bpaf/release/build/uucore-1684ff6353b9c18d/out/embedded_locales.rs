// Generated at compile time - do not edit
// This file contains embedded English locale files


#[expect(clippy::match_same_arms)]
pub fn get_embedded_locale(key: &str) -> Option<&'static str> {
    match key {
        // Static utility locales for crates.io builds
        // Locale for uucore (en-US)
        "uucore/en-US.ftl" => Some(r"# Common strings shared across all uutils commands
# Mostly clap

# Generic words
common-error = error
common-tip = tip
common-usage = Usage
common-help = help
common-version = version
common-write-error = write error

# Common clap error messages
clap-error-unexpected-argument = { $error_word }: unexpected argument '{ $arg }' found
clap-error-unexpected-argument-simple = unexpected argument
clap-error-similar-argument = { $tip_word }: a similar argument exists: '{ $suggestion }'
clap-error-pass-as-value = { $tip_word }: to pass '{ $arg }' as a value, use '{ $tip_command }'
clap-error-invalid-value = { $error_word }: invalid value '{ $value }' for '{ $option }'
clap-error-value-required = { $error_word }: a value is required for '{ $option }' but none was supplied
clap-error-missing-required-arguments = { $error_word }: the following required arguments were not provided:
clap-error-possible-values = possible values
clap-error-help-suggestion = For more information, try '{ $command } --help'.
common-help-suggestion = For more information, try '--help'.

# Common help text patterns
help-flag-help = Print help information
help-flag-version = Print version information

# Common error contexts
error-io = I/O error
error-permission-denied = Permission denied
error-file-not-found = No such file or directory
error-invalid-argument = Invalid argument
error-is-a-directory = { $file }: Is a directory

# Common actions
action-copying = copying
action-moving = moving
action-removing = removing
action-creating = creating
action-reading = reading
action-writing = writing

# SELinux error messages
selinux-error-not-enabled = SELinux is not enabled on this system
selinux-error-file-open-failure = failed to open the file: { $error }
selinux-error-context-retrieval-failure = failed to retrieve the security context: { $error }
selinux-error-context-set-failure = failed to set default file creation context to '{ $context }': { $error }
selinux-error-context-conversion-failure = failed to set default file creation context to '{ $context }': { $error }
selinux-error-operation-not-supported = operation not supported

# SMACK error messages
smack-error-not-enabled = SMACK is not enabled on this system
smack-error-label-retrieval-failure = failed to get security context: { $error }
smack-error-label-set-failure = failed to set default file creation context to '{ $context }': { $error }
smack-error-no-label-set = no security context set

# Safe traversal error messages
safe-traversal-error-path-contains-null = path contains null byte
safe-traversal-error-open-failed = failed to open { $path }: { $source }
safe-traversal-error-stat-failed = failed to stat { $path }: { $source }
safe-traversal-error-read-dir-failed = failed to read directory { $path }: { $source }
safe-traversal-error-unlink-failed = failed to unlink { $path }: { $source }
safe-traversal-error-invalid-fd = invalid file descriptor
safe-traversal-current-directory = <current directory>
safe-traversal-directory = <directory>

# checksum-related messages
checksum-no-properly-formatted = { $checksum_file }: no properly formatted checksum lines found
checksum-no-file-verified = { $checksum_file }: no file was verified
checksum-error-failed-to-read-input = failed to read input
checksum-bad-format = { $count ->
    [1] { $count } line is improperly formatted
   *[other] { $count } lines are improperly formatted
}
checksum-failed-cksum = { $count ->
    [1] { $count } computed checksum did NOT match
   *[other] { $count } computed checksums did NOT match
}
checksum-failed-open-file = { $count ->
    [1] { $count } listed file could not be read
   *[other] { $count } listed files could not be read
}
checksum-error-algo-bad-format = { $file }: { $line }: improperly formatted { $algo } checksum line

# uudoc tldr examples messages
uudoc-tldr-attribution = The examples are provided by the [tldr-pages project](https://tldr.sh) under the [CC BY 4.0 License](https://github.com/tldr-pages/tldr/blob/main/LICENSE.md).
uudoc-tldr-disclaimer = Please note that, as uutils is a work in progress, some examples might fail.
"),
        // Locale for arch (en-US)
        "arch/en-US.ftl" => Some(r"# Error message when system architecture information cannot be retrieved
cannot-get-system = cannot get system name

arch-about = Display machine architecture
arch-after-help = Determine architecture name for current machine.
arch-usage = arch
"),
        // Locale for arch (en-US)
        "arch/en-US.ftl" => Some(r"# Error message when system architecture information cannot be retrieved
cannot-get-system = cannot get system name

arch-about = Display machine architecture
arch-after-help = Determine architecture name for current machine.
arch-usage = arch
"),
        // Locale for b2sum (en-US)
        "b2sum/en-US.ftl" => Some(r"b2sum-about = Print or check the BLAKE2b checksums
b2sum-usage = b2sum [OPTIONS] [FILE]...
"),
        // Locale for b2sum (en-US)
        "b2sum/en-US.ftl" => Some(r"b2sum-about = Print or check the BLAKE2b checksums
b2sum-usage = b2sum [OPTIONS] [FILE]...
"),
        // Locale for base32 (en-US)
        "base32/en-US.ftl" => Some(r"# This file contains base32, base64 and basenc strings
# This is because we have some common strings for all these tools
# and it is easier to have a single file than one file for program
# and loading several bundles at the same time.

base32-about = encode/decode data and print to standard output
  With no FILE, or when FILE is -, read standard input.

  The data are encoded as described for the base32 alphabet in RFC 4648.
  When decoding, the input may contain newlines in addition
  to the bytes of the formal base32 alphabet. Use --ignore-garbage
  to attempt to recover from any other non-alphabet bytes in the
  encoded stream.
base32-usage = base32 [OPTION]... [FILE]

base64-about = encode/decode data and print to standard output
  With no FILE, or when FILE is -, read standard input.

  The data are encoded as described for the base64 alphabet in RFC 3548.
  When decoding, the input may contain newlines in addition
  to the bytes of the formal base64 alphabet. Use --ignore-garbage
  to attempt to recover from any other non-alphabet bytes in the
  encoded stream.
base64-usage = base64 [OPTION]... [FILE]

basenc-about = Encode/decode data and print to standard output
  With no FILE, or when FILE is -, read standard input.

  When decoding, the input may contain newlines in addition to the bytes of
  the formal alphabet. Use --ignore-garbage to attempt to recover
  from any other non-alphabet bytes in the encoded stream.
basenc-usage = basenc [OPTION]... [FILE]

# Help messages for encoding formats
basenc-help-base64 = same as 'base64' program
basenc-help-base64url = file- and url-safe base64
basenc-help-base32 = same as 'base32' program
basenc-help-base32hex = extended hex alphabet base32
basenc-help-base16 = hex encoding
basenc-help-base2lsbf = bit string with least significant bit (lsb) first
basenc-help-base2msbf = bit string with most significant bit (msb) first
basenc-help-z85 = ascii85-like encoding;
  when encoding, input length must be a multiple of 4;
  when decoding, input length must be a multiple of 5
basenc-help-base58 = visually unambiguous base58 encoding

# Error messages
basenc-error-missing-encoding-type = missing encoding type

# Shared base_common error messages (used by base32, base64, basenc)
base-common-extra-operand = extra operand {$operand}
base-common-no-such-file = {$file}: No such file or directory
base-common-invalid-wrap-size = invalid wrap size: {$size}
base-common-read-error = read error: {$error}

# Shared base_common help messages
base-common-help-decode = decode data
base-common-help-ignore-garbage = when decoding, ignore non-alphabetic characters
base-common-help-wrap = wrap encoded lines after COLS character (default {$default}, 0 to disable wrapping)
"),
        // Locale for base32 (en-US)
        "base32/en-US.ftl" => Some(r"# This file contains base32, base64 and basenc strings
# This is because we have some common strings for all these tools
# and it is easier to have a single file than one file for program
# and loading several bundles at the same time.

base32-about = encode/decode data and print to standard output
  With no FILE, or when FILE is -, read standard input.

  The data are encoded as described for the base32 alphabet in RFC 4648.
  When decoding, the input may contain newlines in addition
  to the bytes of the formal base32 alphabet. Use --ignore-garbage
  to attempt to recover from any other non-alphabet bytes in the
  encoded stream.
base32-usage = base32 [OPTION]... [FILE]

base64-about = encode/decode data and print to standard output
  With no FILE, or when FILE is -, read standard input.

  The data are encoded as described for the base64 alphabet in RFC 3548.
  When decoding, the input may contain newlines in addition
  to the bytes of the formal base64 alphabet. Use --ignore-garbage
  to attempt to recover from any other non-alphabet bytes in the
  encoded stream.
base64-usage = base64 [OPTION]... [FILE]

basenc-about = Encode/decode data and print to standard output
  With no FILE, or when FILE is -, read standard input.

  When decoding, the input may contain newlines in addition to the bytes of
  the formal alphabet. Use --ignore-garbage to attempt to recover
  from any other non-alphabet bytes in the encoded stream.
basenc-usage = basenc [OPTION]... [FILE]

# Help messages for encoding formats
basenc-help-base64 = same as 'base64' program
basenc-help-base64url = file- and url-safe base64
basenc-help-base32 = same as 'base32' program
basenc-help-base32hex = extended hex alphabet base32
basenc-help-base16 = hex encoding
basenc-help-base2lsbf = bit string with least significant bit (lsb) first
basenc-help-base2msbf = bit string with most significant bit (msb) first
basenc-help-z85 = ascii85-like encoding;
  when encoding, input length must be a multiple of 4;
  when decoding, input length must be a multiple of 5
basenc-help-base58 = visually unambiguous base58 encoding

# Error messages
basenc-error-missing-encoding-type = missing encoding type

# Shared base_common error messages (used by base32, base64, basenc)
base-common-extra-operand = extra operand {$operand}
base-common-no-such-file = {$file}: No such file or directory
base-common-invalid-wrap-size = invalid wrap size: {$size}
base-common-read-error = read error: {$error}

# Shared base_common help messages
base-common-help-decode = decode data
base-common-help-ignore-garbage = when decoding, ignore non-alphabetic characters
base-common-help-wrap = wrap encoded lines after COLS character (default {$default}, 0 to disable wrapping)
"),
        // Locale for base64 (en-US)
        "base64/en-US.ftl" => Some(r"# This file contains base32, base64 and basenc strings
# This is because we have some common strings for all these tools
# and it is easier to have a single file than one file for program
# and loading several bundles at the same time.

base32-about = encode/decode data and print to standard output
  With no FILE, or when FILE is -, read standard input.

  The data are encoded as described for the base32 alphabet in RFC 4648.
  When decoding, the input may contain newlines in addition
  to the bytes of the formal base32 alphabet. Use --ignore-garbage
  to attempt to recover from any other non-alphabet bytes in the
  encoded stream.
base32-usage = base32 [OPTION]... [FILE]

base64-about = encode/decode data and print to standard output
  With no FILE, or when FILE is -, read standard input.

  The data are encoded as described for the base64 alphabet in RFC 3548.
  When decoding, the input may contain newlines in addition
  to the bytes of the formal base64 alphabet. Use --ignore-garbage
  to attempt to recover from any other non-alphabet bytes in the
  encoded stream.
base64-usage = base64 [OPTION]... [FILE]

basenc-about = Encode/decode data and print to standard output
  With no FILE, or when FILE is -, read standard input.

  When decoding, the input may contain newlines in addition to the bytes of
  the formal alphabet. Use --ignore-garbage to attempt to recover
  from any other non-alphabet bytes in the encoded stream.
basenc-usage = basenc [OPTION]... [FILE]

# Help messages for encoding formats
basenc-help-base64 = same as 'base64' program
basenc-help-base64url = file- and url-safe base64
basenc-help-base32 = same as 'base32' program
basenc-help-base32hex = extended hex alphabet base32
basenc-help-base16 = hex encoding
basenc-help-base2lsbf = bit string with least significant bit (lsb) first
basenc-help-base2msbf = bit string with most significant bit (msb) first
basenc-help-z85 = ascii85-like encoding;
  when encoding, input length must be a multiple of 4;
  when decoding, input length must be a multiple of 5
basenc-help-base58 = visually unambiguous base58 encoding

# Error messages
basenc-error-missing-encoding-type = missing encoding type

# Shared base_common error messages (used by base32, base64, basenc)
base-common-extra-operand = extra operand {$operand}
base-common-no-such-file = {$file}: No such file or directory
base-common-invalid-wrap-size = invalid wrap size: {$size}
base-common-read-error = read error: {$error}

# Shared base_common help messages
base-common-help-decode = decode data
base-common-help-ignore-garbage = when decoding, ignore non-alphabetic characters
base-common-help-wrap = wrap encoded lines after COLS character (default {$default}, 0 to disable wrapping)
"),
        // Locale for base64 (en-US)
        "base64/en-US.ftl" => Some(r"# This file contains base32, base64 and basenc strings
# This is because we have some common strings for all these tools
# and it is easier to have a single file than one file for program
# and loading several bundles at the same time.

base32-about = encode/decode data and print to standard output
  With no FILE, or when FILE is -, read standard input.

  The data are encoded as described for the base32 alphabet in RFC 4648.
  When decoding, the input may contain newlines in addition
  to the bytes of the formal base32 alphabet. Use --ignore-garbage
  to attempt to recover from any other non-alphabet bytes in the
  encoded stream.
base32-usage = base32 [OPTION]... [FILE]

base64-about = encode/decode data and print to standard output
  With no FILE, or when FILE is -, read standard input.

  The data are encoded as described for the base64 alphabet in RFC 3548.
  When decoding, the input may contain newlines in addition
  to the bytes of the formal base64 alphabet. Use --ignore-garbage
  to attempt to recover from any other non-alphabet bytes in the
  encoded stream.
base64-usage = base64 [OPTION]... [FILE]

basenc-about = Encode/decode data and print to standard output
  With no FILE, or when FILE is -, read standard input.

  When decoding, the input may contain newlines in addition to the bytes of
  the formal alphabet. Use --ignore-garbage to attempt to recover
  from any other non-alphabet bytes in the encoded stream.
basenc-usage = basenc [OPTION]... [FILE]

# Help messages for encoding formats
basenc-help-base64 = same as 'base64' program
basenc-help-base64url = file- and url-safe base64
basenc-help-base32 = same as 'base32' program
basenc-help-base32hex = extended hex alphabet base32
basenc-help-base16 = hex encoding
basenc-help-base2lsbf = bit string with least significant bit (lsb) first
basenc-help-base2msbf = bit string with most significant bit (msb) first
basenc-help-z85 = ascii85-like encoding;
  when encoding, input length must be a multiple of 4;
  when decoding, input length must be a multiple of 5
basenc-help-base58 = visually unambiguous base58 encoding

# Error messages
basenc-error-missing-encoding-type = missing encoding type

# Shared base_common error messages (used by base32, base64, basenc)
base-common-extra-operand = extra operand {$operand}
base-common-no-such-file = {$file}: No such file or directory
base-common-invalid-wrap-size = invalid wrap size: {$size}
base-common-read-error = read error: {$error}

# Shared base_common help messages
base-common-help-decode = decode data
base-common-help-ignore-garbage = when decoding, ignore non-alphabetic characters
base-common-help-wrap = wrap encoded lines after COLS character (default {$default}, 0 to disable wrapping)
"),
        // Locale for basename (en-US)
        "basename/en-US.ftl" => Some(r"basename-about = Print NAME with any leading directory components removed
  If specified, also remove a trailing SUFFIX
basename-usage = basename [-z] NAME [SUFFIX]
  basename OPTION... NAME...

# Error messages
basename-error-missing-operand = missing operand
basename-error-extra-operand = extra operand { $operand }

# Help text for command-line arguments
basename-help-multiple = support multiple arguments and treat each as a NAME
basename-help-suffix = remove a trailing SUFFIX; implies -a
basename-help-zero = end each output line with NUL, not newline
"),
        // Locale for basename (en-US)
        "basename/en-US.ftl" => Some(r"basename-about = Print NAME with any leading directory components removed
  If specified, also remove a trailing SUFFIX
basename-usage = basename [-z] NAME [SUFFIX]
  basename OPTION... NAME...

# Error messages
basename-error-missing-operand = missing operand
basename-error-extra-operand = extra operand { $operand }

# Help text for command-line arguments
basename-help-multiple = support multiple arguments and treat each as a NAME
basename-help-suffix = remove a trailing SUFFIX; implies -a
basename-help-zero = end each output line with NUL, not newline
"),
        // Locale for basenc (en-US)
        "basenc/en-US.ftl" => Some(r"# This file contains base32, base64 and basenc strings
# This is because we have some common strings for all these tools
# and it is easier to have a single file than one file for program
# and loading several bundles at the same time.

base32-about = encode/decode data and print to standard output
  With no FILE, or when FILE is -, read standard input.

  The data are encoded as described for the base32 alphabet in RFC 4648.
  When decoding, the input may contain newlines in addition
  to the bytes of the formal base32 alphabet. Use --ignore-garbage
  to attempt to recover from any other non-alphabet bytes in the
  encoded stream.
base32-usage = base32 [OPTION]... [FILE]

base64-about = encode/decode data and print to standard output
  With no FILE, or when FILE is -, read standard input.

  The data are encoded as described for the base64 alphabet in RFC 3548.
  When decoding, the input may contain newlines in addition
  to the bytes of the formal base64 alphabet. Use --ignore-garbage
  to attempt to recover from any other non-alphabet bytes in the
  encoded stream.
base64-usage = base64 [OPTION]... [FILE]

basenc-about = Encode/decode data and print to standard output
  With no FILE, or when FILE is -, read standard input.

  When decoding, the input may contain newlines in addition to the bytes of
  the formal alphabet. Use --ignore-garbage to attempt to recover
  from any other non-alphabet bytes in the encoded stream.
basenc-usage = basenc [OPTION]... [FILE]

# Help messages for encoding formats
basenc-help-base64 = same as 'base64' program
basenc-help-base64url = file- and url-safe base64
basenc-help-base32 = same as 'base32' program
basenc-help-base32hex = extended hex alphabet base32
basenc-help-base16 = hex encoding
basenc-help-base2lsbf = bit string with least significant bit (lsb) first
basenc-help-base2msbf = bit string with most significant bit (msb) first
basenc-help-z85 = ascii85-like encoding;
  when encoding, input length must be a multiple of 4;
  when decoding, input length must be a multiple of 5
basenc-help-base58 = visually unambiguous base58 encoding

# Error messages
basenc-error-missing-encoding-type = missing encoding type

# Shared base_common error messages (used by base32, base64, basenc)
base-common-extra-operand = extra operand {$operand}
base-common-no-such-file = {$file}: No such file or directory
base-common-invalid-wrap-size = invalid wrap size: {$size}
base-common-read-error = read error: {$error}

# Shared base_common help messages
base-common-help-decode = decode data
base-common-help-ignore-garbage = when decoding, ignore non-alphabetic characters
base-common-help-wrap = wrap encoded lines after COLS character (default {$default}, 0 to disable wrapping)
"),
        // Locale for basenc (en-US)
        "basenc/en-US.ftl" => Some(r"# This file contains base32, base64 and basenc strings
# This is because we have some common strings for all these tools
# and it is easier to have a single file than one file for program
# and loading several bundles at the same time.

base32-about = encode/decode data and print to standard output
  With no FILE, or when FILE is -, read standard input.

  The data are encoded as described for the base32 alphabet in RFC 4648.
  When decoding, the input may contain newlines in addition
  to the bytes of the formal base32 alphabet. Use --ignore-garbage
  to attempt to recover from any other non-alphabet bytes in the
  encoded stream.
base32-usage = base32 [OPTION]... [FILE]

base64-about = encode/decode data and print to standard output
  With no FILE, or when FILE is -, read standard input.

  The data are encoded as described for the base64 alphabet in RFC 3548.
  When decoding, the input may contain newlines in addition
  to the bytes of the formal base64 alphabet. Use --ignore-garbage
  to attempt to recover from any other non-alphabet bytes in the
  encoded stream.
base64-usage = base64 [OPTION]... [FILE]

basenc-about = Encode/decode data and print to standard output
  With no FILE, or when FILE is -, read standard input.

  When decoding, the input may contain newlines in addition to the bytes of
  the formal alphabet. Use --ignore-garbage to attempt to recover
  from any other non-alphabet bytes in the encoded stream.
basenc-usage = basenc [OPTION]... [FILE]

# Help messages for encoding formats
basenc-help-base64 = same as 'base64' program
basenc-help-base64url = file- and url-safe base64
basenc-help-base32 = same as 'base32' program
basenc-help-base32hex = extended hex alphabet base32
basenc-help-base16 = hex encoding
basenc-help-base2lsbf = bit string with least significant bit (lsb) first
basenc-help-base2msbf = bit string with most significant bit (msb) first
basenc-help-z85 = ascii85-like encoding;
  when encoding, input length must be a multiple of 4;
  when decoding, input length must be a multiple of 5
basenc-help-base58 = visually unambiguous base58 encoding

# Error messages
basenc-error-missing-encoding-type = missing encoding type

# Shared base_common error messages (used by base32, base64, basenc)
base-common-extra-operand = extra operand {$operand}
base-common-no-such-file = {$file}: No such file or directory
base-common-invalid-wrap-size = invalid wrap size: {$size}
base-common-read-error = read error: {$error}

# Shared base_common help messages
base-common-help-decode = decode data
base-common-help-ignore-garbage = when decoding, ignore non-alphabetic characters
base-common-help-wrap = wrap encoded lines after COLS character (default {$default}, 0 to disable wrapping)
"),
        // Locale for cat (en-US)
        "cat/en-US.ftl" => Some(r"cat-about = Concatenate FILE(s), or standard input, to standard output
  With no FILE, or when FILE is -, read standard input.
cat-usage = cat [OPTION]... [FILE]...

# Help messages
cat-help-show-all = equivalent to -vET
cat-help-number-nonblank = number nonempty output lines, overrides -n
cat-help-show-nonprinting-ends = equivalent to -vE
cat-help-show-ends = display $ at end of each line
cat-help-number = number all output lines
cat-help-squeeze-blank = suppress repeated empty output lines
cat-help-show-nonprinting-tabs = equivalent to -vT
cat-help-show-tabs = display TAB characters at ^I
cat-help-show-nonprinting = use ^ and M- notation, except for LF (\n) and TAB (\t)
cat-help-ignored-u = (ignored)

# Error messages
cat-error-unknown-filetype = unknown filetype: { $ft_debug }
cat-error-is-directory = Is a directory
cat-error-input-file-is-output-file = input file is output file
cat-error-too-many-symbolic-links = Too many levels of symbolic links
cat-error-no-such-device-or-address = No such device or address
"),
        // Locale for cat (en-US)
        "cat/en-US.ftl" => Some(r"cat-about = Concatenate FILE(s), or standard input, to standard output
  With no FILE, or when FILE is -, read standard input.
cat-usage = cat [OPTION]... [FILE]...

# Help messages
cat-help-show-all = equivalent to -vET
cat-help-number-nonblank = number nonempty output lines, overrides -n
cat-help-show-nonprinting-ends = equivalent to -vE
cat-help-show-ends = display $ at end of each line
cat-help-number = number all output lines
cat-help-squeeze-blank = suppress repeated empty output lines
cat-help-show-nonprinting-tabs = equivalent to -vT
cat-help-show-tabs = display TAB characters at ^I
cat-help-show-nonprinting = use ^ and M- notation, except for LF (\n) and TAB (\t)
cat-help-ignored-u = (ignored)

# Error messages
cat-error-unknown-filetype = unknown filetype: { $ft_debug }
cat-error-is-directory = Is a directory
cat-error-input-file-is-output-file = input file is output file
cat-error-too-many-symbolic-links = Too many levels of symbolic links
cat-error-no-such-device-or-address = No such device or address
"),
        // Locale for checksum_common (en-US)
        "checksum_common/en-US.ftl" => Some(r"ck-common-after-help = With no FILE or when FILE is -, read standard input

# checksum argument help messages
ck-common-help-algorithm = select the digest type to use. See DIGEST below
ck-common-help-untagged = create a reversed style checksum, without digest type
ck-common-help-tag-default = create a BSD style checksum (default)
ck-common-help-tag = create a BSD style checksum
ck-common-help-text = read in text mode (default)
ck-common-help-length = digest length in bits; must not exceed the max size and must be a multiple of 8 for blake2b; must be 224, 256, 384, or 512 for sha2 or sha3
ck-common-help-check = read checksums from the FILEs and check them
ck-common-help-base64 = emit base64-encoded digests, not hexadecimal
ck-common-help-raw = emit a raw binary digest, not hexadecimal
ck-common-help-zero = end each output line with NUL, not newline, and disable file name escaping
ck-common-help-strict = exit non-zero for improperly formatted checksum lines
ck-common-help-warn = warn about improperly formatted checksum lines
ck-common-help-status = don't output anything, status code shows success
ck-common-help-quiet = don't print OK for each successfully verified file
ck-common-help-ignore-missing = don't fail or report status for missing files
ck-common-help-debug = print CPU hardware capability detection info used by cksum
"),
        // Locale for checksum_common (en-US)
        "checksum_common/en-US.ftl" => Some(r"ck-common-after-help = With no FILE or when FILE is -, read standard input

# checksum argument help messages
ck-common-help-algorithm = select the digest type to use. See DIGEST below
ck-common-help-untagged = create a reversed style checksum, without digest type
ck-common-help-tag-default = create a BSD style checksum (default)
ck-common-help-tag = create a BSD style checksum
ck-common-help-text = read in text mode (default)
ck-common-help-length = digest length in bits; must not exceed the max size and must be a multiple of 8 for blake2b; must be 224, 256, 384, or 512 for sha2 or sha3
ck-common-help-check = read checksums from the FILEs and check them
ck-common-help-base64 = emit base64-encoded digests, not hexadecimal
ck-common-help-raw = emit a raw binary digest, not hexadecimal
ck-common-help-zero = end each output line with NUL, not newline, and disable file name escaping
ck-common-help-strict = exit non-zero for improperly formatted checksum lines
ck-common-help-warn = warn about improperly formatted checksum lines
ck-common-help-status = don't output anything, status code shows success
ck-common-help-quiet = don't print OK for each successfully verified file
ck-common-help-ignore-missing = don't fail or report status for missing files
ck-common-help-debug = print CPU hardware capability detection info used by cksum
"),
        // Locale for cksum (en-US)
        "cksum/en-US.ftl" => Some(r#"cksum-about = Print CRC and size for each file
cksum-usage = cksum [OPTIONS] [FILE]...
cksum-after-help = DIGEST determines the digest algorithm and default output format:

  - sysv: (equivalent to sum -s)
  - bsd: (equivalent to sum -r)
  - crc: (equivalent to cksum)
  - crc32b: (only available through cksum)
  - md5: (equivalent to md5sum)
  - sha1: (equivalent to sha1sum)
  - sha2: (equivalent to sha{"{224,256,384,512}"}sum)
  - sha3: (only available through cksum)
  - blake2b: (equivalent to b2sum)
  - sm3: (only available through cksum)
"#),
        // Locale for cksum (en-US)
        "cksum/en-US.ftl" => Some(r#"cksum-about = Print CRC and size for each file
cksum-usage = cksum [OPTIONS] [FILE]...
cksum-after-help = DIGEST determines the digest algorithm and default output format:

  - sysv: (equivalent to sum -s)
  - bsd: (equivalent to sum -r)
  - crc: (equivalent to cksum)
  - crc32b: (only available through cksum)
  - md5: (equivalent to md5sum)
  - sha1: (equivalent to sha1sum)
  - sha2: (equivalent to sha{"{224,256,384,512}"}sum)
  - sha3: (only available through cksum)
  - blake2b: (equivalent to b2sum)
  - sm3: (only available through cksum)
"#),
        // Locale for comm (en-US)
        "comm/en-US.ftl" => Some(r"comm-about = Compare two sorted files line by line.

  When FILE1 or FILE2 (not both) is -, read standard input.

  With no options, produce three-column output. Column one contains
  lines unique to FILE1, column two contains lines unique to FILE2,
  and column three contains lines common to both files.
comm-usage = comm [OPTION]... FILE1 FILE2

# Help messages
comm-help-column-1 = suppress column 1 (lines unique to FILE1)
comm-help-column-2 = suppress column 2 (lines unique to FILE2)
comm-help-column-3 = suppress column 3 (lines that appear in both files)
comm-help-delimiter = separate columns with STR
comm-help-zero-terminated = line delimiter is NUL, not newline
comm-help-total = output a summary
comm-help-check-order = check that the input is correctly sorted, even if all input lines are pairable
comm-help-no-check-order = do not check that the input is correctly sorted

# Error messages
comm-error-file-not-sorted = comm: file { $file_num } is not in sorted order
comm-error-input-not-sorted = comm: input is not in sorted order
comm-error-is-directory = Is a directory
comm-error-multiple-conflicting-delimiters = multiple conflicting output delimiters specified

# Other messages
comm-total = total
"),
        // Locale for comm (en-US)
        "comm/en-US.ftl" => Some(r"comm-about = Compare two sorted files line by line.

  When FILE1 or FILE2 (not both) is -, read standard input.

  With no options, produce three-column output. Column one contains
  lines unique to FILE1, column two contains lines unique to FILE2,
  and column three contains lines common to both files.
comm-usage = comm [OPTION]... FILE1 FILE2

# Help messages
comm-help-column-1 = suppress column 1 (lines unique to FILE1)
comm-help-column-2 = suppress column 2 (lines unique to FILE2)
comm-help-column-3 = suppress column 3 (lines that appear in both files)
comm-help-delimiter = separate columns with STR
comm-help-zero-terminated = line delimiter is NUL, not newline
comm-help-total = output a summary
comm-help-check-order = check that the input is correctly sorted, even if all input lines are pairable
comm-help-no-check-order = do not check that the input is correctly sorted

# Error messages
comm-error-file-not-sorted = comm: file { $file_num } is not in sorted order
comm-error-input-not-sorted = comm: input is not in sorted order
comm-error-is-directory = Is a directory
comm-error-multiple-conflicting-delimiters = multiple conflicting output delimiters specified

# Other messages
comm-total = total
"),
        // Locale for cp (en-US)
        "cp/en-US.ftl" => Some(r"cp-about = Copy SOURCE to DEST, or multiple SOURCE(s) to DIRECTORY.
cp-usage = cp [OPTION]... [-T] SOURCE DEST
  cp [OPTION]... SOURCE... DIRECTORY
  cp [OPTION]... -t DIRECTORY SOURCE...
cp-after-help = Do not copy a non-directory that has an existing destination with the same or newer modification timestamp;
  instead, silently skip the file without failing. If timestamps are being preserved, the comparison is to the
  source timestamp truncated to the resolutions of the destination file system and of the system calls used to
  update timestamps; this avoids duplicate work if several cp -pu commands are executed with the same source
  and destination. This option is ignored if the -n or --no-clobber option is also specified. Also, if
  --preserve=links is also specified (like with cp -au for example), that will take precedence; consequently,
  depending on the order that files are processed from the source, newer files in the destination may be replaced,
  to mirror hard links in the source. which gives more control over which existing files in the destination are
  replaced, and its value can be one of the following:

  - all This is the default operation when an --update option is not specified, and results in all existing files in the destination being replaced.
  - none This is similar to the --no-clobber option, in that no files in the destination are replaced, but also skipping a file does not induce a failure.
  - older This is the default operation when --update is specified, and results in files being replaced if they're older than the corresponding source file.

# Help messages
cp-help-target-directory = copy all SOURCE arguments into target-directory
cp-help-no-target-directory = Treat DEST as a regular file and not a directory
cp-help-interactive = ask before overwriting files
cp-help-link = hard-link files instead of copying
cp-help-no-clobber = don't overwrite a file that already exists
cp-help-recursive = copy directories recursively
cp-help-strip-trailing-slashes = remove any trailing slashes from each SOURCE argument
cp-help-debug = explain how a file is copied. Implies -v
cp-help-verbose = explicitly state what is being done
cp-help-symbolic-link = make symbolic links instead of copying
cp-help-force = if an existing destination file cannot be opened, remove it and try again (this option is ignored when the -n option is also used). Currently not implemented for Windows.
cp-help-remove-destination = remove each existing destination file before attempting to open it (contrast with --force). On Windows, currently only works for writeable files.
cp-help-reflink = control clone/CoW copies. See below
cp-help-attributes-only = Don't copy the file data, just the attributes
cp-help-preserve = Preserve the specified attributes (default: mode, ownership (unix only), timestamps), if possible additional attributes: context, links, xattr, all
cp-help-preserve-default = same as --preserve=mode,ownership(unix only),timestamps
cp-help-no-preserve = don't preserve the specified attributes
cp-help-parents = use full source file name under DIRECTORY
cp-help-no-dereference = never follow symbolic links in SOURCE
cp-help-dereference = always follow symbolic links in SOURCE
cp-help-cli-symbolic-links = follow command-line symbolic links in SOURCE
cp-help-archive = Same as -dR --preserve=all
cp-help-no-dereference-preserve-links = same as --no-dereference --preserve=links
cp-help-one-file-system = stay on this file system
cp-help-sparse = control creation of sparse files. See below
cp-help-selinux = set SELinux security context of destination file to default type
cp-help-context = like -Z, or if CTX is specified then set the SELinux or SMACK security context to CTX
cp-help-progress = Display a progress bar. Note: this feature is not supported by GNU coreutils.
cp-help-copy-contents = NotImplemented: copy contents of special files when recursive

# Error messages
cp-error-missing-file-operand = missing file operand
cp-error-missing-destination-operand = missing destination file operand after { $source }
cp-error-extra-operand = extra operand { $operand }
cp-error-same-file = { $source } and { $dest } are the same file
cp-error-backing-up-destroy-source = backing up { $dest } might destroy source;  { $source } not copied
cp-error-cannot-open-for-reading = cannot open { $source } for reading
cp-error-not-writing-dangling-symlink = not writing through dangling symlink { $dest }
cp-error-failed-to-clone = failed to clone { $source } from { $dest }: { $error }
cp-error-cannot-change-attribute = cannot change attribute { $dest }: Source file is a non regular file
cp-error-cannot-stat = cannot stat { $source }: No such file or directory
cp-error-cannot-create-symlink = cannot create symlink { $dest } to { $source }
cp-error-cannot-create-hard-link = cannot create hard link { $dest } to { $source }
cp-error-omitting-directory = -r not specified; omitting directory { $dir }
cp-error-cannot-copy-directory-into-itself = cannot copy a directory, { $source }, into itself, { $dest }
cp-error-will-not-copy-through-symlink = will not copy { $source } through just-created symlink { $dest }
cp-error-will-not-overwrite-just-created = will not overwrite just-created { $dest } with { $source }
cp-error-target-not-directory = target: { $target } is not a directory
cp-error-cannot-overwrite-directory-with-non-directory = cannot overwrite directory { $dir } with non-directory
cp-error-cannot-overwrite-non-directory-with-directory = cannot overwrite non-directory with directory
cp-error-with-parents-dest-must-be-dir = with --parents, the destination must be a directory
cp-error-not-replacing = not replacing { $file }
cp-error-failed-get-current-dir = failed to get current directory { $error }
cp-error-failed-set-permissions = cannot set permissions { $path }
cp-error-backup-mutually-exclusive = options --backup and --no-clobber are mutually exclusive
cp-error-invalid-argument = invalid argument { $arg } for '{ $option }'
cp-error-option-not-implemented = Option '{ $option }' not yet implemented.
cp-error-not-all-files-copied = Not all files were copied
cp-error-reflink-always-sparse-auto = `--reflink=always` can be used only with --sparse=auto
cp-error-file-exists = { $path }: File exists
cp-error-invalid-backup-argument = --backup is mutually exclusive with -n or --update=none-fail
cp-error-reflink-not-supported = --reflink is only supported on linux and macOS
cp-error-sparse-not-supported = --sparse is only supported on linux
cp-error-not-a-directory = { $path } is not a directory
cp-error-selinux-not-enabled = SELinux was not enabled during the compile time!
cp-error-selinux-set-context = failed to set the security context of { $path }: { $error }
cp-error-selinux-get-context = failed to get security context of { $path }
cp-error-selinux-error = SELinux error: { $error }
cp-error-selinux-context-conflict = cannot combine --context (-Z) with --preserve=context
cp-error-cannot-create-fifo = cannot create fifo { $path }: File exists
cp-error-cannot-create-special-file = cannot create special file { $path }: { $error }
cp-error-cannot-create-regular-file = cannot create regular file { $path }
cp-error-invalid-attribute = invalid attribute { $value }
cp-error-failed-to-create-whole-tree = failed to create whole tree
cp-error-failed-to-create-directory = Failed to create directory: { $error }
cp-error-backup-format = cp: { $error }
  Try '{ $exec } --help' for more information.
cp-error-setting-attributes = setting attributes for { $path }
cp-error-write = write error

# Debug enum strings
cp-debug-enum-no = no
cp-debug-enum-yes = yes
cp-debug-enum-avoided = avoided
cp-debug-enum-unsupported = unsupported
cp-debug-enum-unknown = unknown
cp-debug-enum-zeros = zeros
cp-debug-enum-seek-hole = SEEK_HOLE
cp-debug-enum-seek-hole-zeros = SEEK_HOLE + zeros

# Warning messages
cp-warning-source-specified-more-than-once = source { $file_type } { $source } specified more than once

# Verbose and debug messages
cp-debug-skipped = skipped { $path }
cp-verbose-removed = removed { $path }
cp-debug-copy-offload = copy offload: { $offload }, reflink: { $reflink }, sparse detection: { $sparse }

# Prompts
cp-prompt-overwrite = overwrite { $path }?
cp-prompt-overwrite-with-mode = replace { $path }, overriding mode
"),
        // Locale for cp (en-US)
        "cp/en-US.ftl" => Some(r"cp-about = Copy SOURCE to DEST, or multiple SOURCE(s) to DIRECTORY.
cp-usage = cp [OPTION]... [-T] SOURCE DEST
  cp [OPTION]... SOURCE... DIRECTORY
  cp [OPTION]... -t DIRECTORY SOURCE...
cp-after-help = Do not copy a non-directory that has an existing destination with the same or newer modification timestamp;
  instead, silently skip the file without failing. If timestamps are being preserved, the comparison is to the
  source timestamp truncated to the resolutions of the destination file system and of the system calls used to
  update timestamps; this avoids duplicate work if several cp -pu commands are executed with the same source
  and destination. This option is ignored if the -n or --no-clobber option is also specified. Also, if
  --preserve=links is also specified (like with cp -au for example), that will take precedence; consequently,
  depending on the order that files are processed from the source, newer files in the destination may be replaced,
  to mirror hard links in the source. which gives more control over which existing files in the destination are
  replaced, and its value can be one of the following:

  - all This is the default operation when an --update option is not specified, and results in all existing files in the destination being replaced.
  - none This is similar to the --no-clobber option, in that no files in the destination are replaced, but also skipping a file does not induce a failure.
  - older This is the default operation when --update is specified, and results in files being replaced if they're older than the corresponding source file.

# Help messages
cp-help-target-directory = copy all SOURCE arguments into target-directory
cp-help-no-target-directory = Treat DEST as a regular file and not a directory
cp-help-interactive = ask before overwriting files
cp-help-link = hard-link files instead of copying
cp-help-no-clobber = don't overwrite a file that already exists
cp-help-recursive = copy directories recursively
cp-help-strip-trailing-slashes = remove any trailing slashes from each SOURCE argument
cp-help-debug = explain how a file is copied. Implies -v
cp-help-verbose = explicitly state what is being done
cp-help-symbolic-link = make symbolic links instead of copying
cp-help-force = if an existing destination file cannot be opened, remove it and try again (this option is ignored when the -n option is also used). Currently not implemented for Windows.
cp-help-remove-destination = remove each existing destination file before attempting to open it (contrast with --force). On Windows, currently only works for writeable files.
cp-help-reflink = control clone/CoW copies. See below
cp-help-attributes-only = Don't copy the file data, just the attributes
cp-help-preserve = Preserve the specified attributes (default: mode, ownership (unix only), timestamps), if possible additional attributes: context, links, xattr, all
cp-help-preserve-default = same as --preserve=mode,ownership(unix only),timestamps
cp-help-no-preserve = don't preserve the specified attributes
cp-help-parents = use full source file name under DIRECTORY
cp-help-no-dereference = never follow symbolic links in SOURCE
cp-help-dereference = always follow symbolic links in SOURCE
cp-help-cli-symbolic-links = follow command-line symbolic links in SOURCE
cp-help-archive = Same as -dR --preserve=all
cp-help-no-dereference-preserve-links = same as --no-dereference --preserve=links
cp-help-one-file-system = stay on this file system
cp-help-sparse = control creation of sparse files. See below
cp-help-selinux = set SELinux security context of destination file to default type
cp-help-context = like -Z, or if CTX is specified then set the SELinux or SMACK security context to CTX
cp-help-progress = Display a progress bar. Note: this feature is not supported by GNU coreutils.
cp-help-copy-contents = NotImplemented: copy contents of special files when recursive

# Error messages
cp-error-missing-file-operand = missing file operand
cp-error-missing-destination-operand = missing destination file operand after { $source }
cp-error-extra-operand = extra operand { $operand }
cp-error-same-file = { $source } and { $dest } are the same file
cp-error-backing-up-destroy-source = backing up { $dest } might destroy source;  { $source } not copied
cp-error-cannot-open-for-reading = cannot open { $source } for reading
cp-error-not-writing-dangling-symlink = not writing through dangling symlink { $dest }
cp-error-failed-to-clone = failed to clone { $source } from { $dest }: { $error }
cp-error-cannot-change-attribute = cannot change attribute { $dest }: Source file is a non regular file
cp-error-cannot-stat = cannot stat { $source }: No such file or directory
cp-error-cannot-create-symlink = cannot create symlink { $dest } to { $source }
cp-error-cannot-create-hard-link = cannot create hard link { $dest } to { $source }
cp-error-omitting-directory = -r not specified; omitting directory { $dir }
cp-error-cannot-copy-directory-into-itself = cannot copy a directory, { $source }, into itself, { $dest }
cp-error-will-not-copy-through-symlink = will not copy { $source } through just-created symlink { $dest }
cp-error-will-not-overwrite-just-created = will not overwrite just-created { $dest } with { $source }
cp-error-target-not-directory = target: { $target } is not a directory
cp-error-cannot-overwrite-directory-with-non-directory = cannot overwrite directory { $dir } with non-directory
cp-error-cannot-overwrite-non-directory-with-directory = cannot overwrite non-directory with directory
cp-error-with-parents-dest-must-be-dir = with --parents, the destination must be a directory
cp-error-not-replacing = not replacing { $file }
cp-error-failed-get-current-dir = failed to get current directory { $error }
cp-error-failed-set-permissions = cannot set permissions { $path }
cp-error-backup-mutually-exclusive = options --backup and --no-clobber are mutually exclusive
cp-error-invalid-argument = invalid argument { $arg } for '{ $option }'
cp-error-option-not-implemented = Option '{ $option }' not yet implemented.
cp-error-not-all-files-copied = Not all files were copied
cp-error-reflink-always-sparse-auto = `--reflink=always` can be used only with --sparse=auto
cp-error-file-exists = { $path }: File exists
cp-error-invalid-backup-argument = --backup is mutually exclusive with -n or --update=none-fail
cp-error-reflink-not-supported = --reflink is only supported on linux and macOS
cp-error-sparse-not-supported = --sparse is only supported on linux
cp-error-not-a-directory = { $path } is not a directory
cp-error-selinux-not-enabled = SELinux was not enabled during the compile time!
cp-error-selinux-set-context = failed to set the security context of { $path }: { $error }
cp-error-selinux-get-context = failed to get security context of { $path }
cp-error-selinux-error = SELinux error: { $error }
cp-error-selinux-context-conflict = cannot combine --context (-Z) with --preserve=context
cp-error-cannot-create-fifo = cannot create fifo { $path }: File exists
cp-error-cannot-create-special-file = cannot create special file { $path }: { $error }
cp-error-invalid-attribute = invalid attribute { $value }
cp-error-failed-to-create-whole-tree = failed to create whole tree
cp-error-failed-to-create-directory = Failed to create directory: { $error }
cp-error-backup-format = cp: { $error }
  Try '{ $exec } --help' for more information.
cp-error-setting-attributes = setting attributes for { $path }

# Debug enum strings
cp-debug-enum-no = no
cp-debug-enum-yes = yes
cp-debug-enum-avoided = avoided
cp-debug-enum-unsupported = unsupported
cp-debug-enum-unknown = unknown
cp-debug-enum-zeros = zeros
cp-debug-enum-seek-hole = SEEK_HOLE
cp-debug-enum-seek-hole-zeros = SEEK_HOLE + zeros

# Warning messages
cp-warning-source-specified-more-than-once = source { $file_type } { $source } specified more than once

# Verbose and debug messages
cp-debug-skipped = skipped { $path }
cp-verbose-removed = removed { $path }
cp-debug-copy-offload = copy offload: { $offload }, reflink: { $reflink }, sparse detection: { $sparse }

# Prompts
cp-prompt-overwrite = overwrite { $path }?
cp-prompt-overwrite-with-mode = replace { $path }, overriding mode
"),
        // Locale for csplit (en-US)
        "csplit/en-US.ftl" => Some(r"csplit-about = Split a file into sections determined by context lines
csplit-usage = csplit [OPTION]... FILE PATTERN...
csplit-after-help = Output pieces of FILE separated by PATTERN(s) to files 'xx00', 'xx01', ..., and output byte counts of each piece to standard output.

# Help messages
csplit-help-suffix-format = use sprintf FORMAT instead of %02d
csplit-help-prefix = use PREFIX instead of 'xx'
csplit-help-keep-files = do not remove output files on errors
csplit-help-suppress-matched = suppress the lines matching PATTERN
csplit-help-digits = use specified number of digits instead of 2
csplit-help-quiet = do not print counts of output file sizes
csplit-help-elide-empty-files = remove empty output files

# Error messages
csplit-error-line-out-of-range = { $pattern }: line number out of range
csplit-error-line-out-of-range-on-repetition = { $pattern }: line number out of range on repetition { $repetition }
csplit-error-match-not-found = { $pattern }: match not found
csplit-error-match-not-found-on-repetition = { $pattern }: match not found on repetition { $repetition }
csplit-error-line-number-is-zero = 0: line number must be greater than zero
csplit-error-line-number-smaller-than-previous = line number '{ $current }' is smaller than preceding line number, { $previous }
csplit-error-invalid-pattern = { $pattern }: invalid pattern
csplit-error-invalid-number = invalid number: { $number }
csplit-error-suffix-format-incorrect = incorrect conversion specification in suffix
csplit-error-suffix-format-too-many-percents = too many % conversion specifications in suffix
csplit-error-not-regular-file = { $file } is not a regular file
csplit-warning-line-number-same-as-previous = line number '{ $line_number }' is the same as preceding line number
csplit-stream-not-utf8 = stream did not contain valid UTF-8
csplit-read-error = read error
csplit-write-split-not-created = trying to write to a split that was not created
"),
        // Locale for csplit (en-US)
        "csplit/en-US.ftl" => Some(r"csplit-about = Split a file into sections determined by context lines
csplit-usage = csplit [OPTION]... FILE PATTERN...
csplit-after-help = Output pieces of FILE separated by PATTERN(s) to files 'xx00', 'xx01', ..., and output byte counts of each piece to standard output.

# Help messages
csplit-help-suffix-format = use sprintf FORMAT instead of %02d
csplit-help-prefix = use PREFIX instead of 'xx'
csplit-help-keep-files = do not remove output files on errors
csplit-help-suppress-matched = suppress the lines matching PATTERN
csplit-help-digits = use specified number of digits instead of 2
csplit-help-quiet = do not print counts of output file sizes
csplit-help-elide-empty-files = remove empty output files

# Error messages
csplit-error-line-out-of-range = { $pattern }: line number out of range
csplit-error-line-out-of-range-on-repetition = { $pattern }: line number out of range on repetition { $repetition }
csplit-error-match-not-found = { $pattern }: match not found
csplit-error-match-not-found-on-repetition = { $pattern }: match not found on repetition { $repetition }
csplit-error-line-number-is-zero = 0: line number must be greater than zero
csplit-error-line-number-smaller-than-previous = line number '{ $current }' is smaller than preceding line number, { $previous }
csplit-error-invalid-pattern = { $pattern }: invalid pattern
csplit-error-invalid-number = invalid number: { $number }
csplit-error-suffix-format-incorrect = incorrect conversion specification in suffix
csplit-error-suffix-format-too-many-percents = too many % conversion specifications in suffix
csplit-error-not-regular-file = { $file } is not a regular file
csplit-warning-line-number-same-as-previous = line number '{ $line_number }' is the same as preceding line number
csplit-stream-not-utf8 = stream did not contain valid UTF-8
csplit-read-error = read error
csplit-write-split-not-created = trying to write to a split that was not created
"),
        // Locale for cut (en-US)
        "cut/en-US.ftl" => Some(r"cut-about = Prints specified byte or field columns from each line of stdin or the input files
cut-usage = cut OPTION... [FILE]...
cut-after-help = Each call must specify a mode (what to use for columns),
  a sequence (which columns to print), and provide a data source

  ### Specifying a mode

  Use --bytes (-b) or --characters (-c) to specify byte mode

  Use --fields (-f) to specify field mode, where each line is broken into
  fields identified by a delimiter character. For example for a typical CSV
  you could use this in combination with setting comma as the delimiter

  ### Specifying a sequence

  A sequence is a group of 1 or more numbers or inclusive ranges separated
  by a commas.

  cut -f 2,5-7 some_file.txt

  will display the 2nd, 5th, 6th, and 7th field for each source line

  Ranges can extend to the end of the row by excluding the second number

  cut -f 3- some_file.txt

  will display the 3rd field and all fields after for each source line

  The first number of a range can be excluded, and this is effectively the
  same as using 1 as the first number: it causes the range to begin at the
  first column. Ranges can also display a single column

  cut -f 1,3-5 some_file.txt

  will display the 1st, 3rd, 4th, and 5th field for each source line

  The --complement option, when used, inverts the effect of the sequence

  cut --complement -f 4-6 some_file.txt

  will display the every field but the 4th, 5th, and 6th

  ### Specifying a data source

  If no sourcefile arguments are specified, stdin is used as the source of
  lines to print

  If sourcefile arguments are specified, stdin is ignored and all files are
  read in consecutively if a sourcefile is not successfully read, a warning
  will print to stderr, and the eventual status code will be 1, but cut
  will continue to read through proceeding sourcefiles

  To print columns from both STDIN and a file argument, use - (dash) as a
  sourcefile argument to represent stdin.

  ### Field Mode options

  The fields in each line are identified by a delimiter (separator)

  #### Set the delimiter

  Set the delimiter which separates fields in the file using the
  --delimiter (-d) option. Setting the delimiter is optional.
  If not set, a default delimiter of Tab will be used.

  If the -w option is provided, fields will be separated by any number
  of whitespace characters (Space and Tab). The output delimiter will
  be a Tab unless explicitly specified. Only one of -d or -w option can be specified.
  This is an extension adopted from FreeBSD.

  #### Optionally Filter based on delimiter

  If the --only-delimited (-s) flag is provided, only lines which
  contain the delimiter will be printed

  #### Replace the delimiter

  If the --output-delimiter option is provided, the argument used for
  it will replace the delimiter character in each line printed. This is
  useful for transforming tabular data - e.g. to convert a CSV to a
  TSV (tab-separated file)

  ### Line endings

  When the --zero-terminated (-z) option is used, cut sees \\0 (null) as the
  'line ending' character (both for the purposes of reading lines and
  separating printed lines) instead of \\n (newline). This is useful for
  tabular data where some of the cells may contain newlines

  echo 'ab\\0cd' | cut -z -c 1

  will result in 'a\\0c\\0'

# Help messages
cut-help-bytes = filter byte columns from the input source
cut-help-characters = alias for character mode
cut-help-delimiter = specify the delimiter character that separates fields in the input source. Defaults to Tab.
cut-help-whitespace-delimited = Use any number of whitespace (Space, Tab) to separate fields in the input source (FreeBSD extension).
cut-help-fields = filter field columns from the input source
cut-help-complement = invert the filter - instead of displaying only the filtered columns, display all but those columns
cut-help-only-delimited = in field mode, only print lines which contain the delimiter
cut-help-zero-terminated = instead of filtering columns based on line, filter columns based on \\0 (NULL character)
cut-help-output-delimiter = in field mode, replace the delimiter in output lines with this option's argument
cut-help-no-partial = with -b, don't output partial multi-byte characters

# Error messages
cut-error-is-directory = Is a directory
cut-error-write-error = write error
cut-error-delimiter-and-whitespace-conflict = invalid input: Only one of --delimiter (-d) or -w option can be specified
cut-error-delimiter-must-be-single-character = the delimiter must be a single character
cut-error-multiple-mode-args = invalid usage: expects no more than one of --fields (-f), --chars (-c) or --bytes (-b)
cut-error-missing-mode-arg = invalid usage: expects one of --fields (-f), --chars (-c) or --bytes (-b)
cut-error-delimiter-only-with-fields = invalid input: The '--delimiter' ('-d') option can only be used when printing a sequence of fields
cut-error-whitespace-only-with-fields = invalid input: The '-w' option can only be used when printing a sequence of fields
cut-error-only-delimited-only-with-fields = invalid input: The '--only-delimited' ('-s') option can only be used when printing a sequence of fields
"),
        // Locale for cut (en-US)
        "cut/en-US.ftl" => Some(r"cut-about = Prints specified byte or field columns from each line of stdin or the input files
cut-usage = cut OPTION... [FILE]...
cut-after-help = Each call must specify a mode (what to use for columns),
  a sequence (which columns to print), and provide a data source

  ### Specifying a mode

  Use --bytes (-b) or --characters (-c) to specify byte mode

  Use --fields (-f) to specify field mode, where each line is broken into
  fields identified by a delimiter character. For example for a typical CSV
  you could use this in combination with setting comma as the delimiter

  ### Specifying a sequence

  A sequence is a group of 1 or more numbers or inclusive ranges separated
  by a commas.

  cut -f 2,5-7 some_file.txt

  will display the 2nd, 5th, 6th, and 7th field for each source line

  Ranges can extend to the end of the row by excluding the second number

  cut -f 3- some_file.txt

  will display the 3rd field and all fields after for each source line

  The first number of a range can be excluded, and this is effectively the
  same as using 1 as the first number: it causes the range to begin at the
  first column. Ranges can also display a single column

  cut -f 1,3-5 some_file.txt

  will display the 1st, 3rd, 4th, and 5th field for each source line

  The --complement option, when used, inverts the effect of the sequence

  cut --complement -f 4-6 some_file.txt

  will display the every field but the 4th, 5th, and 6th

  ### Specifying a data source

  If no sourcefile arguments are specified, stdin is used as the source of
  lines to print

  If sourcefile arguments are specified, stdin is ignored and all files are
  read in consecutively if a sourcefile is not successfully read, a warning
  will print to stderr, and the eventual status code will be 1, but cut
  will continue to read through proceeding sourcefiles

  To print columns from both STDIN and a file argument, use - (dash) as a
  sourcefile argument to represent stdin.

  ### Field Mode options

  The fields in each line are identified by a delimiter (separator)

  #### Set the delimiter

  Set the delimiter which separates fields in the file using the
  --delimiter (-d) option. Setting the delimiter is optional.
  If not set, a default delimiter of Tab will be used.

  If the -w option is provided, fields will be separated by any number
  of whitespace characters (Space and Tab). The output delimiter will
  be a Tab unless explicitly specified. Only one of -d or -w option can be specified.
  This is an extension adopted from FreeBSD.

  #### Optionally Filter based on delimiter

  If the --only-delimited (-s) flag is provided, only lines which
  contain the delimiter will be printed

  #### Replace the delimiter

  If the --output-delimiter option is provided, the argument used for
  it will replace the delimiter character in each line printed. This is
  useful for transforming tabular data - e.g. to convert a CSV to a
  TSV (tab-separated file)

  ### Line endings

  When the --zero-terminated (-z) option is used, cut sees \\0 (null) as the
  'line ending' character (both for the purposes of reading lines and
  separating printed lines) instead of \\n (newline). This is useful for
  tabular data where some of the cells may contain newlines

  echo 'ab\\0cd' | cut -z -c 1

  will result in 'a\\0c\\0'

# Help messages
cut-help-bytes = filter byte columns from the input source
cut-help-characters = alias for character mode
cut-help-delimiter = specify the delimiter character that separates fields in the input source. Defaults to Tab.
cut-help-whitespace-delimited = Use any number of whitespace (Space, Tab) to separate fields in the input source (FreeBSD extension).
cut-help-fields = filter field columns from the input source
cut-help-complement = invert the filter - instead of displaying only the filtered columns, display all but those columns
cut-help-only-delimited = in field mode, only print lines which contain the delimiter
cut-help-zero-terminated = instead of filtering columns based on line, filter columns based on \\0 (NULL character)
cut-help-output-delimiter = in field mode, replace the delimiter in output lines with this option's argument

# Error messages
cut-error-is-directory = Is a directory
cut-error-write-error = write error
cut-error-delimiter-and-whitespace-conflict = invalid input: Only one of --delimiter (-d) or -w option can be specified
cut-error-delimiter-must-be-single-character = the delimiter must be a single character
cut-error-multiple-mode-args = invalid usage: expects no more than one of --fields (-f), --chars (-c) or --bytes (-b)
cut-error-missing-mode-arg = invalid usage: expects one of --fields (-f), --chars (-c) or --bytes (-b)
cut-error-delimiter-only-with-fields = invalid input: The '--delimiter' ('-d') option can only be used when printing a sequence of fields
cut-error-whitespace-only-with-fields = invalid input: The '-w' option can only be used when printing a sequence of fields
cut-error-only-delimited-only-with-fields = invalid input: The '--only-delimited' ('-s') option can only be used when printing a sequence of fields
"),
        // Locale for date (en-US)
        "date/en-US.ftl" => Some(r#"date-about =
    Print or set the system date and time

date-usage =
    date [OPTION]... [+FORMAT]...
    date [OPTION]... [MMDDhhmm[[CC]YY][.ss]]

    FORMAT controls the output.  Interpreted sequences are:
    { "| Sequence | Description                                                          | Example                |" }
    { "| -------- | -------------------------------------------------------------------- | ---------------------- |" }
    { "| %%       | a literal %                                                          | %                      |" }
    { "| %a       | locale's abbreviated weekday name                                    | Sun                    |" }
    { "| %A       | locale's full weekday name                                           | Sunday                 |" }
    { "| %b       | locale's abbreviated month name                                      | Jan                    |" }
    { "| %B       | locale's full month name                                             | January                |" }
    { "| %c       | locale's date and time                                               | Thu Mar 3 23:05:25 2005|" }
    { "| %C       | century; like %Y, except omit last two digits                        | 20                     |" }
    { "| %d       | day of month                                                         | 01                     |" }
    { "| %D       | date; same as %m/%d/%y                                               | 12/31/99               |" }
    { "| %e       | day of month, space padded; same as %_d                              | 3                      |" }
    { "| %F       | full date; same as %Y-%m-%d                                          | 2005-03-03             |" }
    { "| %g       | last two digits of year of ISO week number (see %G)                  | 05                     |" }
    { "| %G       | year of ISO week number (see %V); normally useful only with %V       | 2005                   |" }
    { "| %h       | same as %b                                                           | Jan                    |" }
    { "| %H       | hour (00..23)                                                        | 23                     |" }
    { "| %I       | hour (01..12)                                                        | 11                     |" }
    { "| %j       | day of year (001..366)                                               | 062                    |" }
    { "| %k       | hour, space padded ( 0..23); same as %_H                             |  3                     |" }
    { "| %l       | hour, space padded ( 1..12); same as %_I                             |  9                     |" }
    { "| %m       | month (01..12)                                                       | 03                     |" }
    { "| %M       | minute (00..59)                                                      | 30                     |" }
    { "| %n       | a newline                                                            | \\n                     |" }
    { "| %N       | nanoseconds (000000000..999999999)                                   | 123456789              |" }
    { "| %p       | locale's equivalent of either AM or PM; blank if not known           | PM                     |" }
    { "| %P       | like %p, but lower case                                              | pm                     |" }
    { "| %q       | quarter of year (1..4)                                               | 1                      |" }
    { "| %r       | locale's 12-hour clock time                                          | 11:11:04 PM            |" }
    { "| %R       | 24-hour hour and minute; same as %H:%M                               | 23:30                  |" }
    { "| %s       | seconds since 1970-01-01 00:00:00 UTC                                | 1615432800             |" }
    { "| %S       | second (00..60)                                                      | 30                     |" }
    { "| %t       | a tab                                                                | \\t                     |" }
    { "| %T       | time; same as %H:%M:%S                                               | 23:30:30               |" }
    { "| %u       | day of week (1..7); 1 is Monday                                      | 4                      |" }
    { "| %U       | week number of year, with Sunday as first day of week (00..53)       | 10                     |" }
    { "| %V       | ISO week number, with Monday as first day of week (01..53)           | 12                     |" }
    { "| %w       | day of week (0..6); 0 is Sunday                                      | 4                      |" }
    { "| %W       | week number of year, with Monday as first day of week (00..53)       | 11                     |" }
    { "| %x       | locale's date representation                                         | 03/03/2005             |" }
    { "| %X       | locale's time representation                                         | 23:30:30               |" }
    { "| %y       | last two digits of year (00..99)                                     | 05                     |" }
    { "| %Y       | year                                                                 | 2005                   |" }
    { "| %z       | +hhmm numeric time zone                                              | -0400                  |" }
    { "| %:z      | +hh:mm numeric time zone                                             | -04:00                 |" }
    { "| %::z     | +hh:mm:ss numeric time zone                                          | -04:00:00              |" }
    { "| %:::z    | numeric time zone with : to necessary precision                      | -04, +05:30            |" }
    { "| %Z       | alphabetic time zone abbreviation                                    | EDT                    |" }

    By default, date pads numeric fields with zeroes.
    The following optional flags may follow '%':
      { "* `-` (hyphen) do not pad the field" }
      { "* `_` (underscore) pad with spaces" }
      { "* `0` (zero) pad with zeros" }
      { "* `^` use upper case if possible" }
      { "* `#` use opposite case if possible" }
    After any flags comes an optional field width, as a decimal number;
    then an optional modifier, which is either
      { "* `E` to use the locale's alternate representations if available, or" }
      { "* `O` to use the locale's alternate numeric symbols if available." }
    Examples:
      Convert seconds since the epoch (1970-01-01 UTC) to a date

      date --date='@2147483647'

      Show the time on the west coast of the US (use tzselect(1) to find TZ)

      TZ='America/Los_Angeles' date

date-help-date = display time described by STRING, not 'now'
date-help-file = like --date; once for each line of DATEFILE
date-help-iso-8601 = output date/time in ISO 8601 format.
  FMT='date' for date only (the default),
  'hours', 'minutes', 'seconds', or 'ns'
  for date and time to the indicated precision.
  Example: 2006-08-14T02:34:56-06:00
date-help-resolution = output the available resolution of timestamps
  Example: 0.000000001
date-help-rfc-email = output date and time in RFC 5322 format.
  Example: Mon, 14 Aug 2006 02:34:56 -0600
date-help-rfc-3339 = output date/time in RFC 3339 format.
  FMT='date', 'seconds', or 'ns'
  for date and time to the indicated precision.
  Example: 2006-08-14 02:34:56-06:00
date-help-debug = annotate the parsed date, and warn about questionable usage to stderr
date-help-reference = display the last modification time of FILE
date-help-set = set time described by STRING
date-help-set-macos = set time described by STRING (not available on mac yet)
date-help-set-redox = set time described by STRING (not available on redox yet)
date-help-universal = print or set Coordinated Universal Time (UTC)

date-error-invalid-date = invalid date '{$date}'
date-error-invalid-format = invalid format '{$format}' ({$error})
date-error-expected-file-got-directory = expected file, got directory {$path}
date-error-date-overflow = date overflow '{$date}'
date-error-setting-date-not-supported-macos = setting the date is not supported by macOS
date-error-setting-date-not-supported-redox = setting the date is not supported by Redox
date-error-cannot-set-date = cannot set date
date-error-extra-operand = extra operand '{$operand}'
date-error-write = write error: {$error}
date-error-format-modifier-width-too-large = format modifier width '{$width}' is too large for specifier '%{$specifier}'
date-error-format-missing-plus = the argument {$arg} lacks a leading '+';
  when using an option to specify date(s), any non-option
  argument must be a format string beginning with '+'
"#),
        // Locale for date (en-US)
        "date/en-US.ftl" => Some(r#"date-about =
    Print or set the system date and time

date-usage =
    date [OPTION]... [+FORMAT]...
    date [OPTION]... [MMDDhhmm[[CC]YY][.ss]]

    FORMAT controls the output.  Interpreted sequences are:
    { "| Sequence | Description                                                          | Example                |" }
    { "| -------- | -------------------------------------------------------------------- | ---------------------- |" }
    { "| %%       | a literal %                                                          | %                      |" }
    { "| %a       | locale's abbreviated weekday name                                    | Sun                    |" }
    { "| %A       | locale's full weekday name                                           | Sunday                 |" }
    { "| %b       | locale's abbreviated month name                                      | Jan                    |" }
    { "| %B       | locale's full month name                                             | January                |" }
    { "| %c       | locale's date and time                                               | Thu Mar 3 23:05:25 2005|" }
    { "| %C       | century; like %Y, except omit last two digits                        | 20                     |" }
    { "| %d       | day of month                                                         | 01                     |" }
    { "| %D       | date; same as %m/%d/%y                                               | 12/31/99               |" }
    { "| %e       | day of month, space padded; same as %_d                              | 3                      |" }
    { "| %F       | full date; same as %Y-%m-%d                                          | 2005-03-03             |" }
    { "| %g       | last two digits of year of ISO week number (see %G)                  | 05                     |" }
    { "| %G       | year of ISO week number (see %V); normally useful only with %V       | 2005                   |" }
    { "| %h       | same as %b                                                           | Jan                    |" }
    { "| %H       | hour (00..23)                                                        | 23                     |" }
    { "| %I       | hour (01..12)                                                        | 11                     |" }
    { "| %j       | day of year (001..366)                                               | 062                    |" }
    { "| %k       | hour, space padded ( 0..23); same as %_H                             |  3                     |" }
    { "| %l       | hour, space padded ( 1..12); same as %_I                             |  9                     |" }
    { "| %m       | month (01..12)                                                       | 03                     |" }
    { "| %M       | minute (00..59)                                                      | 30                     |" }
    { "| %n       | a newline                                                            | \\n                     |" }
    { "| %N       | nanoseconds (000000000..999999999)                                   | 123456789              |" }
    { "| %p       | locale's equivalent of either AM or PM; blank if not known           | PM                     |" }
    { "| %P       | like %p, but lower case                                              | pm                     |" }
    { "| %q       | quarter of year (1..4)                                               | 1                      |" }
    { "| %r       | locale's 12-hour clock time                                          | 11:11:04 PM            |" }
    { "| %R       | 24-hour hour and minute; same as %H:%M                               | 23:30                  |" }
    { "| %s       | seconds since 1970-01-01 00:00:00 UTC                                | 1615432800             |" }
    { "| %S       | second (00..60)                                                      | 30                     |" }
    { "| %t       | a tab                                                                | \\t                     |" }
    { "| %T       | time; same as %H:%M:%S                                               | 23:30:30               |" }
    { "| %u       | day of week (1..7); 1 is Monday                                      | 4                      |" }
    { "| %U       | week number of year, with Sunday as first day of week (00..53)       | 10                     |" }
    { "| %V       | ISO week number, with Monday as first day of week (01..53)           | 12                     |" }
    { "| %w       | day of week (0..6); 0 is Sunday                                      | 4                      |" }
    { "| %W       | week number of year, with Monday as first day of week (00..53)       | 11                     |" }
    { "| %x       | locale's date representation                                         | 03/03/2005             |" }
    { "| %X       | locale's time representation                                         | 23:30:30               |" }
    { "| %y       | last two digits of year (00..99)                                     | 05                     |" }
    { "| %Y       | year                                                                 | 2005                   |" }
    { "| %z       | +hhmm numeric time zone                                              | -0400                  |" }
    { "| %:z      | +hh:mm numeric time zone                                             | -04:00                 |" }
    { "| %::z     | +hh:mm:ss numeric time zone                                          | -04:00:00              |" }
    { "| %:::z    | numeric time zone with : to necessary precision                      | -04, +05:30            |" }
    { "| %Z       | alphabetic time zone abbreviation                                    | EDT                    |" }

    By default, date pads numeric fields with zeroes.
    The following optional flags may follow '%':
      { "* `-` (hyphen) do not pad the field" }
      { "* `_` (underscore) pad with spaces" }
      { "* `0` (zero) pad with zeros" }
      { "* `^` use upper case if possible" }
      { "* `#` use opposite case if possible" }
    After any flags comes an optional field width, as a decimal number;
    then an optional modifier, which is either
      { "* `E` to use the locale's alternate representations if available, or" }
      { "* `O` to use the locale's alternate numeric symbols if available." }
    Examples:
      Convert seconds since the epoch (1970-01-01 UTC) to a date

      date --date='@2147483647'

      Show the time on the west coast of the US (use tzselect(1) to find TZ)

      TZ='America/Los_Angeles' date

date-help-date = display time described by STRING, not 'now'
date-help-file = like --date; once for each line of DATEFILE
date-help-iso-8601 = output date/time in ISO 8601 format.
  FMT='date' for date only (the default),
  'hours', 'minutes', 'seconds', or 'ns'
  for date and time to the indicated precision.
  Example: 2006-08-14T02:34:56-06:00
date-help-resolution = output the available resolution of timestamps
  Example: 0.000000001
date-help-rfc-email = output date and time in RFC 5322 format.
  Example: Mon, 14 Aug 2006 02:34:56 -0600
date-help-rfc-3339 = output date/time in RFC 3339 format.
  FMT='date', 'seconds', or 'ns'
  for date and time to the indicated precision.
  Example: 2006-08-14 02:34:56-06:00
date-help-debug = annotate the parsed date, and warn about questionable usage to stderr
date-help-reference = display the last modification time of FILE
date-help-set = set time described by STRING
date-help-set-macos = set time described by STRING (not available on mac yet)
date-help-set-redox = set time described by STRING (not available on redox yet)
date-help-universal = print or set Coordinated Universal Time (UTC)

date-error-invalid-date = invalid date '{$date}'
date-error-invalid-format = invalid format '{$format}' ({$error})
date-error-expected-file-got-directory = expected file, got directory {$path}
date-error-date-overflow = date overflow '{$date}'
date-error-setting-date-not-supported-macos = setting the date is not supported by macOS
date-error-setting-date-not-supported-redox = setting the date is not supported by Redox
date-error-cannot-set-date = cannot set date
date-error-extra-operand = extra operand '{$operand}'
date-error-write = write error: {$error}
date-error-format-modifier-width-too-large = format modifier width '{$width}' is too large for specifier '%{$specifier}'
date-error-format-missing-plus = the argument {$arg} lacks a leading '+';
  when using an option to specify date(s), any non-option
  argument must be a format string beginning with '+'
"#),
        // Locale for dd (en-US)
        "dd/en-US.ftl" => Some(r"dd-about = Copy, and optionally convert, a file system resource
dd-usage = dd [OPERAND]...
  dd OPTION
dd-after-help = ### Operands

  - bs=BYTES : read and write up to BYTES bytes at a time (default: 512);
     overwrites ibs and obs.
  - cbs=BYTES : the 'conversion block size' in bytes. Applies to the
     conv=block, and conv=unblock operations.
  - conv=CONVS : a comma-separated list of conversion options or (for legacy
     reasons) file flags.
  - count=N : stop reading input after N ibs-sized read operations rather
     than proceeding until EOF. See iflag=count_bytes if stopping after N bytes
     is preferred
  - ibs=N : the size of buffer used for reads (default: 512)
  - if=FILE : the file used for input. When not specified, stdin is used instead
  - iflag=FLAGS : a comma-separated list of input flags which specify how the
     input source is treated. FLAGS may be any of the input-flags or general-flags
     specified below.
  - skip=N (or iseek=N) : skip N ibs-sized records into input before beginning
     copy/convert operations. See iflag=seek_bytes if seeking N bytes is preferred.
  - obs=N : the size of buffer used for writes (default: 512)
  - of=FILE : the file used for output. When not specified, stdout is used
     instead
  - oflag=FLAGS : comma separated list of output flags which specify how the
     output source is treated. FLAGS may be any of the output flags or general
     flags specified below
  - seek=N (or oseek=N) : seeks N obs-sized records into output before
     beginning copy/convert operations. See oflag=seek_bytes if seeking N bytes is
     preferred
  - status=LEVEL : controls whether volume and performance stats are written to
     stderr.

    When unspecified, dd will print stats upon completion. An example is below.

    ```plain
      6+0 records in
      16+0 records out
      8192 bytes (8.2 kB, 8.0 KiB) copied, 0.00057009 s,
      14.4 MB/s

    The first two lines are the 'volume' stats and the final line is the
    'performance' stats.
    The volume stats indicate the number of complete and partial ibs-sized reads,
    or obs-sized writes that took place during the copy. The format of the volume
    stats is <complete>+<partial>. If records have been truncated (see
    conv=block), the volume stats will contain the number of truncated records.

    Possible LEVEL values are:
    - progress : Print periodic performance stats as the copy proceeds.
    - noxfer : Print final volume stats, but not performance stats.
    - none : Do not print any stats.

    Printing performance stats is also triggered by the INFO signal (where supported),
    or the USR1 signal. Setting the POSIXLY_CORRECT environment variable to any value
    (including an empty value) will cause the USR1 signal to be ignored.

  ### Conversion Options

  - ascii : convert from EBCDIC to ASCII. This is the inverse of the ebcdic
    option. Implies conv=unblock.
  - ebcdic : convert from ASCII to EBCDIC. This is the inverse of the ascii
    option. Implies conv=block.
  - ibm : convert from ASCII to EBCDIC, applying the conventions for [, ]
    and ~ specified in POSIX. Implies conv=block.

  - ucase : convert from lower-case to upper-case.
  - lcase : converts from upper-case to lower-case.

  - block : for each newline less than the size indicated by cbs=BYTES, remove
    the newline and pad with spaces up to cbs. Lines longer than cbs are truncated.
  - unblock : for each block of input of the size indicated by cbs=BYTES, remove
    right-trailing spaces and replace with a newline character.

  - sparse : attempts to seek the output when an obs-sized block consists of
    only zeros.
  - swab : swaps each adjacent pair of bytes. If an odd number of bytes is
    present, the final byte is omitted.
  - sync : pad each ibs-sided block with zeros. If block or unblock is
    specified, pad with spaces instead.
  - excl : the output file must be created. Fail if the output file is already
    present.
  - nocreat : the output file will not be created. Fail if the output file in
    not already present.
  - notrunc : the output file will not be truncated. If this option is not
    present, output will be truncated when opened.
  - noerror : all read errors will be ignored. If this option is not present,
    dd will only ignore Error::Interrupted.
  - fdatasync : data will be written before finishing.
  - fsync : data and metadata will be written before finishing.

  ### Input flags

  - count_bytes : a value to count=N will be interpreted as bytes.
  - skip_bytes : a value to skip=N will be interpreted as bytes.
  - fullblock : wait for ibs bytes from each read. zero-length reads are still
    considered EOF.

  ### Output flags

  - append : open file in append mode. Consider setting conv=notrunc as well.
  - seek_bytes : a value to seek=N will be interpreted as bytes.

  ### General Flags

  - direct : use direct I/O for data.
  - directory : fail unless the given input (if used as an iflag) or
    output (if used as an oflag) is a directory.
  - dsync : use synchronized I/O for data.
  - sync : use synchronized I/O for data and metadata.
  - nonblock : use non-blocking I/O.
  - noatime : do not update access time.
  - nocache : request that OS drop cache.
  - noctty : do not assign a controlling tty.
  - nofollow : do not follow system links.

# Common strings
dd-standard-input = 'standard input'
dd-standard-output = 'standard output'

# Error messages
dd-error-failed-to-open = failed to open { $path }
dd-error-write-error = write error
dd-error-failed-to-seek = failed to seek in output file
dd-error-io-error = IO error
dd-error-cannot-skip-offset = '{ $file }': cannot skip to specified offset
dd-error-cannot-skip-invalid = '{ $file }': cannot skip: Invalid argument
dd-error-cannot-seek-invalid = '{ $output }': cannot seek: Invalid argument
dd-error-not-directory = setting flags for '{ $file }': Not a directory
dd-error-failed-discard-cache = failed to discard cache for: { $file }

# Parse errors
dd-error-unrecognized-operand = Unrecognized operand '{ $operand }'
dd-error-multiple-format-table = Only one of conv=ascii conv=ebcdic or conv=ibm may be specified
dd-error-multiple-case = Only one of conv=lcase or conv=ucase may be specified
dd-error-multiple-block = Only one of conv=block or conv=unblock may be specified
dd-error-multiple-excl = Only one ov conv=excl or conv=nocreat may be specified
dd-error-invalid-flag = invalid input flag: ‘{ $flag }’
  Try '{ $cmd } --help' for more information.
dd-error-conv-flag-no-match = Unrecognized conv=CONV -> { $flag }
dd-error-multiplier-parse-failure = invalid number: '{ $input }'
dd-error-multiplier-overflow = Multiplier string would overflow on current system -> { $input }
dd-error-block-without-cbs = conv=block or conv=unblock specified without cbs=N
dd-error-status-not-recognized = status=LEVEL not recognized -> { $level }
dd-error-unimplemented = feature not implemented on this system -> { $feature }
dd-error-bs-out-of-range = { $param }=N cannot fit into memory
dd-error-invalid-number = invalid number: ‘{ $input }’

# Progress messages
dd-progress-records-in = { $complete }+{ $partial } records in
dd-progress-records-out = { $complete }+{ $partial } records out
dd-progress-truncated-record = { $count ->
    [one] { $count } truncated record
   *[other] { $count } truncated records
}
dd-progress-byte-copied = { $bytes } byte copied, { $duration } s, { $rate }/s
dd-progress-bytes-copied = { $bytes } bytes copied, { $duration } s, { $rate }/s
dd-progress-bytes-copied-si = { $bytes } bytes ({ $si }) copied, { $duration } s, { $rate }/s
dd-progress-bytes-copied-si-iec = { $bytes } bytes ({ $si }, { $iec }) copied, { $duration } s, { $rate }/s

# Warnings
dd-warning-zero-multiplier = { $zero } is a zero multiplier; use { $alternative } if that is intended
dd-warning-signal-handler = Internal dd Warning: Unable to register signal handler
"),
        // Locale for dd (en-US)
        "dd/en-US.ftl" => Some(r"dd-about = Copy, and optionally convert, a file system resource
dd-usage = dd [OPERAND]...
  dd OPTION
dd-after-help = ### Operands

  - bs=BYTES : read and write up to BYTES bytes at a time (default: 512);
     overwrites ibs and obs.
  - cbs=BYTES : the 'conversion block size' in bytes. Applies to the
     conv=block, and conv=unblock operations.
  - conv=CONVS : a comma-separated list of conversion options or (for legacy
     reasons) file flags.
  - count=N : stop reading input after N ibs-sized read operations rather
     than proceeding until EOF. See iflag=count_bytes if stopping after N bytes
     is preferred
  - ibs=N : the size of buffer used for reads (default: 512)
  - if=FILE : the file used for input. When not specified, stdin is used instead
  - iflag=FLAGS : a comma-separated list of input flags which specify how the
     input source is treated. FLAGS may be any of the input-flags or general-flags
     specified below.
  - skip=N (or iseek=N) : skip N ibs-sized records into input before beginning
     copy/convert operations. See iflag=seek_bytes if seeking N bytes is preferred.
  - obs=N : the size of buffer used for writes (default: 512)
  - of=FILE : the file used for output. When not specified, stdout is used
     instead
  - oflag=FLAGS : comma separated list of output flags which specify how the
     output source is treated. FLAGS may be any of the output flags or general
     flags specified below
  - seek=N (or oseek=N) : seeks N obs-sized records into output before
     beginning copy/convert operations. See oflag=seek_bytes if seeking N bytes is
     preferred
  - status=LEVEL : controls whether volume and performance stats are written to
     stderr.

    When unspecified, dd will print stats upon completion. An example is below.

    ```plain
      6+0 records in
      16+0 records out
      8192 bytes (8.2 kB, 8.0 KiB) copied, 0.00057009 s,
      14.4 MB/s

    The first two lines are the 'volume' stats and the final line is the
    'performance' stats.
    The volume stats indicate the number of complete and partial ibs-sized reads,
    or obs-sized writes that took place during the copy. The format of the volume
    stats is <complete>+<partial>. If records have been truncated (see
    conv=block), the volume stats will contain the number of truncated records.

    Possible LEVEL values are:
    - progress : Print periodic performance stats as the copy proceeds.
    - noxfer : Print final volume stats, but not performance stats.
    - none : Do not print any stats.

    Printing performance stats is also triggered by the INFO signal (where supported),
    or the USR1 signal. Setting the POSIXLY_CORRECT environment variable to any value
    (including an empty value) will cause the USR1 signal to be ignored.

  ### Conversion Options

  - ascii : convert from EBCDIC to ASCII. This is the inverse of the ebcdic
    option. Implies conv=unblock.
  - ebcdic : convert from ASCII to EBCDIC. This is the inverse of the ascii
    option. Implies conv=block.
  - ibm : convert from ASCII to EBCDIC, applying the conventions for [, ]
    and ~ specified in POSIX. Implies conv=block.

  - ucase : convert from lower-case to upper-case.
  - lcase : converts from upper-case to lower-case.

  - block : for each newline less than the size indicated by cbs=BYTES, remove
    the newline and pad with spaces up to cbs. Lines longer than cbs are truncated.
  - unblock : for each block of input of the size indicated by cbs=BYTES, remove
    right-trailing spaces and replace with a newline character.

  - sparse : attempts to seek the output when an obs-sized block consists of
    only zeros.
  - swab : swaps each adjacent pair of bytes. If an odd number of bytes is
    present, the final byte is omitted.
  - sync : pad each ibs-sided block with zeros. If block or unblock is
    specified, pad with spaces instead.
  - excl : the output file must be created. Fail if the output file is already
    present.
  - nocreat : the output file will not be created. Fail if the output file in
    not already present.
  - notrunc : the output file will not be truncated. If this option is not
    present, output will be truncated when opened.
  - noerror : all read errors will be ignored. If this option is not present,
    dd will only ignore Error::Interrupted.
  - fdatasync : data will be written before finishing.
  - fsync : data and metadata will be written before finishing.

  ### Input flags

  - count_bytes : a value to count=N will be interpreted as bytes.
  - skip_bytes : a value to skip=N will be interpreted as bytes.
  - fullblock : wait for ibs bytes from each read. zero-length reads are still
    considered EOF.

  ### Output flags

  - append : open file in append mode. Consider setting conv=notrunc as well.
  - seek_bytes : a value to seek=N will be interpreted as bytes.

  ### General Flags

  - direct : use direct I/O for data.
  - directory : fail unless the given input (if used as an iflag) or
    output (if used as an oflag) is a directory.
  - dsync : use synchronized I/O for data.
  - sync : use synchronized I/O for data and metadata.
  - nonblock : use non-blocking I/O.
  - noatime : do not update access time.
  - nocache : request that OS drop cache.
  - noctty : do not assign a controlling tty.
  - nofollow : do not follow system links.

# Common strings
dd-standard-input = 'standard input'
dd-standard-output = 'standard output'

# Error messages
dd-error-failed-to-open = failed to open { $path }
dd-error-write-error = write error
dd-error-failed-to-seek = failed to seek in output file
dd-error-io-error = IO error
dd-error-cannot-skip-offset = '{ $file }': cannot skip to specified offset
dd-error-cannot-skip-invalid = '{ $file }': cannot skip: Invalid argument
dd-error-cannot-seek-invalid = '{ $output }': cannot seek: Invalid argument
dd-error-not-directory = setting flags for '{ $file }': Not a directory
dd-error-failed-discard-cache = failed to discard cache for: { $file }

# Parse errors
dd-error-unrecognized-operand = Unrecognized operand '{ $operand }'
dd-error-multiple-format-table = Only one of conv=ascii conv=ebcdic or conv=ibm may be specified
dd-error-multiple-case = Only one of conv=lcase or conv=ucase may be specified
dd-error-multiple-block = Only one of conv=block or conv=unblock may be specified
dd-error-multiple-excl = Only one ov conv=excl or conv=nocreat may be specified
dd-error-invalid-flag = invalid input flag: ‘{ $flag }’
  Try '{ $cmd } --help' for more information.
dd-error-conv-flag-no-match = Unrecognized conv=CONV -> { $flag }
dd-error-multiplier-parse-failure = invalid number: '{ $input }'
dd-error-multiplier-overflow = Multiplier string would overflow on current system -> { $input }
dd-error-block-without-cbs = conv=block or conv=unblock specified without cbs=N
dd-error-status-not-recognized = status=LEVEL not recognized -> { $level }
dd-error-unimplemented = feature not implemented on this system -> { $feature }
dd-error-bs-out-of-range = { $param }=N cannot fit into memory
dd-error-invalid-number = invalid number: ‘{ $input }’

# Progress messages
dd-progress-records-in = { $complete }+{ $partial } records in
dd-progress-records-out = { $complete }+{ $partial } records out
dd-progress-truncated-record = { $count ->
    [one] { $count } truncated record
   *[other] { $count } truncated records
}
dd-progress-byte-copied = { $bytes } byte copied, { $duration } s, { $rate }/s
dd-progress-bytes-copied = { $bytes } bytes copied, { $duration } s, { $rate }/s
dd-progress-bytes-copied-si = { $bytes } bytes ({ $si }) copied, { $duration } s, { $rate }/s
dd-progress-bytes-copied-si-iec = { $bytes } bytes ({ $si }, { $iec }) copied, { $duration } s, { $rate }/s

# Warnings
dd-warning-zero-multiplier = { $zero } is a zero multiplier; use { $alternative } if that is intended
dd-warning-signal-handler = Internal dd Warning: Unable to register signal handler
"),
        // Locale for df (en-US)
        "df/en-US.ftl" => Some(r"df-about = Show information about the file system on which each FILE resides,
  or all file systems by default.
df-usage = df [OPTION]... [FILE]...
df-after-help = Display values are in units of the first available SIZE from --block-size,
  and the DF_BLOCK_SIZE, BLOCK_SIZE and BLOCKSIZE environment variables.
  Otherwise, units default to 1024 bytes (or 512 if POSIXLY_CORRECT is set).

  SIZE is an integer and optional unit (example: 10M is 10*1024*1024).
  Units are K, M, G, T, P, E, Z, Y (powers of 1024) or KB, MB,... (powers
  of 1000). Units can be decimal, hexadecimal, octal, binary.

# Help messages
df-help-print-help = Print help information.
df-help-all = include dummy file systems
df-help-block-size = scale sizes by SIZE before printing them; e.g. '-BM' prints sizes in units of 1,048,576 bytes
df-help-total = produce a grand total
df-help-human-readable = print sizes in human readable format (e.g., 1K 234M 2G)
df-help-si = likewise, but use powers of 1000 not 1024
df-help-inodes = list inode information instead of block usage
df-help-kilo = like --block-size=1K
df-help-local = limit listing to local file systems
df-help-no-sync = do not invoke sync before getting usage info (default)
df-help-output = use output format defined by FIELD_LIST, or print all fields if FIELD_LIST is omitted.
df-help-portability = use the POSIX output format
df-help-sync = invoke sync before getting usage info (non-windows only)
df-help-type = limit listing to file systems of type TYPE
df-help-print-type = print file system type
df-help-exclude-type = limit listing to file systems not of type TYPE

# Error messages
df-error-block-size-too-large = --block-size argument '{ $size }' too large
df-error-invalid-block-size = invalid --block-size argument { $size }
df-error-invalid-suffix = invalid suffix in --block-size argument { $size }
df-error-field-used-more-than-once = option --output: field { $field } used more than once
df-error-filesystem-type-both-selected-and-excluded = file system type { $type } both selected and excluded
df-error-no-such-file-or-directory = { $path }: No such file or directory
df-error-no-file-systems-processed = no file systems processed
df-error-cannot-access-over-mounted = cannot access { $path }: over-mounted by another device
df-error-cannot-read-table-of-mounted-filesystems = cannot read table of mounted file systems
df-error-inodes-not-supported-windows = { $program }: doesn't support -i option

# Headers
df-header-filesystem = Filesystem
df-header-size = Size
df-header-used = Used
df-header-avail = Avail
df-header-available = Available
df-header-use-percent = Use%
df-header-capacity = Capacity
df-header-mounted-on = Mounted on
df-header-inodes = Inodes
df-header-iused = IUsed
df-header-iavail = IFree
df-header-iuse-percent = IUse%
df-header-file = File
df-header-type = Type

# Other
df-total = total
df-blocks-suffix = -blocks
"),
        // Locale for df (en-US)
        "df/en-US.ftl" => Some(r"df-about = Show information about the file system on which each FILE resides,
  or all file systems by default.
df-usage = df [OPTION]... [FILE]...
df-after-help = Display values are in units of the first available SIZE from --block-size,
  and the DF_BLOCK_SIZE, BLOCK_SIZE and BLOCKSIZE environment variables.
  Otherwise, units default to 1024 bytes (or 512 if POSIXLY_CORRECT is set).

  SIZE is an integer and optional unit (example: 10M is 10*1024*1024).
  Units are K, M, G, T, P, E, Z, Y (powers of 1024) or KB, MB,... (powers
  of 1000). Units can be decimal, hexadecimal, octal, binary.

# Help messages
df-help-print-help = Print help information.
df-help-all = include dummy file systems
df-help-block-size = scale sizes by SIZE before printing them; e.g. '-BM' prints sizes in units of 1,048,576 bytes
df-help-total = produce a grand total
df-help-human-readable = print sizes in human readable format (e.g., 1K 234M 2G)
df-help-si = likewise, but use powers of 1000 not 1024
df-help-inodes = list inode information instead of block usage
df-help-kilo = like --block-size=1K
df-help-local = limit listing to local file systems
df-help-no-sync = do not invoke sync before getting usage info (default)
df-help-output = use output format defined by FIELD_LIST, or print all fields if FIELD_LIST is omitted.
df-help-portability = use the POSIX output format
df-help-sync = invoke sync before getting usage info (non-windows only)
df-help-type = limit listing to file systems of type TYPE
df-help-print-type = print file system type
df-help-exclude-type = limit listing to file systems not of type TYPE

# Error messages
df-error-block-size-too-large = --block-size argument '{ $size }' too large
df-error-invalid-block-size = invalid --block-size argument { $size }
df-error-invalid-suffix = invalid suffix in --block-size argument { $size }
df-error-field-used-more-than-once = option --output: field { $field } used more than once
df-error-filesystem-type-both-selected-and-excluded = file system type { $type } both selected and excluded
df-error-no-such-file-or-directory = { $path }: No such file or directory
df-error-no-file-systems-processed = no file systems processed
df-error-cannot-access-over-mounted = cannot access { $path }: over-mounted by another device
df-error-cannot-read-table-of-mounted-filesystems = cannot read table of mounted file systems
df-error-inodes-not-supported-windows = { $program }: doesn't support -i option

# Headers
df-header-filesystem = Filesystem
df-header-size = Size
df-header-used = Used
df-header-avail = Avail
df-header-available = Available
df-header-use-percent = Use%
df-header-capacity = Capacity
df-header-mounted-on = Mounted on
df-header-inodes = Inodes
df-header-iused = IUsed
df-header-iavail = IFree
df-header-iuse-percent = IUse%
df-header-file = File
df-header-type = Type

# Other
df-total = total
df-blocks-suffix = -blocks
"),
        // Locale for dircolors (en-US)
        "dircolors/en-US.ftl" => Some(r"dircolors-about = Output commands to set the LS_COLORS environment variable.
dircolors-usage = dircolors [OPTION]... [FILE]
dircolors-after-help = If FILE is specified, read it to determine which colors to use for which
  file types and extensions. Otherwise, a precompiled database is used.
  For details on the format of these files, run 'dircolors --print-database'

# Help messages
dircolors-help-bourne-shell = output Bourne shell code to set LS_COLORS
dircolors-help-c-shell = output C shell code to set LS_COLORS
dircolors-help-print-database = print the byte counts
dircolors-help-print-ls-colors = output fully escaped colors for display

# Error messages
dircolors-error-shell-and-output-exclusive = the options to output non shell syntax,
  and to select a shell syntax are mutually exclusive
dircolors-error-print-database-and-ls-colors-exclusive = options --print-database and --print-ls-colors are mutually exclusive
dircolors-error-extra-operand-print-database = extra operand { $operand }
  file operands cannot be combined with --print-database (-p)
dircolors-error-no-shell-environment = no SHELL environment variable, and no shell type option given
dircolors-error-extra-operand = extra operand { $operand }
dircolors-error-expected-file-got-directory = expected file, got directory { $path }
dircolors-error-invalid-line-missing-token = { $file }:{ $line }: invalid line;  missing second token
dircolors-error-unrecognized-keyword = unrecognized keyword { $keyword }
"),
        // Locale for dircolors (en-US)
        "dircolors/en-US.ftl" => Some(r"dircolors-about = Output commands to set the LS_COLORS environment variable.
dircolors-usage = dircolors [OPTION]... [FILE]
dircolors-after-help = If FILE is specified, read it to determine which colors to use for which
  file types and extensions. Otherwise, a precompiled database is used.
  For details on the format of these files, run 'dircolors --print-database'

# Help messages
dircolors-help-bourne-shell = output Bourne shell code to set LS_COLORS
dircolors-help-c-shell = output C shell code to set LS_COLORS
dircolors-help-print-database = print the byte counts
dircolors-help-print-ls-colors = output fully escaped colors for display

# Error messages
dircolors-error-shell-and-output-exclusive = the options to output non shell syntax,
  and to select a shell syntax are mutually exclusive
dircolors-error-print-database-and-ls-colors-exclusive = options --print-database and --print-ls-colors are mutually exclusive
dircolors-error-extra-operand-print-database = extra operand { $operand }
  file operands cannot be combined with --print-database (-p)
dircolors-error-no-shell-environment = no SHELL environment variable, and no shell type option given
dircolors-error-extra-operand = extra operand { $operand }
dircolors-error-expected-file-got-directory = expected file, got directory { $path }
dircolors-error-invalid-line-missing-token = { $file }:{ $line }: invalid line;  missing second token
dircolors-error-unrecognized-keyword = unrecognized keyword { $keyword }
"),
        // Locale for dirname (en-US)
        "dirname/en-US.ftl" => Some(r"dirname-about = Strip last component from file name
dirname-usage = dirname [OPTION] NAME...
dirname-after-help = Output each NAME with its last non-slash component and trailing slashes
  removed; if NAME contains no /'s, output '.' (meaning the current directory).
dirname-missing-operand = missing operand
dirname-zero-help = separate output with NUL rather than newline
"),
        // Locale for dirname (en-US)
        "dirname/en-US.ftl" => Some(r"dirname-about = Strip last component from file name
dirname-usage = dirname [OPTION] NAME...
dirname-after-help = Output each NAME with its last non-slash component and trailing slashes
  removed; if NAME contains no /'s, output '.' (meaning the current directory).
dirname-missing-operand = missing operand
dirname-zero-help = separate output with NUL rather than newline
"),
        // Locale for du (en-US)
        "du/en-US.ftl" => Some(r#"du-about = Estimate file space usage
du-usage = du [OPTION]... [FILE]...
  du [OPTION]... --files0-from=F
du-after-help = Display values are in units of the first available SIZE from --block-size,
  and the DU_BLOCK_SIZE, BLOCK_SIZE and BLOCKSIZE environment variables.
  Otherwise, units default to 1024 bytes (or 512 if POSIXLY_CORRECT is set).

  SIZE is an integer and optional unit (example: 10M is 10*1024*1024).
  Units are K, M, G, T, P, E, Z, Y (powers of 1024) or KB, MB,... (powers
  of 1000). Units can be decimal, hexadecimal, octal, binary.

  PATTERN allows some advanced exclusions. For example, the following syntaxes
  are supported:
  ? will match only one character
  { "*" } will match zero or more characters
  {"{"}a,b{"}"} will match a or b

# Help messages
du-help-print-help = Print help information.
du-help-all = write counts for all files, not just directories
du-help-apparent-size = print apparent sizes, rather than disk usage although the apparent size is usually smaller, it may be larger due to holes in ('sparse') files, internal fragmentation, indirect blocks, and the like
du-help-block-size = scale sizes by SIZE before printing them. E.g., '-BM' prints sizes in units of 1,048,576 bytes. See SIZE format below.
du-help-bytes = equivalent to '--apparent-size --block-size=1'
du-help-total = produce a grand total
du-help-max-depth = print the total for a directory (or file, with --all) only if it is N or fewer levels below the command line argument;  --max-depth=0 is the same as --summarize
du-help-human-readable = print sizes in human readable format (e.g., 1K 234M 2G)
du-help-inodes = list inode usage information instead of block usage like --block-size=1K
du-help-block-size-1k = like --block-size=1K
du-help-count-links = count sizes many times if hard linked
du-help-dereference = follow all symbolic links
du-help-dereference-args = follow only symlinks that are listed on the command line
du-help-no-dereference = don't follow any symbolic links (this is the default)
du-help-block-size-1m = like --block-size=1M
du-help-null = end each output line with 0 byte rather than newline
du-help-separate-dirs = do not include size of subdirectories
du-help-summarize = display only a total for each argument
du-help-si = like -h, but use powers of 1000 not 1024
du-help-one-file-system = skip directories on different file systems
du-help-threshold = exclude entries smaller than SIZE if positive, or entries greater than SIZE if negative
du-help-verbose = verbose mode (option not present in GNU/Coreutils)
du-help-exclude = exclude files that match PATTERN
du-help-exclude-from = exclude files that match any pattern in FILE
du-help-files0-from = summarize device usage of the NUL-terminated file names specified in file F; if F is -, then read names from standard input
du-help-time = show time of the last modification of any file in the directory, or any of its subdirectories. If WORD is given, show time as WORD instead of modification time: atime, access, use, ctime, status, birth or creation
du-help-time-style = show times using style STYLE: full-iso, long-iso, iso, +FORMAT FORMAT is interpreted like 'date'

# Error messages
du-error-invalid-max-depth = invalid maximum depth { $depth }
du-error-summarize-depth-conflict = summarizing conflicts with --max-depth={ $depth }
du-error-invalid-time-style = invalid argument { $style } for 'time style'
  Valid arguments are:
    - 'full-iso'
    - 'long-iso'
    - 'iso'
    - +FORMAT (e.g., +%H:%M) for a 'date'-style format
  Try '{ $help }' for more information.
du-error-invalid-time-arg = 'birth' and 'creation' arguments for --time are not supported on this platform.
du-error-invalid-glob = Invalid exclude syntax: { $error }
du-error-cannot-read-directory = cannot read directory { $path }
du-error-cannot-access = cannot access { $path }
du-error-read-error-is-directory = { $file }: read error: Is a directory
du-error-cannot-open-for-reading = cannot open { $file } for reading: No such file or directory
du-error-invalid-zero-length-file-name = { $file }:{ $line }: invalid zero-length file name
du-error-extra-operand-with-files0-from = extra operand { $file }
  file operands cannot be combined with --files0-from
du-error-invalid-block-size-argument = invalid --{ $option } argument { $value }
du-error-cannot-access-no-such-file = cannot access { $path }: No such file or directory
du-error-printing-thread-panicked = Printing thread panicked.
du-error-invalid-suffix = invalid suffix in --{ $option } argument { $value }
du-error-invalid-argument = invalid --{ $option } argument { $value }
du-error-argument-too-large = --{ $option } argument { $value } too large
du-error-hyphen-file-name-not-allowed = when reading file names from standard input, no file name of '-' allowed

# Verbose/status messages
du-verbose-ignored = { $path } ignored
du-verbose-adding-to-exclude-list = adding { $pattern } to the exclude list
du-total = total
du-warning-apparent-size-ineffective-with-inodes = options --apparent-size and -b are ineffective with --inodes
"#),
        // Locale for du (en-US)
        "du/en-US.ftl" => Some(r#"du-about = Estimate file space usage
du-usage = du [OPTION]... [FILE]...
  du [OPTION]... --files0-from=F
du-after-help = Display values are in units of the first available SIZE from --block-size,
  and the DU_BLOCK_SIZE, BLOCK_SIZE and BLOCKSIZE environment variables.
  Otherwise, units default to 1024 bytes (or 512 if POSIXLY_CORRECT is set).

  SIZE is an integer and optional unit (example: 10M is 10*1024*1024).
  Units are K, M, G, T, P, E, Z, Y (powers of 1024) or KB, MB,... (powers
  of 1000). Units can be decimal, hexadecimal, octal, binary.

  PATTERN allows some advanced exclusions. For example, the following syntaxes
  are supported:
  ? will match only one character
  { "*" } will match zero or more characters
  {"{"}a,b{"}"} will match a or b

# Help messages
du-help-print-help = Print help information.
du-help-all = write counts for all files, not just directories
du-help-apparent-size = print apparent sizes, rather than disk usage although the apparent size is usually smaller, it may be larger due to holes in ('sparse') files, internal fragmentation, indirect blocks, and the like
du-help-block-size = scale sizes by SIZE before printing them. E.g., '-BM' prints sizes in units of 1,048,576 bytes. See SIZE format below.
du-help-bytes = equivalent to '--apparent-size --block-size=1'
du-help-total = produce a grand total
du-help-max-depth = print the total for a directory (or file, with --all) only if it is N or fewer levels below the command line argument;  --max-depth=0 is the same as --summarize
du-help-human-readable = print sizes in human readable format (e.g., 1K 234M 2G)
du-help-inodes = list inode usage information instead of block usage like --block-size=1K
du-help-block-size-1k = like --block-size=1K
du-help-count-links = count sizes many times if hard linked
du-help-dereference = follow all symbolic links
du-help-dereference-args = follow only symlinks that are listed on the command line
du-help-no-dereference = don't follow any symbolic links (this is the default)
du-help-block-size-1m = like --block-size=1M
du-help-null = end each output line with 0 byte rather than newline
du-help-separate-dirs = do not include size of subdirectories
du-help-summarize = display only a total for each argument
du-help-si = like -h, but use powers of 1000 not 1024
du-help-one-file-system = skip directories on different file systems
du-help-threshold = exclude entries smaller than SIZE if positive, or entries greater than SIZE if negative
du-help-verbose = verbose mode (option not present in GNU/Coreutils)
du-help-exclude = exclude files that match PATTERN
du-help-exclude-from = exclude files that match any pattern in FILE
du-help-files0-from = summarize device usage of the NUL-terminated file names specified in file F; if F is -, then read names from standard input
du-help-time = show time of the last modification of any file in the directory, or any of its subdirectories. If WORD is given, show time as WORD instead of modification time: atime, access, use, ctime, status, birth or creation
du-help-time-style = show times using style STYLE: full-iso, long-iso, iso, +FORMAT FORMAT is interpreted like 'date'

# Error messages
du-error-invalid-max-depth = invalid maximum depth { $depth }
du-error-summarize-depth-conflict = summarizing conflicts with --max-depth={ $depth }
du-error-invalid-time-style = invalid argument { $style } for 'time style'
  Valid arguments are:
    - 'full-iso'
    - 'long-iso'
    - 'iso'
    - +FORMAT (e.g., +%H:%M) for a 'date'-style format
  Try '{ $help }' for more information.
du-error-invalid-time-arg = 'birth' and 'creation' arguments for --time are not supported on this platform.
du-error-invalid-glob = Invalid exclude syntax: { $error }
du-error-cannot-read-directory = cannot read directory { $path }
du-error-cannot-access = cannot access { $path }
du-error-read-error-is-directory = { $file }: read error: Is a directory
du-error-cannot-open-for-reading = cannot open { $file } for reading: No such file or directory
du-error-invalid-zero-length-file-name = { $file }:{ $line }: invalid zero-length file name
du-error-extra-operand-with-files0-from = extra operand { $file }
  file operands cannot be combined with --files0-from
du-error-invalid-block-size-argument = invalid --{ $option } argument { $value }
du-error-cannot-access-no-such-file = cannot access { $path }: No such file or directory
du-error-printing-thread-panicked = Printing thread panicked.
du-error-invalid-suffix = invalid suffix in --{ $option } argument { $value }
du-error-invalid-argument = invalid --{ $option } argument { $value }
du-error-argument-too-large = --{ $option } argument { $value } too large
du-error-hyphen-file-name-not-allowed = when reading file names from standard input, no file name of '-' allowed

# Verbose/status messages
du-verbose-ignored = { $path } ignored
du-verbose-adding-to-exclude-list = adding { $pattern } to the exclude list
du-total = total
du-warning-apparent-size-ineffective-with-inodes = options --apparent-size and -b are ineffective with --inodes
"#),
        // Locale for echo (en-US)
        "echo/en-US.ftl" => Some(r"echo-about = Display a line of text
echo-usage = echo [OPTIONS]... [STRING]...
echo-after-help = Echo the STRING(s) to standard output.

  If -e is in effect, the following sequences are recognized:

  - \ backslash
  - \a alert (BEL)
  - \b backspace
  - \c produce no further output
  - \e escape
  - \f form feed
  - \n new line
  - \r carriage return
  - \t horizontal tab
  - \v vertical tab
  - \0NNN byte with octal value NNN (1 to 3 digits)
  - \xHH byte with hexadecimal value HH (1 to 2 digits)

echo-help-no-newline = do not output the trailing newline
echo-help-enable-escapes = enable interpretation of backslash escapes
echo-help-disable-escapes = disable interpretation of backslash escapes (default)

echo-error-non-utf8 = Non-UTF-8 arguments provided, but this platform does not support them
"),
        // Locale for echo (en-US)
        "echo/en-US.ftl" => Some(r"echo-about = Display a line of text
echo-usage = echo [OPTIONS]... [STRING]...
echo-after-help = Echo the STRING(s) to standard output.

  If -e is in effect, the following sequences are recognized:

  - \ backslash
  - \a alert (BEL)
  - \b backspace
  - \c produce no further output
  - \e escape
  - \f form feed
  - \n new line
  - \r carriage return
  - \t horizontal tab
  - \v vertical tab
  - \0NNN byte with octal value NNN (1 to 3 digits)
  - \xHH byte with hexadecimal value HH (1 to 2 digits)

echo-help-no-newline = do not output the trailing newline
echo-help-enable-escapes = enable interpretation of backslash escapes
echo-help-disable-escapes = disable interpretation of backslash escapes (default)

echo-error-non-utf8 = Non-UTF-8 arguments provided, but this platform does not support them
"),
        // Locale for env (en-US)
        "env/en-US.ftl" => Some(r#"env-about = Set each NAME to VALUE in the environment and run COMMAND
env-usage = env [OPTION]... [-] [NAME=VALUE]... [COMMAND [ARG]...]
env-after-help = A mere - implies -i. If no COMMAND, print the resulting environment.

# Help messages
env-help-ignore-environment = start with an empty environment
env-help-chdir = change working directory to DIR
env-help-null = end each output line with a 0 byte rather than a newline (only valid when printing the environment)
env-help-file = read and set variables from a ".env"-style configuration file (prior to any unset and/or set)
env-help-unset = remove variable from the environment
env-help-debug = print verbose information for each processing step
env-help-split-string = process and split S into separate arguments; used to pass multiple arguments on shebang lines
env-help-argv0 = Override the zeroth argument passed to the command being executed. Without this option a default value of `command` is used.
env-help-ignore-signal = set handling of SIG signal(s) to do nothing
env-help-default-signal = reset handling of SIG signal(s) to the default action
env-help-block-signal = block delivery of SIG signal(s) while running COMMAND
env-help-list-signal-handling = list signal handling changes requested by preceding options

# Error messages
env-error-missing-closing-quote = no terminating quote in -S string at position { $position } for quote '{ $quote }'
env-error-invalid-backslash-at-end = invalid backslash at end of string in -S at position { $position } in context { $context }
env-error-backslash-c-not-allowed = '\c' must not appear in double-quoted -S string at position { $position }
env-error-invalid-sequence = invalid sequence '\{ $char }' in -S at position { $position }
env-error-missing-closing-brace = Missing closing brace at position { $position }
env-error-missing-variable = Missing variable name at position { $position }
env-error-only-braced-variable = only ${VARNAME} expansion is supported at position { $position }
env-error-only-braced-variable-at = only {"${VARNAME}"} expansion is supported, error at: { $rest }
env-error-unexpected-number = Unexpected character: '{ $char }', expected variable name must not start with 0..9 at position { $position }
env-error-cannot-specify-null-with-command = cannot specify --null (-0) with command
env-error-invalid-signal = { $signal }: invalid signal
env-error-generic = Error: { $error }
env-error-no-such-file = { $program }: No such file or directory
env-error-use-s-shebang = use -[v]S to pass options in shebang lines
env-error-cannot-unset = cannot unset '{ $name }': Invalid argument
env-error-cannot-unset-invalid = cannot unset { $name }: Invalid argument
env-error-must-specify-command-with-chdir = must specify command with --chdir (-C)
env-error-cannot-change-directory = cannot change directory to { $directory }: { $error }
env-error-argv0-not-supported = --argv0 is currently not supported on this platform
env-error-permission-denied = { $program }: Permission denied
env-error-unknown = unknown error: { $error }
env-error-failed-set-signal-action = failed to set signal action for signal { $signal }: { $error }

# Warning messages
env-warning-no-name-specified = no name specified for value { $value }
"#),
        // Locale for env (en-US)
        "env/en-US.ftl" => Some(r#"env-about = Set each NAME to VALUE in the environment and run COMMAND
env-usage = env [OPTION]... [-] [NAME=VALUE]... [COMMAND [ARG]...]
env-after-help = A mere - implies -i. If no COMMAND, print the resulting environment.

# Help messages
env-help-ignore-environment = start with an empty environment
env-help-chdir = change working directory to DIR
env-help-null = end each output line with a 0 byte rather than a newline (only valid when printing the environment)
env-help-file = read and set variables from a ".env"-style configuration file (prior to any unset and/or set)
env-help-unset = remove variable from the environment
env-help-debug = print verbose information for each processing step
env-help-split-string = process and split S into separate arguments; used to pass multiple arguments on shebang lines
env-help-argv0 = Override the zeroth argument passed to the command being executed. Without this option a default value of `command` is used.
env-help-ignore-signal = set handling of SIG signal(s) to do nothing
env-help-default-signal = reset handling of SIG signal(s) to the default action
env-help-block-signal = block delivery of SIG signal(s) while running COMMAND
env-help-list-signal-handling = list signal handling changes requested by preceding options

# Error messages
env-error-missing-closing-quote = no terminating quote in -S string at position { $position } for quote '{ $quote }'
env-error-invalid-backslash-at-end = invalid backslash at end of string in -S at position { $position } in context { $context }
env-error-backslash-c-not-allowed = '\c' must not appear in double-quoted -S string at position { $position }
env-error-invalid-sequence = invalid sequence '\{ $char }' in -S at position { $position }
env-error-missing-closing-brace = Missing closing brace at position { $position }
env-error-missing-variable = Missing variable name at position { $position }
env-error-missing-closing-brace-after-value = Missing closing brace after default value at position { $position }
env-error-unexpected-number = Unexpected character: '{ $char }', expected variable name must not start with 0..9 at position { $position }
env-error-expected-brace-or-colon = Unexpected character: '{ $char }', expected a closing brace ('{"}"}') or colon (':') at position { $position }
env-error-cannot-specify-null-with-command = cannot specify --null (-0) with command
env-error-invalid-signal = { $signal }: invalid signal
env-error-variable-name-issue = variable name issue (at { $position }): { $error }
env-error-generic = Error: { $error }
env-error-no-such-file = { $program }: No such file or directory
env-error-use-s-shebang = use -[v]S to pass options in shebang lines
env-error-cannot-unset = cannot unset '{ $name }': Invalid argument
env-error-cannot-unset-invalid = cannot unset { $name }: Invalid argument
env-error-must-specify-command-with-chdir = must specify command with --chdir (-C)
env-error-cannot-change-directory = cannot change directory to { $directory }: { $error }
env-error-argv0-not-supported = --argv0 is currently not supported on this platform
env-error-permission-denied = { $program }: Permission denied
env-error-unknown = unknown error: { $error }
env-error-failed-set-signal-action = failed to set signal action for signal { $signal }: { $error }

# Warning messages
env-warning-no-name-specified = no name specified for value { $value }
"#),
        // Locale for expand (en-US)
        "expand/en-US.ftl" => Some(r"expand-about = Convert tabs in each FILE to spaces, writing to standard output.
  With no FILE, or when FILE is -, read standard input.
expand-usage = expand [OPTION]... [FILE]...

# Help messages
expand-help-initial = do not convert tabs after non blanks
expand-help-tabs = have tabs N characters apart, not 8 or use comma separated list of explicit tab positions
expand-help-no-utf8 = interpret input file as 8-bit ASCII rather than UTF-8

# Error messages
expand-error-invalid-character = tab size contains invalid character(s): { $char }
expand-error-specifier-not-at-start = { $specifier } specifier not at start of number: { $number }
expand-error-specifier-only-allowed-with-last = { $specifier } specifier only allowed with the last value
expand-error-tab-size-cannot-be-zero = tab size cannot be 0
expand-error-tab-size-too-large = tab stop is too large { $size }
expand-error-tab-sizes-must-be-ascending = tab sizes must be ascending
expand-error-is-directory = { $file }: Is a directory
expand-error-failed-to-write-output = failed to write output
"),
        // Locale for expand (en-US)
        "expand/en-US.ftl" => Some(r"expand-about = Convert tabs in each FILE to spaces, writing to standard output.
  With no FILE, or when FILE is -, read standard input.
expand-usage = expand [OPTION]... [FILE]...

# Help messages
expand-help-initial = do not convert tabs after non blanks
expand-help-tabs = have tabs N characters apart, not 8 or use comma separated list of explicit tab positions
expand-help-no-utf8 = interpret input file as 8-bit ASCII rather than UTF-8

# Error messages
expand-error-invalid-character = tab size contains invalid character(s): { $char }
expand-error-specifier-not-at-start = { $specifier } specifier not at start of number: { $number }
expand-error-specifier-only-allowed-with-last = { $specifier } specifier only allowed with the last value
expand-error-tab-size-cannot-be-zero = tab size cannot be 0
expand-error-tab-size-too-large = tab stop is too large { $size }
expand-error-tab-sizes-must-be-ascending = tab sizes must be ascending
expand-error-is-directory = { $file }: Is a directory
expand-error-failed-to-write-output = failed to write output
"),
        // Locale for expr (en-US)
        "expr/en-US.ftl" => Some(r#"expr-about = Print the value of EXPRESSION to standard output
expr-usage = expr [EXPRESSION]
  expr [OPTIONS]
expr-after-help = Print the value of EXPRESSION to standard output. A blank line below
  separates increasing precedence groups.

  EXPRESSION may be:

  - ARG1 | ARG2: ARG1 if it is neither null nor 0, otherwise ARG2
  - ARG1 & ARG2: ARG1 if neither argument is null or 0, otherwise 0
  - ARG1 < ARG2: ARG1 is less than ARG2
  - ARG1 <= ARG2: ARG1 is less than or equal to ARG2
  - ARG1 = ARG2: ARG1 is equal to ARG2
  - ARG1 != ARG2: ARG1 is unequal to ARG2
  - ARG1 >= ARG2: ARG1 is greater than or equal to ARG2
  - ARG1 > ARG2: ARG1 is greater than ARG2
  - ARG1 + ARG2: arithmetic sum of ARG1 and ARG2
  - ARG1 - ARG2: arithmetic difference of ARG1 and ARG2
  - ARG1 * ARG2: arithmetic product of ARG1 and ARG2
  - ARG1 / ARG2: arithmetic quotient of ARG1 divided by ARG2
  - ARG1 % ARG2: arithmetic remainder of ARG1 divided by ARG2
  - STRING : REGEXP: anchored pattern match of REGEXP in STRING
  - match STRING REGEXP: same as STRING : REGEXP
  - substr STRING POS LENGTH: substring of STRING, POS counted from 1
  - index STRING CHARS: index in STRING where any CHARS is found, or 0
  - length STRING: length of STRING
  - + TOKEN: interpret TOKEN as a string, even if it is a keyword like match
    or an operator like /
  - ( EXPRESSION ): value of EXPRESSION

  Beware that many operators need to be escaped or quoted for shells.
  Comparisons are arithmetic if both ARGs are numbers, else lexicographical.
  Pattern matches return the string matched between \( and \) or null; if
  \( and \) are not used, they return the number of characters matched or 0.

  Exit status is 0 if EXPRESSION is neither null nor 0, 1 if EXPRESSION
  is null or 0, 2 if EXPRESSION is syntactically invalid, and 3 if an
  error occurred.

  Environment variables:

  - EXPR_DEBUG_TOKENS=1: dump expression's tokens
  - EXPR_DEBUG_RPN=1: dump expression represented in reverse polish notation
  - EXPR_DEBUG_SYA_STEP=1: dump each parser step
  - EXPR_DEBUG_AST=1: dump expression represented abstract syntax tree

# Help messages
expr-help-version = output version information and exit
expr-help-help = display this help and exit

# Error messages
expr-error-unexpected-argument = syntax error: unexpected argument { $arg }
expr-error-missing-argument = syntax error: missing argument after { $arg }
expr-error-non-integer-argument = non-integer argument
expr-error-missing-operand = missing operand
expr-error-division-by-zero = division by zero
expr-error-invalid-regex-expression = Invalid regex expression
expr-error-expected-closing-brace-after = syntax error: expecting ')' after { $arg }
expr-error-expected-closing-brace-instead-of = syntax error: expecting ')' instead of { $arg }
expr-error-unmatched-opening-parenthesis = Unmatched ( or \(
expr-error-unmatched-closing-parenthesis = Unmatched ) or \)
expr-error-unmatched-opening-brace = Unmatched {"\\{"}
expr-error-invalid-bracket-content = Invalid content of {"\\{\\}"}
expr-error-trailing-backslash = Trailing backslash
expr-error-too-big-range-quantifier-index = Regular expression too big
expr-error-match-utf8 = match does not support invalid UTF-8 encoding in { $arg }
"#),
        // Locale for expr (en-US)
        "expr/en-US.ftl" => Some(r#"expr-about = Print the value of EXPRESSION to standard output
expr-usage = expr [EXPRESSION]
  expr [OPTIONS]
expr-after-help = Print the value of EXPRESSION to standard output. A blank line below
  separates increasing precedence groups.

  EXPRESSION may be:

  - ARG1 | ARG2: ARG1 if it is neither null nor 0, otherwise ARG2
  - ARG1 & ARG2: ARG1 if neither argument is null or 0, otherwise 0
  - ARG1 < ARG2: ARG1 is less than ARG2
  - ARG1 <= ARG2: ARG1 is less than or equal to ARG2
  - ARG1 = ARG2: ARG1 is equal to ARG2
  - ARG1 != ARG2: ARG1 is unequal to ARG2
  - ARG1 >= ARG2: ARG1 is greater than or equal to ARG2
  - ARG1 > ARG2: ARG1 is greater than ARG2
  - ARG1 + ARG2: arithmetic sum of ARG1 and ARG2
  - ARG1 - ARG2: arithmetic difference of ARG1 and ARG2
  - ARG1 * ARG2: arithmetic product of ARG1 and ARG2
  - ARG1 / ARG2: arithmetic quotient of ARG1 divided by ARG2
  - ARG1 % ARG2: arithmetic remainder of ARG1 divided by ARG2
  - STRING : REGEXP: anchored pattern match of REGEXP in STRING
  - match STRING REGEXP: same as STRING : REGEXP
  - substr STRING POS LENGTH: substring of STRING, POS counted from 1
  - index STRING CHARS: index in STRING where any CHARS is found, or 0
  - length STRING: length of STRING
  - + TOKEN: interpret TOKEN as a string, even if it is a keyword like match
    or an operator like /
  - ( EXPRESSION ): value of EXPRESSION

  Beware that many operators need to be escaped or quoted for shells.
  Comparisons are arithmetic if both ARGs are numbers, else lexicographical.
  Pattern matches return the string matched between \( and \) or null; if
  \( and \) are not used, they return the number of characters matched or 0.

  Exit status is 0 if EXPRESSION is neither null nor 0, 1 if EXPRESSION
  is null or 0, 2 if EXPRESSION is syntactically invalid, and 3 if an
  error occurred.

  Environment variables:

  - EXPR_DEBUG_TOKENS=1: dump expression's tokens
  - EXPR_DEBUG_RPN=1: dump expression represented in reverse polish notation
  - EXPR_DEBUG_SYA_STEP=1: dump each parser step
  - EXPR_DEBUG_AST=1: dump expression represented abstract syntax tree

# Help messages
expr-help-version = output version information and exit
expr-help-help = display this help and exit

# Error messages
expr-error-unexpected-argument = syntax error: unexpected argument { $arg }
expr-error-missing-argument = syntax error: missing argument after { $arg }
expr-error-non-integer-argument = non-integer argument
expr-error-missing-operand = missing operand
expr-error-division-by-zero = division by zero
expr-error-invalid-regex-expression = Invalid regex expression
expr-error-expected-closing-brace-after = syntax error: expecting ')' after { $arg }
expr-error-expected-closing-brace-instead-of = syntax error: expecting ')' instead of { $arg }
expr-error-unmatched-opening-parenthesis = Unmatched ( or \(
expr-error-unmatched-closing-parenthesis = Unmatched ) or \)
expr-error-unmatched-opening-brace = Unmatched {"\\{"}
expr-error-invalid-bracket-content = Invalid content of {"\\{\\}"}
expr-error-trailing-backslash = Trailing backslash
expr-error-too-big-range-quantifier-index = Regular expression too big
expr-error-match-utf8 = match does not support invalid UTF-8 encoding in { $arg }
"#),
        // Locale for factor (en-US)
        "factor/en-US.ftl" => Some(r"factor-about = Print the prime factors of the given NUMBER(s).
  If none are specified, read from standard input.
factor-usage = factor [OPTION]... [NUMBER]...

# Help messages
factor-help-exponents = Print factors in the form p^e
factor-help-help = Print help information.

# Error messages
factor-error-factorization-incomplete = Factorization incomplete. Remainders exists.
factor-error-write-error = write error
factor-error-reading-input = error reading input: { $error }
factor-error-invalid-int = is not a valid positive integer
"),
        // Locale for factor (en-US)
        "factor/en-US.ftl" => Some(r"factor-about = Print the prime factors of the given NUMBER(s).
  If none are specified, read from standard input.
factor-usage = factor [OPTION]... [NUMBER]...

# Help messages
factor-help-exponents = Print factors in the form p^e
factor-help-help = Print help information.

# Error messages
factor-error-factorization-incomplete = Factorization incomplete. Remainders exists.
factor-error-write-error = write error
factor-error-reading-input = error reading input: { $error }
factor-error-invalid-int = invalid digit found in string
"),
        // Locale for false (en-US)
        "false/en-US.ftl" => Some(r"false-about = Returns false, an unsuccessful exit status.

  Immediately returns with the exit status 1. When invoked with one of the recognized options it
  will try to write the help or version text. Any IO error during this operation is diagnosed, yet
  the program will also return 1.

false-help-text = Print help information
false-version-text = Print version information
"),
        // Locale for false (en-US)
        "false/en-US.ftl" => Some(r"false-about = Returns false, an unsuccessful exit status.

  Immediately returns with the exit status 1. When invoked with one of the recognized options it
  will try to write the help or version text. Any IO error during this operation is diagnosed, yet
  the program will also return 1.

false-help-text = Print help information
false-version-text = Print version information
"),
        // Locale for fmt (en-US)
        "fmt/en-US.ftl" => Some(r"fmt-about = Reformat paragraphs from input (or standard input) to stdout.
fmt-usage = [OPTION]... [FILE]...

# Help messages
fmt-crown-margin-help = First and second line of paragraph may have different indentations, in which case the first line's indentation is preserved, and each subsequent line's indentation matches the second line.
fmt-tagged-paragraph-help = Like -c, except that the first and second line of a paragraph *must* have different indentation or they are treated as separate paragraphs.
fmt-preserve-headers-help = Attempt to detect and preserve mail headers in the input. Be careful when combining this flag with -p.
fmt-split-only-help = Split lines only, do not reflow.
fmt-uniform-spacing-help = Insert exactly one space between words, and two between sentences. Sentence breaks in the input are detected as [?!.] followed by two spaces or a newline; other punctuation is not interpreted as a sentence break.
fmt-prefix-help = Reformat only lines beginning with PREFIX, reattaching PREFIX to reformatted lines. Unless -x is specified, leading whitespace will be ignored when matching PREFIX.
fmt-skip-prefix-help = Do not reformat lines beginning with PSKIP. Unless -X is specified, leading whitespace will be ignored when matching PSKIP
fmt-exact-prefix-help = PREFIX must match at the beginning of the line with no preceding whitespace.
fmt-exact-skip-prefix-help = PSKIP must match at the beginning of the line with no preceding whitespace.
fmt-width-help = Fill output lines up to a maximum of WIDTH columns, default 75. This can be specified as a negative number in the first argument.
fmt-goal-help = Goal width, default of 93% of WIDTH. Must be less than or equal to WIDTH.
fmt-quick-help = Break lines more quickly at the expense of a potentially more ragged appearance.
fmt-tab-width-help = Treat tabs as TABWIDTH spaces for determining line length, default 8. Note that this is used only for calculating line lengths; tabs are preserved in the output.

# Error messages
fmt-error-invalid-goal = invalid goal: {$goal}
fmt-error-goal-greater-than-width = GOAL cannot be greater than WIDTH.
fmt-error-invalid-width = invalid width: {$width}
fmt-error-width-out-of-range = invalid width: '{$width}': Numerical result out of range
fmt-error-invalid-tabwidth = Invalid TABWIDTH specification: {$tabwidth}
fmt-error-first-option-width = invalid option -- {$option}; -WIDTH is recognized only when it is the first
  option; use -w N instead
  Try 'fmt --help' for more information.
fmt-error-read = read error
fmt-error-invalid-width-malformed = invalid width: {$width}
fmt-error-cannot-open-for-reading = cannot open {$file} for reading
fmt-error-cannot-get-metadata = cannot get metadata for {$file}
fmt-error-failed-to-write-output = failed to write output
"),
        // Locale for fmt (en-US)
        "fmt/en-US.ftl" => Some(r"fmt-about = Reformat paragraphs from input (or standard input) to stdout.
fmt-usage = [OPTION]... [FILE]...

# Help messages
fmt-crown-margin-help = First and second line of paragraph may have different indentations, in which case the first line's indentation is preserved, and each subsequent line's indentation matches the second line.
fmt-tagged-paragraph-help = Like -c, except that the first and second line of a paragraph *must* have different indentation or they are treated as separate paragraphs.
fmt-preserve-headers-help = Attempt to detect and preserve mail headers in the input. Be careful when combining this flag with -p.
fmt-split-only-help = Split lines only, do not reflow.
fmt-uniform-spacing-help = Insert exactly one space between words, and two between sentences. Sentence breaks in the input are detected as [?!.] followed by two spaces or a newline; other punctuation is not interpreted as a sentence break.
fmt-prefix-help = Reformat only lines beginning with PREFIX, reattaching PREFIX to reformatted lines. Unless -x is specified, leading whitespace will be ignored when matching PREFIX.
fmt-skip-prefix-help = Do not reformat lines beginning with PSKIP. Unless -X is specified, leading whitespace will be ignored when matching PSKIP
fmt-exact-prefix-help = PREFIX must match at the beginning of the line with no preceding whitespace.
fmt-exact-skip-prefix-help = PSKIP must match at the beginning of the line with no preceding whitespace.
fmt-width-help = Fill output lines up to a maximum of WIDTH columns, default 75. This can be specified as a negative number in the first argument.
fmt-goal-help = Goal width, default of 93% of WIDTH. Must be less than or equal to WIDTH.
fmt-quick-help = Break lines more quickly at the expense of a potentially more ragged appearance.
fmt-tab-width-help = Treat tabs as TABWIDTH spaces for determining line length, default 8. Note that this is used only for calculating line lengths; tabs are preserved in the output.

# Error messages
fmt-error-invalid-goal = invalid goal: {$goal}
fmt-error-goal-greater-than-width = GOAL cannot be greater than WIDTH.
fmt-error-invalid-width = invalid width: {$width}
fmt-error-width-out-of-range = invalid width: '{$width}': Numerical result out of range
fmt-error-invalid-tabwidth = Invalid TABWIDTH specification: {$tabwidth}
fmt-error-first-option-width = invalid option -- {$option}; -WIDTH is recognized only when it is the first
  option; use -w N instead
  Try 'fmt --help' for more information.
fmt-error-read = read error
fmt-error-invalid-width-malformed = invalid width: {$width}
fmt-error-cannot-open-for-reading = cannot open {$file} for reading
fmt-error-cannot-get-metadata = cannot get metadata for {$file}
fmt-error-failed-to-write-output = failed to write output
"),
        // Locale for fold (en-US)
        "fold/en-US.ftl" => Some(r"fold-about = Writes each file (or standard input if no files are given)
  to standard output whilst breaking long lines
fold-usage = fold [OPTION]... [FILE]...
fold-bytes-help = count using bytes rather than columns (meaning control characters such as newline are not treated specially)
fold-characters-help = count using character positions rather than display columns
fold-spaces-help = break lines at word boundaries rather than a hard cut-off
fold-width-help = set WIDTH as the maximum line width rather than 80
fold-error-illegal-width = illegal width value
fold-error-readline = failed to read line
"),
        // Locale for fold (en-US)
        "fold/en-US.ftl" => Some(r"fold-about = Writes each file (or standard input if no files are given)
  to standard output whilst breaking long lines
fold-usage = fold [OPTION]... [FILE]...
fold-bytes-help = count using bytes rather than columns (meaning control characters such as newline are not treated specially)
fold-characters-help = count using character positions rather than display columns
fold-spaces-help = break lines at word boundaries rather than a hard cut-off
fold-width-help = set WIDTH as the maximum line width rather than 80
fold-error-illegal-width = illegal width value
fold-error-readline = failed to read line
"),
        // Locale for head (en-US)
        "head/en-US.ftl" => Some(r"head-about = Print the first 10 lines of each FILE to standard output.
  With more than one FILE, precede each with a header giving the file name.
  With no FILE, or when FILE is -, read standard input.

  Mandatory arguments to long options are mandatory for short options too.
head-usage = head [OPTION]... [FILE]...

# Help messages
head-help-bytes = print the first NUM bytes of each file;
 with a leading '-', print all but the last
 NUM bytes of each file
head-help-lines = print the first NUM lines instead of the first 10;
 with a leading '-', print all but the last
 NUM lines of each file
head-help-quiet = never print headers giving file names
head-help-verbose = always print headers giving file names
head-help-zero-terminated = line delimiter is NUL, not newline

# Error messages
head-error-reading-file = error reading {$name}: {$err}
head-error-parse-error = parse error: {$err}
head-error-num-too-large = number of -bytes or -lines is too large
head-error-clap = clap error: {$err}
head-error-invalid-bytes = invalid number of bytes: {$err}
head-error-invalid-lines = invalid number of lines: {$err}
head-error-bad-argument-format = bad argument format: {$arg}
head-error-writing-stdout = error writing 'standard output': {$err}
head-error-cannot-open = cannot open {$name} for reading

# Output headers
head-header-stdin = ==> standard input <==
"),
        // Locale for head (en-US)
        "head/en-US.ftl" => Some(r"head-about = Print the first 10 lines of each FILE to standard output.
  With more than one FILE, precede each with a header giving the file name.
  With no FILE, or when FILE is -, read standard input.

  Mandatory arguments to long flags are mandatory for short flags too.
head-usage = head [FLAG]... [FILE]...

# Help messages
head-help-bytes = print the first NUM bytes of each file;
 with a leading '-', print all but the last
 NUM bytes of each file
head-help-lines = print the first NUM lines instead of the first 10;
 with a leading '-', print all but the last
 NUM lines of each file
head-help-quiet = never print headers giving file names
head-help-verbose = always print headers giving file names
head-help-zero-terminated = line delimiter is NUL, not newline

# Error messages
head-error-reading-file = error reading {$name}: {$err}
head-error-parse-error = parse error: {$err}
head-error-num-too-large = number of -bytes or -lines is too large
head-error-clap = clap error: {$err}
head-error-invalid-bytes = invalid number of bytes: {$err}
head-error-invalid-lines = invalid number of lines: {$err}
head-error-bad-argument-format = bad argument format: {$arg}
head-error-writing-stdout = error writing 'standard output': {$err}
head-error-cannot-open = cannot open {$name} for reading

# Output headers
head-header-stdin = ==> standard input <==
"),
        // Locale for hostname (en-US)
        "hostname/en-US.ftl" => Some(r"hostname-about = Display or set the system's host name.
hostname-usage = hostname [OPTION]... [HOSTNAME]
hostname-help-domain = Display the name of the DNS domain if possible
hostname-help-ip-address = Display the network address(es) of the host
hostname-help-fqdn = Display the FQDN (Fully Qualified Domain Name) (default)
hostname-help-short = Display the short hostname (the portion before the first dot) if possible
hostname-error-permission = hostname: you must be root to change the host name
hostname-error-invalid-name = hostname: invalid hostname '{ $name }'
hostname-error-resolve-failed = hostname: unable to resolve host name '{ $name }'
hostname-error-winsock = failed to start Winsock
hostname-error-set-hostname = failed to set hostname
hostname-error-get-hostname = failed to get hostname
hostname-error-resolve-socket = failed to resolve socket addresses
"),
        // Locale for hostname (en-US)
        "hostname/en-US.ftl" => Some(r"hostname-about = Display or set the system's host name.
hostname-usage = hostname [OPTION]... [HOSTNAME]
hostname-help-domain = Display the name of the DNS domain if possible
hostname-help-ip-address = Display the network address(es) of the host
hostname-help-fqdn = Display the FQDN (Fully Qualified Domain Name) (default)
hostname-help-short = Display the short hostname (the portion before the first dot) if possible
hostname-error-permission = hostname: you must be root to change the host name
hostname-error-invalid-name = hostname: invalid hostname '{ $name }'
hostname-error-resolve-failed = hostname: unable to resolve host name '{ $name }'
hostname-error-winsock = failed to start Winsock
hostname-error-set-hostname = failed to set hostname
hostname-error-get-hostname = failed to get hostname
hostname-error-resolve-socket = failed to resolve socket addresses
"),
        // Locale for join (en-US)
        "join/en-US.ftl" => Some(r"join-about = For each pair of input lines with identical join fields, write a line to
  standard output. The default join field is the first, delimited by blanks.

  When FILE1 or FILE2 (not both) is -, read standard input.
join-usage = join [OPTION]... FILE1 FILE2

# Join help messages
join-help-a = also print unpairable lines from file FILENUM, where
  FILENUM is 1 or 2, corresponding to FILE1 or FILE2
join-help-v = like -a FILENUM, but suppress joined output lines
join-help-e = replace missing input fields with EMPTY
join-help-i = ignore differences in case when comparing fields
join-help-j = equivalent to '-1 FIELD -2 FIELD'
join-help-o = obey FORMAT while constructing output line
join-help-t = use CHAR as input and output field separator
join-help-1 = join on this FIELD of file 1
join-help-2 = join on this FIELD of file 2
join-help-check-order = check that the input is correctly sorted, even if all input lines are pairable
join-help-nocheck-order = do not check that the input is correctly sorted
join-help-header = treat the first line in each file as field headers, print them without trying to pair them
join-help-z = line delimiter is NUL, not newline

# Join error messages
join-error-io = io error: { $error }
join-error-non-utf8-tab = non-UTF-8 multi-byte tab
join-error-unprintable-separators = unprintable field separators are only supported on unix-like platforms
join-error-multi-character-tab = multi-character tab { $value }
join-error-both-files-stdin = both files cannot be standard input
join-error-invalid-field-specifier = invalid field specifier: { $spec }
join-error-invalid-file-number = invalid file number in field spec: { $spec }
join-error-invalid-file-number-simple = invalid file number: { $value }
join-error-invalid-field-number = invalid field number: { $value }
join-error-incompatible-fields = incompatible join fields { $field1 }, { $field2 }
join-error-not-sorted = { $file }:{ $line_num }: is not sorted: { $content }
join-error-input-not-sorted = input is not in sorted order
"),
        // Locale for join (en-US)
        "join/en-US.ftl" => Some(r"join-about = For each pair of input lines with identical join fields, write a line to
  standard output. The default join field is the first, delimited by blanks.

  When FILE1 or FILE2 (not both) is -, read standard input.
join-usage = join [OPTION]... FILE1 FILE2

# Join help messages
join-help-a = also print unpairable lines from file FILENUM, where
  FILENUM is 1 or 2, corresponding to FILE1 or FILE2
join-help-v = like -a FILENUM, but suppress joined output lines
join-help-e = replace missing input fields with EMPTY
join-help-i = ignore differences in case when comparing fields
join-help-j = equivalent to '-1 FIELD -2 FIELD'
join-help-o = obey FORMAT while constructing output line
join-help-t = use CHAR as input and output field separator
join-help-1 = join on this FIELD of file 1
join-help-2 = join on this FIELD of file 2
join-help-check-order = check that the input is correctly sorted, even if all input lines are pairable
join-help-nocheck-order = do not check that the input is correctly sorted
join-help-header = treat the first line in each file as field headers, print them without trying to pair them
join-help-z = line delimiter is NUL, not newline

# Join error messages
join-error-io = io error: { $error }
join-error-non-utf8-tab = non-UTF-8 multi-byte tab
join-error-unprintable-separators = unprintable field separators are only supported on unix-like platforms
join-error-multi-character-tab = multi-character tab { $value }
join-error-both-files-stdin = both files cannot be standard input
join-error-invalid-field-specifier = invalid field specifier: { $spec }
join-error-invalid-file-number = invalid file number in field spec: { $spec }
join-error-invalid-file-number-simple = invalid file number: { $value }
join-error-invalid-field-number = invalid field number: { $value }
join-error-incompatible-fields = incompatible join fields { $field1 }, { $field2 }
join-error-not-sorted = { $file }:{ $line_num }: is not sorted: { $content }
join-error-input-not-sorted = input is not in sorted order
"),
        // Locale for link (en-US)
        "link/en-US.ftl" => Some(r"link-about = Call the link function to create a link named FILE2 to an existing FILE1.
link-usage = link FILE1 FILE2

link-error-cannot-create-link = cannot create link { $new } to { $old }
"),
        // Locale for link (en-US)
        "link/en-US.ftl" => Some(r"link-about = Call the link function to create a link named FILE2 to an existing FILE1.
link-usage = link FILE1 FILE2

link-error-cannot-create-link = cannot create link { $new } to { $old }
"),
        // Locale for ln (en-US)
        "ln/en-US.ftl" => Some(r"ln-about = Make links between files.
ln-usage = ln [OPTION]... [-T] TARGET LINK_NAME
  ln [OPTION]... TARGET
  ln [OPTION]... TARGET... DIRECTORY
  ln [OPTION]... -t DIRECTORY TARGET...
ln-after-help = In the 1st form, create a link to TARGET with the name LINK_NAME.
  In the 2nd form, create a link to TARGET in the current directory.
  In the 3rd and 4th forms, create links to each TARGET in DIRECTORY.
  Create hard links by default, symbolic links with --symbolic.
  By default, each destination (name of new link) should not already exist.
  When creating hard links, each TARGET must exist. Symbolic links
  can hold arbitrary text; if later resolved, a relative link is
  interpreted in relation to its parent directory.

ln-help-force = remove existing destination files
ln-help-interactive = prompt whether to remove existing destination files
ln-help-no-dereference = treat LINK_NAME as a normal file if it is a
                          symbolic link to a directory
ln-help-logical = follow TARGETs that are symbolic links
ln-help-physical = make hard links directly to symbolic links
ln-help-symbolic = make symbolic links instead of hard links
ln-help-target-directory = specify the DIRECTORY in which to create the links
ln-help-no-target-directory = treat LINK_NAME as a normal file always
ln-help-relative = create symbolic links relative to link location
ln-help-verbose = print name of each linked file
ln-error-target-is-not-directory = target {$target} is not a directory
ln-error-same-file = {$file1} and {$file2} are the same file
ln-error-missing-destination = missing destination file operand after {$operand}
ln-error-extra-operand = extra operand {$operand}
  Try '{$program} --help' for more information.
ln-error-could-not-update = Could not update {$target}: {$error}
ln-error-will-not-overwrite = will not overwrite just-created {$target} with {$source}
ln-prompt-replace = replace {$file}?
ln-cannot-backup = cannot backup {$file}
ln-failed-to-access = failed to access {$file}
ln-failed-to-create-hard-link = failed to create hard link {$source} => {$dest}
ln-failed-to-create-symbolic-link = failed to create symbolic link {$dest}
ln-failed-to-create-hard-link-dir = {$source}: hard link not allowed for directory
ln-backup = backup: {$backup}
"),
        // Locale for ln (en-US)
        "ln/en-US.ftl" => Some(r"ln-about = Make links between files.
ln-usage = ln [OPTION]... [-T] TARGET LINK_NAME
  ln [OPTION]... TARGET
  ln [OPTION]... TARGET... DIRECTORY
  ln [OPTION]... -t DIRECTORY TARGET...
ln-after-help = In the 1st form, create a link to TARGET with the name LINK_NAME.
  In the 2nd form, create a link to TARGET in the current directory.
  In the 3rd and 4th forms, create links to each TARGET in DIRECTORY.
  Create hard links by default, symbolic links with --symbolic.
  By default, each destination (name of new link) should not already exist.
  When creating hard links, each TARGET must exist. Symbolic links
  can hold arbitrary text; if later resolved, a relative link is
  interpreted in relation to its parent directory.

ln-help-force = remove existing destination files
ln-help-interactive = prompt whether to remove existing destination files
ln-help-no-dereference = treat LINK_NAME as a normal file if it is a
                          symbolic link to a directory
ln-help-logical = follow TARGETs that are symbolic links
ln-help-physical = make hard links directly to symbolic links
ln-help-symbolic = make symbolic links instead of hard links
ln-help-target-directory = specify the DIRECTORY in which to create the links
ln-help-no-target-directory = treat LINK_NAME as a normal file always
ln-help-relative = create symbolic links relative to link location
ln-help-verbose = print name of each linked file
ln-error-target-is-not-directory = target {$target} is not a directory
ln-error-same-file = {$file1} and {$file2} are the same file
ln-error-missing-destination = missing destination file operand after {$operand}
ln-error-extra-operand = extra operand {$operand}
  Try '{$program} --help' for more information.
ln-error-could-not-update = Could not update {$target}: {$error}
ln-error-cannot-stat = cannot stat {$path}: No such file or directory
ln-error-will-not-overwrite = will not overwrite just-created {$target} with {$source}
ln-prompt-replace = replace {$file}?
ln-cannot-backup = cannot backup {$file}
ln-failed-to-access = failed to access {$file}
ln-failed-to-create-hard-link = failed to create hard link {$source} => {$dest}
ln-failed-to-create-hard-link-dir = {$source}: hard link not allowed for directory
ln-backup = backup: {$backup}
"),
        // Locale for ls (en-US)
        "ls/en-US.ftl" => Some(r"ls-about = List directory contents.
  Ignore files and directories starting with a '.' by default
dir-about = List directory contents.
  Ignore files and directories starting with a '.' by default
vdir-about = List directory contents.
  Ignore files and directories starting with a '.' by default

  Mandatory arguments to long options are mandatory for short options too.
ls-usage = ls [OPTION]... [FILE]...
dir-usage = dir [OPTION]... [FILE]...
vdir-usage = vdir [OPTION]... [FILE]...
ls-after-help = The TIME_STYLE argument can be full-iso, long-iso, iso, locale or +FORMAT. FORMAT is interpreted like in date. Also the TIME_STYLE environment variable sets the default style to use.

# Error messages
ls-error-invalid-tab-size = invalid tab size: {$size}
ls-error-invalid-line-width = invalid line width: {$width}
ls-error-general-io = general io error: {$error}
ls-error-cannot-access-no-such-file = cannot access {$path}: No such file or directory
ls-error-cannot-access-operation-not-permitted = cannot access {$path}: Operation not permitted
ls-error-cannot-open-directory-permission-denied = cannot open directory {$path}: Permission denied
ls-error-cannot-open-file-permission-denied = cannot open file {$path}: Permission denied
ls-error-cannot-open-directory-bad-descriptor = cannot open directory {$path}: Bad file descriptor
ls-error-unknown-io-error = unknown io error: {$path}, '{$error}'
ls-error-invalid-block-size = invalid --block-size argument {$size}
ls-error-dired-and-zero-incompatible = --dired and --zero are incompatible
ls-error-not-directory = cannot access {$path}: Not a directory
ls-error-not-listing-already-listed = {$path}: not listing already-listed directory
ls-error-invalid-time-style = invalid --time-style argument {$style}
  Possible values are:
    - [posix-]full-iso
    - [posix-]long-iso
    - [posix-]iso
    - [posix-]locale
    - +FORMAT (e.g., +%H:%M) for a 'date'-style format

  For more information try --help

# Help messages
ls-help-print-help = Print help information.
ls-help-set-display-format = Set the display format.
ls-help-display-files-columns = Display the files in columns.
ls-help-display-detailed-info = Display detailed information.
ls-help-list-entries-rows = List entries in rows instead of in columns.
ls-help-assume-tab-stops = Assume tab stops at each COLS instead of 8
ls-help-list-entries-commas = List entries separated by commas.
ls-help-list-entries-nul = List entries separated by ASCII NUL characters.
ls-help-generate-dired-output = generate output designed for Emacs' dired (Directory Editor) mode
ls-help-hyperlink-filenames = hyperlink file names WHEN
ls-help-list-one-file-per-line = List one file per line.
ls-help-long-format-no-group = Long format without group information.
  Identical to --format=long with --no-group.
ls-help-long-no-owner = Long format without owner information.
ls-help-long-numeric-uid-gid = -l with numeric UIDs and GIDs.
ls-help-set-quoting-style = Set quoting style.
ls-help-literal-quoting-style = Use literal quoting style. Equivalent to `--quoting-style=literal`
ls-help-escape-quoting-style = Use escape quoting style. Equivalent to `--quoting-style=escape`
ls-help-c-quoting-style = Use C quoting style. Equivalent to `--quoting-style=c`
ls-help-replace-control-chars = Replace control characters with '?' if they are not escaped.
ls-help-show-control-chars = Show control characters 'as is' if they are not escaped.
ls-help-show-time-field = Show time in `<field>`:
    access time (-u): atime, access, use;
    change time (-t): ctime, status.
    modification time: mtime, modification.
    birth time: birth, creation;
ls-help-time-change = If the long listing format (e.g., -l, -o) is being used, print the
  status change time (the 'ctime' in the inode) instead of the modification
  time. When explicitly sorting by time (--sort=time or -t) or when not
  using a long listing format, sort according to the status change time.
ls-help-time-access = If the long listing format (e.g., -l, -o) is being used, print the
  status access time instead of the modification time. When explicitly
  sorting by time (--sort=time or -t) or when not using a long listing
  format, sort according to the access time.
ls-help-hide-pattern = do not list implied entries matching shell PATTERN (overridden by -a or -A)
ls-help-ignore-pattern = do not list implied entries matching shell PATTERN
ls-help-ignore-backups = Ignore entries which end with ~.
ls-help-sort-by-field = Sort by `<field>`: name, none (-U), time (-t), size (-S), extension (-X) or width
ls-help-sort-by-size = Sort by file size, largest first.
ls-help-sort-by-time = Sort by modification time (the 'mtime' in the inode), newest first.
ls-help-sort-by-version = Natural sort of (version) numbers in the filenames.
ls-help-sort-by-extension = Sort alphabetically by entry extension.
ls-help-sort-none = Do not sort; list the files in whatever order they are stored in the
  directory.  This is especially useful when listing very large directories,
  since not doing any sorting can be noticeably faster.
ls-help-dereference-all = When showing file information for a symbolic link, show information for the
  file the link references rather than the link itself.
ls-help-dereference-dir-args = Do not follow symlinks except when they link to directories and are
  given as command line arguments.
ls-help-dereference-args = Do not follow symlinks except when given as command line arguments.
ls-help-no-group = Do not show group in long format.
ls-help-author = Show author in long format. On the supported platforms,
  the author always matches the file owner.
ls-help-all-files = Do not ignore hidden files (files with names that start with '.').
ls-help-almost-all = In a directory, do not ignore all file names that start with '.',
  only ignore '.' and '..'.
ls-help-unsorted-all = List all files in directory order, unsorted. Equivalent to -aU. Disables --color unless explicitly specified.
ls-help-directory = Only list the names of directories, rather than listing directory contents.
  This will not follow symbolic links unless one of `--dereference-command-line
  (-H)`, `--dereference (-L)`, or `--dereference-command-line-symlink-to-dir` is
  specified.
ls-help-human-readable = Print human readable file sizes (e.g. 1K 234M 56G).
ls-help-kibibytes = default to 1024-byte blocks for file system usage; used only with -s and per
  directory totals
ls-help-si = Print human readable file sizes using powers of 1000 instead of 1024.
ls-help-block-size = scale sizes by BLOCK_SIZE when printing them
ls-help-print-inode = print the index number of each file
ls-help-reverse-sort = Reverse whatever the sorting method is e.g., list files in reverse
  alphabetical order, youngest first, smallest first, or whatever.
ls-help-recursive = List the contents of all directories recursively.
ls-help-terminal-width = Assume that the terminal is COLS columns wide.
ls-help-allocation-size = print the allocated size of each file, in blocks
ls-help-color-output = Color output based on file type.
ls-help-indicator-style = Append indicator with style WORD to entry names:
  none (default),  slash (-p), file-type (--file-type), classify (-F)
ls-help-classify = Append a character to each file name indicating the file type. Also, for
  regular files that are executable, append '*'. The file type indicators are
  '/' for directories, '@' for symbolic links, '|' for FIFOs, '=' for sockets,
  '>' for doors, and nothing for regular files. when may be omitted, or one of:
      none - Do not classify. This is the default.
      auto - Only classify if standard output is a terminal.
      always - Always classify.
  Specifying --classify and no when is equivalent to --classify=always. This will
  not follow symbolic links listed on the command line unless the
  --dereference-command-line (-H), --dereference (-L), or
  --dereference-command-line-symlink-to-dir options are specified.
ls-help-file-type = Same as --classify, but do not append '*'
ls-help-slash-directories = Append / indicator to directories.
ls-help-time-style = time/date format with -l; see TIME_STYLE below
ls-help-full-time = like -l --time-style=full-iso
ls-help-context = print any security context of each file
ls-help-group-directories-first = group directories before files; can be augmented with
  a --sort option, but any use of --sort=none (-U) disables grouping
ls-invalid-quoting-style = {$program}: Ignoring invalid value of environment variable QUOTING_STYLE: '{$style}'
ls-invalid-columns-width = ignoring invalid width in environment variable COLUMNS: {$width}
ls-invalid-ignore-pattern = Invalid pattern for ignore: {$pattern}
ls-invalid-hide-pattern = Invalid pattern for hide: {$pattern}
ls-warning-unrecognized-ls-colors-prefix = unrecognized prefix: {$prefix}
ls-warning-unparsable-ls-colors = unparsable value for LS_COLORS environment variable
ls-total = total {$size}

# Security context warnings
ls-warning-failed-to-get-security-context = failed to get security context of: {$path}
ls-warning-getting-security-context = getting security context of: {$path}: {$error}

# SMACK error messages (used by uucore::smack when called from ls)
smack-error-not-enabled = SMACK is not enabled on this system
smack-error-label-retrieval-failure = failed to get SMACK label: { $error }
smack-error-label-set-failure = failed to set SMACK label to '{ $context }': { $error }
smack-error-no-label-set = no SMACK label set
"),
        // Locale for ls (en-US)
        "ls/en-US.ftl" => Some(r"ls-about = List directory contents.
  Ignore files and directories starting with a '.' by default
dir-about = List directory contents.
  Ignore files and directories starting with a '.' by default
vdir-about = List directory contents.
  Ignore files and directories starting with a '.' by default

  Mandatory arguments to long options are mandatory for short options too.
ls-usage = ls [OPTION]... [FILE]...
dir-usage = dir [OPTION]... [FILE]...
vdir-usage = vdir [OPTION]... [FILE]...
ls-after-help = The TIME_STYLE argument can be full-iso, long-iso, iso, locale or +FORMAT. FORMAT is interpreted like in date. Also the TIME_STYLE environment variable sets the default style to use.

# Error messages
ls-error-invalid-line-width = invalid line width: {$width}
ls-error-general-io = general io error: {$error}
ls-error-cannot-access-no-such-file = cannot access {$path}: No such file or directory
ls-error-cannot-access-operation-not-permitted = cannot access {$path}: Operation not permitted
ls-error-cannot-open-directory-permission-denied = cannot open directory {$path}: Permission denied
ls-error-cannot-open-file-permission-denied = cannot open file {$path}: Permission denied
ls-error-cannot-open-directory-bad-descriptor = cannot open directory {$path}: Bad file descriptor
ls-error-unknown-io-error = unknown io error: {$path}, '{$error}'
ls-error-invalid-block-size = invalid --block-size argument {$size}
ls-error-dired-and-zero-incompatible = --dired and --zero are incompatible
ls-error-not-directory = cannot access {$path}: Not a directory
ls-error-not-listing-already-listed = {$path}: not listing already-listed directory
ls-error-invalid-time-style = invalid --time-style argument {$style}
  Possible values are:
    - [posix-]full-iso
    - [posix-]long-iso
    - [posix-]iso
    - [posix-]locale
    - +FORMAT (e.g., +%H:%M) for a 'date'-style format

  For more information try --help

# Help messages
ls-help-print-help = Print help information.
ls-help-set-display-format = Set the display format.
ls-help-display-files-columns = Display the files in columns.
ls-help-display-detailed-info = Display detailed information.
ls-help-list-entries-rows = List entries in rows instead of in columns.
ls-help-assume-tab-stops = Assume tab stops at each COLS instead of 8
ls-help-list-entries-commas = List entries separated by commas.
ls-help-list-entries-nul = List entries separated by ASCII NUL characters.
ls-help-generate-dired-output = generate output designed for Emacs' dired (Directory Editor) mode
ls-help-hyperlink-filenames = hyperlink file names WHEN
ls-help-list-one-file-per-line = List one file per line.
ls-help-long-format-no-group = Long format without group information.
  Identical to --format=long with --no-group.
ls-help-long-no-owner = Long format without owner information.
ls-help-long-numeric-uid-gid = -l with numeric UIDs and GIDs.
ls-help-set-quoting-style = Set quoting style.
ls-help-literal-quoting-style = Use literal quoting style. Equivalent to `--quoting-style=literal`
ls-help-escape-quoting-style = Use escape quoting style. Equivalent to `--quoting-style=escape`
ls-help-c-quoting-style = Use C quoting style. Equivalent to `--quoting-style=c`
ls-help-replace-control-chars = Replace control characters with '?' if they are not escaped.
ls-help-show-control-chars = Show control characters 'as is' if they are not escaped.
ls-help-show-time-field = Show time in `<field>`:
    access time (-u): atime, access, use;
    change time (-t): ctime, status.
    modification time: mtime, modification.
    birth time: birth, creation;
ls-help-time-change = If the long listing format (e.g., -l, -o) is being used, print the
  status change time (the 'ctime' in the inode) instead of the modification
  time. When explicitly sorting by time (--sort=time or -t) or when not
  using a long listing format, sort according to the status change time.
ls-help-time-access = If the long listing format (e.g., -l, -o) is being used, print the
  status access time instead of the modification time. When explicitly
  sorting by time (--sort=time or -t) or when not using a long listing
  format, sort according to the access time.
ls-help-hide-pattern = do not list implied entries matching shell PATTERN (overridden by -a or -A)
ls-help-ignore-pattern = do not list implied entries matching shell PATTERN
ls-help-ignore-backups = Ignore entries which end with ~.
ls-help-sort-by-field = Sort by `<field>`: name, none (-U), time (-t), size (-S), extension (-X) or width
ls-help-sort-by-size = Sort by file size, largest first.
ls-help-sort-by-time = Sort by modification time (the 'mtime' in the inode), newest first.
ls-help-sort-by-version = Natural sort of (version) numbers in the filenames.
ls-help-sort-by-extension = Sort alphabetically by entry extension.
ls-help-sort-none = Do not sort; list the files in whatever order they are stored in the
  directory.  This is especially useful when listing very large directories,
  since not doing any sorting can be noticeably faster.
ls-help-dereference-all = When showing file information for a symbolic link, show information for the
  file the link references rather than the link itself.
ls-help-dereference-dir-args = Do not follow symlinks except when they link to directories and are
  given as command line arguments.
ls-help-dereference-args = Do not follow symlinks except when given as command line arguments.
ls-help-no-group = Do not show group in long format.
ls-help-author = Show author in long format. On the supported platforms,
  the author always matches the file owner.
ls-help-all-files = Do not ignore hidden files (files with names that start with '.').
ls-help-almost-all = In a directory, do not ignore all file names that start with '.',
  only ignore '.' and '..'.
ls-help-unsorted-all = List all files in directory order, unsorted. Equivalent to -aU. Disables --color unless explicitly specified.
ls-help-directory = Only list the names of directories, rather than listing directory contents.
  This will not follow symbolic links unless one of `--dereference-command-line
  (-H)`, `--dereference (-L)`, or `--dereference-command-line-symlink-to-dir` is
  specified.
ls-help-human-readable = Print human readable file sizes (e.g. 1K 234M 56G).
ls-help-kibibytes = default to 1024-byte blocks for file system usage; used only with -s and per
  directory totals
ls-help-si = Print human readable file sizes using powers of 1000 instead of 1024.
ls-help-block-size = scale sizes by BLOCK_SIZE when printing them
ls-help-print-inode = print the index number of each file
ls-help-reverse-sort = Reverse whatever the sorting method is e.g., list files in reverse
  alphabetical order, youngest first, smallest first, or whatever.
ls-help-recursive = List the contents of all directories recursively.
ls-help-terminal-width = Assume that the terminal is COLS columns wide.
ls-help-allocation-size = print the allocated size of each file, in blocks
ls-help-color-output = Color output based on file type.
ls-help-indicator-style = Append indicator with style WORD to entry names:
  none (default),  slash (-p), file-type (--file-type), classify (-F)
ls-help-classify = Append a character to each file name indicating the file type. Also, for
  regular files that are executable, append '*'. The file type indicators are
  '/' for directories, '@' for symbolic links, '|' for FIFOs, '=' for sockets,
  '>' for doors, and nothing for regular files. when may be omitted, or one of:
      none - Do not classify. This is the default.
      auto - Only classify if standard output is a terminal.
      always - Always classify.
  Specifying --classify and no when is equivalent to --classify=always. This will
  not follow symbolic links listed on the command line unless the
  --dereference-command-line (-H), --dereference (-L), or
  --dereference-command-line-symlink-to-dir options are specified.
ls-help-file-type = Same as --classify, but do not append '*'
ls-help-slash-directories = Append / indicator to directories.
ls-help-time-style = time/date format with -l; see TIME_STYLE below
ls-help-full-time = like -l --time-style=full-iso
ls-help-context = print any security context of each file
ls-help-group-directories-first = group directories before files; can be augmented with
  a --sort option, but any use of --sort=none (-U) disables grouping
ls-invalid-quoting-style = {$program}: Ignoring invalid value of environment variable QUOTING_STYLE: '{$style}'
ls-invalid-columns-width = ignoring invalid width in environment variable COLUMNS: {$width}
ls-invalid-ignore-pattern = Invalid pattern for ignore: {$pattern}
ls-invalid-hide-pattern = Invalid pattern for hide: {$pattern}
ls-warning-unrecognized-ls-colors-prefix = unrecognized prefix: {$prefix}
ls-warning-unparsable-ls-colors = unparsable value for LS_COLORS environment variable
ls-total = total {$size}

# Security context warnings
ls-warning-failed-to-get-security-context = failed to get security context of: {$path}
ls-warning-getting-security-context = getting security context of: {$path}: {$error}

# SMACK error messages (used by uucore::smack when called from ls)
smack-error-not-enabled = SMACK is not enabled on this system
smack-error-label-retrieval-failure = failed to get SMACK label: { $error }
smack-error-label-set-failure = failed to set SMACK label to '{ $context }': { $error }
smack-error-no-label-set = no SMACK label set
"),
        // Locale for md5sum (en-US)
        "md5sum/en-US.ftl" => Some(r"md5sum-about = Print or check the MD5 checksums
md5sum-usage = md5sum [OPTIONS] [FILE]...
"),
        // Locale for md5sum (en-US)
        "md5sum/en-US.ftl" => Some(r"md5sum-about = Print or check the MD5 checksums
md5sum-usage = md5sum [OPTIONS] [FILE]...
"),
        // Locale for mkdir (en-US)
        "mkdir/en-US.ftl" => Some(r"mkdir-about = Create the given DIRECTORY(ies) if they do not exist
mkdir-usage = mkdir [OPTION]... DIRECTORY...
mkdir-after-help = Each MODE is of the form [ugoa]*([-+=]([rwxXst]*|[ugo]))+|[-+=]?[0-7]+.

# Help messages
mkdir-help-mode = set file mode (not implemented on windows)
mkdir-help-parents = make parent directories as needed
mkdir-help-verbose = print a message for each printed directory
mkdir-help-selinux = set SELinux security context of each created directory to the default type
mkdir-help-context = like -Z, or if CTX is specified then set the SELinux or SMACK security context to CTX

# Error messages
mkdir-error-empty-directory-name = cannot create directory '': No such file or directory
mkdir-error-file-exists = { $path }: File exists
mkdir-error-failed-to-create-tree = failed to create whole tree
mkdir-error-cannot-set-permissions = cannot set permissions { $path }

# Verbose output
mkdir-verbose-created-directory = { $util_name }: created directory { $path }
"),
        // Locale for mkdir (en-US)
        "mkdir/en-US.ftl" => Some(r"mkdir-about = Create the given DIRECTORY(ies) if they do not exist
mkdir-usage = mkdir [OPTION]... DIRECTORY...
mkdir-after-help = Each MODE is of the form [ugoa]*([-+=]([rwxXst]*|[ugo]))+|[-+=]?[0-7]+.

# Help messages
mkdir-help-mode = set file mode (not implemented on windows)
mkdir-help-parents = make parent directories as needed
mkdir-help-verbose = print a message for each printed directory
mkdir-help-selinux = set SELinux security context of each created directory to the default type
mkdir-help-context = like -Z, or if CTX is specified then set the SELinux or SMACK security context to CTX

# Error messages
mkdir-error-empty-directory-name = cannot create directory '': No such file or directory
mkdir-error-file-exists = { $path }: File exists
mkdir-error-failed-to-create-tree = failed to create whole tree
mkdir-error-cannot-set-permissions = cannot set permissions { $path }

# Verbose output
mkdir-verbose-created-directory = { $util_name }: created directory { $path }
"),
        // Locale for mktemp (en-US)
        "mktemp/en-US.ftl" => Some(r"mktemp-about = Create a temporary file or directory.
mktemp-usage = mktemp [OPTION]... [TEMPLATE]

# Help messages
mktemp-help-directory = Make a directory instead of a file
mktemp-help-dry-run = do not create anything; merely print a name (unsafe)
mktemp-help-quiet = Fail silently if an error occurs.
mktemp-help-suffix = append SUFFIX to TEMPLATE; SUFFIX must not contain a path separator. This option is implied if TEMPLATE does not end with X.
mktemp-help-p = short form of --tmpdir
mktemp-help-tmpdir = interpret TEMPLATE relative to DIR; if DIR is not specified, use $TMPDIR ($TMP on windows) if set, else /tmp. With this option, TEMPLATE must not be an absolute name; unlike with -t, TEMPLATE may contain slashes, but mktemp creates only the final component
mktemp-help-t = Generate a template (using the supplied prefix and TMPDIR (TMP on windows) if set) to create a filename template [deprecated]

# Error messages
mktemp-error-persist-file = could not persist file { $path }
mktemp-error-must-end-in-x = with --suffix, template { $template } must end in X
mktemp-error-too-few-xs = too few X's in template { $template }
mktemp-error-prefix-contains-separator = invalid template, { $template }, contains directory separator
mktemp-error-suffix-contains-separator = invalid suffix { $suffix }, contains directory separator
mktemp-error-invalid-template = invalid template, { $template }; with --tmpdir, it may not be absolute
mktemp-error-too-many-templates = too many templates
mktemp-error-not-found = failed to create { $template_type } via template { $template }: No such file or directory
mktemp-error-failed-print = failed to print directory name

# Template types
mktemp-template-type-directory = directory
mktemp-template-type-file = file
"),
        // Locale for mktemp (en-US)
        "mktemp/en-US.ftl" => Some(r"mktemp-about = Create a temporary file or directory.
mktemp-usage = mktemp [OPTION]... [TEMPLATE]

# Help messages
mktemp-help-directory = Make a directory instead of a file
mktemp-help-dry-run = do not create anything; merely print a name (unsafe)
mktemp-help-quiet = Fail silently if an error occurs.
mktemp-help-suffix = append SUFFIX to TEMPLATE; SUFFIX must not contain a path separator. This option is implied if TEMPLATE does not end with X.
mktemp-help-p = short form of --tmpdir
mktemp-help-tmpdir = interpret TEMPLATE relative to DIR; if DIR is not specified, use $TMPDIR ($TMP on windows) if set, else /tmp. With this option, TEMPLATE must not be an absolute name; unlike with -t, TEMPLATE may contain slashes, but mktemp creates only the final component
mktemp-help-t = Generate a template (using the supplied prefix and TMPDIR (TMP on windows) if set) to create a filename template [deprecated]

# Error messages
mktemp-error-persist-file = could not persist file { $path }
mktemp-error-must-end-in-x = with --suffix, template { $template } must end in X
mktemp-error-too-few-xs = too few X's in template { $template }
mktemp-error-prefix-contains-separator = invalid template, { $template }, contains directory separator
mktemp-error-suffix-contains-separator = invalid suffix { $suffix }, contains directory separator
mktemp-error-invalid-template = invalid template, { $template }; with --tmpdir, it may not be absolute
mktemp-error-too-many-templates = too many templates
mktemp-error-not-found = failed to create { $template_type } via template { $template }: No such file or directory
mktemp-error-failed-print = failed to print directory name

# Template types
mktemp-template-type-directory = directory
mktemp-template-type-file = file
"),
        // Locale for more (en-US)
        "more/en-US.ftl" => Some(r"more-about = Display the contents of a text file
more-usage = more [OPTIONS] FILE...

# Error messages
more-error-is-directory = {$path} is a directory.
more-error-cannot-open-no-such-file = cannot open {$path}: No such file or directory
more-error-cannot-open-io-error = cannot open {$path}: {$error}
more-error-bad-usage = bad usage
more-error-cannot-seek-to-line = Cannot seek to line number {$line}
more-error-pattern-not-found = Pattern not found
more-error-unknown-key = Unknown key: '{$key}'. Press 'h' for instructions. (unimplemented)

# Help messages
more-help-silent = Display help instead of ringing bell when an illegal key is pressed
more-help-logical = Do not pause after any line containing a ^L (form feed)
more-help-exit-on-eof = Exit on End-Of-File
more-help-no-pause = Count logical lines, rather than screen lines
more-help-print-over = Do not scroll, clear screen and display text
more-help-clean-print = Do not scroll, display text and clean line ends
more-help-squeeze = Squeeze multiple blank lines into one
more-help-plain = Suppress underlining
more-help-lines = The number of lines per screen full
more-help-number = Same as --lines option argument
more-help-from-line = Start displaying each file at line number
more-help-pattern = The string to be searched in each file before starting to display it
more-help-files = Path to the files to be read

# Other messages
more-help-message = [Press space to continue, 'q' to quit.]
more-press-return = press RETURN
"),
        // Locale for more (en-US)
        "more/en-US.ftl" => Some(r"more-about = Display the contents of a text file
more-usage = more [OPTIONS] FILE...

# Error messages
more-error-is-directory = {$path} is a directory.
more-error-cannot-open-no-such-file = cannot open {$path}: No such file or directory
more-error-cannot-open-io-error = cannot open {$path}: {$error}
more-error-bad-usage = bad usage
more-error-cannot-seek-to-line = Cannot seek to line number {$line}
more-error-pattern-not-found = Pattern not found
more-error-unknown-key = Unknown key: '{$key}'. Press 'h' for instructions. (unimplemented)

# Help messages
more-help-silent = Display help instead of ringing bell when an illegal key is pressed
more-help-logical = Do not pause after any line containing a ^L (form feed)
more-help-exit-on-eof = Exit on End-Of-File
more-help-no-pause = Count logical lines, rather than screen lines
more-help-print-over = Do not scroll, clear screen and display text
more-help-clean-print = Do not scroll, display text and clean line ends
more-help-squeeze = Squeeze multiple blank lines into one
more-help-plain = Suppress underlining
more-help-lines = The number of lines per screen full
more-help-number = Same as --lines option argument
more-help-from-line = Start displaying each file at line number
more-help-pattern = The string to be searched in each file before starting to display it
more-help-files = Path to the files to be read

# Other messages
more-help-message = [Press space to continue, 'q' to quit.]
more-press-return = press RETURN
"),
        // Locale for mv (en-US)
        "mv/en-US.ftl" => Some(r"mv-about = Move SOURCE to DEST, or multiple SOURCE(s) to DIRECTORY.
mv-usage = mv [OPTION]... [-T] SOURCE DEST
  mv [OPTION]... SOURCE... DIRECTORY
  mv [OPTION]... -t DIRECTORY SOURCE...
mv-after-help = When specifying more than one of -i, -f, -n, only the final one will take effect.

  Do not move a non-directory that has an existing destination with the same or newer modification timestamp;
  instead, silently skip the file without failing. If the move is across file system boundaries, the comparison is
  to the source timestamp truncated to the resolutions of the destination file system and of the system calls used
  to update timestamps; this avoids duplicate work if several mv -u commands are executed with the same source
  and destination. This option is ignored if the -n or --no-clobber option is also specified. which gives more control
  over which existing files in the destination are replaced, and its value can be one of the following:

  - all This is the default operation when an --update option is not specified, and results in all existing files in the destination being replaced.
  - none This is similar to the --no-clobber option, in that no files in the destination are replaced, but also skipping a file does not induce a failure.
  - older This is the default operation when --update is specified, and results in files being replaced if they’re older than the corresponding source file.

# Error messages
mv-error-insufficient-arguments = The argument '<{$arg_files}>...' requires at least 2 values, but only 1 was provided
mv-error-no-such-file = cannot stat {$path}: No such file or directory
mv-error-cannot-stat-not-directory = cannot stat {$path}: Not a directory
mv-error-same-file = {$source} and {$target} are the same file
mv-error-self-target-subdirectory = cannot move {$source} to a subdirectory of itself, {$target}
mv-error-directory-to-non-directory = cannot overwrite directory {$path} with non-directory
mv-error-non-directory-to-directory = cannot overwrite non-directory {$target} with directory {$source}
mv-error-not-directory = target {$path}: Not a directory
mv-error-target-not-directory = target directory {$path}: Not a directory
mv-error-failed-access-not-directory = failed to access {$path}: Not a directory
mv-error-backup-with-no-clobber = cannot combine --backup with -n/--no-clobber or --update=none-fail
mv-error-extra-operand = mv: extra operand {$operand}
mv-error-backup-might-destroy-source = backing up {$target} might destroy source;  {$source} not moved
mv-error-will-not-overwrite-just-created = will not overwrite just-created {$target} with {$source}
mv-error-not-replacing = not replacing {$target}
mv-error-cannot-move = cannot move {$source} to {$target}
mv-error-directory-not-empty = Directory not empty
mv-error-dangling-symlink = can't determine symlink type, since it is dangling
mv-error-no-symlink-support = your operating system does not support symlinks
mv-error-permission-denied = Permission denied
mv-error-inter-device-move-failed = inter-device move failed: {$from} to {$to}; unable to remove target: {$err}
mv-error-exchange-two-operands = --exchange requires exactly two operands
mv-error-exchange-not-supported = --exchange is not supported on this platform

# Help messages
mv-help-force = do not prompt before overwriting
mv-help-interactive = prompt before override
mv-help-no-clobber = do not overwrite an existing file
mv-help-strip-trailing-slashes = remove any trailing slashes from each SOURCE argument
mv-help-target-directory = move all SOURCE arguments into DIRECTORY
mv-help-no-target-directory = treat DEST as a normal file
mv-help-verbose = explain what is being done
mv-help-progress = Display a progress bar.
  Note: this feature is not supported by GNU coreutils.
mv-help-debug = explain how a file is copied. Implies -v
mv-help-exchange = exchange source and destination (atomically swap them)
mv-help-selinux = set SELinux security context of destination file to default type
mv-help-context = like -Z, or if CTX is specified then set the SELinux security context to CTX

# Verbose messages
mv-verbose-renamed = renamed {$from} -> {$to}
mv-verbose-renamed-with-backup = renamed {$from} -> {$to} (backup: {$backup})
mv-verbose-exchanged = exchanged {$from} <-> {$to}

# Debug messages
mv-debug-skipped = skipped {$target}

# Prompt messages
mv-prompt-overwrite = overwrite {$target}?
mv-prompt-overwrite-mode = replace {$target}, overriding mode {$mode_info}?

# Progress messages
mv-progress-moving = moving
mv-error-dest-appeared = destination { $path } was replaced while moving; refusing to follow it
"),
        // Locale for mv (en-US)
        "mv/en-US.ftl" => Some(r"mv-about = Move SOURCE to DEST, or multiple SOURCE(s) to DIRECTORY.
mv-usage = mv [OPTION]... [-T] SOURCE DEST
  mv [OPTION]... SOURCE... DIRECTORY
  mv [OPTION]... -t DIRECTORY SOURCE...
mv-after-help = When specifying more than one of -i, -f, -n, only the final one will take effect.

  Do not move a non-directory that has an existing destination with the same or newer modification timestamp;
  instead, silently skip the file without failing. If the move is across file system boundaries, the comparison is
  to the source timestamp truncated to the resolutions of the destination file system and of the system calls used
  to update timestamps; this avoids duplicate work if several mv -u commands are executed with the same source
  and destination. This option is ignored if the -n or --no-clobber option is also specified. which gives more control
  over which existing files in the destination are replaced, and its value can be one of the following:

  - all This is the default operation when an --update option is not specified, and results in all existing files in the destination being replaced.
  - none This is similar to the --no-clobber option, in that no files in the destination are replaced, but also skipping a file does not induce a failure.
  - older This is the default operation when --update is specified, and results in files being replaced if they’re older than the corresponding source file.

# Error messages
mv-error-insufficient-arguments = The argument '<{$arg_files}>...' requires at least 2 values, but only 1 was provided
mv-error-no-such-file = cannot stat {$path}: No such file or directory
mv-error-cannot-stat-not-directory = cannot stat {$path}: Not a directory
mv-error-same-file = {$source} and {$target} are the same file
mv-error-self-target-subdirectory = cannot move {$source} to a subdirectory of itself, {$target}
mv-error-directory-to-non-directory = cannot overwrite directory {$path} with non-directory
mv-error-non-directory-to-directory = cannot overwrite non-directory {$target} with directory {$source}
mv-error-not-directory = target {$path}: Not a directory
mv-error-target-not-directory = target directory {$path}: Not a directory
mv-error-failed-access-not-directory = failed to access {$path}: Not a directory
mv-error-backup-with-no-clobber = cannot combine --backup with -n/--no-clobber or --update=none-fail
mv-error-extra-operand = mv: extra operand {$operand}
mv-error-backup-might-destroy-source = backing up {$target} might destroy source;  {$source} not moved
mv-error-will-not-overwrite-just-created = will not overwrite just-created {$target} with {$source}
mv-error-not-replacing = not replacing {$target}
mv-error-cannot-move = cannot move {$source} to {$target}
mv-error-directory-not-empty = Directory not empty
mv-error-dangling-symlink = can't determine symlink type, since it is dangling
mv-error-no-symlink-support = your operating system does not support symlinks
mv-error-permission-denied = Permission denied
mv-error-inter-device-move-failed = inter-device move failed: {$from} to {$to}; unable to remove target: {$err}

# Help messages
mv-help-force = do not prompt before overwriting
mv-help-interactive = prompt before override
mv-help-no-clobber = do not overwrite an existing file
mv-help-strip-trailing-slashes = remove any trailing slashes from each SOURCE argument
mv-help-target-directory = move all SOURCE arguments into DIRECTORY
mv-help-no-target-directory = treat DEST as a normal file
mv-help-verbose = explain what is being done
mv-help-progress = Display a progress bar.
  Note: this feature is not supported by GNU coreutils.
mv-help-debug = explain how a file is copied. Implies -v
mv-help-selinux = set SELinux security context of destination file to default type
mv-help-context = like -Z, or if CTX is specified then set the SELinux security context to CTX

# Verbose messages
mv-verbose-renamed = renamed {$from} -> {$to}
mv-verbose-renamed-with-backup = renamed {$from} -> {$to} (backup: {$backup})

# Debug messages
mv-debug-skipped = skipped {$target}

# Prompt messages
mv-prompt-overwrite = overwrite {$target}?
mv-prompt-overwrite-mode = replace {$target}, overriding mode {$mode_info}?

# Progress messages
mv-progress-moving = moving
"),
        // Locale for nl (en-US)
        "nl/en-US.ftl" => Some(r"nl-about = Number lines of files
nl-usage = nl [OPTION]... [FILE]...
nl-after-help = STYLE is one of:

  - a number all lines
  - t number only nonempty lines
  - n number no lines
  - pBRE number only lines that contain a match for the basic regular
          expression, BRE

  FORMAT is one of:

  - ln left justified, no leading zeros
  - rn right justified, no leading zeros
  - rz right justified, leading zeros

# Help messages
nl-help-help = Print help information.
nl-help-body-numbering = use STYLE for numbering body lines
nl-help-section-delimiter = use CC for separating logical pages
nl-help-footer-numbering = use STYLE for numbering footer lines
nl-help-header-numbering = use STYLE for numbering header lines
nl-help-line-increment = line number increment at each line
nl-help-join-blank-lines = group of NUMBER empty lines counted as one
nl-help-number-format = insert line numbers according to FORMAT
nl-help-no-renumber = do not reset line numbers at logical pages
nl-help-number-separator = add STRING after (possible) line number
nl-help-starting-line-number = first line number on each logical page
nl-help-number-width = use NUMBER columns for line numbers

# Error messages
nl-error-invalid-arguments = Invalid arguments supplied.
nl-error-could-not-read-line = could not read line
nl-error-could-not-write = could not write output
nl-error-line-number-overflow = line number overflow
nl-error-invalid-line-width = Invalid line number field width: ‘{ $value }’: Numerical result out of range
nl-error-invalid-regex = invalid regular expression
nl-error-invalid-numbering-style = invalid numbering style: '{ $style }'
nl-error-is-directory = { $path }: Is a directory
"),
        // Locale for nl (en-US)
        "nl/en-US.ftl" => Some(r"nl-about = Number lines of files
nl-usage = nl [OPTION]... [FILE]...
nl-after-help = STYLE is one of:

  - a number all lines
  - t number only nonempty lines
  - n number no lines
  - pBRE number only lines that contain a match for the basic regular
          expression, BRE

  FORMAT is one of:

  - ln left justified, no leading zeros
  - rn right justified, no leading zeros
  - rz right justified, leading zeros

# Help messages
nl-help-help = Print help information.
nl-help-body-numbering = use STYLE for numbering body lines
nl-help-section-delimiter = use CC for separating logical pages
nl-help-footer-numbering = use STYLE for numbering footer lines
nl-help-header-numbering = use STYLE for numbering header lines
nl-help-line-increment = line number increment at each line
nl-help-join-blank-lines = group of NUMBER empty lines counted as one
nl-help-number-format = insert line numbers according to FORMAT
nl-help-no-renumber = do not reset line numbers at logical pages
nl-help-number-separator = add STRING after (possible) line number
nl-help-starting-line-number = first line number on each logical page
nl-help-number-width = use NUMBER columns for line numbers

# Error messages
nl-error-invalid-arguments = Invalid arguments supplied.
nl-error-could-not-read-line = could not read line
nl-error-could-not-write = could not write output
nl-error-line-number-overflow = line number overflow
nl-error-invalid-line-width = Invalid line number field width: ‘{ $value }’: Numerical result out of range
nl-error-invalid-regex = invalid regular expression
nl-error-invalid-numbering-style = invalid numbering style: '{ $style }'
nl-error-is-directory = { $path }: Is a directory
"),
        // Locale for nproc (en-US)
        "nproc/en-US.ftl" => Some(r"nproc-about = Print the number of cores available to the current process.
  If the OMP_NUM_THREADS or OMP_THREAD_LIMIT environment variables are set, then
  they will determine the minimum and maximum returned value respectively.
nproc-usage = nproc [OPTIONS]...

# Help text for command-line arguments
nproc-help-all = print the number of cores available to the system
nproc-help-ignore = ignore up to N cores
"),
        // Locale for nproc (en-US)
        "nproc/en-US.ftl" => Some(r"nproc-about = Print the number of cores available to the current process.
  If the OMP_NUM_THREADS or OMP_THREAD_LIMIT environment variables are set, then
  they will determine the minimum and maximum returned value respectively.
nproc-usage = nproc [OPTIONS]...

# Error messages
nproc-error-invalid-number = { $value } is not a valid number: { $error }

# Help text for command-line arguments
nproc-help-all = print the number of cores available to the system
nproc-help-ignore = ignore up to N cores
"),
        // Locale for numfmt (en-US)
        "numfmt/en-US.ftl" => Some(r"numfmt-about = Convert numbers from/to human-readable strings
numfmt-usage = numfmt [OPTION]... [NUMBER]...
numfmt-after-help = UNIT options:

  - none: no auto-scaling is done; suffixes will trigger an error
  - auto: accept optional single/two letter suffix:

      1K = 1000, 1Ki = 1024, 1M = 1000000, 1Mi = 1048576,

  - si: accept optional single letter suffix:

      1K = 1000, 1M = 1000000, ...

  - iec: accept optional single letter suffix:

      1K = 1024, 1M = 1048576, ...

  - iec-i: accept optional two-letter suffix:

      1Ki = 1024, 1Mi = 1048576, ...

  - FIELDS supports cut(1) style field ranges:

      N N'th field, counted from 1
      N- from N'th field, to end of line
      N-M from N'th to M'th field (inclusive)
      -M from first to M'th field (inclusive)
      - all fields

  Multiple fields/ranges can be separated with commas

  FORMAT must be suitable for printing one floating-point argument %f.
  Optional quote (%'f) will enable --grouping (if supported by current locale).
  Optional width value (%10f) will pad output. Optional zero (%010f) width
  will zero pad the number. Optional negative values (%-10f) will left align.
  Optional precision (%.1f) will override the input determined precision.

# Help messages
numfmt-help-debug = print warnings about invalid input
numfmt-help-delimiter = use X instead of whitespace for field delimiter
numfmt-help-field = replace the numbers in these input fields; see FIELDS below
numfmt-help-format = use printf style floating-point FORMAT; see FORMAT below for details
numfmt-help-from = auto-scale input numbers to UNITs; see UNIT below
numfmt-help-from-unit = specify the input unit size
numfmt-help-grouping = use locale-defined grouping of digits, for example 1,000,000 (which means it has no effect in the C/POSIX locale)
numfmt-help-to = auto-scale output numbers to UNITs; see UNIT below
numfmt-help-to-unit = the output unit size
numfmt-help-padding = pad the output to N characters; positive N will right-align; negative N will left-align; padding is ignored if the output is wider than N; the default is to automatically pad if a whitespace is found
numfmt-help-header = print (without converting) the first N header lines; N defaults to 1 if not specified
numfmt-help-round = use METHOD for rounding when scaling
numfmt-help-suffix = print SUFFIX after each formatted number, and accept inputs optionally ending with SUFFIX
numfmt-help-unit-separator = use STRING to separate the number from any unit when printing; by default, no separator is used
numfmt-help-invalid = set the failure mode for invalid input
numfmt-help-zero-terminated = line delimiter is NUL, not newline

# Error messages
numfmt-error-unsupported-unit = Unsupported unit is specified
numfmt-error-invalid-unit-size = invalid unit size: { $size }
numfmt-error-invalid-padding = invalid padding value { $value }
numfmt-error-invalid-header = invalid header value { $value }
numfmt-error-grouping-cannot-be-combined-with-format = --grouping cannot be combined with --format
numfmt-error-grouping-cannot-be-combined-with-to = grouping cannot be combined with --to
numfmt-error-delimiter-must-be-single-character = the delimiter must be a single character
numfmt-error-invalid-number-empty = invalid number: ''
numfmt-error-invalid-specific-suffix = invalid suffix in input { $input }: { $suffix }
numfmt-error-invalid-suffix = invalid suffix in input: { $input }
numfmt-error-invalid-number = invalid number: { $input }
numfmt-error-missing-i-suffix = missing 'i' suffix in input: '{ $number }{ $suffix }' (e.g Ki/Mi/Gi)
numfmt-error-rejecting-suffix = rejecting suffix in input: '{ $number }{ $suffix }' (consider using --from)
numfmt-error-suffix-unsupported-for-unit = This suffix is unsupported for specified unit
numfmt-error-invalid-unit-argument = invalid argument '{$arg}' for '{$opt}'
numfmt-error-number-too-big = Number is too big and unsupported
numfmt-error-format-no-percent = format '{ $format }' has no % directive
numfmt-error-format-ends-in-percent = format '{ $format }' ends in %
numfmt-error-invalid-format-directive = invalid format '{ $format }', directive must be %[0]['][-][N][.][N]f
numfmt-error-invalid-format-width-overflow = invalid format '{ $format }' (width overflow)
numfmt-error-invalid-precision = invalid precision in format '{ $format }'
numfmt-error-format-too-many-percent = format '{ $format }' has too many % directives
numfmt-error-unknown-invalid-mode = Unknown invalid mode: { $mode }

# Debug messages
numfmt-debug-no-conversion = no conversion option specified
numfmt-debug-grouping-no-effect = grouping has no effect in this locale
numfmt-debug-failed-to-convert = failed to convert some of the input numbers
numfmt-debug-header-ignored = --header ignored with command-line input
"),
        // Locale for numfmt (en-US)
        "numfmt/en-US.ftl" => Some(r"numfmt-about = Convert numbers from/to human-readable strings
numfmt-usage = numfmt [OPTION]... [NUMBER]...
numfmt-after-help = UNIT options:

  - none: no auto-scaling is done; suffixes will trigger an error
  - auto: accept optional single/two letter suffix:

      1K = 1000, 1Ki = 1024, 1M = 1000000, 1Mi = 1048576,

  - si: accept optional single letter suffix:

      1K = 1000, 1M = 1000000, ...

  - iec: accept optional single letter suffix:

      1K = 1024, 1M = 1048576, ...

  - iec-i: accept optional two-letter suffix:

      1Ki = 1024, 1Mi = 1048576, ...

  - FIELDS supports cut(1) style field ranges:

      N N'th field, counted from 1
      N- from N'th field, to end of line
      N-M from N'th to M'th field (inclusive)
      -M from first to M'th field (inclusive)
      - all fields

  Multiple fields/ranges can be separated with commas

  FORMAT must be suitable for printing one floating-point argument %f.
  Optional quote (%'f) will enable --grouping (if supported by current locale).
  Optional width value (%10f) will pad output. Optional zero (%010f) width
  will zero pad the number. Optional negative values (%-10f) will left align.
  Optional precision (%.1f) will override the input determined precision.

# Help messages
numfmt-help-debug = print warnings about invalid input
numfmt-help-delimiter = use X instead of whitespace for field delimiter
numfmt-help-field = replace the numbers in these input fields; see FIELDS below
numfmt-help-format = use printf style floating-point FORMAT; see FORMAT below for details
numfmt-help-from = auto-scale input numbers to UNITs; see UNIT below
numfmt-help-from-unit = specify the input unit size
numfmt-help-grouping = use locale-defined grouping of digits, for example 1,000,000 (which means it has no effect in the C/POSIX locale)
numfmt-help-to = auto-scale output numbers to UNITs; see UNIT below
numfmt-help-to-unit = the output unit size
numfmt-help-padding = pad the output to N characters; positive N will right-align; negative N will left-align; padding is ignored if the output is wider than N; the default is to automatically pad if a whitespace is found
numfmt-help-header = print (without converting) the first N header lines; N defaults to 1 if not specified
numfmt-help-round = use METHOD for rounding when scaling
numfmt-help-suffix = print SUFFIX after each formatted number, and accept inputs optionally ending with SUFFIX
numfmt-help-unit-separator = use STRING to separate the number from any unit when printing; by default, no separator is used
numfmt-help-invalid = set the failure mode for invalid input
numfmt-help-zero-terminated = line delimiter is NUL, not newline

# Error messages
numfmt-error-unsupported-unit = Unsupported unit is specified
numfmt-error-invalid-unit-size = invalid unit size: { $size }
numfmt-error-invalid-padding = invalid padding value { $value }
numfmt-error-invalid-header = invalid header value { $value }
numfmt-error-grouping-cannot-be-combined-with-format = --grouping cannot be combined with --format
numfmt-error-grouping-cannot-be-combined-with-to = grouping cannot be combined with --to
numfmt-error-delimiter-must-be-single-character = the delimiter must be a single character
numfmt-error-invalid-number-empty = invalid number: ''
numfmt-error-invalid-specific-suffix = invalid suffix in input { $input }: { $suffix }
numfmt-error-invalid-suffix = invalid suffix in input: { $input }
numfmt-error-invalid-number = invalid number: { $input }
numfmt-error-missing-i-suffix = missing 'i' suffix in input: '{ $number }{ $suffix }' (e.g Ki/Mi/Gi)
numfmt-error-rejecting-suffix = rejecting suffix in input: '{ $number }{ $suffix }' (consider using --from)
numfmt-error-suffix-unsupported-for-unit = This suffix is unsupported for specified unit
numfmt-error-unit-auto-not-supported-with-to = Unit 'auto' isn't supported with --to options
numfmt-error-number-too-big = Number is too big and unsupported
numfmt-error-format-no-percent = format '{ $format }' has no % directive
numfmt-error-format-ends-in-percent = format '{ $format }' ends in %
numfmt-error-invalid-format-directive = invalid format '{ $format }', directive must be %[0]['][-][N][.][N]f
numfmt-error-invalid-format-width-overflow = invalid format '{ $format }' (width overflow)
numfmt-error-invalid-precision = invalid precision in format '{ $format }'
numfmt-error-format-too-many-percent = format '{ $format }' has too many % directives
numfmt-error-unknown-invalid-mode = Unknown invalid mode: { $mode }

# Debug messages
numfmt-debug-no-conversion = no conversion option specified
numfmt-debug-grouping-no-effect = grouping has no effect in this locale
numfmt-debug-failed-to-convert = failed to convert some of the input numbers
numfmt-debug-header-ignored = --header ignored with command-line input
"),
        // Locale for od (en-US)
        "od/en-US.ftl" => Some(r#"od-about = Dump files in octal and other formats
od-usage = od [OPTION]... [--] [FILENAME]...
  od [-abcdDefFhHiIlLoOsxX] [FILENAME] [[+][0x]OFFSET[.][b]]
  od --traditional [OPTION]... [FILENAME] [[+][0x]OFFSET[.][b] [[+][0x]LABEL[.][b]]]
od-after-help = Displays data in various human-readable formats. If multiple formats are
  specified, the output will contain all formats in the order they appear on the
  command line. Each format will be printed on a new line. Only the line
  containing the first format will be prefixed with the offset.

  If no filename is specified, or it is "-", stdin will be used. After a "--", no
  more options will be recognized. This allows for filenames starting with a "-".

  If a filename is a valid number which can be used as an offset in the second
  form, you can force it to be recognized as a filename if you include an option
  like "-j0", which is only valid in the first form.

  RADIX is one of o,d,x,n for octal, decimal, hexadecimal or none.

  BYTES is decimal by default, octal if prefixed with a "0", or hexadecimal if
  prefixed with "0x". The suffixes b, KB, K, MB, M, GB, G, will multiply the
  number with 512, 1000, 1024, 1000^2, 1024^2, 1000^3, 1024^3, 1000^2, 1024^2.

  OFFSET and LABEL are octal by default, hexadecimal if prefixed with "0x" or
  decimal if a "." suffix is added. The "b" suffix will multiply with 512.

  TYPE contains one or more format specifications consisting of:
      a for printable 7-bits ASCII
      c for utf-8 characters or octal for undefined characters
      d[SIZE] for signed decimal
      f[SIZE] for floating point
      o[SIZE] for octal
      u[SIZE] for unsigned decimal
      x[SIZE] for hexadecimal
  SIZE is the number of bytes which can be the number 1, 2, 4, 8 or 16,
      or C, S, I, L for 1, 2, 4, 8 bytes for integer types,
      or F, D, L for 4, 8, 16 bytes for floating point.
  Any type specification can have a "z" suffix, which will add a ASCII dump at
      the end of the line.

  If an error occurred, a diagnostic message will be printed to stderr, and the
  exit code will be non-zero.

# Error messages
od-error-invalid-endian = Invalid argument --endian={$endian}
od-error-invalid-inputs = Invalid inputs: {$msg}
od-error-too-large = value is too large
od-error-radix-invalid = Radix must be one of [o, d, x, n], got: {$radix}
od-error-radix-empty = Radix cannot be empty, and must be one of [o, d, x, n]
od-error-invalid-width = invalid width {$width}; using {$min} instead
od-error-missing-format-spec = missing format specification after '--format' / '-t'
od-error-unexpected-char = unexpected char '{$char}' in format specification {$spec}
od-error-invalid-number = invalid number {$number} in format specification {$spec}
od-error-invalid-size = invalid size '{$size}' in format specification {$spec}
od-error-invalid-offset = invalid offset: {$offset}
od-error-invalid-label = invalid label: {$label}
od-error-too-many-inputs = too many inputs after --traditional: {$input}
od-error-parse-failed = parse failed
od-error-overflow = Numerical result out of range
od-error-invalid-suffix = invalid suffix in {$option} argument {$value}
od-error-invalid-argument = invalid {$option} argument {$value}
od-error-argument-too-large = {$option} argument {$value} too large
od-error-skip-past-end = tried to skip past end of input

# Help messages
od-help-help = Print help information.
od-help-address-radix = Select the base in which file offsets are printed.
od-help-skip-bytes = Skip bytes input bytes before formatting and writing.
od-help-read-bytes = limit dump to BYTES input bytes
od-help-endian = byte order to use for multi-byte formats
od-help-strings = output strings of at least BYTES graphic chars. 3 is assumed when BYTES is not specified.
od-help-a = named characters, ignoring high-order bit
od-help-b = octal bytes
od-help-c = ASCII characters or backslash escapes
od-help-d = unsigned decimal 2-byte units
od-help-d4 = unsigned decimal 4-byte units
od-help-format = select output format or formats
od-help-output-duplicates = do not use * to mark line suppression
od-help-width = output BYTES bytes per output line. 32 is implied when BYTES is not
                specified.
od-help-traditional = compatibility mode with one input, offset and label.
od-help-o = octal 2-byte units
od-help-capital-i = decimal 8-byte units
od-help-capital-l = decimal 8-byte units
od-help-i = decimal 4-byte units
od-help-l = decimal 8-byte units
od-help-x = hexadecimal 2-byte units
od-help-h = hexadecimal 2-byte units
od-help-capital-o = octal 4-byte units
od-help-s = decimal 2-byte units
od-help-capital-x = hexadecimal 4-byte units
od-help-capital-h = hexadecimal 4-byte units
od-help-e = floating point double precision (64-bit) units
od-help-f = floating point single precision (32-bit) units
od-help-capital-f = floating point double precision (64-bit) units
"#),
        // Locale for od (en-US)
        "od/en-US.ftl" => Some(r#"od-about = Dump files in octal and other formats
od-usage = od [OPTION]... [--] [FILENAME]...
  od [-abcdDefFhHiIlLoOsxX] [FILENAME] [[+][0x]OFFSET[.][b]]
  od --traditional [OPTION]... [FILENAME] [[+][0x]OFFSET[.][b] [[+][0x]LABEL[.][b]]]
od-after-help = Displays data in various human-readable formats. If multiple formats are
  specified, the output will contain all formats in the order they appear on the
  command line. Each format will be printed on a new line. Only the line
  containing the first format will be prefixed with the offset.

  If no filename is specified, or it is "-", stdin will be used. After a "--", no
  more options will be recognized. This allows for filenames starting with a "-".

  If a filename is a valid number which can be used as an offset in the second
  form, you can force it to be recognized as a filename if you include an option
  like "-j0", which is only valid in the first form.

  RADIX is one of o,d,x,n for octal, decimal, hexadecimal or none.

  BYTES is decimal by default, octal if prefixed with a "0", or hexadecimal if
  prefixed with "0x". The suffixes b, KB, K, MB, M, GB, G, will multiply the
  number with 512, 1000, 1024, 1000^2, 1024^2, 1000^3, 1024^3, 1000^2, 1024^2.

  OFFSET and LABEL are octal by default, hexadecimal if prefixed with "0x" or
  decimal if a "." suffix is added. The "b" suffix will multiply with 512.

  TYPE contains one or more format specifications consisting of:
      a for printable 7-bits ASCII
      c for utf-8 characters or octal for undefined characters
      d[SIZE] for signed decimal
      f[SIZE] for floating point
      o[SIZE] for octal
      u[SIZE] for unsigned decimal
      x[SIZE] for hexadecimal
  SIZE is the number of bytes which can be the number 1, 2, 4, 8 or 16,
      or C, S, I, L for 1, 2, 4, 8 bytes for integer types,
      or F, D, L for 4, 8, 16 bytes for floating point.
  Any type specification can have a "z" suffix, which will add a ASCII dump at
      the end of the line.

  If an error occurred, a diagnostic message will be printed to stderr, and the
  exit code will be non-zero.

# Error messages
od-error-invalid-endian = Invalid argument --endian={$endian}
od-error-invalid-inputs = Invalid inputs: {$msg}
od-error-too-large = value is too large
od-error-radix-invalid = Radix must be one of [o, d, x, n], got: {$radix}
od-error-radix-empty = Radix cannot be empty, and must be one of [o, d, x, n]
od-error-invalid-width = invalid width {$width}; using {$min} instead
od-error-missing-format-spec = missing format specification after '--format' / '-t'
od-error-unexpected-char = unexpected char '{$char}' in format specification {$spec}
od-error-invalid-number = invalid number {$number} in format specification {$spec}
od-error-invalid-size = invalid size '{$size}' in format specification {$spec}
od-error-invalid-offset = invalid offset: {$offset}
od-error-invalid-label = invalid label: {$label}
od-error-too-many-inputs = too many inputs after --traditional: {$input}
od-error-parse-failed = parse failed
od-error-overflow = Numerical result out of range
od-error-invalid-suffix = invalid suffix in {$option} argument {$value}
od-error-invalid-argument = invalid {$option} argument {$value}
od-error-argument-too-large = {$option} argument {$value} too large
od-error-skip-past-end = tried to skip past end of input

# Help messages
od-help-help = Print help information.
od-help-address-radix = Select the base in which file offsets are printed.
od-help-skip-bytes = Skip bytes input bytes before formatting and writing.
od-help-read-bytes = limit dump to BYTES input bytes
od-help-endian = byte order to use for multi-byte formats
od-help-strings = output strings of at least BYTES graphic chars. 3 is assumed when BYTES is not specified.
od-help-a = named characters, ignoring high-order bit
od-help-b = octal bytes
od-help-c = ASCII characters or backslash escapes
od-help-d = unsigned decimal 2-byte units
od-help-d4 = unsigned decimal 4-byte units
od-help-format = select output format or formats
od-help-output-duplicates = do not use * to mark line suppression
od-help-width = output BYTES bytes per output line. 32 is implied when BYTES is not
                specified.
od-help-traditional = compatibility mode with one input, offset and label.
od-help-o = octal 2-byte units
od-help-capital-i = decimal 8-byte units
od-help-capital-l = decimal 8-byte units
od-help-i = decimal 4-byte units
od-help-l = decimal 8-byte units
od-help-x = hexadecimal 2-byte units
od-help-h = hexadecimal 2-byte units
od-help-capital-o = octal 4-byte units
od-help-s = decimal 2-byte units
od-help-capital-x = hexadecimal 4-byte units
od-help-capital-h = hexadecimal 4-byte units
od-help-e = floating point double precision (64-bit) units
od-help-f = floating point single precision (32-bit) units
od-help-capital-f = floating point double precision (64-bit) units
"#),
        // Locale for paste (en-US)
        "paste/en-US.ftl" => Some(r"paste-about = Write lines consisting of the sequentially corresponding lines from each
  FILE, separated by TABs, to standard output.
paste-usage = paste [OPTIONS] [FILE]...

# Help messages
paste-help-serial = paste one file at a time instead of in parallel
paste-help-delimiter = reuse characters from LIST instead of TABs
paste-help-zero-terminated = line delimiter is NUL, not newline

# Error messages
paste-error-delimiter-unescaped-backslash = delimiter list ends with an unescaped backslash: { $delimiters }
paste-error-stdin-borrow = failed to access standard input: { $error }
"),
        // Locale for paste (en-US)
        "paste/en-US.ftl" => Some(r"paste-about = Write lines consisting of the sequentially corresponding lines from each
  FILE, separated by TABs, to standard output.
paste-usage = paste [OPTIONS] [FILE]...

# Help messages
paste-help-serial = paste one file at a time instead of in parallel
paste-help-delimiter = reuse characters from LIST instead of TABs
paste-help-zero-terminated = line delimiter is NUL, not newline

# Error messages
paste-error-delimiter-unescaped-backslash = delimiter list ends with an unescaped backslash: { $delimiters }
paste-error-stdin-borrow = failed to access standard input: { $error }
"),
        // Locale for pr (en-US)
        "pr/en-US.ftl" => Some(r#"pr-about = paginate or columnate FILE(s) for printing
pr-after-help =
  If no FILE(s) are given, or if FILE is -, read standard input.

  When creating multicolumn output, columns will be of equal width. When using
  the '-s' option to separate columns, the default separator is a single tab
  character. When using the '-S' option to separate columns, the default separator
  is a single space character.
pr-usage = pr [OPTION]... [FILE]...

# Help messages
pr-help-pages = Begin and stop printing with page FIRST_PAGE[:LAST_PAGE]
pr-help-header =
  Use the string header to replace the file name
                  in the header line.
pr-help-date-format =
  Use 'date'-style FORMAT for the header date.
pr-help-double-space =
  Produce output that is double spaced. An extra `<newline>`
                  character is output following every `<newline>` found in the input.
pr-help-number-lines =
  Provide width digit line numbering.  The default for width,
                  if not specified, is 5.  The number occupies the first width column
                  positions of each text column or each line of -m output.  If char
                  (any non-digit character) is given, it is appended to the line number
                  to separate it from whatever follows.  The default for char is a `<tab>`.
                  Line numbers longer than width columns are truncated.
pr-help-first-line-number = start counting with NUMBER at 1st line of first page printed
pr-help-omit-header =
  Write neither the five-line identifying header nor the five-line
                  trailer usually supplied for each page. Quit writing after the last line
                   of each file without spacing to the end of the page.
pr-help-omit-pagination =
  omit page headers and trailers, eliminate any pagination
                  by form feeds set in input files
pr-help-page-length =
  Override the 66-line default (default number of lines of text 56,
                  and with -F 63) and reset the page length to lines.  If lines is not
                  greater than the sum  of  both the  header  and trailer depths (in lines),
                  the pr utility shall suppress both the header and trailer, as if the -t
                  option were in effect.
pr-help-no-file-warnings = omit warning when a file cannot be opened
pr-help-form-feed =
  Use a `<form-feed>` for new pages, instead of the default behavior that
                  uses a sequence of `<newline>`s.
pr-help-column-width =
  Set the width of the line to width column positions for multiple
                  text-column output only. If the -w option is not specified and the -s option
                  is not specified, the default width shall be 72. If the -w option is not specified
                  and the -s option is specified, the default width shall be 512.
pr-help-page-width =
  set page width to PAGE_WIDTH (72) characters always,
                  truncate lines, except -J option is set, no interference
                  with -S or -s
pr-help-across =
  Modify the effect of the - column option so that the columns are filled
                  across the page in a  round-robin  order (for example, when column is 2, the
                  first input line heads column 1, the second heads column 2, the third is the
                  second line in column 1, and so on).
pr-help-column =
  Produce multi-column output that is arranged in column columns
                  (the default shall be 1) and is written down each column  in  the order in which
                  the text is received from the input file. This option should not be used with -m.
                  The options -e and -i shall be assumed for multiple text-column output.  Whether
                  or not text columns are produced with identical vertical lengths is unspecified,
                  but a text column shall never exceed the length of the page (see the -l option).
                  When used with -t, use the minimum number of lines to write the output.
pr-help-column-char-separator =
  Separate text columns by the single character char instead of by the
                  appropriate number of `<space>`s (default for char is the `<tab>` character).
pr-help-column-string-separator =
  separate columns by STRING,
                  without -S: Default separator `<TAB>` with -J and `<space>`
                  otherwise (same as -S\" \"), no effect on column options
pr-help-merge =
  Merge files. Standard output shall be formatted so the pr utility
                  writes one line from each file specified by a file operand, side by side
                  into text columns of equal fixed widths, in terms of the number of column
                  positions. Implementations shall support merging of at least nine file operands.
pr-help-indent =
  Each line of output shall be preceded by offset `<space>`s. If the -o
                  option is not specified, the default offset shall be zero. The space taken is
                  in addition to the output line width (see the -w option below).
pr-help-join-lines =
  merge full lines, turns off -W line truncation, no column
                  alignment, --sep-string[=STRING] sets separators
pr-help-expand-tabs = expand input CHARs (TABs) to tab WIDTH (8)
pr-help-help = Print help information

# Page header text
pr-page = Page

pr-try-help-message = Try 'pr --help' for more information.

# Error messages
pr-error-reading-input = pr: Reading from input {$file} gave error
pr-error-unknown-filetype = pr: {$file}: unknown filetype
pr-error-is-directory = pr: {$file}: Is a directory
pr-error-socket-not-supported = pr: cannot open {$file}, Operation not supported on socket
pr-error-no-such-file = pr: cannot open {$file}, No such file or directory
pr-error-column-merge-conflict = cannot specify number of columns when printing in parallel
pr-error-across-merge-conflict = cannot specify both printing across and printing in parallel
pr-error-invalid-pages-range = invalid --pages argument '{$start}:{$end}'
pr-error-starting-page-exceeds-page-count = starting page number {$start} exceeds page count {$count}
pr-error-invalid-expand-tab-argument ='-e' extra characters or invalid number in the argument: ‘{$arg}’
pr-error-invalid-number-argument ='-n' extra characters or invalid number in the argument: ‘{$arg}’
"#),
        // Locale for pr (en-US)
        "pr/en-US.ftl" => Some(r#"pr-about = paginate or columnate FILE(s) for printing
pr-after-help =
  If no FILE(s) are given, or if FILE is -, read standard input.

  When creating multicolumn output, columns will be of equal width. When using
  the '-s' option to separate columns, the default separator is a single tab
  character. When using the '-S' option to separate columns, the default separator
  is a single space character.
pr-usage = pr [OPTION]... [FILE]...

# Help messages
pr-help-pages = Begin and stop printing with page FIRST_PAGE[:LAST_PAGE]
pr-help-header =
  Use the string header to replace the file name
                  in the header line.
pr-help-date-format =
  Use 'date'-style FORMAT for the header date.
pr-help-double-space =
  Produce output that is double spaced. An extra `<newline>`
                  character is output following every `<newline>` found in the input.
pr-help-number-lines =
  Provide width digit line numbering.  The default for width,
                  if not specified, is 5.  The number occupies the first width column
                  positions of each text column or each line of -m output.  If char
                  (any non-digit character) is given, it is appended to the line number
                  to separate it from whatever follows.  The default for char is a `<tab>`.
                  Line numbers longer than width columns are truncated.
pr-help-first-line-number = start counting with NUMBER at 1st line of first page printed
pr-help-omit-header =
  Write neither the five-line identifying header nor the five-line
                  trailer usually supplied for each page. Quit writing after the last line
                   of each file without spacing to the end of the page.
pr-help-omit-pagination =
  omit page headers and trailers, eliminate any pagination
                  by form feeds set in input files
pr-help-page-length =
  Override the 66-line default (default number of lines of text 56,
                  and with -F 63) and reset the page length to lines.  If lines is not
                  greater than the sum  of  both the  header  and trailer depths (in lines),
                  the pr utility shall suppress both the header and trailer, as if the -t
                  option were in effect.
pr-help-no-file-warnings = omit warning when a file cannot be opened
pr-help-form-feed =
  Use a `<form-feed>` for new pages, instead of the default behavior that
                  uses a sequence of `<newline>`s.
pr-help-column-width =
  Set the width of the line to width column positions for multiple
                  text-column output only. If the -w option is not specified and the -s option
                  is not specified, the default width shall be 72. If the -w option is not specified
                  and the -s option is specified, the default width shall be 512.
pr-help-page-width =
  set page width to PAGE_WIDTH (72) characters always,
                  truncate lines, except -J option is set, no interference
                  with -S or -s
pr-help-across =
  Modify the effect of the - column option so that the columns are filled
                  across the page in a  round-robin  order (for example, when column is 2, the
                  first input line heads column 1, the second heads column 2, the third is the
                  second line in column 1, and so on).
pr-help-column =
  Produce multi-column output that is arranged in column columns
                  (the default shall be 1) and is written down each column  in  the order in which
                  the text is received from the input file. This option should not be used with -m.
                  The options -e and -i shall be assumed for multiple text-column output.  Whether
                  or not text columns are produced with identical vertical lengths is unspecified,
                  but a text column shall never exceed the length of the page (see the -l option).
                  When used with -t, use the minimum number of lines to write the output.
pr-help-column-char-separator =
  Separate text columns by the single character char instead of by the
                  appropriate number of `<space>`s (default for char is the `<tab>` character).
pr-help-column-string-separator =
  separate columns by STRING,
                  without -S: Default separator `<TAB>` with -J and `<space>`
                  otherwise (same as -S\" \"), no effect on column options
pr-help-merge =
  Merge files. Standard output shall be formatted so the pr utility
                  writes one line from each file specified by a file operand, side by side
                  into text columns of equal fixed widths, in terms of the number of column
                  positions. Implementations shall support merging of at least nine file operands.
pr-help-indent =
  Each line of output shall be preceded by offset `<space>`s. If the -o
                  option is not specified, the default offset shall be zero. The space taken is
                  in addition to the output line width (see the -w option below).
pr-help-join-lines =
  merge full lines, turns off -W line truncation, no column
                  alignment, --sep-string[=STRING] sets separators
pr-help-expand-tabs = expand input CHARs (TABs) to tab WIDTH (8)
pr-help-help = Print help information

# Page header text
pr-page = Page

pr-try-help-message = Try 'pr --help' for more information.

# Error messages
pr-error-reading-input = pr: Reading from input {$file} gave error
pr-error-unknown-filetype = pr: {$file}: unknown filetype
pr-error-is-directory = pr: {$file}: Is a directory
pr-error-socket-not-supported = pr: cannot open {$file}, Operation not supported on socket
pr-error-no-such-file = pr: cannot open {$file}, No such file or directory
pr-error-column-merge-conflict = cannot specify number of columns when printing in parallel
pr-error-across-merge-conflict = cannot specify both printing across and printing in parallel
pr-error-invalid-pages-range = invalid --pages argument '{$start}:{$end}'
pr-error-invalid-expand-tab-argument ='-e' extra characters or invalid number in the argument: ‘{$arg}’
"#),
        // Locale for printenv (en-US)
        "printenv/en-US.ftl" => Some(r"printenv-about = Display the values of the specified environment VARIABLE(s), or (with no VARIABLE) display name and value pairs for them all.
printenv-usage = printenv [OPTION]... [VARIABLE]...

printenv-help-null = end each output line with 0 byte rather than newline
"),
        // Locale for printenv (en-US)
        "printenv/en-US.ftl" => Some(r"printenv-about = Display the values of the specified environment VARIABLE(s), or (with no VARIABLE) display name and value pairs for them all.
printenv-usage = printenv [OPTION]... [VARIABLE]...

printenv-help-null = end each output line with 0 byte rather than newline
"),
        // Locale for printf (en-US)
        "printf/en-US.ftl" => Some(r#"printf-about = Print output based off of the format string and proceeding arguments.
printf-usage = printf FORMAT [ARGUMENT]...
  printf OPTION
printf-after-help = basic anonymous string templating:

  prints format string at least once, repeating as long as there are remaining arguments
  output prints escaped literals in the format string as character literals
  output replaces anonymous fields with the next unused argument, formatted according to the field.

  Prints the , replacing escaped character sequences with character literals
  and substitution field sequences with passed arguments

  literally, with the exception of the below
  escaped character sequences, and the substitution sequences described further down.

  ### ESCAPE SEQUENCES

  The following escape sequences, organized here in alphabetical order,
  will print the corresponding character literal:

  - \" double quote

  - \\ backslash

  - \\a alert (BEL)

  - \\b backspace

  - \\c End-of-Input

  - \\e escape

  - \\f form feed

  - \\n new line

  - \\r carriage return

  - \\t horizontal tab

  - \\v vertical tab

  - \\NNN byte with value expressed in octal value NNN (1 to 3 digits)
            values greater than 256 will be treated

  - \\xHH byte with value expressed in hexadecimal value NN (1 to 2 digits)

  - \\uHHHH Unicode (IEC 10646) character with value expressed in hexadecimal value HHHH (4 digits)

  - \\uHHHH Unicode character with value expressed in hexadecimal value HHHH (8 digits)

  - %% a single %

  ### SUBSTITUTIONS

  #### SUBSTITUTION QUICK REFERENCE

  Fields

  - %s: string
  - %b: string parsed for literals second parameter is max length

  - %c: char no second parameter

  - %i or %d: 64-bit integer
  - %u: 64 bit unsigned integer
  - %x or %X: 64-bit unsigned integer as hex
  - %o: 64-bit unsigned integer as octal
              second parameter is min-width, integer
              output below that width is padded with leading zeroes

  - %q: ARGUMENT is printed in a format that can be reused as shell input, escaping non-printable
              characters with the proposed POSIX $'' syntax.

  - %f or %F: decimal floating point value
  - %e or %E: scientific notation floating point value
  - %g or %G: shorter of specially interpreted decimal or SciNote floating point value.
              second parameter is
                -max places after decimal point for floating point output
                -max number of significant digits for scientific notation output

  parameterizing fields

  examples:

  printf '%4.3i' 7

  It has a first parameter of 4 and a second parameter of 3 and will result in ' 007'

  printf '%.1s' abcde

  It has no first parameter and a second parameter of 1 and will result in 'a'

  printf '%4c' q

  It has a first parameter of 4 and no second parameter and will result in ' q'

  The first parameter of a field is the minimum width to pad the output to
  if the output is less than this absolute value of this width,
  it will be padded with leading spaces, or, if the argument is negative,
  with trailing spaces. the default is zero.

  The second parameter of a field is particular to the output field type.
  defaults can be found in the full substitution help below

  special prefixes to numeric arguments

  - 0: (e.g. 010) interpret argument as octal (integer output fields only)
  - 0x: (e.g. 0xABC) interpret argument as hex (numeric output fields only)
  - \': (e.g. \'a) interpret argument as a character constant

  #### HOW TO USE SUBSTITUTIONS

  Substitutions are used to pass additional argument(s) into the FORMAT string, to be formatted a
  particular way. E.g.

  printf 'the letter %X comes before the letter %X' 10 11

  will print

  the letter A comes before the letter B

  because the substitution field %X means
  'take an integer argument and write it as a hexadecimal number'

  Passing more arguments than are in the format string will cause the format string to be
  repeated for the remaining substitutions

  printf 'it is %i F in %s \n' 22 Portland 25 Boston 27 New York

  will print

  it is 22 F in Portland
  it is 25 F in Boston
  it is 27 F in Boston

  If a format string is printed but there are less arguments remaining
  than there are substitution fields, substitution fields without
  an argument will default to empty strings, or for numeric fields
  the value 0

  #### AVAILABLE SUBSTITUTIONS

  This program, like GNU coreutils printf,
  interprets a modified subset of the POSIX C printf spec,
  a quick reference to substitutions is below.

  #### STRING SUBSTITUTIONS

  All string fields have a 'max width' parameter
  %.3s means 'print no more than three characters of the original input'

  - %s: string

  - %b: escaped string - the string will be checked for any escaped literals from
        the escaped literal list above, and translate them to literal characters.
        e.g. \\n will be transformed into a newline character.
        One special rule about %b mode is that octal literals are interpreted differently
        In arguments passed by %b, pass octal-interpreted literals must be in the form of \\0NNN
        instead of \\NNN. (Although, for legacy reasons, octal literals in the form of \\NNN will
        still be interpreted and not throw a warning, you will have problems if you use this for a
        literal whose code begins with zero, as it will be viewed as in \\0NNN form.)

  - %q: escaped string - the string in a format that can be reused as input by most shells.
        Non-printable characters are escaped with the POSIX proposed ‘$''’ syntax,
        and shell meta-characters are quoted appropriately.
        This is an equivalent format to ls --quoting=shell-escape output.

  #### CHAR SUBSTITUTIONS

  The character field does not have a secondary parameter.

  - %c: a single character

  #### INTEGER SUBSTITUTIONS

  All integer fields have a 'pad with zero' parameter
  %.4i means an integer which if it is less than 4 digits in length,
  is padded with leading zeros until it is 4 digits in length.

  - %d or %i: 64-bit integer

  - %u: 64-bit unsigned integer

  - %x or %X: 64-bit unsigned integer printed in Hexadecimal (base 16)
              %X instead of %x means to use uppercase letters for 'a' through 'f'

  - %o: 64-bit unsigned integer printed in octal (base 8)

  #### FLOATING POINT SUBSTITUTIONS

  All floating point fields have a 'max decimal places / max significant digits' parameter
  %.10f means a decimal floating point with 7 decimal places past 0
  %.10e means a scientific notation number with 10 significant digits
  %.10g means the same behavior for decimal and Sci. Note, respectively, and provides the shortest
  of each's output.

  Like with GNU coreutils, the value after the decimal point is these outputs is parsed as a
  double first before being rendered to text. For both implementations do not expect meaningful
  precision past the 18th decimal place. When using a number of decimal places that is 18 or
  higher, you can expect variation in output between GNU coreutils printf and this printf at the
  18th decimal place of +/- 1

  - %f: floating point value presented in decimal, truncated and displayed to 6 decimal places by
        default. There is not past-double behavior parity with Coreutils printf, values are not
        estimated or adjusted beyond input values.

  - %e or %E: floating point value presented in scientific notation
              7 significant digits by default
              %E means use to use uppercase E for the mantissa.

  - %g or %G: floating point value presented in the shortest of decimal and scientific notation
              behaves differently from %f and %E, please see posix printf spec for full details,
              some examples of different behavior:
              Sci Note has 6 significant digits by default
              Trailing zeroes are removed
              Instead of being truncated, digit after last is rounded

  Like other behavior in this utility, the design choices of floating point
  behavior in this utility is selected to reproduce in exact
  the behavior of GNU coreutils' printf from an inputs and outputs standpoint.

  ### USING PARAMETERS

  Most substitution fields can be parameterized using up to 2 numbers that can
  be passed to the field, between the % sign and the field letter.

  The 1st parameter always indicates the minimum width of output, it is useful for creating
  columnar output. Any output that would be less than this minimum width is padded with
  leading spaces
  The 2nd parameter is proceeded by a dot.
  You do not have to use parameters

  ### SPECIAL FORMS OF INPUT

  For numeric input, the following additional forms of input are accepted besides decimal:

  Octal (only with integer): if the argument begins with a 0 the proceeding characters
  will be interpreted as octal (base 8) for integer fields

  Hexadecimal: if the argument begins with 0x the proceeding characters will be interpreted
  will be interpreted as hex (base 16) for any numeric fields
  for float fields, hexadecimal input results in a precision
  limit (in converting input past the decimal point) of 10^-15

  Character Constant: if the argument begins with a single quote character, the first byte
  of the next character will be interpreted as an 8-bit unsigned integer. If there are
  additional bytes, they will throw an error (unless the environment variable POSIXLY_CORRECT
  is set)

printf-error-missing-operand = missing operand
printf-warning-ignoring-excess-arguments = ignoring excess arguments, starting with { $arg }
printf-help-version = Print version information
printf-help-help = Print help information
"#),
        // Locale for printf (en-US)
        "printf/en-US.ftl" => Some(r#"printf-about = Print output based off of the format string and proceeding arguments.
printf-usage = printf FORMAT [ARGUMENT]...
  printf OPTION
printf-after-help = basic anonymous string templating:

  prints format string at least once, repeating as long as there are remaining arguments
  output prints escaped literals in the format string as character literals
  output replaces anonymous fields with the next unused argument, formatted according to the field.

  Prints the , replacing escaped character sequences with character literals
  and substitution field sequences with passed arguments

  literally, with the exception of the below
  escaped character sequences, and the substitution sequences described further down.

  ### ESCAPE SEQUENCES

  The following escape sequences, organized here in alphabetical order,
  will print the corresponding character literal:

  - \" double quote

  - \\ backslash

  - \\a alert (BEL)

  - \\b backspace

  - \\c End-of-Input

  - \\e escape

  - \\f form feed

  - \\n new line

  - \\r carriage return

  - \\t horizontal tab

  - \\v vertical tab

  - \\NNN byte with value expressed in octal value NNN (1 to 3 digits)
            values greater than 256 will be treated

  - \\xHH byte with value expressed in hexadecimal value NN (1 to 2 digits)

  - \\uHHHH Unicode (IEC 10646) character with value expressed in hexadecimal value HHHH (4 digits)

  - \\uHHHH Unicode character with value expressed in hexadecimal value HHHH (8 digits)

  - %% a single %

  ### SUBSTITUTIONS

  #### SUBSTITUTION QUICK REFERENCE

  Fields

  - %s: string
  - %b: string parsed for literals second parameter is max length

  - %c: char no second parameter

  - %i or %d: 64-bit integer
  - %u: 64 bit unsigned integer
  - %x or %X: 64-bit unsigned integer as hex
  - %o: 64-bit unsigned integer as octal
              second parameter is min-width, integer
              output below that width is padded with leading zeroes

  - %q: ARGUMENT is printed in a format that can be reused as shell input, escaping non-printable
              characters with the proposed POSIX $'' syntax.

  - %f or %F: decimal floating point value
  - %e or %E: scientific notation floating point value
  - %g or %G: shorter of specially interpreted decimal or SciNote floating point value.
              second parameter is
                -max places after decimal point for floating point output
                -max number of significant digits for scientific notation output

  parameterizing fields

  examples:

  printf '%4.3i' 7

  It has a first parameter of 4 and a second parameter of 3 and will result in ' 007'

  printf '%.1s' abcde

  It has no first parameter and a second parameter of 1 and will result in 'a'

  printf '%4c' q

  It has a first parameter of 4 and no second parameter and will result in ' q'

  The first parameter of a field is the minimum width to pad the output to
  if the output is less than this absolute value of this width,
  it will be padded with leading spaces, or, if the argument is negative,
  with trailing spaces. the default is zero.

  The second parameter of a field is particular to the output field type.
  defaults can be found in the full substitution help below

  special prefixes to numeric arguments

  - 0: (e.g. 010) interpret argument as octal (integer output fields only)
  - 0x: (e.g. 0xABC) interpret argument as hex (numeric output fields only)
  - \': (e.g. \'a) interpret argument as a character constant

  #### HOW TO USE SUBSTITUTIONS

  Substitutions are used to pass additional argument(s) into the FORMAT string, to be formatted a
  particular way. E.g.

  printf 'the letter %X comes before the letter %X' 10 11

  will print

  the letter A comes before the letter B

  because the substitution field %X means
  'take an integer argument and write it as a hexadecimal number'

  Passing more arguments than are in the format string will cause the format string to be
  repeated for the remaining substitutions

  printf 'it is %i F in %s \n' 22 Portland 25 Boston 27 New York

  will print

  it is 22 F in Portland
  it is 25 F in Boston
  it is 27 F in Boston

  If a format string is printed but there are less arguments remaining
  than there are substitution fields, substitution fields without
  an argument will default to empty strings, or for numeric fields
  the value 0

  #### AVAILABLE SUBSTITUTIONS

  This program, like GNU coreutils printf,
  interprets a modified subset of the POSIX C printf spec,
  a quick reference to substitutions is below.

  #### STRING SUBSTITUTIONS

  All string fields have a 'max width' parameter
  %.3s means 'print no more than three characters of the original input'

  - %s: string

  - %b: escaped string - the string will be checked for any escaped literals from
        the escaped literal list above, and translate them to literal characters.
        e.g. \\n will be transformed into a newline character.
        One special rule about %b mode is that octal literals are interpreted differently
        In arguments passed by %b, pass octal-interpreted literals must be in the form of \\0NNN
        instead of \\NNN. (Although, for legacy reasons, octal literals in the form of \\NNN will
        still be interpreted and not throw a warning, you will have problems if you use this for a
        literal whose code begins with zero, as it will be viewed as in \\0NNN form.)

  - %q: escaped string - the string in a format that can be reused as input by most shells.
        Non-printable characters are escaped with the POSIX proposed ‘$''’ syntax,
        and shell meta-characters are quoted appropriately.
        This is an equivalent format to ls --quoting=shell-escape output.

  #### CHAR SUBSTITUTIONS

  The character field does not have a secondary parameter.

  - %c: a single character

  #### INTEGER SUBSTITUTIONS

  All integer fields have a 'pad with zero' parameter
  %.4i means an integer which if it is less than 4 digits in length,
  is padded with leading zeros until it is 4 digits in length.

  - %d or %i: 64-bit integer

  - %u: 64-bit unsigned integer

  - %x or %X: 64-bit unsigned integer printed in Hexadecimal (base 16)
              %X instead of %x means to use uppercase letters for 'a' through 'f'

  - %o: 64-bit unsigned integer printed in octal (base 8)

  #### FLOATING POINT SUBSTITUTIONS

  All floating point fields have a 'max decimal places / max significant digits' parameter
  %.10f means a decimal floating point with 7 decimal places past 0
  %.10e means a scientific notation number with 10 significant digits
  %.10g means the same behavior for decimal and Sci. Note, respectively, and provides the shortest
  of each's output.

  Like with GNU coreutils, the value after the decimal point is these outputs is parsed as a
  double first before being rendered to text. For both implementations do not expect meaningful
  precision past the 18th decimal place. When using a number of decimal places that is 18 or
  higher, you can expect variation in output between GNU coreutils printf and this printf at the
  18th decimal place of +/- 1

  - %f: floating point value presented in decimal, truncated and displayed to 6 decimal places by
        default. There is not past-double behavior parity with Coreutils printf, values are not
        estimated or adjusted beyond input values.

  - %e or %E: floating point value presented in scientific notation
              7 significant digits by default
              %E means use to use uppercase E for the mantissa.

  - %g or %G: floating point value presented in the shortest of decimal and scientific notation
              behaves differently from %f and %E, please see posix printf spec for full details,
              some examples of different behavior:
              Sci Note has 6 significant digits by default
              Trailing zeroes are removed
              Instead of being truncated, digit after last is rounded

  Like other behavior in this utility, the design choices of floating point
  behavior in this utility is selected to reproduce in exact
  the behavior of GNU coreutils' printf from an inputs and outputs standpoint.

  ### USING PARAMETERS

  Most substitution fields can be parameterized using up to 2 numbers that can
  be passed to the field, between the % sign and the field letter.

  The 1st parameter always indicates the minimum width of output, it is useful for creating
  columnar output. Any output that would be less than this minimum width is padded with
  leading spaces
  The 2nd parameter is proceeded by a dot.
  You do not have to use parameters

  ### SPECIAL FORMS OF INPUT

  For numeric input, the following additional forms of input are accepted besides decimal:

  Octal (only with integer): if the argument begins with a 0 the proceeding characters
  will be interpreted as octal (base 8) for integer fields

  Hexadecimal: if the argument begins with 0x the proceeding characters will be interpreted
  will be interpreted as hex (base 16) for any numeric fields
  for float fields, hexadecimal input results in a precision
  limit (in converting input past the decimal point) of 10^-15

  Character Constant: if the argument begins with a single quote character, the first byte
  of the next character will be interpreted as an 8-bit unsigned integer. If there are
  additional bytes, they will throw an error (unless the environment variable POSIXLY_CORRECT
  is set)

printf-error-missing-operand = missing operand
printf-warning-ignoring-excess-arguments = ignoring excess arguments, starting with { $arg }
printf-help-version = Print version information
printf-help-help = Print help information
"#),
        // Locale for ptx (en-US)
        "ptx/en-US.ftl" => Some(r"ptx-about = Produce a permuted index of file contents
  Output a permuted index, including context, of the words in the input files.
  Mandatory arguments to long options are mandatory for short options too.
  With no FILE, or when FILE is -, read standard input. Default is '-F /'.
ptx-usage = ptx [OPTION]... [INPUT]...
  ptx -G [OPTION]... [INPUT [OUTPUT]]

# Help messages
ptx-help-auto-reference = output automatically generated references
ptx-help-traditional = behave more like System V 'ptx'
ptx-help-flag-truncation = use STRING for flagging line truncations
ptx-help-macro-name = macro name to use instead of 'xx'
ptx-help-roff = generate output as roff directives
ptx-help-tex = generate output as TeX directives
ptx-help-right-side-refs = put references at right, not counted in -w
ptx-help-sentence-regexp = for end of lines or end of sentences
ptx-help-word-regexp = use REGEXP to match each keyword
ptx-help-break-file = word break characters in this FILE
ptx-help-ignore-case = fold lower case to upper case for sorting
ptx-help-gap-size = gap size in columns between output fields
ptx-help-ignore-file = read ignore word list from FILE
ptx-help-only-file = read only word list from this FILE
ptx-help-references = first field of each line is a reference
ptx-help-typeset-mode = change the default width from 72 to 100
ptx-help-width = output width in columns, reference excluded

# Error messages
ptx-error-dumb-format = There is no dumb format with GNU extensions disabled
ptx-error-not-implemented = { $feature } not implemented yet
ptx-error-write-failed = write failed
ptx-error-extra-operand = extra operand { $operand }
ptx-error-empty-regexp = A regular expression cannot match a length zero string
ptx-error-invalid-regexp = Invalid regexp: { $error }
"),
        // Locale for ptx (en-US)
        "ptx/en-US.ftl" => Some(r"ptx-about = Produce a permuted index of file contents
  Output a permuted index, including context, of the words in the input files.
  Mandatory arguments to long options are mandatory for short options too.
  With no FILE, or when FILE is -, read standard input. Default is '-F /'.
ptx-usage = ptx [OPTION]... [INPUT]...
  ptx -G [OPTION]... [INPUT [OUTPUT]]

# Help messages
ptx-help-auto-reference = output automatically generated references
ptx-help-traditional = behave more like System V 'ptx'
ptx-help-flag-truncation = use STRING for flagging line truncations
ptx-help-macro-name = macro name to use instead of 'xx'
ptx-help-roff = generate output as roff directives
ptx-help-tex = generate output as TeX directives
ptx-help-right-side-refs = put references at right, not counted in -w
ptx-help-sentence-regexp = for end of lines or end of sentences
ptx-help-word-regexp = use REGEXP to match each keyword
ptx-help-break-file = word break characters in this FILE
ptx-help-ignore-case = fold lower case to upper case for sorting
ptx-help-gap-size = gap size in columns between output fields
ptx-help-ignore-file = read ignore word list from FILE
ptx-help-only-file = read only word list from this FILE
ptx-help-references = first field of each line is a reference
ptx-help-typeset-mode = change the default width from 72 to 100
ptx-help-width = output width in columns, reference excluded

# Error messages
ptx-error-dumb-format = There is no dumb format with GNU extensions disabled
ptx-error-not-implemented = { $feature } not implemented yet
ptx-error-write-failed = write failed
ptx-error-extra-operand = extra operand { $operand }
ptx-error-empty-regexp = A regular expression cannot match a length zero string
ptx-error-invalid-regexp = Invalid regexp: { $error }
"),
        // Locale for pwd (en-US)
        "pwd/en-US.ftl" => Some(r"pwd-about = Display the full filename of the current working directory.
pwd-usage = pwd [OPTION]...

# Help messages
pwd-help-logical = use PWD from environment, even if it contains symlinks
pwd-help-physical = avoid all symlinks

# Warning messages
pwd-ignoring-non-option-arguments = ignoring non-option arguments

# Error messages
pwd-error-failed-to-get-current-directory = failed to get current directory
pwd-error-failed-to-print-current-directory = failed to print current directory
"),
        // Locale for pwd (en-US)
        "pwd/en-US.ftl" => Some(r"pwd-about = Display the full filename of the current working directory.
pwd-usage = pwd [OPTION]...

# Help messages
pwd-help-logical = use PWD from environment, even if it contains symlinks
pwd-help-physical = avoid all symlinks

# Error messages
pwd-error-failed-to-get-current-directory = failed to get current directory
pwd-error-failed-to-print-current-directory = failed to print current directory
"),
        // Locale for readlink (en-US)
        "readlink/en-US.ftl" => Some(r"readlink-about = Print value of a symbolic link or canonical file name.
readlink-usage = readlink [OPTION]... [FILE]...

# Help messages
readlink-help-canonicalize = canonicalize by following every symlink in every component of the given name recursively; all but the last component must exist
readlink-help-canonicalize-existing = canonicalize by following every symlink in every component of the given name recursively, all components must exist
readlink-help-canonicalize-missing = canonicalize by following every symlink in every component of the given name recursively, without requirements on components existence
readlink-help-no-newline = do not output the trailing delimiter
readlink-help-quiet = suppress most error messages
readlink-help-silent = suppress most error messages
readlink-help-verbose = report error message
readlink-help-zero = separate output with NUL rather than newline

# Error messages
readlink-error-missing-operand = missing operand
readlink-error-ignoring-no-newline = ignoring --no-newline with multiple arguments
readlink-error-invalid-argument = {$path}: Invalid argument
"),
        // Locale for readlink (en-US)
        "readlink/en-US.ftl" => Some(r"readlink-about = Print value of a symbolic link or canonical file name.
readlink-usage = readlink [OPTION]... [FILE]...

# Help messages
readlink-help-canonicalize = canonicalize by following every symlink in every component of the given name recursively; all but the last component must exist
readlink-help-canonicalize-existing = canonicalize by following every symlink in every component of the given name recursively, all components must exist
readlink-help-canonicalize-missing = canonicalize by following every symlink in every component of the given name recursively, without requirements on components existence
readlink-help-no-newline = do not output the trailing delimiter
readlink-help-quiet = suppress most error messages
readlink-help-silent = suppress most error messages
readlink-help-verbose = report error message
readlink-help-zero = separate output with NUL rather than newline

# Error messages
readlink-error-missing-operand = missing operand
readlink-error-ignoring-no-newline = ignoring --no-newline with multiple arguments
readlink-error-invalid-argument = {$path}: Invalid argument
"),
        // Locale for realpath (en-US)
        "realpath/en-US.ftl" => Some(r"realpath-about = Print the resolved path
realpath-usage = realpath [OPTION]... FILE...

# Help messages
realpath-help-quiet = Do not print warnings for invalid paths
realpath-help-strip = Only strip '.' and '..' components, but don't resolve symbolic links
realpath-help-zero = Separate output filenames with \0 rather than newline
realpath-help-logical = resolve '..' components before symlinks
realpath-help-physical = resolve symlinks as encountered (default)
realpath-help-canonicalize = all but the last component must exist (default)
realpath-help-canonicalize-existing = canonicalize by following every symlink in every component of the given name recursively, all components must exist
realpath-help-canonicalize-missing = canonicalize by following every symlink in every component of the given name recursively, without requirements on components existence
realpath-help-relative-to = print the resolved path relative to DIR
realpath-help-relative-base = print absolute paths unless paths below DIR

# Error messages
realpath-invalid-empty-operand = invalid operand: empty string
"),
        // Locale for realpath (en-US)
        "realpath/en-US.ftl" => Some(r"realpath-about = Print the resolved path
realpath-usage = realpath [OPTION]... FILE...

# Help messages
realpath-help-quiet = Do not print warnings for invalid paths
realpath-help-strip = Only strip '.' and '..' components, but don't resolve symbolic links
realpath-help-zero = Separate output filenames with \0 rather than newline
realpath-help-logical = resolve '..' components before symlinks
realpath-help-physical = resolve symlinks as encountered (default)
realpath-help-canonicalize = all but the last component must exist (default)
realpath-help-canonicalize-existing = canonicalize by following every symlink in every component of the given name recursively, all components must exist
realpath-help-canonicalize-missing = canonicalize by following every symlink in every component of the given name recursively, without requirements on components existence
realpath-help-relative-to = print the resolved path relative to DIR
realpath-help-relative-base = print absolute paths unless paths below DIR

# Error messages
realpath-invalid-empty-operand = invalid operand: empty string
"),
        // Locale for rm (en-US)
        "rm/en-US.ftl" => Some(r"rm-about = Remove (unlink) the FILE(s)
rm-usage = rm [OPTION]... FILE...
rm-after-help = By default, rm does not remove directories. Use the --recursive (-r or -R)
  option to remove each listed directory, too, along with all of its contents

  To remove a file whose name starts with a '-', for example '-foo',
  use one of these commands:
  rm -- -foo

  rm ./-foo

  Note that if you use rm to remove a file, it might be possible to recover
  some of its contents, given sufficient expertise and/or time. For greater
  assurance that the contents are truly unrecoverable, consider using shred.

# Help text for options
rm-help-force = ignore nonexistent files and arguments, never prompt
rm-help-prompt-always = prompt before every removal
rm-help-prompt-once = prompt once before removing more than three files, or when removing recursively.
  Less intrusive than -i, while still giving some protection against most mistakes
rm-help-interactive = prompt according to WHEN: never, once (-I), or always (-i). Without WHEN,
  prompts always
rm-help-one-file-system = when removing a hierarchy recursively, skip any directory that is on a file
  system different from that of the corresponding command line argument (NOT
  IMPLEMENTED)
rm-help-no-preserve-root = do not treat '/' specially
rm-help-preserve-root = do not remove '/' (default); with 'all', reject any command line argument on a separate device from its parent
rm-help-recursive = remove directories and their contents recursively
rm-help-dir = remove empty directories
rm-help-verbose = explain what is being done
rm-help-progress = display a progress bar. Note: this feature is not supported by GNU coreutils.

# Progress messages
rm-progress-removing = Removing

# Error messages
rm-error-missing-operand = missing operand
  Try '{$util_name} --help' for more information.
rm-hint-dash-file = Try '{$util_name} ./{$path}' to remove the file {$file}.
  Try '{$util_name} --help' for more information.
rm-error-cannot-remove-no-such-file = cannot remove {$file}: No such file or directory
rm-error-cannot-remove-permission-denied = cannot remove {$file}: Permission denied
rm-error-cannot-remove-is-directory = cannot remove {$file}: Is a directory
rm-error-dangerous-recursive-operation = it is dangerous to operate recursively on '/'
rm-error-dangerous-recursive-operation-same-as-root = it is dangerous to operate recursively on '{$path}' (same as '/')
rm-error-use-no-preserve-root = use --no-preserve-root to override this failsafe
rm-error-refusing-to-remove-directory = refusing to remove '.' or '..' directory: skipping {$path}
rm-error-skipping-different-device = skipping {$file}, since it's on a different device
rm-error-and-preserve-root-all-in-effect = and --preserve-root=all is in effect
rm-error-cannot-remove = cannot remove {$file}
rm-error-may-not-abbreviate-no-preserve-root = you may not abbreviate the --no-preserve-root option
rm-error-standard-output = standard output: {$error}

# Verbose messages
rm-verbose-removed = removed {$file}
rm-verbose-removed-directory = removed directory {$file}
"),
        // Locale for rm (en-US)
        "rm/en-US.ftl" => Some(r"rm-about = Remove (unlink) the FILE(s)
rm-usage = rm [OPTION]... FILE...
rm-after-help = By default, rm does not remove directories. Use the --recursive (-r or -R)
  option to remove each listed directory, too, along with all of its contents

  To remove a file whose name starts with a '-', for example '-foo',
  use one of these commands:
  rm -- -foo

  rm ./-foo

  Note that if you use rm to remove a file, it might be possible to recover
  some of its contents, given sufficient expertise and/or time. For greater
  assurance that the contents are truly unrecoverable, consider using shred.

# Help text for options
rm-help-force = ignore nonexistent files and arguments, never prompt
rm-help-prompt-always = prompt before every removal
rm-help-prompt-once = prompt once before removing more than three files, or when removing recursively.
  Less intrusive than -i, while still giving some protection against most mistakes
rm-help-interactive = prompt according to WHEN: never, once (-I), or always (-i). Without WHEN,
  prompts always
rm-help-one-file-system = when removing a hierarchy recursively, skip any directory that is on a file
  system different from that of the corresponding command line argument (NOT
  IMPLEMENTED)
rm-help-no-preserve-root = do not treat '/' specially
rm-help-preserve-root = do not remove '/' (default)
rm-help-recursive = remove directories and their contents recursively
rm-help-dir = remove empty directories
rm-help-verbose = explain what is being done
rm-help-progress = display a progress bar. Note: this feature is not supported by GNU coreutils.

# Progress messages
rm-progress-removing = Removing

# Error messages
rm-error-missing-operand = missing operand
  Try '{$util_name} --help' for more information.
rm-error-cannot-remove-no-such-file = cannot remove {$file}: No such file or directory
rm-error-cannot-remove-permission-denied = cannot remove {$file}: Permission denied
rm-error-cannot-remove-is-directory = cannot remove {$file}: Is a directory
rm-error-dangerous-recursive-operation = it is dangerous to operate recursively on '/'
rm-error-dangerous-recursive-operation-same-as-root = it is dangerous to operate recursively on '{$path}' (same as '/')
rm-error-use-no-preserve-root = use --no-preserve-root to override this failsafe
rm-error-refusing-to-remove-directory = refusing to remove '.' or '..' directory: skipping {$path}
rm-error-cannot-remove = cannot remove {$file}
rm-error-may-not-abbreviate-no-preserve-root = you may not abbreviate the --no-preserve-root option

# Verbose messages
rm-verbose-removed = removed {$file}
rm-verbose-removed-directory = removed directory {$file}
"),
        // Locale for rmdir (en-US)
        "rmdir/en-US.ftl" => Some(r"rmdir-about = Remove the DIRECTORY(ies), if they are empty.
rmdir-usage = rmdir [OPTION]... DIRECTORY...

# Help messages
rmdir-help-ignore-fail-non-empty = ignore each failure that is solely because a directory is non-empty
rmdir-help-parents = remove DIRECTORY and its ancestors; e.g., 'rmdir -p a/b/c' is similar to rmdir a/b/c a/b a
rmdir-help-verbose = output a diagnostic for every directory processed

# Error messages
rmdir-error-symbolic-link-not-followed = failed to remove { $path }: Symbolic link not followed
rmdir-error-failed-to-remove = failed to remove { $path }: { $err }

# Verbose output
rmdir-verbose-removing-directory = { $util_name }: removing directory, { $path }
"),
        // Locale for rmdir (en-US)
        "rmdir/en-US.ftl" => Some(r"rmdir-about = Remove the DIRECTORY(ies), if they are empty.
rmdir-usage = rmdir [OPTION]... DIRECTORY...

# Help messages
rmdir-help-ignore-fail-non-empty = ignore each failure that is solely because a directory is non-empty
rmdir-help-parents = remove DIRECTORY and its ancestors; e.g., 'rmdir -p a/b/c' is similar to rmdir a/b/c a/b a
rmdir-help-verbose = output a diagnostic for every directory processed

# Error messages
rmdir-error-symbolic-link-not-followed = failed to remove { $path }: Symbolic link not followed
rmdir-error-failed-to-remove = failed to remove { $path }: { $err }

# Verbose output
rmdir-verbose-removing-directory = { $util_name }: removing directory, { $path }
"),
        // Locale for seq (en-US)
        "seq/en-US.ftl" => Some(r"seq-about = Display numbers from FIRST to LAST, in steps of INCREMENT.
seq-usage = seq [OPTION]... LAST
  seq [OPTION]... FIRST LAST
  seq [OPTION]... FIRST INCREMENT LAST

# Help messages
seq-help-separator = Separator character (defaults to \n)
seq-help-terminator = Terminator character (defaults to \n)
seq-help-equal-width = Equalize widths of all numbers by padding with zeros
seq-help-format = use printf style floating-point FORMAT

# Error messages
seq-error-parse = invalid { $type } argument: { $arg }
seq-error-zero-increment = invalid Zero increment value: { $arg }
seq-error-no-arguments = missing operand
seq-error-format-and-equal-width = format string may not be specified when printing equal width strings

# Parse error types
seq-parse-error-type-float = floating point
seq-parse-error-type-nan = 'not-a-number'
"),
        // Locale for seq (en-US)
        "seq/en-US.ftl" => Some(r"seq-about = Display numbers from FIRST to LAST, in steps of INCREMENT.
seq-usage = seq [OPTION]... LAST
  seq [OPTION]... FIRST LAST
  seq [OPTION]... FIRST INCREMENT LAST

# Help messages
seq-help-separator = Separator character (defaults to \n)
seq-help-terminator = Terminator character (defaults to \n)
seq-help-equal-width = Equalize widths of all numbers by padding with zeros
seq-help-format = use printf style floating-point FORMAT

# Error messages
seq-error-parse = invalid { $type } argument: { $arg }
seq-error-zero-increment = invalid Zero increment value: { $arg }
seq-error-no-arguments = missing operand
seq-error-format-and-equal-width = format string may not be specified when printing equal width strings

# Parse error types
seq-parse-error-type-float = floating point
seq-parse-error-type-nan = 'not-a-number'
"),
        // Locale for sha1sum (en-US)
        "sha1sum/en-US.ftl" => Some(r"sha1sum-about = Print or check the SHA1 checksums
sha1sum-usage = sha1sum [OPTIONS] [FILE]...
"),
        // Locale for sha1sum (en-US)
        "sha1sum/en-US.ftl" => Some(r"sha1sum-about = Print or check the SHA1 checksums
sha1sum-usage = sha1sum [OPTIONS] [FILE]...
"),
        // Locale for sha224sum (en-US)
        "sha224sum/en-US.ftl" => Some(r"sha224sum-about = Print or check the SHA224 checksums
sha224sum-usage = sha224sum [OPTIONS] [FILE]...
"),
        // Locale for sha224sum (en-US)
        "sha224sum/en-US.ftl" => Some(r"sha224sum-about = Print or check the SHA224 checksums
sha224sum-usage = sha224sum [OPTIONS] [FILE]...
"),
        // Locale for sha256sum (en-US)
        "sha256sum/en-US.ftl" => Some(r"sha256sum-about = Print or check the SHA256 checksums
sha256sum-usage = sha256sum [OPTIONS] [FILE]...
"),
        // Locale for sha256sum (en-US)
        "sha256sum/en-US.ftl" => Some(r"sha256sum-about = Print or check the SHA256 checksums
sha256sum-usage = sha256sum [OPTIONS] [FILE]...
"),
        // Locale for sha384sum (en-US)
        "sha384sum/en-US.ftl" => Some(r"sha384sum-about = Print or check the SHA384 checksums
sha384sum-usage = sha384sum [OPTIONS] [FILE]...
"),
        // Locale for sha384sum (en-US)
        "sha384sum/en-US.ftl" => Some(r"sha384sum-about = Print or check the SHA384 checksums
sha384sum-usage = sha384sum [OPTIONS] [FILE]...
"),
        // Locale for sha512sum (en-US)
        "sha512sum/en-US.ftl" => Some(r"sha512sum-about = Print or check the SHA512 checksums
sha512sum-usage = sha512sum [OPTIONS] [FILE]...
"),
        // Locale for sha512sum (en-US)
        "sha512sum/en-US.ftl" => Some(r"sha512sum-about = Print or check the SHA512 checksums
sha512sum-usage = sha512sum [OPTIONS] [FILE]...
"),
        // Locale for shred (en-US)
        "shred/en-US.ftl" => Some(r"shred-about = Overwrite the specified FILE(s) repeatedly, in order to make it harder for even
  very expensive hardware probing to recover the data.
shred-usage = shred [OPTION]... FILE...
shred-after-help = Delete FILE(s) if --remove (-u) is specified. The default is not to remove
  the files because it is common to operate on device files like /dev/hda, and
  those files usually should not be removed.

  CAUTION: Note that shred relies on a very important assumption: that the file
  system overwrites data in place. This is the traditional way to do things, but
  many modern file system designs do not satisfy this assumption. The following
  are examples of file systems on which shred is not effective, or is not
  guaranteed to be effective in all file system modes:

   - log-structured or journal file systems, such as those supplied with
     AIX and Solaris (and JFS, ReiserFS, XFS, Ext3, etc.)

   - file systems that write redundant data and carry on even if some writes
     fail, such as RAID-based file systems

   - file systems that make snapshots, such as Network Appliance's NFS server

   - file systems that cache in temporary locations, such as NFS
     version 3 clients

   - compressed file systems

  In the case of ext3 file systems, the above disclaimer applies (and shred is
  thus of limited effectiveness) only in data=journal mode, which journals file
  data in addition to just metadata. In both the data=ordered (default) and
  data=writeback modes, shred works as usual. Ext3 journal modes can be changed
  by adding the data=something option to the mount options for a particular
  file system in the /etc/fstab file, as documented in the mount man page (`man
  mount`).

  In addition, file system backups and remote mirrors may contain copies of
  the file that cannot be removed, and that will allow a shredded file to be
  recovered later.

# Error messages
shred-missing-file-operand = missing file operand
shred-invalid-number-of-passes = invalid number of passes: {$passes}
shred-cannot-open-random-source = cannot open random source: {$source}
shred-invalid-file-size = invalid file size: {$size}
shred-no-such-file-or-directory = {$file}: No such file or directory
shred-failed-to-open-for-writing-not-a-directory = {$file}: failed to open for writing: Not a directory
shred-failed-to-open-for-writing-is-a-directory = {$file}: failed to open for writing: Is a directory
shred-not-a-file = {$file}: Not a file

# Option help text
shred-force-help = change permissions to allow writing if necessary
shred-iterations-help = overwrite N times instead of the default (3)
shred-size-help = shred this many bytes (suffixes like K, M, G accepted)
shred-deallocate-help = deallocate and remove file after overwriting
shred-remove-help = like -u but give control on HOW to delete;  See below
shred-verbose-help = show progress
shred-exact-help = do not round file sizes up to the next full block;
                   this is the default for non-regular files
shred-zero-help = add a final overwrite with zeros to hide shredding
shred-random-source-help = take random bytes from FILE

# Verbose messages
shred-removing = {$file}: removing
shred-removed = {$file}: removed
shred-renamed-to = renamed to
shred-pass-progress = {$file}: pass
shred-couldnt-rename = {$file}: Couldn't rename to {$new_name}: {$error}
shred-failed-to-open-for-writing = {$file}: failed to open for writing
shred-file-write-pass-failed = {$file}: File write pass failed
shred-failed-to-remove-file = {$file}: failed to remove file

# File I/O error messages
shred-failed-to-clone-file-handle = failed to clone file handle
shred-failed-to-seek-file = failed to seek in file
shred-failed-to-read-seed-bytes = failed to read seed bytes from file
shred-failed-to-get-metadata = failed to get file metadata
shred-failed-to-set-permissions = failed to set file permissions
"),
        // Locale for shred (en-US)
        "shred/en-US.ftl" => Some(r"shred-about = Overwrite the specified FILE(s) repeatedly, in order to make it harder for even
  very expensive hardware probing to recover the data.
shred-usage = shred [OPTION]... FILE...
shred-after-help = Delete FILE(s) if --remove (-u) is specified. The default is not to remove
  the files because it is common to operate on device files like /dev/hda, and
  those files usually should not be removed.

  CAUTION: Note that shred relies on a very important assumption: that the file
  system overwrites data in place. This is the traditional way to do things, but
  many modern file system designs do not satisfy this assumption. The following
  are examples of file systems on which shred is not effective, or is not
  guaranteed to be effective in all file system modes:

   - log-structured or journal file systems, such as those supplied with
     AIX and Solaris (and JFS, ReiserFS, XFS, Ext3, etc.)

   - file systems that write redundant data and carry on even if some writes
     fail, such as RAID-based file systems

   - file systems that make snapshots, such as Network Appliance's NFS server

   - file systems that cache in temporary locations, such as NFS
     version 3 clients

   - compressed file systems

  In the case of ext3 file systems, the above disclaimer applies (and shred is
  thus of limited effectiveness) only in data=journal mode, which journals file
  data in addition to just metadata. In both the data=ordered (default) and
  data=writeback modes, shred works as usual. Ext3 journal modes can be changed
  by adding the data=something option to the mount options for a particular
  file system in the /etc/fstab file, as documented in the mount man page (`man
  mount`).

  In addition, file system backups and remote mirrors may contain copies of
  the file that cannot be removed, and that will allow a shredded file to be
  recovered later.

# Error messages
shred-missing-file-operand = missing file operand
shred-invalid-number-of-passes = invalid number of passes: {$passes}
shred-cannot-open-random-source = cannot open random source: {$source}
shred-invalid-file-size = invalid file size: {$size}
shred-no-such-file-or-directory = {$file}: No such file or directory
shred-not-a-file = {$file}: Not a file

# Option help text
shred-force-help = change permissions to allow writing if necessary
shred-iterations-help = overwrite N times instead of the default (3)
shred-size-help = shred this many bytes (suffixes like K, M, G accepted)
shred-deallocate-help = deallocate and remove file after overwriting
shred-remove-help = like -u but give control on HOW to delete;  See below
shred-verbose-help = show progress
shred-exact-help = do not round file sizes up to the next full block;
                   this is the default for non-regular files
shred-zero-help = add a final overwrite with zeros to hide shredding
shred-random-source-help = take random bytes from FILE

# Verbose messages
shred-removing = {$file}: removing
shred-removed = {$file}: removed
shred-renamed-to = renamed to
shred-pass-progress = {$file}: pass
shred-couldnt-rename = {$file}: Couldn't rename to {$new_name}: {$error}
shred-failed-to-open-for-writing = {$file}: failed to open for writing
shred-file-write-pass-failed = {$file}: File write pass failed
shred-failed-to-remove-file = {$file}: failed to remove file

# File I/O error messages
shred-failed-to-clone-file-handle = failed to clone file handle
shred-failed-to-seek-file = failed to seek in file
shred-failed-to-read-seed-bytes = failed to read seed bytes from file
shred-failed-to-get-metadata = failed to get file metadata
shred-failed-to-set-permissions = failed to set file permissions
"),
        // Locale for shuf (en-US)
        "shuf/en-US.ftl" => Some(r"shuf-about = Shuffle the input by outputting a random permutation of input lines.
  Each output permutation is equally likely.
  With no FILE, or when FILE is -, read standard input.
shuf-usage = shuf [OPTION]... [FILE]
  shuf -e [OPTION]... [ARG]...
  shuf -i LO-HI [OPTION]...

# Help messages
shuf-help-echo = treat each ARG as an input line
shuf-help-input-range = treat each number LO through HI as an input line
shuf-help-head-count = output at most COUNT lines
shuf-help-output = write result to FILE instead of standard output
shuf-help-random-seed = seed with STRING for reproducible output
shuf-help-random-source = get random bytes from FILE
shuf-help-repeat = output lines can be repeated
shuf-help-zero-terminated = line delimiter is NUL, not newline

# Error messages
shuf-error-unexpected-argument = unexpected argument { $arg } found
shuf-error-failed-to-open-for-writing = failed to open { $file } for writing
shuf-error-failed-to-open-random-source = failed to open random source { $file }
shuf-error-read-error = read error
shuf-error-read-random-bytes = reading random bytes failed
shuf-error-end-of-random-bytes = end of random source
shuf-error-no-lines-to-repeat = no lines to repeat
shuf-error-start-exceeds-end = start exceeds end
shuf-error-missing-dash = missing '-'
shuf-error-write-failed = write failed
shuf-error-memory-exhausted = memory exhausted
"),
        // Locale for shuf (en-US)
        "shuf/en-US.ftl" => Some(r"shuf-about = Shuffle the input by outputting a random permutation of input lines.
  Each output permutation is equally likely.
  With no FILE, or when FILE is -, read standard input.
shuf-usage = shuf [OPTION]... [FILE]
  shuf -e [OPTION]... [ARG]...
  shuf -i LO-HI [OPTION]...

# Help messages
shuf-help-echo = treat each ARG as an input line
shuf-help-input-range = treat each number LO through HI as an input line
shuf-help-head-count = output at most COUNT lines
shuf-help-output = write result to FILE instead of standard output
shuf-help-random-seed = seed with STRING for reproducible output
shuf-help-random-source = get random bytes from FILE
shuf-help-repeat = output lines can be repeated
shuf-help-zero-terminated = line delimiter is NUL, not newline

# Error messages
shuf-error-unexpected-argument = unexpected argument { $arg } found
shuf-error-failed-to-open-for-writing = failed to open { $file } for writing
shuf-error-failed-to-open-random-source = failed to open random source { $file }
shuf-error-read-error = read error
shuf-error-read-random-bytes = reading random bytes failed
shuf-error-end-of-random-bytes = end of random source
shuf-error-no-lines-to-repeat = no lines to repeat
shuf-error-start-exceeds-end = start exceeds end
shuf-error-missing-dash = missing '-'
shuf-error-write-failed = write failed
"),
        // Locale for sleep (en-US)
        "sleep/en-US.ftl" => Some(r"sleep-about = Pause for NUMBER seconds.
sleep-usage = sleep NUMBER[SUFFIX]...
  sleep OPTION
sleep-after-help = Pause for NUMBER seconds. SUFFIX may be 's' for seconds (the default),
  'm' for minutes, 'h' for hours or 'd' for days. Unlike most implementations
  that require NUMBER be an integer, here NUMBER may be an arbitrary floating
  point number. Given two or more arguments, pause for the amount of time
  specified by the sum of their values.

# Error messages
sleep-error-missing-operand = missing operand
  Try '{ $program } --help' for more information.

# Help messages
sleep-help-number = pause for NUMBER seconds
"),
        // Locale for sleep (en-US)
        "sleep/en-US.ftl" => Some(r"sleep-about = Pause for NUMBER seconds.
sleep-usage = sleep NUMBER[SUFFIX]...
  sleep OPTION
sleep-after-help = Pause for NUMBER seconds. SUFFIX may be 's' for seconds (the default),
  'm' for minutes, 'h' for hours or 'd' for days. Unlike most implementations
  that require NUMBER be an integer, here NUMBER may be an arbitrary floating
  point number. Given two or more arguments, pause for the amount of time
  specified by the sum of their values.

# Error messages
sleep-error-missing-operand = missing operand
  Try '{ $program } --help' for more information.

# Help messages
sleep-help-number = pause for NUMBER seconds
"),
        // Locale for sort (en-US)
        "sort/en-US.ftl" => Some(r"sort-about = Display sorted concatenation of all FILE(s). With no FILE, or when FILE is -, read standard input.
sort-usage = sort [OPTION]... [FILE]...
sort-after-help = The key format is FIELD[.CHAR][OPTIONS][,FIELD[.CHAR]][OPTIONS].

  Fields by default are separated by the first whitespace after a non-whitespace character. Use -t to specify a custom separator.
  In the default case, whitespace is appended at the beginning of each field. Custom separators however are not included in fields.

  FIELD and CHAR both start at 1 (i.e. they are 1-indexed). If there is no end specified after a comma, the end will be the end of the line.
  If CHAR is set 0, it means the end of the field. CHAR defaults to 1 for the start position and to 0 for the end position.

  Valid options are: MbdfhnRrV. They override the global options for this key.

# Error messages
sort-open-failed = open failed: {$path}: {$error}
sort-parse-key-error = failed to parse key {$key}: {$msg}
sort-cannot-read = cannot read: {$path}: {$error}
sort-open-tmp-file-failed = failed to open temporary file: {$error}
sort-compress-prog-execution-failed = could not run compress program '{$prog}': {$error}
sort-compress-prog-terminated-abnormally = {$prog} terminated abnormally
sort-cannot-create-tmp-file = cannot create temporary file in {$path}:
sort-file-operands-combined = extra operand {$file}
    file operands cannot be combined with --files0-from
    Try '{$help} --help' for more information.
sort-multiple-output-files = multiple output files specified
sort-minus-in-stdin = when reading file names from standard input, no file name of '-' allowed
sort-no-input-from = no input from {$file}
sort-invalid-zero-length-filename = {$file}:{$line_num}: invalid zero-length file name
sort-options-incompatible = options '-{$opt1}{$opt2}' are incompatible
sort-invalid-key = invalid key {$key}
sort-failed-parse-field-index = failed to parse field index {$field} {$error}
sort-field-index-cannot-be-zero = field index can not be 0
sort-failed-parse-char-index = failed to parse character index {$char}: {$error}
sort-invalid-option = invalid option: '{$option}'
sort-invalid-char-index-zero-start = invalid character index 0 for the start position of a field
sort-invalid-field-spec = {$msg}: invalid field specification {$spec}
sort-invalid-count-at-start-of = invalid count at start of {$string}
sort-invalid-number-at-field-start = invalid number at field start
sort-invalid-number-after-dash = invalid number after '-'
sort-invalid-number-after-dot = invalid number after '.'
sort-invalid-number-after-comma = invalid number after ','
sort-field-number-is-zero = field number is zero
sort-character-offset-is-zero = character offset is zero
sort-stray-character-field-spec = stray character in field spec
sort-invalid-batch-size-arg = invalid --batch-size argument '{$arg}'
sort-minimum-batch-size-two = minimum --batch-size argument is '2'
sort-batch-size-too-large = --batch-size argument {$arg} too large
sort-maximum-batch-size-rlimit = maximum --batch-size argument with current rlimit is {$rlimit}
sort-extra-operand-not-allowed-with-c = extra operand {$operand} not allowed with -c
sort-separator-not-valid-unicode = separator is not valid unicode: {$arg}
sort-separator-must-be-one-char = separator must be exactly one character long: {$separator}
sort-only-one-file-allowed-with-c = only one file allowed with -c
sort-failed-fetch-rlimit = Failed to fetch rlimit
sort-invalid-suffix-in-option-arg = invalid suffix in --{$option} argument {$arg}
sort-invalid-option-arg = invalid --{$option} argument {$arg}
sort-option-arg-too-large = --{$option} argument {$arg} too large
sort-error-disorder = {$file}:{$line_number}: disorder: {$line}
sort-error-buffer-size-too-big = Buffer size {$size} does not fit in address space
sort-error-no-match-for-key = ^ no match for key
sort-error-write-failed = write failed: {$output}
sort-failed-to-delete-temporary-directory = failed to delete temporary directory: {$error}
sort-failed-to-set-up-signal-handler = failed to set up signal handler: {$error}

# Warning messages
sort-warning-failed-to-set-locale = failed to set locale
sort-warning-simple-byte-comparison = text ordering performed using simple byte comparison
sort-warning-sort-rule = text ordering performed using ‘{$locale}’ sorting rules
sort-warning-key-zero-width = key {$key} has zero width and will be ignored
sort-warning-key-numeric-spans-fields = key {$key} is numeric and spans multiple fields
sort-warning-leading-blanks-significant = leading blanks are significant in key {$key}; consider also specifying 'b'
sort-warning-numbers-use-decimal-point = numbers use '.' as a decimal point in this locale
sort-warning-options-ignored = options '-{$options}' are ignored
sort-warning-option-ignored = option '-{$option}' is ignored
sort-warning-option-reverse-last-resort = option '-r' only applies to last-resort comparison
sort-warning-obsolescent-key = obsolescent key '{$key}' used; consider '-k {$replacement}' instead
sort-warning-separator-grouping = field separator '{$sep}' is treated as a group separator in numbers
sort-warning-separator-decimal = field separator '{$sep}' is treated as a decimal point in numbers
sort-warning-separator-minus = field separator '{$sep}' is treated as a minus sign in numbers
sort-warning-separator-plus = field separator '{$sep}' is treated as a plus sign in numbers

# Help messages
sort-help-help = Print help information.
sort-help-version = Print version information.
sort-help-human-numeric = compare according to human readable sizes, eg 1M > 100k
sort-help-month = compare according to month name abbreviation
sort-help-numeric = compare according to string numerical value
sort-help-general-numeric = compare according to string general numerical value
sort-help-version-sort = Sort by SemVer version number, eg 1.12.2 > 1.1.2
sort-help-random = shuffle in random order
sort-help-random-source = use FILE as a source of random data
sort-help-dictionary-order = consider only blanks and alphanumeric characters
sort-help-merge = merge already sorted files; do not sort
sort-help-check = check for sorted input; do not sort
sort-help-check-silent = exit successfully if the given file is already sorted, and exit with status 1 otherwise.
sort-help-ignore-case = fold lower case to upper case characters
sort-help-ignore-nonprinting = ignore nonprinting characters
sort-help-ignore-leading-blanks = ignore leading blanks when finding sort keys in each line
sort-help-output = write output to FILENAME instead of stdout
sort-help-reverse = reverse the output
sort-help-stable = stabilize sort by disabling last-resort comparison
sort-help-unique = output only the first of an equal run
sort-help-key = sort by a key
sort-help-separator = custom separator for -k
sort-help-zero-terminated = line delimiter is NUL, not newline
sort-help-parallel = change the number of threads running concurrently to NUM_THREADS
sort-help-buf-size = sets the maximum SIZE of each segment in number of sorted items
sort-help-tmp-dir = use DIR for temporaries, not $TMPDIR or /tmp
sort-help-compress-prog = compress temporary files with PROG, decompress with PROG -d; PROG has to take input from stdin and output to stdout
sort-help-batch-size = Merge at most N_MERGE inputs at once.
sort-help-files0-from = read input from the files specified by NUL-terminated NUL_FILE
sort-help-debug = underline the parts of the line that are actually used for sorting
"),
        // Locale for sort (en-US)
        "sort/en-US.ftl" => Some(r"sort-about = Display sorted concatenation of all FILE(s). With no FILE, or when FILE is -, read standard input.
sort-usage = sort [OPTION]... [FILE]...
sort-after-help = The key format is FIELD[.CHAR][OPTIONS][,FIELD[.CHAR]][OPTIONS].

  Fields by default are separated by the first whitespace after a non-whitespace character. Use -t to specify a custom separator.
  In the default case, whitespace is appended at the beginning of each field. Custom separators however are not included in fields.

  FIELD and CHAR both start at 1 (i.e. they are 1-indexed). If there is no end specified after a comma, the end will be the end of the line.
  If CHAR is set 0, it means the end of the field. CHAR defaults to 1 for the start position and to 0 for the end position.

  Valid options are: MbdfhnRrV. They override the global options for this key.

# Error messages
sort-open-failed = open failed: {$path}: {$error}
sort-parse-key-error = failed to parse key {$key}: {$msg}
sort-cannot-read = cannot read: {$path}: {$error}
sort-open-tmp-file-failed = failed to open temporary file: {$error}
sort-compress-prog-execution-failed = could not run compress program '{$prog}': {$error}
sort-compress-prog-terminated-abnormally = {$prog} terminated abnormally
sort-cannot-create-tmp-file = cannot create temporary file in {$path}:
sort-file-operands-combined = extra operand {$file}
    file operands cannot be combined with --files0-from
    Try '{$help} --help' for more information.
sort-multiple-output-files = multiple output files specified
sort-minus-in-stdin = when reading file names from standard input, no file name of '-' allowed
sort-no-input-from = no input from {$file}
sort-invalid-zero-length-filename = {$file}:{$line_num}: invalid zero-length file name
sort-options-incompatible = options '-{$opt1}{$opt2}' are incompatible
sort-invalid-key = invalid key {$key}
sort-failed-parse-field-index = failed to parse field index {$field} {$error}
sort-field-index-cannot-be-zero = field index can not be 0
sort-failed-parse-char-index = failed to parse character index {$char}: {$error}
sort-invalid-option = invalid option: '{$option}'
sort-invalid-char-index-zero-start = invalid character index 0 for the start position of a field
sort-invalid-field-spec = {$msg}: invalid field specification {$spec}
sort-invalid-count-at-start-of = invalid count at start of {$string}
sort-invalid-number-at-field-start = invalid number at field start
sort-invalid-number-after-dash = invalid number after '-'
sort-invalid-number-after-dot = invalid number after '.'
sort-invalid-number-after-comma = invalid number after ','
sort-field-number-is-zero = field number is zero
sort-character-offset-is-zero = character offset is zero
sort-stray-character-field-spec = stray character in field spec
sort-invalid-batch-size-arg = invalid --batch-size argument '{$arg}'
sort-minimum-batch-size-two = minimum --batch-size argument is '2'
sort-batch-size-too-large = --batch-size argument {$arg} too large
sort-maximum-batch-size-rlimit = maximum --batch-size argument with current rlimit is {$rlimit}
sort-extra-operand-not-allowed-with-c = extra operand {$operand} not allowed with -c
sort-separator-not-valid-unicode = separator is not valid unicode: {$arg}
sort-separator-must-be-one-char = separator must be exactly one character long: {$separator}
sort-only-one-file-allowed-with-c = only one file allowed with -c
sort-failed-fetch-rlimit = Failed to fetch rlimit
sort-invalid-suffix-in-option-arg = invalid suffix in --{$option} argument {$arg}
sort-invalid-option-arg = invalid --{$option} argument {$arg}
sort-option-arg-too-large = --{$option} argument {$arg} too large
sort-error-disorder = {$file}:{$line_number}: disorder: {$line}
sort-error-buffer-size-too-big = Buffer size {$size} does not fit in address space
sort-error-no-match-for-key = ^ no match for key
sort-error-write-failed = write failed: {$output}
sort-failed-to-delete-temporary-directory = failed to delete temporary directory: {$error}
sort-failed-to-set-up-signal-handler = failed to set up signal handler: {$error}

# Warning messages
sort-warning-failed-to-set-locale = failed to set locale
sort-warning-simple-byte-comparison = text ordering performed using simple byte comparison
sort-warning-sort-rule = text ordering performed using ‘{$locale}’ sorting rules
sort-warning-key-zero-width = key {$key} has zero width and will be ignored
sort-warning-key-numeric-spans-fields = key {$key} is numeric and spans multiple fields
sort-warning-leading-blanks-significant = leading blanks are significant in key {$key}; consider also specifying 'b'
sort-warning-numbers-use-decimal-point = numbers use '.' as a decimal point in this locale
sort-warning-options-ignored = options '-{$options}' are ignored
sort-warning-option-ignored = option '-{$option}' is ignored
sort-warning-option-reverse-last-resort = option '-r' only applies to last-resort comparison
sort-warning-obsolescent-key = obsolescent key '{$key}' used; consider '-k {$replacement}' instead
sort-warning-separator-grouping = field separator '{$sep}' is treated as a group separator in numbers
sort-warning-separator-decimal = field separator '{$sep}' is treated as a decimal point in numbers
sort-warning-separator-minus = field separator '{$sep}' is treated as a minus sign in numbers
sort-warning-separator-plus = field separator '{$sep}' is treated as a plus sign in numbers

# Help messages
sort-help-help = Print help information.
sort-help-version = Print version information.
sort-help-human-numeric = compare according to human readable sizes, eg 1M > 100k
sort-help-month = compare according to month name abbreviation
sort-help-numeric = compare according to string numerical value
sort-help-general-numeric = compare according to string general numerical value
sort-help-version-sort = Sort by SemVer version number, eg 1.12.2 > 1.1.2
sort-help-random = shuffle in random order
sort-help-random-source = use FILE as a source of random data
sort-help-dictionary-order = consider only blanks and alphanumeric characters
sort-help-merge = merge already sorted files; do not sort
sort-help-check = check for sorted input; do not sort
sort-help-check-silent = exit successfully if the given file is already sorted, and exit with status 1 otherwise.
sort-help-ignore-case = fold lower case to upper case characters
sort-help-ignore-nonprinting = ignore nonprinting characters
sort-help-ignore-leading-blanks = ignore leading blanks when finding sort keys in each line
sort-help-output = write output to FILENAME instead of stdout
sort-help-reverse = reverse the output
sort-help-stable = stabilize sort by disabling last-resort comparison
sort-help-unique = output only the first of an equal run
sort-help-key = sort by a key
sort-help-separator = custom separator for -k
sort-help-zero-terminated = line delimiter is NUL, not newline
sort-help-parallel = change the number of threads running concurrently to NUM_THREADS
sort-help-buf-size = sets the maximum SIZE of each segment in number of sorted items
sort-help-tmp-dir = use DIR for temporaries, not $TMPDIR or /tmp
sort-help-compress-prog = compress temporary files with PROG, decompress with PROG -d; PROG has to take input from stdin and output to stdout
sort-help-batch-size = Merge at most N_MERGE inputs at once.
sort-help-files0-from = read input from the files specified by NUL-terminated NUL_FILE
sort-help-debug = underline the parts of the line that are actually used for sorting
"),
        // Locale for split (en-US)
        "split/en-US.ftl" => Some(r"split-about = Create output files containing consecutive or interleaved sections of input
split-usage = split [OPTION]... [INPUT [PREFIX]]
split-after-help = Output fixed-size pieces of INPUT to PREFIXaa, PREFIXab, ...; default size is 1000, and default PREFIX is 'x'. With no INPUT, or when INPUT is -, read standard input.

  The SIZE argument is an integer and optional unit (example: 10K is 10*1024).
  Units are K,M,G,T,P,E,Z,Y,R,Q (powers of 1024) or KB,MB,... (powers of 1000).
  Binary prefixes can be used, too: KiB=K, MiB=M, and so on.

  CHUNKS may be:

  - N split into N files based on size of input
  - K/N output Kth of N to stdout
  - l/N split into N files without splitting lines/records
  - l/K/N output Kth of N to stdout without splitting lines/records
  - r/N like 'l' but use round robin distribution
  - r/K/N likewise but only output Kth of N to stdout

# messages
split-creating-file = creating file { $file }
# Error messages
split-error-suffix-not-parsable = invalid suffix length: { $value }
split-error-suffix-contains-separator = invalid suffix { $value }, contains directory separator
split-error-suffix-too-small = the suffix length needs to be at least { $length }
split-error-multi-character-separator = multi-character separator { $separator }
split-error-multiple-separator-characters = multiple separator characters specified
split-error-filter-with-kth-chunk = --filter does not process a chunk extracted to stdout
split-error-invalid-io-block-size = invalid IO block size: { $size }
split-error-not-supported = --filter is currently not supported in this platform
split-error-invalid-number-of-chunks = invalid number of chunks: { $chunks }
split-error-invalid-chunk-number = invalid chunk number: { $chunk }
split-error-invalid-number-of-lines = invalid number of lines: { $error }
split-error-invalid-number-of-bytes = invalid number of bytes: { $error }
split-error-cannot-split-more-than-one-way = cannot split in more than one way
split-error-overflow = Overflow
split-error-output-file-suffixes-exhausted = output file suffixes exhausted
split-error-numerical-suffix-start-too-large = numerical suffix start value is too large for the suffix length
split-error-cannot-open-for-reading = cannot open { $file } for reading
split-error-would-overwrite-input = { $file } would overwrite input; aborting
split-error-cannot-determine-input-size = { $input }: cannot determine input size
split-error-cannot-determine-file-size = { $input }: cannot determine file size
split-error-cannot-read-from-input = { $input }: cannot read from input : { $error }
split-error-unable-to-open-file = unable to open { $file }; aborting
split-error-unable-to-reopen-file = unable to re-open { $file }; aborting
split-error-file-descriptor-limit = at file descriptor limit, but no file descriptor left to close. Closed { $count } writers before.
split-error-shell-process-returned = Shell process returned { $code }
split-error-shell-process-terminated = Shell process terminated by signal

# Help messages for command-line options
split-help-bytes = put SIZE bytes per output file
split-help-line-bytes = put at most SIZE bytes of lines per output file
split-help-lines = put NUMBER lines/records per output file
split-help-number = generate CHUNKS output files; see explanation below
split-help-additional-suffix = additional SUFFIX to append to output file names
split-help-filter = write to shell COMMAND; file name is $FILE (Currently not implemented for Windows)
split-help-elide-empty-files = do not generate empty output files with '-n'
split-help-numeric-suffixes-short = use numeric suffixes starting at 0, not alphabetic
split-help-numeric-suffixes = same as -d, but allow setting the start value
split-help-hex-suffixes-short = use hex suffixes starting at 0, not alphabetic
split-help-hex-suffixes = same as -x, but allow setting the start value
split-help-suffix-length = generate suffixes of length N (default 2)
split-help-verbose = print a diagnostic just before each output file is opened
split-help-separator = use SEP instead of newline as the record separator; '\\0' (zero) specifies the NUL character
"),
        // Locale for split (en-US)
        "split/en-US.ftl" => Some(r"split-about = Create output files containing consecutive or interleaved sections of input
split-usage = split [OPTION]... [INPUT [PREFIX]]
split-after-help = Output fixed-size pieces of INPUT to PREFIXaa, PREFIXab, ...; default size is 1000, and default PREFIX is 'x'. With no INPUT, or when INPUT is -, read standard input.

  The SIZE argument is an integer and optional unit (example: 10K is 10*1024).
  Units are K,M,G,T,P,E,Z,Y,R,Q (powers of 1024) or KB,MB,... (powers of 1000).
  Binary prefixes can be used, too: KiB=K, MiB=M, and so on.

  CHUNKS may be:

  - N split into N files based on size of input
  - K/N output Kth of N to stdout
  - l/N split into N files without splitting lines/records
  - l/K/N output Kth of N to stdout without splitting lines/records
  - r/N like 'l' but use round robin distribution
  - r/K/N likewise but only output Kth of N to stdout

# Error messages
split-error-suffix-not-parsable = invalid suffix length: { $value }
split-error-suffix-contains-separator = invalid suffix { $value }, contains directory separator
split-error-suffix-too-small = the suffix length needs to be at least { $length }
split-error-multi-character-separator = multi-character separator { $separator }
split-error-multiple-separator-characters = multiple separator characters specified
split-error-filter-with-kth-chunk = --filter does not process a chunk extracted to stdout
split-error-invalid-io-block-size = invalid IO block size: { $size }
split-error-not-supported = --filter is currently not supported in this platform
split-error-invalid-number-of-chunks = invalid number of chunks: { $chunks }
split-error-invalid-chunk-number = invalid chunk number: { $chunk }
split-error-invalid-number-of-lines = invalid number of lines: { $error }
split-error-invalid-number-of-bytes = invalid number of bytes: { $error }
split-error-cannot-split-more-than-one-way = cannot split in more than one way
split-error-overflow = Overflow
split-error-output-file-suffixes-exhausted = output file suffixes exhausted
split-error-numerical-suffix-start-too-large = numerical suffix start value is too large for the suffix length
split-error-cannot-open-for-reading = cannot open { $file } for reading
split-error-would-overwrite-input = { $file } would overwrite input; aborting
split-error-cannot-determine-input-size = { $input }: cannot determine input size
split-error-cannot-determine-file-size = { $input }: cannot determine file size
split-error-cannot-read-from-input = { $input }: cannot read from input : { $error }
split-error-input-output-error = input/output error
split-error-unable-to-open-file = unable to open { $file }; aborting
split-error-unable-to-reopen-file = unable to re-open { $file }; aborting
split-error-file-descriptor-limit = at file descriptor limit, but no file descriptor left to close. Closed { $count } writers before.
split-error-shell-process-returned = Shell process returned { $code }
split-error-shell-process-terminated = Shell process terminated by signal
split-error-is-a-directory = { $dir }: Is a directory

# Help messages for command-line options
split-help-bytes = put SIZE bytes per output file
split-help-line-bytes = put at most SIZE bytes of lines per output file
split-help-lines = put NUMBER lines/records per output file
split-help-number = generate CHUNKS output files; see explanation below
split-help-additional-suffix = additional SUFFIX to append to output file names
split-help-filter = write to shell COMMAND; file name is $FILE (Currently not implemented for Windows)
split-help-elide-empty-files = do not generate empty output files with '-n'
split-help-numeric-suffixes-short = use numeric suffixes starting at 0, not alphabetic
split-help-numeric-suffixes = same as -d, but allow setting the start value
split-help-hex-suffixes-short = use hex suffixes starting at 0, not alphabetic
split-help-hex-suffixes = same as -x, but allow setting the start value
split-help-suffix-length = generate suffixes of length N (default 2)
split-help-verbose = print a diagnostic just before each output file is opened
split-help-separator = use SEP instead of newline as the record separator; '\\0' (zero) specifies the NUL character
"),
        // Locale for sum (en-US)
        "sum/en-US.ftl" => Some(r"sum-about = Checksum and count the blocks in a file.

  With no FILE, or when FILE is -, read standard input.
sum-usage = sum [OPTION]... [FILE]...

# Help messages
sum-help-bsd-compatible = use the BSD sum algorithm, use 1K blocks (default)
sum-help-sysv-compatible = use System V sum algorithm, use 512 bytes blocks

# Error messages
sum-error-is-directory = { $name }: Is a directory
sum-error-no-such-file-or-directory = { $name }: No such file or directory
sum-error-not-a-directory = { $name }: Not a directory
"),
        // Locale for sum (en-US)
        "sum/en-US.ftl" => Some(r"sum-about = Checksum and count the blocks in a file.

  With no FILE, or when FILE is -, read standard input.
sum-usage = sum [OPTION]... [FILE]...

# Help messages
sum-help-bsd-compatible = use the BSD sum algorithm, use 1K blocks (default)
sum-help-sysv-compatible = use System V sum algorithm, use 512 bytes blocks

# Error messages
sum-error-is-directory = { $name }: Is a directory
sum-error-no-such-file-or-directory = { $name }: No such file or directory
"),
        // Locale for sync (en-US)
        "sync/en-US.ftl" => Some(r"sync-about = Synchronize cached writes to persistent storage
sync-usage = sync [OPTION]... FILE...

# Help messages
sync-help-file-system = sync the file systems that contain the files (Linux and Windows only)
sync-help-data = sync only file data, no unneeded metadata (Linux only)

# Error messages
sync-error-data-needs-argument = --data needs at least one argument
sync-error-opening-file = error opening { $file }: { $err }
sync-error-no-such-file = error opening { $file }: No such file or directory
sync-error-syncing-file = error syncing { $file }

# Warning messages
sync-warning-fcntl-failed = warning: failed to reset O_NONBLOCK flag for { $file }: { $error }

# Windows-specific error messages
sync-error-flush-file-buffer = failed to flush file buffer
sync-error-create-volume-handle = failed to create volume handle
sync-error-find-first-volume = failed to find first volume
sync-error-find-next-volume = failed to find next volume
"),
        // Locale for sync (en-US)
        "sync/en-US.ftl" => Some(r"sync-about = Synchronize cached writes to persistent storage
sync-usage = sync [OPTION]... FILE...

# Help messages
sync-help-file-system = sync the file systems that contain the files (Linux and Windows only)
sync-help-data = sync only file data, no unneeded metadata (Linux only)

# Error messages
sync-error-data-needs-argument = --data needs at least one argument
sync-error-opening-file = error opening { $file }: { $err }
sync-error-no-such-file = error opening { $file }: No such file or directory
sync-error-syncing-file = error syncing { $file }

# Warning messages
sync-warning-fcntl-failed = warning: failed to reset O_NONBLOCK flag for { $file }: { $error }

# Windows-specific error messages
sync-error-flush-file-buffer = failed to flush file buffer
sync-error-create-volume-handle = failed to create volume handle
sync-error-find-first-volume = failed to find first volume
sync-error-find-next-volume = failed to find next volume
"),
        // Locale for tac (en-US)
        "tac/en-US.ftl" => Some(r"tac-about = Write each file to standard output, last line first.
tac-usage = tac [OPTION]... [FILE]...
tac-help-before = attach the separator before instead of after
tac-help-regex = interpret the sequence as a regular expression
tac-help-separator = use STRING as the separator instead of newline

# Error messages
tac-error-invalid-regex = invalid regular expression: { $error }
tac-error-open-error = failed to open { $filename } for reading: { $error }
tac-error-read-error = { $filename }: read error: { $error }
tac-error-write-error = failed to write to stdout: { $error }
"),
        // Locale for tac (en-US)
        "tac/en-US.ftl" => Some(r"tac-about = Write each file to standard output, last line first.
tac-usage = tac [OPTION]... [FILE]...
tac-help-before = attach the separator before instead of after
tac-help-regex = interpret the sequence as a regular expression
tac-help-separator = use STRING as the separator instead of newline

# Error messages
tac-error-invalid-regex = invalid regular expression: { $error }
tac-error-open-error = failed to open { $filename } for reading: { $error }
tac-error-read-error = { $filename }: read error: { $error }
tac-error-write-error = failed to write to stdout: { $error }
"),
        // Locale for tail (en-US)
        "tail/en-US.ftl" => Some(r"tail-about = Print the last 10 lines of each FILE to standard output.
  With more than one FILE, precede each with a header giving the file name.
  With no FILE, or when FILE is -, read standard input.

  Mandatory arguments to long options are mandatory for short options too.
tail-usage = tail [OPTION]... [FILE]...

# Help messages
tail-help-bytes = Number of bytes to print
tail-help-debug = indicate which --follow implementation is used
tail-help-follow = Print the file as it grows
tail-help-lines = Number of lines to print
tail-help-pid = With -f, terminate after process ID, PID dies
tail-help-quiet = Never output headers giving file names
tail-help-sleep-interval = Number of seconds to sleep between polling the file when running with -f
tail-help-max-unchanged-stats = Reopen a FILE which has not changed size after N (default 5) iterations to see if it has been unlinked or renamed (this is the usual case of rotated log files); This option is meaningful only when polling (i.e., with --use-polling) and when --follow=name
tail-help-verbose = Always output headers giving file names
tail-help-zero-terminated = Line delimiter is NUL, not newline
tail-help-retry = Keep trying to open a file if it is inaccessible
tail-help-follow-retry = Same as --follow=name --retry
tail-help-polling-linux = Disable 'inotify' support and use polling instead
tail-help-polling-unix = Disable 'kqueue' support and use polling instead
tail-help-polling-windows = Disable 'ReadDirectoryChanges' support and use polling instead

# Error messages
tail-error-cannot-follow-stdin-by-name = cannot follow { $stdin } by name
tail-error-cannot-open-no-such-file = cannot open '{ $file }' for reading: { $error }
tail-error-reading-file = error reading '{ $file }': { $error }
tail-error-cannot-follow-file-type = { $file }: cannot follow end of this type of file{ $msg }
tail-error-cannot-open-for-reading = cannot open '{ $file }' for reading
tail-error-cannot-fstat = cannot fstat { $file }: { $error }
tail-error-invalid-number-of-bytes = invalid number of bytes: { $arg }
tail-error-invalid-number-of-lines = invalid number of lines: { $arg }
tail-error-invalid-number-of-seconds = invalid number of seconds: '{ $source }'
tail-error-invalid-max-unchanged-stats = invalid maximum number of unchanged stats between opens: { $value }
tail-error-invalid-pid = invalid PID: { $pid }
tail-error-invalid-pid-with-error = invalid PID: { $pid }: { $error }
tail-error-invalid-number-out-of-range = invalid number: { $arg }: Numerical result out of range
tail-error-invalid-number-overflow = invalid number: { $arg }
tail-error-option-used-in-invalid-context = option used in invalid context -- { $option }
tail-error-bad-argument-encoding = bad argument encoding: { $arg }
tail-error-cannot-watch-parent-directory = cannot watch parent directory of { $path }
tail-error-backend-cannot-be-used-too-many-files = { $backend } cannot be used, reverting to polling: Too many open files
tail-error-backend-resources-exhausted = { $backend } resources exhausted
tail-error-notify-error = NotifyError: { $error }
tail-error-recv-timeout-error = RecvTimeoutError: { $error }

# Warning messages
tail-warning-retry-ignored = --retry ignored; --retry is useful only when following
tail-warning-retry-only-effective = --retry only effective for the initial open
tail-warning-pid-ignored = PID ignored; --pid=PID is useful only when following
tail-warning-pid-not-supported = --pid=PID is not supported on this system
tail-warning-following-stdin-ineffective = following standard input indefinitely is ineffective

# Status messages
tail-status-has-become-accessible = { $file } has become accessible
tail-status-has-appeared-following-new-file = { $file } has appeared;  following new file
tail-status-has-been-replaced-following-new-file = { $file } has been replaced;  following new file
tail-status-file-truncated = { $file }: file truncated
tail-status-replaced-with-untailable-file = { $file } has been replaced with an untailable file
tail-status-replaced-with-untailable-symlink = { $file } has been replaced with an untailable symbolic link
tail-status-replaced-with-untailable-file-giving-up = { $file } has been replaced with an untailable file; giving up on this name
tail-status-directory-containing-watched-file-removed = directory containing watched file was removed
tail-status-backend-cannot-be-used-reverting-to-polling = { $backend } cannot be used, reverting to polling

# Text constants
tail-bad-fd = Bad file descriptor
tail-no-such-file-or-directory = No such file or directory
tail-is-a-directory = Is a directory
tail-giving-up-on-this-name = ; giving up on this name
tail-stdin-header = standard input
tail-no-files-remaining = no files remaining
tail-become-inaccessible = has become inaccessible

# Debug messages
tail-debug-using-notification-mode = using notification mode
tail-debug-using-polling-mode = using polling mode
"),
        // Locale for tail (en-US)
        "tail/en-US.ftl" => Some(r"tail-about = Print the last 10 lines of each FILE to standard output.
  With more than one FILE, precede each with a header giving the file name.
  With no FILE, or when FILE is -, read standard input.
  Mandatory arguments to long flags are mandatory for short flags too.
tail-usage = tail [FLAG]... [FILE]...

# Help messages
tail-help-bytes = Number of bytes to print
tail-help-debug = indicate which --follow implementation is used
tail-help-follow = Print the file as it grows
tail-help-lines = Number of lines to print
tail-help-pid = With -f, terminate after process ID, PID dies
tail-help-quiet = Never output headers giving file names
tail-help-sleep-interval = Number of seconds to sleep between polling the file when running with -f
tail-help-max-unchanged-stats = Reopen a FILE which has not changed size after N (default 5) iterations to see if it has been unlinked or renamed (this is the usual case of rotated log files); This option is meaningful only when polling (i.e., with --use-polling) and when --follow=name
tail-help-verbose = Always output headers giving file names
tail-help-zero-terminated = Line delimiter is NUL, not newline
tail-help-retry = Keep trying to open a file if it is inaccessible
tail-help-follow-retry = Same as --follow=name --retry
tail-help-polling-linux = Disable 'inotify' support and use polling instead
tail-help-polling-unix = Disable 'kqueue' support and use polling instead
tail-help-polling-windows = Disable 'ReadDirectoryChanges' support and use polling instead

# Error messages
tail-error-cannot-follow-stdin-by-name = cannot follow { $stdin } by name
tail-error-cannot-open-no-such-file = cannot open '{ $file }' for reading: { $error }
tail-error-reading-file = error reading '{ $file }': { $error }
tail-error-cannot-follow-file-type = { $file }: cannot follow end of this type of file{ $msg }
tail-error-cannot-open-for-reading = cannot open '{ $file }' for reading
tail-error-cannot-fstat = cannot fstat { $file }: { $error }
tail-error-invalid-number-of-bytes = invalid number of bytes: { $arg }
tail-error-invalid-number-of-lines = invalid number of lines: { $arg }
tail-error-invalid-number-of-seconds = invalid number of seconds: '{ $source }'
tail-error-invalid-max-unchanged-stats = invalid maximum number of unchanged stats between opens: { $value }
tail-error-invalid-pid = invalid PID: { $pid }
tail-error-invalid-pid-with-error = invalid PID: { $pid }: { $error }
tail-error-invalid-number-out-of-range = invalid number: { $arg }: Numerical result out of range
tail-error-invalid-number-overflow = invalid number: { $arg }
tail-error-option-used-in-invalid-context = option used in invalid context -- { $option }
tail-error-bad-argument-encoding = bad argument encoding: { $arg }
tail-error-cannot-watch-parent-directory = cannot watch parent directory of { $path }
tail-error-backend-cannot-be-used-too-many-files = { $backend } cannot be used, reverting to polling: Too many open files
tail-error-backend-resources-exhausted = { $backend } resources exhausted
tail-error-notify-error = NotifyError: { $error }
tail-error-recv-timeout-error = RecvTimeoutError: { $error }

# Warning messages
tail-warning-retry-ignored = --retry ignored; --retry is useful only when following
tail-warning-retry-only-effective = --retry only effective for the initial open
tail-warning-pid-ignored = PID ignored; --pid=PID is useful only when following
tail-warning-pid-not-supported = --pid=PID is not supported on this system
tail-warning-following-stdin-ineffective = following standard input indefinitely is ineffective

# Status messages
tail-status-has-become-accessible = { $file } has become accessible
tail-status-has-appeared-following-new-file = { $file } has appeared;  following new file
tail-status-has-been-replaced-following-new-file = { $file } has been replaced;  following new file
tail-status-file-truncated = { $file }: file truncated
tail-status-replaced-with-untailable-file = { $file } has been replaced with an untailable file
tail-status-replaced-with-untailable-file-giving-up = { $file } has been replaced with an untailable file; giving up on this name
tail-status-directory-containing-watched-file-removed = directory containing watched file was removed
tail-status-backend-cannot-be-used-reverting-to-polling = { $backend } cannot be used, reverting to polling

# Text constants
tail-bad-fd = Bad file descriptor
tail-no-such-file-or-directory = No such file or directory
tail-is-a-directory = Is a directory
tail-giving-up-on-this-name = ; giving up on this name
tail-stdin-header = standard input
tail-no-files-remaining = no files remaining
tail-become-inaccessible = has become inaccessible

# Debug messages
tail-debug-using-notification-mode = using notification mode
tail-debug-using-polling-mode = using polling mode
"),
        // Locale for tee (en-US)
        "tee/en-US.ftl" => Some(r"tee-about = Copy standard input to each FILE, and also to standard output.
tee-usage = tee [OPTION]... [FILE]...
tee-after-help = If a FILE is -, it refers to a file named - .

# Help messages
tee-help-help = Print help
tee-help-append = append to the given FILEs, do not overwrite
tee-help-ignore-interrupts = ignore interrupt signals (ignored on non-Unix platforms)
tee-help-ignore-pipe-errors = set write error behavior (ignored on non-Unix platforms)
tee-help-output-error = set write error behavior
tee-help-output-error-warn = produce warnings for errors writing to any output
tee-help-output-error-warn-nopipe = produce warnings for errors that are not pipe errors (ignored on non-unix platforms)
tee-help-output-error-exit = exit on write errors to any output
tee-help-output-error-exit-nopipe = exit on write errors to any output that are not pipe errors (equivalent to exit on non-unix platforms)

# Error messages
tee-error-stdin = read error: { $error }

# Other messages
tee-standard-output = 'standard output'
"),
        // Locale for tee (en-US)
        "tee/en-US.ftl" => Some(r"tee-about = Copy standard input to each FILE, and also to standard output.
tee-usage = tee [OPTION]... [FILE]...
tee-after-help = If a FILE is -, it refers to a file named - .

# Help messages
tee-help-help = Print help
tee-help-append = append to the given FILEs, do not overwrite
tee-help-ignore-interrupts = ignore interrupt signals (ignored on non-Unix platforms)
tee-help-ignore-pipe-errors = set write error behavior (ignored on non-Unix platforms)
tee-help-output-error = set write error behavior
tee-help-output-error-warn = produce warnings for errors writing to any output
tee-help-output-error-warn-nopipe = produce warnings for errors that are not pipe errors (ignored on non-unix platforms)
tee-help-output-error-exit = exit on write errors to any output
tee-help-output-error-exit-nopipe = exit on write errors to any output that are not pipe errors (equivalent to exit on non-unix platforms)

# Error messages
tee-error-stdin = read error: { $error }

# Other messages
tee-standard-output = 'standard output'
"),
        // Locale for test (en-US)
        "test/en-US.ftl" => Some(r#"test-about = Check file types and compare values.
test-usage = test EXPRESSION
  test
  {"[ EXPRESSION ]"}
  {"[ ]"}
  {"[ OPTION"}
test-after-help = Exit with the status determined by EXPRESSION.

  An omitted EXPRESSION defaults to false.
  Otherwise, EXPRESSION is true or false and sets exit status.
  It is one of:

  ( EXPRESSION )
        EXPRESSION is true

  ! EXPRESSION
        EXPRESSION is false

  EXPRESSION1 -a EXPRESSION2
        both EXPRESSION1 and EXPRESSION2 are true

  EXPRESSION1 -o EXPRESSION2
        either EXPRESSION1 or EXPRESSION2 is true

  String operations:

  -n STRING
        the length of STRING is nonzero

  STRING
        equivalent to -n STRING

  -z STRING
        the length of STRING is zero

  STRING1 = STRING2
        the strings are equal

  STRING1 != STRING2
        the strings are not equal

  STRING1 > STRING2
        STRING1 is greater than STRING2 in the current locale

  STRING1 < STRING2
        STRING1 is less than STRING2 in the current locale

  Integer comparisons:

  INTEGER1 -eq INTEGER2
        INTEGER1 is equal to INTEGER2

  INTEGER1 -ge INTEGER2
        INTEGER1 is greater than or equal to INTEGER2

  INTEGER1 -gt INTEGER2
        INTEGER1 is greater than INTEGER2

  INTEGER1 -le INTEGER2
        INTEGER1 is less than or equal to INTEGER2

  INTEGER1 -lt INTEGER2
        INTEGER1 is less than INTEGER2

  INTEGER1 -ne INTEGER2
        INTEGER1 is not equal to INTEGER2

  File operations:

  FILE1 -ef FILE2
        FILE1 and FILE2 have the same device and inode numbers

  FILE1 -nt FILE2
        FILE1 is newer (modification date) than FILE2

  FILE1 -ot FILE2
        FILE1 is older than FILE2

  -b FILE
        FILE exists and is block special

  -c FILE
        FILE exists and is character special

  -d FILE
        FILE exists and is a directory

  -e FILE
        FILE exists

  -f FILE
        FILE exists and is a regular file

  -g FILE
        FILE exists and is set-group-ID

  -G FILE
        FILE exists and is owned by the effective group ID

  -h FILE
        FILE exists and is a symbolic link (same as -L)

  -k FILE
        FILE exists and has its sticky bit set

  -L FILE
        FILE exists and is a symbolic link (same as -h)

  -N FILE
        FILE exists and has been modified since it was last read

  -O FILE
        FILE exists and is owned by the effective user ID

  -p FILE
        FILE exists and is a named pipe

  -r FILE
        FILE exists and read permission is granted

  -s FILE
        FILE exists and has a size greater than zero

  -S FILE
        FILE exists and is a socket

  -t FD
        file descriptor FD is opened on a terminal

  -u FILE
        FILE exists and its set-user-ID bit is set

  -w FILE
        FILE exists and write permission is granted

  -x FILE
        FILE exists and execute (or search) permission is granted

  Except for -h and -L, all FILE-related tests dereference (follow) symbolic links.
  Beware that parentheses need to be escaped (e.g., by backslashes) for shells.
  INTEGER may also be -l STRING, which evaluates to the length of STRING.

  NOTE: Binary -a and -o are inherently ambiguous.
  Use test EXPR1 && test EXPR2 or test EXPR1 || test EXPR2 instead.

  NOTE: {"["} honors the --help and --version options, but test does not.
  test treats each of those as it treats any other nonempty STRING.

  NOTE: your shell may have its own version of test and/or {"["}, which usually supersedes the version described here.
  Please refer to your shell's documentation for details about the options it supports.

# Error messages
test-error-missing-closing-bracket = missing '{"]"}'
test-error-expected = expected { $value }
test-error-expected-value = expected value
test-error-missing-argument = missing argument after { $argument }
test-error-extra-argument = extra argument { $argument }
test-error-unknown-operator = unknown operator { $operator }
test-error-invalid-integer = invalid integer { $value }
test-error-unary-operator-expected = { $operator }: unary operator expected
"#),
        // Locale for test (en-US)
        "test/en-US.ftl" => Some(r#"test-about = Check file types and compare values.
test-usage = test EXPRESSION
  test
  {"[ EXPRESSION ]"}
  {"[ ]"}
  {"[ OPTION"}
test-after-help = Exit with the status determined by EXPRESSION.

  An omitted EXPRESSION defaults to false.
  Otherwise, EXPRESSION is true or false and sets exit status.
  It is one of:

  ( EXPRESSION )
        EXPRESSION is true

  ! EXPRESSION
        EXPRESSION is false

  EXPRESSION1 -a EXPRESSION2
        both EXPRESSION1 and EXPRESSION2 are true

  EXPRESSION1 -o EXPRESSION2
        either EXPRESSION1 or EXPRESSION2 is true

  String operations:

  -n STRING
        the length of STRING is nonzero

  STRING
        equivalent to -n STRING

  -z STRING
        the length of STRING is zero

  STRING1 = STRING2
        the strings are equal

  STRING1 != STRING2
        the strings are not equal

  STRING1 > STRING2
        STRING1 is greater than STRING2 in the current locale

  STRING1 < STRING2
        STRING1 is less than STRING2 in the current locale

  Integer comparisons:

  INTEGER1 -eq INTEGER2
        INTEGER1 is equal to INTEGER2

  INTEGER1 -ge INTEGER2
        INTEGER1 is greater than or equal to INTEGER2

  INTEGER1 -gt INTEGER2
        INTEGER1 is greater than INTEGER2

  INTEGER1 -le INTEGER2
        INTEGER1 is less than or equal to INTEGER2

  INTEGER1 -lt INTEGER2
        INTEGER1 is less than INTEGER2

  INTEGER1 -ne INTEGER2
        INTEGER1 is not equal to INTEGER2

  File operations:

  FILE1 -ef FILE2
        FILE1 and FILE2 have the same device and inode numbers

  FILE1 -nt FILE2
        FILE1 is newer (modification date) than FILE2

  FILE1 -ot FILE2
        FILE1 is older than FILE2

  -b FILE
        FILE exists and is block special

  -c FILE
        FILE exists and is character special

  -d FILE
        FILE exists and is a directory

  -e FILE
        FILE exists

  -f FILE
        FILE exists and is a regular file

  -g FILE
        FILE exists and is set-group-ID

  -G FILE
        FILE exists and is owned by the effective group ID

  -h FILE
        FILE exists and is a symbolic link (same as -L)

  -k FILE
        FILE exists and has its sticky bit set

  -L FILE
        FILE exists and is a symbolic link (same as -h)

  -N FILE
        FILE exists and has been modified since it was last read

  -O FILE
        FILE exists and is owned by the effective user ID

  -p FILE
        FILE exists and is a named pipe

  -r FILE
        FILE exists and read permission is granted

  -s FILE
        FILE exists and has a size greater than zero

  -S FILE
        FILE exists and is a socket

  -t FD
        file descriptor FD is opened on a terminal

  -u FILE
        FILE exists and its set-user-ID bit is set

  -w FILE
        FILE exists and write permission is granted

  -x FILE
        FILE exists and execute (or search) permission is granted

  Except for -h and -L, all FILE-related tests dereference (follow) symbolic links.
  Beware that parentheses need to be escaped (e.g., by backslashes) for shells.
  INTEGER may also be -l STRING, which evaluates to the length of STRING.

  NOTE: Binary -a and -o are inherently ambiguous.
  Use test EXPR1 && test EXPR2 or test EXPR1 || test EXPR2 instead.

  NOTE: {"["} honors the --help and --version options, but test does not.
  test treats each of those as it treats any other nonempty STRING.

  NOTE: your shell may have its own version of test and/or {"["}, which usually supersedes the version described here.
  Please refer to your shell's documentation for details about the options it supports.

# Error messages
test-error-missing-closing-bracket = missing '{"]"}'
test-error-expected = expected { $value }
test-error-expected-value = expected value
test-error-missing-argument = missing argument after { $argument }
test-error-extra-argument = extra argument { $argument }
test-error-unknown-operator = unknown operator { $operator }
test-error-invalid-integer = invalid integer { $value }
test-error-unary-operator-expected = { $operator }: unary operator expected
"#),
        // Locale for touch (en-US)
        "touch/en-US.ftl" => Some(r#"touch-about = Update the access and modification times of each FILE to the current time.
touch-usage = touch [OPTION]... [FILE]...

# Help messages
touch-help-help = Print help information.
touch-help-access = change only the access time
touch-help-timestamp = use [[CC]YY]MMDDhhmm[.ss] instead of the current time
touch-help-date = parse argument and use it instead of current time
touch-help-modification = change only the modification time
touch-help-no-create = do not create any files
touch-help-no-deref = affect each symbolic link instead of any referenced file (only for systems that can change the timestamps of a symlink)
touch-help-reference = use this file's times instead of the current time
touch-help-time = change only the specified time: "access", "atime", or "use" are equivalent to -a; "modify" or "mtime" are equivalent to -m

# Error messages
touch-error-missing-file-operand = missing file operand
  Try '{ $help_command } --help' for more information.
touch-error-setting-times-of = setting times of { $filename }
touch-error-setting-times-no-such-file = setting times of { $filename }: No such file or directory
touch-error-cannot-touch = cannot touch { $filename }
touch-error-no-such-file-or-directory = No such file or directory
touch-error-failed-to-get-attributes = failed to get attributes of { $path }
touch-error-setting-times-of-path = setting times of { $path }
touch-error-invalid-date-format = invalid date format { $date }
touch-error-unable-to-parse-date = Unable to parse date: { $date }
touch-error-windows-stdout-path-failed = GetFinalPathNameByHandleW failed with code { $code }
touch-error-invalid-filetime = Source has invalid access or modification time: { $time }
touch-error-reference-file-inaccessible = failed to get attributes of { $path }: { $error }
"#),
        // Locale for touch (en-US)
        "touch/en-US.ftl" => Some(r#"touch-about = Update the access and modification times of each FILE to the current time.
touch-usage = touch [OPTION]... [FILE]...

# Help messages
touch-help-help = Print help information.
touch-help-access = change only the access time
touch-help-timestamp = use [[CC]YY]MMDDhhmm[.ss] instead of the current time
touch-help-date = parse argument and use it instead of current time
touch-help-modification = change only the modification time
touch-help-no-create = do not create any files
touch-help-no-deref = affect each symbolic link instead of any referenced file (only for systems that can change the timestamps of a symlink)
touch-help-reference = use this file's times instead of the current time
touch-help-time = change only the specified time: "access", "atime", or "use" are equivalent to -a; "modify" or "mtime" are equivalent to -m

# Error messages
touch-error-missing-file-operand = missing file operand
  Try '{ $help_command } --help' for more information.
touch-error-setting-times-of = setting times of { $filename }
touch-error-setting-times-no-such-file = setting times of { $filename }: No such file or directory
touch-error-cannot-touch = cannot touch { $filename }
touch-error-no-such-file-or-directory = No such file or directory
touch-error-failed-to-get-attributes = failed to get attributes of { $path }
touch-error-setting-times-of-path = setting times of { $path }
touch-error-invalid-date-ts-format = invalid date ts format { $date }
touch-error-invalid-date-format = invalid date format { $date }
touch-error-unable-to-parse-date = Unable to parse date: { $date }
touch-error-windows-stdout-path-failed = GetFinalPathNameByHandleW failed with code { $code }
touch-error-invalid-filetime = Source has invalid access or modification time: { $time }
touch-error-reference-file-inaccessible = failed to get attributes of { $path }: { $error }
"#),
        // Locale for tr (en-US)
        "tr/en-US.ftl" => Some(r"tr-about = Translate or delete characters
tr-usage = tr [OPTION]... SET1 [SET2]
tr-after-help = Translate, squeeze, and/or delete characters from standard input, writing to standard output.

# Help messages
tr-help-complement = use the complement of SET1
tr-help-delete = delete characters in SET1, do not translate
tr-help-squeeze = replace each sequence of a repeated character that is listed in the last specified SET, with a single occurrence of that character
tr-help-truncate-set1 = first truncate SET1 to length of SET2

# Error messages
tr-error-missing-operand = missing operand
tr-error-missing-operand-translating = missing operand after { $set }
  Two strings must be given when translating.
tr-error-missing-operand-deleting-squeezing = missing operand after { $set }
  Two strings must be given when deleting and squeezing.
tr-error-extra-operand-deleting-without-squeezing = extra operand { $operand }
  Only one string may be given when deleting without squeezing repeats.
tr-error-extra-operand-simple = extra operand { $operand }
tr-error-read-directory = read error: Is a directory
tr-error-write-error = write error

# Warning messages
tr-warning-unescaped-backslash = warning: an unescaped backslash at end of string is not portable
tr-warning-ambiguous-octal-escape = the ambiguous octal escape \{ $origin_octal } is being interpreted as the 2-byte sequence \0{ $actual_octal_tail }, { $outstand_char }
tr-warning-invalid-utf8 = invalid utf8 sequence

# Sequence parsing error messages
tr-error-missing-char-class-name = missing character class name '[::]'
tr-error-invalid-char-class = invalid character class { $class }
tr-error-missing-equivalence-class-char = missing equivalence class character '[==]'
tr-error-multiple-char-repeat-in-set2 = only one [c*] repeat construct may appear in string2
tr-error-char-repeat-in-set1 = the [c*] repeat construct may not appear in string1
tr-error-invalid-repeat-count = invalid repeat count { $count } in [c*n] construct
tr-error-empty-set2-when-not-truncating = when not truncating set1, string2 must be non-empty
tr-error-class-except-lower-upper-in-set2 = when translating, the only character classes that may appear in set2 are 'upper' and 'lower'
tr-error-class-in-set2-not-matched = when translating, every 'upper'/'lower' in set2 must be matched by a 'upper'/'lower' in the same position in set1
tr-error-set1-longer-set2-ends-in-class = when translating with string1 longer than string2,
  the latter string must not end with a character class
tr-error-complement-more-than-one-unique = when translating with complemented character classes,
  string2 must map all characters in the domain to one
tr-error-backwards-range = range-endpoints of '{ $start }-{ $end }' are in reverse collating sequence order
tr-error-multiple-char-in-equivalence = { $chars }: equivalence class operand must be a single character
"),
        // Locale for tr (en-US)
        "tr/en-US.ftl" => Some(r"tr-about = Translate or delete characters
tr-usage = tr [OPTION]... SET1 [SET2]
tr-after-help = Translate, squeeze, and/or delete characters from standard input, writing to standard output.

# Help messages
tr-help-complement = use the complement of SET1
tr-help-delete = delete characters in SET1, do not translate
tr-help-squeeze = replace each sequence of a repeated character that is listed in the last specified SET, with a single occurrence of that character
tr-help-truncate-set1 = first truncate SET1 to length of SET2

# Error messages
tr-error-missing-operand = missing operand
tr-error-missing-operand-translating = missing operand after { $set }
  Two strings must be given when translating.
tr-error-missing-operand-deleting-squeezing = missing operand after { $set }
  Two strings must be given when deleting and squeezing.
tr-error-extra-operand-deleting-without-squeezing = extra operand { $operand }
  Only one string may be given when deleting without squeezing repeats.
tr-error-extra-operand-simple = extra operand { $operand }
tr-error-read-directory = read error: Is a directory
tr-error-write-error = write error

# Warning messages
tr-warning-unescaped-backslash = warning: an unescaped backslash at end of string is not portable
tr-warning-ambiguous-octal-escape = the ambiguous octal escape \{ $origin_octal } is being interpreted as the 2-byte sequence \0{ $actual_octal_tail }, { $outstand_char }
tr-warning-invalid-utf8 = invalid utf8 sequence

# Sequence parsing error messages
tr-error-missing-char-class-name = missing character class name '[::]'
tr-error-invalid-char-class = invalid character class { $class }
tr-error-missing-equivalence-class-char = missing equivalence class character '[==]'
tr-error-multiple-char-repeat-in-set2 = only one [c*] repeat construct may appear in string2
tr-error-char-repeat-in-set1 = the [c*] repeat construct may not appear in string1
tr-error-invalid-repeat-count = invalid repeat count { $count } in [c*n] construct
tr-error-empty-set2-when-not-truncating = when not truncating set1, string2 must be non-empty
tr-error-class-except-lower-upper-in-set2 = when translating, the only character classes that may appear in set2 are 'upper' and 'lower'
tr-error-class-in-set2-not-matched = when translating, every 'upper'/'lower' in set2 must be matched by a 'upper'/'lower' in the same position in set1
tr-error-set1-longer-set2-ends-in-class = when translating with string1 longer than string2,
  the latter string must not end with a character class
tr-error-complement-more-than-one-unique = when translating with complemented character classes,
  string2 must map all characters in the domain to one
tr-error-backwards-range = range-endpoints of '{ $start }-{ $end }' are in reverse collating sequence order
tr-error-multiple-char-in-equivalence = { $chars }: equivalence class operand must be a single character
"),
        // Locale for true (en-US)
        "true/en-US.ftl" => Some(r"true-about = Returns true, a successful exit status.

  Immediately returns with the exit status 0, except when invoked with one of the recognized
  options. In those cases it will try to write the help or version text. Any IO error during this
  operation causes the program to return 1 instead.

true-help-text = Print help information
true-version-text = Print version information
"),
        // Locale for true (en-US)
        "true/en-US.ftl" => Some(r"true-about = Returns true, a successful exit status.

  Immediately returns with the exit status 0, except when invoked with one of the recognized
  options. In those cases it will try to write the help or version text. Any IO error during this
  operation causes the program to return 1 instead.

true-help-text = Print help information
true-version-text = Print version information
"),
        // Locale for truncate (en-US)
        "truncate/en-US.ftl" => Some(r"truncate-about = Shrink or extend the size of each file to the specified size.
truncate-usage = truncate [OPTION]... [FILE]...
truncate-after-help = SIZE is an integer with an optional prefix and optional unit.
  The available units (K, M, G, T, P, E, Z, and Y) use the following format:
      'KB' => 1000 (kilobytes)
      'K' => 1024 (kibibytes)
      'MB' => 1000*1000 (megabytes)
      'M' => 1024*1024 (mebibytes)
      'GB' => 1000*1000*1000 (gigabytes)
      'G' => 1024*1024*1024 (gibibytes)
  SIZE may also be prefixed by one of the following to adjust the size of each
  file based on its current size:
      '+' => extend by
      '-' => reduce by
      '<' => at most
      '>' => at least
      '/' => round down to multiple of
      '%' => round up to multiple of

# Help messages
truncate-help-io-blocks = treat SIZE as the number of I/O blocks of the file rather than bytes (NOT IMPLEMENTED)
truncate-help-no-create = do not create files that do not exist
truncate-help-reference = base the size of each file on the size of RFILE
truncate-help-size = set or adjust the size of each file according to SIZE, which is in bytes unless --io-blocks is specified

# Error messages
truncate-error-missing-file-operand = missing file operand
truncate-error-cannot-open-no-device = cannot open { $filename } for writing: No such device or address
truncate-error-cannot-open-for-writing = cannot open { $filename } for writing
truncate-error-invalid-number = Invalid number: { $error }
truncate-error-must-specify-relative-size = you must specify a relative '--size' with '--reference'
truncate-error-division-by-zero = division by zero
truncate-error-cannot-stat-no-such-file = cannot stat { $filename }: No such file or directory
truncate-error-value-too-large = Value too large for defined data type
truncate-error-value-too-large-arg = { $arg }: Value too large for defined data type
"),
        // Locale for truncate (en-US)
        "truncate/en-US.ftl" => Some(r"truncate-about = Shrink or extend the size of each file to the specified size.
truncate-usage = truncate [OPTION]... [FILE]...
truncate-after-help = SIZE is an integer with an optional prefix and optional unit.
  The available units (K, M, G, T, P, E, Z, and Y) use the following format:
      'KB' => 1000 (kilobytes)
      'K' => 1024 (kibibytes)
      'MB' => 1000*1000 (megabytes)
      'M' => 1024*1024 (mebibytes)
      'GB' => 1000*1000*1000 (gigabytes)
      'G' => 1024*1024*1024 (gibibytes)
  SIZE may also be prefixed by one of the following to adjust the size of each
  file based on its current size:
      '+' => extend by
      '-' => reduce by
      '<' => at most
      '>' => at least
      '/' => round down to multiple of
      '%' => round up to multiple of

# Help messages
truncate-help-io-blocks = treat SIZE as the number of I/O blocks of the file rather than bytes (NOT IMPLEMENTED)
truncate-help-no-create = do not create files that do not exist
truncate-help-reference = base the size of each file on the size of RFILE
truncate-help-size = set or adjust the size of each file according to SIZE, which is in bytes unless --io-blocks is specified

# Error messages
truncate-error-missing-file-operand = missing file operand
truncate-error-cannot-open-no-device = cannot open { $filename } for writing: No such device or address
truncate-error-cannot-open-for-writing = cannot open { $filename } for writing
truncate-error-invalid-number = Invalid number: { $error }
truncate-error-must-specify-relative-size = you must specify a relative '--size' with '--reference'
truncate-error-division-by-zero = division by zero
truncate-error-cannot-stat-no-such-file = cannot stat { $filename }: No such file or directory
"),
        // Locale for tsort (en-US)
        "tsort/en-US.ftl" => Some(r"tsort-about = Topological sort the strings in FILE.
  Strings are defined as any sequence of tokens separated by whitespace (tab, space, or newline), ordering them based on dependencies in a directed acyclic graph (DAG).
  Useful for scheduling and determining execution order.
  If FILE is not passed in, stdin is used instead.
tsort-usage = tsort [OPTIONS] FILE
tsort-error-is-dir = read error: Is a directory
tsort-error-odd = input contains an odd number of tokens
tsort-error-loop = input contains a loop:
tsort-error-extra-operand = extra operand { $operand }
  Try 'tsort --help' for more information.
"),
        // Locale for tsort (en-US)
        "tsort/en-US.ftl" => Some(r"tsort-about = Topological sort the strings in FILE.
  Strings are defined as any sequence of tokens separated by whitespace (tab, space, or newline), ordering them based on dependencies in a directed acyclic graph (DAG).
  Useful for scheduling and determining execution order.
  If FILE is not passed in, stdin is used instead.
tsort-usage = tsort [OPTIONS] FILE
tsort-error-is-dir = read error: Is a directory
tsort-error-odd = input contains an odd number of tokens
tsort-error-loop = input contains a loop:
tsort-error-extra-operand = extra operand { $operand }
  Try '{ $util } --help' for more information.
tsort-error-at-least-one-input = at least one input
"),
        // Locale for uname (en-US)
        "uname/en-US.ftl" => Some(r"uname-about = Print certain system information.
  With no OPTION, same as -s.
uname-usage = uname [OPTION]...

# Error messages
uname-error-cannot-get-system-name = cannot get system name

# Default values
uname-unknown = unknown

# Help text for command-line arguments
uname-help-all = Behave as though all of the options -mnrsvo were specified.
uname-help-kernel-name = print the kernel name.
uname-help-nodename = print the nodename (the nodename may be a name that the system is known by to a communications network).
uname-help-kernel-release = print the operating system release.
uname-help-kernel-version = print the operating system version.
uname-help-machine = print the machine hardware name.
uname-help-os = print the operating system name.
uname-help-processor = print the processor type (non-portable)
uname-help-hardware-platform = print the hardware platform (non-portable)
"),
        // Locale for uname (en-US)
        "uname/en-US.ftl" => Some(r"uname-about = Print certain system information.
  With no OPTION, same as -s.
uname-usage = uname [OPTION]...

# Error messages
uname-error-cannot-get-system-name = cannot get system name

# Default values
uname-unknown = unknown

# Help text for command-line arguments
uname-help-all = Behave as though all of the options -mnrsvo were specified.
uname-help-kernel-name = print the kernel name.
uname-help-nodename = print the nodename (the nodename may be a name that the system is known by to a communications network).
uname-help-kernel-release = print the operating system release.
uname-help-kernel-version = print the operating system version.
uname-help-machine = print the machine hardware name.
uname-help-os = print the operating system name.
uname-help-processor = print the processor type (non-portable)
uname-help-hardware-platform = print the hardware platform (non-portable)
"),
        // Locale for unexpand (en-US)
        "unexpand/en-US.ftl" => Some(r"unexpand-about = Convert blanks in each FILE to tabs, writing to standard output.
  With no FILE, or when FILE is -, read standard input.
unexpand-usage = unexpand [OPTION]... [FILE]...

# Help messages
unexpand-help-all = convert all blanks, instead of just initial blanks
unexpand-help-first-only = convert only leading sequences of blanks (overrides -a)
unexpand-help-tabs = use comma separated LIST of tab positions or have tabs N characters apart instead of 8 (enables -a)
unexpand-help-no-utf8 = interpret input file as 8-bit ASCII rather than UTF-8

# Error messages
unexpand-error-invalid-character = tab size contains invalid character(s): { $char }
unexpand-error-tab-size-cannot-be-zero = tab size cannot be 0
unexpand-error-tab-size-too-large = tab stop value is too large
unexpand-error-tab-sizes-must-be-ascending = tab sizes must be ascending
unexpand-error-is-directory = { $path }: Is a directory
"),
        // Locale for unexpand (en-US)
        "unexpand/en-US.ftl" => Some(r"unexpand-about = Convert blanks in each FILE to tabs, writing to standard output.
  With no FILE, or when FILE is -, read standard input.
unexpand-usage = unexpand [OPTION]... [FILE]...

# Help messages
unexpand-help-all = convert all blanks, instead of just initial blanks
unexpand-help-first-only = convert only leading sequences of blanks (overrides -a)
unexpand-help-tabs = use comma separated LIST of tab positions or have tabs N characters apart instead of 8 (enables -a)
unexpand-help-no-utf8 = interpret input file as 8-bit ASCII rather than UTF-8

# Error messages
unexpand-error-invalid-character = tab size contains invalid character(s): { $char }
unexpand-error-tab-size-cannot-be-zero = tab size cannot be 0
unexpand-error-tab-size-too-large = tab stop value is too large
unexpand-error-tab-sizes-must-be-ascending = tab sizes must be ascending
unexpand-error-is-directory = { $path }: Is a directory
"),
        // Locale for uniq (en-US)
        "uniq/en-US.ftl" => Some(r"uniq-about = Report or omit repeated lines.
uniq-usage = uniq [OPTION]... [INPUT [OUTPUT]]
uniq-after-help = Filter adjacent matching lines from INPUT (or standard input),
  writing to OUTPUT (or standard output).

  Note: uniq does not detect repeated lines unless they are adjacent.
  You may want to sort the input first, or use sort -u without uniq.

# Help messages
uniq-help-all-repeated = print all duplicate lines. Delimiting is done with blank lines. [default: none]
uniq-help-group = show all items, separating groups with an empty line. [default: separate]
uniq-help-check-chars = compare no more than N characters in lines
uniq-help-count = prefix lines by the number of occurrences
uniq-help-ignore-case = ignore differences in case when comparing
uniq-help-repeated = only print duplicate lines
uniq-help-skip-chars = avoid comparing the first N characters
uniq-help-skip-fields = avoid comparing the first N fields
uniq-help-unique = only print unique lines
uniq-help-zero-terminated = end lines with 0 byte, not newline

# Error messages
uniq-error-write-line-terminator = Could not write line terminator
uniq-error-write-error = write error
uniq-error-read-error = read error
uniq-error-invalid-argument = Invalid argument for { $opt_name }: { $arg }
uniq-error-try-help = Try 'uniq --help' for more information.
uniq-error-group-mutually-exclusive = --group is mutually exclusive with -c/-d/-D/-u
uniq-error-group-badoption = invalid argument 'badoption' for '--group'
  Valid arguments are:
    - 'prepend'
    - 'append'
    - 'separate'
    - 'both'

uniq-error-all-repeated-badoption = invalid argument 'badoption' for '--all-repeated'
  Valid arguments are:
    - 'none'
    - 'prepend'
    - 'separate'

uniq-error-counts-and-repeated-meaningless = printing all duplicated lines and repeat counts is meaningless
  Try 'uniq --help' for more information.

uniq-error-could-not-open = Could not open { $path }
"),
        // Locale for uniq (en-US)
        "uniq/en-US.ftl" => Some(r"uniq-about = Report or omit repeated lines.
uniq-usage = uniq [OPTION]... [INPUT [OUTPUT]]
uniq-after-help = Filter adjacent matching lines from INPUT (or standard input),
  writing to OUTPUT (or standard output).

  Note: uniq does not detect repeated lines unless they are adjacent.
  You may want to sort the input first, or use sort -u without uniq.

# Help messages
uniq-help-all-repeated = print all duplicate lines. Delimiting is done with blank lines. [default: none]
uniq-help-group = show all items, separating groups with an empty line. [default: separate]
uniq-help-check-chars = compare no more than N characters in lines
uniq-help-count = prefix lines by the number of occurrences
uniq-help-ignore-case = ignore differences in case when comparing
uniq-help-repeated = only print duplicate lines
uniq-help-skip-chars = avoid comparing the first N characters
uniq-help-skip-fields = avoid comparing the first N fields
uniq-help-unique = only print unique lines
uniq-help-zero-terminated = end lines with 0 byte, not newline

# Error messages
uniq-error-write-line-terminator = Could not write line terminator
uniq-error-write-error = write error
uniq-error-read-error = read error
uniq-error-invalid-argument = Invalid argument for { $opt_name }: { $arg }
uniq-error-try-help = Try 'uniq --help' for more information.
uniq-error-group-mutually-exclusive = --group is mutually exclusive with -c/-d/-D/-u
uniq-error-group-badoption = invalid argument 'badoption' for '--group'
  Valid arguments are:
    - 'prepend'
    - 'append'
    - 'separate'
    - 'both'

uniq-error-all-repeated-badoption = invalid argument 'badoption' for '--all-repeated'
  Valid arguments are:
    - 'none'
    - 'prepend'
    - 'separate'

uniq-error-counts-and-repeated-meaningless = printing all duplicated lines and repeat counts is meaningless
  Try 'uniq --help' for more information.

uniq-error-could-not-open = Could not open { $path }
"),
        // Locale for unlink (en-US)
        "unlink/en-US.ftl" => Some(r"unlink-about = Unlink the file at FILE.
unlink-usage = unlink FILE
  unlink OPTION
unlink-error-cannot-unlink = cannot unlink { $path }
"),
        // Locale for unlink (en-US)
        "unlink/en-US.ftl" => Some(r"unlink-about = Unlink the file at FILE.
unlink-usage = unlink FILE
  unlink OPTION
unlink-error-cannot-unlink = cannot unlink { $path }
"),
        // Locale for wc (en-US)
        "wc/en-US.ftl" => Some(r"wc-about = Print newline, word, and byte counts for each FILE, and a total line if more than one FILE is specified.
wc-usage = wc [OPTION]... [FILE]...

# Help messages
wc-help-bytes = print the byte counts
wc-help-chars = print the character counts
wc-help-files0-from = read input from the files specified by
  NUL-terminated names in file F;
  If F is - then read names from standard input
wc-help-lines = print the newline counts
wc-help-max-line-length = print the length of the longest line
wc-help-total = when to print a line with total counts;
  WHEN can be: auto, always, only, never
wc-help-words = print the word counts

# Error messages
wc-error-files-disabled = extra operand { $extra }
  file operands cannot be combined with --files0-from
wc-error-stdin-repr-not-allowed = when reading file names from standard input, no file name of '-' allowed
wc-error-zero-length-filename = invalid zero-length file name
wc-error-zero-length-filename-ctx = { $path }:{ $idx }: invalid zero-length file name
wc-error-cannot-open-for-reading = cannot open { $path } for reading
wc-error-read-error = { $path }: read error
wc-error-failed-to-print-result = failed to print result for { $title }
wc-error-failed-to-print-total = failed to print total

# Decoder error messages
decoder-error-invalid-byte-sequence = invalid byte sequence: { $bytes }
decoder-error-io = underlying bytestream error: { $error }

# Other messages
wc-standard-input = standard input
wc-total = total

# Debug messages
wc-debug-hw-unavailable = debug: hardware support unavailable on this CPU
wc-debug-hw-using = debug: using hardware support (features: { $features })
wc-debug-hw-disabled-env = debug: hardware support disabled by environment
wc-debug-hw-disabled-glibc = debug: hardware support disabled by GLIBC_TUNABLES ({ $features })
wc-debug-hw-limited-glibc = debug: hardware support limited by GLIBC_TUNABLES (disabled: { $disabled }; enabled: { $enabled })
"),
        // Locale for wc (en-US)
        "wc/en-US.ftl" => Some(r"wc-about = Print newline, word, and byte counts for each FILE, and a total line if more than one FILE is specified.
wc-usage = wc [OPTION]... [FILE]...

# Help messages
wc-help-bytes = print the byte counts
wc-help-chars = print the character counts
wc-help-files0-from = read input from the files specified by
  NUL-terminated names in file F;
  If F is - then read names from standard input
wc-help-lines = print the newline counts
wc-help-max-line-length = print the length of the longest line
wc-help-total = when to print a line with total counts;
  WHEN can be: auto, always, only, never
wc-help-words = print the word counts

# Error messages
wc-error-files-disabled = extra operand { $extra }
  file operands cannot be combined with --files0-from
wc-error-stdin-repr-not-allowed = when reading file names from standard input, no file name of '-' allowed
wc-error-zero-length-filename = invalid zero-length file name
wc-error-zero-length-filename-ctx = { $path }:{ $idx }: invalid zero-length file name
wc-error-cannot-open-for-reading = cannot open { $path } for reading
wc-error-read-error = { $path }: read error
wc-error-failed-to-print-result = failed to print result for { $title }
wc-error-failed-to-print-total = failed to print total

# Decoder error messages
decoder-error-invalid-byte-sequence = invalid byte sequence: { $bytes }
decoder-error-io = underlying bytestream error: { $error }

# Other messages
wc-standard-input = standard input
wc-total = total

# Debug messages
wc-debug-hw-unavailable = debug: hardware support unavailable on this CPU
wc-debug-hw-using = debug: using hardware support (features: { $features })
wc-debug-hw-disabled-env = debug: hardware support disabled by environment
wc-debug-hw-disabled-glibc = debug: hardware support disabled by GLIBC_TUNABLES ({ $features })
wc-debug-hw-limited-glibc = debug: hardware support limited by GLIBC_TUNABLES (disabled: { $disabled }; enabled: { $enabled })
"),
        // Locale for whoami (en-US)
        "whoami/en-US.ftl" => Some(r"whoami-about = Print the current username.
whoami-usage = whoami

# Error messages
whoami-error-failed-to-print = failed to print username
whoami-error-failed-to-get = failed to get username
"),
        // Locale for whoami (en-US)
        "whoami/en-US.ftl" => Some(r"whoami-about = Print the current username.
whoami-usage = whoami

# Error messages
whoami-error-failed-to-print = failed to print username
whoami-error-failed-to-get = failed to get username
"),
        // Locale for yes (en-US)
        "yes/en-US.ftl" => Some(r"yes-about = Repeatedly display a line with STRING (or 'y')
yes-usage = yes [STRING]...

# Error messages
yes-error-standard-output = standard output: { $error }
"),
        // Locale for yes (en-US)
        "yes/en-US.ftl" => Some(r"yes-about = Repeatedly display a line with STRING (or 'y')
yes-usage = yes [STRING]...

# Error messages
yes-error-standard-output = standard output: { $error }
yes-error-invalid-utf8 = arguments contain invalid UTF-8
"),
        _ => None,
    }
}
