//! Test-only backend that replays a canned sequence of turns. Never
//! compiles into the shipped binary — gated by `#![cfg(test)]` here AND by
//! `#[cfg(test)] pub mod scripted;` in `backends/mod.rs`.

#![cfg(test)]

use std::sync::Mutex;

use anyhow::Result;
use async_trait::async_trait;

use super::{Backend, BackendTurn};
use crate::assistant::tools::{ChatMessage, ToolDef};

pub struct ScriptedBackend {
    turns: Mutex<Vec<BackendTurn>>,
}

impl ScriptedBackend {
    /// `turns` are consumed in the order given — the first call to `send()`
    /// returns `turns[0]`, the second `turns[1]`, and so on.
    pub fn new(turns: Vec<BackendTurn>) -> Self {
        let mut turns = turns;
        turns.reverse();
        Self {
            turns: Mutex::new(turns),
        }
    }
}

#[async_trait]
impl Backend for ScriptedBackend {
    async fn send(
        &self,
        _system: &str,
        _tools: &[ToolDef],
        _history: &[ChatMessage],
    ) -> Result<BackendTurn> {
        let mut turns = self.turns.lock().expect("ScriptedBackend mutex poisoned");
        turns
            .pop()
            .ok_or_else(|| anyhow::anyhow!("ScriptedBackend exhausted — no more turns queued"))
    }
}
