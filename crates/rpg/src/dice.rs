//! Dice notation parser and roller
//!
//! Supports standard dice notation:
//! - `2d6` - Roll 2 six-sided dice
//! - `1d20+5` - Roll d20, add 5
//! - `4d6kh3` - Roll 4d6, keep highest 3
//! - `2d20kl1` - Roll 2d20, keep lowest 1 (disadvantage)
//! - `4d6!` - Exploding dice (max value rerolls and adds)

use rand::Rng;
use std::fmt;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DiceError {
    #[error("Invalid dice notation: {0}")]
    InvalidNotation(String),
    #[error("Too many dice (max 100): {0}")]
    TooManyDice(u32),
    #[error("Invalid die size (must be 2-1000): {0}")]
    InvalidDieSize(u32),
    #[error("Keep count exceeds dice count")]
    KeepExceedsDice,
}

/// A single die result
#[derive(Debug, Clone, Copy)]
pub struct DieResult {
    pub value: u32,
    pub kept: bool,
    pub exploded: bool,
}

/// Result of rolling dice
#[derive(Debug, Clone)]
pub struct RollResult {
    /// The original notation
    pub notation: String,
    /// Individual die results
    pub dice: Vec<DieResult>,
    /// Modifier applied
    pub modifier: i32,
    /// Final total
    pub total: i32,
    /// Label for the roll
    pub label: Option<String>,
}

impl RollResult {
    /// Format dice for display: [4] [6] or [~~3~~] [5] [6] for dropped
    pub fn format_dice(&self) -> String {
        self.dice
            .iter()
            .map(|d| {
                if d.kept {
                    if d.exploded {
                        format!("[{}!]", d.value)
                    } else {
                        format!("[{}]", d.value)
                    }
                } else {
                    format!("[~~{}~~]", d.value)
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Get just the kept dice total (before modifier)
    pub fn dice_total(&self) -> i32 {
        self.dice
            .iter()
            .filter(|d| d.kept)
            .map(|d| d.value as i32)
            .sum()
    }
}

/// Parsed dice notation
#[derive(Debug, Clone)]
pub struct DiceRoll {
    /// Number of dice to roll
    pub count: u32,
    /// Number of sides on each die
    pub sides: u32,
    /// Keep highest N dice (None = keep all)
    pub keep_highest: Option<u32>,
    /// Keep lowest N dice (None = keep all)
    pub keep_lowest: Option<u32>,
    /// Exploding dice (reroll and add on max)
    pub exploding: bool,
    /// Modifier to add/subtract
    pub modifier: i32,
}

impl DiceRoll {
    /// Parse dice notation string
    pub fn parse(notation: &str) -> Result<Self, DiceError> {
        let notation = notation.trim().to_lowercase();

        // Handle special shortcuts
        if notation == "adv" || notation == "advantage" {
            return Ok(DiceRoll {
                count: 2,
                sides: 20,
                keep_highest: Some(1),
                keep_lowest: None,
                exploding: false,
                modifier: 0,
            });
        }
        if notation == "dis" || notation == "disadvantage" {
            return Ok(DiceRoll {
                count: 2,
                sides: 20,
                keep_highest: None,
                keep_lowest: Some(1),
                exploding: false,
                modifier: 0,
            });
        }

        let mut remaining = notation.as_str();

        // Parse count (default 1)
        let count: u32;
        if let Some(d_pos) = remaining.find('d') {
            let count_str = &remaining[..d_pos];
            count = if count_str.is_empty() {
                1
            } else {
                count_str.parse().map_err(|_| DiceError::InvalidNotation(notation.clone()))?
            };
            remaining = &remaining[d_pos + 1..];
        } else {
            return Err(DiceError::InvalidNotation(notation));
        }

        if count > 100 {
            return Err(DiceError::TooManyDice(count));
        }

        // Parse sides and modifiers
        let mut keep_highest: Option<u32> = None;
        let mut keep_lowest: Option<u32> = None;
        let mut exploding = false;
        let mut modifier: i32 = 0;

        // Find where the sides number ends
        let mut sides_end = 0;
        for (i, c) in remaining.char_indices() {
            if c.is_ascii_digit() {
                sides_end = i + 1;
            } else {
                break;
            }
        }

        if sides_end == 0 {
            return Err(DiceError::InvalidNotation(notation));
        }

        let sides: u32 = remaining[..sides_end]
            .parse()
            .map_err(|_| DiceError::InvalidNotation(notation.clone()))?;

        if sides < 2 || sides > 1000 {
            return Err(DiceError::InvalidDieSize(sides));
        }

        remaining = &remaining[sides_end..];

        // Parse modifiers (kh, kl, !, +, -)
        while !remaining.is_empty() {
            if remaining.starts_with("kh") {
                remaining = &remaining[2..];
                let mut num_end = 0;
                for (i, c) in remaining.char_indices() {
                    if c.is_ascii_digit() {
                        num_end = i + 1;
                    } else {
                        break;
                    }
                }
                keep_highest = Some(if num_end == 0 {
                    1
                } else {
                    remaining[..num_end].parse().unwrap_or(1)
                });
                remaining = &remaining[num_end..];
            } else if remaining.starts_with("kl") {
                remaining = &remaining[2..];
                let mut num_end = 0;
                for (i, c) in remaining.char_indices() {
                    if c.is_ascii_digit() {
                        num_end = i + 1;
                    } else {
                        break;
                    }
                }
                keep_lowest = Some(if num_end == 0 {
                    1
                } else {
                    remaining[..num_end].parse().unwrap_or(1)
                });
                remaining = &remaining[num_end..];
            } else if remaining.starts_with('!') {
                exploding = true;
                remaining = &remaining[1..];
            } else if remaining.starts_with('+') || remaining.starts_with('-') {
                modifier = remaining
                    .parse()
                    .map_err(|_| DiceError::InvalidNotation(notation.clone()))?;
                break;
            } else {
                return Err(DiceError::InvalidNotation(notation));
            }
        }

        // Validate keep counts
        if let Some(kh) = keep_highest {
            if kh > count {
                return Err(DiceError::KeepExceedsDice);
            }
        }
        if let Some(kl) = keep_lowest {
            if kl > count {
                return Err(DiceError::KeepExceedsDice);
            }
        }

        Ok(DiceRoll {
            count,
            sides,
            keep_highest,
            keep_lowest,
            exploding,
            modifier,
        })
    }

    /// Roll the dice and return the result
    pub fn roll(&self) -> RollResult {
        let mut rng = rand::thread_rng();
        let mut dice: Vec<DieResult> = Vec::new();

        // Roll all dice
        for _ in 0..self.count {
            let mut value = rng.gen_range(1..=self.sides);
            let mut exploded = false;

            // Handle exploding dice
            if self.exploding {
                while value == self.sides {
                    exploded = true;
                    let extra = rng.gen_range(1..=self.sides);
                    value += extra;
                    if extra != self.sides {
                        break;
                    }
                }
            }

            dice.push(DieResult {
                value,
                kept: true,
                exploded,
            });
        }

        // Sort for keep highest/lowest
        let mut indices: Vec<usize> = (0..dice.len()).collect();
        indices.sort_by_key(|&i| dice[i].value);

        // Mark dice to drop
        if let Some(kh) = self.keep_highest {
            // Keep highest: drop the lowest (count - kh)
            let drop_count = self.count.saturating_sub(kh) as usize;
            for &i in indices.iter().take(drop_count) {
                dice[i].kept = false;
            }
        } else if let Some(kl) = self.keep_lowest {
            // Keep lowest: drop the highest (count - kl)
            let drop_count = self.count.saturating_sub(kl) as usize;
            for &i in indices.iter().rev().take(drop_count) {
                dice[i].kept = false;
            }
        }

        // Calculate total
        let dice_sum: i32 = dice.iter().filter(|d| d.kept).map(|d| d.value as i32).sum();
        let total = dice_sum + self.modifier;

        RollResult {
            notation: self.to_string(),
            dice,
            modifier: self.modifier,
            total,
            label: None,
        }
    }
}

impl fmt::Display for DiceRoll {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}d{}", self.count, self.sides)?;
        if let Some(kh) = self.keep_highest {
            write!(f, "kh{}", kh)?;
        }
        if let Some(kl) = self.keep_lowest {
            write!(f, "kl{}", kl)?;
        }
        if self.exploding {
            write!(f, "!")?;
        }
        if self.modifier > 0 {
            write!(f, "+{}", self.modifier)?;
        } else if self.modifier < 0 {
            write!(f, "{}", self.modifier)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple() {
        let roll = DiceRoll::parse("2d6").unwrap();
        assert_eq!(roll.count, 2);
        assert_eq!(roll.sides, 6);
        assert_eq!(roll.modifier, 0);
    }

    #[test]
    fn test_parse_with_modifier() {
        let roll = DiceRoll::parse("1d20+5").unwrap();
        assert_eq!(roll.count, 1);
        assert_eq!(roll.sides, 20);
        assert_eq!(roll.modifier, 5);

        let roll = DiceRoll::parse("1d20-3").unwrap();
        assert_eq!(roll.modifier, -3);
    }

    #[test]
    fn test_parse_keep_highest() {
        let roll = DiceRoll::parse("4d6kh3").unwrap();
        assert_eq!(roll.count, 4);
        assert_eq!(roll.sides, 6);
        assert_eq!(roll.keep_highest, Some(3));
    }

    #[test]
    fn test_parse_advantage() {
        let roll = DiceRoll::parse("adv").unwrap();
        assert_eq!(roll.count, 2);
        assert_eq!(roll.sides, 20);
        assert_eq!(roll.keep_highest, Some(1));
    }

    #[test]
    fn test_parse_exploding() {
        let roll = DiceRoll::parse("4d6!").unwrap();
        assert!(roll.exploding);
    }

    #[test]
    fn test_roll_range() {
        let roll = DiceRoll::parse("1d6").unwrap();
        for _ in 0..100 {
            let result = roll.roll();
            assert!(result.total >= 1 && result.total <= 6);
        }
    }

    #[test]
    fn test_keep_highest() {
        let roll = DiceRoll::parse("4d6kh3").unwrap();
        let result = roll.roll();
        let kept_count = result.dice.iter().filter(|d| d.kept).count();
        assert_eq!(kept_count, 3);
    }
}
