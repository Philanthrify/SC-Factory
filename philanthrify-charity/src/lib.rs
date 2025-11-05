#![no_std]

extern crate alloc;

multiversx_sc::imports!();
multiversx_sc::derive_imports!();

#[multiversx_sc::contract]
pub trait PhilanthrifyCharity {
    #[init]
    fn init(
        &self,
        charity_name: ManagedBuffer,
        factory_address: ManagedAddress,
        owner_address: ManagedAddress,
        project_template: ManagedAddress,
    ) {
        self.charity_name().set(&charity_name);
        self.factory_address().set(&factory_address);
        self.owner().set(&owner_address);
        self.project_template().set(&project_template);
    }

    #[upgrade]
    fn upgrade(&self) {
        let caller = self.blockchain().get_caller();
        let owner = self.owner().get();
        let factory = self.factory_address().get();

        require!(
            caller == owner || caller == factory,
            "Only owner or factory allowed"
        );
    }

    fn only_owner(&self) {
        require!(
            self.blockchain().get_caller() == self.owner().get(),
            "Only owner allowed"
        );
    }

    #[payable("EGLD")]
    #[endpoint(donateToCharity)]
    fn donate_to_charity(&self, custom_tags: MultiValueEncoded<ManagedBuffer>) {
        let payment = self.call_value().egld();
        require!(*payment > BigUint::zero(), "Must send some EGLD");

        let caller = self.blockchain().get_caller();
        let factory = self.factory_address().get();
        let charity_name = self.charity_name().get();

        let mut call = self.tx()
            .to(&factory)
            .raw_call("mintNft")
            .argument(&caller)
            .argument(&*payment)
            .argument(&charity_name)
            .argument(&ManagedBuffer::from("charity"));

        for tag in custom_tags.into_iter() {
            call = call.argument(&tag);
        }

        call.sync_call();

        self.donation_event(&caller, &*payment, &charity_name);
    }

    #[payable("EGLD")]
    #[endpoint(batchDonateToCharity)]
    fn batch_donate_to_charity(&self, num_donations: u64, custom_tags: MultiValueEncoded<ManagedBuffer>) {
        let total_payment = self.call_value().egld();
        require!(*total_payment > BigUint::zero(), "Must send some EGLD");
        require!(num_donations > 0 && num_donations <= 100, "Batch must be 1-100");

        let payment_per_donation = total_payment.clone() / BigUint::from(num_donations);
        require!(payment_per_donation > BigUint::zero(), "Payment per donation too low");

        let caller = self.blockchain().get_caller();
        let factory = self.factory_address().get();
        let charity_name = self.charity_name().get();

        let mut tags_vec: ManagedVec<Self::Api, ManagedBuffer> = ManagedVec::new();
        for tag in custom_tags.into_iter() {
            tags_vec.push(tag);
        }

        for i in 0..num_donations {
            let mut call = self.tx()
                .to(&factory)
                .raw_call("mintNft")
                .argument(&caller)
                .argument(&payment_per_donation)
                .argument(&charity_name)
                .argument(&ManagedBuffer::from("charity"));

            for tag in tags_vec.iter() {
                call = call.argument(&tag);
            }

            call.sync_call();

            self.batch_event(&caller, i + 1, num_donations, &payment_per_donation, &charity_name);
        }
    }

    #[payable("EGLD")]
    #[endpoint(forwardDonationToProject)]
    fn forward_donation_to_project(&self, project_address: ManagedAddress) {
        let payment = self.call_value().egld();
        require!(*payment > BigUint::zero(), "Must send some EGLD");
        require!(!project_address.is_zero(), "Invalid project address");

        let caller = self.blockchain().get_caller();

        self.tx()
            .to(&project_address)
            .egld(&*payment)
            .raw_call("donateToProject")
            .sync_call();

        self.forwarded_event(&caller, &*payment, &project_address);
    }

    #[payable("EGLD")]
    #[endpoint(batchForwardDonationToProject)]
    fn batch_forward_donation_to_project(&self, project_address: ManagedAddress, num_donations: u64) {
        let total_payment = self.call_value().egld();
        require!(*total_payment > BigUint::zero(), "Must send some EGLD");
        require!(num_donations > 0 && num_donations <= 100, "Batch must be 1-100");
        require!(!project_address.is_zero(), "Invalid project address");

        let payment_per_donation = total_payment.clone() / BigUint::from(num_donations);
        require!(payment_per_donation > BigUint::zero(), "Payment per donation too low");

        let caller = self.blockchain().get_caller();

        for i in 0..num_donations {
            self.tx()
                .to(&project_address)
                .egld(&payment_per_donation)
                .raw_call("donateToProject")
                .sync_call();

            self.batch_event_project(&caller, i + 1, num_donations, &payment_per_donation, &project_address);
        }
    }

    #[endpoint(deployProject)]
    fn deploy_project(&self, project_name: ManagedBuffer) -> ManagedAddress {
        self.only_owner();

        let project_template = self.project_template().get();
        require!(!project_template.is_zero(), "Project template not set");

        let factory_address = self.factory_address().get();
        let charity_address = self.blockchain().get_sc_address();

        let new_project: ManagedAddress = self
            .tx()
            .raw_deploy()
            .from_source(project_template)
            .code_metadata(
                CodeMetadata::PAYABLE
                    | CodeMetadata::PAYABLE_BY_SC
                    | CodeMetadata::UPGRADEABLE
                    | CodeMetadata::READABLE,
            )
            .argument(&project_name)
            .argument(&charity_address)
            .argument(&factory_address)
            .argument(&charity_address)
            .gas(15_000_000)
            .returns(ReturnsNewAddress)
            .sync_call()
            .into();

        self.project_deployed_event(&project_name, &new_project);
        new_project
    }

    #[endpoint(transferOwnership)]
    fn transfer_ownership(&self, new_owner: ManagedAddress) {
        self.only_owner();
        require!(!new_owner.is_zero(), "Invalid owner address");
        self.owner().set(&new_owner);
    }

    #[endpoint(setOwner)]
    fn set_owner(&self, new_owner: ManagedAddress) {
        self.only_owner();
        self.owner().set(&new_owner);
    }

    #[endpoint(setProjectTemplate)]
    fn set_project_template(&self, project_template: ManagedAddress) {
        self.only_owner();
        require!(!project_template.is_zero(), "Invalid template address");
        self.project_template().set(project_template);
    }

    #[event("donation_event")]
    fn donation_event(
        &self,
        #[indexed] donor: &ManagedAddress,
        #[indexed] amount: &BigUint,
        #[indexed] entity: &ManagedBuffer,
    );

    #[event("batch_event")]
    fn batch_event(
        &self,
        #[indexed] caller: &ManagedAddress,
        #[indexed] current: u64,
        #[indexed] total: u64,
        #[indexed] amount_per: &BigUint,
        #[indexed] entity: &ManagedBuffer,
    );

    #[event("batch_event_project")]
    fn batch_event_project(
        &self,
        #[indexed] caller: &ManagedAddress,
        #[indexed] current: u64,
        #[indexed] total: u64,
        #[indexed] amount_per: &BigUint,
        #[indexed] project: &ManagedAddress,
    );

    #[event("forwarded_event")]
    fn forwarded_event(
        &self,
        #[indexed] caller: &ManagedAddress,
        #[indexed] amount: &BigUint,
        #[indexed] project: &ManagedAddress,
    );

    #[event("project_deployed")]
    fn project_deployed_event(
        &self,
        #[indexed] project_name: &ManagedBuffer,
        #[indexed] address: &ManagedAddress,
    );

    #[view(getCharityName)]
    #[storage_mapper("charity_name")]
    fn charity_name(&self) -> SingleValueMapper<ManagedBuffer>;

    #[view(getOwner)]
    #[storage_mapper("owner")]
    fn owner(&self) -> SingleValueMapper<ManagedAddress>;

    #[view(getFactoryAddress)]
    #[storage_mapper("factory_address")]
    fn factory_address(&self) -> SingleValueMapper<ManagedAddress>;

    #[view(getProjectTemplate)]
    #[storage_mapper("project_template")]
    fn project_template(&self) -> SingleValueMapper<ManagedAddress>;
}