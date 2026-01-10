# interactive.tape - Showcase interactive shell features
# Works with https://github.com/charmbracelet/vhs

Output interactive.gif

Set FontFamily "CaskaydiaMono Nerd Font Mono"
Set FontSize 22
Set Theme "Monokai Pro"
Set Width 1400
Set Height 500
Set CursorBlink false

Env HISTFILE ""
Env PS1 'brush$ '

Hide
Type `brush --enable-highlighting --norc --noprofile`
Enter
Sleep 0.3s
Show

Type `# Syntax highlighting as you type`
Enter
Sleep 0.5s
Type `for i in {1..5}; do echo "Number: $i"; done`
Sleep 1.5s
Enter
Sleep 1s

Type `# History-based autosuggestions`
Enter
Sleep 0.5s
Type `for`
Sleep 1s
Right
Right
Right
Right
Sleep 0.5s
Ctrl+C

Type `# Works with starship prompt`
Enter
Sleep 0.5s
Type `eval "$(starship init bash)"`
Enter
Sleep 1s

Type `# And fzf for fuzzy finding`
Enter
Sleep 0.5s
Type `eval "$(fzf --bash)"`
Enter
Sleep 0.5s
Type `# Ctrl+R now uses fzf for history search`
Enter
Sleep 2s
