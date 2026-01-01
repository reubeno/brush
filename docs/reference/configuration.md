# Configuration File

brush supports an optional TOML configuration file that allows you to customize shell behavior without command-line arguments.

## File Location

`brush` looks for the configuration file at:

- **Linux/macOS**: `${XDG_CONFIG_HOME}/brush/config.toml`*
- **Windows**: `%APPDATA%\brush\config.toml`

> [!NOTE]
> On Linux/macOS falls back to `~/.config/brush/config.toml` if `XDG_CONFIG_HOME` is undefined.

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

| Setting               | Type    | Default | Description                                  |
|-----------------------|---------|---------|----------------------------------------------|
| `syntax-highlighting` | boolean | `false` | Enable syntax highlighting in the input line |

### `[experimental]` Section

Experimental features that may change or be removed in future versions.

| Setting                      | Type    | Default | Description                           |
|------------------------------|---------|---------|---------------------------------------|
| `zsh-hooks`                  | boolean | `false` | Enable zsh-style preexec/precmd hooks |
| `terminal-shell-integration` | boolean | `false` | Enable terminal shell integration     |

## JSON Schema

A JSON Schema for the configuration file is available at [`schemas/config.schema.json`](../../schemas/config.schema.json). This can be used with editors that support schema-based validation and autocompletion for TOML files.

### Using the Schema with VS Code

To enable schema validation in VS Code with the [Even Better TOML](https://marketplace.visualstudio.com/items?itemName=tamasfe.even-better-toml) extension, add this to your `config.toml`:

```toml
#:schema https://raw.githubusercontent.com/reubeno/brush/main/schemas/config.schema.json

[ui]
syntax-highlighting = true
```

The `#:schema` directive tells the editor where to find the schema for validation and autocompletion.

### Using the Schema with Other Editors

Many editors support JSON Schema for TOML files. Consult your editor's documentation for how to associate a schema with a file. You can reference the schema via:

- **URL**: `https://raw.githubusercontent.com/reubeno/brush/main/schemas/config.schema.json`
- **Local path**: Point to `schemas/config.schema.json` in your brush source checkout

## Sample Configuration

A sample configuration file is available at [`samples/config.toml`](../../samples/config.toml) in the brush repository. You can copy this file to get started:

```bash
# Linux/macOS
mkdir -p ~/.config/brush
cp samples/config.toml ~/.config/brush/config.toml
```

## Forward Compatibility

brush ignores unknown settings in the configuration file. This allows configuration files to be shared across different versions of brush without causing errors.

## Error Handling

If the configuration file cannot be read or parsed, brush logs an error message and continues with default settings. The shell will still start normally.
