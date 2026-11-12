# SPIKE: async stream hooks and lazily materialized flume pipes

Local exploratory implementation against upstream `reubeno/brush` commit
`4fe5edb673564ace41977deabbe156dea9b4165b`, on branch
`spike/async-io-and-flume-pipes`. Not a production-ready transport migration.
Nothing has been pushed and no PR has been opened.

The work is a two-commit series, applied in order:

1. **Async I/O conversion** — add async stream hooks and migrate `echo` onto
   them. Pipelines still use OS pipes. This commit is meant to compile and
   run on its own.
2. **Flume pipes with lazy fd mode** — introduce the channel transport and
   switch Unix pipeline edges to it, materializing a real descriptor when
   one is requested.

Most builtins still use the synchronous `write!` API; the first commit is
the conversion *path*, with `echo` as the template, not a rewrite of every
builtin.

## What changed

### Patch 1: async I/O

- `brush-core/src/openfiles.rs`: additive async read/write hooks on `Stream`
  and `OpenFile`. Legacy native file/terminal/stream I/O goes through Tokio's
  blocking pool; later channel streams override the hooks. Existing
  synchronous API remains available. New trait methods have defaults for
  existing embedders.
- `brush-core/src/commands.rs`: `ExecutionContext::{write_stdout,write_stderr,
  read_stdin}` helpers. The stdout/stderr write helpers write all and flush.
- `brush-builtins/src/echo.rs`: first migrated builtin; async stdout write.

### Patch 2: flume and fd materialization

- `brush-core/src/byte_pipe.rs`: binary-safe, cloneable, bounded flume byte
  channel, synchronous compatibility I/O, cooperative async I/O, Unix fd
  materialization, and nine unit tests.
- `brush-core/src/interp.rs`: every Unix pipeline edge starts as a channel;
  there is deliberately no builtin/external preclassification.
- `brush-core/src/lib.rs`, `brush-core/Cargo.toml`, `Cargo.lock`: module and
  `bytes`/`flume` dependencies. Flume's random fairness defaults are disabled,
  avoiding an unnecessary RNG dependency on WASM.
- `spike/smoke.py`: executable-shell differential smoke suite against bash.

The channel ends use `OpenFile::Stream`; its clone, display/serde label
(`stream`), non-directory and non-terminal behavior apply without adding
variants or changing exhaustive platform launcher matches.

## Transport state machine (implemented)

There are two independently shared endpoint states, each monotonically
transitioning from **Channel** to **Bridged**. Every clone of an endpoint sees
that transition. `OnceLock` owns the stable native handle, allowing borrowed
fds without unsafe lifetime tricks; the per-end mutex serializes I/O against
materialization.

```
                  materialize reader
 Channel/Channel --------------------> BridgedReader/Channel
       |                                         |
       | materialize writer                      | materialize writer
       v                                         v
 Channel/BridgedWriter ----------------> BridgedReader/BridgedWriter
                         materialize reader
```

- Initially, writes enqueue <=1024-byte `Bytes` messages, with a capacity of
  64 messages (approximately 64KiB, not an exact byte budget). Shared reader
  state holds the leftover chunk. Cloned readers cannot reorder short reads.
- Reader materialization creates an OS pipe. The reader endpoint switches to
  its read handle; **only its bridge** owns the flume receiver afterwards.
  The bridge writes any leftover partial chunk first, then queued messages,
  using `recv_async` and Tokio Unix `AsyncWrite`. Surviving in-process reader
  clones now read the OS pipe, never a second flume receiver.
- Writer materialization creates an OS pipe. All writer clones switch to its
  write handle; the bridge owns the sole flume sender and runs Tokio Unix
  `AsyncRead` -> `send_async`. Previously queued channel bytes precede new
  native writes. The two modes cannot overlap inside one endpoint operation.
- Both `try_clone_to_owned` (native child launch) and `try_borrow_as_fd`
  trigger materialization through `Stream`. Repeated calls reuse the same
  endpoint descriptor and do not start new bridges.
- A materialization error occurs before taking channel ownership. A running
  Tokio runtime with an enabled I/O reactor is required for materialization.
- Closing the last channel writer disconnects flume after queued bytes drain;
  the reader bridge drops its OS writer for EOF. A failed OS write drops its
  receiver, giving channel writers `BrokenPipe`. A writer bridge drops its
  sender on EOF or failure. OS handles belong to endpoint reference counts,
  not to a global transport object that accidentally keeps both ends alive.

### Important deviation: no single-pipe Native upgrade

When **both** ends materialize, the implementation retains **two OS pipes and
both bridges**, with the channel in the middle. External | external works,
including prefix data queued before either materialization; it does **not**
yet collapse to one shared OS pipe. This is intentionally safer than returning
an existing opposite OS end while a live bridge competes for its data, or
canceling a bridge that owns an in-flight partial write. The requested
single-pipe upgrade/fast path remains unfinished. A future version needs a
quiescence/acknowledgement protocol covering queued chunks and in-flight OS
writes before it can remove bridges without loss or reordering.

## Async migration boundary

No existing execution `spawn_blocking` calls were removed. Legacy synchronous
channel reads/writes use `try_recv`/`try_send`; when the channel would block
they wait on a shared `Condvar` (woken by the peer end's notify, with a 10ms
timed fallback recheck for lost-wakeup safety). They must stay on the
existing blocking execution paths; they are not suitable for general
runtime-worker use.

The channel async methods never call blocking flume `send`/`recv` and never
sleep. They wait event-driven: a `tokio::sync::Notify` per direction, enabled
before rechecking channel state under the endpoint mutex (lost-wakeup-safe).
Bridges, in contrast, use flume async operations and truly nonblocking Tokio
pipe I/O. Native I/O through the async compatibility hooks runs on the
blocking pool, with copied owned buffers. Native blocking operations are not
abortable after dispatch.

No flume receive future is retained across materialization, so a future can
never own a delivered chunk while a new bridge also receives. The mode check
happens under the endpoint mutex, so a bridge can never steal chunks from a
pending read.

## Intentionally unchanged / risks

- Command substitution retains its original OS pipe + `AsyncPipeReader` +
  blocking execution pairing: concurrent draining (PR #748) is not changed.
- Process substitution still creates native pipes when assigning real fd
  numbers. Here-docs/strings retain their existing OS pipe and Linux resizing.
- Most builtins still use synchronous I/O; functions/eval and compound commands
  retain their existing execution scheduling. Reedline is untouched.
- Unix pipelines use channels. Windows/WASM pipeline wiring stays on the
  original backend; WASI core compilation was checked, Windows was not.
  Unix materialization methods are absent on WASM, as before for native fd
  APIs. Custom non-Tokio spawners are not integrated with bridge creation.
- No bridge-task registry, join-on-cancel, or explicit child cancellation
  coordinator exists yet. Tasks normally terminate on EOF/BrokenPipe. An idle
  producer's OS reader bridge can linger until it produces bytes or closes,
  even if downstream has gone; immediate idle-disconnect notification is not
  implemented. Runtime shutdown cancels bridge tasks; abrupt cancellation can
  discard in-flight bytes. Native blocking-pool operations can outlive the
  awaiting future. These are known spike limitations, not solved cancellation.
- No job/wait/signal bookkeeping changed. `lastpipe`, `PIPESTATUS`, `pipefail`,
  function/eval/exec late materialization and `yes | head` have smoke coverage,
  but exhaustive job-control and single-worker legacy-builtin scheduling have
  not been proved safe. Most synchronous builtins cannot simply move to `spawn`.
- Binary bytes stay bytes through channels and bridges. Existing text/NUL
  processing in command substitution is unchanged.
- Async helpers preserve per-write ordering, not serialization of an entire
  multi-chunk write against concurrent writers (similar to large pipe writes).

## Try it

```sh
cd /workspace/brush-spike
cargo test -p brush-core --lib
cargo check --workspace --exclude brush-fuzz
cargo build -p brush-shell --bin brush
python3 spike/smoke.py
./target/debug/brush --noprofile --norc -c 'echo hello | /bin/cat'
./target/debug/brush --noprofile --norc -c 'f() { /bin/cat; }; echo late | f'
```

Optional portable compilation check (requires Rust WASI standard library):

```sh
cargo check -p brush-core --target wasm32-wasip1
```

## Validation

- `cargo test -p brush-core --lib`: 117 passed, 0 failed, including nine new
  byte-pipe tests (short reads, clones, EOF, BrokenPipe, single-worker async
  backpressure, each fd direction, both fd directions with buffered prefix,
  surviving OpenFile clones, and closed native consumer).
- `cargo check --workspace`: attempted; blocked by the unrelated existing
  `brush-fuzz`/`libfuzzer-sys` build requiring absent `c++`.
- `cargo check --workspace --exclude brush-fuzz`: passed, including builtins,
  clap/winnow adapters, interactive and shell crates.
- `cargo build -p brush-shell --bin brush`: passed.
- `python3 spike/smoke.py`: 19 passed, 0 failed against bash, including mixed
  transports, external-external, function/eval/exec, fd duplication and
  `/dev/fd`, backpressure, `yes | head`, lastpipe/status/pipefail, substitutions,
  heredocs, and binary NUL/0xff bytes.
- `cargo check -p brush-core --target wasm32-wasip1`: passed.
- `cargo test -p brush-builtins --lib`: 31 passed, 0 failed.
- `cargo check -p brush-core --features serde`: passed.
- `cargo fmt --all --check` and `git diff --check`: passed.

Logs are beside the checkout under `/workspace/brush-spike-*.log`.
