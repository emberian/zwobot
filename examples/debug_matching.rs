use zwobot::world::WorldState;

fn main() -> anyhow::Result<()> {
    let world = WorldState::load_from_file("data/world_state.ron")?;

    let room = world.spatial.rooms.get("cellar").unwrap();

    println!("Objects in cellar:");
    for obj_id in &room.objects {
        if let Some(obj) = world.object_defs.get(obj_id) {
            println!("  ID: {}, Name: {}", obj_id, obj.name);

            let target = "old key";
            let target_lower = target.to_lowercase();
            let name_lower = obj.name.to_lowercase();
            let matches = name_lower.contains(&target_lower);

            println!("    Target: '{}' | Name lower: '{}' | Matches: {}",
                target_lower, name_lower, matches);
        }
    }

    Ok(())
}
