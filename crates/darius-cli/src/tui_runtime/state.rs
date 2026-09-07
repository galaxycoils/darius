//! Actor lifecycle state and running turn handle.
use super::{ChannelRunControl, view::SessionView};
use crate::runtime::SessionRuntime;
use darius_core::runtime_protocol::TurnId;
use std::sync::Arc;

pub(crate) enum State {
    Idle(Box<SessionRuntime>),
    Running(Running),
    Stopped,
}

pub(crate) struct Running {
    pub turn_id: TurnId,
    pub join: tokio::task::JoinHandle<super::turn::TurnResult>,
    pub control: Arc<ChannelRunControl>,
    pub view: SessionView,
}
