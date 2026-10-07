//! `/fast` and `/ultrafast` saves: the one transaction they make, and the
//! thread the band writes them from.
//!
//! **What a save writes**, and says, is upstream's `persistPreferenceTargets`
//! with the patch `applyFastMode` and `applyUltrafastMode` give it
//! (`session_commands.zig:781-791`, `:839-849`): the model in force under
//! `models.<provider>`, the provider, the two switches and the binding -- and
//! of its commit report only what is kept when the caller announces the change
//! (`reportUserSettingsCommit`, `:27-85`): that a layer above the profile will
//! still decide one of the fields a fresh session reads
//! (`appendShadowedUserSources`, `:170-186`), or that the write failed
//! (`reportUserSettingsFailure`, `:110-133`).
//!
//! **When it is said.** Upstream persists *before* it announces
//! (`applyFastMode`, `:781-797`; `applyUltrafastMode`, `:839-859`), so that
//! report is the line above `* fast: on`. It persists on the thread that ran
//! the command; here that is the UI thread, which must not wait on a settings
//! write's two `fsync`s, and the runtime takes a speed change only between
//! turns, where upstream says `* fast: on` while an answer streams. So the
//! band hands each save to one writer thread of its own -- **one**, so two in
//! a row land in the order they were decided, as `/sound`'s do
//! ([`super::sound`]) -- and holds the announcement until the write has
//! answered, with everything after it behind it (`Mark::Saving` in
//! [`super::shell`]).
//!
//! The thread is started with every owned signal blocked
//! ([`super::signals::spawn_unsignalled`]) and joined when the band is dropped,
//! so a `/fast` and an immediate `/quit` still leave the switch saved.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::JoinHandle;
use std::time::Duration;

use super::notice::Notice;
use crate::config::Sources;
use crate::provider::ProviderId;

/// The name the writer thread is given, for a debugger or a panic message.
const THREAD_NAME: &str = "xfx-speed-settings";

/// One save, as the band decided it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Order {
    /// The provider and the model in force when it was decided: what
    /// upstream writes (`provider_runtime.provider(app)`,
    /// `provider_runtime.model(app)`).
    pub(crate) provider: ProviderId,
    pub(crate) model: String,
    pub(crate) fast: bool,
    pub(crate) ultrafast: bool,
    /// Upstream's label for the write (`fast` or `ultrafast`), which is what
    /// the report says it under.
    pub(crate) topic: &'static str,
}

/// How one save went: what to say above the announcement, if anything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Saved {
    pub(crate) id: u64,
    pub(crate) said: Option<Notice>,
}

/// What starting a save came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Started {
    /// There is no settings file, so nothing is written and nothing waits.
    Nowhere,
    /// Handed to the writer; its answer comes back as [`Saved`] with this id.
    Writing(u64),
    /// The writer could not be reached: the failure, to be said at once,
    /// ahead of the announcement, as upstream's failure report is.
    Failed(Notice),
}

/// The writer thread, once there is one.
#[derive(Debug)]
struct Writer {
    orders: Sender<(u64, Order)>,
    thread: JoinHandle<()>,
}

/// The band's speed saves.
#[derive(Debug)]
pub(crate) struct Saves {
    settings: Option<PathBuf>,
    /// The layers a fresh session's fields come from, as the launch ranked
    /// them. A `/setup` switch re-reads the profile, but the layers that can
    /// stand above it -- the environment and this workspace's entry -- are not
    /// something a session changes.
    sources: Sources,
    /// Started on the first save: a session that never changes speed has no
    /// thread for it.
    writer: Option<Writer>,
    done: Sender<Saved>,
    saved: Receiver<Saved>,
    next: u64,
    /// The orders a set built by a test was given, in order; such a set
    /// starts no thread, so what it answers is only what the test delivers.
    #[cfg(test)]
    by_hand: Option<Vec<(u64, Order)>>,
}

impl Saves {
    pub(crate) fn new(settings: Option<PathBuf>, sources: Sources) -> Self {
        let (done, saved) = mpsc::channel();
        Self {
            settings,
            sources,
            writer: None,
            done,
            saved,
            next: 1,
            #[cfg(test)]
            by_hand: None,
        }
    }

    /// A set that writes nothing: it keeps each order, and answers only with
    /// what [`Self::deliver`] hands it.
    #[cfg(test)]
    pub(crate) fn by_hand() -> Self {
        let mut saves = Self::new(None, Sources::default());
        saves.by_hand = Some(Vec::new());
        saves
    }

    /// The orders a [`Self::by_hand`] set was given, oldest first.
    #[cfg(test)]
    pub(crate) fn orders(&self) -> &[(u64, Order)] {
        self.by_hand.as_deref().unwrap_or(&[])
    }

    /// Stands in for the writer, for the cases about what the band does with
    /// an answer.
    #[cfg(test)]
    pub(crate) fn deliver(&self, id: u64, said: Option<Notice>) {
        let _ = self.done.send(Saved { id, said });
    }

    /// Starts writing `order`, and returns without waiting.
    pub(crate) fn start(&mut self, order: Order) -> Started {
        let id = self.next;
        #[cfg(test)]
        if let Some(orders) = self.by_hand.as_mut() {
            self.next += 1;
            orders.push((id, order));
            return Started::Writing(id);
        }
        let Some(settings) = self.settings.clone() else {
            return Started::Nowhere;
        };
        if self.writer.is_none() {
            match spawn_writer(settings, self.sources, self.done.clone()) {
                Ok(writer) => self.writer = Some(writer),
                Err(reason) => return Started::Failed(not_saved(order.topic, &reason)),
            }
        }
        let writer = self.writer.as_ref().expect("the writer was just started");
        let topic = order.topic;
        if writer.orders.send((id, order)).is_err() {
            // The thread is gone, which only a panic in it does: it is not
            // started again, because whatever killed it would kill the next.
            return Started::Failed(not_saved(topic, "the settings writer stopped"));
        }
        self.next += 1;
        Started::Writing(id)
    }

    /// How one save that has finished went, or `None` when none has since the
    /// last call. In the order they were decided, because one thread wrote
    /// them.
    pub(crate) fn saved(&self) -> Option<Saved> {
        self.saved.try_recv().ok()
    }

    /// [`Self::saved`], waiting up to `limit` for one: the exit's, which has
    /// announcements to write and nothing left to keep responsive.
    pub(crate) fn wait(&self, limit: Duration) -> Option<Saved> {
        self.saved.recv_timeout(limit).ok()
    }
}

impl Drop for Saves {
    /// Lets a save in flight land before the session is gone, as
    /// [`super::sound::Sound`]'s drop does and for its reasons.
    fn drop(&mut self) {
        if let Some(Writer { orders, thread }) = self.writer.take() {
            drop(orders);
            let _ = thread.join();
        }
    }
}

/// Starts the one thread that writes this session's speed saves, in order.
fn spawn_writer(
    settings: PathBuf,
    sources: Sources,
    done: Sender<Saved>,
) -> Result<Writer, String> {
    let (orders, received) = mpsc::channel::<(u64, Order)>();
    let thread = super::signals::spawn_unsignalled(THREAD_NAME, move || {
        for (id, order) in received {
            let said = save(
                &settings,
                order.provider,
                &order.model,
                order.fast,
                order.ultrafast,
                &sources,
                order.topic,
            );
            // Nobody left to tell is a session that has ended, and the file
            // was written for it all the same.
            let _ = done.send(Saved { id, said });
        }
    })
    .map_err(|err| format!("no thread to save with: {err}"))?;
    Ok(Writer { orders, thread })
}

/// The speed switches as a settings write carries them, with the binding
/// upstream writes beside every speed change (`fast_mode_model_bound`).
pub(crate) fn switches(
    fast: bool,
    ultrafast: bool,
) -> [(&'static [&'static str], serde_json::Value); 3] {
    [
        (
            &[crate::config::FAST_MODE_KEY],
            serde_json::Value::Bool(fast),
        ),
        (
            &[crate::config::ULTRAFAST_MODE_KEY],
            serde_json::Value::Bool(ultrafast),
        ),
        (
            &[crate::config::FAST_MODE_MODEL_BOUND_KEY],
            serde_json::Value::Bool(true),
        ),
    ]
}

/// What a write of the model and the speed switches beside it -- and the
/// effort, when `effort` says one was written -- says when a layer above the
/// profile will still decide any of them for a fresh session: each, in
/// upstream's order (`appendShadowedUserSources`, `session_commands.zig:170-186`
/// -- `model`, `effort`, `fast_mode`, `ultrafast_mode`).
pub(crate) fn shadowed(sources: &Sources, effort: bool) -> Option<String> {
    let mut fields = vec![("model", sources.model)];
    if effort {
        fields.push(("effort", sources.effort));
    }
    fields.push(("fast_mode", sources.fast_mode));
    fields.push(("ultrafast_mode", sources.ultrafast_mode));
    crate::provider::profile::saved_but_any_shadowed(&fields)
}

/// Saves the speed switches with what upstream writes beside them, as one
/// transaction under the settings lock, and says what upstream's report keeps
/// once the change has been announced: the layer above the profile that still
/// decides a field, or the failure. `None` is the ordinary case.
pub(crate) fn save(
    path: &Path,
    provider: ProviderId,
    model: &str,
    fast: bool,
    ultrafast: bool,
    sources: &Sources,
    topic: &'static str,
) -> Option<Notice> {
    let model_key = [crate::config::MODELS_KEY, provider.label()];
    let provider_key = [crate::config::PROVIDER_KEY];
    let mut preferences: Vec<(&[&str], serde_json::Value)> = vec![
        (&model_key, serde_json::Value::String(model.to_string())),
        (
            &provider_key,
            serde_json::Value::String(provider.label().to_string()),
        ),
    ];
    preferences.extend(switches(fast, ultrafast));
    match crate::provider::profile::write_preferences(path, &preferences) {
        Ok(()) => shadowed(sources, false).map(|said| Notice::neutral(topic, said)),
        Err(err) => Some(not_saved(topic, &err.to_string())),
    }
}

/// Upstream's failure report for a change already in force
/// (`reportUserSettingsFailure`, `session_commands.zig:110-133`).
fn not_saved(topic: &'static str, reason: &str) -> Notice {
    Notice::error(
        topic,
        format!("active for this process but not saved to user settings ({reason})"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn settings(home: &Path) -> serde_json::Value {
        let path = home.join(".xfx").join("settings.json");
        serde_json::from_str(&std::fs::read_to_string(path).expect("written")).expect("json")
    }

    fn order(fast: bool) -> Order {
        Order {
            provider: ProviderId::Gateway,
            model: "anthropic/claude-opus-5".to_string(),
            fast,
            ultrafast: false,
            topic: "fast",
        }
    }

    /// Waits for one save to finish, or fails the case after a minute.
    fn answered(saves: &Saves) -> Saved {
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            if let Some(saved) = saves.wait(Duration::from_millis(50)) {
                return saved;
            }
            assert!(Instant::now() < deadline, "the save never finished");
        }
    }

    #[test]
    fn saves_land_in_the_order_they_were_decided_and_each_says_how_it_went() {
        let home = tempfile::tempdir().expect("a home");
        let path = home.path().join(".xfx").join("settings.json");
        let mut saves = Saves::new(Some(path), Sources::default());
        let mut ids = Vec::new();
        for fast in [true, false, true] {
            let Started::Writing(id) = saves.start(order(fast)) else {
                panic!("not started");
            };
            ids.push(id);
        }
        let said: Vec<Saved> = ids.iter().map(|_| answered(&saves)).collect();
        assert_eq!(
            said,
            ids.iter()
                .map(|&id| Saved { id, said: None })
                .collect::<Vec<_>>()
        );
        assert_eq!(
            settings(home.path()),
            serde_json::json!({"models": {"gateway": "anthropic/claude-opus-5"},
                               "provider": "gateway", "fast_mode": true,
                               "ultrafast_mode": false, "fast_mode_model_bound": true})
        );
    }

    #[test]
    fn a_save_says_the_layer_above_it_or_its_failure_and_with_no_file_starts_nothing() {
        let home = tempfile::tempdir().expect("a home");
        let path = home.path().join(".xfx").join("settings.json");
        let sources = Sources {
            model: crate::config::SettingSource::ProcessOverride,
            ..Sources::default()
        };
        assert_eq!(
            save(
                &path,
                ProviderId::Gateway,
                "pinned/model",
                true,
                false,
                &sources,
                "fast"
            ),
            Some(Notice::neutral(
                "fast",
                "saved to user settings (scope=user); fresh sessions here use \
                 higher-precedence model=process_override"
            ))
        );
        // A profile xfx could not read is never written over.
        std::fs::write(&path, "{ not json").expect("seed");
        let failed = save(
            &path,
            ProviderId::Gateway,
            "m",
            true,
            false,
            &sources,
            "ultrafast",
        )
        .expect("a failure is said");
        assert_eq!(
            (failed.tone, failed.topic.as_str()),
            (super::super::notice::Tone::Error, "ultrafast")
        );
        assert!(
            failed
                .body
                .starts_with("active for this process but not saved to user settings ("),
            "{failed:?}"
        );
        assert_eq!(std::fs::read_to_string(&path).expect("read"), "{ not json");
        assert_eq!(
            Saves::new(None, Sources::default()).start(order(true)),
            Started::Nowhere
        );
    }
}
