# sizzle.tape - 🔥 Hero GIF for README
# Works with https://github.com/charmbracelet/vhs
#
# This is the main demo showcasing brush's visual polish:
# 1. Real-time semantic syntax highlighting
# 2. Fish-style autosuggestions from history
# 3. Bash compatibility (brace expansion)
# 4. Clean, colorful, modern shell experience
#
# Run: vhs docs/demos/sizzle.tape

Output sizzle.gif

Set FontFamily "CaskaydiaMono Nerd Font Mono"
Set FontSize 24
Set Theme "Catppuccin Mocha"
Set Width 1000
Set Height 420
Set CursorBlink false
Set TypingSpeed 40ms

Env HISTFILE ""

# Setup: starship prompt + brush with highlighting
Hide
Type `export STARSHIP_CONFIG=/dev/null`
Enter
Type `eval "$(starship init bash)"`
Enter
Type `brush --enable-highlighting`
Enter
Sleep 0.5s
Show

# Scene 1: Syntax highlighting - colors bloom as you type
# Keywords (yellow), variables (magenta), builtins (green), strings (yellow)
Type `for f in *.md; do echo `
Sleep 0.15s
Type `"📄 $f"`
Sleep 0.15s
Type `; done`
Sleep 0.6s
Enter
Sleep 0.8s

# Scene 2: Autosuggestion - ghost appears, accept with →
Type `for`
Sleep 0.7s
# Ghost of previous "for f in..." appears - accept it
Right
Right
Right
Right
Right
Right
Right
Right
Right
Right
Right
Right
Right
Right
Right
Right
Right
Right
Right
Right
Right
Right
Right
Right
Right
Right
Right
Right
Right
Sleep 0.3s
Ctrl+C

# Scene 3: Bash power - brace expansion
Type `echo {api,web,db}-{prod,staging}`
Sleep 0.5s
Enter
Sleep 0.8s

# Scene 4: Pipeline with live highlighting
Type `ls -la | grep md | head -3`
Sleep 0.4s
Enter
Sleep 1.2s
