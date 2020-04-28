use crate::test_utils::TestRandom;
use crate::*;
use serde_derive::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use ssz_types::typenum::U33;
use test_random_derive::TestRandom;
use tree_hash_derive::TreeHash;

pub const DEPOSIT_TREE_DEPTH: usize = 32;

#[cfg(feature = "arbitrary-fuzz")]
use arbitrary::Arbitrary;

/// A deposit to potentially become a beacon chain validator.
///
/// Spec v0.11.1
#[cfg_attr(feature = "arbitrary-fuzz", derive(Arbitrary))]
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash, TestRandom)]
pub struct Deposit {
    pub proof: FixedVector<Hash256, U33>,
    pub data: DepositData,
}

#[cfg(test)]
mod tests {
    use super::*;

    ssz_and_tree_hash_tests!(Deposit);
}
