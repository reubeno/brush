# Script Execution Mode Documentation

## Overview

This directory contains the design documents for adding "Script Execution Mode" to tuish, enabling live visualization and debugging of bash scripts.

## Documents

### 1. [SCRIPT_EXECUTION_MODE.md](./SCRIPT_EXECUTION_MODE.md)
**The comprehensive design document** (34KB)

- Full UX vision for three modes (Watch, Step, Replay)
- Complete architecture with code examples
- 4-phase implementation roadmap
- UI mockups and design decisions

**Read this for:** Complete understanding of the full vision

### 2. [DESIGN_DISCUSSION.md](./DESIGN_DISCUSSION.md)
**Response to design review questions** (16KB)

- DAP vs TUI analysis
- Replay mode feasibility discussion
- **Killer demo scenario: CI/CD debugging**
- Phase 0 approach (ship without hooks)
- Revised recommendations

**Read this for:** Understanding design tradeoffs and decisions

### 3. [PHASE_0_PROTOTYPE.md](./PHASE_0_PROTOTYPE.md)
**Implementation guide for MVP** (17KB)

- Build "Lite X-Ray" mode using polling + tracing
- Complete code examples
- Demo flow and testing plan
- 2-3 day implementation timeline

**Read this for:** How to build and ship something THIS WEEKEND

### 4. [SCRIPT_MODE_QUICK_START.md](./SCRIPT_MODE_QUICK_START.md)
**User-facing guide** (12KB)

- How to use the three modes
- Keybindings reference
- Common workflows
- Tips & tricks
- FAQ and troubleshooting

**Read this for:** End-user documentation (once implemented)

## Quick Summary

### The Vision
**"X-ray vision for shell scripts"** — See execution flow, variable state, and call stack in real-time while scripts run, like a debugger but for bash.

### Three Modes

1. **Watch Mode** — See scripts execute at normal speed with live state
2. **Step Mode** — Pause and step through line-by-line (like gdb)
3. **Replay Mode** — Record and replay executions (future/optional)

### Implementation Strategy

#### Phase 0 (Ship NOW — 2-3 days)
**"Lite X-Ray"** using existing infrastructure:
- Poll shell state every 100ms
- Use tracing events for hints
- Source pane + variables + call stack + terminal
- **No brush-core changes needed**

#### Phase 1 (After hooks — 2 weeks)
**"Full X-Ray"** with ExecutionObserver:
- Accurate line highlighting
- Command text visible
- Step mode (pause/continue)
- Breakpoints

#### Phase 2 (1 week)
**Polish:**
- Syntax highlighting
- Variable change highlighting
- Better UX

#### Phase 3 (Future/Optional)
**Replay Mode** — Only if users request it

### Killer Demo: CI/CD Debugging

**The Pain:**
```
GitHub Actions fails with "Connection refused"
45 minutes of trial-and-error debugging
Adding prints, pushing, waiting, repeat...
```

**With tuish:**
```bash
tuish deploy.sh

# Variables pane immediately shows:
#   DATABASE_URL = (unset)  ← Bug!
#   DB_HOST      = "localhost"  ← Wrong!

# Bug found in 60 seconds
```

### Design Decisions (from review)

1. **TUI first, not DAP**
   - TUI works anywhere (SSH, containers)
   - Showcases brush capabilities
   - DAP can come later (reuse same backend)

2. **Defer Replay Mode**
   - High effort, uncertain value
   - Users can request it later
   - Focus on live x-ray (unique value)

3. **Ship Phase 0 immediately**
   - Don't wait for brush-core hooks
   - Polling + tracing is "good enough"
   - Get user feedback fast
   - Clean upgrade path when hooks ready

4. **Lead with CI/CD demo**
   - Universal pain point
   - Obvious value (45 min → 60 sec)
   - Easy to demonstrate

## File Structure

```
tuish/
├── src/
│   ├── main.rs                    # Entry point (add script mode)
│   ├── app_ui.rs                  # Main UI loop (existing)
│   ├── source_pane.rs             # NEW: Source code display
│   ├── script_watch_mode.rs       # NEW: Phase 0 implementation
│   └── ...
├── docs/
│   ├── README_SCRIPT_MODE.md      # This file
│   ├── SCRIPT_EXECUTION_MODE.md   # Full design
│   ├── DESIGN_DISCUSSION.md       # Design review responses
│   ├── PHASE_0_PROTOTYPE.md       # MVP implementation guide
│   └── SCRIPT_MODE_QUICK_START.md # User documentation
└── Cargo.toml
```

## Next Steps

### Immediate (This Weekend)
1. [ ] Implement Phase 0 prototype (see PHASE_0_PROTOTYPE.md)
2. [ ] Create demo video with CI/CD scenario
3. [ ] Share with community (Discord, GitHub)
4. [ ] Gather feedback

### Short Term (2 weeks)
1. [ ] Collaborate on brush-core hooks design
2. [ ] Upgrade to Phase 1 (Full X-Ray)
3. [ ] Polish UX based on feedback

### Long Term (Q1 2025)
1. [ ] Syntax highlighting
2. [ ] Documentation and tutorials
3. [ ] Consider DAP (if demand exists)
4. [ ] Consider Replay (if users request)

## Questions?

- **Discord:** [brush community](https://discord.gg/kPRgC9j3Tj)
- **GitHub Issues:** [brush/issues](https://github.com/reubeno/brush/issues)
- **Discussions:** [brush/discussions](https://github.com/reubeno/brush/discussions)

## Key Insights

1. **Live x-ray is the killer feature** (unique, valuable)
2. **Don't wait for perfect** (ship Lite mode, iterate)
3. **CI/CD debugging is the demo** (everyone feels this pain)
4. **Leave space for hooks** (design for clean upgrade)
5. **Replay is speculative** (defer until proven demand)

---

**Status:** Design complete, ready for implementation
**Next:** Build Phase 0 prototype (2-3 days)
**Goal:** Ship something this weekend, gather feedback, iterate

---

**The vision:** Make shell script debugging as powerful as debugging compiled code. 🚀
