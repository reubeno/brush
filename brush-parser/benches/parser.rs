#[cfg(unix)]
mod unix {
    use brush_parser::{parse_tokens, tokenize_str};
    use criterion::{black_box, Criterion};

    fn parse_script(contents: &str) -> brush_parser::ast::Program {
        let tokens = tokenize_str(contents).unwrap();
        parse_tokens(
            &tokens,
            &brush_parser::ParserOptions::default(),
            &brush_parser::SourceInfo::default(),
        )
        .unwrap()
    }

    fn parse_sample_script(input: &str) -> brush_parser::ast::Program {
        parse_script(input)
    }
    fn parse_sample_script2(input: &str) -> brush_parser::ast::Program {
        brush_parser::parse_program(brush_parser::ParserOptions::default(), input).unwrap()
    }

    fn benchmark_parsing_script(c: &mut Criterion, script_path: &std::path::Path) {
        let contents = std::fs::read_to_string(script_path).unwrap();

        c.bench_function(
            std::format!(
                "parse_{}",
                script_path.file_name().unwrap().to_string_lossy()
            )
            .as_str(),
            |b| b.iter(|| black_box(parse_script(contents.as_str()))),
        );
    }

    pub(crate) fn criterion_benchmark(c: &mut Criterion) {
        let input = r#"
            for f in A B C; do
                echo "${f@L}" >&2
            done
    "#;
        c.bench_function("parse_sample_script", |b| {
            b.iter(|| black_box(parse_sample_script(input)))
        });

        const POSSIBLE_BASH_COMPLETION_SCRIPT_PATH: &str =
            "/usr/share/bash-completion/bash_completion";
        let well_known_complicated_script =
            std::path::PathBuf::from(POSSIBLE_BASH_COMPLETION_SCRIPT_PATH);

        if well_known_complicated_script.exists() {
            benchmark_parsing_script(c, &well_known_complicated_script);
        }
    }

    pub(crate) fn compare_parsers(c: &mut Criterion) {
        // compare_parsers_cached(c);
        compare_parsers_uncached(c);
    }

    fn compare_parsers_uncached(c: &mut Criterion) {
        let mut group = c.benchmark_group("compare_parsers");
        // prevent caching
        let mut i: usize = 0;
        group.bench_function("old_parser_uncached", |b| {
            b.iter_batched(
                || {
                    i += 1;
                    format!(
                        r#"
            for f in A B C; do
                echo {i} "${{f@L}}" >&2
            done
    "#
                    )
                },
                |input| black_box(parse_sample_script(input.as_str())),
                criterion::BatchSize::SmallInput,
            )
        });
        let mut i: usize = 0;
        group.bench_function("new_parser_uncached", |b| {
            b.iter_batched(
                || {
                    i += 1;
                    format!(
                        r#"
            for f in A B C; do
                echo {i} "${{f@L}}" >&2
            done
    "#
                    )
                },
                |input| {
                    black_box(
                        brush_parser::parse_program(
                            brush_parser::ParserOptions::default(),
                            input.as_str(),
                        )
                        .unwrap(),
                    )
                },
                criterion::BatchSize::SmallInput,
            )
        });

        group.finish();
    }
    fn compare_parsers_cached(c: &mut Criterion) {
        let input = r#"
            for f in A B C; do
                echo "${f@L}" >&2
            done
    "#;
        let mut group = c.benchmark_group("compare_parsers_cached");

        group.bench_function("old_parser_cached", |b| {
            b.iter(|| black_box(parse_sample_script(input)))
        });
        group.bench_function("new_parser_cached", |b| {
            b.iter(|| {
                black_box(black_box(
                    brush_parser::cacheable_parse_program(
                        brush_parser::ParserOptions::default(),
                        input.to_string(),
                    )
                    .unwrap(),
                ))
            })
        });
        group.finish();
    }
}

#[cfg(unix)]
criterion::criterion_group! {
    name = benches;
    config = criterion::Criterion::default().with_profiler(pprof::criterion::PProfProfiler::new(100, pprof::criterion::Output::Flamegraph(None)));
    targets = unix::criterion_benchmark
}

#[cfg(unix)]
criterion::criterion_group! {
    name = compare_parsers;
    config = criterion::Criterion::default().with_profiler(pprof::criterion::PProfProfiler::new(100, pprof::criterion::Output::Flamegraph(None)));
    targets =unix::compare_parsers
}

#[cfg(unix)]
criterion::criterion_main!(compare_parsers);

#[cfg(not(unix))]
fn main() -> () {}
