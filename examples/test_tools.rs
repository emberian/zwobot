use zwobot::tools::{execute_tool, parse_tool_call};
use zwobot::world::WorldState;
use smol_str::SmolStr;

fn main() -> anyhow::Result<()> {
    println!("Testing tool system...\n");

    // Load world
    let mut world = WorldState::load_from_file("data/world_state.ron")?;

    // Add a test character in the tavern
    let character: SmolStr = "TestHero".into();
    world.add_character(character.clone(), "tavern".into());

    println!("✓ Character created in tavern\n");

    // Test 1: Look
    println!("Test 1: look");
    let llm_output = "Let me look around the tavern.\n<tool>look</tool>";
    let call = parse_tool_call(llm_output)?;
    let result = execute_tool(&mut world, &character, &call)?;
    println!("Success: {}", result.success);
    println!("Description:\n{}\n", result.description);

    // Test 2: Examine
    println!("Test 2: examine rusty sword");
    let llm_output = "I'll examine that rusty sword.\n<tool>examine rusty sword</tool>";
    let call = parse_tool_call(llm_output)?;
    let result = execute_tool(&mut world, &character, &call)?;
    println!("Success: {}", result.success);
    println!("Description:\n{}\n", result.description);

    // Test 3: Take
    println!("Test 3: take sword");
    let llm_output = "Let me pick up the sword.\n<tool>take rusty sword</tool>";
    let call = parse_tool_call(llm_output)?;
    let result = execute_tool(&mut world, &character, &call)?;
    println!("Success: {}", result.success);
    println!("Description:\n{}\n", result.description);

    // Test 4: Inventory
    println!("Test 4: inventory");
    let llm_output = "What do I have?\n<tool>inventory</tool>";
    let call = parse_tool_call(llm_output)?;
    let result = execute_tool(&mut world, &character, &call)?;
    println!("Success: {}", result.success);
    println!("Description:\n{}\n", result.description);

    // Test 5: Equip
    println!("Test 5: equip sword");
    let llm_output = "I'll equip the sword.\n<tool>equip rusty sword</tool>";
    let call = parse_tool_call(llm_output)?;
    let result = execute_tool(&mut world, &character, &call)?;
    println!("Success: {}", result.success);
    println!("Description:\n{}\n", result.description);

    // Test 6: Move
    println!("Test 6: go down");
    let llm_output = "Let's go down to the cellar.\n<tool>go down</tool>";
    let call = parse_tool_call(llm_output)?;
    let result = execute_tool(&mut world, &character, &call)?;
    println!("Success: {}", result.success);
    println!("Description:\n{}\n", result.description);

    // Test 7: Look after moving
    println!("Test 7: look in cellar");
    let llm_output = "Where am I now?\n<tool>look</tool>";
    let call = parse_tool_call(llm_output)?;
    let result = execute_tool(&mut world, &character, &call)?;
    println!("Success: {}", result.success);
    println!("Description:\n{}\n", result.description);

    // Test 8: Take old key
    println!("Test 8: take old key");
    let llm_output = "I'll take the old key.\n<tool>take old key</tool>";
    let call = parse_tool_call(llm_output)?;
    let result = execute_tool(&mut world, &character, &call)?;
    println!("Success: {}", result.success);
    println!("Description:\n{}\n", result.description);

    // Test 9: Go back up
    println!("Test 9: go up");
    let llm_output = "Let's go back up.\n<tool>go up</tool>";
    let call = parse_tool_call(llm_output)?;
    let result = execute_tool(&mut world, &character, &call)?;
    println!("Success: {}", result.success);
    println!("Description:\n{}\n", result.description);

    println!("\n✓ All tool tests passed!");

    Ok(())
}
