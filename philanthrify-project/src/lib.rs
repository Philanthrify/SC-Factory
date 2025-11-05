#![no_std]

extern crate alloc;

multiversx_sc::imports!();
multiversx_sc::derive_imports!();

#[multiversx_sc::contract]
pub trait PhilanthrifyProject {
    #[init]
    fn init(
        &self,
        project_name: ManagedBuffer,
        charity_address: ManagedAddress,
        factory_address: ManagedAddress,
        global_admin: ManagedAddress,
    ) {
        self.project_name().set(&project_name);
        self.charity_address().set(&charity_address);
        self.factory_address().set(&factory_address);
        self.global_admin().set(&global_admin);
        self.owner().set(&charity_address);
    }

    #[upgrade]
    fn upgrade(&self) {
        let caller = self.blockchain().get_caller();
        let owner = self.owner().get();
        let factory = self.factory_address().get();
        let admin = self.global_admin().get();

        require!(
            caller == owner || caller == factory || caller == admin,
            "Only charity, factory, or admin allowed"
        );
    }

    fn only_owner(&self) {
        require!(
            self.blockchain().get_caller() == self.owner().get(),
            "Only owner allowed"
        );
    }

    #[payable("EGLD")]
    #[endpoint(donateToProject)]
    fn donate_to_project(&self, custom_tags: MultiValueEncoded<ManagedBuffer>) {
        let payment = self.call_value().egld();
        require!(*payment > BigUint::zero(), "Must send some EGLD");

        let caller = self.blockchain().get_caller();
        let factory = self.factory_address().get();
        let project_name = self.project_name().get();

        let mut call = self.tx()
            .to(&factory)
            .raw_call("mintNft")
            .argument(&caller)
            .argument(&*payment)
            .argument(&project_name)
            .argument(&ManagedBuffer::from("project"));

        for tag in custom_tags.into_iter() {
            call = call.argument(&tag);
        }

        call.sync_call();

        self.donation_event(&caller, &*payment, &project_name);
    }

    #[payable("EGLD")]
    #[endpoint(batchDonateToProject)]
    fn batch_donate_to_project(&self, num_donations: u64, custom_tags: MultiValueEncoded<ManagedBuffer>) {
        let total_payment = self.call_value().egld();
        require!(*total_payment > BigUint::zero(), "Must send some EGLD");
        require!(num_donations > 0 && num_donations <= 100, "Batch must be 1-100");

        let payment_per = total_payment.clone() / BigUint::from(num_donations);
        require!(payment_per > BigUint::zero(), "Payment per donation too low");

        let caller = self.blockchain().get_caller();
        let factory = self.factory_address().get();
        let project_name = self.project_name().get();

        let mut tags_vec: ManagedVec<Self::Api, ManagedBuffer> = ManagedVec::new();
        for tag in custom_tags.into_iter() {
            tags_vec.push(tag);
        }

        for i in 0..num_donations {
            let mut call = self.tx()
                .to(&factory)
                .raw_call("mintNft")
                .argument(&caller)
                .argument(&payment_per)
                .argument(&project_name)
                .argument(&ManagedBuffer::from("project"));

            for tag in tags_vec.iter() {
                call = call.argument(&tag);
            }

            call.sync_call();

            self.batch_event(&caller, i + 1, num_donations, &payment_per, &project_name);
        }
    }

    #[endpoint(setOwner)]
    fn set_owner(&self, new_owner: ManagedAddress) {
        self.only_owner();
        require!(!new_owner.is_zero(), "Invalid owner address");
        self.owner().set(&new_owner);
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

    #[view(getProjectName)]
    #[storage_mapper("project_name")]
    fn project_name(&self) -> SingleValueMapper<ManagedBuffer>;

    #[view(getCharityAddress)]
    #[storage_mapper("charity_address")]
    fn charity_address(&self) -> SingleValueMapper<ManagedAddress>;

    #[view(getFactoryAddress)]
    #[storage_mapper("factory_address")]
    fn factory_address(&self) -> SingleValueMapper<ManagedAddress>;

    #[view(getGlobalAdmin)]
    #[storage_mapper("global_admin")]
    fn global_admin(&self) -> SingleValueMapper<ManagedAddress>;

    #[view(getOwner)]
    #[storage_mapper("owner")]
    fn owner(&self) -> SingleValueMapper<ManagedAddress>;
}