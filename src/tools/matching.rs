/// Check if a target phrase matches an object/NPC name
/// Supports partial matching where all words in target must appear in name
pub fn matches_name(name: &str, target: &str) -> bool {
    let name_lower = name.to_lowercase();
    let target_lower = target.to_lowercase();

    // First try exact substring match (fast path)
    if name_lower.contains(&target_lower) {
        return true;
    }

    // If that fails, check if all target words appear in name
    let target_words: Vec<&str> = target_lower.split_whitespace().collect();
    let name_lower_str = name_lower.as_str();

    target_words.iter().all(|word| name_lower_str.contains(word))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exact_match() {
        assert!(matches_name("Rusty Sword", "rusty sword"));
        assert!(matches_name("Rusty Sword", "RUSTY SWORD"));
    }

    #[test]
    fn test_substring_match() {
        assert!(matches_name("Rusty Sword", "rusty"));
        assert!(matches_name("Rusty Sword", "sword"));
    }

    #[test]
    fn test_partial_words() {
        assert!(matches_name("Old Iron Key", "old key"));
        assert!(matches_name("Old Iron Key", "iron key"));
        assert!(matches_name("Old Iron Key", "old iron"));
    }

    #[test]
    fn test_no_match() {
        assert!(!matches_name("Rusty Sword", "golden"));
        assert!(!matches_name("Old Iron Key", "silver"));
    }

    #[test]
    fn test_order_independent() {
        assert!(matches_name("Old Iron Key", "key old"));
        assert!(matches_name("Rusty Sword", "sword rusty"));
    }
}
