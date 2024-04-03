use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn tokio() -> tokio::runtime::Runtime {
    tokio::runtime::Runtime::new().unwrap()
}

async fn instantiate_shell() -> shell::Shell {
    let options = shell::CreateOptions::default();
    shell::Shell::new(&options).await.unwrap()
}

async fn instantiate_shell_with_init_scripts() -> shell::Shell {
    let options = shell::CreateOptions {
        interactive: true,
        read_commands_from_stdin: true,
        ..shell::CreateOptions::default()
    };
    shell::Shell::new(&options).await.unwrap()
}

async fn run_one_command(command: &str) -> shell::ExecutionResult {
    let options = shell::CreateOptions::default();
    let mut shell = shell::Shell::new(&options).await.unwrap();
    shell.run_string(command, false).await.unwrap()
}

async fn run_command_directly(command: &str, args: &[&str]) -> std::process::ExitStatus {
    let mut command = tokio::process::Command::new(command);
    command
        .args(args)
        .stdout(std::process::Stdio::null())
        .status()
        .await
        .unwrap()
}

async fn expand_one_string() -> String {
    let options = shell::CreateOptions::default();
    let mut shell = shell::Shell::new(&options).await.unwrap();
    shell
        .basic_expand_string("The answer is $((6 * 7))")
        .await
        .unwrap()
}

fn criterion_benchmark(c: &mut Criterion) {
    c.bench_function("instantiate shell", |b| {
        b.to_async(tokio()).iter(|| black_box(instantiate_shell()));
    });
    c.bench_function("instantiate shell with init scripts", |b| {
        b.to_async(tokio())
            .iter(|| black_box(instantiate_shell_with_init_scripts()));
    });
    c.bench_function("run one built-in command", |b| {
        b.to_async(tokio())
            .iter(|| black_box(run_one_command("declare new-variable")));
    });
    c.bench_function("run one external command", |b| {
        b.to_async(tokio())
            .iter(|| black_box(run_one_command("echo 'Hello, world!' >/dev/null")));
    });
    c.bench_function("run one external command (directly)", |b| {
        b.to_async(tokio())
            .iter(|| black_box(run_command_directly("echo", &["Hello, world!"])));
    });
    c.bench_function("expand one string", |b| {
        b.iter(|| black_box(expand_one_string()));
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
