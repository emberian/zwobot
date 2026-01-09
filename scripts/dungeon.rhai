// @title Pocket Dungeon
// @description A roguelike adventure - explore, fight, survive!
// @namespace dungeon
// @command dungeon() - Enter the dungeon (new game or continue)
// @command stats() - View your hero's stats
// @command explore() - Venture deeper into the dungeon
// @command rest() - Rest to recover HP (takes time)
// @command flee() - Attempt to escape combat
// @command attack() - Attack the current monster
// @command retire() - End your adventure and record your score
// @llm model=bartowski/Llama-3.2-3B-Instruct-GGUF quant=Q4_K_M tokens=256 temp=0.9

// ============================================================
// Game Constants
// ============================================================

const MAX_HP = 100;
const REST_HEAL_AMOUNT = 25;
const REST_TIME_MS = 5000;  // 5 seconds to rest
const FLEE_CHANCE = 60;     // 60% chance to flee

// Monster definitions [name, min_hp, max_hp, min_dmg, max_dmg, gold]
const MONSTERS = [
    ["Goblin",       10, 20,  3,  8,  5],
    ["Skeleton",     15, 25,  5, 10, 10],
    ["Orc",          25, 40,  8, 15, 20],
    ["Dark Elf",     20, 35, 10, 18, 25],
    ["Troll",        40, 60, 12, 20, 35],
    ["Wraith",       30, 45, 15, 25, 40],
    ["Dragon Wyrmling", 50, 80, 20, 35, 100]
];

const ROOM_TYPES = [
    "a dimly lit chamber",
    "a narrow corridor",
    "a vast cavern",
    "an ancient crypt",
    "a flooded passage",
    "a forgotten shrine",
    "a treasure vault",
    "a monster's lair"
];

const ROOM_PROMPT = `You are a dungeon master. Describe this dungeon room in 1-2 vivid sentences:
Type: {room_type}
Monster present: {monster}
Depth: Floor {depth}
Be atmospheric and brief. No game mechanics, just flavor.`;

const MONSTER_IMAGE_PROMPT = "dark fantasy art, {monster} creature in a dungeon, menacing pose, dramatic lighting, detailed illustration, no text";

const TREASURE_IMAGE_PROMPT = "fantasy treasure pile, gold coins and gems scattered in ancient dungeon room, glowing magical light, detailed illustration, no text";

// ============================================================
// Utility Functions
// ============================================================

fn random_range(min, max) {
    // Simple pseudo-random using turn count as seed
    let seed = turn() * 31 + sender_id() * 17 + state_get("depth");
    let range = max - min + 1;
    min + (seed % range)
}

fn random_percent() {
    random_range(0, 99)
}

fn pick_random(arr) {
    let idx = random_range(0, arr.len() - 1);
    arr[idx]
}

fn get_monster_for_depth(depth) {
    // Deeper = harder monsters
    let max_idx = if depth < 3 { 2 }
                  else if depth < 6 { 4 }
                  else if depth < 10 { 5 }
                  else { MONSTERS.len() - 1 };

    let idx = random_range(0, max_idx);
    let m = MONSTERS[idx];

    #{
        name: m[0],
        hp: random_range(m[1], m[2]),
        min_dmg: m[3],
        max_dmg: m[4],
        gold: m[5]
    }
}

fn format_hp_bar(current, max) {
    let pct = (current * 10) / max;
    let bar = "";
    let i = 0;
    while i < 10 {
        if i < pct { bar += "█"; }
        else { bar += "░"; }
        i += 1;
    }
    bar + " " + current + "/" + max
}

fn generate_monster_image(monster_name) {
    let prompt = prompt_template(MONSTER_IMAGE_PROMPT, #{
        monster: monster_name
    });
    let image_path = image_generate(prompt);
    // Check if generation failed (error messages start with "[")
    if image_path.starts_with("[") {
        log("Image generation failed: " + image_path);
        ()
    } else {
        image_path
    }
}

fn generate_treasure_image() {
    let image_path = image_generate(TREASURE_IMAGE_PROMPT);
    if image_path.starts_with("[") {
        log("Image generation failed: " + image_path);
        ()
    } else {
        image_path
    }
}

// ============================================================
// Save/Load with Persistence
// ============================================================

fn save_character() {
    let char_data = #{
        hp: state_get("hp"),
        gold: state_get("gold"),
        depth: state_get("depth"),
        kills: state_get("kills"),
        name: sender()
    };
    persist_save("dungeon", "hero_" + sender_id(), char_data);
    log("Character saved for " + sender());
}

fn load_character() {
    let data = persist_load("dungeon", "hero_" + sender_id());
    if data != () {
        state_set("hp", data.hp);
        state_set("gold", data.gold);
        state_set("depth", data.depth);
        state_set("kills", data.kills);
        true
    } else {
        false
    }
}

fn update_leaderboard(name, gold, depth, kills) {
    // Load current leaderboard
    let board = global_get("dungeon_leaderboard");
    if board == () {
        board = [];
    }

    // Add this score
    board.push(#{
        name: name,
        gold: gold,
        depth: depth,
        kills: kills,
        score: gold + (depth * 50) + (kills * 10)
    });

    // Sort by score (bubble sort since Rhai doesn't have sort)
    let n = board.len();
    let i = 0;
    while i < n - 1 {
        let j = 0;
        while j < n - i - 1 {
            if board[j].score < board[j + 1].score {
                let temp = board[j];
                board[j] = board[j + 1];
                board[j + 1] = temp;
            }
            j += 1;
        }
        i += 1;
    }

    // Keep top 10
    if board.len() > 10 {
        board = board.extract(0, 10);
    }

    global_set("dungeon_leaderboard", board);
}

// ============================================================
// Command Handlers
// ============================================================

fn on_command_dungeon() {
    add_player();

    // Try to load existing character
    if load_character() {
        let hp = state_get("hp");
        if hp <= 0 {
            // Dead character, start fresh
            init_new_character();
            respond_embed(#{
                title: "Your previous hero fell...",
                description: "But a new adventurer takes up the torch!",
                color: 0xe74c3c,
                fields: [
                    #{ name: "New Hero", value: sender(), inline: true },
                    #{ name: "HP", value: "" + MAX_HP, inline: true },
                    #{ name: "Gold", value: "0", inline: true }
                ],
                footer: "Use /explore to venture into the dungeon!"
            });
        } else {
            respond_embed(#{
                title: "Welcome back, " + sender() + "!",
                description: "Your adventure continues...",
                color: 0x2ecc71,
                fields: [
                    #{ name: "HP", value: format_hp_bar(hp, MAX_HP), inline: true },
                    #{ name: "Gold", value: "" + state_get("gold"), inline: true },
                    #{ name: "Floor", value: "" + state_get("depth"), inline: true }
                ],
                footer: "Use /explore to continue or /stats for details"
            });
        }
    } else {
        init_new_character();
        respond_embed(#{
            title: "A New Adventure Begins!",
            description: "You stand at the entrance to the Pocket Dungeon...",
            color: 0x3498db,
            fields: [
                #{ name: "Hero", value: sender(), inline: true },
                #{ name: "HP", value: "" + MAX_HP, inline: true },
                #{ name: "Goal", value: "Collect gold, go deep, survive!", inline: false }
            ],
            footer: "Use /explore to enter the dungeon!"
        });
    }
}

fn init_new_character() {
    state_set("hp", MAX_HP);
    state_set("gold", 0);
    state_set("depth", 0);
    state_set("kills", 0);
    state_set("monster", ());
    state_set("resting", false);
    save_character();
}

fn on_command_stats() {
    if !state_has("hp") {
        respond("You haven't entered the dungeon yet! Use `/dungeon` to begin.");
        return;
    }

    let hp = state_get("hp");
    let gold = state_get("gold");
    let depth = state_get("depth");
    let kills = state_get("kills");
    let score = gold + (depth * 50) + (kills * 10);

    let status = if state_get("resting") { "Resting..." }
                 else if state_get("monster") != () { "In Combat!" }
                 else { "Ready to explore" };

    respond_embed(#{
        title: sender() + "'s Stats",
        color: 0x9b59b6,
        fields: [
            #{ name: "HP", value: format_hp_bar(hp, MAX_HP), inline: false },
            #{ name: "Gold", value: "" + gold, inline: true },
            #{ name: "Floor", value: "" + depth, inline: true },
            #{ name: "Kills", value: "" + kills, inline: true },
            #{ name: "Score", value: "" + score, inline: true },
            #{ name: "Status", value: status, inline: true }
        ]
    });
}

fn on_command_explore() {
    if !state_has("hp") {
        respond("Use `/dungeon` to start your adventure first!");
        return;
    }

    if state_get("hp") <= 0 {
        respond("You are dead! Use `/dungeon` to start a new adventure.");
        return;
    }

    if state_get("resting") {
        respond("You're still resting! Wait a moment...");
        return;
    }

    if state_get("monster") != () {
        respond("You're in combat! Use `/attack` or `/flee`!");
        return;
    }

    // Go deeper
    let depth = state_get("depth") + 1;
    state_set("depth", depth);
    next_turn();  // Advance for randomness

    // Pick a room type
    let room_type = pick_random(ROOM_TYPES);

    // 70% chance of monster
    if random_percent() < 70 {
        let monster = get_monster_for_depth(depth);
        state_set("monster", monster);

        // Generate flavor text
        let prompt = prompt_template(ROOM_PROMPT, #{
            room_type: room_type,
            monster: monster.name,
            depth: depth
        });
        let desc = llm_generate(prompt);

        // Generate monster image
        let monster_image = generate_monster_image(monster.name);

        let embed = #{
            title: "Floor " + depth + " - " + monster.name + " appears!",
            description: desc,
            color: 0xe74c3c,
            fields: [
                #{ name: "Monster HP", value: "" + monster.hp, inline: true },
                #{ name: "Your HP", value: format_hp_bar(state_get("hp"), MAX_HP), inline: true }
            ],
            footer: "Use /attack or /flee!"
        };

        // Add image if generation succeeded
        if monster_image != () {
            embed.image = monster_image;
        }

        respond_embed(embed);
    } else {
        // Found treasure!
        let gold_found = random_range(5, 15 + depth * 3);
        state_set("gold", state_get("gold") + gold_found);
        save_character();

        // Generate treasure image
        let treasure_image = generate_treasure_image();

        let embed = #{
            title: "Floor " + depth + " - Treasure!",
            description: "You found an empty " + room_type + " with scattered gold!",
            color: 0xf1c40f,
            fields: [
                #{ name: "Gold Found", value: "+" + gold_found, inline: true },
                #{ name: "Total Gold", value: "" + state_get("gold"), inline: true }
            ],
            footer: "Use /explore to continue deeper..."
        };

        // Add image if generation succeeded
        if treasure_image != () {
            embed.image = treasure_image;
        }

        respond_embed(embed);
    }
}

fn on_command_attack() {
    if !state_has("hp") {
        respond("Use `/dungeon` to start your adventure first!");
        return;
    }

    let monster = state_get("monster");
    if monster == () {
        respond("There's nothing to fight! Use `/explore` to find monsters.");
        return;
    }

    next_turn();  // For randomness

    // Player attacks first
    let player_dmg = random_range(8, 20);
    let new_monster_hp = monster.hp - player_dmg;

    if new_monster_hp <= 0 {
        // Victory!
        let gold_reward = monster.gold + random_range(0, monster.gold);
        state_set("gold", state_get("gold") + gold_reward);
        state_set("kills", state_get("kills") + 1);
        state_set("monster", ());
        save_character();

        respond_embed(#{
            title: "Victory!",
            description: "You defeated the " + monster.name + "!",
            color: 0x2ecc71,
            fields: [
                #{ name: "Damage Dealt", value: "" + player_dmg, inline: true },
                #{ name: "Gold Earned", value: "+" + gold_reward, inline: true },
                #{ name: "Your HP", value: format_hp_bar(state_get("hp"), MAX_HP), inline: true }
            ],
            footer: "Use /explore to continue, or /rest to heal"
        });
        return;
    }

    // Monster attacks back
    let monster_dmg = random_range(monster.min_dmg, monster.max_dmg);
    let new_hp = state_get("hp") - monster_dmg;

    if new_hp <= 0 {
        // Death!
        state_set("hp", 0);
        let final_gold = state_get("gold");
        let final_depth = state_get("depth");
        let final_kills = state_get("kills");

        update_leaderboard(sender(), final_gold, final_depth, final_kills);

        // Delete saved character
        persist_delete("dungeon", "hero_" + sender_id());

        respond_embed(#{
            title: "You Have Fallen!",
            description: "The " + monster.name + " has slain you...",
            color: 0x2c3e50,
            fields: [
                #{ name: "Final Floor", value: "" + final_depth, inline: true },
                #{ name: "Gold Collected", value: "" + final_gold, inline: true },
                #{ name: "Monsters Slain", value: "" + final_kills, inline: true }
            ],
            footer: "Your score has been recorded. Use /dungeon to try again!"
        });
        end_session();
        return;
    }

    // Combat continues
    monster.hp = new_monster_hp;
    state_set("monster", monster);
    state_set("hp", new_hp);
    save_character();

    respond_embed(#{
        title: "Combat!",
        description: "The battle rages on...",
        color: 0xe67e22,
        fields: [
            #{ name: "You dealt", value: "" + player_dmg + " damage", inline: true },
            #{ name: monster.name + " dealt", value: "" + monster_dmg + " damage", inline: true },
            #{ name: "Your HP", value: format_hp_bar(new_hp, MAX_HP), inline: false },
            #{ name: monster.name + " HP", value: "" + new_monster_hp, inline: true }
        ],
        footer: "Use /attack or /flee!"
    });
}

fn on_command_flee() {
    if !state_has("hp") {
        respond("Use `/dungeon` to start your adventure first!");
        return;
    }

    let monster = state_get("monster");
    if monster == () {
        respond("There's nothing to flee from!");
        return;
    }

    next_turn();

    if random_percent() < FLEE_CHANCE {
        // Successful flee
        state_set("monster", ());
        // Go back a floor
        let depth = state_get("depth");
        if depth > 1 {
            state_set("depth", depth - 1);
        }
        save_character();

        respond_embed(#{
            title: "Escaped!",
            description: "You fled from the " + monster.name + "!",
            color: 0x3498db,
            fields: [
                #{ name: "Current Floor", value: "" + state_get("depth"), inline: true },
                #{ name: "HP", value: format_hp_bar(state_get("hp"), MAX_HP), inline: true }
            ],
            footer: "Use /explore to continue"
        });
    } else {
        // Failed to flee, monster attacks
        let monster_dmg = random_range(monster.min_dmg, monster.max_dmg);
        let new_hp = state_get("hp") - monster_dmg;

        if new_hp <= 0 {
            state_set("hp", 0);
            update_leaderboard(sender(), state_get("gold"), state_get("depth"), state_get("kills"));
            persist_delete("dungeon", "hero_" + sender_id());

            respond_embed(#{
                title: "Failed to Escape!",
                description: "The " + monster.name + " catches you as you flee...",
                color: 0x2c3e50,
                footer: "Game Over. Use /dungeon to try again."
            });
            end_session();
            return;
        }

        state_set("hp", new_hp);
        save_character();

        respond_embed(#{
            title: "Failed to Escape!",
            description: "The " + monster.name + " blocks your path and strikes!",
            color: 0xe74c3c,
            fields: [
                #{ name: "Damage Taken", value: "" + monster_dmg, inline: true },
                #{ name: "Your HP", value: format_hp_bar(new_hp, MAX_HP), inline: true }
            ],
            footer: "Use /attack or try /flee again!"
        });
    }
}

fn on_command_rest() {
    if !state_has("hp") {
        respond("Use `/dungeon` to start your adventure first!");
        return;
    }

    if state_get("monster") != () {
        respond("You can't rest while in combat!");
        return;
    }

    if state_get("resting") {
        respond("You're already resting!");
        return;
    }

    let hp = state_get("hp");
    if hp >= MAX_HP {
        respond("You're already at full health!");
        return;
    }

    state_set("resting", true);

    // Set a timer for rest completion
    set_timer(REST_TIME_MS, "rest_complete");

    respond_embed(#{
        title: "Resting...",
        description: "You find a quiet corner to rest and recover.",
        color: 0x1abc9c,
        fields: [
            #{ name: "Current HP", value: format_hp_bar(hp, MAX_HP), inline: true },
            #{ name: "Healing", value: "+" + REST_HEAL_AMOUNT + " in a few seconds", inline: true }
        ],
        footer: "Wait for rest to complete..."
    });
}

fn on_command_retire() {
    if !state_has("hp") {
        respond("You haven't started an adventure to retire from!");
        return;
    }

    let gold = state_get("gold");
    let depth = state_get("depth");
    let kills = state_get("kills");
    let score = gold + (depth * 50) + (kills * 10);

    update_leaderboard(sender(), gold, depth, kills);
    persist_delete("dungeon", "hero_" + sender_id());

    // Show leaderboard
    let board = global_get("dungeon_leaderboard");
    let board_text = "";
    if board != () {
        let i = 0;
        while i < board.len() && i < 5 {
            let entry = board[i];
            board_text += (i + 1) + ". " + entry.name + " - " + entry.score + " pts\n";
            i += 1;
        }
    }

    respond_embed(#{
        title: sender() + " Retires!",
        description: "You leave the dungeon with your treasures...",
        color: 0xf39c12,
        fields: [
            #{ name: "Final Stats", value: "Floor " + depth + " | " + gold + " gold | " + kills + " kills", inline: false },
            #{ name: "Final Score", value: "" + score, inline: true },
            #{ name: "Top Scores", value: if board_text == "" { "None yet!" } else { board_text }, inline: false }
        ],
        footer: "Thanks for playing! Use /dungeon to play again."
    });

    end_session();
}

// ============================================================
// Timer Handler
// ============================================================

fn on_timer(timer_id, context) {
    // Rest complete
    state_set("resting", false);

    let hp = state_get("hp");
    let new_hp = hp + REST_HEAL_AMOUNT;
    if new_hp > MAX_HP {
        new_hp = MAX_HP;
    }
    state_set("hp", new_hp);
    save_character();

    respond_embed(#{
        title: "Rest Complete!",
        description: "You feel refreshed and ready to continue.",
        color: 0x2ecc71,
        fields: [
            #{ name: "HP Restored", value: "+" + (new_hp - hp), inline: true },
            #{ name: "Current HP", value: format_hp_bar(new_hp, MAX_HP), inline: true }
        ],
        footer: "Use /explore to continue your adventure!"
    });
}
