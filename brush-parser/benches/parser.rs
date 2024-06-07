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

    fn parse_sample_script() -> brush_parser::ast::Program {
        let input = r#"
            for f in A B C; do
                echo "${f@L}" >&2
            done
    "#;

        parse_script(input)
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
        c.bench_function("parse_sample_script", |b| {
            b.iter(|| black_box(parse_sample_script()))
        });

        const POSSIBLE_BASH_COMPLETION_SCRIPT_PATH: &str =
            "/usr/share/bash-completion/bash_completion";
        let well_known_complicated_script =
            std::path::PathBuf::from(POSSIBLE_BASH_COMPLETION_SCRIPT_PATH);

        if well_known_complicated_script.exists() {
            benchmark_parsing_script(c, &well_known_complicated_script);
        }
    }
}

#[cfg(unix)]
criterion::criterion_group! {
    name = benches;
    config = criterion::Criterion::default().with_profiler(pprof::criterion::PProfProfiler::new(100, pprof::criterion::Output::Flamegraph(None)));
    targets = unix::criterion_benchmark
}
#[cfg(unix)]
criterion::criterion_main!(benches);

#[cfg(not(unix))]
fn main() -> () {}
