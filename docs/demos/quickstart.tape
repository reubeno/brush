# quickstart.tape - Quick intro for README
# Works with https://github.com/charmbracelet/vhs

Output quickstart.gif

Set FontFamily "CaskaydiaMono Nerd Font Mono"
Set FontSize 24
Set Theme "Monokai Pro"
Set Width 1200
Set Height 400
Set CursorBlink false

Env HISTFILE ""
Env PS1 '$ '

Type `# Install brush`
Enter
Sleep 0.5s
Type `cargo binstall brush-shell`
Enter
Sleep 1.5s

Type `# Launch the shell`
Enter
Sleep 0.3s
Type `brush`
Enter
Sleep 0.8s

Env PS1 'brush$ '
Type `echo "Hello from brush!"`
Enter
Sleep 0.8s

Type `# Your .bashrc just works`
Enter
Sleep 0.5s
Type `type ls  # aliases are loaded`
Enter
Sleep 1s

Type `# 1400+ compatibility tests against bash`
Enter
Sleep 2s
