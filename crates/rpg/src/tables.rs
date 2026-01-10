//! Random tables for loot, encounters, trinkets, etc.

use rand::seq::SliceRandom;
use rand::Rng;

/// A random table entry with weight
pub struct TableEntry {
    pub text: &'static str,
    pub weight: u32,
}

impl TableEntry {
    pub const fn new(text: &'static str) -> Self {
        Self { text, weight: 1 }
    }

    pub const fn weighted(text: &'static str, weight: u32) -> Self {
        Self { text, weight }
    }
}

/// Roll on a weighted table
pub fn roll_table(entries: &[TableEntry]) -> &'static str {
    let mut rng = rand::thread_rng();
    let total_weight: u32 = entries.iter().map(|e| e.weight).sum();
    let mut roll = rng.gen_range(0..total_weight);

    for entry in entries {
        if roll < entry.weight {
            return entry.text;
        }
        roll -= entry.weight;
    }

    entries.last().map(|e| e.text).unwrap_or("Nothing")
}

/// Treasure table (coins and gems)
pub static TREASURE: &[TableEntry] = &[
    TableEntry::weighted("A handful of copper coins (2d6 cp)", 10),
    TableEntry::weighted("A small pouch of silver (3d6 sp)", 8),
    TableEntry::weighted("A gold coin or two (1d4 gp)", 6),
    TableEntry::weighted("A gem worth 10 gp", 4),
    TableEntry::weighted("A small pile of gold (2d10 gp)", 3),
    TableEntry::weighted("A valuable gemstone (50 gp)", 2),
    TableEntry::weighted("A chest of gold coins (3d20 gp)", 1),
    TableEntry::weighted("A rare gem (100 gp)", 1),
];

/// Encounter table (monsters)
pub static ENCOUNTER: &[TableEntry] = &[
    TableEntry::weighted("1d4 Goblins", 10),
    TableEntry::weighted("1d3 Skeletons", 8),
    TableEntry::weighted("1d2 Wolves", 7),
    TableEntry::weighted("1 Giant Spider", 6),
    TableEntry::weighted("1d4 Kobolds", 6),
    TableEntry::weighted("1 Orc", 5),
    TableEntry::weighted("1 Zombie", 5),
    TableEntry::weighted("1d3 Giant Rats", 5),
    TableEntry::weighted("1 Hobgoblin", 4),
    TableEntry::weighted("1 Ghoul", 3),
    TableEntry::weighted("1 Bugbear", 2),
    TableEntry::weighted("1 Troll", 1),
    TableEntry::weighted("1 Ogre", 1),
];

/// Trinket table (random curiosities)
pub static TRINKET: &[TableEntry] = &[
    TableEntry::new("A small glass orb filled with smoke"),
    TableEntry::new("A brass key that unlocks nothing"),
    TableEntry::new("A dried flower pressed in paper"),
    TableEntry::new("A coin from a forgotten kingdom"),
    TableEntry::new("A tiny mechanical bird that chirps"),
    TableEntry::new("A blank book that resists writing"),
    TableEntry::new("A crystal that glows faintly at night"),
    TableEntry::new("A silver ring with an unknown sigil"),
    TableEntry::new("A compass that always points to you"),
    TableEntry::new("A small portrait of someone you don't know"),
    TableEntry::new("A lock of hair tied with red ribbon"),
    TableEntry::new("A tooth from an unknown creature"),
    TableEntry::new("A chess piece carved from bone"),
    TableEntry::new("A vial of water that never evaporates"),
    TableEntry::new("A pair of loaded dice"),
    TableEntry::new("A handwritten recipe for something inedible"),
    TableEntry::new("A map to somewhere that doesn't exist"),
    TableEntry::new("A broken pocket watch stuck at midnight"),
    TableEntry::new("A feather from an exotic bird"),
    TableEntry::new("A small mirror that shows a different reflection"),
];

/// NPC name components
pub static NPC_FIRST_NAMES: &[&str] = &[
    "Aldric", "Bran", "Cedric", "Doran", "Elara", "Fiona", "Gideon", "Helena",
    "Isolde", "Jasper", "Kira", "Lyra", "Magnus", "Nadia", "Osric", "Petra",
    "Quinn", "Rowan", "Sera", "Theron", "Una", "Vex", "Wren", "Xara", "Yara", "Zephyr",
];

pub static NPC_EPITHETS: &[&str] = &[
    "the Bold", "the Wise", "the Swift", "the Silent", "the Red",
    "the Black", "the Fair", "the Grim", "the Lucky", "the Lost",
    "One-Eye", "Ironhand", "Shadowcloak", "Fireheart", "Stormborn",
];

pub static NPC_OCCUPATIONS: &[&str] = &[
    "Blacksmith", "Innkeeper", "Merchant", "Guard", "Farmer",
    "Scholar", "Healer", "Hunter", "Sailor", "Thief",
    "Priest", "Bard", "Soldier", "Noble", "Beggar",
];

/// Generate a random NPC
pub fn generate_npc() -> String {
    let mut rng = rand::thread_rng();
    let name = NPC_FIRST_NAMES.choose(&mut rng).unwrap_or(&"Unknown");
    let has_epithet = rng.gen_bool(0.3);
    let occupation = NPC_OCCUPATIONS.choose(&mut rng).unwrap_or(&"Commoner");

    if has_epithet {
        let epithet = NPC_EPITHETS.choose(&mut rng).unwrap_or(&"the Unknown");
        format!("{} {}, {}", name, epithet, occupation)
    } else {
        format!("{}, {}", name, occupation)
    }
}

/// Roll on a table by name
pub fn roll_by_name(table_name: &str) -> String {
    match table_name.to_lowercase().as_str() {
        "treasure" | "loot" => roll_table(TREASURE).to_string(),
        "encounter" | "monster" => roll_table(ENCOUNTER).to_string(),
        "trinket" | "item" => roll_table(TRINKET).to_string(),
        "npc" | "character" => generate_npc(),
        _ => format!("Unknown table: {}", table_name),
    }
}
