//! Rhai engine with bot-specific functions registered

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use rhai::{Engine, AST, Dynamic, Map, Scope};
use tokio::sync::{mpsc, RwLock};
use tracing::{info, error, debug};

use crate::api::{ExecutionContext, ScriptResponse, ResponseBuilder, EmbedData};
use crate::bridge::{
    AsyncRequest, LlmConfig,
    blocking_llm_complete, blocking_llm_generate,
    blocking_http_get, blocking_http_post,
    blocking_persist_save, blocking_persist_load, blocking_persist_list,
    blocking_persist_delete, blocking_persist_exists,
};
use crate::session::GameSession;
use crate::script::GameScript;
use crate::{RhaiError, Result};

// Thread-local storage for execution context
thread_local! {
    static EXEC_CONTEXT: RefCell<Option<ExecutionContext>> = RefCell::new(None);
    static RESPONSE_BUILDER: RefCell<Option<ResponseBuilder>> = RefCell::new(None);
    static SESSION_STATE: RefCell<Option<Map>> = RefCell::new(None);
    static SESSION_PLAYERS: RefCell<Option<rhai::Array>> = RefCell::new(None);
    static SESSION_TURN: RefCell<i64> = RefCell::new(0);
    static LLM_CONFIG: RefCell<Option<LlmConfig>> = RefCell::new(None);
    static BRIDGE_TX: RefCell<Option<mpsc::Sender<AsyncRequest>>> = RefCell::new(None);
    static END_SESSION_FLAG: RefCell<bool> = RefCell::new(false);
    // New thread-locals for extended features
    static CURRENT_NAMESPACE: RefCell<Option<String>> = RefCell::new(None);
    static CURRENT_TOPIC: RefCell<Option<String>> = RefCell::new(None);
    static CURRENT_SCRIPT_ID: RefCell<Option<String>> = RefCell::new(None);
    static SCHEDULED_TIMERS: RefCell<Vec<(u64, Duration, Dynamic, bool)>> = RefCell::new(Vec::new());
    static CANCELLED_TIMERS: RefCell<Vec<u64>> = RefCell::new(Vec::new());
}

/// Bot-specialized Rhai engine with pre-registered functions
pub struct RhaiEngine {
    engine: Arc<Engine>,
}

impl RhaiEngine {
    /// Create a new Rhai engine with bot functions registered
    pub fn new(bridge_tx: mpsc::Sender<AsyncRequest>) -> Self {
        let mut engine = Engine::new();

        // Safety limits
        engine.set_max_operations(1_000_000);
        engine.set_max_call_levels(64);
        engine.set_max_expr_depths(64, 32);
        engine.set_max_string_size(1_000_000);
        engine.set_max_array_size(10_000);
        engine.set_max_map_size(10_000);

        // Register all API functions
        Self::register_context_api(&mut engine);
        Self::register_response_api(&mut engine);
        Self::register_state_api(&mut engine);
        Self::register_llm_api(&mut engine);
        Self::register_utility_api(&mut engine);
        Self::register_http_api(&mut engine);
        Self::register_persist_api(&mut engine);
        Self::register_shared_state_api(&mut engine);
        Self::register_timer_api(&mut engine);

        // Store bridge sender for LLM calls
        BRIDGE_TX.with(|b| {
            *b.borrow_mut() = Some(bridge_tx);
        });

        Self { engine: Arc::new(engine) }
    }

    /// Compile a script
    pub fn compile(&self, source: &str, filename: &str) -> Result<AST> {
        self.engine
            .compile(source)
            .map_err(|e| RhaiError::Parse(filename.to_string(), e.to_string()))
    }

    /// Get a clone of the engine Arc for use in spawn_blocking
    pub fn engine_arc(&self) -> Arc<Engine> {
        self.engine.clone()
    }

    /// Call a script function
    pub async fn call_script_fn(
        &self,
        script: &GameScript,
        sessions: &RwLock<HashMap<String, GameSession>>,
        topic: &str,
        fn_name: &str,
        ctx: &ExecutionContext,
        bridge_tx: &mpsc::Sender<AsyncRequest>,
    ) -> Result<Option<ScriptResponse>> {
        // Load session state into thread-local
        {
            let sessions = sessions.read().await;
            if let Some(session) = sessions.get(topic) {
                SESSION_STATE.with(|s| *s.borrow_mut() = Some(session.state.clone()));
                SESSION_PLAYERS.with(|p| *p.borrow_mut() = Some(session.players_as_dynamic()));
                SESSION_TURN.with(|t| *t.borrow_mut() = session.turn);
            } else {
                SESSION_STATE.with(|s| *s.borrow_mut() = Some(Map::new()));
                SESSION_PLAYERS.with(|p| *p.borrow_mut() = Some(rhai::Array::new()));
                SESSION_TURN.with(|t| *t.borrow_mut() = 0);
            }
        }

        // Set up thread-local context
        EXEC_CONTEXT.with(|c| *c.borrow_mut() = Some(ctx.clone()));
        RESPONSE_BUILDER.with(|r| *r.borrow_mut() = Some(ResponseBuilder::new()));
        LLM_CONFIG.with(|l| *l.borrow_mut() = script.meta.default_llm.clone());
        BRIDGE_TX.with(|b| *b.borrow_mut() = Some(bridge_tx.clone()));
        END_SESSION_FLAG.with(|f| *f.borrow_mut() = false);

        // Set up extended context
        let namespace = {
            let sessions = sessions.read().await;
            sessions.get(topic).and_then(|s| s.namespace.clone())
        };
        CURRENT_NAMESPACE.with(|n| *n.borrow_mut() = namespace);
        CURRENT_TOPIC.with(|t| *t.borrow_mut() = Some(topic.to_string()));
        CURRENT_SCRIPT_ID.with(|s| *s.borrow_mut() = Some(script.meta.title.clone()));
        SCHEDULED_TIMERS.with(|t| t.borrow_mut().clear());
        CANCELLED_TIMERS.with(|t| t.borrow_mut().clear());

        // Create scope with command arguments
        let mut scope = Scope::new();

        // For command handlers, push the first required argument as positional
        if fn_name.starts_with("on_command_") && !ctx.args.is_empty() {
            // Find the command in script meta
            let cmd_name = fn_name.strip_prefix("on_command_").unwrap_or("");
            if let Some(cmd) = script.meta.commands.iter().find(|c| c.name == cmd_name) {
                // Push args in order
                for opt in &cmd.options {
                    if let Some(value) = ctx.args.get(&opt.name) {
                        scope.push(opt.name.clone(), value.clone());
                    }
                }
            }
        }

        // Run script in blocking context (Rhai is sync)
        let engine = self.engine.clone();
        let ast = script.ast.clone();
        let fn_name_owned = fn_name.to_string();

        let result = tokio::task::spawn_blocking(move || {
            // Check if function exists in script
            let has_fn = ast.iter_functions().any(|f| f.name == fn_name_owned);
            if !has_fn {
                return Err(RhaiError::FunctionNotFound(fn_name_owned));
            }

            // Call the function
            let call_result: std::result::Result<Dynamic, _> = engine.call_fn(&mut scope, &ast, &fn_name_owned, ());

            match call_result {
                Ok(_) => Ok(()),
                Err(e) => Err(RhaiError::Runtime(e.to_string())),
            }
        })
        .await
        .map_err(|e| RhaiError::Runtime(format!("Task join error: {}", e)))?;

        // Handle script execution result
        if let Err(e) = result {
            // Check if it's just a missing function (not an error for optional handlers)
            if matches!(e, RhaiError::FunctionNotFound(_)) && fn_name == "on_message" {
                return Ok(None);
            }
            return Err(e);
        }

        // Get response from thread-local
        let response = RESPONSE_BUILDER.with(|r| {
            r.borrow_mut().take().map(|b| b.take())
        }).unwrap_or_default();

        // Check if session should end
        let should_end = END_SESSION_FLAG.with(|f| *f.borrow());

        // Save session state back
        {
            let mut sessions = sessions.write().await;
            if let Some(session) = sessions.get_mut(topic) {
                SESSION_STATE.with(|s| {
                    if let Some(state) = s.borrow_mut().take() {
                        session.state = state;
                    }
                });
                SESSION_TURN.with(|t| {
                    session.turn = *t.borrow();
                });
                // Handle add_player calls
                // Players are managed via add_player API calls which update the thread-local
                // and then get synced back to the session here
                SESSION_PLAYERS.with(|p| {
                    // Take the players list - it was populated during script execution
                    let _ = p.borrow_mut().take();
                });
            }

            // End session if flagged
            if should_end {
                sessions.remove(topic);
            }
        }

        // Add end_session flag to response
        let mut response = response;
        response.end_session = should_end;

        if response.is_empty() && !should_end {
            Ok(None)
        } else {
            Ok(Some(response))
        }
    }

    /// Register context API functions
    fn register_context_api(engine: &mut Engine) {
        // topic() -> String
        engine.register_fn("topic", || -> String {
            EXEC_CONTEXT.with(|c| {
                c.borrow().as_ref().map(|ctx| ctx.topic.clone()).unwrap_or_default()
            })
        });

        // channel() -> String
        engine.register_fn("channel", || -> String {
            EXEC_CONTEXT.with(|c| {
                c.borrow().as_ref().map(|ctx| ctx.channel.clone()).unwrap_or_default()
            })
        });

        // sender() -> String
        engine.register_fn("sender", || -> String {
            EXEC_CONTEXT.with(|c| {
                c.borrow().as_ref().map(|ctx| ctx.sender_name.clone()).unwrap_or_default()
            })
        });

        // sender_id() -> i64
        engine.register_fn("sender_id", || -> i64 {
            EXEC_CONTEXT.with(|c| {
                c.borrow().as_ref().map(|ctx| ctx.sender_id).unwrap_or(0)
            })
        });

        // content() -> String
        engine.register_fn("content", || -> String {
            EXEC_CONTEXT.with(|c| {
                c.borrow().as_ref().map(|ctx| ctx.content.clone()).unwrap_or_default()
            })
        });

        // arg(name: &str) -> Dynamic
        engine.register_fn("arg", |name: &str| -> Dynamic {
            EXEC_CONTEXT.with(|c| {
                c.borrow()
                    .as_ref()
                    .and_then(|ctx| ctx.args.get(name).map(|s| Dynamic::from(s.clone())))
                    .unwrap_or(Dynamic::UNIT)
            })
        });

        // has_arg(name: &str) -> bool
        engine.register_fn("has_arg", |name: &str| -> bool {
            EXEC_CONTEXT.with(|c| {
                c.borrow()
                    .as_ref()
                    .map(|ctx| ctx.args.contains_key(name))
                    .unwrap_or(false)
            })
        });
    }

    /// Register response API functions
    fn register_response_api(engine: &mut Engine) {
        // respond(text: &str)
        engine.register_fn("respond", |text: &str| {
            RESPONSE_BUILDER.with(|r| {
                if let Some(ref mut builder) = *r.borrow_mut() {
                    builder.set_content(text.to_string());
                }
            });
        });

        // respond(text: String) - overload for String type
        engine.register_fn("respond", |text: String| {
            RESPONSE_BUILDER.with(|r| {
                if let Some(ref mut builder) = *r.borrow_mut() {
                    builder.set_content(text);
                }
            });
        });

        // respond_embed(map: Map)
        engine.register_fn("respond_embed", |map: Map| {
            RESPONSE_BUILDER.with(|r| {
                if let Some(ref mut builder) = *r.borrow_mut() {
                    builder.set_embed(EmbedData::from_map(&map));
                }
            });
        });

        // react(emoji: &str)
        engine.register_fn("react", |emoji: &str| {
            RESPONSE_BUILDER.with(|r| {
                if let Some(ref mut builder) = *r.borrow_mut() {
                    builder.add_reaction(emoji.to_string());
                }
            });
        });

        // unreact(emoji: &str)
        engine.register_fn("unreact", |emoji: &str| {
            RESPONSE_BUILDER.with(|r| {
                if let Some(ref mut builder) = *r.borrow_mut() {
                    builder.remove_reaction(emoji.to_string());
                }
            });
        });
    }

    /// Register state API functions
    fn register_state_api(engine: &mut Engine) {
        // state_get(key: &str) -> Dynamic
        engine.register_fn("state_get", |key: &str| -> Dynamic {
            SESSION_STATE.with(|s| {
                s.borrow()
                    .as_ref()
                    .and_then(|state| state.get(key).cloned())
                    .unwrap_or(Dynamic::UNIT)
            })
        });

        // state_set(key: &str, value: Dynamic)
        engine.register_fn("state_set", |key: &str, value: Dynamic| {
            SESSION_STATE.with(|s| {
                if let Some(ref mut state) = *s.borrow_mut() {
                    state.insert(key.into(), value);
                }
            });
        });

        // state_has(key: &str) -> bool
        engine.register_fn("state_has", |key: &str| -> bool {
            SESSION_STATE.with(|s| {
                s.borrow()
                    .as_ref()
                    .map(|state| state.contains_key(key))
                    .unwrap_or(false)
            })
        });

        // state_clear()
        engine.register_fn("state_clear", || {
            SESSION_STATE.with(|s| {
                if let Some(ref mut state) = *s.borrow_mut() {
                    state.clear();
                }
            });
            SESSION_TURN.with(|t| *t.borrow_mut() = 0);
        });

        // state_all() -> Map
        engine.register_fn("state_all", || -> Map {
            SESSION_STATE.with(|s| {
                s.borrow().clone().unwrap_or_default()
            })
        });

        // turn() -> i64
        engine.register_fn("turn", || -> i64 {
            SESSION_TURN.with(|t| *t.borrow())
        });

        // set_turn(n: i64)
        engine.register_fn("set_turn", |n: i64| {
            SESSION_TURN.with(|t| *t.borrow_mut() = n);
        });

        // next_turn() -> i64
        engine.register_fn("next_turn", || -> i64 {
            SESSION_TURN.with(|t| {
                let mut turn = t.borrow_mut();
                *turn += 1;
                *turn
            })
        });

        // add_player()
        engine.register_fn("add_player", || {
            let (id, name) = EXEC_CONTEXT.with(|c| {
                c.borrow()
                    .as_ref()
                    .map(|ctx| (ctx.sender_id, ctx.sender_name.clone()))
                    .unwrap_or((0, String::new()))
            });

            SESSION_PLAYERS.with(|p| {
                if let Some(ref mut players) = *p.borrow_mut() {
                    // Check if player already exists
                    let exists = players.iter().any(|pl| {
                        pl.clone()
                            .try_cast::<Map>()
                            .and_then(|m| m.get("id").cloned())
                            .and_then(|v| v.try_cast::<i64>())
                            .map(|pid| pid == id)
                            .unwrap_or(false)
                    });

                    if !exists {
                        let mut player_map = Map::new();
                        player_map.insert("id".into(), Dynamic::from(id));
                        player_map.insert("name".into(), Dynamic::from(name));
                        players.push(Dynamic::from(player_map));
                    }
                }
            });
        });

        // players() -> Array
        engine.register_fn("players", || -> rhai::Array {
            SESSION_PLAYERS.with(|p| {
                p.borrow().clone().unwrap_or_default()
            })
        });

        // end_session()
        engine.register_fn("end_session", || {
            END_SESSION_FLAG.with(|f| *f.borrow_mut() = true);
        });
    }

    /// Register LLM API functions
    fn register_llm_api(engine: &mut Engine) {
        // llm_generate(prompt: &str) -> String
        engine.register_fn("llm_generate", |prompt: &str| -> String {
            let config = LLM_CONFIG.with(|l| l.borrow().clone());
            let bridge_tx = BRIDGE_TX.with(|b| b.borrow().clone());

            match (config, bridge_tx) {
                (Some(config), Some(tx)) => {
                    match blocking_llm_generate(&tx, config, prompt.to_string()) {
                        Ok(result) => result,
                        Err(e) => {
                            error!("LLM generation failed: {}", e);
                            format!("[LLM Error: {}]", e)
                        }
                    }
                }
                (None, _) => {
                    error!("No LLM config set for script");
                    "[Error: No LLM config]".to_string()
                }
                (_, None) => {
                    error!("No bridge available for LLM");
                    "[Error: No LLM bridge]".to_string()
                }
            }
        });

        // llm_generate(prompt: String) -> String (overload)
        engine.register_fn("llm_generate", |prompt: String| -> String {
            let config = LLM_CONFIG.with(|l| l.borrow().clone());
            let bridge_tx = BRIDGE_TX.with(|b| b.borrow().clone());

            match (config, bridge_tx) {
                (Some(config), Some(tx)) => {
                    match blocking_llm_generate(&tx, config, prompt) {
                        Ok(result) => result,
                        Err(e) => {
                            error!("LLM generation failed: {}", e);
                            format!("[LLM Error: {}]", e)
                        }
                    }
                }
                (None, _) => "[Error: No LLM config]".to_string(),
                (_, None) => "[Error: No LLM bridge]".to_string(),
            }
        });

        // llm_generate_with(config: Map, prompt: &str) -> String
        engine.register_fn("llm_generate_with", |config_map: Map, prompt: &str| -> String {
            let bridge_tx = BRIDGE_TX.with(|b| b.borrow().clone());

            let config = LlmConfig {
                model_id: config_map
                    .get("model_id")
                    .and_then(|v| v.clone().try_cast::<String>())
                    .unwrap_or_default(),
                quantization: config_map
                    .get("quantization")
                    .and_then(|v| v.clone().try_cast::<String>())
                    .unwrap_or_else(|| "Q4_K_M".to_string()),
                max_tokens: config_map
                    .get("max_tokens")
                    .and_then(|v| v.clone().try_cast::<i64>())
                    .map(|v| v as u32)
                    .unwrap_or(1024),
                temperature: config_map
                    .get("temperature")
                    .and_then(|v| v.clone().try_cast::<f64>())
                    .map(|v| v as f32)
                    .unwrap_or(0.7),
            };

            if config.model_id.is_empty() {
                return "[Error: No model_id in config]".to_string();
            }

            match bridge_tx {
                Some(tx) => {
                    match blocking_llm_generate(&tx, config, prompt.to_string()) {
                        Ok(result) => result,
                        Err(e) => format!("[LLM Error: {}]", e),
                    }
                }
                None => "[Error: No LLM bridge]".to_string(),
            }
        });

        // llm_complete(prompt: &str) -> String (raw completion, no chat template)
        engine.register_fn("llm_complete", |prompt: &str| -> String {
            let config = LLM_CONFIG.with(|l| l.borrow().clone());
            let bridge_tx = BRIDGE_TX.with(|b| b.borrow().clone());

            match (config, bridge_tx) {
                (Some(config), Some(tx)) => {
                    match blocking_llm_complete(&tx, config, prompt.to_string()) {
                        Ok(result) => result,
                        Err(e) => {
                            error!("LLM completion failed: {}", e);
                            format!("[LLM Error: {}]", e)
                        }
                    }
                }
                (None, _) => {
                    error!("No LLM config set for script");
                    "[Error: No LLM config]".to_string()
                }
                (_, None) => {
                    error!("No bridge available for LLM");
                    "[Error: No LLM bridge]".to_string()
                }
            }
        });

        // llm_complete(prompt: String) -> String (overload)
        engine.register_fn("llm_complete", |prompt: String| -> String {
            let config = LLM_CONFIG.with(|l| l.borrow().clone());
            let bridge_tx = BRIDGE_TX.with(|b| b.borrow().clone());

            match (config, bridge_tx) {
                (Some(config), Some(tx)) => {
                    match blocking_llm_complete(&tx, config, prompt) {
                        Ok(result) => result,
                        Err(e) => {
                            error!("LLM completion failed: {}", e);
                            format!("[LLM Error: {}]", e)
                        }
                    }
                }
                (None, _) => "[Error: No LLM config]".to_string(),
                (_, None) => "[Error: No LLM bridge]".to_string(),
            }
        });

        // llm_complete_with(config: Map, prompt: &str) -> String
        engine.register_fn("llm_complete_with", |config_map: Map, prompt: &str| -> String {
            let bridge_tx = BRIDGE_TX.with(|b| b.borrow().clone());

            let config = LlmConfig {
                model_id: config_map
                    .get("model_id")
                    .and_then(|v| v.clone().try_cast::<String>())
                    .unwrap_or_default(),
                quantization: config_map
                    .get("quantization")
                    .and_then(|v| v.clone().try_cast::<String>())
                    .unwrap_or_else(|| "Q4_K_M".to_string()),
                max_tokens: config_map
                    .get("max_tokens")
                    .and_then(|v| v.clone().try_cast::<i64>())
                    .map(|v| v as u32)
                    .unwrap_or(1024),
                temperature: config_map
                    .get("temperature")
                    .and_then(|v| v.clone().try_cast::<f64>())
                    .map(|v| v as f32)
                    .unwrap_or(0.7),
            };

            if config.model_id.is_empty() {
                return "[Error: No model_id in config]".to_string();
            }

            match bridge_tx {
                Some(tx) => {
                    match blocking_llm_complete(&tx, config, prompt.to_string()) {
                        Ok(result) => result,
                        Err(e) => format!("[LLM Error: {}]", e),
                    }
                }
                None => "[Error: No LLM bridge]".to_string(),
            }
        });
    }

    /// Register utility API functions
    fn register_utility_api(engine: &mut Engine) {
        // prompt_template(template: &str, vars: Map) -> String
        engine.register_fn("prompt_template", |template: &str, vars: Map| -> String {
            let mut result = template.to_string();
            for (key, value) in vars.iter() {
                let placeholder = format!("{{{}}}", key);
                let replacement = value.to_string();
                result = result.replace(&placeholder, &replacement);
            }
            result
        });

        // log(message: &str) - for debugging
        engine.register_fn("log", |message: &str| {
            info!("[Rhai Script] {}", message);
        });

        // log(message: String)
        engine.register_fn("log", |message: String| {
            info!("[Rhai Script] {}", message);
        });
    }

    /// Register HTTP API functions
    fn register_http_api(engine: &mut Engine) {
        // http_get(url: &str) -> String
        engine.register_fn("http_get", |url: &str| -> String {
            let bridge_tx = BRIDGE_TX.with(|b| b.borrow().clone());
            match bridge_tx {
                Some(tx) => {
                    match blocking_http_get(&tx, url.to_string(), HashMap::new()) {
                        Ok(body) => body,
                        Err(e) => format!("[HTTP Error: {}]", e),
                    }
                }
                None => "[Error: No bridge]".to_string(),
            }
        });

        // http_get(url: String) -> String
        engine.register_fn("http_get", |url: String| -> String {
            let bridge_tx = BRIDGE_TX.with(|b| b.borrow().clone());
            match bridge_tx {
                Some(tx) => {
                    match blocking_http_get(&tx, url, HashMap::new()) {
                        Ok(body) => body,
                        Err(e) => format!("[HTTP Error: {}]", e),
                    }
                }
                None => "[Error: No bridge]".to_string(),
            }
        });

        // http_get_with(url: &str, headers: Map) -> String
        engine.register_fn("http_get_with", |url: &str, headers: Map| -> String {
            let bridge_tx = BRIDGE_TX.with(|b| b.borrow().clone());
            let headers_map: HashMap<String, String> = headers
                .iter()
                .filter_map(|(k, v)| {
                    v.clone().try_cast::<String>().map(|s| (k.to_string(), s))
                })
                .collect();

            match bridge_tx {
                Some(tx) => {
                    match blocking_http_get(&tx, url.to_string(), headers_map) {
                        Ok(body) => body,
                        Err(e) => format!("[HTTP Error: {}]", e),
                    }
                }
                None => "[Error: No bridge]".to_string(),
            }
        });

        // http_post(url: &str, body: &str) -> String
        engine.register_fn("http_post", |url: &str, body: &str| -> String {
            let bridge_tx = BRIDGE_TX.with(|b| b.borrow().clone());
            match bridge_tx {
                Some(tx) => {
                    match blocking_http_post(
                        &tx,
                        url.to_string(),
                        HashMap::new(),
                        body.to_string(),
                        "application/json".to_string(),
                    ) {
                        Ok(resp) => resp,
                        Err(e) => format!("[HTTP Error: {}]", e),
                    }
                }
                None => "[Error: No bridge]".to_string(),
            }
        });

        // http_post(url: &str, body: Map) -> String (serializes Map to JSON)
        engine.register_fn("http_post", |url: &str, body: Map| -> String {
            let bridge_tx = BRIDGE_TX.with(|b| b.borrow().clone());
            let json_body = crate::persistence::dynamic_to_json(&Dynamic::from(body));
            let body_str = serde_json::to_string(&json_body).unwrap_or_default();

            match bridge_tx {
                Some(tx) => {
                    match blocking_http_post(
                        &tx,
                        url.to_string(),
                        HashMap::new(),
                        body_str,
                        "application/json".to_string(),
                    ) {
                        Ok(resp) => resp,
                        Err(e) => format!("[HTTP Error: {}]", e),
                    }
                }
                None => "[Error: No bridge]".to_string(),
            }
        });

        // http_post_with(url: &str, body: &str, headers: Map) -> String
        engine.register_fn("http_post_with", |url: &str, body: &str, headers: Map| -> String {
            let bridge_tx = BRIDGE_TX.with(|b| b.borrow().clone());
            let headers_map: HashMap<String, String> = headers
                .iter()
                .filter_map(|(k, v)| {
                    v.clone().try_cast::<String>().map(|s| (k.to_string(), s))
                })
                .collect();

            match bridge_tx {
                Some(tx) => {
                    match blocking_http_post(
                        &tx,
                        url.to_string(),
                        headers_map,
                        body.to_string(),
                        "application/json".to_string(),
                    ) {
                        Ok(resp) => resp,
                        Err(e) => format!("[HTTP Error: {}]", e),
                    }
                }
                None => "[Error: No bridge]".to_string(),
            }
        });
    }

    /// Register persistence API functions
    fn register_persist_api(engine: &mut Engine) {
        // persist_save(namespace: &str, key: &str, data: Dynamic)
        engine.register_fn("persist_save", |namespace: &str, key: &str, data: Dynamic| {
            let bridge_tx = BRIDGE_TX.with(|b| b.borrow().clone());
            match bridge_tx {
                Some(tx) => {
                    if let Err(e) = blocking_persist_save(&tx, namespace.to_string(), key.to_string(), data) {
                        error!("persist_save failed: {}", e);
                    }
                }
                None => error!("persist_save: No bridge"),
            }
        });

        // persist_load(namespace: &str, key: &str) -> Dynamic
        engine.register_fn("persist_load", |namespace: &str, key: &str| -> Dynamic {
            let bridge_tx = BRIDGE_TX.with(|b| b.borrow().clone());
            match bridge_tx {
                Some(tx) => {
                    match blocking_persist_load(&tx, namespace.to_string(), key.to_string()) {
                        Ok(data) => data,
                        Err(e) => {
                            debug!("persist_load failed: {}", e);
                            Dynamic::UNIT
                        }
                    }
                }
                None => Dynamic::UNIT,
            }
        });

        // persist_list(namespace: &str) -> Array
        engine.register_fn("persist_list", |namespace: &str| -> rhai::Array {
            let bridge_tx = BRIDGE_TX.with(|b| b.borrow().clone());
            match bridge_tx {
                Some(tx) => {
                    match blocking_persist_list(&tx, namespace.to_string()) {
                        Ok(keys) => keys.into_iter().map(Dynamic::from).collect(),
                        Err(e) => {
                            debug!("persist_list failed: {}", e);
                            rhai::Array::new()
                        }
                    }
                }
                None => rhai::Array::new(),
            }
        });

        // persist_delete(namespace: &str, key: &str) -> bool
        engine.register_fn("persist_delete", |namespace: &str, key: &str| -> bool {
            let bridge_tx = BRIDGE_TX.with(|b| b.borrow().clone());
            match bridge_tx {
                Some(tx) => {
                    blocking_persist_delete(&tx, namespace.to_string(), key.to_string()).unwrap_or(false)
                }
                None => false,
            }
        });

        // persist_exists(namespace: &str, key: &str) -> bool
        engine.register_fn("persist_exists", |namespace: &str, key: &str| -> bool {
            let bridge_tx = BRIDGE_TX.with(|b| b.borrow().clone());
            match bridge_tx {
                Some(tx) => blocking_persist_exists(&tx, namespace.to_string(), key.to_string()),
                None => false,
            }
        });
    }

    /// Register shared state API functions
    fn register_shared_state_api(engine: &mut Engine) {
        // global_get(key: &str) -> Dynamic
        engine.register_fn("global_get", |key: &str| -> Dynamic {
            let bridge_tx = BRIDGE_TX.with(|b| b.borrow().clone());
            match bridge_tx {
                Some(tx) => {
                    blocking_persist_load(&tx, "_global".to_string(), key.to_string()).unwrap_or(Dynamic::UNIT)
                }
                None => Dynamic::UNIT,
            }
        });

        // global_set(key: &str, value: Dynamic)
        engine.register_fn("global_set", |key: &str, value: Dynamic| {
            let bridge_tx = BRIDGE_TX.with(|b| b.borrow().clone());
            if let Some(tx) = bridge_tx {
                let _ = blocking_persist_save(&tx, "_global".to_string(), key.to_string(), value);
            }
        });

        // global_has(key: &str) -> bool
        engine.register_fn("global_has", |key: &str| -> bool {
            let bridge_tx = BRIDGE_TX.with(|b| b.borrow().clone());
            match bridge_tx {
                Some(tx) => blocking_persist_exists(&tx, "_global".to_string(), key.to_string()),
                None => false,
            }
        });

        // namespace_get(key: &str) -> Dynamic (uses session's namespace)
        engine.register_fn("namespace_get", |key: &str| -> Dynamic {
            let namespace = CURRENT_NAMESPACE.with(|n| n.borrow().clone());
            let bridge_tx = BRIDGE_TX.with(|b| b.borrow().clone());

            match (namespace, bridge_tx) {
                (Some(ns), Some(tx)) => {
                    let ns_key = format!("{}/_shared", ns);
                    blocking_persist_load(&tx, ns_key, key.to_string()).unwrap_or(Dynamic::UNIT)
                }
                _ => Dynamic::UNIT,
            }
        });

        // namespace_set(key: &str, value: Dynamic)
        engine.register_fn("namespace_set", |key: &str, value: Dynamic| {
            let namespace = CURRENT_NAMESPACE.with(|n| n.borrow().clone());
            let bridge_tx = BRIDGE_TX.with(|b| b.borrow().clone());

            if let (Some(ns), Some(tx)) = (namespace, bridge_tx) {
                let ns_key = format!("{}/_shared", ns);
                let _ = blocking_persist_save(&tx, ns_key, key.to_string(), value);
            }
        });

        // namespace_has(key: &str) -> bool
        engine.register_fn("namespace_has", |key: &str| -> bool {
            let namespace = CURRENT_NAMESPACE.with(|n| n.borrow().clone());
            let bridge_tx = BRIDGE_TX.with(|b| b.borrow().clone());

            match (namespace, bridge_tx) {
                (Some(ns), Some(tx)) => {
                    let ns_key = format!("{}/_shared", ns);
                    blocking_persist_exists(&tx, ns_key, key.to_string())
                }
                _ => false,
            }
        });

        // shared_get(namespace: &str, key: &str) -> Dynamic (explicit namespace)
        engine.register_fn("shared_get", |namespace: &str, key: &str| -> Dynamic {
            let bridge_tx = BRIDGE_TX.with(|b| b.borrow().clone());
            match bridge_tx {
                Some(tx) => {
                    let ns_key = format!("{}/_shared", namespace);
                    blocking_persist_load(&tx, ns_key, key.to_string()).unwrap_or(Dynamic::UNIT)
                }
                None => Dynamic::UNIT,
            }
        });

        // shared_set(namespace: &str, key: &str, value: Dynamic)
        engine.register_fn("shared_set", |namespace: &str, key: &str, value: Dynamic| {
            let bridge_tx = BRIDGE_TX.with(|b| b.borrow().clone());
            if let Some(tx) = bridge_tx {
                let ns_key = format!("{}/_shared", namespace);
                let _ = blocking_persist_save(&tx, ns_key, key.to_string(), value);
            }
        });
    }

    /// Register timer API functions
    fn register_timer_api(engine: &mut Engine) {
        // Internal counter for generating temporary timer IDs
        static TEMP_TIMER_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1_000_000);

        // set_timer(delay_ms: i64, context: Dynamic) -> i64
        engine.register_fn("set_timer", |delay_ms: i64, context: Dynamic| -> i64 {
            let temp_id = TEMP_TIMER_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let delay = Duration::from_millis(delay_ms.max(0) as u64);

            SCHEDULED_TIMERS.with(|t| {
                t.borrow_mut().push((temp_id, delay, context, false));
            });

            debug!("Scheduled one-shot timer {} for {}ms", temp_id, delay_ms);
            temp_id as i64
        });

        // set_interval(interval_ms: i64, context: Dynamic) -> i64
        engine.register_fn("set_interval", |interval_ms: i64, context: Dynamic| -> i64 {
            let temp_id = TEMP_TIMER_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let interval = Duration::from_millis(interval_ms.max(100) as u64); // Min 100ms

            SCHEDULED_TIMERS.with(|t| {
                t.borrow_mut().push((temp_id, interval, context, true));
            });

            debug!("Scheduled repeating timer {} for {}ms", temp_id, interval_ms);
            temp_id as i64
        });

        // cancel_timer(timer_id: i64) -> bool
        engine.register_fn("cancel_timer", |timer_id: i64| -> bool {
            CANCELLED_TIMERS.with(|t| {
                t.borrow_mut().push(timer_id as u64);
            });
            debug!("Requested cancellation of timer {}", timer_id);
            true
        });
    }

    /// Get scheduled timers from thread-local (called after script execution)
    pub fn take_scheduled_timers() -> Vec<(u64, Duration, Dynamic, bool)> {
        SCHEDULED_TIMERS.with(|t| std::mem::take(&mut *t.borrow_mut()))
    }

    /// Get cancelled timer IDs from thread-local (called after script execution)
    pub fn take_cancelled_timers() -> Vec<u64> {
        CANCELLED_TIMERS.with(|t| std::mem::take(&mut *t.borrow_mut()))
    }
}
