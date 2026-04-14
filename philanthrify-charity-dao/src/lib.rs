#![no_std]

extern crate alloc;

multiversx_sc::imports!();
multiversx_sc::derive_imports!();

// Role IDs match the DAO plan:
// 0=CharityHead, 1=CharityAdmin, 2=OperationsHead, 3=OperationsAdmin,
// 4=FundraisingHead, 5=FundraisingAdmin, 6=ProjectLead, 7=ProjectReporter, 8=CharityMember
const ROLE_WEIGHTS: [u64; 9] = [10, 5, 6, 4, 5, 3, 3, 2, 1];

#[type_abi]
#[derive(Clone, TopEncode, TopDecode, NestedEncode, NestedDecode)]
pub struct Proposal<M: ManagedTypeApi> {
    pub creator: ManagedAddress<M>,
    pub description: ManagedBuffer<M>,
    pub status: u8, // 1=Active, 2=Passed, 3=Failed, 4=Executed
    pub votes_for: u64,
    pub votes_against: u64,
    pub voting_ends_at: u64, // block nonce
    pub total_snapshot_power: u64,
}

impl<M: ManagedTypeApi> Proposal<M> {
    pub const STATUS_ACTIVE: u8 = 1;
    pub const STATUS_PASSED: u8 = 2;
    pub const STATUS_FAILED: u8 = 3;
    pub const STATUS_EXECUTED: u8 = 4;
}

#[multiversx_sc::contract]
pub trait PhilanthrifyCharityDao {
    #[init]
    fn init(
        &self,
        charity_address: ManagedAddress,
        bootstrap_address: ManagedAddress,
        quorum_percent: u64,
        voting_period_blocks: u64,
    ) {
        require!(quorum_percent > 0 && quorum_percent <= 100, "Quorum must be 1-100");
        require!(voting_period_blocks > 0, "Voting period must be > 0");
        require!(!charity_address.is_zero(), "Invalid charity address");
        require!(!bootstrap_address.is_zero(), "Invalid bootstrap address");

        self.charity_address().set(&charity_address);
        self.bootstrap_address().set(&bootstrap_address);
        self.quorum_percent().set(quorum_percent);
        self.voting_period_blocks().set(voting_period_blocks);

        self.proposal_count().set(0u64);

        // bootstrap period ends after N blocks; default ~30 days if block time ~2s
        let block_nonce = self.blockchain().get_block_nonce();
        self.bootstrap_deadline().set(block_nonce + 30 * 60 * 24);
    }

    // =========================
    // Bootstrap & role registry
    // =========================

    #[endpoint(assignRole)]
    fn assign_role(&self, member: ManagedAddress, role_id: u8) {
        require!(role_id < 9, "Invalid role id (0-8)");
        require!(!member.is_zero(), "Invalid member");

        self.require_can_manage_roles();

        let mut roles = self.member_roles().get(&member).unwrap_or(0u16);
        roles |= 1u16 << role_id;
        self.member_roles().insert(member.clone(), roles);
        self.members_set().insert(member.clone());

        self.role_assigned_event(&member, role_id);
    }

    #[endpoint(removeRole)]
    fn remove_role(&self, member: ManagedAddress, role_id: u8) {
        require!(role_id < 9, "Invalid role id (0-8)");
        require!(!member.is_zero(), "Invalid member");

        self.require_can_manage_roles();

        let mut roles = self.member_roles().get(&member).unwrap_or(0u16);
        roles &= !(1u16 << role_id);

        if roles == 0 {
            self.member_roles().remove(&member);
            self.members_set().remove(&member);
        } else {
            self.member_roles().insert(member.clone(), roles);
        }

        self.role_removed_event(&member, role_id);
    }

    #[endpoint(renounceBootstrap)]
    fn renounce_bootstrap(&self) {
        let caller = self.blockchain().get_caller();
        require!(caller == self.bootstrap_address().get(), "Only bootstrap can renounce");
        self.bootstrap_address().set(&ManagedAddress::zero());
        self.bootstrap_renounced_event();
    }

    fn require_can_manage_roles(&self) {
        let caller = self.blockchain().get_caller();
        let block_nonce = self.blockchain().get_block_nonce();
        let bootstrap = self.bootstrap_address().get();

        // During bootstrap period: only bootstrap.
        // After: only Charity Head (role 0).
        if block_nonce <= self.bootstrap_deadline().get() {
            require!(caller == bootstrap, "Only bootstrap can manage roles");
        } else {
            require!(self.has_role(&caller, 0), "Only Charity Head can manage roles");
        }
    }

    // =========================
    // Proposals & voting (snapshot)
    // =========================

    #[endpoint(createProposal)]
    fn create_proposal(&self, description: ManagedBuffer) -> u64 {
        let caller = self.blockchain().get_caller();
        require!(self.get_member_power(&caller) > 0, "Only members can create proposals");

        let proposal_id = self.proposal_count().get();
        self.proposal_count().set(proposal_id + 1);

        let block_nonce = self.blockchain().get_block_nonce();
        let voting_ends_at = block_nonce + self.voting_period_blocks().get();

        // Snapshot: store (address, power) in parallel vectors.
        let mut total_power = 0u64;
        for member in self.members_set().iter() {
            let power = self.get_member_power(&member);
            if power > 0 {
                total_power += power;
                self.proposal_snapshot_addresses(proposal_id).push(&member);
                self.proposal_snapshot_powers(proposal_id).push(&power);
            }
        }

        let proposal = Proposal::<Self::Api> {
            creator: caller.clone(),
            description,
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

        if total_voted >= quorum_power && proposal.votes_for > proposal.votes_against {
            proposal.status = Proposal::<Self::Api>::STATUS_PASSED;
        } else {
            proposal.status = Proposal::<Self::Api>::STATUS_FAILED;
        }

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
        self.proposal_executed_event(proposal_id);
    }

    // =========================
    // Governance hook views
    // =========================

    #[view(hasPermission)]
    fn has_permission(&self, address: ManagedAddress, action_id: ManagedBuffer) -> bool {
        if action_id == ManagedBuffer::from("assign_roles") {
            return self.has_role(&address, 0);
        }
        if action_id == ManagedBuffer::from("approve_spend") || action_id == ManagedBuffer::from("publish_report") {
            return self.get_member_power(&address) > 0;
        }
        self.get_member_power(&address) > 0
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

    #[view(getRoleSet)]
    fn get_role_set(&self, address: ManagedAddress) -> ManagedVec<u8> {
        let mut out = ManagedVec::new();
        let roles = self.member_roles().get(&address).unwrap_or(0u16);
        for i in 0..9u8 {
            if (roles & (1u16 << i)) != 0 {
                out.push(i);
            }
        }
        out
    }

    #[view(getConfig)]
    fn get_config(&self) -> (u64, u64) {
        (self.quorum_percent().get(), self.voting_period_blocks().get())
    }

    // =========================
    // Helpers
    // =========================

    fn has_role(&self, address: &ManagedAddress, role_id: u8) -> bool {
        let roles = self.member_roles().get(address).unwrap_or(0u16);
        (roles & (1u16 << role_id)) != 0
    }

    fn get_member_power(&self, address: &ManagedAddress) -> u64 {
        let roles = self.member_roles().get(address).unwrap_or(0u16);
        let mut power = 0u64;
        for i in 0..9usize {
            if (roles & (1u16 << i)) != 0 {
                power += ROLE_WEIGHTS[i];
            }
        }
        power
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

    #[event("role_assigned")]
    fn role_assigned_event(&self, #[indexed] member: &ManagedAddress, #[indexed] role_id: u8);

    #[event("role_removed")]
    fn role_removed_event(&self, #[indexed] member: &ManagedAddress, #[indexed] role_id: u8);

    #[event("bootstrap_renounced")]
    fn bootstrap_renounced_event(&self);

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

    #[view(getCharityAddress)]
    #[storage_mapper("charity_address")]
    fn charity_address(&self) -> SingleValueMapper<ManagedAddress>;

    #[view(getBootstrapAddress)]
    #[storage_mapper("bootstrap_address")]
    fn bootstrap_address(&self) -> SingleValueMapper<ManagedAddress>;

    #[view(getBootstrapDeadline)]
    #[storage_mapper("bootstrap_deadline")]
    fn bootstrap_deadline(&self) -> SingleValueMapper<u64>;

    #[view(getQuorumPercent)]
    #[storage_mapper("quorum_percent")]
    fn quorum_percent(&self) -> SingleValueMapper<u64>;

    #[view(getVotingPeriodBlocks)]
    #[storage_mapper("voting_period_blocks")]
    fn voting_period_blocks(&self) -> SingleValueMapper<u64>;

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

    #[storage_mapper("member_roles")]
    fn member_roles(&self) -> MapMapper<ManagedAddress, u16>;

    #[storage_mapper("members_set")]
    fn members_set(&self) -> SetMapper<ManagedAddress>;
}

