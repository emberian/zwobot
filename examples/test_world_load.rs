use zwobot::world::WorldState;

fn main() -> anyhow::Result<()> {
    println!("Loading world state from data/world_state.ron...");

    let world = WorldState::load_from_file("data/world_state.ron")?;

    println!("✓ World loaded successfully!");
    println!("  Rooms: {}", world.spatial.rooms.len());
    println!("  Objects: {}", world.object_defs.len());
    println!("  NPCs: {}", world.npc_defs.len());

    // List rooms
    println!("\nRooms:");
    for (id, room) in &world.spatial.rooms {
        println!("  - {} ({})", room.name, id);
    }

    // List objects
    println!("\nObjects:");
    for (id, obj) in &world.object_defs {
        println!("  - {} ({})", obj.name, id);
    }

    // List NPCs
    println!("\nNPCs:");
    for (id, npc) in &world.npc_defs {
        println!("  - {} ({})", npc.name, id);
    }

    println!("\n✓ World state is valid and ready to use!");

    Ok(())
}
