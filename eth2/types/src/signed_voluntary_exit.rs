use crate::{test_utils::TestRandom, VoluntaryExit};
use bls::Signature;

use serde_derive::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use test_random_derive::TestRandom;
use tree_hash_derive::TreeHash;

#[cfg(feature = "arbitrary-fuzz")]
use arbitrary::Arbitrary;

/// An exit voluntarily submitted a validator who wishes to withdraw.
///
/// Spec v0.11.1
#[cfg_attr(feature = "arbitrary-fuzz", derive(Arbitrary))]
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash, TestRandom)]
pub struct SignedVoluntaryExit {
    pub message: VoluntaryExit,
    pub signature: Signature,
}

#[cfg(test)]
mod tests {
    use super::*;

    ssz_and_tree_hash_tests!(SignedVoluntaryExit);
}
