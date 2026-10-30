<!-- omit in toc -->
# Contributing to brush

First off, thanks for taking the time to contribute! ❤️

All types of contributions are encouraged and valued. See the [Table of Contents](#table-of-contents) for different ways to help and details about how this project handles them. Please make sure to read the relevant section before making your contribution. It will make it a lot easier for us maintainers and smooth out the experience for all involved. The community looks forward to your contributions. 🎉

> And if you like the project, but just don't have time to contribute, that's fine. There are other easy ways to support the project and show your appreciation, which we would also be very happy about:
> - Star the project
> - Send us feedback via issues or discussion
> - Refer this project in your project's readme
> - Post about it
> - Tell your friends/colleagues

<!-- omit in toc -->
## Table of Contents
- [Code of Conduct](#code-of-conduct)
- [I Have a Question](#i-have-a-question)
  - [I Want To Contribute](#i-want-to-contribute)
  - [Independent Implementation and Provenance](#independent-implementation-and-provenance)
  - [AI Policy](#ai-policy)
  - [Reporting Bugs](#reporting-bugs)
  - [Suggesting Enhancements](#suggesting-enhancements)
  - [Your First Code Contribution](#your-first-code-contribution)
  - [Development Setup and Checks](#development-setup-and-checks)
  - [Improving The Documentation](#improving-the-documentation)
- [Styleguides](#styleguides)
  - [Commit Messages](#commit-messages)

## Code of Conduct

This project and everyone participating in it is governed by the
[brush Code of Conduct](CODE_OF_CONDUCT.md).
By participating, you are expected to uphold this code. Please report unacceptable behavior
to project admins.

## I Have a Question

> If you want to ask a question, we assume that you have read the available [Documentation](docs).

Before you ask a question, it is best to search for existing [Issues](https://github.com/reubeno/brush/issues) that might help you. In case you have found a suitable issue and still need clarification, you can write your question in this issue. It is also advisable to search the internet for answers first.

If you then still feel the need to ask a question and need clarification, we recommend the following:

- Open an [Issue](https://github.com/reubeno/brush/issues/new).
- Provide as much context as you can about what you're running into.
- Provide project and platform versions, depending on what seems relevant.

We will then take care of the issue as soon as possible.

## I Want To Contribute

### Legal Notice <!-- omit in toc -->

> [!NOTE]
> By contributing, you confirm that you have the necessary rights to submit your
> contribution under the project's [MIT license](LICENSE). Identify any
> third-party material and its license, and preserve required notices. Using AI
> tools does not remove these obligations.

### Independent Implementation and Provenance

Brush is independently implemented. Matching Bash's observable behavior is a
project goal; deriving brush's implementation from Bash's implementation is not
an acceptable way to achieve it.

#### Implementation boundaries

- **Do not copy, translate, port, or adapt Bash implementation material into
  brush.** This includes code, comments, implementation-specific tables, and
  upstream tests from its code base. Changing languages, identifiers,
  formatting, or structure, removing notices, or adding attribution does not
  establish independent provenance or grant permission to use the material
  under brush's license.
- **Do not inspect or retrieve Bash source implementation material for
  implementation guidance.** This includes mirrors, patches, source excerpts in
  issues or articles, and source packages installed locally. Do not ask another
  person or agent to inspect it on your behalf.
- **Do not route that material through an intermediary.** AI-generated
  translations, pseudocode, and implementation summaries derived from
  Bash source are not independent inputs to a brush implementation.
- **Apply the same licensing discipline to other third-party material.**
  Identify the source, license, and required notices before including it. If
  the terms do not permit the proposed inclusion, or the provenance is unclear,
  stop and ask maintainers rather than attempting to rewrite around the terms.

#### Behavioral compatibility work

Use documented interfaces, specifications, and independently written behavioral
reproductions. It's useful to run an installed Bash executable as a black-box
oracle; we've implemented a testing framework that simplifies passing original
input scripts through both Bash and brush and compare stdout, stderr, exit
status, filesystem effects, etc. We strongly encourage adding original compat
cases under `brush-shell/tests/cases/`.

Our build and CI processes clone and run third-party test suites without
importing their sources into this repository. Use them as external tests,
not as source material or implementation guidance for brush.

### AI Policy

> [!NOTE]
> AI assistants have arrived. With them come many questions and much debate about policy, rights, appropriate usage, etc. Our evolving experiences show that they can be wielded as powerful tools, but we acknowledge that it is critical that they be used *responsibly* and thoughtfully. We set out the following principles for our project.

* **Human supervision and review remain critical.** You, the human authors, are the contributors to this project. Regardless of which tools you use along the way, you are all fully accountable for your contributions. This means that you are still accountable for ensuring that you have the necessary rights to the content you submit to this project. It also remains your responsibility to review, test, understand, and vouch for all code and content that you submit. Accordingly, you must fully understand all code and other content in your contributions and be able to explain their behavior and purpose.

* **Please be transparent about your use of AI.** When contributions have been significantly assisted by AI tools, note this in your PR description or commit message via an `Assisted-by:` trailer.

* **Our policies and practices will evolve.** Things are moving quickly; we consider it far better to adapt as we go, learn, and iterate rather than to remain static. We remain open to your input and feedback on this policy as it evolves, and commit to engaged, respectful discussion regarding it.

#### References

We take inspiration (and direct language) from the emerging Fedora proposal for [policy regarding AI-assisted contributions](https://communityblog.fedoraproject.org/council-policy-proposal-policy-on-ai-assisted-contributions/) as well as from the Apache Software Foundation's [Generative Tooling Guidance](https://www.apache.org/legal/generative-tooling.html).

### Reporting Bugs

<!-- omit in toc -->
#### Before Submitting a Bug Report

A good bug report shouldn't leave others needing to chase you up for more information. Therefore, we ask you to investigate carefully, collect information and describe the issue in detail in your report. Please complete the following steps in advance to help us fix any potential bug as fast as possible.

- Make sure that you are using the latest version.
- Determine if your bug is really a bug and not an error on your side e.g. using incompatible environment components/versions (Make sure that you have read the [documentation](docs). If you are looking for support, you might want to check [this section](#i-have-a-question)).
- To see if other users have experienced (and potentially already solved) the same issue you are having, check if there is not already a bug report existing for your bug or error in the [bug tracker](https://github.com/reubeno/brush/issues?q=label%3Abug).
- Also make sure to search the internet (including Stack Overflow) to see if users outside of the GitHub community have discussed the issue.
- Collect information about the bug:
  - Stack trace (Traceback)
  - OS, Platform and Version (Windows, Linux, macOS, x86, ARM)
  - Version of the interpreter, compiler, SDK, runtime environment, package manager, depending on what seems relevant.
  - Possibly your input and the output
  - Can you reliably reproduce the issue? And can you also reproduce it with older versions?

<!-- omit in toc -->
#### How Do I Submit a Good Bug Report?

We use GitHub issues to track bugs and errors. If you run into an issue with the project:

- Open an [Issue](https://github.com/reubeno/brush/issues/new).
- Explain the behavior you would expect and the actual behavior.
- Please provide as much context as possible and describe the *reproduction steps* that someone else can follow to recreate the issue on their own. This usually includes your code. For good bug reports you should isolate the problem and create a reduced test case.
- Provide the information you collected in the previous section.

Once it's filed:

- The project team will label the issue accordingly.
- A team member will try to reproduce the issue with your provided steps. If there are no reproduction steps or no obvious way to reproduce the issue, the team will ask you for those steps and mark the issue as `needs-repro`. Bugs with the `needs-repro` tag will not be addressed until they are reproduced.
- If the team is able to reproduce the issue, it will be marked `needs-fix`, as well as possibly other tags (such as `critical`), and the issue will be left to be [implemented by someone](#your-first-code-contribution).


### Suggesting Enhancements

This section guides you through submitting an enhancement suggestion for brush, **including completely new features and minor improvements to existing functionality**. Following these guidelines will help maintainers and the community to understand your suggestion and find related suggestions.

<!-- omit in toc -->
#### Before Submitting an Enhancement

- Make sure that you are using the latest version.
- Read the [documentation](docs) carefully and find out if the functionality is already covered, maybe by an individual configuration.
- Perform a [search](https://github.com/reubeno/brush/issues) to see if the enhancement has already been suggested. If it has, add a comment to the existing issue instead of opening a new one.
- Find out whether your idea fits with the scope and aims of the project. It's up to you to make a strong case to convince the project's developers of the merits of this feature. Keep in mind that we want features that will be useful to the majority of our users and not just a small subset. If you're just targeting a minority of users, consider writing an add-on/plugin library.

<!-- omit in toc -->
#### How Do I Submit a Good Enhancement Suggestion?

Enhancement suggestions are tracked as [GitHub issues](https://github.com/reubeno/brush/issues).

- Use a **clear and descriptive title** for the issue to identify the suggestion.
- Provide a **step-by-step description of the suggested enhancement** in as many details as possible.
- **Describe the current behavior** and **explain which behavior you expected to see instead** and why. At this point you can also tell which alternatives do not work for you.
- **Explain why this enhancement would be useful** to most brush users. You may also want to point out the other projects that solved it better and which could serve as inspiration.

### Your First Code Contribution
We're excited to see your first contribution! We don't bite, so we'd much rather you publish something  than not. If you submit your PR as a draft we're happy to give it a quick scan and provide some early input. Once the PR is marked "ready for review" we'll look through it more carefully.

### Development Setup and Checks
Building brush needs nothing but [rustup](https://rustup.rs/). `rust-toolchain.toml`
pins the exact toolchain, along with the `rustfmt` and `clippy` components, and
rustup installs it automatically the first time you run `cargo` in the repository,
so a fresh clone is ready to go after `cargo build`. The pin keeps formatting and
lint results identical between your machine and CI; Dependabot bumps it as new
Rust releases ship.

All development tasks run through `cargo xtask`, which is the single entry point
and is what CI invokes too:

| Command | When | Needs |
|---|---|---|
| `cargo xtask ci quick` | Inner loop (~7s warm): fmt, crate structure, build, clippy, unit tests | Rust toolchain only |
| `cargo xtask ci full` | Before opening a PR (~60s warm): adds the pre-commit hooks, schemas, integration tests | also `prek` |
| `cargo xtask check <name>` | One check at a time; `cargo xtask check --help` lists them | varies |

`cargo xtask ci full` also runs the third-party linters -- spelling (typos),
GitHub Actions analysis (zizmor), link checking (lychee), dependency auditing
(cargo-deny), structural lints (ast-grep), and assorted file hygiene. Those are defined and version-pinned in
`.pre-commit-config.yaml` and executed by [prek](https://prek.j178.dev):

```bash
cargo binstall prek   # or: uv tool install prek / brew install prek
```

If you would rather not install it, `cargo xtask ci full --no-hooks` skips that
portion; CI will still run it.

`cargo xtask ci full` reproduces every check CI performs that does not need a
different machine or toolchain. CI additionally covers the three-OS matrix, the
MSRV toolchain, cross-compilation, coverage, benchmarks, the bash-completion
suite, the nightly-only checks, and CodeQL -- so a green local run is a strong
signal, not a guarantee.

#### Git hooks (optional)
Git hooks are entirely optional; the commands above are the supported path. If
you want them:

```bash
prek install
```

That wires up formatting and hygiene fixes on commit, Conventional Commits
validation on the commit message, and `cargo xtask ci quick` on push -- the same
command you would otherwise run by hand.

### Improving The Documentation
We appreciate documentation updates as much as code updates! We try to follow the [Diátaxis](https://diataxis.fr/) approach to technical docs. The short version is we recognize the need for tutorials, how-to guides, reference material, and sometimes longer-form explanation docs--*and* we recognize the value of being intentional about organizing by those categories.
## Styleguides
### Commit Messages
When you look through our repo history, you'll see commits prefixed with a tag like `feat:` or `fix:`. We follow the [Conventional Commits](https://www.conventionalcommits.org/) specification to maintain consistent readability of commits, both for humans and mechanical release processes.

## Attribution
This guide is based on the **contributing-gen**. [Make your own](https://github.com/bttger/contributing-gen)!
