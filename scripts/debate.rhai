// @title Debate Club
// @description AI-powered debate opponent and judge
// @command rdebate(proposition: string!) - Start a new Rhai-powered debate
// @command rjudge() - Request judgment on the current debate
// @command rclear() - Clear the current debate
// @llm model=bartowski/Llama-3.2-3B-Instruct-GGUF quant=Q4_K_M tokens=1024 temp=0.7

// ============================================================
// Prompt Templates
// ============================================================

const OPPONENT_TEMPLATE = `You are a skilled debater arguing AGAINST the following proposition:

"{proposition}"

Your opponent {human_name} is arguing FOR this proposition.

{history}

{human_name} just said:
"{human_argument}"

Respond with a compelling counter-argument. Be concise but persuasive.
Address their specific points and make your case clearly.`;

const JUDGE_TEMPLATE = `You are an impartial judge evaluating a debate on the following proposition:

"{proposition}"

Here is the complete debate:

{history}

Please evaluate this debate and declare a winner. Consider:
1. Strength of arguments and evidence
2. Logical reasoning and coherence
3. Effective rebuttals of opposing points
4. Overall persuasiveness

Provide a brief analysis and then clearly state who won the debate and why.`;

// ============================================================
// Command Handlers
// ============================================================

/// Handle /rdebate command - start a new debate
fn on_command_rdebate(proposition) {
    // Check for existing debate
    if state_has("proposition") {
        respond("A debate is already in progress! Use /rclear to end it first.");
        return;
    }

    // Initialize state
    state_set("proposition", proposition);
    state_set("history", []);
    add_player();

    respond_embed(#{
        title: "New Debate Started!",
        description: proposition,
        color: 0x3498db,
        fields: [
            #{ name: "FOR", value: sender(), inline: true },
            #{ name: "AGAINST", value: "Bot", inline: true }
        ],
        footer: "Make your opening argument! Say 'judge' when ready for judgment."
    });
}

/// Handle /rjudge command - request judgment
fn on_command_rjudge() {
    if !state_has("proposition") {
        respond("No active debate. Start one with `/rdebate <proposition>`");
        return;
    }

    let history = state_get("history");
    if history.len() == 0 {
        respond("The debate has no arguments yet!");
        return;
    }

    log("Building judge prompt...");

    // Build prompt
    let prompt = build_judge_prompt();

    log("Generating judgment...");
    let judgment = llm_generate(prompt);
    log("Judgment generated");

    // Clear the debate
    state_clear();

    respond_embed(#{
        title: "The Judge's Verdict",
        description: judgment,
        color: 0x9b59b6,
        footer: "Debate concluded. Start a new one with /rdebate"
    });
}

/// Handle /rclear command
fn on_command_rclear() {
    state_clear();
    end_session();
    respond("Debate cleared. Start a new one with `/rdebate <proposition>`");
}

// ============================================================
// Message Handler
// ============================================================

/// Handle regular messages (debate turns)
fn on_message() {
    // Check for judge shorthand
    let msg = content();
    let trimmed = msg.trim();
    if trimmed == "judge" || trimmed == "judge!" {
        on_command_rjudge();
        return;
    }

    // Check for active debate
    if !state_has("proposition") {
        // No active debate - don't respond (let other handlers deal with it)
        return;
    }

    let human_arg = content();
    let human_name = sender();

    // Add to history
    let history = state_get("history");
    history.push(#{ speaker: human_name, message: human_arg });
    state_set("history", history);

    log("Building opponent prompt...");

    // Generate opponent response
    let prompt = build_opponent_prompt(human_name, human_arg);

    log("Generating opponent response...");
    let opponent_response = llm_generate(prompt);
    log("Response generated");

    // Add opponent response to history
    let history = state_get("history");
    history.push(#{ speaker: "Opponent", message: opponent_response });
    state_set("history", history);

    respond(opponent_response);
}

// ============================================================
// Helper Functions
// ============================================================

fn build_opponent_prompt(human_name, human_arg) {
    let proposition = state_get("proposition");
    let history = state_get("history");

    let history_text = "";
    // Skip the last entry since it's the current argument
    let history_len = history.len();
    if history_len > 1 {
        history_text = "Previous exchanges:\n";
        let i = 0;
        while i < history_len - 1 {
            let turn = history[i];
            if turn.speaker == "Opponent" {
                history_text += "You: " + turn.message + "\n\n";
            } else {
                history_text += turn.speaker + ": " + turn.message + "\n\n";
            }
            i += 1;
        }
    }

    prompt_template(OPPONENT_TEMPLATE, #{
        proposition: proposition,
        human_name: human_name,
        history: history_text,
        human_argument: human_arg
    })
}

fn build_judge_prompt() {
    let proposition = state_get("proposition");
    let history = state_get("history");

    let history_text = "";
    for turn in history {
        let role = if turn.speaker == "Opponent" {
            "AGAINST"
        } else {
            "FOR (" + turn.speaker + ")"
        };
        history_text += "[" + role + "]: " + turn.message + "\n\n";
    }

    prompt_template(JUDGE_TEMPLATE, #{
        proposition: proposition,
        history: history_text
    })
}
