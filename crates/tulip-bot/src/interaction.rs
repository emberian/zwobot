//! Interaction handling for buttons, select menus, modals

use crate::context::InteractionContext;
use crate::error::Result;
use crate::response::Response;
use crate::types::{InteractionEvent, Message, User};
use futures::future::BoxFuture;
use std::collections::HashMap;

/// Types of interactions
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InteractionType {
    ButtonClick,
    SelectMenu,
    ModalSubmit,
    Freeform,
}

impl InteractionType {
    /// Parse from the raw interaction type string
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "button_click" => Some(Self::ButtonClick),
            "select_menu" => Some(Self::SelectMenu),
            "modal_submit" => Some(Self::ModalSubmit),
            "freeform" => Some(Self::Freeform),
            _ => None,
        }
    }
}

/// Data associated with an interaction
#[derive(Debug, Clone)]
pub enum InteractionData {
    /// No additional data (button clicks)
    None,
    /// Selected values (select menus)
    SelectValues(Vec<String>),
    /// Modal field values
    ModalFields(HashMap<String, String>),
    /// Freeform data
    Freeform(serde_json::Value),
}

impl InteractionData {
    /// Parse from raw interaction data
    pub fn from_raw(interaction_type: InteractionType, raw: &serde_json::Value) -> Self {
        match interaction_type {
            InteractionType::ButtonClick => Self::None,
            InteractionType::SelectMenu => {
                if let Some(values) = raw.get("values").and_then(|v| v.as_array()) {
                    let strings: Vec<String> = values
                        .iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect();
                    Self::SelectValues(strings)
                } else {
                    Self::None
                }
            }
            InteractionType::ModalSubmit => {
                if let Some(fields) = raw.get("fields").and_then(|v| v.as_object()) {
                    let map: HashMap<String, String> = fields
                        .iter()
                        .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                        .collect();
                    Self::ModalFields(map)
                } else {
                    Self::None
                }
            }
            InteractionType::Freeform => Self::Freeform(raw.clone()),
        }
    }

    /// Get selected values (for select menus)
    pub fn values(&self) -> Option<&[String]> {
        match self {
            Self::SelectValues(v) => Some(v),
            _ => None,
        }
    }

    /// Get modal field values
    pub fn fields(&self) -> Option<&HashMap<String, String>> {
        match self {
            Self::ModalFields(f) => Some(f),
            _ => None,
        }
    }

    /// Get a specific modal field
    pub fn field(&self, name: &str) -> Option<&str> {
        self.fields().and_then(|f| f.get(name).map(|s| s.as_str()))
    }
}

/// A parsed interaction
#[derive(Debug, Clone)]
pub struct Interaction {
    pub id: String,
    pub interaction_type: InteractionType,
    pub custom_id: String,
    pub data: InteractionData,
    pub message: Message,
    pub user: User,
}

impl Interaction {
    /// Parse from a raw interaction event
    pub fn from_event(event: InteractionEvent) -> Option<Self> {
        let interaction_type = InteractionType::from_str(&event.interaction_type)?;
        let data = InteractionData::from_raw(interaction_type, &event.data);

        Some(Self {
            id: event.interaction_id,
            interaction_type,
            custom_id: event.custom_id,
            data,
            message: event.message,
            user: event.user,
        })
    }
}

/// Trait for handling interactions
///
/// # Example
///
/// ```ignore
/// struct ApprovalHandler;
///
/// impl InteractionHandler<MyData> for ApprovalHandler {
///     fn matches(&self, custom_id: &str) -> bool {
///         custom_id.starts_with("approve_") || custom_id.starts_with("reject_")
///     }
///
///     fn handle<'a>(&'a self, ctx: InteractionContext<'a, MyData>) -> BoxFuture<'a, Result<Response>> {
///         Box::pin(async move {
///             if ctx.interaction.custom_id.starts_with("approve_") {
///                 Ok(Response::message("Approved!"))
///             } else {
///                 Ok(Response::ephemeral("Rejected."))
///             }
///         })
///     }
/// }
/// ```
pub trait InteractionHandler<D>: Send + Sync {
    /// Return true if this handler should handle the given custom_id
    fn matches(&self, custom_id: &str) -> bool;

    /// Handle the interaction
    fn handle<'a>(&'a self, ctx: InteractionContext<'a, D>) -> BoxFuture<'a, Result<Response>>;
}

/// Type alias for boxed interaction handler function
type BoxedInteractionFn<D> = Box<
    dyn for<'a> Fn(InteractionContext<'a, D>) -> BoxFuture<'a, Result<Response>> + Send + Sync,
>;

/// A simple function-based interaction handler with prefix matching
pub struct PrefixHandler<D> {
    prefix: String,
    handler: BoxedInteractionFn<D>,
}

impl<D: Send + Sync + 'static> PrefixHandler<D> {
    /// Create a new prefix handler
    pub fn new<F, Fut>(prefix: impl Into<String>, handler: F) -> Self
    where
        F: Fn(InteractionContext<'_, D>) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = Result<Response>> + Send + 'static,
    {
        Self {
            prefix: prefix.into(),
            handler: Box::new(move |ctx| Box::pin(handler(ctx))),
        }
    }
}

impl<D: Send + Sync + 'static> InteractionHandler<D> for PrefixHandler<D> {
    fn matches(&self, custom_id: &str) -> bool {
        custom_id.starts_with(&self.prefix)
    }

    fn handle<'a>(&'a self, ctx: InteractionContext<'a, D>) -> BoxFuture<'a, Result<Response>> {
        (self.handler)(ctx)
    }
}

/// Create a prefix-based interaction handler from a function
pub fn on_interaction<D, F, Fut>(prefix: impl Into<String>, handler: F) -> PrefixHandler<D>
where
    D: Send + Sync + 'static,
    F: Fn(InteractionContext<'_, D>) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = Result<Response>> + Send + 'static,
{
    PrefixHandler::new(prefix, handler)
}
