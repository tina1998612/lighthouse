// #![cfg(not(debug_assertions))]

#[macro_use]
extern crate lazy_static;

use beacon_chain::{
    test_utils::{AttestationStrategy, BeaconChainHarness, BlockStrategy, HarnessType},
    BeaconSnapshot, BlockError,
};
use types::{
    test_utils::generate_deterministic_keypair, AggregateSignature, AttestationData,
    AttesterSlashing, Checkpoint, Deposit, DepositData, Epoch, EthSpec, Hash256,
    IndexedAttestation, Keypair, MainnetEthSpec, ProposerSlashing, Signature, SignedBeaconBlock,
    SignedBeaconBlockHeader, SignedVoluntaryExit, Slot, VoluntaryExit, DEPOSIT_TREE_DEPTH,
};

type E = MainnetEthSpec;

// Should ideally be divisible by 3.
pub const VALIDATOR_COUNT: usize = 24;
pub const CHAIN_SEGMENT_LENGTH: usize = 64 * 5;

lazy_static! {
    /// A cached set of keys.
    static ref KEYPAIRS: Vec<Keypair> = types::test_utils::generate_deterministic_keypairs(VALIDATOR_COUNT);

    /// A cached set of valid blocks
    static ref CHAIN_SEGMENT: Vec<BeaconSnapshot<E>> = get_chain_segment();
}

fn get_chain_segment() -> Vec<BeaconSnapshot<E>> {
    let harness = get_harness(VALIDATOR_COUNT);

    harness.extend_chain(
        CHAIN_SEGMENT_LENGTH,
        BlockStrategy::OnCanonicalHead,
        AttestationStrategy::AllValidators,
    );

    harness
        .chain
        .chain_dump()
        .expect("should dump chain")
        .into_iter()
        .skip(1)
        .collect()
}

fn get_harness(validator_count: usize) -> BeaconChainHarness<HarnessType<E>> {
    let harness = BeaconChainHarness::new(MainnetEthSpec, KEYPAIRS[0..validator_count].to_vec());

    harness.advance_slot();

    harness
}

fn chain_segment_blocks() -> Vec<SignedBeaconBlock<E>> {
    CHAIN_SEGMENT
        .iter()
        .map(|snapshot| snapshot.beacon_block.clone())
        .collect()
}

fn junk_signature() -> Signature {
    let kp = generate_deterministic_keypair(VALIDATOR_COUNT);
    let message = &[42, 42];
    Signature::new(message, &kp.sk)
}

fn junk_aggregate_signature() -> AggregateSignature {
    let mut agg_sig = AggregateSignature::new();
    agg_sig.add(&junk_signature());
    agg_sig
}

fn update_proposal_signatures(
    snapshots: &mut [BeaconSnapshot<E>],
    harness: &BeaconChainHarness<HarnessType<E>>,
) {
    for snapshot in snapshots {
        let spec = &harness.chain.spec;
        let slot = snapshot.beacon_block.slot();
        let state = &snapshot.beacon_state;
        let proposer_index = state
            .get_beacon_proposer_index(slot, spec)
            .expect("should find proposer index");
        let keypair = harness
            .keypairs
            .get(proposer_index)
            .expect("proposer keypair should be available");

        snapshot.beacon_block =
            snapshot
                .beacon_block
                .message
                .clone()
                .sign(&keypair.sk, &state.fork, spec);
    }
}

fn update_parent_roots(snapshots: &mut [BeaconSnapshot<E>]) {
    for i in 0..snapshots.len() {
        let root = snapshots[i].beacon_block.canonical_root();
        if let Some(child) = snapshots.get_mut(i + 1) {
            child.beacon_block.message.parent_root = root
        }
    }
}

#[test]
fn chain_segment_full_segment() {
    let harness = get_harness(VALIDATOR_COUNT);
    let blocks = chain_segment_blocks();

    harness
        .chain
        .slot_clock
        .set_slot(blocks.last().unwrap().slot().as_u64());

    // Sneak in a little check to ensure we can process empty chain segments.
    harness
        .chain
        .import_chain_segment(vec![])
        .expect("should import empty chain segment");

    harness
        .chain
        .import_chain_segment(blocks.clone())
        .expect("should import chain segment");

    harness.chain.fork_choice().expect("should run fork choice");

    assert_eq!(
        harness
            .chain
            .head_info()
            .expect("should get harness b head")
            .block_root,
        blocks.last().unwrap().canonical_root(),
        "harness should have last block as head"
    );
}

#[test]
fn chain_segment_varying_chunk_size() {
    for chunk_size in &[1, 2, 3, 5, 31, 32, 33, 42] {
        let harness = get_harness(VALIDATOR_COUNT);
        let blocks = chain_segment_blocks();

        harness
            .chain
            .slot_clock
            .set_slot(blocks.last().unwrap().slot().as_u64());

        for chunk in blocks.chunks(*chunk_size) {
            harness
                .chain
                .import_chain_segment(chunk.to_vec())
                .expect(&format!(
                    "should import chain segment of len {}",
                    chunk_size
                ));
        }

        harness.chain.fork_choice().expect("should run fork choice");

        assert_eq!(
            harness
                .chain
                .head_info()
                .expect("should get harness b head")
                .block_root,
            blocks.last().unwrap().canonical_root(),
            "harness should have last block as head"
        );
    }
}

#[test]
fn chain_segment_non_linear_parent_roots() {
    let harness = get_harness(VALIDATOR_COUNT);
    harness
        .chain
        .slot_clock
        .set_slot(CHAIN_SEGMENT.last().unwrap().beacon_block.slot().as_u64());

    /*
     * Test with a block removed.
     */
    let mut blocks = chain_segment_blocks();
    blocks.remove(2);

    assert_eq!(
        harness.chain.import_chain_segment(blocks.clone()),
        Err(BlockError::NonLinearParentRoots),
        "should not import chain with missing parent"
    );

    /*
     * Test with a modified parent root.
     */
    let mut blocks = chain_segment_blocks();
    blocks[3].message.parent_root = Hash256::zero();

    assert_eq!(
        harness.chain.import_chain_segment(blocks.clone()),
        Err(BlockError::NonLinearParentRoots),
        "should not import chain with a broken parent root link"
    );
}

#[test]
fn chain_segment_non_linear_slots() {
    let harness = get_harness(VALIDATOR_COUNT);
    harness
        .chain
        .slot_clock
        .set_slot(CHAIN_SEGMENT.last().unwrap().beacon_block.slot().as_u64());

    /*
     * Test where a child is lower than the parent.
     */

    let mut blocks = chain_segment_blocks();
    blocks[3].message.slot = Slot::new(0);

    assert_eq!(
        harness.chain.import_chain_segment(blocks.clone()),
        Err(BlockError::NonLinearSlots),
        "should not import chain with a parent that has a lower slot than its child"
    );

    /*
     * Test where a child is equal to the parent.
     */

    let mut blocks = chain_segment_blocks();
    blocks[3].message.slot = blocks[2].message.slot;

    assert_eq!(
        harness.chain.import_chain_segment(blocks.clone()),
        Err(BlockError::NonLinearSlots),
        "should not import chain with a parent that has an equal slot to its child"
    );
}

#[test]
fn invalid_signatures() {
    let mut checked_attestation = false;

    for &block_index in &[0, 1, 32, 64, 68 + 1, 129, CHAIN_SEGMENT.len() - 1] {
        let harness = get_harness(VALIDATOR_COUNT);
        harness
            .chain
            .slot_clock
            .set_slot(CHAIN_SEGMENT.last().unwrap().beacon_block.slot().as_u64());

        // Import all the ancestors before the `block_index` block.
        let ancestor_blocks = CHAIN_SEGMENT
            .iter()
            .take(block_index)
            .map(|snapshot| snapshot.beacon_block.clone())
            .collect();
        harness
            .chain
            .import_chain_segment(ancestor_blocks)
            .expect("should import all blocks prior to the one being tested");

        // For the given snapshots, test the following:
        //
        // - The `import_chain_segment` function returns `InvalidSignature`.
        // - The `import_block` function returns `InvalidSignature` when importing the
        //    `SignedBeaconBlock` directly.
        // - The `verify_block_for_gossip` function does _not_ return an error.
        // - The `import_block` function returns `InvalidSignature` when verifying the
        //    GossipVerifiedBlock.
        let assert_invalid_signature = |snapshots: &[BeaconSnapshot<E>], item: &str| {
            let blocks = snapshots
                .iter()
                .map(|snapshot| snapshot.beacon_block.clone())
                .collect();

            // Ensure the block will be rejected if imported in a chain segment.
            assert_eq!(
                harness.chain.import_chain_segment(blocks),
                Err(BlockError::InvalidSignature),
                "should not import chain segment with an invalid {} signature",
                item
            );

            // Ensure the block will be rejected if imported on its own (without gossip checking).
            assert_eq!(
                harness
                    .chain
                    .import_block(snapshots[block_index].beacon_block.clone()),
                Err(BlockError::InvalidSignature),
                "should not import individual block with an invalid {} signature",
                item
            );

            let gossip_verified = harness
                .chain
                .verify_block_for_gossip(snapshots[block_index].beacon_block.clone())
                .expect("should obtain gossip verified block");
            assert_eq!(
                harness.chain.import_block(gossip_verified),
                Err(BlockError::InvalidSignature),
                "should not import gossip verified block with an invalid {} signature",
                item
            );
        };

        /*
         * Block proposal
         */
        let mut snapshots = CHAIN_SEGMENT.clone();
        snapshots[block_index].beacon_block.signature = junk_signature();
        let blocks = snapshots
            .iter()
            .map(|snapshot| snapshot.beacon_block.clone())
            .collect();
        // Ensure the block will be rejected if imported in a chain segment.
        assert_eq!(
            harness.chain.import_chain_segment(blocks),
            Err(BlockError::InvalidSignature),
            "should not import chain segment with an invalid gossip signature",
        );
        // Ensure the block will be rejected if imported on its own (without gossip checking).
        assert_eq!(
            harness
                .chain
                .import_block(snapshots[block_index].beacon_block.clone()),
            Err(BlockError::InvalidSignature),
            "should not import individual block with an invalid gossip signature",
        );

        /*
         * Randao reveal
         */
        let mut snapshots = CHAIN_SEGMENT.clone();
        snapshots[block_index]
            .beacon_block
            .message
            .body
            .randao_reveal = junk_signature();
        update_parent_roots(&mut snapshots);
        update_proposal_signatures(&mut snapshots, &harness);
        assert_invalid_signature(&snapshots, "randao");

        /*
         * Proposer slashing
         */
        let mut snapshots = CHAIN_SEGMENT.clone();
        let proposer_slashing = ProposerSlashing {
            proposer_index: 0,
            signed_header_1: SignedBeaconBlockHeader {
                message: snapshots[block_index].beacon_block.message.block_header(),
                signature: junk_signature(),
            },
            signed_header_2: SignedBeaconBlockHeader {
                message: snapshots[block_index].beacon_block.message.block_header(),
                signature: junk_signature(),
            },
        };
        snapshots[block_index]
            .beacon_block
            .message
            .body
            .proposer_slashings
            .push(proposer_slashing)
            .expect("should update proposer slashing");
        update_parent_roots(&mut snapshots);
        update_proposal_signatures(&mut snapshots, &harness);
        assert_invalid_signature(&snapshots, "proposer slashing");

        /*
         * Attester slashing
         */
        let mut snapshots = CHAIN_SEGMENT.clone();
        let indexed_attestation = IndexedAttestation {
            attesting_indices: vec![0].into(),
            data: AttestationData {
                slot: Slot::new(0),
                index: 0,
                beacon_block_root: Hash256::zero(),
                source: Checkpoint {
                    epoch: Epoch::new(0),
                    root: Hash256::zero(),
                },
                target: Checkpoint {
                    epoch: Epoch::new(0),
                    root: Hash256::zero(),
                },
            },
            signature: junk_aggregate_signature(),
        };
        let attester_slashing = AttesterSlashing {
            attestation_1: indexed_attestation.clone(),
            attestation_2: indexed_attestation,
        };
        snapshots[block_index]
            .beacon_block
            .message
            .body
            .attester_slashings
            .push(attester_slashing)
            .expect("should update attester slashing");
        update_parent_roots(&mut snapshots);
        update_proposal_signatures(&mut snapshots, &harness);
        assert_invalid_signature(&snapshots, "attester slashing");

        /*
         * Attestation
         */
        let mut snapshots = CHAIN_SEGMENT.clone();
        if let Some(attestation) = snapshots[block_index]
            .beacon_block
            .message
            .body
            .attestations
            .get_mut(0)
        {
            attestation.signature = junk_aggregate_signature();
            update_parent_roots(&mut snapshots);
            update_proposal_signatures(&mut snapshots, &harness);
            assert_invalid_signature(&snapshots, "attestation");
            checked_attestation = true;
        }

        /*
         * Deposit
         *
         * Note: an invalid deposit signature is permitted!
         */
        let mut snapshots = CHAIN_SEGMENT.clone();
        let deposit = Deposit {
            proof: vec![Hash256::zero(); DEPOSIT_TREE_DEPTH + 1].into(),
            data: DepositData {
                pubkey: Keypair::random().pk.into(),
                withdrawal_credentials: Hash256::zero(),
                amount: 0,
                signature: junk_signature().into(),
            },
        };
        snapshots[block_index]
            .beacon_block
            .message
            .body
            .deposits
            .push(deposit)
            .expect("should update deposit");
        update_parent_roots(&mut snapshots);
        update_proposal_signatures(&mut snapshots, &harness);
        let blocks = snapshots
            .iter()
            .map(|snapshot| snapshot.beacon_block.clone())
            .collect();
        assert!(
            harness.chain.import_chain_segment(blocks) != Err(BlockError::InvalidSignature),
            "should not throw an invalid signature error for a bad deposit signature"
        );

        /*
         * Voluntary exit
         */
        let mut snapshots = CHAIN_SEGMENT.clone();
        let epoch = snapshots[block_index].beacon_state.current_epoch();
        snapshots[block_index]
            .beacon_block
            .message
            .body
            .voluntary_exits
            .push(SignedVoluntaryExit {
                message: VoluntaryExit {
                    epoch,
                    validator_index: 0,
                },
                signature: junk_signature(),
            })
            .expect("should update deposit");
        update_parent_roots(&mut snapshots);
        update_proposal_signatures(&mut snapshots, &harness);
        assert_invalid_signature(&snapshots, "voluntary exit");
    }

    assert!(
        checked_attestation,
        "the test should check an attestation signature"
    )
}

fn unwrap_err<T, E>(result: Result<T, E>) -> E {
    match result {
        Ok(_) => panic!("called unwrap_err on Ok"),
        Err(e) => e,
    }
}

#[test]
fn gossip_verification() {
    let harness = get_harness(VALIDATOR_COUNT);

    let block_index = CHAIN_SEGMENT_LENGTH - 2;

    harness
        .chain
        .slot_clock
        .set_slot(CHAIN_SEGMENT[block_index].beacon_block.slot().as_u64());

    // Import the ancestors prior to the block we're testing.
    for snapshot in &CHAIN_SEGMENT[0..block_index] {
        let gossip_verified = harness
            .chain
            .verify_block_for_gossip(snapshot.beacon_block.clone())
            .expect("should obtain gossip verified block");

        harness
            .chain
            .import_block(gossip_verified)
            .expect("should import valid gossip verfied block");
    }

    /*
     * Block with invalid signature
     */

    let mut block = CHAIN_SEGMENT[block_index].beacon_block.clone();
    block.signature = junk_signature();
    assert_eq!(
        unwrap_err(harness.chain.verify_block_for_gossip(block)),
        BlockError::ProposalSignatureInvalid,
        "should not import a block with an invalid proposal signature"
    );

    /*
     * Block from a future slot.
     */

    let mut block = CHAIN_SEGMENT[block_index].beacon_block.clone();
    let block_slot = block.message.slot + 1;
    block.message.slot = block_slot;
    assert_eq!(
        unwrap_err(harness.chain.verify_block_for_gossip(block)),
        BlockError::FutureSlot {
            present_slot: block_slot - 1,
            block_slot
        },
        "should not import a block with a future slot"
    );

    /*
     * Block from a finalized slot.
     */

    let mut block = CHAIN_SEGMENT[block_index].beacon_block.clone();
    let finalized_slot = harness
        .chain
        .head_info()
        .expect("should get head info")
        .finalized_checkpoint
        .epoch
        .start_slot(E::slots_per_epoch());
    block.message.slot = finalized_slot;
    assert_eq!(
        unwrap_err(harness.chain.verify_block_for_gossip(block)),
        BlockError::WouldRevertFinalizedSlot {
            block_slot: finalized_slot,
            finalized_slot
        },
        "should not import a block with a finalized slot"
    );
}