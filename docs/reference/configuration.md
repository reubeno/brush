# Configuration File

brush supports an optional TOML configuration file that allows you to customize shell behavior without command-line arguments.

## File Location

brush looks for the configuration file at:

- **Linux/macOS**: `~/.config/brush/config.toml`
- **Windows**: `%APPDATA%\brush\config.toml`

You can override this location with the `--config` flag:

```bash
brush --config /path/to/custom/config.toml
```

To disable configuration file loading entirely, use:

```bash
brush --no-config
```

## Configuration Priority

Settings are applied in the following order (later values override earlier ones):

1. **Defaults** - Built-in default values
2. **Configuration file** - Values from `config.toml`
3. **Command-line arguments** - Flags passed to brush

## File Format

The configuration file uses [TOML](https://toml.io/) format. All settings are optional; brush uses sensible defaults for any unspecified values.

### Example Configuration

```toml
[ui]
syntax-highlighting = true

[experimental]
zsh-hooks = true
terminal-shell-integration = true
```

## Available Settings

### `[ui]` Section

User interface settings.

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| `syntax-highlighting` | boolean | `false`* | Enable syntax highlighting in the input line |

\* Default is `true` when built with the `experimental` feature.

### `[experimental]` Section

Experimental features that may change or be removed in future versions.

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| `zsh-hooks` | boolean | `false` | Enable zsh-style preexec/precmd hooks |
| `terminal-shell-integration` | boolean | `false` | Enable terminal shell integration |

## JSON Schema

A JSON Schema for the configuration file is available at [`schemas/brush-config.schema.json`](../../schemas/brush-config.schema.json). This can be used with editors that support schema-based validation and autocompletion for TOML files.

## Forward Compatibility

brush ignores unknown settings in the configuration file. This allows configuration files to be shared across different versions of brush without causing errors.

## Error Handling

If the configuration file cannot be read or parsed, brush logs an error message and continues with default settings. The shell will still start normally.
