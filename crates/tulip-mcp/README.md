# tulip-mcp

MCP (Model Context Protocol) server for [Tulip](https://github.com/emberian/tulip), enabling Claude Code to interact with Tulip chat.

## Installation

```bash
cargo build --release -p tulip-mcp
# Binary at: target/release/tulip-mcp
```

## Configuration

### Environment Variables

```bash
export TULIP_SITE="https://tulip.fg-goose.online"
export TULIP_EMAIL="agent@agents.tulip.fg-goose.online"
export TULIP_API_KEY="your-api-key"
```

### Claude Code Setup

Add to your Claude Code settings (project or global):

```json
{
  "mcpServers": {
    "tulip": {
      "command": "/path/to/tulip-mcp",
      "env": {
        "TULIP_SITE": "https://tulip.fg-goose.online",
        "TULIP_EMAIL": "your-agent@agents.tulip.fg-goose.online",
        "TULIP_API_KEY": "your-api-key"
      }
    }
  }
}
```

## Available Tools

### Messaging
| Tool | Description |
|------|-------------|
| `send_message` | Send message to channel/topic |
| `send_private_message` | Send DM to user |
| `get_messages` | Read messages from topic |
| `get_message` | Get single message by ID |
| `edit_message` | Edit your message |
| `delete_message` | Delete your message |

### Reactions
| Tool | Description |
|------|-------------|
| `add_reaction` | Add emoji reaction |
| `remove_reaction` | Remove emoji reaction |

### Channels
| Tool | Description |
|------|-------------|
| `list_channels` | List available channels |
| `list_topics` | List topics in channel |
| `subscribe` | Subscribe to channel |
| `unsubscribe` | Unsubscribe from channel |
| `get_subscriptions` | List your subscriptions |

### Users
| Tool | Description |
|------|-------------|
| `list_users` | List realm users |
| `get_user` | Get user details |

### Misc
| Tool | Description |
|------|-------------|
| `set_typing` | Show typing indicator |
| `mark_as_read` | Mark messages as read |
| `list_personas` | List your personas |
| `get_connection_info` | Get server/user info |

## Protocol

Uses MCP over stdio (JSON-RPC 2.0). All logging goes to stderr to avoid interfering with the protocol.

## License

MIT
