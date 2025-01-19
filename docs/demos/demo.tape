# Works with https://github.com/charmbracelet/vhs

Output sample.gif

Set FontFamily "CaskaydiaMono Nerd Font Mono"
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
Type `source /usr/share/bash-completion/bash_completion && clear`
Enter
Show

# Enable starship
Type `# Let's start with a better prompt. starship to the rescue!`
Sleep 0.8s
Enter
Type `eval "$(starship init bash)"`
Sleep 0.8s
Enter
Sleep 1s

# git describe
Type `git d`
Tab
Sleep 1.3s
Enter
Sleep 0.4s
Type `--l`
Sleep 0.4s
Tab
Sleep 0.8s
Type `brush-she`
Sleep 0.5s
Tab
Sleep 0.2s
Right
Sleep 0.2s
Right
Sleep 0.8s
Enter
Sleep 0.8s
Enter
Sleep 1s

# vim
Type `vim Ca`
Sleep 0.5s
Tab
Sleep 0.8s
Right
Sleep 0.5s
Enter
Sleep 1s
Enter

Type `1G`
Type `i`
Enter
Up
Type `# Let's try suspending vim...`
Enter
Escape
Sleep 1s
Ctrl+Z
Sleep 0.7s

Type `# Yep, it's suspended.`
Sleep 0.4s
Type ` Let's bring it back.`
Enter
Sleep 0.8s

Type `fg`
Sleep 0.4s
Enter
Sleep 0.6s

Type `:q!`
Sleep 0.3s
Enter
Sleep 0.5s
Ctrl+L
Sleep 0.2s

Type `# Let's properly greet the world.`
Enter
Sleep 0.4s

# Figure out version
Type `verline=$(help | head -n1)`
Sleep 0.2s
Enter
Sleep 0.3s
Type `[[ "${verline}" =~ ^.*version\ ([[:digit:]\.]+).*$ ]] && ver=${BASH_REMATCH[1]}`
Sleep 0.4s
Enter
Sleep 0.2s
Type `declare -p ver`
Enter
Sleep 0.4s

# Declare function
Type `function greet() {`
Enter
Type `  echo "Hello from brush ${ver}!"`
Enter
Type `}`
Sleep 1s
Enter
Type `type greet`
Sleep 0.4s
Enter
Sleep 1s

# Use function
Ctrl+L
Type `for ((i = 0; i < 5; i++)); do greet; done`
Sleep 1s
Enter
Sleep 0.8s

Type `# Surely we can make that more colorful.`
Enter
Sleep 1s

# Now with lolcat
Type `for `
Sleep 1s
Right
Sleep 0.8s
Type ` | lolcat -F 0.3 -S 12`
Sleep 0.6s
Enter

Sleep 3s
