# zwobot

A Zulip bot serving local LLMs via llama.cpp. Each topic in the configured Zulip channel can have its own dedicated model, allowing multiple models to run simultaneously.

## Features

- Multiple models running concurrently (one per topic)
- Automatic conversation history formatting
- Configurable per-topic model settings
- Runs entirely local using llama.cpp
- GPU acceleration support

## Requirements

- Rust 1.75+
- 96GB+ RAM (for running multiple models)
- GPU recommended (Metal on macOS, CUDA on Linux/Windows)
- Zulip instance with bot credentials

## Quick Start

1. **Ensure you have a `zuliprc` file in the project root:**
   ```
   [api]
   email=your-bot@zulip.example.com
   key=your_api_key_here
   site=https://zulip.example.com
   ```

2. **Configure topics and models:**
   Edit `config/default.toml` to map topics to models:
   ```toml
   channel = "clankerville"

   [topics.general]
   model_id = "Qwen/Qwen3-0.6B"
   quantization = "Q8_0"
   max_tokens = 1024
   temperature = 0.7

   [topics."tech-talk"]
   model_id = "Qwen/Qwen3-1.7B"
   quantization = "Q4_K_M"
   max_tokens = 2048
   temperature = 0.8

   [default_topic]
   model_id = "Qwen/Qwen3-0.6B"
   quantization = "Q8_0"
   max_tokens = 1024
   temperature = 0.7
   ```

3. **Build and run:**
   ```bash
   cargo build --release
   cargo run --release
   ```

## How It Works

1. The bot monitors the configured channel (e.g., "clankerville")
2. When a message arrives in a topic, it:
   - Loads the configured model for that topic (if not already loaded)
   - Fetches the entire conversation history for that topic
   - For each configured character in the topic:
     - Formats the history as a prompt with the character's personality
     - Generates a response using the topic's model
     - Sends the response back to the topic
3. Models are cached in memory, so subsequent messages in the same topic reuse the loaded model

## Roleplaying with Multiple Characters

Each topic can have one or more characters with unique personalities. When multiple characters are configured, the bot will respond as each character in sequence, creating a dynamic roleplaying experience.

**Example configuration:**
```toml
[topics.tavern]
model_id = "Qwen/Qwen3-1.7B"
quantization = "Q4_K_M"
max_tokens = 1024
temperature = 0.9

[[topics.tavern.characters]]
name = "Grimnar the Dwarf"
system_prompt = "You are Grimnar, a gruff but good-hearted dwarf warrior. You speak in a rough manner, love talking about ale and smithing, and are fiercely loyal to your companions."

[[topics.tavern.characters]]
name = "Elara the Elf"
system_prompt = "You are Elara, a wise and graceful elven mage. You speak eloquently and often reference ancient lore and nature."
```

When a user sends a message to the "tavern" topic, both Grimnar and Elara will respond, each with their own personality!

**Single character topics:**
If no characters are configured, the bot defaults to a helpful "Assistant" persona.

## Configuration

### Channel Settings

- `channel`: The Zulip channel/stream to monitor (default: "clankerville")

### Topic Model Configuration

Each topic can have its own configuration:

- `model_id`: HuggingFace model repo (e.g., "Qwen/Qwen3-0.6B") or local GGUF file path
- `quantization`: Quantization level (Q4_K_M, Q8_0, Q6_K, Q5_K_M, Q3_K_M, Q2_K)
- `max_tokens`: Maximum tokens to generate (default: 1024)
- `temperature`: Sampling temperature (default: 0.7)
- `characters`: (Optional) Array of character configurations for roleplaying

### Character Configuration

Each character has:

- `name`: The character's name (displayed when they respond)
- `system_prompt`: The character's personality and background (guides their responses)

Use `[[topics.TOPIC_NAME.characters]]` to define multiple characters for a topic.

### Default Topic

The `[default_topic]` section defines the model to use for topics not explicitly configured.

## Memory Management

With 96GB of RAM, you can run several small to medium models simultaneously:

- Multiple Q8_0 0.6B models: ~600MB each
- Multiple Q4_K_M 1.7B models: ~1GB each
- One Q4_K_M 7B model: ~4GB
- Mix and match based on your needs

Models are loaded on-demand when a message arrives in a topic and stay loaded for subsequent messages.

## Model Selection

The bot automatically downloads models from HuggingFace. For a model like `Qwen/Qwen3-0.6B`:

1. It first tries the GGUF variant repo: `Qwen/Qwen3-0.6B-GGUF`
2. Looks for files matching the quantization (e.g., `Qwen3-0.6B-Q8_0.gguf`)
3. Falls back to the original repo if needed

**Recommended models:**
- `Qwen/Qwen3-0.6B` (Q8_0) - Small, fast, good for general topics (~600MB)
- `Qwen/Qwen3-1.7B` (Q4_K_M) - Better quality, still fast (~1GB)
- `mistralai/Mistral-7B-v0.1` (Q4_K_M) - High quality (~4GB)

## Logging

Set the `RUST_LOG` environment variable to control logging:

```bash
RUST_LOG=zwobot=debug cargo run --release  # Debug logging
RUST_LOG=zwobot=info cargo run --release   # Info logging (default)
```

## Architecture

### Project Structure

```
zwobot/
├── src/
│   ├── main.rs      # Entry point, event loop
│   ├── config.rs    # Configuration loading
│   ├── llm.rs       # LLM engine (llama.cpp wrapper)
│   ├── error.rs     # Error types
│   └── zulip.rs     # Zulip API client
├── config/
│   └── default.toml # Configuration file
├── Cargo.toml
└── README.md
```

### How Models are Managed

- Models are stored in a cache: `HashMap<Topic, Arc<LlmEngine>>`
- When a message arrives, the bot checks if the model for that topic is loaded
- If not, it loads the model according to the topic's configuration
- Models stay in memory for fast subsequent responses

## Differences from qwobot

- Uses Zulip instead of Discord
- Multiple models instead of single global model
- No SPW/spweeboard support
- Topic-based model assignment instead of global configuration
- Event-based message handling instead of Discord command framework
- Character/roleplaying support - each topic can have multiple characters with unique personalities

## License

MIT
