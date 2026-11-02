//! A text adapter for tests that answers as it is told: each step is
//! answered with the next of its script, its text written a word at a
//! time, and every conversation it was asked is kept to be looked at.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use super::registry::Can;
use super::text::{Conversation, Reply, Step, TextAdapter};
use super::ModelError;

#[derive(Debug, Clone)]
pub struct Scripted {
    script: Arc<Mutex<VecDeque<Result<Reply, ModelError>>>>,
    asked: Arc<Mutex<Vec<Conversation>>>,
    can: Vec<Can>,
}

impl Scripted {
    /// Answers each step with the next of `script`; it can do what `can` says.
    pub fn new(script: Vec<Result<Reply, ModelError>>, can: &[Can]) -> Scripted {
        Scripted { script: Arc::new(Mutex::new(script.into())), asked: Arc::default(), can: can.to_vec() }
    }

    /// The conversations it was asked, in order.
    pub fn asked(&self) -> Vec<Conversation> {
        self.asked.lock().unwrap().clone()
    }
}

impl TextAdapter for Scripted {
    fn can(&self, need: Can) -> bool {
        self.can.contains(&need)
    }

    fn model(&self) -> String {
        "scripted".into()
    }

    fn step<'a>(&'a self, conversation: &'a Conversation, on_text: &'a mut (dyn FnMut(&str) + Send)) -> Step<'a> {
        Box::pin(async move {
            self.asked.lock().unwrap().push(conversation.clone());
            // What it says it cannot be given, it is not.
            if conversation.has_images() && !self.can(Can::Images) {
                return Err(ModelError::Failed("The script sees no pictures.".into()));
            }
            if conversation.has_tools() && !self.can(Can::ToolCalls) {
                return Err(ModelError::Failed("The script calls no tools.".into()));
            }
            let next = self.script.lock().unwrap().pop_front();
            let reply = next.unwrap_or_else(|| Err(ModelError::Failed("The script has no more answers.".into())))?;
            if self.can(Can::Streaming) {
                let mut so_far = String::new();
                for word in reply.text.split_inclusive(' ') {
                    so_far.push_str(word);
                    on_text(&so_far);
                    // Somewhere for it to be called off.
                    tokio::task::yield_now().await;
                }
            } else if !reply.text.is_empty() {
                on_text(&reply.text);
            }
            Ok(reply)
        })
    }
}
