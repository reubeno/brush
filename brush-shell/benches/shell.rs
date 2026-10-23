//! Benchmarks for the brush-shell crate.

#![allow(missing_docs)]
#![allow(clippy::unwrap_used)]

#[cfg(unix)]
mod unix {
    use brush_builtins::ShellBuilderExt;
    use brush_parser::SourceSpan;
    use criterion::Criterion;
    use std::hint::black_box;

    async fn instantiate_shell() -> brush_core::Shell {
        brush_core::Shell::builder()
            .default_builtins(brush_builtins::BuiltinSet::BashMode)
            .build()
            .await
            .unwrap()
    }

    async fn instantiate_shell_with_init_scripts() -> brush_core::Shell {
        brush_core::Shell::builder()
            .interactive(true)
            .read_commands_from_stdin(true)
            .default_builtins(brush_builtins::BuiltinSet::BashMode)
            .build()
            .await
            .unwrap()
    }

    async fn run_one_command(shell: &mut brush_core::Shell, command: &str) {
        let _ = shell
            .run_string(
                command.to_owned(),
                &brush_core::SourceInfo::default(),
                &shell.default_exec_params(),
            )
            .await
            .unwrap();
    }

    async fn expand_string(shell: &mut brush_core::Shell, s: &str) {
        let params = shell.default_exec_params();
        let _ = shell.basic_expand_string(&params, s).await.unwrap();
    }

    fn eval_arithmetic_expr(shell: &mut brush_core::Shell, expr: &str) {
        let parsed_expr = brush_parser::arithmetic::parse(expr).unwrap();
        let _ = shell.eval_arithmetic(&parsed_expr).unwrap();
    }

    /// A realistic script exercising a mixed set of shell constructs.
    const MIXED_SCRIPT: &str = r#"
declare x=10
x=$((x * 2 + 5))
y=${x:-0}
name="brush"
echo "$name-$y" > /dev/null
printf '%s\n' "$y" > /dev/null
acc=0
for i in 1 2 3 4 5; do
    acc=$((acc + i))
done
n=0
while [ "$n" -lt 10 ]; do
    n=$((n + 1))
done
if [ "$y" -gt 20 ]; then
    result="large"
elif [ "$y" -gt 10 ]; then
    result="medium"
else
    result="small"
fi
case "$result" in
    large) echo big ;;
    small) echo little ;;
    *) echo mid ;;
esac > /dev/null
greet() {
    echo "hello $1"
}
greet world > /dev/null
out=$(echo nested)
arr=(one two three)
echo "${arr[1]}" > /dev/null
set -- alpha beta gamma
[ $# -eq 3 ]
[ -n "$y" ] && echo nonempty > /dev/null
[ -z "" ] || echo fallback > /dev/null
command -v echo > /dev/null
read -r line < /dev/null
echo done
"#;

    /// A pool of distinct commands, sized so that it exceeds the 64-entry parse
    /// cache and the cache cannot serve every command.
    fn mixed_command_pool() -> Vec<String> {
        let mut pool = Vec::with_capacity(128);
        for i in 0..128 {
            match i % 6 {
                0 => pool.push(format!("echo cmd_{i} > /dev/null")),
                1 => pool.push(format!("n_{i}=$(({i} * 2))")),
                2 => pool.push(format!("echo ${{n_{i}:-{i}}} > /dev/null")),
                3 => pool.push(format!("[ {i} -gt 0 ]")),
                4 => pool.push(format!("printf '%s\\n' {i} > /dev/null")),
                _ => pool.push(format!("x_{i}={i}")),
            }
        }
        pool
    }

    /// This function defines core shell benchmarks.
    pub(crate) fn criterion_benchmark(c: &mut Criterion) {
        // Construct a runtime for us to run async code on.
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();

        // Benchmark shell instantiation.
        c.bench_function("instantiate_shell", |b| {
            b.to_async(&rt).iter(|| black_box(instantiate_shell()));
        });
        c.bench_function("instantiate_shell_with_init_scripts", |b| {
            b.to_async(&rt)
                .iter(|| black_box(instantiate_shell_with_init_scripts()));
        });

        // Benchmark: cloning a shell object.
        let shell = rt.block_on(instantiate_shell());
        c.bench_function("clone_shell_object", |b| {
            b.iter(|| black_box(shell.clone()));
        });

        // Benchmark: parsing and evaluating an arithmetic expression..
        let shell = rt.block_on(instantiate_shell());
        c.bench_function("eval_arithmetic", |b| {
            b.iter_batched_ref(
                || shell.clone(),
                |s| eval_arithmetic_expr(s, "3 + 10 * 2"),
                criterion::BatchSize::SmallInput,
            );
        });

        // Benchmark: running the echo built-in command.
        let shell = rt.block_on(instantiate_shell());
        c.bench_function("run_echo_builtin_command", |b| {
            b.iter_batched_ref(
                || shell.clone(),
                |s| rt.block_on(run_one_command(s, "echo 'Hello, world!' >/dev/null")),
                criterion::BatchSize::SmallInput,
            );
        });

        // Benchmark: running an external command.
        // let shell = rt.block_on(instantiate_shell());
        // c.bench_function("run_one_external_command", |b| {
        //     b.iter_batched_ref(
        //         || shell.clone(),
        //         |s| {
        //             rt.block_on(run_one_command(
        //                 s,
        //                 "/usr/bin/echo 'Hello, world!' >/dev/null",
        //             ));
        //         },
        //         criterion::BatchSize::SmallInput,
        //     );
        // });

        // Benchmark: word expansion.
        let shell = rt.block_on(instantiate_shell());
        c.bench_function("expand_one_string", |b| {
            b.iter_batched_ref(
                || shell.clone(),
                |s| rt.block_on(expand_string(s, "My version is ${BASH_VERSINFO[@]}")),
                criterion::BatchSize::SmallInput,
            );
        });

        // Benchmark: function invocation.
        let mut shell = rt.block_on(instantiate_shell());
        shell.define_func(
            String::from("testfunc"),
            brush_parser::ast::FunctionDefinition {
                fname: String::from("testfunc").into(),
                body: brush_parser::ast::FunctionBody(
                    brush_parser::ast::CompoundCommand::BraceGroup(
                        brush_parser::ast::BraceGroupCommand {
                            list: brush_parser::ast::CompoundList(vec![]),
                            loc: SourceSpan::default(),
                        },
                    ),
                    None,
                ),
            },
            &brush_core::SourceInfo::default(),
        );
        c.bench_function("function_call", |b| {
            b.iter_batched_ref(
                || shell.clone(),
                |s| {
                    rt.block_on(run_one_command(s, "testfunc"));
                },
                criterion::BatchSize::SmallInput,
            );
        });

        // Benchmark: for loop.
        let shell = rt.block_on(instantiate_shell());
        c.bench_function("for_loop", |b| {
            b.iter_batched_ref(
                || shell.clone(),
                |s| {
                    rt.block_on(run_one_command(s, "for ((i = 0; i < 10; i++)); do :; done"));
                },
                criterion::BatchSize::SmallInput,
            );
        });

        // Benchmark: running a realistic script with a mixed set of commands.
        let shell = rt.block_on(instantiate_shell());
        c.bench_function("run_mixed_script", |b| {
            b.iter_batched_ref(
                || shell.clone(),
                |s| {
                    rt.block_on(run_one_command(s, MIXED_SCRIPT));
                },
                criterion::BatchSize::SmallInput,
            );
        });

        // Benchmark: running a mixed set of distinct commands one at a time.
        // The pool exceeds the 64-entry parse cache, so most commands miss.
        let shell = rt.block_on(instantiate_shell());
        let mixed_commands = mixed_command_pool();
        c.bench_function("run_mixed_commands", |b| {
            b.iter_batched_ref(
                || shell.clone(),
                |s| {
                    for command in &mixed_commands {
                        rt.block_on(run_one_command(s, command));
                    }
                },
                criterion::BatchSize::SmallInput,
            );
        });
    }
}

#[cfg(unix)]
criterion::criterion_group! {
    name = benches;
    config = criterion::Criterion::default()
                .measurement_time(std::time::Duration::from_secs(10));
    targets = unix::criterion_benchmark
}

#[cfg(unix)]
criterion::criterion_main!(benches);

#[cfg(not(unix))]
fn main() {}
