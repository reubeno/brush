use criterion::{black_box, criterion_group, criterion_main, Criterion};
use parser::{parse_tokens, tokenize_str};

fn parse_sample_script() -> parser::ast::Program {
    let input = r#"
        for f in A B C; do
            echo "${f@L}" >&2
        done
"#;

    let tokens = tokenize_str(input).unwrap();
    parse_tokens(&tokens).unwrap()
}

fn criterion_benchmark(c: &mut Criterion) {
    c.bench_function("parse sample script", |b| {
        b.iter(|| black_box(parse_sample_script()))
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
