use criterion::{black_box, criterion_group, criterion_main, Criterion};
use parser::{parse_tokens, tokenize_str};
use pprof::criterion::{Output, PProfProfiler};

fn parse_script(contents: &str) -> parser::ast::Program {
    let tokens = tokenize_str(contents).unwrap();
    parse_tokens(&tokens).unwrap()
}

fn parse_sample_script() -> parser::ast::Program {
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

fn criterion_benchmark(c: &mut Criterion) {
    c.bench_function("parse_sample_script", |b| {
        b.iter(|| black_box(parse_sample_script()))
    });

    const POSSIBLE_BASH_COMPLETION_SCRIPT_PATH: &str = "/usr/share/bash-completion/bash_completion";
    let well_known_complicated_script =
        std::path::PathBuf::from(POSSIBLE_BASH_COMPLETION_SCRIPT_PATH);

    if well_known_complicated_script.exists() {
        benchmark_parsing_script(c, &well_known_complicated_script);
    }
}

criterion_group! {
    name = benches;
    config = Criterion::default().with_profiler(PProfProfiler::new(100, Output::Flamegraph(None)));
    targets = criterion_benchmark
}
criterion_main!(benches);
