# Works with https://github.com/charmbracelet/vhs
# This tape assumes that fzf and brush are already pre-installed and ready to go.

Output sample.gif

# Set FontFamily "CaskaydiaMono Nerd Font Mono"
Set FontSize 20
Set Theme "Monokai Pro"
Set Width 1600
Set Height 600
Set CursorBlink false

# Setup environment to launch brush in
Env HISTFILE ""
Env PS1 '$0$ '

# Launch brush and set up bash-completion
Hide
Type `brush --enable-highlighting --norc --noprofile`
Enter
Show

# Enable fzf
Type `# Let's enable fzf with standard bash mode`
Sleep 0.8s
Enter
Type `eval "$(fzf --bash)"`
Sleep 0.8s
Enter
Sleep 1s

Enter
Enter

Type `# ^^^ we see an error above. Some key combos are missing but more are functional.`
Enter

Enter
Enter

# Type some things.
Type `echo 'We will type something'`
Enter
Type `echo '...just to populate history'`
Enter
Type `echo 'And now, hit Ctrl+R.'`
Enter
Sleep 1s

# Ctrl+R
Ctrl+R
Sleep 1.5s
Up
Sleep 0.8s
Enter

Sleep 2s
Enter
Sleep 2s

