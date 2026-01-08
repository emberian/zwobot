# Agent Instructions

## Issue Tracking

This project uses **bd (beads)** for issue tracking.
Run `bd prime` for workflow context, or install hooks (`bd hooks install`) for auto-injection.

**Quick reference:**
- `bd ready` - Find unblocked work
- `bd create "Title" --type task --priority 2` - Create issue
- `bd close <id>` - Complete work
- `bd sync` - Sync with git (run at session end)

For full workflow details: `bd prime`

## Landing the Plane (Session Completion)

**When ending a work session**, you MUST complete ALL steps below. Work is NOT complete until `git push` succeeds.

**MANDATORY WORKFLOW:**

1. **File issues for remaining work** - Create issues for anything that needs follow-up
2. **Run quality gates** (if code changed) - Tests, linters, builds
3. **Update issue status** - Close finished work, update in-progress items
4. **PUSH TO REMOTE** - This is MANDATORY:
   ```bash
   git pull --rebase
   bd sync
   git push
   git status  # MUST show "up to date with origin"
   ```
5. **Clean up** - Clear stashes, prune remote branches
6. **Verify** - All changes committed AND pushed
7. **Hand off** - Provide context for next session

**CRITICAL RULES:**
- Work is NOT complete until `git push` succeeds
- NEVER stop before pushing - that leaves work stranded locally
- NEVER say "ready to push when you are" - YOU must push
- If push fails, resolve and retry until it succeeds

## Code Philosophy

**This project values comprehensibility and type-driven correctness over mechanical test coverage.**

### Core Principles

1. **Maximum simplification while maintaining sufficient sophistication**
   - Solve the actual problem with the minimum accidental complexity
   - Ensure sophisticated techniques rise to the challenge of hard stuff

2. **Representability bijects sensibility**
   - Use the type system to make invalid states unrepresentable
   - Don't use boolean flags when an enum precisely models the state space
   - If you can represent it in the type system, it should make sense
   - Let the compiler enforce correctness

3. **Good Rust best practices and patterns**
   - Follow Rust idioms and conventions
   - Use the type system for safety and correctness
   - Leverage the compiler to catch bugs at compile time

### Testing Philosophy

**DO NOT write mechanical unit tests** that just verify "if I set x=5, then x equals 5."

**Instead:**
- Write modules that are self-evidently correct through good design
- Use types to enforce invariants (invalid states shouldn't compile)
- Write integration tests that capture real behavior holistically
- Test contracts and behavior, not implementation details

**Bad example:**
```rust
#[test]
fn test_add_character() {
    let mut world = WorldState::new();
    world.add_character("Hero".into(), "room1".into());
    assert_eq!(world.characters.len(), 1); // Mechanical verification
}
```

**Good approach:**
- Make `WorldState` comprehensible enough that adding a character is obviously correct
- Write integration tests that exercise full game scenarios
- Let the type system prevent invalid operations

**When to write tests:**
- Complex algorithms with non-obvious edge cases
- Integration points between systems
- Behavior that involves multiple components working together
- Regression tests for actual bugs found in practice

**When NOT to write tests:**
- Simple getters/setters
- Straightforward data structure operations
- Things that would fail to compile if wrong
- Mechanical verification of what the code literally says

### State Modeling

**Use enums and types to model state precisely:**

**Bad:**
```rust
struct Character {
    in_combat: bool,
    in_dialogue: bool,
    in_scene: bool,
}
// Can represent invalid states like all three being true
```

**Good:**
```rust
enum CharacterActivity {
    Exploring,
    InCombat { opponent: SmolStr },
    InDialogue { npc: SmolStr },
    InScene { scene_id: SmolStr, passage: SmolStr },
}
// Only valid states are representable
```
