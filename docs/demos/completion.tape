# completion.tape - Showcase programmable completion
# Works with https://github.com/charmbracelet/vhs
# Requires: bash-completion installed on system

Output completion.gif

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
Type `source /usr/share/bash-completion/bash_completion && clear`
Enter
Sleep 0.3s
Show

Type `# Git completions work out of the box`
Enter
Sleep 0.5s
Type `git ch`
Tab
Sleep 0.8s
Tab
Sleep 1s
Escape
Sleep 0.3s
Ctrl+C

Type `git log --one`
Tab
Sleep 0.8s
Type ` -5`
Enter
Sleep 1.5s

Type `# Custom completion functions`
Enter
Sleep 0.5s
Type `complete -W "start stop restart status" myservice`
Enter
Type `myservice st`
Tab
Tab
Sleep 1s
Escape
Ctrl+C
Sleep 0.5s

Type `# File path completion`
Enter
Sleep 0.5s
Type `ls /usr/sha`
Tab
Sleep 0.5s
Type `/doc`
Tab
Tab
Sleep 1.5s
Escape
Ctrl+C
Sleep 1s
