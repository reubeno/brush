# Future Work: Terminal Scrollback Capture

**Problem:** Commands with output exceeding visible terminal height lose early lines (e.g., `for i in {1..40}; do echo $i; done` only shows last ~13 lines in captured block).

**Root Cause:** `capture_styled_output()` only reads visible screen rows. vt100 has scrollback buffer (10K lines) but accessing it requires `screen_mut().set_scrollback()` which needs vt100 0.16+. However, tui-term (used for alternate screen rendering) depends on vt100 0.15.x.

**Solutions (pick one):**

1. **Fork tui-term** - Update its vt100 dep to 0.16, use our fork
2. **Incremental capture** - Use vt100's `Callbacks` trait to capture lines as they scroll off, accumulate in `CommandOutputBlock`
3. **Large virtual terminal** - Size PTY to 1000+ rows during command execution, resize after

**Recommendation:** Option 2 (incremental capture) is cleanest - no fork maintenance, captures output progressively.
