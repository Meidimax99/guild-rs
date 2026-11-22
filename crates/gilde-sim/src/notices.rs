//! Notices (HE events) and their deadlines (`msg_ProcessNotices` `0x52d390`).
//!
//! Every game minute, each notice whose deadline (`+0x52`) has passed and
//! that is not flagged `0x18` runs the handler of its type (167 types,
//! `docs/notices.tsv`). The handlers are not ported yet: a due notice is
//! reported once and kept.

use crate::date::Date;
use gilde_formats::savegame::Notice;

#[derive(Debug, Clone)]
pub struct PendingNotice {
    pub notice: Notice,
    pub deadline: Date,
    /// Already reported as due (stand-in for the handler running).
    pub fired: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Notices {
    pub pending: Vec<PendingNotice>,
}

impl Notices {
    pub fn from_saved(saved: Vec<Notice>) -> Self {
        let pending = saved
            .into_iter()
            .map(|n| PendingNotice { deadline: Date::from_bytes(&n.deadline()).unwrap_or_default(), notice: n, fired: false })
            .collect();
        Notices { pending }
    }

    /// Notices that became due at `now`: (type, character slot).
    pub fn process(&mut self, now: &Date) -> Vec<(u8, u16)> {
        let mut due = Vec::new();
        for p in &mut self.pending {
            if !p.fired && p.notice.flags & 0x18 == 0 && *now > p.deadline && p.notice.kind < 0xa7 {
                p.fired = true;
                due.push((p.notice.kind, p.notice.character));
            }
        }
        due
    }
}
