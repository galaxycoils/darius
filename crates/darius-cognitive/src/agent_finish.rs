//! Transactional turn completion and exactly-once terminal events.
use crate::agent_loop::AgentLoop;
use crate::conversation::{Conversation, Message};
use crate::{CognitiveError, UiEvent};

impl AgentLoop {
    pub(crate) fn finish(
        &self,
        outcome: Result<String, CognitiveError>,
        msgs: Vec<Message>,
        convo: &mut Conversation,
    ) -> Result<String, CognitiveError> {
        match outcome {
            Ok(text) => match Conversation::from_messages(msgs) {
                Ok(valid) => {
                    *convo = valid;
                    self.sink.emit(UiEvent::Done);
                    Ok(text)
                }
                Err(error) => self.finish_error(error),
            },
            Err(CognitiveError::Cancelled) => {
                self.sink.emit(UiEvent::Interrupted {
                    reason: "turn cancelled".into(),
                });
                self.sink.emit(UiEvent::Done);
                Err(CognitiveError::Cancelled)
            }
            Err(error) => self.finish_error(error),
        }
    }

    fn finish_error(&self, error: CognitiveError) -> Result<String, CognitiveError> {
        self.sink.emit(UiEvent::Error {
            message: error.to_string(),
        });
        self.sink.emit(UiEvent::Done);
        Err(error)
    }
}
