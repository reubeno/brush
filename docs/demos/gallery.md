# Demo Gallery

Explore brush's features through these short demos. Each showcases a different aspect of the shell.

## ⌨️ Interactive Features

### Syntax Highlighting & Autosuggestions

![Interactive demo](interactive.gif)

Real-time syntax highlighting as you type, with fish-style autosuggestions from your command history. Accept suggestions with the right arrow key.

**Generate:** `vhs interactive.tape`

### Tab Completion

![Completion demo](completion.gif)

Programmable completion compatible with [bash-completion](https://github.com/scop/bash-completion). Git, docker, and other completions work out of the box.

**Generate:** `vhs completion.tape`

### FZF Integration

![FZF demo](fzf.gif)

Works with [fzf](https://github.com/junegunn/fzf) for fuzzy history search (Ctrl+R) and file finding.

**Generate:** `vhs fzf.tape`

---

## 🐚 Bash Compatibility

### Advanced Bash Features

![Bash compatibility demo](bash-compat.gif)

Associative arrays, brace expansion, process substitution, extended globbing, arithmetic with different bases, and traps.

**Generate:** `vhs bash-compat.tape`

---

## 🔧 For Developers

### Embeddable API

![Embedding demo](embedding.gif)

Use brush as a library in your Rust applications with `Shell::builder()`.

**Generate:** `vhs embedding.tape`

---

## 🎬 Full Tour

### Complete Demo

![Full demo](demo.gif)

A comprehensive walkthrough of brush's features including starship integration, git completions, and more.

**Generate:** `vhs demo.tape`

---

## Running the Demos

All demos use [VHS](https://github.com/charmbracelet/vhs) to generate GIFs from `.tape` files.

### Using Docker (Recommended)

```bash
docker build -t brush-vhs -f docs/demos/Dockerfile.vhs .
docker run --rm -v $(pwd)/docs/demos:/output brush-vhs sizzle.tape
```

### Local Installation

```bash
# Install VHS (requires Go)
go install github.com/charmbracelet/vhs@latest

# Generate a demo
cd docs/demos
vhs sizzle.tape
```

### Dependencies

The demos assume these tools are available:
- `brush` (built and in PATH)
- `starship` (for prompt demos)
- `fzf` (for fuzzy finding demos)
- `git` (for completion demos)
- Nerd Font (for icons)

The [Dockerfile.vhs](Dockerfile.vhs) includes all dependencies.
