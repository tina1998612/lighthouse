use crate::*;
use serde_derive::{Deserialize, Serialize};

#[cfg(feature = "arbitrary-fuzz")]
use arbitrary::Arbitrary;

#[cfg_attr(feature = "arbitrary-fuzz", derive(Arbitrary))]
#[derive(Debug, PartialEq, Clone, Copy, Default, Serialize, Deserialize)]
pub struct AttestationDuty {
    /// The slot during which the attester must attest.
    pub slot: Slot,
    /// The index of this committee within the committees in `slot`.
    pub index: CommitteeIndex,
    /// The position of the attester within the committee.
    pub committee_position: usize,
    /// The total number of attesters in the committee.
    pub committee_len: usize,
}
