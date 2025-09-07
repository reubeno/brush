use base64::Engine as _;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::Write;
use std::path::Path;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, unix::OwnedWriteHalf};
use tokio::sync::{Mutex, mpsc};

use brush_core::{
    AliasEvent, ProcessEvent,
    BuiltinEvent,
    env::{EnvironmentLookup, EnvironmentScope},
    set_alias_event_sender, set_process_event_sender, set_builtin_event_sender,
    variables::ShellValueLiteral,
};
use brush_interactive::{InteractiveShell, ShellError};

#[derive(Deserialize)]
#[serde(tag = "method", rename_all = "camelCase")]
enum RpcIncoming {
    RunCommand {
        id: Option<Value>,
        params: RunCommandParams,
    },
    GetEnv { id: Option<Value> },
    GetAliases { id: Option<Value> },
    GetEnvVar { id: Option<Value>, params: GetNameParams },
    GetAlias { id: Option<Value>, params: GetNameParams },
    SetCwd {
        id: Option<Value>,
        params: SetCwdParams,
    },
    SetEnv {
        id: Option<Value>,
        params: SetEnvParams,
    },
    GetForegroundPid {
        id: Option<Value>,
    },
    SetAlias {
        id: Option<Value>,
        params: SetAliasParams,
    },
    UnsetAlias {
        id: Option<Value>,
        params: UnsetAliasParams,
    },
}

#[derive(Deserialize)]
struct RunCommandParams {
    command: String,
}

#[derive(Deserialize)]
struct GetNameParams {
    name: String,
}

#[derive(Deserialize)]
struct SetCwdParams {
    cwd: String,
}

#[derive(Deserialize)]
struct SetEnvParams {
    key: String,
    value: String,
}

#[derive(Deserialize)]
struct SetAliasParams {
    name: String,
    value: String,
}

#[derive(Deserialize)]
struct UnsetAliasParams {
    name: String,
}

#[derive(Serialize)]
#[serde(tag = "method", content = "params", rename_all = "camelCase")]
enum Notification {
    ProcessSpawn { ppid: i32, pid: i32, command: String, args: Vec<String>, cwd: std::path::PathBuf },
    ProcessExit { pid: i32, exit_code: i32 },
    CommandStart { id: i32, command: String },
    CommandEnd { id: i32, exit_code: i32 },
    CwdChanged { cwd: String },
    EnvChanged { key: String, value: String },
    AliasAdded { name: String, value: String },
    AliasRemoved { name: String },
    Stdout { #[serde(flatten)] owner: OutputOwner, #[serde(flatten)] output: ProcessOutput },
    Stderr { #[serde(flatten)] owner: OutputOwner, #[serde(flatten)] output: ProcessOutput },
    BuiltinSpawn { id: u64, name: String, args: Vec<String>, cwd: std::path::PathBuf },
    BuiltinExit { id: u64, exit_code: i32 },
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RunCommandEndResult { exit_code: i32 }

#[derive(Serialize)]
struct Response<T: Serialize> { id: Value, result: T }

#[derive(Serialize)]
struct ErrorObject { code: i32, message: String }

#[derive(Serialize)]
struct ErrorResponse { id: Value, error: ErrorObject }

#[derive(Clone, Debug)]
enum ProcessOutput {
    Text(String),
    Data(Vec<u8>),
}

impl Serialize for ProcessOutput {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(Some(1))?;
        match self {
            ProcessOutput::Text(s) => {
                map.serialize_entry("text", s)?;
            }
            ProcessOutput::Data(bytes) => {
                let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
                map.serialize_entry("data", &encoded)?;
            }
        }
        map.end()
    }
}

#[derive(Clone, Debug)]
enum OutputOwner {
    Process { pid: i32 },
    Builtin { bid: u64 },
}

impl Serialize for OutputOwner {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(Some(1))?;
        match *self {
            OutputOwner::Process { pid } => map.serialize_entry("pid", &pid)?,
            OutputOwner::Builtin { bid } => map.serialize_entry("bid", &bid)?,
        }
        map.end()
    }
}

async fn write_json<T: Serialize>(
    writer: &Arc<Mutex<OwnedWriteHalf>>,
    msg: &T,
) -> Result<(), std::io::Error> {
    let mut bytes = serde_json::to_vec(msg).unwrap();
    bytes.push(b'\n');
    {
        let mut w = writer.lock().await;
        w.write_all(&bytes).await?;
    }
    Ok(())
}

pub async fn serve_rpc(
    socket_path: &Path,
    shell: &mut impl InteractiveShell,
) -> Result<(), ShellError> {
    let _ = std::fs::remove_file(socket_path);
    let listener = UnixListener::bind(socket_path)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(socket_path, perms)?;
    }

    let (stream, _) = listener.accept().await?;
    let (read_half, write_half) = stream.into_split();
    let write_half = Arc::new(Mutex::new(write_half));
    let mut reader = BufReader::new(read_half).lines();

    // Disable interactive job control while serving RPC so external commands
    // run in the current process group and return promptly.
    {
        let mut s = shell.shell_mut();
        s.as_mut().options.interactive = false;
    }

    let (tx, mut rx) = mpsc::unbounded_channel();
    let last_spawn_pid = Arc::new(Mutex::new(0i32));
    let last_spawn_pid_for_task = Arc::clone(&last_spawn_pid);
    // Defer exit notifications until after output drains
    let deferred_proc_exits: Arc<Mutex<Vec<(i32, i32)>>> = Arc::new(Mutex::new(Vec::new()));
    let deferred_proc_exits_task = Arc::clone(&deferred_proc_exits);
    set_process_event_sender(tx);
    let write_half_events = Arc::clone(&write_half);
    tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            let msg = match event {
                ProcessEvent::Spawn {
                    ppid,
                    pid,
                    command,
                    args,
                    cwd,
                } => {
                    let mut guard = last_spawn_pid_for_task.lock().await;
                    *guard = pid;
                    Notification::ProcessSpawn {
                        ppid,
                        pid,
                        command,
                        args,
                        cwd,
                    }
                }
                ProcessEvent::Exit { pid, exit_code } => {
                    // Defer process exit until output is drained
                    let mut d = deferred_proc_exits_task.lock().await;
                    d.push((pid, exit_code));
                    continue;
                }
            };
            if write_json(&write_half_events, &msg).await.is_err() {
                break;
            }
        }
    });

    let last_builtin_id = Arc::new(Mutex::new(None::<u64>));
    let last_builtin_id_for_forward_reset = Arc::clone(&last_builtin_id);

    let (alias_tx, mut alias_rx) = mpsc::unbounded_channel();
    set_alias_event_sender(alias_tx);
    let write_half_alias = Arc::clone(&write_half);
    tokio::spawn(async move {
        while let Some(event) = alias_rx.recv().await {
            let msg = match event {
                AliasEvent::Set { name, value } => Notification::AliasAdded { name, value },
                AliasEvent::Unset { name } => Notification::AliasRemoved { name },
            };
            if write_json(&write_half_alias, &msg).await.is_err() {
                break;
            }
        }
    });

    // Builtin events
    let (builtin_tx, mut builtin_rx) = mpsc::unbounded_channel();
    set_builtin_event_sender(builtin_tx);
    let write_half_builtin = Arc::clone(&write_half);
    let last_builtin_id_for_task = Arc::clone(&last_builtin_id);
    let deferred_builtin_exits: Arc<Mutex<Vec<(u64, i32)>>> = Arc::new(Mutex::new(Vec::new()));
    let deferred_builtin_exits_task = Arc::clone(&deferred_builtin_exits);
    tokio::spawn(async move {
        while let Some(event) = builtin_rx.recv().await {
            let msg = match event {
                BuiltinEvent::Spawn { id, name, args, cwd } => {
                    if let Ok(mut guard) = last_builtin_id_for_task.try_lock() {
                        *guard = Some(id);
                    }
                    Notification::BuiltinSpawn { id, name, args, cwd }
                }
                BuiltinEvent::Exit { id, exit_code } => {
                    // Defer builtin exit until output is drained
                    let mut d = deferred_builtin_exits_task.lock().await;
                    d.push((id, exit_code));
                    continue;
                }
            };
            if write_json(&write_half_builtin, &msg).await.is_err() {
                break;
            }
        }
    });

    let mut next_exec_id: i32 = 1;
    while let Some(line) = reader.next_line().await? {
        if line.is_empty() {
            continue;
        }
        let req: RpcIncoming = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                let msg = ErrorResponse { id: Value::Null, error: ErrorObject { code: -32700, message: e.to_string() } };
                write_json(&write_half, &msg).await?;
                continue;
            }
        };

        match req {
            RpcIncoming::RunCommand { id, params } => {
                let parse = shell.shell().as_ref().parse_string(&params.command);
                match parse {
                    Ok(_) => {
                        let exec_id = next_exec_id;
                        next_exec_id += 1;
                        let notify = Notification::CommandStart { id: exec_id, command: params.command.clone() };
                        write_json(&write_half, &notify).await?;
                        // Reset last seen owners for this execution
                        {
                            let mut pg = last_spawn_pid.lock().await;
                            *pg = 0;
                        }
                        {
                            let mut bg = last_builtin_id_for_forward_reset.lock().await;
                            *bg = None;
                        }
                        // Clear any deferred exits accumulated from prior runs
                        {
                            let mut v = deferred_proc_exits.lock().await;
                            v.clear();
                        }
                        {
                            let mut v = deferred_builtin_exits.lock().await;
                            v.clear();
                        }

                        // Prepare pipes to capture stdout/stderr
                        let (stdout_reader, stdout_writer) = brush_core::pipe()?;
                        let (stderr_reader, stderr_writer) = brush_core::pipe()?;

                        // Execution params overriding stdout/stderr to go to our pipes
                        let mut exec_params = shell.shell().as_ref().default_exec_params();
                        exec_params
                            .open_files
                            .set(brush_core::OpenFiles::STDOUT_FD, stdout_writer.into());
                        exec_params
                            .open_files
                            .set(brush_core::OpenFiles::STDERR_FD, stderr_writer.into());

                        // Channel to forward output
                        enum OutputChunk {
                            Stdout(Vec<u8>),
                            Stderr(Vec<u8>),
                        }
                        let (tx, mut rx) = mpsc::unbounded_channel::<OutputChunk>();

                        // Blocking reader for stdout
                        let tx_out = tx.clone();
                        let stdout_reader_blocking = stdout_reader;
                        let stdout_handle = tokio::task::spawn_blocking(move || {
                            use std::io::Read;
                            let mut of = brush_core::OpenFile::from(stdout_reader_blocking);
                            let mut buf = [0u8; 4096];
                            loop {
                                match of.read(&mut buf) {
                                    Ok(0) => break,
                                    Ok(n) => {
                                        let _ = tx_out.send(OutputChunk::Stdout(buf[..n].to_vec()));
                                    }
                                    Err(_) => break,
                                }
                            }
                        });

                        // Blocking reader for stderr
                        let tx_err = tx.clone();
                        let stderr_reader_blocking = stderr_reader;
                        let stderr_handle = tokio::task::spawn_blocking(move || {
                            use std::io::Read;
                            let mut of = brush_core::OpenFile::from(stderr_reader_blocking);
                            let mut buf = [0u8; 4096];
                            loop {
                                match of.read(&mut buf) {
                                    Ok(0) => break,
                                    Ok(n) => {
                                        let _ = tx_err.send(OutputChunk::Stderr(buf[..n].to_vec()));
                                    }
                                    Err(_) => break,
                                }
                            }
                        });

                        drop(tx);

                        // Forwarder to client
                        let write_half_output = Arc::clone(&write_half);
                        let last_spawn_pid_for_forward = Arc::clone(&last_spawn_pid);
                        let last_builtin_id_for_forward = Arc::clone(&last_builtin_id);
                        let forward_handle = tokio::spawn(async move {
                            while let Some(chunk) = rx.recv().await {
                                let pid = *last_spawn_pid_for_forward.lock().await;
                                let bid_opt = *last_builtin_id_for_forward.lock().await;
                                let make_output =
                                    |bytes: &Vec<u8>| match String::from_utf8(bytes.clone()) {
                                        Ok(s) => ProcessOutput::Text(s),
                                        Err(_) => ProcessOutput::Data(bytes.clone()),
                                    };
                                let msg = match &chunk {
                                    OutputChunk::Stdout(bytes) => {
                                        if pid != 0 {
                                            Notification::Stdout { owner: OutputOwner::Process { pid }, output: make_output(bytes) }
                                        } else if let Some(bid) = bid_opt {
                                            Notification::Stdout { owner: OutputOwner::Builtin { bid }, output: make_output(bytes) }
                                        } else {
                                            // Default to process with pid 0 if neither is available
                                            Notification::Stdout { owner: OutputOwner::Process { pid: 0 }, output: make_output(bytes) }
                                        }
                                    }
                                    OutputChunk::Stderr(bytes) => {
                                        if pid != 0 {
                                            Notification::Stderr { owner: OutputOwner::Process { pid }, output: make_output(bytes) }
                                        } else if let Some(bid) = bid_opt {
                                            Notification::Stderr { owner: OutputOwner::Builtin { bid }, output: make_output(bytes) }
                                        } else {
                                            Notification::Stderr { owner: OutputOwner::Process { pid: 0 }, output: make_output(bytes) }
                                        }
                                    }
                                };
                                if write_json(&write_half_output, &msg).await.is_err() {
                                    break;
                                }

                                // Tee to server console
                                match chunk {
                                    OutputChunk::Stdout(bytes) => {
                                        let _ = std::io::stdout().write_all(&bytes);
                                        let _ = std::io::stdout().flush();
                                    }
                                    OutputChunk::Stderr(bytes) => {
                                        let _ = std::io::stderr().write_all(&bytes);
                                        let _ = std::io::stderr().flush();
                                    }
                                }
                            }
                        });

                        // Execute the command directly with custom params
                        let exec_result = shell
                            .shell_mut()
                            .as_mut()
                            .run_string(params.command.clone(), &exec_params)
                            .await;
                        // Close parent's writers so readers can finish
                        drop(exec_params);

                        // Ensure all chunks are forwarded before finishing
                        let _ = stdout_handle.await;
                        let _ = stderr_handle.await;
                        let _ = forward_handle.await;

                        // Flush deferred builtin/process exit notifications now
                        {
                            let mut v = deferred_builtin_exits.lock().await;
                            for (id, code) in v.drain(..) {
                                let msg = Notification::BuiltinExit { id, exit_code: code };
                                write_json(&write_half, &msg).await?;
                            }
                        }
                        {
                            let mut v = deferred_proc_exits.lock().await;
                            for (pid, code) in v.drain(..) {
                                let msg = Notification::ProcessExit { pid, exit_code: code };
                                write_json(&write_half, &msg).await?;
                            }
                        }

                        let exit_code = match exec_result {
                            Ok(res) => res.exit_code,
                            Err(_) => 1,
                        };
                        let done = Notification::CommandEnd { id: exec_id, exit_code: exit_code as i32 };
                        write_json(&write_half, &done).await?;

                        if let Some(id) = id {
                            let resp = Response { id, result: RunCommandEndResult { exit_code: exit_code as i32 } };
                            write_json(&write_half, &resp).await?;
                        }
                    }
                    Err(e) => {
                        if let Some(id) = id {
                            let resp = ErrorResponse { id, error: ErrorObject { code: -32602, message: e.to_string() } };
                            write_json(&write_half, &resp).await?;
                        }
                    }
                }
            }
            RpcIncoming::GetEnv { id } => {
                if let Some(id) = id {
                    let shell_arc = shell.shell();
                    let shell_ref = shell_arc.as_ref();
                    let mut env_map = std::collections::BTreeMap::new();
                    for (k, v) in shell_ref.env.iter() {
                        env_map.insert(k.clone(), v.value().to_cow_str(shell_ref).into_owned());
                    }
                    #[derive(Serialize)]
                    struct EnvPayload { env: std::collections::BTreeMap<String, String> }
                    let resp = Response { id, result: EnvPayload { env: env_map } };
                    write_json(&write_half, &resp).await?;
                }
            }
            RpcIncoming::GetAliases { id } => {
                if let Some(id) = id {
                    let shell_arc = shell.shell();
                    let shell_ref = shell_arc.as_ref();
                    let mut alias_map = std::collections::BTreeMap::new();
                    for (k, v) in &shell_ref.aliases {
                        alias_map.insert(k.clone(), v.clone());
                    }
                    #[derive(Serialize)]
                    struct AliasesPayload { aliases: std::collections::BTreeMap<String, String> }
                    let resp = Response { id, result: AliasesPayload { aliases: alias_map } };
                    write_json(&write_half, &resp).await?;
                }
            }
            RpcIncoming::GetEnvVar { id, params } => {
                if let Some(id) = id {
                    let shell_arc = shell.shell();
                    let shell_ref = shell_arc.as_ref();
                    let value = shell_ref.env.get_str(&params.name, shell_ref).map(|c| c.into_owned());
                    #[derive(Serialize)]
                    struct ValuePayload { value: Option<String> }
                    let resp = Response { id, result: ValuePayload { value } };
                    write_json(&write_half, &resp).await?;
                }
            }
            RpcIncoming::GetAlias { id, params } => {
                if let Some(id) = id {
                    let shell_arc = shell.shell();
                    let shell_ref = shell_arc.as_ref();
                    let value = shell_ref.aliases.get(&params.name).cloned();
                    #[derive(Serialize)]
                    struct ValuePayload { value: Option<String> }
                    let resp = Response { id, result: ValuePayload { value } };
                    write_json(&write_half, &resp).await?;
                }
            }
            RpcIncoming::SetCwd { id, params } => {
                let result = (|| {
                    shell
                        .shell_mut()
                        .as_mut()
                        .set_working_dir(&params.cwd)
                        .map_err(|e| e.to_string())?;
                    std::env::set_current_dir(&params.cwd).map_err(|e| e.to_string())?;
                    Ok::<_, String>(())
                })();

                let ok = result.is_ok();
                if let Some(id) = id {
                    #[derive(Serialize)]
                    #[serde(rename_all = "camelCase")]
                    struct SetCwdPayload { ok: bool, error: Option<String> }
                    let resp = Response { id, result: SetCwdPayload { ok, error: result.err() } };
                    write_json(&write_half, &resp).await?;
                }
                if ok {
                    let notify = Notification::CwdChanged { cwd: params.cwd };
                    write_json(&write_half, &notify).await?;
                }
            }
            RpcIncoming::SetEnv { id, params } => {
                let result = shell
                    .shell_mut()
                    .as_mut()
                    .env
                    .update_or_add(
                        &params.key,
                        ShellValueLiteral::Scalar(params.value.clone()),
                        |var| {
                            var.export();
                            Ok(())
                        },
                        EnvironmentLookup::Anywhere,
                        EnvironmentScope::Global,
                    )
                    .map_err(|e| e.to_string());

                if result.is_ok() {
                    unsafe {
                        std::env::set_var(&params.key, &params.value);
                    }
                }

                if let Some(id) = id {
                    match &result {
                        Ok(_) => {
                            #[derive(Serialize)]
                            struct Empty {}
                            let resp = Response { id, result: Empty {} };
                            write_json(&write_half, &resp).await?;
                        }
                        Err(e) => {
                            let resp = ErrorResponse { id, error: ErrorObject { code: -32603, message: e.clone() } };
                            write_json(&write_half, &resp).await?;
                        }
                    }
                }

                if result.is_ok() {
                    let notify = Notification::EnvChanged {
                        key: params.key,
                        value: params.value,
                    };
                    write_json(&write_half, &notify).await?;
                }
            }
            RpcIncoming::SetAlias { id, params } => {
                shell
                    .shell_mut()
                    .as_mut()
                    .aliases
                    .insert(params.name.clone(), params.value.clone());
                brush_core::emit_alias_event(brush_core::AliasEvent::Set {
                    name: params.name.clone(),
                    value: params.value.clone(),
                });
                if let Some(id) = id {
                    #[derive(Serialize)]
                    struct Empty {}
                    let resp = Response { id, result: Empty {} };
                    write_json(&write_half, &resp).await?;
                }
            }
            RpcIncoming::UnsetAlias { id, params } => {
                let removed = shell
                    .shell_mut()
                    .as_mut()
                    .aliases
                    .remove(&params.name)
                    .is_some();
                if removed {
                    brush_core::emit_alias_event(brush_core::AliasEvent::Unset {
                        name: params.name.clone(),
                    });
                }
                if let Some(id) = id {
                    #[derive(Serialize)]
                    struct Removed { removed: bool }
                    let resp = Response { id, result: Removed { removed } };
                    write_json(&write_half, &resp).await?;
                }
            }
            RpcIncoming::GetForegroundPid { id } => {
                if let Some(id) = id {
                    let pid = brush_core::get_foreground_pid().unwrap_or(0);
                    #[derive(Serialize)]
                    struct Pid { pid: i32 }
                    let resp = Response { id, result: Pid { pid } };
                    write_json(&write_half, &resp).await?;
                }
            }
        }
    }

    Ok(())
}
