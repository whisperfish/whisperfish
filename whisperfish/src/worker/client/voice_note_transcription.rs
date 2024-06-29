use std::collections::VecDeque;

impl super::ClientWorker {
    #[allow(non_snake_case)]
    #[qmeta_async::with_executor]
    pub(super) fn transcribeVoiceNote(&self, message_id: i32) {
        let transcribe_voice_note = TranscribeVoiceNote { message_id };
        let actor = self.actor.clone().unwrap();
        actix::spawn(async move { actor.send(transcribe_voice_note).await.unwrap() });
    }
}

#[derive(Debug, Default)]
pub(super) struct VoiceNoteTranscriptionQueue {
    current_message: Option<(i32, u32)>,
    queue: VecDeque<i32>,
}

#[derive(actix::Message)]
#[rtype(result = "()")]
struct TranscribeVoiceNote {
    pub message_id: i32,
}

impl actix::Handler<TranscribeVoiceNote> for super::ClientActor {
    type Result = ();

    fn handle(
        &mut self,
        TranscribeVoiceNote { message_id }: TranscribeVoiceNote,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.voice_note_transcription_queue
            .queue
            .push_back(message_id);
        self.try_queue_next_voice_note_transcription();
    }
}

impl super::ClientActor {
    fn try_queue_next_voice_note_transcription(&mut self) {
        if self
            .voice_note_transcription_queue
            .current_message
            .is_some()
        {
            return;
        }

        if let Some(_message_id) = self.voice_note_transcription_queue.queue.pop_front() {}
    }
}
