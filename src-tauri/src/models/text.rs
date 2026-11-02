//! The seam every text model is behind: one step of a conversation.
//!
//! A [`TextAdapter`] is given the conversation so far (the system prompt,
//! the turns, the tools it may call) and answers with the next assistant
//! turn: text, written to a callback as it grows, and the calls of tools
//! it wants made. Whoever holds the conversation adds that turn, and the
//! results of the calls, and asks again; nothing is kept between steps.
//! A step is called off by dropping its future.
//!
//! What an adapter can be given besides text (pictures, tools) it declares
//! itself ([`TextAdapter::can`]), and the provider kind that opens it says
//! the same by asking it (`kinds::text_can`). What goes wrong it words
//! itself, into a [`ModelError`] of one of a few classes.

use std::future::Future;
use std::pin::Pin;

use serde_json::Value;

use super::registry::Can;
use super::{ModelError, PromptImage, Question};

/// What a model is asked: one step of a conversation.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Conversation {
    pub system: String,
    /// In order, beginning with the user's, ending with the user's or with
    /// the results of the tools the model called.
    pub turns: Vec<Turn>,
    /// The tools it may call; none: it answers in text.
    pub tools: Vec<ToolSpec>,
    /// How freely to answer, 0 to 2. A hint: a model that takes none ignores it.
    pub temperature: Option<f64>,
}

/// A turn of a conversation.
#[derive(Debug, Clone, PartialEq)]
pub enum Turn {
    User(Vec<Part>),
    /// What the model answered, as a step gave it back.
    Assistant(Reply),
    /// The results of the tools the turn before called, one for each call.
    /// Nothing calls tools yet: what will is an agent's loop.
    #[cfg_attr(not(test), allow(dead_code))]
    Results(Vec<ToolResult>),
}

/// A part of what the user says.
#[derive(Debug, Clone, PartialEq)]
pub enum Part {
    /// A message to the model.
    Text(String),
    /// Text to work on (translate, rewrite, clean up…), by the system
    /// prompt's instructions: never a request to the model. Each adapter
    /// sets it apart as its model keeps to that best.
    Material(String),
    /// Only for an adapter that `can(Can::Images)`.
    Image(PromptImage),
}

/// A tool the model may call.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    /// A JSON Schema of what it takes, an object.
    pub input_schema: Value,
}

/// A call the model made of a tool.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolCall {
    /// What its result is matched to.
    pub id: String,
    pub name: String,
    /// What it was given, as the model wrote it: an object, or (where the
    /// model wrote what is not JSON) the text as a string.
    pub input: Value,
}

/// What came of a call.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolResult {
    /// The call's id.
    pub call: String,
    pub content: String,
    /// It failed, and `content` says why.
    pub failed: bool,
}

/// What a step answered: the assistant's next turn.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Reply {
    /// Trimmed, with nothing of what the model says before its answer.
    pub text: String,
    pub calls: Vec<ToolCall>,
    /// The turn as the adapter's own protocol wrote it, which it sends back
    /// as it was when the conversation goes on (Claude's thinking blocks
    /// must be); none: it is made from `text` and `calls`.
    pub replay: Option<Value>,
}

/// One step, being answered.
pub type Step<'a> = Pin<Box<dyn Future<Output = Result<Reply, ModelError>> + Send + 'a>>;

/// A model that answers in text, as an adapter of its protocol.
pub trait TextAdapter: Send + Sync + std::fmt::Debug {
    /// Whether it can be given pictures (`Images`), says what it has so
    /// far while it answers (`Streaming`), or calls tools (`ToolCalls`).
    fn can(&self, need: Can) -> bool;
    /// The model it asks for; empty: its service's default.
    fn model(&self) -> String;
    /// The next turn of `conversation`. `on_text` is given the text so far
    /// each time it grows.
    fn step<'a>(&'a self, conversation: &'a Conversation, on_text: &'a mut (dyn FnMut(&str) + Send)) -> Step<'a>;
}

impl Conversation {
    /// The conversation a one-shot question is: its history as turns of
    /// text, then the prompt (its pictures before it), as material when it
    /// has no history.
    pub fn of(question: &Question) -> Conversation {
        let mut turns: Vec<Turn> = question
            .history
            .iter()
            .flatten()
            .map(|turn| match turn.role.as_str() {
                "assistant" => Turn::Assistant(Reply { text: turn.content.clone(), ..Reply::default() }),
                _ => Turn::User(vec![Part::Text(turn.content.clone())]),
            })
            .collect();
        let mut parts: Vec<Part> = question.images.iter().cloned().map(Part::Image).collect();
        parts.push(match question.history {
            Some(_) => Part::Text(question.prompt.clone()),
            None => Part::Material(question.prompt.clone()),
        });
        turns.push(Turn::User(parts));
        Conversation { system: question.system.clone(), turns, tools: Vec::new(), temperature: question.temperature }
    }

    /// Whether the last thing the user said is text to work on.
    pub fn material(&self) -> bool {
        matches!(self.turns.last(), Some(Turn::User(parts)) if parts.iter().any(|part| matches!(part, Part::Material(_))))
    }

    /// Whether any turn has a picture.
    pub fn has_images(&self) -> bool {
        self.turns.iter().any(|turn| matches!(turn, Turn::User(parts) if parts.iter().any(|part| matches!(part, Part::Image(_)))))
    }

    /// Whether tools are offered or have been called.
    pub fn has_tools(&self) -> bool {
        !self.tools.is_empty() || self.turns.iter().any(|turn| matches!(turn, Turn::Results(_)) || matches!(turn, Turn::Assistant(reply) if !reply.calls.is_empty()))
    }
}

impl Part {
    /// Its words, should it be text.
    pub fn text(&self) -> Option<&str> {
        match self {
            Part::Text(text) | Part::Material(text) => Some(text),
            Part::Image(_) => None,
        }
    }
}
