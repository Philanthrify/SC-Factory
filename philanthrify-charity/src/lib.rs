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

    // ============================================================
    // TRANSACTION - MINTS NFT TO TRACK CHARITY SPENDING
    // ============================================================
    #[endpoint(transactionForCharity)]
    fn transaction_for_charity(
        &self,
        display_amount: BigUint,
        category: ManagedBuffer,
        description: ManagedBuffer,
        user_image_uri: ManagedBuffer,  // Optional user image (CID or full URL) - empty string means no image
    ) {
        self.only_owner();

        // 0 EGLD - only gas fees paid. display_amount is for NFT display only
        require!(display_amount > BigUint::zero(), "Display amount must be > 0");

        let charity_name = self.charity_name().get();
        let owner = self.owner().get();
        let factory = self.factory_address().get();

        // Call factory to mint transaction NFT
        self.tx()
            .to(&factory)
            .raw_call("mintTransactionNft")
            .argument(&owner)
            .argument(&display_amount)  // Display amount only
            .argument(&charity_name)
            .argument(&ManagedBuffer::from("charity"))
            .argument(&category)
            .argument(&description)
            .argument(&user_image_uri)  // Empty string if not provided
            .sync_call();

        self.transaction_event(&charity_name, &display_amount, &category, &description);
    }

    #[allow_multiple_var_args]
    #[endpoint(batchTransactionForCharity)]
    fn batch_transaction_for_charity(
        &self,
        num_transactions: u64,
        display_amounts: MultiValueEncoded<BigUint>,
        categories: MultiValueEncoded<ManagedBuffer>,
        descriptions: MultiValueEncoded<ManagedBuffer>,
    ) {
        self.only_owner();
        require!(num_transactions > 0, "Number of transactions must be > 0");
        
        let mut amounts_vec: ManagedVec<Self::Api, BigUint> = ManagedVec::new();
        for amount in display_amounts.into_iter() {
            amounts_vec.push(amount);
        }
        
        let mut categories_vec: ManagedVec<Self::Api, ManagedBuffer> = ManagedVec::new();
        for category in categories.into_iter() {
            categories_vec.push(category);
        }
        
        let mut descriptions_vec: ManagedVec<Self::Api, ManagedBuffer> = ManagedVec::new();
        for description in descriptions.into_iter() {
            descriptions_vec.push(description);
        }
        
        require!(
            amounts_vec.len() == num_transactions as usize &&
            categories_vec.len() == num_transactions as usize &&
            descriptions_vec.len() == num_transactions as usize,
            "Arrays length mismatch"
        );

        let charity_name = self.charity_name().get();
        let owner = self.owner().get();
        let factory = self.factory_address().get();

        for i in 0..num_transactions {
            let display_amount = amounts_vec.get(i as usize);
            let category = categories_vec.get(i as usize);
            let description = descriptions_vec.get(i as usize);
            
            require!(*display_amount > BigUint::zero(), "Display amount must be > 0");
            
            self.tx()
                .to(&factory)
                .raw_call("mintTransactionNft")
                .argument(&owner)
                .argument(&display_amount)
                .argument(&charity_name)
                .argument(&ManagedBuffer::from("charity"))
                .argument(&category)
                .argument(&description)
                .argument(&ManagedBuffer::new())  // Empty string for batch (no user image)
                .sync_call();

            self.batch_transaction_event(&charity_name, i + 1, num_transactions, &display_amount, &category, &description);
        }
    }

    // ============================================================
    // DONATION FUNCTIONS
    // ============================================================

    #[endpoint(donateToCharity)]
    fn donate_to_charity(
        &self,
        display_amount: BigUint,
        user_image_uri: ManagedBuffer,  // Optional user image (CID or full URL) - empty string means no image
        custom_tags: MultiValueEncoded<ManagedBuffer>,
    ) {
        // 0 EGLD - only gas fees paid. display_amount is for NFT display only
        let caller = self.blockchain().get_caller();
        let factory = self.factory_address().get();
        let charity_name = self.charity_name().get();

        let mut call = self.tx()
            .to(&factory)
            .raw_call("mintNft")
            .argument(&caller)
            .argument(&display_amount)  // Display amount only
            .argument(&charity_name)
            .argument(&ManagedBuffer::from("charity"))
            .argument(&user_image_uri);  // user_image_uri before custom_tags

        for tag in custom_tags.into_iter() {
            call = call.argument(&tag);
        }

        call.sync_call();

        self.donation_event(&caller, &display_amount, &charity_name);
    }

    #[endpoint(batchDonateToCharity)]
    fn batch_donate_to_charity(&self, num_donations: u64, display_amount_per_donation: BigUint, custom_tags: MultiValueEncoded<ManagedBuffer>) {
        // 0 EGLD - only gas fees paid. display_amount is for NFT display only
        require!(num_donations > 0 && num_donations <= 100, "Batch must be 1-100");
        require!(display_amount_per_donation > BigUint::zero(), "Display amount must be > 0");

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
                .argument(&display_amount_per_donation)  // Display amount only
                .argument(&charity_name)
                .argument(&ManagedBuffer::from("charity"));

            for tag in tags_vec.iter() {
                call = call.argument(&tag);
            }

            call.sync_call();

            self.batch_event(&caller, i + 1, num_donations, &display_amount_per_donation, &charity_name);
        }
    }

    #[endpoint(deployProject)]
    fn deploy_project(&self, project_name: ManagedBuffer) -> ManagedAddress {
        self.only_owner();

        let project_template = self.project_template().get();
        require!(!project_template.is_zero(), "Project template not set. Use setProjectTemplate or deploy charity with template set in factory.");

        let factory_address = self.factory_address().get();
        let charity_address = self.blockchain().get_sc_address();
        let charity_owner = self.owner().get(); // This is the admin address

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
            .argument(&charity_owner)  // Pass charity owner (admin) as global_admin, not charity address
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

    // ============================================================
    // EVENTS (Fixed - all indexed to avoid data parameter error)
    // ============================================================

    #[event("transaction_event")]
    fn transaction_event(
        &self,
        #[indexed] entity: &ManagedBuffer,
        #[indexed] amount: &BigUint,
        #[indexed] category: &ManagedBuffer,
        #[indexed] description: &ManagedBuffer,
    );

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

    #[event("project_deployed")]
    fn project_deployed_event(
        &self,
        #[indexed] project_name: &ManagedBuffer,
        #[indexed] address: &ManagedAddress,
    );

    #[event("batch_transaction_event")]
    fn batch_transaction_event(
        &self,
        #[indexed] entity: &ManagedBuffer,
        #[indexed] current: u64,
        #[indexed] total: u64,
        #[indexed] amount: &BigUint,
        #[indexed] category: &ManagedBuffer,
        #[indexed] description: &ManagedBuffer,
    );

    // ============================================================
    // STORAGE
    // ============================================================

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