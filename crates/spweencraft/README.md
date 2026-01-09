# Spweencraft

Spween scene execution for Tulip bots. Enables interactive narrative experiences with choice-based gameplay.

## Features

- **Registry Streams**: Designate Tulip streams as "registries" where users can post spween scenes
- **Code Block Syntax**: Use ````spween` code blocks to define scenes
- **Auto-Registration**: Scenes posted in registry streams are automatically parsed and registered
- **Interactive Execution**: Players make choices by replying with numbers (1, 2, 3, etc.)
- **Per-Topic Sessions**: Multiple topics can run different spweens simultaneously
- **Rich Embeds**: Displays prose and choices in formatted embeds with reactions

## Usage

### Setup

```rust
use spweencraft::{SpweencraftData, commands::*};

// Define which streams are registries
let registry_streams = vec![
    "spween-library".to_string(),
    "narrative-content".to_string(),
];

let data = SpweencraftData::new(registry_streams);

let framework = Framework::builder()
    .data(data)
    .command(SpweenCommand)
    .command(SpweenEndCommand)
    .command(SpweenListCommand)
    .command(SpweenReloadCommand)
    .on_message(|ctx| Box::pin(handle_spween_message(ctx)))
    .build(config)
    .await?;
```

### Adding Scenes

In a registry stream, post a message with a ````spween` code block:

````
Here's a new adventure scene:

```spween
---
id: forest_encounter
title: The Dark Forest
weight: 10
cooldown: 3600
---

=== intro

You find yourself at the edge of a dark forest.
Strange sounds echo from within.

* [Enter bravely] { courage >= 5 }
  ~ courage += 1
  -> deep_forest

* [Scout the perimeter]
  ~ caution += 1
  -> forest_edge

* [Turn back]
  -> END

=== deep_forest

You venture into the darkness...
```
````

The bot will automatically:
1. Detect the ````spween` block
2. Parse the scene
3. Register it with ID `forest_encounter`
4. React with ✅ to confirm

### Running Scenes

**Start a scene:**
```
/spween forest_encounter
```

**Make choices:**
Reply with the choice number:
```
1
```

**List available scenes:**
```
/spween_list
```

**End current session:**
```
/spween_end
```

## Scene Format

Scenes use the spween DSL with YAML frontmatter:

```spween
---
id: unique_scene_id
title: Human-Readable Title
weight: 10           # Selection weight for random selection
cooldown: 3600       # Cooldown in seconds
tags: [combat, story]
---

=== passage_name

Prose text goes here.

* [Choice text] { condition }
  ~ effect_variable = value
  -> next_passage

* [Another choice]
  -> END
```

### Conditions

- `{ variable >= 5 }` - Compare variables
- `{ inventory.sword }` - Check if item exists
- `{ !enemy_defeated }` - Negate condition

### Effects

- `~ gold += 10` - Modify variable
- `~ visited = true` - Set variable
- `~ notify "event"` - Call custom function

## Architecture

- **SpweenRegistry**: Manages loaded scenes, tracks which came from which stream
- **BotEffectHandler**: Implements spween's EffectHandler trait for bot state
- **SpweenSession**: Runtime state for an active spween execution
- **SessionManager**: Manages active sessions per topic

## Example Bot Flow

1. User posts scene in `#spween-library` stream
2. Bot detects ````spween` block, parses and registers it
3. User runs `/spween forest_encounter` in `#games` stream
4. Bot creates a session for that topic, displays first passage
5. User replies `1` to make choice
6. Bot updates the session, displays next passage
7. Scene ends, bot shows "The End" and clears session
