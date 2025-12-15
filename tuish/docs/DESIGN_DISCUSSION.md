# Script Execution Mode: Design Discussion & Refinements

## Questions from Design Review

### Q1: Should we implement DAP for VSCode instead?

**Short Answer:** No, not instead. TUI mode is uniquely valuable. But DAP could be a *future* addition.

**Reasoning:**

**DAP (Debug Adapter Protocol) Pros:**
- ✅ Familiar interface for VSCode users
- ✅ Rich IDE features (hover, inline values, etc.)
- ✅ Large existing user base

**DAP Cons:**
- ❌ Requires GUI/IDE (can't use over SSH in terminal)
- ❌ Heavy dependency (VSCode, LSP infrastructure)
- ❌ Doesn't showcase brush/tuish uniqueness
- ❌ Already exists: `bash-debug` VSCode extension (though limited)

**TUI Unique Value:**
- ✅ Works anywhere (SSH, containers, headless servers)
- ✅ Showcases brush's capabilities (embeddable, observable)
- ✅ No external dependencies
- ✅ Faster iteration (no IDE protocol overhead)
- ✅ **Demonstrates brush architecture** — live introspection into shell internals

**Recommendation:** 
- **Phase 1-3:** Focus on TUI (unique value, faster to demo)
- **Future:** Consider DAP as "brush-dap" crate that reuses same observability hooks
- **Architecture:** Design ExecutionObserver to support both (TUI and DAP could share the same backend)

**Analogy:** GDB has both TUI mode (`gdb -tui`) and DAP support. Both are valuable for different contexts.

---

### Q2: Would users actually use capture/replay?

**Short Answer:** Uncertain. It's high effort for speculative value. **Recommend deferring to "future work"** unless we see demand.

**Who might use it:**

1. **SREs debugging production incidents** 
   - "Script failed at 3am, let's replay what happened"
   - **But:** Most scripts are idempotent or have logs anyway
   - **Reality check:** How often do you replay bash executions vs. just re-run?

2. **Educators creating tutorials**
   - "Watch how this expert wrote this script"
   - **But:** Could just record a video/asciinema
   - **Reality check:** Marginal improvement over screen recording

3. **Bug report sharing**
   - "Here's the exact execution that failed"
   - **But:** Stack traces + variable dumps usually sufficient
   - **Reality check:** Logs + exit codes cover 95% of cases

**Challenges:**
- 🔴 Trace files could be huge (every variable change, every command)
- 🔴 Non-determinism (time, network, filesystem changes)
- 🔴 Sensitive data in traces (passwords, API keys)
- 🔴 Complex state reconstruction

**Recommendation:**
- ❌ **Remove from MVP** (Phases 1-3)
- ❌ **Remove "Replay Mode" from initial design**
- ✅ **Keep trace logging** as optional feature (`--trace-to-file`)
  - Simple append-only event log (for post-mortem analysis)
  - No replay UI, just structured data
  - Users can grep/jq the trace file
- ✅ **Add replay mode later** if users request it (v2.0 feature)

**Alternative:** Focus effort on **"Pause & Inspect"** mode instead:
- User can Ctrl+Z during watch mode
- Inspect full state while paused
- Continue or step from there
- Much simpler, still powerful

---

### Q3: Killer demo scenario for live x-ray?

**Short Answer:** **CI/CD pipeline debugging** is the killer demo. It's painful today and tuish makes it trivial.

#### 🎯 Killer Demo: "Debug GitHub Actions in 60 seconds"

**The Pain:**
```yaml
# .github/workflows/deploy.yml
- name: Deploy to production
  run: |
    ./scripts/deploy.sh prod
    # ❌ FAILS: "Connection refused"
    # ❌ Have to re-run entire workflow to add debug prints
    # ❌ Takes 10+ minutes per iteration
    # ❌ Can't see intermediate state
```

**With tuish:**
```bash
# Pull the failing script from GitHub
gh run download 12345 --name deploy-logs
cat deploy.sh  # See the script

# Run with tuish locally
tuish --watch deploy.sh

# 🎬 Live visualization shows:
# Line 42: DB_HOST="${DB_HOST:-localhost}"  ← defaults to localhost!
# Line 43: curl "http://${DB_HOST}:5432"     ← tries localhost, not prod!
# Variables pane shows: DB_HOST = "localhost" (should be "prod-db.company.com")

# 🐛 Bug found in 30 seconds:
#    PROD_DB_HOST was set in CI, but script reads DB_HOST
#    Variable name mismatch!
```

**Why this is killer:**
- 🎯 **Common pain point** (everyone debugs CI/CD failures)
- 🎯 **Massive time savings** (minutes → seconds)
- 🎯 **Obvious value** (can see the bug immediately)
- 🎯 **Demo-friendly** (fits in 60 seconds)
- 🎯 **Not possible today** (bash -x is overwhelming, logs don't show state)

#### Alternative Killer Demos

**Demo 2: "Understand someone else's script"**
```bash
# You inherit a complex deployment script
# 500 lines, no comments, many functions

tuish --watch legacy-deploy.sh

# Watch mode shows:
# - Which functions are called in what order
# - How variables flow through the script
# - What external commands are invoked
# - Where it spends time (slow curl calls, etc.)

# Understand the script in 10 minutes vs 2 hours of reading
```

**Demo 3: "Debug bash dark magic"**
```bash
# Script uses advanced bash features
# Parameter expansion: ${var:+alt}${var:=default}
# You don't understand what it does

tuish --step weird-script.sh

# Step through each expansion
# Variables pane shows BEFORE and AFTER values
# See exactly how ${var:+alt} evaluates
# Instant learning!
```

**Recommendation:** Lead with **CI/CD demo** (#1). It's the most relatable and valuable.

---

### Q4: What can we demo TODAY without hooks in brush-core?

**Short Answer:** Yes! We can build a **simplified "live x-ray" using existing tracing**.

#### MVP Demo Without Hooks (1-2 days of work)

**What we can do NOW:**
1. ✅ **Source code pane** — Load script, display with line numbers
2. ✅ **Parse with brush-parser** — Already works, gives us AST
3. ✅ **Variables pane** — Query shell state periodically (poll every 100ms)
4. ✅ **Call stack pane** — Already exists in tuish!
5. ✅ **Terminal output** — Already works via PTY

**Approach: "Polling + Inference"**

```rust
// In tuish/src/script_mode.rs (new file)

/// Simple script visualizer without execution hooks
pub struct ScriptVisualizer {
    source_pane: SourceCodePane,
    shell: Arc<Mutex<Shell>>,
    script_ast: brush_parser::ast::Program,
}

impl ScriptVisualizer {
    async fn run_with_polling(&mut self, script_path: &Path) -> Result<()> {
        // 1. Load and parse script
        let script_content = std::fs::read_to_string(script_path)?;
        self.source_pane.load_script(script_path)?;
        self.script_ast = brush_parser::parse(&script_content)?;
        
        // 2. Start script execution in background
        let shell = self.shell.clone();
        let content = script_content.clone();
        let exec_task = tokio::spawn(async move {
            let mut shell = shell.lock().await;
            shell.run_string(content, &SourceInfo::default(), &ExecutionParameters::default()).await
        });
        
        // 3. Poll shell state while script runs
        let mut last_vars = HashMap::new();
        let mut poll_interval = tokio::time::interval(Duration::from_millis(100));
        
        loop {
            // Check if execution finished
            if exec_task.is_finished() {
                break;
            }
            
            // Poll shell state
            if let Ok(shell) = self.shell.try_lock() {
                // Get current variables
                let current_vars = shell.env().variables().clone();
                
                // Detect changes
                for (name, value) in &current_vars {
                    if last_vars.get(name) != Some(value) {
                        // Variable changed! Highlight in UI
                        self.highlight_variable_change(name, value);
                    }
                }
                
                last_vars = current_vars;
                
                // Update call stack display
                self.update_call_stack_display(shell.call_stack());
            }
            
            // Render UI
            self.render()?;
            
            // Handle input
            self.handle_events()?;
            
            poll_interval.tick().await;
        }
        
        Ok(())
    }
}
```

**What this gives us:**
- ✅ Source code visible while script runs
- ✅ Variables update in real-time (polled)
- ✅ Call stack visible
- ✅ Terminal output visible
- ❌ **No line highlighting** (don't know current line without hooks)
- ❌ **No command tracing** (don't know what's executing)
- ❌ **Can't step** (no pause points)

**But we CAN fake line highlighting!**

Use existing `tracing` infrastructure:

```rust
// In tuish: Subscribe to tracing events

use tracing_subscriber::layer::SubscriberExt;

// Create a custom tracing layer that captures events
struct TuishTracingLayer {
    event_tx: mpsc::Sender<TraceEvent>,
}

impl<S> Layer<S> for TuishTracingLayer 
where S: Subscriber {
    fn on_event(&self, event: &Event, _ctx: Context<S>) {
        // Parse tracing events for hints about execution
        if event.metadata().target() == trace_categories::COMMANDS {
            // Extract source info from event if available
            // Send to UI for highlighting
        }
    }
}

// In script mode, install this layer
let (tx, rx) = mpsc::channel(100);
let layer = TuishTracingLayer { event_tx: tx };
tracing_subscriber::registry().with(layer).init();
```

**Existing tracing points we can use:**
- `trace_categories::COMMANDS` — Command execution
- `trace_categories::FUNCTIONS` — Function entry/exit (already logs depth!)
- `trace_categories::EXPANSION` — Variable expansion

**Demo v0.1: "Lite X-Ray" (No hooks needed)**

```bash
tuish --watch-lite script.sh

# Shows:
# - Source code (static, no highlighting)
# - Variables updating in real-time (polled)
# - Call stack depth (from tracing::debug! calls)
# - Terminal output

# Marketing: "See your script's state evolve in real-time"
# Not full x-ray, but still valuable!
```

#### Migration Path to Full X-Ray

**Phase 0 (Now): Lite Mode via Tracing**
```
Source Pane: ✅ Static display
Variables:   ✅ Polled updates
Call Stack:  ✅ Via tracing events
Line Trace:  ⚠️  Inferred from tracing (imperfect)
Commands:    ❌ Not visible
```

**Phase 1 (After hooks): Full X-Ray**
```
Source Pane: ✅ Current line highlighted
Variables:   ✅ Change events (not polled)
Call Stack:  ✅ Full stack with args
Line Trace:  ✅ Accurate from SourceInfo
Commands:    ✅ Command text + expansion visible
```

**Design to Leave Space:**

```rust
// In tuish/src/execution_observer.rs

/// Execution event source (abstracts over tracing vs hooks)
pub enum EventSource {
    /// Events from tracing subscriber (Phase 0)
    TracingEvents(TracingEventReceiver),
    
    /// Events from execution hooks (Phase 1+)
    ExecutionHooks(ExecutionObserver),
}

impl EventSource {
    pub async fn next_event(&mut self) -> Option<ExecutionEvent> {
        match self {
            Self::TracingEvents(rx) => {
                // Parse tracing events into ExecutionEvent
                rx.recv().await.map(|trace_event| {
                    ExecutionEvent::from_tracing(trace_event)
                })
            }
            Self::ExecutionHooks(observer) => {
                // Receive from observer channel
                observer.recv().await
            }
        }
    }
}
```

**This design:**
- ✅ Lets us ship something NOW
- ✅ Doesn't block on brush-core changes
- ✅ Validates the UI/UX with real users
- ✅ Provides clear upgrade path
- ✅ Hooks slot in cleanly when ready

---

## Revised Recommendation

### Phase 0 (Ship NOW — 2-3 days)
**"Lite X-Ray Mode"** using existing tracing

**Deliverables:**
- ✅ `tuish --watch script.sh` works
- ✅ Source pane shows script
- ✅ Variables pane polls state (100ms interval)
- ✅ Call stack inferred from tracing
- ✅ Terminal shows output
- ✅ Simple but **immediately valuable**

**Marketing:** "See your script's state in real-time"

**Demo:** CI/CD debugging (can still see variable values, just not line-by-line)

### Phase 1 (After hooks — 2 weeks)
**"Full X-Ray Mode"** with ExecutionObserver

**Deliverables:**
- ✅ Current line highlighting (accurate)
- ✅ Command text visible
- ✅ Variable change events (not polled)
- ✅ Precise source location tracking
- ✅ **Step mode** (pause/continue)

**Marketing:** "X-ray vision for shell scripts"

**Demo:** Full CI/CD debugging with step-through

### Phase 2 (Future — 1 week)
**Polish & Advanced Features**

**Deliverables:**
- ✅ Syntax highlighting
- ✅ Breakpoints
- ✅ Variable search/filter
- ✅ Better UX (status bar, hints)

### Phase 3 (v2.0 — Optional)
**Replay Mode** (only if users request it)

**Deliverables:**
- ✅ `--record` flag writes trace to file
- ✅ `--replay` mode for time-travel debugging

---

## Revised Killer Demo Script

### Demo: "Find CI/CD Bug in 60 Seconds"

**Setup:**
```bash
# GitHub Actions workflow fails mysteriously
# Error: "Failed to connect to database"
# But DATABASE_URL is set in secrets!

# Pull the script
gh run download 12345
```

**Act 1: The Old Way (15 seconds of pain)**
```bash
# Try to debug with logs
cat workflow-log.txt
# Output:
#   Deploying to production...
#   Connecting to database...
#   Error: Connection refused

# Add debug prints, push, wait 10 minutes...
# Repeat 3-4 times
# Total time: 45 minutes
```

**Act 2: The tuish Way (45 seconds of joy)**
```bash
# Run with tuish
tuish --watch deploy.sh

# 🎬 Watch execution:
# Line 15: DB_URL="${DATABASE_URL:-localhost}"
#          ^^^^^^^^^ BUG! Reads DATABASE_URL but defaults to localhost
#
# Variables pane shows:
#   DATABASE_URL = (unset)  ← Not exported from secrets!
#   DB_URL       = "localhost"  ← Wrong!
#
# Line 16: psql -h "$DB_URL"  ← Connects to localhost, not prod

# 🎯 Bug found: Variable name mismatch
# Fix: Change line 15 to use correct variable name
# Total time: 60 seconds
```

**Act 3: The Fix**
```bash
# Fix the script
sed -i 's/DATABASE_URL/DB_URL/' deploy.sh

# Verify with tuish
tuish --watch deploy.sh
# Variables pane now shows:
#   DB_URL = "prod-db.company.com" ✅

# Push and deploy with confidence!
```

**Why this works:**
- 🎯 Relatable problem (CI/CD pain is universal)
- 🎯 Clear before/after (45 min → 60 sec)
- 🎯 Visual impact (seeing the variable mismatch is obvious)
- 🎯 Immediate value (can use it today)

---

## Action Items

### Immediate (This Weekend)
1. ✅ **Build Phase 0** (Lite X-Ray with tracing)
   - Source pane
   - Variables polling
   - Tracing integration
   - Basic demo

2. ✅ **Record demo video**
   - CI/CD debugging scenario
   - 60-second format
   - Share on Discord/Twitter

3. ✅ **Get feedback**
   - Does this solve a real problem?
   - Is Lite mode useful enough?
   - What's missing?

### Short Term (Next 2 Weeks)
1. 🔄 **Work with brush-core team on hooks**
   - Design ExecutionObserver trait
   - Identify hook points
   - Implement minimal hooks

2. 🔄 **Upgrade to Full X-Ray (Phase 1)**
   - Current line highlighting
   - Command tracing
   - Step mode

### Long Term (Q1 2025)
1. 📋 **Polish & release**
   - Syntax highlighting
   - Breakpoints
   - Documentation

2. 📋 **Consider DAP** (if demand exists)
   - Reuse ExecutionObserver backend
   - Implement DAP protocol
   - VSCode extension

3. 📋 **Consider Replay** (if users request)
   - Trace file format
   - Replay UI
   - Documentation

---

## Questions for Discussion

1. **Does Lite X-Ray (Phase 0) provide enough value to ship?**
   - My opinion: Yes, seeing variable state in real-time is still valuable

2. **Is CI/CD debugging the right killer demo?**
   - Alternative: "Understand legacy scripts" or "Learn bash"

3. **Should we rename "watch mode" to something else?**
   - "Live mode"? "Observe mode"? "X-ray mode"?

4. **DAP vs TUI: Should we do both eventually?**
   - My opinion: TUI first (faster iteration), DAP later if demand exists

5. **What's the timeline for hooks in brush-core?**
   - Affects when we can ship Full X-Ray (Phase 1)

---

**Bottom Line:**

- ✅ **Skip Replay Mode** (defer to v2.0 if users want it)
- ✅ **Build Lite X-Ray NOW** (ship something this week)
- ✅ **Lead with CI/CD demo** (killer use case)
- ✅ **Leave space for hooks** (clean upgrade path)
- ✅ **Consider DAP later** (reuse same backend)

The key insight: **Ship iteratively**. Lite mode proves the concept, Full mode delivers the promise, Replay mode is speculative luxury.
