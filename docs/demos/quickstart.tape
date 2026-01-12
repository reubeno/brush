# quickstart.tape - Hero GIF for README
# Works with https://github.com/charmbracelet/vhs
# Run with: docker run --rm -v $PWD:/vhs -v $PWD/target/debug/brush:/usr/bin/brush vhs-fedora docs/demos/quickstart.tape

Output docs/demos/quickstart.gif

Set FontSize 22
Set Theme "Catppuccin Mocha"
Set Width 900
Set Height 340
Set CursorBlink false
Set TypingSpeed 50ms

# Set prompt BEFORE launching brush (so it inherits it cleanly)
Env HISTFILE ""
Env PS1 "brush$ "

# Launch brush with highlighting enabled
Hide
Type "brush --enable-highlighting --norc --noprofile"
Enter
Sleep 0.5s
Show

# Syntax highlighting on a for loop
Type "for f in *.md; do echo Found: $f; done"
Sleep 0.8s
Enter
Sleep 1.2s

# Show brace expansion (bash power feature)
Type "echo {a,b,c}-{1..3}"
Sleep 0.8s
Enter
Sleep 1s

# Command with pipes and highlighting  
Type "ls -la | head -5"
Sleep 0.6s
Enter
Sleep 1.2s

# Final pause
Sleep 1s
