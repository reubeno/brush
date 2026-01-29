//! Benchmarks for filter infrastructure overhead.
//!
//! These benchmarks measure the cost of the filter infrastructure when
//! using `NoOpCmdExecFilter` (the default). The goal is to verify that
//! no-op filters have near-zero overhead compared to direct execution.

#![allow(missing_docs)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::implicit_clone)]
#![allow(clippy::explicit_iter_loop)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::items_after_statements)]
#![allow(clippy::manual_async_fn)]
#![allow(clippy::unnecessary_literal_unwrap)]

use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};

/// Benchmark the cost of Vec<String> allocation for filter params.
///
/// This measures the overhead of converting CommandArg-like data to strings,
/// which happens unconditionally before filter invocation.
fn bench_args_allocation(c: &mut Criterion) {
    let mut group = c.benchmark_group("filter_param_allocation");

    // Simulate different argument counts
    for arg_count in [0, 1, 5, 10, 20] {
        let args: Vec<String> = (0..arg_count).map(|i| format!("arg{i}")).collect();

        group.bench_with_input(
            BenchmarkId::new("clone_vec_string", arg_count),
            &args,
            |b, args| {
                b.iter(|| {
                    let cloned: Vec<String> = black_box(args.clone());
                    black_box(cloned)
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("to_string_collect", arg_count),
            &args,
            |b, args| {
                b.iter(|| {
                    // This simulates what happens in commands.rs
                    let collected: Vec<String> =
                        black_box(args.iter().map(|s| s.to_string()).collect());
                    black_box(collected)
                });
            },
        );

        // Measure just iteration (no allocation) for comparison
        group.bench_with_input(
            BenchmarkId::new("iterate_only", arg_count),
            &args,
            |b, args| {
                b.iter(|| {
                    for arg in args.iter() {
                        black_box(arg);
                    }
                });
            },
        );
    }

    group.finish();
}

/// Benchmark the cost of Cow<str> creation patterns.
fn bench_cow_patterns(c: &mut Criterion) {
    let mut group = c.benchmark_group("cow_patterns");

    let owned_string = String::from("echo");
    let static_str = "echo";

    group.bench_function("cow_borrowed_from_str", |b| {
        b.iter(|| {
            let cow: std::borrow::Cow<'_, str> = black_box(static_str.into());
            black_box(cow)
        });
    });

    group.bench_function("cow_borrowed_from_string_ref", |b| {
        b.iter(|| {
            let cow: std::borrow::Cow<'_, str> = black_box(owned_string.as_str().into());
            black_box(cow)
        });
    });

    group.bench_function("cow_owned_from_string", |b| {
        b.iter(|| {
            let cow: std::borrow::Cow<'_, str> = black_box(owned_string.clone().into());
            black_box(cow)
        });
    });

    group.finish();
}

/// Benchmark async overhead for immediate-return futures.
///
/// NoOp filters return `async { PreFilterResult::Continue(params) }`.
/// This measures the overhead of that async machinery.
fn bench_async_noop(c: &mut Criterion) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();

    #[allow(unused_imports)]
    use std::future::Future;

    let mut group = c.benchmark_group("async_overhead");

    // Direct return (no async)
    group.bench_function("direct_return", |b| {
        b.iter(|| {
            let result: i32 = black_box(42);
            black_box(result)
        });
    });

    // Async immediate return
    group.bench_function("async_immediate_return", |b| {
        b.to_async(&rt).iter(|| async {
            let result: i32 = black_box(42);
            black_box(result)
        });
    });

    // Async with match (simulates with_filter! macro pattern)
    group.bench_function("async_with_match", |b| {
        b.to_async(&rt).iter(|| async {
            let pre_result = async { black_box(Ok::<i32, ()>(42)) }.await;
            match pre_result {
                Ok(v) => {
                    let body_result = black_box(v + 1);
                    let post_result = async { black_box(body_result) }.await;
                    black_box(post_result)
                }
                Err(e) => black_box(Err(e).unwrap_or(0)),
            }
        });
    });

    group.finish();
}

/// Benchmark filter trait method dispatch overhead.
///
/// Measures cost of calling filter methods through trait objects vs monomorphized.
fn bench_filter_dispatch(c: &mut Criterion) {
    use brush_core::filter::{CmdExecFilter, NoOpCmdExecFilter, PostFilterResult, SimpleCmdOutput};

    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();

    let mut group = c.benchmark_group("filter_dispatch");

    // NoOp filter (should be optimized away)
    let noop_filter = NoOpCmdExecFilter;

    group.bench_function("noop_pre_filter_call", |b| {
        b.to_async(&rt).iter(|| async {
            // We can't easily construct SimpleCmdParams without a Shell,
            // so just measure the async overhead of calling the method
            let result: SimpleCmdOutput =
                black_box(Ok(brush_core::results::ExecutionSpawnResult::Completed(
                    brush_core::results::ExecutionResult::success(),
                )));
            let post_result = noop_filter.post_simple_cmd(result).await;
            black_box(post_result)
        });
    });

    // Custom filter that does minimal work
    #[derive(Clone, Default)]
    struct MinimalFilter;

    impl CmdExecFilter for MinimalFilter {
        fn post_simple_cmd(
            &self,
            result: SimpleCmdOutput,
        ) -> impl std::future::Future<Output = PostFilterResult<SimpleCmdOutput>> + Send {
            async { PostFilterResult::Return(result) }
        }
    }

    let minimal_filter = MinimalFilter;

    group.bench_function("minimal_post_filter_call", |b| {
        b.to_async(&rt).iter(|| async {
            let result: SimpleCmdOutput =
                black_box(Ok(brush_core::results::ExecutionSpawnResult::Completed(
                    brush_core::results::ExecutionResult::success(),
                )));
            let post_result = minimal_filter.post_simple_cmd(result).await;
            black_box(post_result)
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_args_allocation,
    bench_cow_patterns,
    bench_async_noop,
    bench_filter_dispatch,
    bench_simple_cmd_params_construction,
);
criterion_main!(benches);

/// Benchmark `SimpleCmdParams` construction: eager (old) vs lazy (new).
///
/// This directly measures the optimization we made: using `from_command_args`
/// instead of pre-allocating a `Vec<String>`.
fn bench_simple_cmd_params_construction(c: &mut Criterion) {
    use brush_core::commands::CommandArg;

    let mut group = c.benchmark_group("simple_cmd_params_construction");

    for arg_count in [0, 1, 5, 10, 20] {
        // Create CommandArg slice (what we actually have in commands.rs)
        let command_args: Vec<CommandArg> = (0..arg_count)
            .map(|i| CommandArg::String(format!("arg{i}")))
            .collect();

        // EAGER (old pattern): allocate Vec<String> upfront
        group.bench_with_input(
            BenchmarkId::new("eager_vec_string", arg_count),
            &command_args,
            |b, args| {
                b.iter(|| {
                    // This is what the OLD code did unconditionally
                    let args_as_strings: Vec<String> = args.iter().map(|a| a.to_string()).collect();
                    black_box(args_as_strings)
                });
            },
        );

        // LAZY (new pattern): just store reference, no allocation
        group.bench_with_input(
            BenchmarkId::new("lazy_reference_only", arg_count),
            &command_args,
            |b, args| {
                b.iter(|| {
                    // This is what the NEW code does - just stores a reference
                    let args_ref: &[CommandArg] = black_box(args.as_slice());
                    black_box(args_ref)
                });
            },
        );

        // LAZY with actual access (when filter DOES inspect args)
        group.bench_with_input(
            BenchmarkId::new("lazy_with_access", arg_count),
            &command_args,
            |b, args| {
                b.iter(|| {
                    // Store reference first (cheap)
                    let args_ref: &[CommandArg] = args.as_slice();
                    // Then allocate only when accessed (same cost as eager)
                    let args_as_strings: Vec<String> =
                        args_ref.iter().map(|a| a.to_string()).collect();
                    black_box(args_as_strings)
                });
            },
        );
    }

    group.finish();
}
