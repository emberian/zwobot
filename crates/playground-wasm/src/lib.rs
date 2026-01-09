//! WASM playground for testing game widgets
//!
//! This crate exposes functions to generate widget data from Rust,
//! demonstrating that the serialization matches what the React
//! components expect.

use wasm_bindgen::prelude::*;
use tulip_bot::widget::{
    Dialogue, Choice, Mood, GameWidget,
    game_widget_dependencies,
};

// Initialize panic hook for better error messages
#[wasm_bindgen(start)]
pub fn init() {
    console_error_panic_hook::set_once();
}

/// Generate a simple dialogue widget
#[wasm_bindgen]
pub fn create_simple_dialogue(text: &str, speaker_name: &str) -> JsValue {
    let dialogue = Dialogue::new(text)
        .speaker(speaker_name);

    let game_widget = GameWidget::from(dialogue);
    let freeform = game_widget.to_freeform();

    serde_wasm_bindgen::to_value(&freeform).unwrap()
}

/// Generate a dialogue with choices
#[wasm_bindgen]
pub fn create_dialogue_with_choices(
    text: &str,
    speaker_name: &str,
    portrait_url: Option<String>,
) -> JsValue {
    let mut dialogue = Dialogue::new(text);

    if let Some(url) = portrait_url {
        dialogue = dialogue.speaker_with_portrait(speaker_name, url);
    } else {
        dialogue = dialogue.speaker(speaker_name);
    }

    dialogue = dialogue
        .mood(Mood::Neutral)
        .choice("Accept the quest", "accept")
        .choice("Ask for more information", "info")
        .choice("Decline politely", "decline");

    // Add a disabled choice with requirement
    let mut choices = dialogue.choices.unwrap_or_default();
    choices.push(
        Choice::new("Intimidate", "intimidate")
            .disabled()
            .requirement("[Requires 15 STR]")
    );
    dialogue.choices = Some(choices);

    let game_widget = GameWidget::from(dialogue);
    let freeform = game_widget.to_freeform();

    serde_wasm_bindgen::to_value(&freeform).unwrap()
}

/// Generate a dialogue with history
#[wasm_bindgen]
pub fn create_dialogue_with_history() -> JsValue {
    use tulip_bot::widget::HistoryEntry;

    let dialogue = Dialogue::new("So you've returned. Did you find the artifact?")
        .speaker_with_portrait("Sage Elara", "https://api.dicebear.com/7.x/personas/svg?seed=elara")
        .mood(Mood::Surprised)
        .history(vec![
            HistoryEntry { speaker: Some("You".into()), text: "I seek the ancient artifact.".into() },
            HistoryEntry { speaker: Some("Sage Elara".into()), text: "The artifact lies in the Forgotten Temple, to the north.".into() },
            HistoryEntry { speaker: Some("You".into()), text: "I will find it.".into() },
        ])
        .choice("Yes, here it is", "give_artifact")
        .choice("Not yet, I need more time", "need_time")
        .choice("What artifact?", "confused");

    let game_widget = GameWidget::from(dialogue);
    let freeform = game_widget.to_freeform();

    serde_wasm_bindgen::to_value(&freeform).unwrap()
}

/// Generate dialogue with different moods
#[wasm_bindgen]
pub fn create_dialogue_with_mood(mood: &str) -> JsValue {
    let mood_enum = match mood {
        "happy" => Mood::Happy,
        "angry" => Mood::Angry,
        "sad" => Mood::Sad,
        "surprised" => Mood::Surprised,
        _ => Mood::Neutral,
    };

    let text = match mood_enum {
        Mood::Happy => "Wonderful! You've done it!",
        Mood::Angry => "How dare you speak to me that way!",
        Mood::Sad => "I... I thought we were friends...",
        Mood::Surprised => "Wait, what?! That's impossible!",
        Mood::Neutral => "I see. Interesting.",
    };

    let dialogue = Dialogue::new(text)
        .speaker_with_portrait("Character", "https://api.dicebear.com/7.x/personas/svg?seed=mood")
        .mood(mood_enum)
        .continue_button("continue");

    let game_widget = GameWidget::from(dialogue);
    let freeform = game_widget.to_freeform();

    serde_wasm_bindgen::to_value(&freeform).unwrap()
}

/// Get just the dialogue props (without the freeform wrapper)
/// Useful for directly calling TulipWidgets.render()
#[wasm_bindgen]
pub fn get_dialogue_props(
    text: &str,
    speaker_name: &str,
    portrait_url: Option<String>,
    mood: Option<String>,
) -> JsValue {
    let mut dialogue = Dialogue::new(text);

    if let Some(url) = portrait_url {
        dialogue = dialogue.speaker_with_portrait(speaker_name, url);
    } else {
        dialogue = dialogue.speaker(speaker_name);
    }

    if let Some(m) = mood {
        dialogue = dialogue.mood(match m.as_str() {
            "happy" => Mood::Happy,
            "angry" => Mood::Angry,
            "sad" => Mood::Sad,
            "surprised" => Mood::Surprised,
            _ => Mood::Neutral,
        });
    }

    serde_wasm_bindgen::to_value(&dialogue).unwrap()
}

/// Get the game widget dependencies (URLs for script and style)
#[wasm_bindgen]
pub fn get_dependencies() -> JsValue {
    let deps = game_widget_dependencies();
    serde_wasm_bindgen::to_value(&deps).unwrap()
}

/// Demo: Create a full conversation flow
#[wasm_bindgen]
pub struct ConversationDemo {
    step: usize,
}

#[wasm_bindgen]
impl ConversationDemo {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self { step: 0 }
    }

    /// Get current dialogue props
    pub fn current(&self) -> JsValue {
        let dialogue = match self.step {
            0 => Dialogue::new("Greetings, traveler. What brings you to our village?")
                .speaker_with_portrait("Village Elder", "https://api.dicebear.com/7.x/personas/svg?seed=elder")
                .mood(Mood::Neutral)
                .choice("I'm looking for work", "work")
                .choice("Just passing through", "passing")
                .choice("I heard there's trouble here", "trouble"),
            1 => Dialogue::new("Ah, you've heard then. Yes, goblins have been raiding our farms at night.")
                .speaker_with_portrait("Village Elder", "https://api.dicebear.com/7.x/personas/svg?seed=elder")
                .mood(Mood::Sad)
                .choice("I can help", "help")
                .choice("That sounds dangerous", "dangerous")
                .choice("What's the pay?", "pay"),
            2 => Dialogue::new("Bless you! We'll pay 50 gold, and you can keep any loot you find.")
                .speaker_with_portrait("Village Elder", "https://api.dicebear.com/7.x/personas/svg?seed=elder")
                .mood(Mood::Happy)
                .choice("Deal. Where are they?", "accept")
                .choice("Make it 100 gold", "negotiate")
                .choice("On second thought, no thanks", "decline"),
            _ => Dialogue::new("The goblins lurk in the caves to the east. Be careful, adventurer!")
                .speaker_with_portrait("Village Elder", "https://api.dicebear.com/7.x/personas/svg?seed=elder")
                .mood(Mood::Neutral)
                .continue_button("start_quest"),
        };

        serde_wasm_bindgen::to_value(&dialogue).unwrap()
    }

    /// Advance to next step
    pub fn next(&mut self) {
        if self.step < 3 {
            self.step += 1;
        }
    }

    /// Go back to previous step
    pub fn prev(&mut self) {
        if self.step > 0 {
            self.step -= 1;
        }
    }

    /// Reset to beginning
    pub fn reset(&mut self) {
        self.step = 0;
    }

    /// Get current step number
    pub fn step(&self) -> usize {
        self.step
    }
}
