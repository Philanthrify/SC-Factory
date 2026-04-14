#![no_std]

extern crate alloc;

multiversx_sc::imports!();
multiversx_sc::derive_imports!();

/// A minimal PlatformDAO contract:
/// - Token-weighted governance for platform decisions
/// - Linear voting power: 1 token balance unit = 1 vote (clamped to u64)
/// - Snapshot at proposal creation (snapshot list is provided by the creator)
/// - Proposal lifecycle: create -> vote -> finalize -> execute
///
/// Note: This contract stores the snapshot for the voter list passed during `create_proposal`.
/// For best production UX, you would typically restrict/curate the snapshot voter set (e.g. via a voter registry).
///
/// **Demo mode** (`demo_mode = true` at init): ignores real ESDT balances and treats every non-zero
/// address as having voting power **1**. Lets you demo proposals without issuing $PHIL. **Do not use in production.**
#[type_abi]
#[derive(
    Clone, TopEncode, TopDecode, NestedEncode, NestedDecode
)]
pub struct Proposal<M: ManagedTypeApi> {
    pub creator: ManagedAddress<M>,
    pub description: ManagedBuffer<M>,
    /// 0 = none, 1 = set platform param (demo action)
    pub action_kind: u8,
    pub action_key: ManagedBuffer<M>,
    pub action_value: ManagedBuffer<M>,
    /// 1=Active, 2=Passed, 3=Failed, 4=Executed
    pub status: u8,
    pub votes_for: u64,
    pub votes_against: u64,
    pub voting_ends_at: u64,
    pub total_snapshot_power: u64,
}

impl<M: ManagedTypeApi> Proposal<M> {
    pub const STATUS_ACTIVE: u8 = 1;
    pub const STATUS_PASSED: u8 = 2;
    pub const STATUS_FAILED: u8 = 3;
    pub const STATUS_EXECUTED: u8 = 4;
}

#[multiversx_sc::contract]
pub trait PhilanthrifyPlatformDao {
    // =========================
    // Init
    // =========================
    #[init]
    fn init(
        &self,
        phil_token_id: TokenIdentifier,
        quorum_percent: u64,
        voting_period_blocks: u64,
        demo_mode: bool,
    ) {
        require!(
            quorum_percent > 0 && quorum_percent <= 100,
            "Quorum must be 1-100"
        );
        require!(voting_period_blocks > 0, "Voting period must be > 0");
        require!(
            phil_token_id.is_valid_esdt_identifier(),
            "Invalid token id"
        );

        self.phil_token_id().set(&phil_token_id);
        self.quorum_percent().set(&quorum_percent);
        self.voting_period_blocks().set(&voting_period_blocks);
        self.demo_mode().set(demo_mode);
        self.proposal_count().set(0u64);
    }

    // =========================
    // Proposals & voting
    // =========================
    #[endpoint(createProposal)]
    fn create_proposal(
        &self,
        description: ManagedBuffer,
        action_kind: u8,
        action_key: ManagedBuffer,
        action_value: ManagedBuffer,
        voters: MultiValueEncoded<ManagedAddress>,
    ) -> u64 {
        require!(voters.len() > 0, "Provide at least 1 voter");
        require!(action_kind <= 1, "Invalid action_kind");

        let caller = self.blockchain().get_caller();
        let token_power = self.get_token_voting_power(&caller);
        require!(token_power > 0, "Only token holders can create proposals");

        let proposal_id = self.proposal_count().get();
        self.proposal_count().set(proposal_id + 1);

        let block_nonce = self.blockchain().get_block_nonce();
        let voting_ends_at = block_nonce + self.voting_period_blocks().get();

        // Snapshot: store (address, power) in parallel vectors.
        let mut total_power = 0u64;
        for voter in voters {
            let power = self.get_token_voting_power_from_esdt_balance(&voter);
            if power > 0 {
                self.proposal_snapshot_addresses(proposal_id).push(&voter);
                self.proposal_snapshot_powers(proposal_id).push(&power);
                total_power += power;
            }
        }
        require!(total_power > 0, "Snapshot total power must be > 0");

        let proposal = Proposal::<Self::Api> {
            creator: caller.clone(),
            description,
            action_kind,
            action_key,
            action_value,
            status: Proposal::<Self::Api>::STATUS_ACTIVE,
            votes_for: 0,
            votes_against: 0,
            voting_ends_at,
            total_snapshot_power: total_power,
        };

        self.proposals(proposal_id).set(&proposal);
        self.proposal_created_event(proposal_id, &caller, voting_ends_at);
        proposal_id
    }

    #[endpoint(vote)]
    fn vote(&self, proposal_id: u64, for_proposal: bool) {
        require!(!self.proposals(proposal_id).is_empty(), "Proposal does not exist");

        let caller = self.blockchain().get_caller();
        require!(
            !self.proposal_has_voted().contains_key(&(proposal_id, caller.clone())),
            "Already voted"
        );

        let mut proposal = self.proposals(proposal_id).get();
        require!(proposal.status == Proposal::<Self::Api>::STATUS_ACTIVE, "Proposal not active");

        let block_nonce = self.blockchain().get_block_nonce();
        require!(block_nonce <= proposal.voting_ends_at, "Voting ended");

        let power = self.get_voting_power_from_snapshot(proposal_id, &caller);
        require!(power > 0, "No voting power for this proposal");

        self.proposal_has_voted().insert((proposal_id, caller.clone()), true);

        if for_proposal {
            proposal.votes_for += power;
        } else {
            proposal.votes_against += power;
        }

        self.proposals(proposal_id).set(&proposal);
        self.vote_cast_event(proposal_id, &caller, for_proposal, power);
    }

    #[endpoint(finalizeProposal)]
    fn finalize_proposal(&self, proposal_id: u64) {
        require!(!self.proposals(proposal_id).is_empty(), "Proposal does not exist");

        let mut proposal = self.proposals(proposal_id).get();
        require!(proposal.status == Proposal::<Self::Api>::STATUS_ACTIVE, "Proposal not active");

        let block_nonce = self.blockchain().get_block_nonce();
        require!(block_nonce > proposal.voting_ends_at, "Voting not ended");

        let total_voted = proposal.votes_for + proposal.votes_against;
        let quorum_power = (proposal.total_snapshot_power * self.quorum_percent().get()) / 100;

        let passed = total_voted >= quorum_power && proposal.votes_for > proposal.votes_against;
        proposal.status = if passed {
            Proposal::<Self::Api>::STATUS_PASSED
        } else {
            Proposal::<Self::Api>::STATUS_FAILED
        };

        self.proposals(proposal_id).set(&proposal);
        self.proposal_finalized_event(proposal_id, proposal.status);
    }

    #[endpoint(executeProposal)]
    fn execute_proposal(&self, proposal_id: u64) {
        require!(!self.proposals(proposal_id).is_empty(), "Proposal does not exist");

        let mut proposal = self.proposals(proposal_id).get();
        require!(proposal.status == Proposal::<Self::Api>::STATUS_PASSED, "Proposal not passed");

        proposal.status = Proposal::<Self::Api>::STATUS_EXECUTED;
        self.proposals(proposal_id).set(&proposal);

        // Demo execution: apply a deterministic internal state change.
        // In production, you would wire this to Factory/other contracts via a governance executor pattern.
        if proposal.action_kind == 1 {
            // `MapMapper::insert` takes values by value (not references).
            self.platform_params()
                .insert(proposal.action_key, proposal.action_value);
        }

        self.proposal_executed_event(proposal_id);
    }

    // =========================
    // Governance hook views
    // =========================
    #[view(hasPermission)]
    fn has_permission(&self, address: ManagedAddress, action_id: ManagedBuffer) -> bool {
        // Minimal permission model:
        // - anyone with token power can create proposals / vote
        // - execution is checked by proposal state (finalized + passed) in `execute_proposal`
        if action_id == ManagedBuffer::from("create_proposal") {
            return self.get_token_voting_power(&address) > 0;
        }
        if action_id == ManagedBuffer::from("vote") {
            return self.get_token_voting_power(&address) > 0;
        }
        if action_id == ManagedBuffer::from("execute_proposal") {
            return true;
        }
        self.get_token_voting_power(&address) > 0
    }

    #[view(getVotingPower)]
    fn get_voting_power(&self, proposal_id: u64, address: ManagedAddress) -> u64 {
        self.get_voting_power_from_snapshot(proposal_id, &address)
    }

    #[view(isProposalApproved)]
    fn is_proposal_approved(&self, proposal_id: u64) -> bool {
        if self.proposals(proposal_id).is_empty() {
            return false;
        }
        let proposal = self.proposals(proposal_id).get();
        proposal.status == Proposal::<Self::Api>::STATUS_PASSED
            || proposal.status == Proposal::<Self::Api>::STATUS_EXECUTED
    }

    #[view(getConfig)]
    fn get_config(&self) -> (u64, u64) {
        (self.quorum_percent().get(), self.voting_period_blocks().get())
    }

    #[view(getDemoMode)]
    fn get_demo_mode(&self) -> bool {
        self.demo_mode().get()
    }

    #[view(getPlatformParam)]
    fn get_platform_param(&self, key: ManagedBuffer) -> Option<ManagedBuffer> {
        self.platform_params().get(&key)
    }

    // =========================
    // Helpers
    // =========================
    fn get_token_voting_power(&self, address: &ManagedAddress) -> u64 {
        self.get_token_voting_power_from_esdt_balance(address)
    }

    fn get_token_voting_power_from_esdt_balance(&self, address: &ManagedAddress) -> u64 {
        if address.is_zero() {
            return 0;
        }
        if self.demo_mode().get() {
            // Demo / progress showcase: no real token required.
            return 1;
        }

        let token_id = self.phil_token_id().get();
        let balance = self.blockchain().get_esdt_balance(address, &token_id, 0);

        // Linear voting power: 1 token = 1 vote.
        // Convert to u64 to keep arithmetic simple; if balance doesn't fit u64, we clamp.
        let bal_u64 = balance.to_u64().unwrap_or(u64::MAX);
        bal_u64
    }

    fn get_voting_power_from_snapshot(&self, proposal_id: u64, address: &ManagedAddress) -> u64 {
        let addresses = self.proposal_snapshot_addresses(proposal_id);
        let powers = self.proposal_snapshot_powers(proposal_id);
        let len = addresses.len();
        for i in 0..len {
            if addresses.get(i) == *address {
                return powers.get(i);
            }
        }
        0
    }

    // =========================
    // Events
    // =========================
    #[event("proposal_created")]
    fn proposal_created_event(
        &self,
        #[indexed] proposal_id: u64,
        #[indexed] creator: &ManagedAddress,
        #[indexed] voting_ends_at: u64,
    );

    #[event("vote_cast")]
    fn vote_cast_event(
        &self,
        #[indexed] proposal_id: u64,
        #[indexed] voter: &ManagedAddress,
        #[indexed] for_proposal: bool,
        #[indexed] power: u64,
    );

    #[event("proposal_finalized")]
    fn proposal_finalized_event(&self, #[indexed] proposal_id: u64, #[indexed] status: u8);

    #[event("proposal_executed")]
    fn proposal_executed_event(&self, #[indexed] proposal_id: u64);

    // =========================
    // Storage
    // =========================
    #[view(getPhilTokenId)]
    #[storage_mapper("phil_token_id")]
    fn phil_token_id(&self) -> SingleValueMapper<TokenIdentifier<Self::Api>>;

    #[view(getQuorumPercent)]
    #[storage_mapper("quorum_percent")]
    fn quorum_percent(&self) -> SingleValueMapper<u64>;

    #[view(getVotingPeriodBlocks)]
    #[storage_mapper("voting_period_blocks")]
    fn voting_period_blocks(&self) -> SingleValueMapper<u64>;

    #[storage_mapper("demo_mode")]
    fn demo_mode(&self) -> SingleValueMapper<bool>;

    #[storage_mapper("proposal_count")]
    fn proposal_count(&self) -> SingleValueMapper<u64>;

    #[view(getProposal)]
    #[storage_mapper("proposals")]
    fn proposals(&self, id: u64) -> SingleValueMapper<Proposal<Self::Api>>;

    #[storage_mapper("proposal_snapshot_addresses")]
    fn proposal_snapshot_addresses(&self, id: u64) -> VecMapper<ManagedAddress>;

    #[storage_mapper("proposal_snapshot_powers")]
    fn proposal_snapshot_powers(&self, id: u64) -> VecMapper<u64>;

    #[storage_mapper("proposal_has_voted")]
    fn proposal_has_voted(&self) -> MapMapper<(u64, ManagedAddress), bool>;

    /// A generic key/value storage updated by executed proposals (demo action).
    #[storage_mapper("platform_params")]
    fn platform_params(&self) -> MapMapper<ManagedBuffer, ManagedBuffer>;
}

