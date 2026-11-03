//! The skills the band offers, discovered on a worker thread and held between
//! refreshes.
//!
//! Upstream's runtime keeps one list (`app.skills.items`) that the `$` menu,
//! the slash popup, the inline completion and `/skills` all read, and never
//! discovers on the thread that paints: the first list is asked for at launch
//! so the first frame does not wait for it (`beginStartupSkillsLoad`,
//! `vercel-labs/fx@c1db9191 src/core/app/app_bootstrap_runtime.zig:278-289`),
//! a refresh runs on a catalog worker (`CatalogRefreshTask`, started by
//! `requestRefresh`, `src/core/skills/skill_runtime.zig:2181-2240`), and the
//! loop adopts what it found when it lands (`pollRefresh`, `:2249-2310`). A
//! refresh asked for while one runs is started once that one lands, and the
//! generation it was given is answered only then (`refresh_pending`,
//! `startPendingRefresh`, `generationStatus`, `:2312-2347`).
//!
//! What waits for a refresh is **one** thing, and a newer one takes its place
//! (`queueRefreshAction`, `:2353-2383`; `takeReadyRefreshAction`,
//! `:2386-2397`): the launch's diagnostics, `/skills list` or `show`, or the
//! sentence a `create`, `remove` or install says once what it changed is in
//! the list.
//!
//! **No loading state.** Upstream's menus read whatever list is held: before
//! the first one lands it is empty, a `$` finds nothing to show, and a
//! `/skills` waits for the refresh it asked for and opens when it lands. The
//! only empty rows its menu has are `No skills available.` and
//! `No skills found.` (`composeEmptyRow`,
//! `src/ui/footer/skills_menu_presentation.zig:411-431`); there is no row that
//! says the list is loading, so there is none here.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, TryRecvError};

use crate::skills::{Discovery, Skill};

/// One discovery running on its own thread, and the generation it answers.
#[derive(Debug)]
struct Job {
    generation: u64,
    found: Receiver<Discovery>,
}

/// What waits for a refresh to land (`RefreshAction`,
/// `skill_runtime.zig:1895-1910`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Waiting {
    /// The launch's list: what discovery skipped is said once it lands.
    Startup,
    /// `/skills` or `/skills list`.
    List,
    /// `/skills show <name>`.
    Show(String),
    /// What a command that changed the managed root says, once the change is
    /// in the list.
    Notice(String),
}

/// Where one generation's refresh is (`GenerationStatus`, `:2331-2342`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Status {
    Pending,
    Current,
    Failed,
}

/// The list the band reads, and the refresh that will replace it.
#[derive(Debug)]
pub(crate) struct HeldSkills {
    workspace: PathBuf,
    home: Option<PathBuf>,
    found: Discovery,
    job: Option<Job>,
    /// The generation a refresh asked for while another ran.
    again: Option<u64>,
    /// The last generation handed out.
    next: u64,
    /// Every generation up to this one is answered by the list held.
    fresh_through: u64,
    /// The last generation whose discovery came to nothing.
    failed: Option<u64>,
    /// What waits for which generation.
    waiting: Option<(u64, Waiting)>,
    /// A list was adopted that the band has not taken in yet.
    landed: bool,
}

impl HeldSkills {
    /// An empty list, with the first refresh already running and the
    /// launch's notice waiting on it.
    pub(crate) fn new(workspace: &Path, home: Option<&Path>) -> Self {
        let mut held = Self {
            // Canonical, so the paths a menu binds are the paths the turn's
            // own discovery finds (`crate::skills::discovery::discover` reads
            // the scope's primary directory, which is canonical).
            workspace: std::fs::canonicalize(workspace).unwrap_or_else(|_| workspace.to_path_buf()),
            home: home.map(Path::to_path_buf),
            found: Discovery::default(),
            job: None,
            again: None,
            next: 0,
            fresh_through: 0,
            failed: None,
            waiting: None,
            landed: false,
        };
        let generation = held.request();
        held.wait(generation, Waiting::Startup);
        held
    }

    /// The skills held now, in discovery order.
    pub(crate) fn skills(&self) -> &[Skill] {
        &self.found.skills
    }

    /// The discovery held now, with what it skipped.
    pub(crate) fn found(&self) -> &Discovery {
        &self.found
    }

    /// Asks for a fresh list and says which generation will hold it
    /// (`requestRefresh`): one starts at once unless another is running, in
    /// which case it starts when that one lands.
    pub(crate) fn request(&mut self) -> u64 {
        if self.job.is_some() {
            if let Some(pending) = self.again {
                return pending;
            }
            self.next += 1;
            self.again = Some(self.next);
            return self.next;
        }
        self.next += 1;
        self.start(self.next);
        self.next
    }

    /// Puts `what` to wait for `generation`, in place of whatever waited
    /// before (`queueRefreshAction`'s supersession).
    pub(crate) fn wait(&mut self, generation: u64, what: Waiting) {
        self.waiting = Some((generation, what));
    }

    fn start(&mut self, generation: u64) {
        let (sender, found) = mpsc::channel();
        let workspace = self.workspace.clone();
        let home = self.home.clone();
        let spawned = std::thread::Builder::new()
            .name("xfx-skills".to_string())
            .spawn(move || {
                let _ = sender.send(crate::skills::discovery::discover(
                    &workspace,
                    home.as_deref(),
                ));
            });
        match spawned {
            Ok(_) => self.job = Some(Job { generation, found }),
            // A refresh that could not start fails its generation, and the
            // list held stands (`failed_refresh_generation`).
            Err(_) => self.failed = Some(generation),
        }
    }

    /// Takes in a refresh that has landed, starting the one asked for behind
    /// it (`pollRefresh`), and says whether a list was adopted that the band
    /// has not taken in -- this time or an earlier one ([`Self::take_landed`]).
    pub(crate) fn poll(&mut self) -> bool {
        self.adopt();
        std::mem::take(&mut self.landed)
    }

    fn adopt(&mut self) {
        let Some(job) = self.job.as_ref() else {
            return;
        };
        let landed = match job.found.try_recv() {
            Ok(found) => Some(found),
            Err(TryRecvError::Empty) => return,
            // The thread died without an answer: the list held stands, and
            // the generation it was asked for failed.
            Err(TryRecvError::Disconnected) => None,
        };
        let generation = job.generation;
        self.job = None;
        match landed {
            Some(found) => {
                self.found = found;
                self.fresh_through = self.fresh_through.max(generation);
                self.landed = true;
            }
            None => self.failed = Some(generation),
        }
        if let Some(pending) = self.again.take() {
            self.start(pending);
        }
    }

    /// Where `generation` is (`generationStatus`).
    pub(crate) fn status(&self, generation: u64) -> Status {
        if self.fresh_through >= generation {
            Status::Current
        } else if self.failed == Some(generation) {
            Status::Failed
        } else {
            Status::Pending
        }
    }

    /// What waited for a generation that is no longer pending, and whether
    /// its refresh succeeded (`takeReadyRefreshAction`).
    pub(crate) fn take_ready(&mut self) -> Option<(Waiting, bool)> {
        let (generation, _) = self.waiting.as_ref()?;
        let status = self.status(*generation);
        if status == Status::Pending {
            return None;
        }
        let (_, what) = self.waiting.take()?;
        Some((what, status == Status::Current))
    }

    /// Waits for every refresh asked for so far to land, for a case that has
    /// to read the list without a loop to tick it.
    ///
    /// What landed is still the band's to take in at its next [`Self::poll`].
    #[cfg(test)]
    pub(crate) fn settle_now(&mut self) {
        while self.job.is_some() {
            self.adopt();
            if self.job.is_some() {
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn skill(dir: &Path, name: &str) {
        let at = dir.join(".xfx/skills").join(name);
        std::fs::create_dir_all(&at).unwrap();
        std::fs::write(at.join("SKILL.md"), format!("---\nname: {name}\n---\n")).unwrap();
    }

    #[test]
    fn the_list_lands_off_the_thread_that_asked_and_a_second_ask_waits_its_turn() {
        let base = tempfile::tempdir().unwrap();
        let work = base.path().canonicalize().unwrap();
        skill(&work, "one");
        let mut held = HeldSkills::new(&work, None);
        // Nothing is held until the first refresh lands, and the launch's
        // notice waits for it.
        assert_eq!(held.status(1), Status::Pending);
        assert_eq!(held.take_ready(), None);
        held.settle_now();
        assert!(
            held.poll(),
            "the list that landed was not the band's to take in"
        );
        assert!(!held.poll());
        assert_eq!(held.take_ready(), Some((Waiting::Startup, true)));
        assert_eq!(
            held.skills()
                .iter()
                .map(|s| s.name.as_str())
                .collect::<Vec<_>>(),
            ["one"]
        );

        skill(&work, "two");
        let first = held.request();
        // Asked again while the first runs: one more generation, not two.
        let second = held.request();
        let third = held.request();
        assert_eq!((first, second, third), (2, 3, 3));
        held.wait(second, Waiting::List);
        // A newer wait takes the older one's place.
        held.wait(third, Waiting::Show("two".to_string()));
        held.settle_now();
        assert_eq!(held.status(3), Status::Current);
        assert_eq!(
            held.take_ready(),
            Some((Waiting::Show("two".to_string()), true))
        );
        assert_eq!(held.take_ready(), None);
        assert_eq!(held.skills().len(), 2);
    }
}
