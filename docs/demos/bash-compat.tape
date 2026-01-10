# bash-compat.tape - Demonstrate bash compatibility features
# Works with https://github.com/charmbracelet/vhs

Output bash-compat.gif

Set FontFamily "CaskaydiaMono Nerd Font Mono"
Set FontSize 20
Set Theme "Monokai Pro"
Set Width 1500
Set Height 550
Set CursorBlink false

Env HISTFILE ""
Env PS1 'brush$ '

Hide
Type `brush --norc --noprofile`
Enter
Sleep 0.3s
Show

Type `# Associative arrays`
Enter
Sleep 0.5s
Type `declare -A colors=([red]="#ff0000" [green]="#00ff00" [blue]="#0000ff")`
Enter
Sleep 0.3s
Type `echo "Green is ${colors[green]}, keys: ${!colors[@]}"`
Enter
Sleep 1s

Type `# Brace expansion + sequences`
Enter
Sleep 0.5s
Type `echo {a..e} {1..10..2} file{A,B,C}.txt`
Enter
Sleep 1s

Type `# Process substitution`
Enter
Sleep 0.5s
Type `diff <(echo "hello") <(echo "hello") && echo "Files match!"`
Enter
Sleep 1s

Type `# Extended globbing + case`
Enter
Sleep 0.5s
Type `shopt -s extglob`
Enter
Type `case "script.sh" in *.@(sh|bash)) echo "Shell script";; esac`
Enter
Sleep 1s

Type `# Arithmetic with bases`
Enter
Sleep 0.5s
Type `echo "Hex FF=$((16#FF)) Binary 1010=$((2#1010)) Octal 77=$((8#77))"`
Enter
Sleep 1s

Type `# Traps work too`
Enter
Sleep 0.5s
Type `trap 'echo "Exiting..."' EXIT`
Enter
Type `exit`
Enter
Sleep 2s
