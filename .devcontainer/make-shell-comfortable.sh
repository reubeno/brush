#!/bin/bash
set -euo pipefail

# Settings
zsh_theme="agnoster"

# Install oh-my-zsh
sh -c "$(curl -fsSL https://raw.githubusercontent.com/ohmyzsh/ohmyzsh/master/tools/install.sh)"

# Clone preferred plugins
git clone https://github.com/zsh-users/zsh-autosuggestions.git ~/.oh-my-zsh/custom/plugins/zsh-autosuggestions
git clone https://github.com/zsh-users/zsh-syntax-highlighting.git ~/.oh-my-zsh/custom/plugins/zsh-syntax-highlighting

# Enable plugins
sed -i "s/^plugins=.*/plugins=(git zsh-autosuggestions zsh-syntax-highlighting)/" ~/.zshrc

# Select theme
sed -i "s/^ZSH_THEME=.*/ZSH_THEME=\"${zsh_theme}\"/" ~/.zshrc
