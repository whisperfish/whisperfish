use std::collections::VecDeque;

use actix::prelude::*;
use dbus::message::MatchRule;
use futures::prelude::*;

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
    // attachment-id, task-id
    current_attachment: Option<(i32, i32)>,
    queue: VecDeque<i32>,
}

#[derive(actix::Message)]
#[rtype(result = "()")]
pub(super) struct TranscribeVoiceNote {
    pub message_id: i32,
}

impl actix::Handler<TranscribeVoiceNote> for super::ClientActor {
    type Result = ();

    fn handle(
        &mut self,
        TranscribeVoiceNote { message_id }: TranscribeVoiceNote,
        ctx: &mut Self::Context,
    ) -> Self::Result {
        let attachments = self
            .storage
            .as_ref()
            .unwrap()
            .fetch_attachments_for_message(message_id);
        if attachments.is_empty() {
            tracing::warn!("No attachments found for message {}", message_id);
            return;
        }
        for attachment in attachments {
            if attachment.is_voice_note {
                self.voice_note_transcription_queue
                    .queue
                    .push_back(attachment.id);
            }
        }
        self.try_queue_next_voice_note_transcription(ctx);
    }
}

impl super::ClientActor {
    fn try_queue_next_voice_note_transcription(
        &mut self,
        ctx: &mut <Self as actix::Actor>::Context,
    ) {
        if self
            .voice_note_transcription_queue
            .current_attachment
            .is_some()
        {
            return;
        }

        if let Some(attachment_id) = self.voice_note_transcription_queue.queue.pop_front() {
            let task =
                TranscriptionTask::start_transcribe(self.storage.clone().unwrap(), attachment_id);
            let addr = ctx.address();
            actix::spawn(async move {
                let task = match task.await {
                    Ok(t) => t,
                    Err(e) => {
                        // XXX: On failure, we should probably retry, if the daemon was already
                        // busy.
                        tracing::error!("Failed to start transcription task: {}", e);
                        // TODO: mark the transcription as failed
                        return;
                    }
                };
                let task_id = task.task_id;

                addr.send(TranscriptionStarted {
                    attachment_id,
                    task_id: task.task_id,
                })
                .await
                .unwrap();

                let transcription = task.wait_for_transcription().await.unwrap();
                addr.send(TranscriptionFinished {
                    task_id,
                    transcription,
                })
                .await
                .unwrap();
            });
        }
    }
}

#[derive(actix::Message)]
#[rtype(result = "()")]
struct TranscriptionStarted {
    attachment_id: i32,
    task_id: i32,
}

impl actix::Handler<TranscriptionStarted> for super::ClientActor {
    type Result = ();

    fn handle(
        &mut self,
        TranscriptionStarted {
            attachment_id,
            task_id,
        }: TranscriptionStarted,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.voice_note_transcription_queue.current_attachment = Some((attachment_id, task_id));
    }
}

#[derive(actix::Message)]
#[rtype(result = "()")]
struct TranscriptionFinished {
    task_id: i32,
    transcription: String,
}

impl actix::Handler<TranscriptionFinished> for super::ClientActor {
    type Result = ();

    fn handle(
        &mut self,
        TranscriptionFinished {
            task_id,
            transcription,
        }: TranscriptionFinished,
        ctx: &mut Self::Context,
    ) -> Self::Result {
        if let Some((attachment_id, current_task_id)) =
            self.voice_note_transcription_queue.current_attachment
        {
            tracing::info!(
                "Transcription finished for attachment {}: {}",
                attachment_id,
                transcription
            );
            if current_task_id == task_id {
                self.voice_note_transcription_queue.current_attachment = None;
                self.try_queue_next_voice_note_transcription(ctx);
            }
        }
    }
}

struct TranscriptionTask {
    attachment_id: i32,
    task_id: i32,
    storage: super::Storage,
    start_time: std::time::Instant,
}

impl TranscriptionTask {
    /// Transcribe the voice note
    async fn start_transcribe(storage: super::Storage, attachment_id: i32) -> anyhow::Result<Self> {
        let (resource, conn) = dbus_tokio::connection::new_session_sync()?;

        let attachment = storage
            .fetch_attachment(attachment_id)
            .expect("valid attachment");
        assert!(attachment.is_voice_note);

        actix::spawn(async {
            let err = resource.await;
            panic!("Lost connection to D-Bus: {}", err);
        });

        let proxy = dbus::nonblock::Proxy::new(
            "org.mkiol.Speech",
            "/",
            std::time::Duration::from_secs(5),
            conn,
        );

        // XXX we should probably first check whether the dbus daemon is busy or not.

        // Prepare the arguments
        let file_path = attachment
            .absolute_attachment_path()
            .expect("valid attachment path");
        let lang = "auto";
        let out_lang = "auto";
        let options: std::collections::HashMap<
            &str,
            dbus::arg::Variant<Box<dyn dbus::arg::RefArg>>,
        > = std::collections::HashMap::new();
        let path = std::path::Path::new(file_path.as_ref());
        assert!(path.exists(), "file exists: {:?}", path);

        let (task_id,): (i32,) = proxy
            .method_call(
                "org.mkiol.Speech",
                "SttTranscribeFile",
                (file_path.as_ref(), lang, out_lang, options),
            )
            .await?;

        if task_id < 0 {
            anyhow::bail!("Failed to start transcription task");
        }

        Ok(Self {
            attachment_id,
            task_id,
            storage,
            start_time: std::time::Instant::now(),
        })
    }

    async fn wait_for_transcription(mut self) -> anyhow::Result<String> {
        let (resource, conn) = dbus_tokio::connection::new_session_sync()?;
        actix::spawn(async {
            let err = resource.await;
            panic!("Lost connection to D-Bus: {}", err);
        });
        let proxy = dbus::nonblock::Proxy::new(
            "org.mkiol.Speech",
            "/",
            std::time::Duration::from_secs(5),
            conn.clone(),
        );
        // Keep-alive timer of 5 seconds
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
        // Signal
        let (signal, mut decoded) = conn
            .add_match(MatchRule::new_signal("org.mkiol.Speech", "SttTextDecoded"))
            .await?
            .stream();
        let token = signal.token();

        let (intermediate_signal, mut intermediate_decoded) = conn
            .add_match(MatchRule::new_signal(
                "org.mkiol.Speech",
                "SttIntermediateTextDecoded",
            ))
            .await?
            .stream();
        let intermediate_token = intermediate_signal.token();
        loop {
            let interval = interval.tick();
            let duration = self.start_time.elapsed();
            futures::select! {
                _ = interval.fuse() => {
                    // Call KeepAliveTask(task_id)
                    tracing::trace!("Sending keep-alive for task {}", self.task_id);
                    #[allow(clippy::let_unit_value)]
                    let _: () = proxy.method_call("org.mkiol.Speech", "KeepAliveTask", (self.task_id,)).await?;
                }
                signal = decoded.next().fuse() => {
                    let (message, ()) = signal.unwrap();
                    let (Some(text), Some(lang), Some(task_id) ): (Option<String>, Option<String>, Option<i32>) = message.get3() else {
                        panic!("Invalid arguments for signal");
                    };
                    tracing::info!(%lang, "Received transcription for task {} after {} seconds: {}", task_id, duration.as_secs(), text);
                    if task_id == self.task_id {
                        conn.remove_match(intermediate_token).await?;
                        conn.remove_match(token).await?;
                        self.storage.update_transcription(self.attachment_id, &text);
                        return Ok(text);
                    }
                }
                intermediate_decoded = intermediate_decoded.next().fuse() => {
                    let (message, ()) = intermediate_decoded.unwrap();
                    let (Some(text), Some(lang), Some(task_id) ): (Option<String>, Option<String>, Option<i32>) = message.get3() else {
                        panic!("Invalid arguments for signal");
                    };
                    tracing::info!(%lang, "Received partial transcription for task {} after {} seconds: {}", task_id, duration.as_secs(), text);
                    if task_id == self.task_id {
                        self.storage.update_transcription(self.attachment_id, &text);
                    }
                }
            }
        }
    }
}
