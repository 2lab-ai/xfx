//! What makes an affirmative answer to a permission question answerable.
//!
//! A `1` at a question is a grant, and a grant given against a screen the user
//! could not read is a grant nobody made. So this module holds the one fact the
//! shell's gate asks for: **did a frame that really disclosed this request's
//! target, its three controls and what "always" would grant reach the terminal,
//! and is that frame still what the terminal is showing.**
//!
//! Three properties, and each is here because leaving it out is a way to grant
//! against nothing:
//!
//! * **Disclosure is decided at composition, from the source text and the rows
//!   it was allotted** -- never by looking at a string that has already been
//!   cut. Every truncation on both surfaces is silent: `super::approval`'s
//!   `fitted` appends an ellipsis to whatever survived its allotment, and
//!   `super::approval_screen`'s `compose` truncates the heading a second time
//!   into whatever the choices left. A rule written against the cut string would
//!   read `2. Yes, and don't ask again for th` as a disclosed control, and that
//!   prefix is exactly where the difference between one call and the rest of the
//!   session is written.
//! * **A successful write is not a receipt.** A `SIGWINCH` that lands inside the
//!   write is drained one tick later (`super::event_loop`'s `collect_facts`), so
//!   the frame is only proven once the loop has come round and found no resize
//!   pending. Until then it is *provisional*.
//! * **A repaint that wrote nothing may keep a receipt and may never mint one.**
//!   `super::frame::Commit::NoChange` says the screen already holds these bytes,
//!   which is proof only if something proved them before -- for this same
//!   request, at this same size, in this same layout.
//!
//! Pure state: no I/O, no `Shell`, no `Band`. What calls it is
//! `super::shell::Shell`, and where each call sits in a tick is
//! `super::event_loop`'s.

/// Which question is being asked, for as long as it is being asked.
///
/// Minted once per `super::approval::TuiPrompter::request` call and carried on
/// the event and the answer, so that a keystroke left over from a question that
/// has already gone cannot be read as an answer to the one that is up. **A TUI
/// fact**: `crate::permission` neither mints nor sees one, because what an id
/// distinguishes is two screens rather than two policies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ApprovalId(pub u64);

/// Which plane a composition was for.
///
/// Part of a screen's identity rather than a detail of it: the band's panel and
/// the review plane lay the same question out differently, so a receipt earned
/// on one is not a receipt for the other.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Surface {
    /// The band's own panel, with the document still above it.
    Inline,
    /// A screen of its own, for a change the band's summary cannot show.
    Alternate,
}

/// What the terminal did with a frame's bytes.
///
/// The two answers `super::frame::Band::commit` gives, named here so that this
/// module does not depend on the frame layer to state its own contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    /// Bytes were written and flushed.
    Painted,
    /// The screen already held them, so nothing was written.
    Unchanged,
}

/// What one composition really put on the screen, decided by the composition
/// itself.
///
/// Every field is a claim about the **source** text and its allotment, not about
/// a string that has already been cut. A composition that cannot answer `true`
/// to the first three is a composition on which no affirmative can be given at
/// this size, however cleanly its bytes reach the terminal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Disclosure {
    /// Absolute terminal row, one-based, of the last control row.
    ///
    /// Absolute rather than panel-local, because the band puts its activity row
    /// above the panel when the geometry has one: a local index is not a row of
    /// anybody's screen, and the guard in [`Intent::capture`] is about the
    /// screen.
    pub last_control_row: u16,
    /// The three control labels reached the screen with nothing cut off.
    pub controls_whole: bool,
    /// The always-scope line reached the screen with nothing cut off.
    pub scope_whole: bool,
    /// Tool and target were rendered in full: no row dropped, no ellipsis.
    pub subject_whole: bool,
    /// The change, or the notice standing in for it, is on the screen now.
    ///
    /// Recorded rather than required: the clause is "changed content **or its
    /// notice**, visible now **or previously observed** for that screen state",
    /// and the second half is [`Readiness`]'s `seen`.
    pub change_visible: bool,
}

/// One composition that could disclose its question, and the screen it was for.
///
/// The **identity of a screen** as far as readiness is concerned: which
/// question, on which plane, at which size, laid out how. Deliberately not the
/// cursor -- a marker that moved repaints and re-earns a receipt through
/// [`Outcome::Painted`], and treating that as a different screen would revoke a
/// grant for a keystroke that disclosed nothing new.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Intent {
    id: ApprovalId,
    surface: Surface,
    cols: u16,
    rows: u16,
    disclosure: Disclosure,
}

impl Intent {
    /// `None` when this composition could not disclose the question, whatever
    /// the terminal then does with the bytes.
    ///
    /// The row is **one-based**, so a zero is not a row on any screen -- it is an
    /// unset field, and an unset field must never read as a disclosed control.
    pub(crate) fn capture(
        id: ApprovalId,
        surface: Surface,
        cols: u16,
        rows: u16,
        disclosure: Disclosure,
    ) -> Option<Intent> {
        let disclosed = disclosure.controls_whole
            && disclosure.scope_whole
            && disclosure.subject_whole
            && cols > 0
            && rows > 0
            && disclosure.last_control_row > 0
            && disclosure.last_control_row <= rows;
        disclosed.then_some(Intent {
            id,
            surface,
            cols,
            rows,
            disclosure,
        })
    }
}

/// The receipt state machine: what has been composed, what has been written,
/// and what has been proven.
#[derive(Debug, Default)]
pub(crate) struct Readiness {
    /// The composition the frame about to be written is of.
    pending: Option<Intent>,
    /// A composition whose bytes the terminal took, not yet checked against the
    /// screen it was for.
    provisional: Option<Intent>,
    /// A composition that was written **and** reconciled: the proof.
    receipt: Option<Intent>,
    /// The request, and the size, at which the change itself was on the screen.
    ///
    /// What lets a user scroll the change out of the viewport and still answer:
    /// they read it, at this size, for this question. Dropped with the receipt
    /// whenever a resize is pending, because what it is about is a screen state
    /// that has since reflowed.
    seen: Option<(ApprovalId, u16, u16)>,
    /// Whether the last composition disclosed nothing.
    ///
    /// So the shell can say *why* an affirmative is impossible rather than
    /// gating in silence.
    undisclosed: bool,
}

impl Readiness {
    /// What the frame about to be written is composed of, or `None` when this
    /// screen cannot disclose the question at all.
    /// A `None` **revokes**, and does not merely leave the pending slot empty.
    /// It is the surface reporting that it has nothing to show for this
    /// question -- a screen too narrow for a control, or no question installed
    /// at all -- and a receipt that outlived that would go on answering for a
    /// screen nobody composed. Defence in depth rather than a reachable path
    /// today: a grant already re-checks the id and the dimensions, and a
    /// composition that fails to capture is one the shell is about to refuse
    /// on. What makes it worth holding is the amendment ahead, where the
    /// *content* of a question can change under an unchanged id at an unchanged
    /// size -- at which point "the same id at the same dimensions" stops being
    /// enough to mean "the same screen", and this is the clause that was
    /// already saying so.
    pub(crate) fn intend(&mut self, intent: Option<Intent>) {
        self.undisclosed = intent.is_none();
        match intent {
            Some(intent) => self.pending = Some(intent),
            // Everything, `seen` included: it is the one piece of state a later
            // receipt may lean on rather than re-earn.
            None => self.invalidate(),
        }
    }

    /// What the terminal did with those bytes.
    ///
    /// `Painted` moves the intent to *provisional* and never straight to the
    /// receipt: the post-write check is [`Self::reconcile`]'s.
    ///
    /// `Unchanged` keeps a standing receipt only when the composition it was
    /// asked to repaint **is** that receipt. A zero-byte repaint is the screen
    /// saying it already holds these bytes, which proves nothing on its own --
    /// so it can never grant first-time readiness.
    pub(crate) fn landed(&mut self, outcome: Outcome) {
        let pending = self.pending.take();
        match outcome {
            Outcome::Painted => self.provisional = pending,
            // Retained only for the *same* screen: same request, same plane,
            // same size, same layout. Anything else is the screen saying it
            // already holds bytes nobody proved, so both go.
            Outcome::Unchanged if self.receipt.is_some() && self.receipt == pending => {}
            Outcome::Unchanged => {
                self.provisional = None;
                self.receipt = None;
            }
        }
    }

    /// The post-write check, one tick after the write.
    ///
    /// A `SIGWINCH` that landed inside the write is drained here
    /// (`super::event_loop`'s `collect_facts`), and `resize_pending` then stays
    /// true for the whole `super::render_request::RESIZE_DEBOUNCE` while the
    /// geometry still reports the **old** size. So all three go: dropping only
    /// the provisional would leave the previous receipt matching those unchanged
    /// dimensions and granting against a screen that has already reflowed, and
    /// keeping `seen` would let a later zero-byte repaint resurrect it.
    pub(crate) fn reconcile(&mut self, cols: u16, rows: u16, resize_pending: bool) {
        if resize_pending {
            self.provisional = None;
            self.receipt = None;
            self.seen = None;
            return;
        }
        let Some(provisional) = self.provisional.take() else {
            return;
        };
        if provisional.cols != cols || provisional.rows != rows {
            // A frame composed for a screen that is no longer this size proves
            // nothing about the one in front of the user.
            return;
        }
        if provisional.disclosure.change_visible {
            self.seen = Some((provisional.id, cols, rows));
        }
        self.receipt = Some(provisional);
    }

    /// A write the screen refused. A refused write may have left half a frame,
    /// so nothing that depended on those bytes survives it.
    pub(crate) fn write_failed(&mut self) {
        self.pending = None;
        self.provisional = None;
        self.receipt = None;
    }

    /// Everything the screen said is no longer true: external damage, a
    /// `/clear`, or the surface being taken down.
    pub(crate) fn invalidate(&mut self) {
        self.pending = None;
        self.provisional = None;
        self.receipt = None;
        self.seen = None;
    }

    /// Whether an affirmative answer to `id` may be taken at this size.
    pub(crate) fn ready(&self, id: ApprovalId, cols: u16, rows: u16) -> bool {
        let Some(receipt) = self.receipt.as_ref() else {
            return false;
        };
        receipt.id == id
            && receipt.cols == cols
            && receipt.rows == rows
            && (receipt.disclosure.change_visible || self.seen == Some((id, cols, rows)))
    }

    /// Why an affirmative is impossible at this size, for the notice.
    pub(crate) fn undisclosed(&self) -> bool {
        self.undisclosed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn full() -> Disclosure {
        Disclosure {
            last_control_row: 20,
            controls_whole: true,
            scope_whole: true,
            subject_whole: true,
            change_visible: true,
        }
    }

    fn intent(id: u64) -> Option<Intent> {
        Intent::capture(ApprovalId(id), Surface::Inline, 80, 24, full())
    }

    /// A receipt, the way every caller earns one: intend, land, reconcile.
    fn granted(id: u64) -> Readiness {
        let mut readiness = Readiness::default();
        readiness.intend(intent(id));
        readiness.landed(Outcome::Painted);
        readiness.reconcile(80, 24, false);
        readiness
    }

    #[test]
    fn a_committed_and_reconciled_frame_grants_its_own_request_only() {
        let readiness = granted(7);
        assert!(readiness.ready(ApprovalId(7), 80, 24));
        assert!(!readiness.ready(ApprovalId(8), 80, 24));
    }

    #[test]
    fn nothing_short_of_a_reconciled_write_grants() {
        let mut readiness = Readiness::default();
        readiness.intend(intent(7));
        assert!(
            !readiness.ready(ApprovalId(7), 80, 24),
            "an intent is not a receipt"
        );
        readiness.landed(Outcome::Painted);
        assert!(
            !readiness.ready(ApprovalId(7), 80, 24),
            "a write is not yet reconciled"
        );
        readiness.reconcile(80, 24, false);
        assert!(readiness.ready(ApprovalId(7), 80, 24));
    }

    #[test]
    fn a_winch_that_landed_inside_the_write_refuses_the_promotion() {
        let mut readiness = Readiness::default();
        readiness.intend(intent(7));
        readiness.landed(Outcome::Painted);
        readiness.reconcile(80, 24, true);
        assert!(!readiness.ready(ApprovalId(7), 80, 24));
        readiness.reconcile(80, 24, false);
        assert!(
            !readiness.ready(ApprovalId(7), 80, 24),
            "the provisional was dropped, not parked"
        );
    }

    #[test]
    fn a_winch_revokes_the_standing_receipt_at_the_very_same_dimensions() {
        // `geometry` still says 80x24 for the whole debounce
        // (`super::super::render_request::RESIZE_DEBOUNCE`), so a receipt kept
        // here would match and grant against a screen that has already
        // reflowed.
        let mut readiness = granted(7);
        readiness.reconcile(80, 24, true);
        assert!(
            !readiness.ready(ApprovalId(7), 80, 24),
            "the old receipt survived the signal"
        );

        // And no repaint that writes nothing may bring it back: `seen` went
        // with it, so this is a first grant and a first grant needs bytes.
        readiness.intend(intent(7));
        readiness.landed(Outcome::Unchanged);
        readiness.reconcile(80, 24, false);
        assert!(
            !readiness.ready(ApprovalId(7), 80, 24),
            "a no-change repaint resurrected it"
        );

        // A real redraw once the resize has settled earns readiness again.
        readiness.intend(intent(7));
        readiness.landed(Outcome::Painted);
        readiness.reconcile(80, 24, false);
        assert!(readiness.ready(ApprovalId(7), 80, 24));
    }

    #[test]
    fn a_first_frame_that_wrote_nothing_never_grants() {
        let mut readiness = Readiness::default();
        readiness.intend(intent(7));
        readiness.landed(Outcome::Unchanged);
        readiness.reconcile(80, 24, false);
        assert!(!readiness.ready(ApprovalId(7), 80, 24));
    }

    #[test]
    fn a_no_change_repaint_retains_only_a_receipt_of_the_same_identity() {
        let mut readiness = granted(7);
        readiness.intend(intent(7));
        readiness.landed(Outcome::Unchanged);
        assert!(
            readiness.ready(ApprovalId(7), 80, 24),
            "same identity, still proven"
        );
        readiness.intend(intent(9));
        readiness.landed(Outcome::Unchanged);
        assert!(!readiness.ready(ApprovalId(7), 80, 24));
        assert!(!readiness.ready(ApprovalId(9), 80, 24));
    }

    #[test]
    fn a_moved_marker_does_not_invalidate_but_a_changed_screen_does() {
        let mut readiness = granted(7);
        readiness.intend(intent(7));
        readiness.landed(Outcome::Painted);
        readiness.reconcile(80, 24, false);
        assert!(
            readiness.ready(ApprovalId(7), 80, 24),
            "the cursor is not part of identity"
        );
        let taller = Disclosure {
            last_control_row: 21,
            ..full()
        };
        readiness.intend(Intent::capture(
            ApprovalId(7),
            Surface::Inline,
            80,
            24,
            taller,
        ));
        readiness.landed(Outcome::Unchanged);
        assert!(
            !readiness.ready(ApprovalId(7), 80, 24),
            "a different layout is a different screen"
        );
    }

    #[test]
    fn a_failed_write_grants_nothing_and_revokes_what_was_granted() {
        let mut readiness = granted(7);
        readiness.intend(intent(7));
        readiness.write_failed();
        assert!(!readiness.ready(ApprovalId(7), 80, 24));
    }

    #[test]
    fn damage_and_a_changed_size_both_close_the_gate() {
        let mut readiness = granted(7);
        assert!(
            !readiness.ready(ApprovalId(7), 100, 24),
            "a different width is a different screen"
        );
        readiness.invalidate();
        assert!(!readiness.ready(ApprovalId(7), 80, 24));
    }

    #[test]
    fn an_undisclosed_composition_is_capturable_by_nothing_and_says_so() {
        for cut in [
            Disclosure {
                controls_whole: false,
                ..full()
            },
            Disclosure {
                scope_whole: false,
                ..full()
            },
            Disclosure {
                subject_whole: false,
                ..full()
            },
            Disclosure {
                last_control_row: 25,
                ..full()
            },
            // One-based, so zero is no row at all.
            Disclosure {
                last_control_row: 0,
                ..full()
            },
        ] {
            assert!(Intent::capture(ApprovalId(7), Surface::Inline, 80, 24, cut).is_none());
        }
        let mut readiness = Readiness::default();
        readiness.intend(None);
        assert!(
            readiness.undisclosed(),
            "the shell needs a reason to give the user"
        );
    }

    #[test]
    fn a_screen_with_no_rows_or_no_columns_discloses_nothing() {
        // The other half of `capture`'s guard, and it is not decoration: a
        // geometry solved for a screen that has gone to nothing would otherwise
        // hand back an intent whose `last_control_row <= rows` check passed
        // vacuously.
        assert!(Intent::capture(ApprovalId(7), Surface::Inline, 0, 24, full()).is_none());
        assert!(Intent::capture(ApprovalId(7), Surface::Inline, 80, 0, full()).is_none());
    }

    #[test]
    fn a_composition_that_can_disclose_nothing_revokes_what_was_proven() {
        // **Defense in depth for the amendment slice ahead.** An amendment can
        // change what a question *says* without changing its id or the screen's
        // size, so "the same id at the same dimensions" is about to stop being
        // enough to mean "the same screen". A `None` intent is the surface
        // reporting it has nothing to show for this question at all, and a
        // receipt that outlived it would answer for a screen nobody composed.
        let mut readiness = granted(7);
        assert!(readiness.ready(ApprovalId(7), 80, 24));
        readiness.intend(None);
        assert!(
            !readiness.ready(ApprovalId(7), 80, 24),
            "an undisclosed composition left the receipt standing"
        );
        assert!(readiness.undisclosed(), "and it still has to say why");
    }

    #[test]
    fn nothing_after_an_undisclosed_composition_resurrects_the_receipt() {
        let mut readiness = granted(7);
        readiness.intend(None);
        // The write of the frame that could not disclose, and the post-write
        // check. There is no pending intent to promote, so neither may put
        // anything back.
        readiness.landed(Outcome::Painted);
        readiness.reconcile(80, 24, false);
        assert!(
            !readiness.ready(ApprovalId(7), 80, 24),
            "a painted frame with nothing pending brought the receipt back"
        );
        // And a zero-byte repaint cannot stand in for the bytes a first grant
        // needs, because this is a first grant again.
        readiness.intend(intent(7));
        readiness.landed(Outcome::Unchanged);
        readiness.reconcile(80, 24, false);
        assert!(
            !readiness.ready(ApprovalId(7), 80, 24),
            "a no-change repaint resurrected it"
        );
        // Only a real frame for this request, reconciled, earns it again.
        readiness.intend(intent(7));
        readiness.landed(Outcome::Painted);
        readiness.reconcile(80, 24, false);
        assert!(readiness.ready(ApprovalId(7), 80, 24));
    }

    #[test]
    fn an_undisclosed_composition_takes_the_observation_with_it() {
        // `seen` goes too, and it has to be checked separately: it is the one
        // piece of state a later receipt can lean on rather than re-earn, so a
        // `seen` that outlived the composition which earned it would let a
        // frame showing no change grant against an observation made of a screen
        // that has since stopped disclosing.
        let hidden = Disclosure {
            change_visible: false,
            ..full()
        };
        let mut readiness = granted(7);
        readiness.intend(None);
        readiness.intend(Intent::capture(
            ApprovalId(7),
            Surface::Alternate,
            80,
            24,
            hidden,
        ));
        readiness.landed(Outcome::Painted);
        readiness.reconcile(80, 24, false);
        assert!(
            !readiness.ready(ApprovalId(7), 80, 24),
            "`seen` outlived the composition that earned it"
        );
    }

    #[test]
    fn a_change_counts_as_seen_only_after_this_request_really_showed_it() {
        let hidden = Disclosure {
            change_visible: false,
            ..full()
        };
        let away = |id| Intent::capture(ApprovalId(id), Surface::Alternate, 80, 24, hidden);

        let mut never = Readiness::default();
        never.intend(away(7));
        never.landed(Outcome::Painted);
        never.reconcile(80, 24, false);
        assert!(
            !never.ready(ApprovalId(7), 80, 24),
            "this request never showed the change"
        );

        let mut seen = granted(7);
        seen.intend(away(7));
        seen.landed(Outcome::Painted);
        seen.reconcile(80, 24, false);
        assert!(
            seen.ready(ApprovalId(7), 80, 24),
            "scrolled away, but observed at this size"
        );
    }
}
