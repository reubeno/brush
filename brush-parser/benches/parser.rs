//! Benchmarks for the brush-parser crate.
//!
//! Compares three parsing approaches:
//! 1. PEG parser (tokenize + peg parse)
//! 2. Winnow parser (tokenize + winnow parse) - when use-winnow-parser feature enabled
//! 3. Winnow_str parser (direct string parse) - when use-winnow-parser feature enabled

#![allow(missing_docs)]

#[cfg(unix)]
mod unix {
    use brush_parser::Token;
    use criterion::Criterion;

    fn uncached_tokenize(content: &str) -> Vec<brush_parser::Token> {
        brush_parser::uncached_tokenize_str(content, &brush_parser::TokenizerOptions::default())
            .unwrap()
    }

    fn cacheable_tokenize(content: &str) -> Vec<brush_parser::Token> {
        brush_parser::tokenize_str_with_options(content, &brush_parser::TokenizerOptions::default())
            .unwrap()
    }

    fn parse_peg(tokens: &Vec<Token>) -> brush_parser::ast::Program {
        brush_parser::parse_tokens(
            tokens,
            &brush_parser::ParserOptions::default(),
        )
        .unwrap()
    }

    #[cfg(feature = "use-winnow-parser")]
    fn parse_winnow(tokens: &Vec<Token>) -> brush_parser::ast::Program {
        use brush_parser::{ParserOptions, SourceInfo, winnow};
        winnow::parse_program(
            tokens,
            &ParserOptions::default(),
            &SourceInfo::default(),
        )
        .unwrap()
    }

    #[cfg(feature = "use-winnow-parser")]
    fn parse_winnow_str(content: &str) -> brush_parser::ast::Program {
        use brush_parser::{ParserOptions, SourceInfo, winnow_str};
        winnow_str::parse_program(
            content,
            &ParserOptions::default(),
            &SourceInfo::default(),
        )
        .unwrap()
    }

    // Combined tokenize + parse functions for full pipeline comparison
    fn tokenize_and_parse_peg(content: &str) -> brush_parser::ast::Program {
        let tokens = uncached_tokenize(content);
        parse_peg(&tokens)
    }

    #[cfg(feature = "use-winnow-parser")]
    fn tokenize_and_parse_winnow(content: &str) -> brush_parser::ast::Program {
        let tokens = uncached_tokenize(content);
        parse_winnow(&tokens)
    }

    const SAMPLE_SCRIPT: &str = r#"
for f in A B C; do
    echo "${f@L}" >&2
done
"#;

    const SIMPLE_SCRIPT: &str = "echo hello world";

    const PIPELINE_SCRIPT: &str = "cat file.txt | grep pattern | wc -l";

    const COMPLEX_SCRIPT: &str = r#"
#!/bin/bash
# Complex script with multiple constructs

function process_file() {
    local file="$1"
    if [[ -f "$file" ]]; then
        while read -r line; do
            case "$line" in
                start*)
                    echo "Starting: $line"
                    ;;
                end*)
                    echo "Ending: $line"
                    ;;
                *)
                    echo "Processing: $line"
                    ;;
            esac
        done < "$file"
    fi
}

for i in {1..10}; do
    if (( i % 2 == 0 )); then
        echo "$i is even" | tee -a output.txt
    else
        echo "$i is odd" >> output.txt
    fi
done

process_file "input.txt" && echo "Success" || echo "Failed"
"#;

    fn benchmark_parsing_script_using_caches(c: &mut Criterion, script_path: &std::path::Path) {
        let contents = std::fs::read_to_string(script_path).unwrap();
        let filename = script_path.file_name().unwrap().to_string_lossy();

        c.bench_function(std::format!("parse_peg_{}", filename).as_str(), |b| {
            b.iter(|| parse_peg(&cacheable_tokenize(contents.as_str())))
        });

        #[cfg(feature = "use-winnow-parser")]
        c.bench_function(std::format!("parse_winnow_{}", filename).as_str(), |b| {
            b.iter(|| parse_winnow(&cacheable_tokenize(contents.as_str())))
        });
    }

    pub(crate) fn criterion_benchmark(c: &mut Criterion) {
        const POSSIBLE_BASH_COMPLETION_SCRIPT_PATH: &str =
            "/usr/share/bash-completion/bash_completion";

        // Tokenization benchmark (applies to both parsers)
        c.bench_function("tokenize_sample_script", |b| {
            b.iter(|| uncached_tokenize(SAMPLE_SCRIPT));
        });

        // Simple script benchmarks
        let simple_tokens = uncached_tokenize(SIMPLE_SCRIPT);
        c.bench_function("parse_peg_simple", |b| b.iter(|| parse_peg(&simple_tokens)));
        #[cfg(feature = "use-winnow-parser")]
        c.bench_function("parse_winnow_simple", |b| {
            b.iter(|| parse_winnow(&simple_tokens))
        });
        #[cfg(feature = "use-winnow-parser")]
        c.bench_function("parse_winnow_str_simple", |b| {
            b.iter(|| parse_winnow_str(SIMPLE_SCRIPT))
        });

        // Pipeline script benchmarks
        let pipeline_tokens = uncached_tokenize(PIPELINE_SCRIPT);
        c.bench_function("parse_peg_pipeline", |b| {
            b.iter(|| parse_peg(&pipeline_tokens))
        });
        #[cfg(feature = "use-winnow-parser")]
        c.bench_function("parse_winnow_pipeline", |b| {
            b.iter(|| parse_winnow(&pipeline_tokens))
        });
        #[cfg(feature = "use-winnow-parser")]
        c.bench_function("parse_winnow_str_pipeline", |b| {
            b.iter(|| parse_winnow_str(PIPELINE_SCRIPT))
        });

        // Sample script (for loop) benchmarks
        let sample_tokens = uncached_tokenize(SAMPLE_SCRIPT);
        c.bench_function("parse_peg_for_loop", |b| {
            b.iter(|| parse_peg(&sample_tokens))
        });
        #[cfg(feature = "use-winnow-parser")]
        c.bench_function("parse_winnow_for_loop", |b| {
            b.iter(|| parse_winnow(&sample_tokens))
        });
        #[cfg(feature = "use-winnow-parser")]
        c.bench_function("parse_winnow_str_for_loop", |b| {
            b.iter(|| parse_winnow_str(SAMPLE_SCRIPT))
        });

        // Complex script benchmarks
        let complex_tokens = uncached_tokenize(COMPLEX_SCRIPT);
        c.bench_function("parse_peg_complex", |b| {
            b.iter(|| parse_peg(&complex_tokens))
        });
        #[cfg(feature = "use-winnow-parser")]
        c.bench_function("parse_winnow_complex", |b| {
            b.iter(|| parse_winnow(&complex_tokens))
        });
        #[cfg(feature = "use-winnow-parser")]
        c.bench_function("parse_winnow_str_complex", |b| {
            b.iter(|| parse_winnow_str(COMPLEX_SCRIPT))
        });

        // Real-world bash completion script (if available)
        let well_known_complicated_script =
            std::path::PathBuf::from(POSSIBLE_BASH_COMPLETION_SCRIPT_PATH);

        if well_known_complicated_script.exists() {
            benchmark_parsing_script_using_caches(c, &well_known_complicated_script);
        }

        // ========================================================================
        // FULL PIPELINE BENCHMARKS (tokenize + parse)
        // ========================================================================
        // These benchmarks measure the complete parsing pipeline from string to AST,
        // allowing fair comparison between different approaches:
        // - tokenize_and_parse_peg: Legacy tokenizer + PEG parser
        // - tokenize_and_parse_winnow: Legacy tokenizer + Winnow parser
        // - parse_winnow_str: Direct string parsing (no separate tokenization)

        // Simple script full pipeline
        c.bench_function("full_peg_simple", |b| {
            b.iter(|| tokenize_and_parse_peg(SIMPLE_SCRIPT))
        });
        #[cfg(feature = "use-winnow-parser")]
        c.bench_function("full_winnow_simple", |b| {
            b.iter(|| tokenize_and_parse_winnow(SIMPLE_SCRIPT))
        });
        #[cfg(feature = "use-winnow-parser")]
        c.bench_function("full_winnow_str_simple", |b| {
            b.iter(|| parse_winnow_str(SIMPLE_SCRIPT))
        });

        // Pipeline script full pipeline
        c.bench_function("full_peg_pipeline", |b| {
            b.iter(|| tokenize_and_parse_peg(PIPELINE_SCRIPT))
        });
        #[cfg(feature = "use-winnow-parser")]
        c.bench_function("full_winnow_pipeline", |b| {
            b.iter(|| tokenize_and_parse_winnow(PIPELINE_SCRIPT))
        });
        #[cfg(feature = "use-winnow-parser")]
        c.bench_function("full_winnow_str_pipeline", |b| {
            b.iter(|| parse_winnow_str(PIPELINE_SCRIPT))
        });

        // For loop full pipeline
        c.bench_function("full_peg_for_loop", |b| {
            b.iter(|| tokenize_and_parse_peg(SAMPLE_SCRIPT))
        });
        #[cfg(feature = "use-winnow-parser")]
        c.bench_function("full_winnow_for_loop", |b| {
            b.iter(|| tokenize_and_parse_winnow(SAMPLE_SCRIPT))
        });
        #[cfg(feature = "use-winnow-parser")]
        c.bench_function("full_winnow_str_for_loop", |b| {
            b.iter(|| parse_winnow_str(SAMPLE_SCRIPT))
        });

        // Complex script full pipeline
        c.bench_function("full_peg_complex", |b| {
            b.iter(|| tokenize_and_parse_peg(COMPLEX_SCRIPT))
        });
        #[cfg(feature = "use-winnow-parser")]
        c.bench_function("full_winnow_complex", |b| {
            b.iter(|| tokenize_and_parse_winnow(COMPLEX_SCRIPT))
        });
        #[cfg(feature = "use-winnow-parser")]
        c.bench_function("full_winnow_str_complex", |b| {
            b.iter(|| parse_winnow_str(COMPLEX_SCRIPT))
        });
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
fn main() {}
