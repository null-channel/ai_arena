# Ollama agent support

Status: implemented.

Ollama is available through the `llm-connector` integration. The selected CLI or CSV model, temperature, and seed are sent with each request. Endpoints can be local or remote and resolve from a named secrets profile, `OLLAMA_BASE_URL`, a `default` profile, or `http://localhost:11434`, in that order.

Provider calls share the arena's timeout, retry, concurrency, and optional token-budget policy. See the main [README](../README.md) for current commands, configuration, capability limits, and security guidance.
