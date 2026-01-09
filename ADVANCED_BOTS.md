# Advanced Bot Development Guide

This guide covers Tulip's bot extensibility system, which enables bots to create rich interactive widgets, register slash commands with autocomplete, and receive user interactions via webhooks.

## Table of Contents

1. [Overview](#overview)
2. [Widget Types](#widget-types)
   - [Rich Embed](#rich-embed)
   - [Interactive](#interactive)
   - [Freeform](#freeform-trusted-bots-only)
3. [Bot Commands](#bot-commands)
   - [Registering Commands](#registering-commands)
   - [Command Options](#command-options)
   - [Dynamic Autocomplete](#dynamic-autocomplete)
4. [Receiving Interactions](#receiving-interactions)
   - [Webhook Payload Format](#webhook-payload-format)
   - [Responding to Interactions](#responding-to-interactions)
5. [API Reference](#api-reference)
6. [Security Model](#security-model)
7. [Complete Examples](#complete-examples)

---

## Overview

The bot extensibility system allows outgoing webhook bots and embedded bots to:

- **Send rich widgets** in messages (embeds, buttons, select menus, or custom HTML/JS)
- **Register slash commands** that appear in the compose box typeahead with autocomplete
- **Receive interaction events** when users click buttons, select options, or submit forms
- **Respond dynamically** to interactions with ephemeral or public messages

### Architecture

```
User clicks button
        │
        ▼
POST /json/bot_interactions
        │
        ▼
Queue: bot_interactions worker
        │
        ├─► Outgoing Webhook Bot: HTTP POST to bot URL
        │
        └─► Embedded Bot: call handle_interaction()
        │
        ▼
Bot processes and optionally responds
```

---

## Widget Types

Widgets are created by including `widget_content` when sending a message. The widget replaces the message content with interactive UI.

### Rich Embed

Discord-style embeds for displaying formatted information.

```json
{
  "widget_type": "rich_embed",
  "extra_data": {
    "title": "Weather Report",
    "description": "Current conditions for San Francisco",
    "url": "https://weather.example.com/sf",
    "color": 3447003,
    "author": {
      "name": "Weather Bot",
      "icon_url": "https://example.com/weather-icon.png"
    },
    "thumbnail": {
      "url": "https://example.com/sunny.png"
    },
    "fields": [
      {"name": "Temperature", "value": "68°F", "inline": true},
      {"name": "Humidity", "value": "45%", "inline": true},
      {"name": "Wind", "value": "5 mph NW", "inline": true}
    ],
    "footer": {
      "text": "Last updated",
      "icon_url": "https://example.com/clock.png"
    },
    "timestamp": "2024-01-15T10:30:00Z"
  }
}
```

#### Rich Embed Schema

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `title` | string | No* | Embed title (can be a link if `url` is set) |
| `description` | string | No* | Main content text (supports whitespace) |
| `url` | string | No | URL for the title link |
| `color` | integer | No | Left border color as RGB integer (e.g., `3447003` for blue) |
| `author` | object | No | Author info: `{name, url?, icon_url?}` |
| `thumbnail` | object | No | Thumbnail image: `{url}` |
| `image` | object | No | Large image: `{url}` |
| `fields` | array | No | Array of `{name, value, inline?}` objects |
| `footer` | object | No | Footer: `{text, icon_url?}` |
| `timestamp` | string | No | ISO 8601 timestamp |

*At least one of `title` or `description` is required.

---

### Interactive

Buttons and select menus that trigger interaction events.

```json
{
  "widget_type": "interactive",
  "extra_data": {
    "content": "Choose an action:",
    "components": [
      {
        "type": "action_row",
        "components": [
          {
            "type": "button",
            "label": "Approve",
            "style": "success",
            "custom_id": "approve_request_123"
          },
          {
            "type": "button",
            "label": "Reject",
            "style": "danger",
            "custom_id": "reject_request_123"
          },
          {
            "type": "button",
            "label": "View Details",
            "style": "link",
            "url": "https://example.com/request/123"
          }
        ]
      },
      {
        "type": "action_row",
        "components": [
          {
            "type": "select_menu",
            "custom_id": "assign_to",
            "placeholder": "Assign to team member",
            "options": [
              {"label": "Alice", "value": "user_1", "description": "Engineering"},
              {"label": "Bob", "value": "user_2", "description": "Design"},
              {"label": "Carol", "value": "user_3", "description": "Product"}
            ]
          }
        ]
      }
    ]
  }
}
```

#### Component Types

**Button**

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `type` | `"button"` | Yes | Component type |
| `label` | string | Yes | Button text |
| `style` | string | No | `primary`, `secondary`, `success`, `danger`, or `link` (default: `secondary`) |
| `custom_id` | string | No* | Identifier sent with interaction event |
| `url` | string | No* | URL for link-style buttons |
| `disabled` | boolean | No | Disable the button |
| `modal` | object | No | Modal to show when clicked (see [Modals](#modals)) |

*Either `custom_id` or `url` is required (but not both).

**Select Menu**

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `type` | `"select_menu"` | Yes | Component type |
| `custom_id` | string | Yes | Identifier sent with interaction event |
| `options` | array | Yes | Array of `{label, value, description?, default?}` |
| `placeholder` | string | No | Placeholder text |
| `min_values` | integer | No | Minimum selections (default: 1) |
| `max_values` | integer | No | Maximum selections (default: 1) |
| `disabled` | boolean | No | Disable the menu |

**Button Styles**

| Style | Appearance |
|-------|------------|
| `primary` | Blue, main action |
| `secondary` | Gray, secondary action |
| `success` | Green, positive action |
| `danger` | Red, destructive action |
| `link` | Blue text, opens URL |

#### Modals

Buttons can open modal dialogs for collecting user input:

```json
{
  "type": "button",
  "label": "Submit Feedback",
  "style": "primary",
  "custom_id": "open_feedback",
  "modal": {
    "custom_id": "feedback_form",
    "title": "Submit Feedback",
    "components": [
      {
        "type": "action_row",
        "components": [
          {
            "type": "text_input",
            "custom_id": "feedback_text",
            "label": "Your Feedback",
            "style": "paragraph",
            "placeholder": "Tell us what you think...",
            "min_length": 10,
            "max_length": 1000,
            "required": true
          }
        ]
      },
      {
        "type": "action_row",
        "components": [
          {
            "type": "text_input",
            "custom_id": "email",
            "label": "Email (optional)",
            "style": "short",
            "placeholder": "your@email.com"
          }
        ]
      }
    ]
  }
}
```

**Text Input**

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `type` | `"text_input"` | Yes | Component type |
| `custom_id` | string | Yes | Field identifier |
| `label` | string | Yes | Field label |
| `style` | string | No | `short` (single line) or `paragraph` (multi-line) |
| `placeholder` | string | No | Placeholder text |
| `value` | string | No | Pre-filled value |
| `min_length` | integer | No | Minimum character count |
| `max_length` | integer | No | Maximum character count |
| `required` | boolean | No | Whether field is required |

---

### Freeform (Trusted Bots Only)

Custom HTML, CSS, and JavaScript widgets. **Only available to trusted bots** (see [Security Model](#security-model)).

```json
{
  "widget_type": "freeform",
  "extra_data": {
    "html": "<div class=\"counter\"><span id=\"count\">0</span><button id=\"inc\">+</button></div>",
    "css": ".counter { display: flex; gap: 10px; } button { padding: 8px 16px; }",
    "js": "ctx.on('click', '#inc', () => { const el = container.querySelector('#count'); el.textContent = parseInt(el.textContent) + 1; ctx.post_interaction({ count: parseInt(el.textContent) }); });"
  }
}
```

#### JavaScript Context

The JavaScript receives a `ctx` object and `container` element:

```javascript
// ctx object
{
  message_id: number,           // ID of the message containing the widget
  post_interaction: (data) => void,  // Send interaction to bot
  on: (event, selector, handler) => void,  // jQuery-style event binding
  update_html: (html) => void   // Replace widget HTML content
}

// container is the DOM element containing your widget
```

**Example: Interactive Counter**

```javascript
let count = 0;

ctx.on('click', '#increment', () => {
  count++;
  ctx.update_html(`<div>Count: ${count} <button id="increment">+1</button></div>`);
  ctx.post_interaction({ action: 'increment', count: count });
});
```

#### CSS Scoping

CSS rules are automatically scoped to your widget to prevent style leaks:

```css
/* You write: */
.button { color: red; }

/* Becomes: */
.widget-freeform-12345 .button { color: red; }
```

---

## Bot Commands

Bots can register slash commands that appear in the compose box typeahead.

### Registering Commands

**POST** `/json/bot_commands/register`

```json
{
  "name": "weather",
  "description": "Get weather forecast for a location",
  "options": [
    {
      "name": "location",
      "type": "string",
      "description": "City or ZIP code",
      "required": true
    },
    {
      "name": "units",
      "type": "string",
      "description": "Temperature units",
      "choices": [
        {"name": "Celsius", "value": "c"},
        {"name": "Fahrenheit", "value": "f"}
      ]
    }
  ]
}
```

**Response:**

```json
{
  "result": "success",
  "id": 42,
  "name": "weather",
  "created": true
}
```

After registration, users will see `/weather` in the compose box typeahead.

### Command Options

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `name` | string | Yes | Option name (used for autocomplete) |
| `type` | string | Yes | Data type (e.g., `"string"`, `"number"`) |
| `description` | string | No | Help text shown in UI |
| `required` | boolean | No | Whether option is required |
| `choices` | array | No | Static choices: `[{name, value}]` |

### Dynamic Autocomplete

For dynamic suggestions (e.g., inventory items, user data), implement an autocomplete endpoint.

**Request from Tulip:**

**POST** to your bot's webhook URL:

```json
{
  "type": "autocomplete",
  "token": "your-bot-token",
  "command": "inventory",
  "option": "item",
  "partial": "sw",
  "context": {},
  "user": {
    "id": 123,
    "email": "alice@example.com",
    "full_name": "Alice"
  }
}
```

**Your Response:**

```json
{
  "choices": [
    {"value": "sword_iron", "label": "Iron Sword"},
    {"value": "sword_steel", "label": "Steel Sword"},
    {"value": "shield_wooden", "label": "Wooden Shield"}
  ]
}
```

---

## Receiving Interactions

When users interact with your widgets, Tulip sends an HTTP POST to your bot's webhook URL.

### Webhook Payload Format

```json
{
  "type": "interaction",
  "token": "your-bot-token",
  "bot_email": "weatherbot@example.com",
  "bot_full_name": "Weather Bot",
  "interaction_id": "550e8400-e29b-41d4-a716-446655440000",
  "interaction_type": "button_click",
  "custom_id": "approve_request_123",
  "data": {},
  "message": {
    "id": 12345,
    "sender_id": 100,
    "content": "Request #123",
    "topic": "Approvals",
    "stream_id": 5
  },
  "user": {
    "id": 456,
    "email": "alice@example.com",
    "full_name": "Alice"
  }
}
```

#### Interaction Types

| Type | `custom_id` | `data` |
|------|-------------|--------|
| `button_click` | Button's custom_id | `{}` |
| `select_menu` | Menu's custom_id | `{"values": ["selected_value"]}` |
| `modal_submit` | Modal's custom_id | `{"fields": {"field_id": "value"}}` |
| `freeform` | `"freeform"` | Your custom data |

### Responding to Interactions

Your webhook can return a JSON response to:

#### 1. Send a Public Reply

```json
{
  "content": "Request #123 has been approved!"
}
```

#### 2. Send an Ephemeral Response (Only Visible to Interacting User)

```json
{
  "ephemeral": true,
  "content": "You approved request #123. The requester has been notified."
}
```

#### 3. Send a Private Response (Visible to Specific Users)

```json
{
  "visible_user_ids": [456, 789],
  "content": "This message is only visible to Alice and Bob."
}
```

#### 4. Reply with a New Widget

```json
{
  "content": "Updated status:",
  "widget_content": {
    "widget_type": "rich_embed",
    "extra_data": {
      "title": "Request #123 - Approved",
      "color": 3066993,
      "description": "Approved by Alice"
    }
  }
}
```

#### 5. No Response

Return an empty body or `{}` to acknowledge without any visible response.

---

## API Reference

### Endpoints

| Method | Endpoint | Description |
|--------|----------|-------------|
| `POST` | `/json/bot_interactions` | Handle widget interaction |
| `GET` | `/json/bot_commands` | List all registered commands |
| `POST` | `/json/bot_commands/register` | Register a new command (bots only) |
| `DELETE` | `/json/bot_commands/{command_id}` | Delete a command |
| `GET` | `/json/bot_commands/{bot_id}/autocomplete` | Fetch dynamic autocomplete |

### Interaction Request

**POST** `/json/bot_interactions`

| Parameter | Type | Description |
|-----------|------|-------------|
| `message_id` | integer (JSON) | Message containing the widget |
| `interaction_type` | string | `button_click`, `select_menu`, `modal_submit`, or `freeform` |
| `custom_id` | string | Component identifier |
| `data` | string (JSON) | Additional interaction data |

**Response:**

```json
{
  "result": "success",
  "interaction_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

---

## Security Model

### Trusted Bots

Freeform widgets can execute arbitrary JavaScript in users' browsers. To prevent abuse:

1. **Only trusted bots can send freeform widgets**
2. Realm administrators mark bots as trusted via the `is_trusted_bot` flag
3. Attempting to send a freeform widget from an untrusted bot returns an error

### Message Access Control

- Users can only interact with messages they can see
- Interactions are only routed to the bot that sent the message
- Commands are scoped to the realm (organization)

### Ephemeral/Private Responses

The `visible_to` field on submessages allows bots to send responses visible only to specific users:

- **Ephemeral**: `visible_user_ids: [interacting_user_id]`
- **Private**: `visible_user_ids: [user1_id, user2_id, ...]`
- **Public**: `visible_user_ids: null` (all users can see)

### Bot Types That Support Interactions

| Bot Type | Receives Interactions |
|----------|----------------------|
| Outgoing Webhook Bot | Yes (via HTTP POST) |
| Embedded Bot | Yes (via `handle_interaction`) |
| Default Bot | No |
| Incoming Webhook Bot | No |

---

## Complete Examples

### Example 1: Approval Workflow Bot

**Webhook Server (Python/Flask):**

```python
from flask import Flask, request, jsonify

app = Flask(__name__)

# Store pending requests (use a database in production)
pending_requests = {}

@app.route('/webhook', methods=['POST'])
def webhook():
    data = request.json

    if data.get('type') == 'interaction':
        return handle_interaction(data)

    # Handle regular messages (slash commands)
    return handle_message(data)

def handle_interaction(data):
    custom_id = data['custom_id']
    user = data['user']

    if custom_id.startswith('approve_'):
        request_id = custom_id.replace('approve_', '')
        return jsonify({
            'content': f"Request {request_id} approved by {user['full_name']}!"
        })

    if custom_id.startswith('reject_'):
        request_id = custom_id.replace('reject_', '')
        return jsonify({
            'ephemeral': True,
            'content': f"You rejected request {request_id}."
        })

    return jsonify({})

def handle_message(data):
    message = data.get('message', {})
    content = message.get('content', '')

    if content.startswith('/approve'):
        # Create approval request widget
        return jsonify({
            'content': '',
            'widget_content': {
                'widget_type': 'interactive',
                'extra_data': {
                    'content': 'New approval request from ' + data['user']['full_name'],
                    'components': [{
                        'type': 'action_row',
                        'components': [
                            {
                                'type': 'button',
                                'label': 'Approve',
                                'style': 'success',
                                'custom_id': 'approve_001'
                            },
                            {
                                'type': 'button',
                                'label': 'Reject',
                                'style': 'danger',
                                'custom_id': 'reject_001'
                            }
                        ]
                    }]
                }
            }
        })

    return jsonify({})

if __name__ == '__main__':
    app.run(port=5000)
```

### Example 2: Weather Bot with Autocomplete

**Register the command:**

```bash
curl -X POST https://your-zulip.com/api/v1/bot_commands/register \
  -u bot@example.com:BOT_API_KEY \
  -d 'name=weather' \
  -d 'description=Get weather for a location' \
  -d 'options=[{"name":"location","type":"string","required":true}]'
```

**Handle autocomplete and messages:**

```python
@app.route('/webhook', methods=['POST'])
def webhook():
    data = request.json

    if data.get('type') == 'autocomplete':
        return handle_autocomplete(data)

    if data.get('type') == 'interaction':
        return handle_interaction(data)

    return handle_message(data)

def handle_autocomplete(data):
    if data['command'] == 'weather' and data['option'] == 'location':
        partial = data['partial'].lower()
        cities = ['San Francisco', 'New York', 'London', 'Tokyo', 'Sydney']
        matches = [c for c in cities if partial in c.lower()]
        return jsonify({
            'choices': [{'value': c, 'label': c} for c in matches[:5]]
        })
    return jsonify({'choices': []})
```

### Example 3: Embedded Bot with Interactions

For embedded bots (Python code running in Zulip):

```python
class MyBot:
    def handle_message(self, message, bot_handler):
        if message['content'].startswith('/quiz'):
            bot_handler.send_reply(
                message,
                content='',
                widget_content={
                    'widget_type': 'interactive',
                    'extra_data': {
                        'content': 'What is 2 + 2?',
                        'components': [{
                            'type': 'action_row',
                            'components': [
                                {'type': 'button', 'label': '3', 'custom_id': 'answer_3'},
                                {'type': 'button', 'label': '4', 'custom_id': 'answer_4', 'style': 'primary'},
                                {'type': 'button', 'label': '5', 'custom_id': 'answer_5'}
                            ]
                        }]
                    }
                }
            )

    def handle_interaction(self, interaction, bot_handler):
        custom_id = interaction['custom_id']
        if custom_id == 'answer_4':
            bot_handler.send_message({
                'type': 'stream',
                'to': 'general',
                'topic': 'Quiz',
                'content': f"{interaction['user']['full_name']} got it right!"
            })

    def get_autocomplete(self, command, option, partial, context, user, bot_handler):
        # Return dynamic suggestions
        return [
            {'value': 'option1', 'label': 'Option 1'},
            {'value': 'option2', 'label': 'Option 2'}
        ]
```

---

## Database Schema

### BotCommand

```sql
CREATE TABLE zerver_botcommand (
    id SERIAL PRIMARY KEY,
    bot_profile_id INTEGER REFERENCES zerver_userprofile(id),
    realm_id INTEGER REFERENCES zerver_realm(id),
    name VARCHAR(32) NOT NULL,
    description VARCHAR(100) NOT NULL,
    options_schema JSONB DEFAULT '[]',
    UNIQUE (realm_id, name)
);
```

### SubMessage (visible_to field)

```sql
ALTER TABLE zerver_submessage
ADD COLUMN visible_to JSONB DEFAULT NULL;
```

### UserProfile (is_trusted_bot field)

```sql
ALTER TABLE zerver_userprofile
ADD COLUMN is_trusted_bot BOOLEAN DEFAULT FALSE;
```

---

## Troubleshooting

### Interaction not received

1. Verify your bot is an **outgoing webhook bot** or **embedded bot**
2. Check the webhook URL is accessible from the Tulip server
3. Look for errors in the `bot_interactions` worker logs

### Freeform widget rejected

1. Ensure the bot has `is_trusted_bot = True`
2. Only realm admins can grant trusted status

### Command not showing in typeahead

1. Verify the command was registered successfully
2. Check the `bot_commands` event is being processed
3. Commands are realm-scoped; switch to the correct organization

### Autocomplete not working

1. Check your webhook returns `{"choices": [...]}` format
2. Autocomplete has a 5-second timeout
3. Verify the `type: "autocomplete"` request is being received
