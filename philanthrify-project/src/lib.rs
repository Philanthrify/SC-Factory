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
        // Project owner is the charity contract address
        // But we need to allow the charity owner (admin) to call transactionForProject
        // So check if caller is either:
        // 1. The charity contract itself (owner)
        // 2. The global admin (who is the charity owner)
        let caller = self.blockchain().get_caller();
        let owner = self.owner().get(); // This is the charity contract address
        let admin = self.global_admin().get(); // This is the charity owner (admin)
        
        require!(
            caller == owner || caller == admin,
            "Only owner allowed"
        );
    }

    // ============================================================
    // TRANSACTION - MINTS NFT TO TRACK PROJECT SPENDING
    // ============================================================
    #[endpoint(transactionForProject)]
    fn transaction_for_project(
        &self,
        display_amount: BigUint,
        category: ManagedBuffer,
        description: ManagedBuffer,
        user_image_uri: ManagedBuffer,  // Optional user image (CID or full URL) - empty string means no image
    ) {
        self.only_owner();

        // 0 EGLD - only gas fees paid. display_amount is for NFT display only
        require!(display_amount > BigUint::zero(), "Display amount must be > 0");

        let project_name = self.project_name().get();
        // Use the caller (admin) as the entity owner for the NFT, not the project owner (charity contract)
        let caller = self.blockchain().get_caller();
        let factory = self.factory_address().get();

        // Call factory to mint transaction NFT
        // Pass caller (admin) as entity_owner since admin is calling this
        self.tx()
            .to(&factory)
            .raw_call("mintTransactionNft")
            .argument(&caller)
            .argument(&display_amount)  // Display amount only
            .argument(&project_name)
            .argument(&ManagedBuffer::from("project"))
            .argument(&category)
            .argument(&description)
            .argument(&user_image_uri)  // Empty string if not provided
            .sync_call();

        self.transaction_event(&project_name, &display_amount, &category, &description);
    }

    #[allow_multiple_var_args]
    #[endpoint(batchTransactionForProject)]
    fn batch_transaction_for_project(
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

        let project_name = self.project_name().get();
        let caller = self.blockchain().get_caller();
        let factory = self.factory_address().get();

        for i in 0..num_transactions {
            let display_amount = amounts_vec.get(i as usize);
            let category = categories_vec.get(i as usize);
            let description = descriptions_vec.get(i as usize);
            
            require!(*display_amount > BigUint::zero(), "Display amount must be > 0");
            
            self.tx()
                .to(&factory)
                .raw_call("mintTransactionNft")
                .argument(&caller)
                .argument(&display_amount)
                .argument(&project_name)
                .argument(&ManagedBuffer::from("project"))
                .argument(&category)
                .argument(&description)
                .argument(&ManagedBuffer::new())  // Empty string for batch (no user image)
                .sync_call();

            self.batch_transaction_event(&project_name, i + 1, num_transactions, &display_amount, &category, &description);
        }
    }

    // ============================================================
    // DONATION FUNCTIONS
    // ============================================================

    #[endpoint(donateToProject)]
    fn donate_to_project(
        &self,
        display_amount: BigUint,
        user_image_uri: ManagedBuffer,  // Optional user image (CID or full URL) - empty string means no image
        custom_tags: MultiValueEncoded<ManagedBuffer>,
    ) {
        // 0 EGLD - only gas fees paid. display_amount is for NFT display only
        let caller = self.blockchain().get_caller();
        let factory = self.factory_address().get();
        let project_name = self.project_name().get();

        let mut call = self.tx()
            .to(&factory)
            .raw_call("mintNft")
            .argument(&caller)
            .argument(&display_amount)  // Display amount only
            .argument(&project_name)
            .argument(&ManagedBuffer::from("project"))
            .argument(&user_image_uri);  // user_image_uri before custom_tags

        for tag in custom_tags.into_iter() {
            call = call.argument(&tag);
        }

        call.sync_call();

        self.donation_event(&caller, &display_amount, &project_name);
    }

    #[endpoint(batchDonateToProject)]
    fn batch_donate_to_project(&self, num_donations: u64, display_amount_per_donation: BigUint, custom_tags: MultiValueEncoded<ManagedBuffer>) {
        // 0 EGLD - only gas fees paid. display_amount is for NFT display only
        require!(num_donations > 0 && num_donations <= 100, "Batch must be 1-100");
        require!(display_amount_per_donation > BigUint::zero(), "Display amount must be > 0");

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
                .argument(&display_amount_per_donation)  // Display amount only
                .argument(&project_name)
                .argument(&ManagedBuffer::from("project"));

            for tag in tags_vec.iter() {
                call = call.argument(&tag);
            }

            call.sync_call();

            self.batch_event(&caller, i + 1, num_donations, &display_amount_per_donation, &project_name);
        }
    }

    #[endpoint(setOwner)]
    fn set_owner(&self, new_owner: ManagedAddress) {
        self.only_owner();
        require!(!new_owner.is_zero(), "Invalid owner address");
        self.owner().set(&new_owner);
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