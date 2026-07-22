# Secret configuration separation

Status: implemented.

Sensitive provider credentials and endpoints are stored separately from shareable match configuration. Named profiles live in `$XDG_CONFIG_HOME/ai_arena/secrets.toml` or `~/.config/ai_arena/secrets.toml`; CLI and CSV agent settings hold only an optional profile name.

Environment variables remain supported for backward compatibility. The secrets file is plaintext and should use mode `0600`. See [`examples/secrets.toml.example`](../examples/secrets.toml.example) and the main [README](../README.md) for the current format and resolution order.
