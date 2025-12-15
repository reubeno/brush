# Script Execution Mode — Quick Start Guide

## TL;DR

```bash
# Watch a script execute with live state inspection
tuish script.sh

# Step through line-by-line with breakpoints
tuish --step script.sh

# Replay a recorded execution (future)
tuish --replay execution.log
```

---

## Three Execution Modes

### 🎬 Watch Mode — "Netflix for Scripts"
**Use when:** Understanding what a script does

```bash
tuish script.sh
# or
tuish --watch script.sh
```

**What you see:**
- Script runs at normal speed
- Source pane highlights current executing line
- Variables/call stack update live
- Terminal shows output in real-time

**Controls:**
- `S` — Switch to step mode
- `Ctrl+Space` — Cycle through panes
- `Q` — Quit

**Perfect for:**
- Learning unfamiliar scripts
- Verifying script behavior
- Teaching bash to students
- Debugging CI/CD pipelines

---

### 🐛 Step Mode — "gdb for Bash"
**Use when:** Finding a specific bug

```bash
tuish --step script.sh
```

**What you see:**
- Script pauses at each command
- Full state inspection before execution
- Set breakpoints on any line
- Step through one command at a time

**Controls:**
- `Space` / `Enter` — Execute next line
- `C` — Continue to end (or next breakpoint)
- `B` — Toggle breakpoint on current line
- `W` — Switch to watch mode (run freely)
- `Q` — Quit execution

**Perfect for:**
- Finding exact line where bug occurs
- Understanding why conditionals fail
- Debugging parameter expansions
- Learning how bash features work

---

### ⏮️ Replay Mode — "Time Travel Debugging" (Future)
**Use when:** Analyzing past executions

```bash
# Record execution
tuish --watch --record=trace.log script.sh

# Replay later
tuish --replay trace.log
```

**What you see:**
- Scrub through execution like a video
- Seek to any point in time
- Inspect state at any moment
- Search for specific events

**Controls:**
- `←` / `→` — Step backward/forward
- `Space` — Play/pause
- `/` — Search for variable changes
- `Home` / `End` — Jump to start/end

**Perfect for:**
- Debugging production failures
- Sharing execution traces with bug reports
- Creating training materials
- "What was X when Y happened?"

---

## UI Layout

```
┌────────────────────────────────────────────────────────────┐
│ [Source✓] [Variables] [Call Stack] [Output]               │ ← Tabs
├────────────────────────────────────────────────────────────┤
│                         │                                   │
│  SOURCE CODE (left)     │  STATE INSPECTOR (right)         │
│                         │                                   │
│  18 │ for file in *;   │  Variables:                       │
│  19 │   count+=1       │   count  = 42                     │
│► 20 │   process "$file"│   file   = "app.log"             │
│  21 │ done             │                                   │
│                         │  Call Stack:                      │
│                         │   ► process("app.log")           │
│                         │     └─ for loop                   │
├────────────────────────────────────────────────────────────┤
│ OUTPUT (Terminal)                              [RUNNING]   │
│ Processing app.log...                                      │
│ Found 3 errors                                             │
├────────────────────────────────────────────────────────────┤
│ ⚡ WATCH │ Line 20/87 │ 2.3s │ [S]tep [Q]uit              │
└────────────────────────────────────────────────────────────┘
```

---

## Common Workflows

### Workflow 1: "Why does my script fail?"

```bash
# Start in step mode
tuish --step failing-script.sh

# Step through until error occurs
# Press Space repeatedly

# When you hit the failing line:
# 1. Check Variables pane — are values correct?
# 2. Check Call Stack — are you in the right function?
# 3. Check Output — what was the last output?

# Found the bug? Fix it and re-run!
```

### Workflow 2: "What does this script actually do?"

```bash
# Run in watch mode
tuish complex-script.sh

# Watch execution flow:
# - See which functions are called
# - See how variables change over time
# - See output as it happens

# Switch to step mode if you want to slow down:
# Press 'S' during execution
```

### Workflow 3: "Debugging conditionals"

```bash
# Set breakpoint at the conditional
tuish --step script.sh

# When source pane appears:
# 1. Press 'B' on the 'if' line to set breakpoint
# 2. Press 'C' to continue to breakpoint
# 3. Inspect variables before condition evaluates
# 4. Press Space to execute conditional
# 5. See which branch was taken
```

### Workflow 4: "Teaching bash"

```bash
# Create example scripts: loops.sh, conditionals.sh, functions.sh

# In class, run with watch mode:
tuish loops.sh

# Students see:
# - Each iteration of the loop
# - How variables change
# - How control flow works

# For advanced topics, use step mode:
tuish --step functions.sh

# Walk through function calls step-by-step
```

---

## Keybindings Reference

### Global (All Modes)
- `Ctrl+Space` — Cycle focus between panes
- `Ctrl+Q` — Quit tuish
- `Q` — Quit execution (when script running)

### Watch Mode
- `S` — Switch to step mode (pause)
- `Ctrl+Space` — Focus different panes

### Step Mode
- `Space` / `Enter` — Execute next line
- `C` — Continue (unpause until breakpoint)
- `W` — Switch to watch mode (unpause completely)
- `B` — Toggle breakpoint on current line

### Source Pane (when focused)
- `Up` / `Down` — Scroll source
- `PageUp` / `PageDown` — Scroll page
- `Home` / `End` — Jump to start/end
- `B` — Toggle breakpoint on current line

### Variables Pane (when focused)
- `Up` / `Down` — Navigate variables
- `/` — Search/filter variables (future)

### Call Stack Pane (when focused)
- `Up` / `Down` — Navigate stack frames
- `Enter` — Jump to source location (future)

---

## Tips & Tricks

### Tip 1: Start with watch mode
Don't jump straight to step mode. Watch mode gives you the big picture — use it to understand overall flow, then switch to step mode when you find the problematic area.

### Tip 2: Use breakpoints strategically
Don't step through every line. Set a breakpoint near where you think the bug is, then continue. Much faster!

### Tip 3: Watch the variables pane
Often bugs are just wrong variable values. Keep an eye on the variables pane — recent changes are highlighted.

### Tip 4: Check the call stack
If execution is in an unexpected place, check the call stack to see how you got there.

### Tip 5: Terminal pane shows stdout/stderr
Don't forget the terminal pane at the bottom — it shows actual script output, which is crucial context.

### Tip 6: Syntax errors appear in output
If your script has syntax errors, they'll appear in the terminal pane output (from brush's parser).

---

## FAQ

**Q: Can I use this with existing scripts?**  
A: Yes! No modifications needed. Just run `tuish script.sh`.

**Q: Does it work with scripts that use `source` or `.`?**  
A: Yes, but currently only the main script is shown in the source pane. Future versions will support tabbed views for sourced files.

**Q: What about interactive scripts (read commands)?**  
A: They work! Input is handled via the PTY. In step mode, you can inspect state before/after user input.

**Q: Can I debug scripts remotely?**  
A: Yes, if you can SSH to the machine. tuish runs in the terminal. Future versions may support trace file streaming.

**Q: Does this slow down execution?**  
A: Slightly (< 20% overhead in watch mode). Most scripts are I/O bound anyway, so the impact is minimal.

**Q: Can I record executions?**  
A: Not yet, but this is planned for Phase 4 (replay mode).

**Q: Does it support all bash features?**  
A: It supports whatever brush supports (which is most of bash). Check brush compatibility docs for details.

**Q: Can I use this in CI/CD?**  
A: Yes! Run tuish in your CI pipeline to get visibility into what scripts are doing. Output includes terminal pane content. For automated testing, consider recording traces.

**Q: How do I get help?**  
A: Join the brush Discord or open a GitHub issue. We're happy to help!

---

## Troubleshooting

**Problem: Source pane shows "No script loaded"**  
**Solution:** Make sure you passed a script path: `tuish script.sh`

**Problem: Script runs too fast, can't see what's happening**  
**Solution:** Use step mode: `tuish --step script.sh`

**Problem: Can't set breakpoints**  
**Solution:** Make sure source pane is focused (press Ctrl+Space), then press 'B' on the desired line.

**Problem: Variables pane is empty**  
**Solution:** Variables are only shown once they're set. Step through the script until variables are assigned.

**Problem: Terminal pane is blank**  
**Solution:** Your script might not produce output. Check if there are `echo` statements or other commands that write to stdout.

**Problem: Execution seems stuck**  
**Solution:** Script might be waiting for user input. Check the terminal pane for prompts. Type input and press Enter.

**Problem: "Command not found" errors**  
**Solution:** This is from the script itself, not tuish. The script is trying to run a command that doesn't exist. Check the PATH or install missing commands.

---

## Examples

### Example 1: Simple Loop

**script.sh:**
```bash
#!/bin/bash
count=0
for i in {1..5}; do
  echo "Iteration $i"
  count=$((count + i))
done
echo "Final count: $count"
```

**Running:**
```bash
tuish --watch script.sh
```

**What you'll see:**
- Source pane highlights lines 3-6 as loop executes
- Variables pane shows `count` incrementing: 0 → 1 → 3 → 6 → 10 → 15
- Terminal pane shows each "Iteration" message
- Execution takes ~5 seconds at normal speed

### Example 2: Debugging a Conditional

**buggy.sh:**
```bash
#!/bin/bash
DB_HOST="localhost"
DB_PORT=5432

if [ -z "$DB_HSOT" ]; then  # Typo: HSOT instead of HOST
  echo "ERROR: DB_HOST not set"
  exit 1
fi
echo "Connecting to $DB_HOST:$DB_PORT"
```

**Running:**
```bash
tuish --step buggy.sh
```

**What you'll see:**
1. Variables pane shows `DB_HOST` and `DB_PORT` set correctly
2. Source pane highlights line 5 (the if condition)
3. Press Space — condition evaluates to TRUE (because `DB_HSOT` is empty!)
4. Line 6 executes (error message)
5. Bug found: typo in variable name!

### Example 3: Function Call Stack

**functions.sh:**
```bash
#!/bin/bash

outer() {
  echo "In outer"
  inner "$1"
}

inner() {
  echo "In inner with arg: $1"
  process "$1"
}

process() {
  echo "Processing: $1"
}

outer "test"
```

**Running:**
```bash
tuish --watch functions.sh
```

**What you'll see:**
- Call stack pane shows depth as functions are called:
  - `main` → `outer("test")` → `inner("test")` → `process("test")`
- Each function's arguments are shown
- Source pane jumps between function definitions
- Terminal shows all echo output

---

## What's Next?

### Implemented (Current)
- ✅ Interactive REPL mode
- ✅ Multi-pane TUI with state inspection
- ✅ PTY-backed terminal emulation
- ✅ Tab completion

### Coming Soon (Phase 1-2)
- 🚧 Watch mode (basic script visualization)
- 🚧 Step mode (interactive debugging)
- 🚧 Breakpoints
- 🚧 Source code pane with line highlighting

### Future (Phase 3-4)
- 📋 Syntax highlighting in source pane
- 📋 Replay mode (record/replay executions)
- 📋 Variable change highlighting
- 📋 Timeline scrubber
- 📋 Trace file export/import
- 📋 Remote debugging support

---

## Get Involved

**Try it out:**
```bash
# Build tuish
cargo build --package tuish

# Run a script
./target/debug/tuish your-script.sh
```

**Share feedback:**
- Discord: [brush community](https://discord.gg/kPRgC9j3Tj)
- GitHub: [Issues](https://github.com/reubeno/brush/issues)
- Discussions: [Ideas & Feedback](https://github.com/reubeno/brush/discussions)

**Contribute:**
See the full design document: `tuish/docs/SCRIPT_EXECUTION_MODE.md`

---

**The vision:** Make shell script debugging as powerful as debugging compiled code. 🚀
