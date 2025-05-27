#![no_std]

extern crate alloc;

multiversx_sc::imports!();
multiversx_sc::derive_imports!();

#[multiversx_sc::contract]
pub trait PhilanthrifyCharity {
    #[init]
    fn init(&self, charity_name: ManagedBuffer, factory_address: ManagedAddress, owner: ManagedAddress) {
        let branded_name = self.format_charity_name(&charity_name);
        self.charity_name().set(&branded_name);
        self.factory_address().set(&factory_address);
        self.owner().set(&owner);
        self.donation_count().set(0u64); // Initialize donation count
    }

    #[upgrade]
    fn upgrade(&self) {
        // No owner check - any wallet can upgrade the contract
    }

    #[payable("EGLD")]
    #[endpoint(donate)]
    fn donate(&self) {
        let payment = self.call_value().egld();
        require!(*payment > BigUint::zero(), "Must send some EGLD");

        let caller = self.blockchain().get_caller();
        let token_id = self.nft_token_id().get();
        require!(!token_id.as_managed_buffer().is_empty(), "NFT token not set");

        // Increment donation count
        let mut count = self.donation_count().get();
        count += 1;
        self.donation_count().set(count);

        // Create dynamic NFT name: "Philanthrify Impact Token - <charity_name> - Donation #<count>"
        let charity_name = self.charity_name().get();
        let mut token_name = ManagedBuffer::new_from_bytes(b"Philanthrify Impact Token - ");
        token_name.append(&charity_name);
        token_name.append(&ManagedBuffer::new_from_bytes(b" - Donation #"));
        token_name.append(&self.u64_to_managed_buffer(count));

        let amount = BigUint::from(1u32);
        let royalties = BigUint::from(1000u32); // 10% royalties (1000 basis points)
        let attributes = ManagedBuffer::new_from_bytes(b"tags:charity-donation,philanthrify");
        let hash_buffer = self.crypto().sha256(&attributes);
        let attributes_hash = hash_buffer.as_managed_buffer();

        let nonce = self.send().esdt_nft_create(
            &token_id,
            &amount,
            &token_name,
            &royalties,
            &attributes_hash,
            &attributes,
            &ManagedVec::new(),
        );

        self.send().direct_esdt(&caller, &token_id, nonce, &amount);

        self.donation_event(&caller, &*payment, &token_id, nonce);
    }

    #[endpoint(deployProject)]
    fn deploy_project(&self, project_name: ManagedBuffer, project_template: ManagedAddress) -> ManagedAddress {
        let gas_for_deploy = 15_000_000u64;
        let new_project_address: ManagedAddress<Self::Api> = self
            .tx()
            .raw_deploy()
            .from_source(project_template)
            .code_metadata(
                multiversx_sc::types::CodeMetadata::PAYABLE
                    | multiversx_sc::types::CodeMetadata::PAYABLE_BY_SC
                    | multiversx_sc::types::CodeMetadata::UPGRADEABLE
                    | multiversx_sc::types::CodeMetadata::READABLE,
            )
            .argument(&project_name)
            .argument(&self.blockchain().get_sc_address())
            .gas(gas_for_deploy)
            .returns(multiversx_sc::types::ReturnsNewAddress)
            .sync_call()
            .into();

        self.tx()
            .to(new_project_address.clone())
            .raw_call("setNftTokenId")
            .argument(&self.nft_token_id().get())
            .sync_call();

        let mut projects = self.deployed_projects().get();
        projects.push(new_project_address.clone());
        self.deployed_projects().set(projects);

        self.project_deployed_event(&project_name, &new_project_address);

        new_project_address
    }

    #[payable("EGLD")]
    #[endpoint(donateToProject)]
    fn donate_to_project(&self, project_address: ManagedAddress) {
        let payment = self.call_value().egld();
        require!(*payment > BigUint::zero(), "Must send some EGLD");

        let projects = self.deployed_projects().get();
        require!(
            projects.contains(&project_address),
            "Project not deployed by this contract"
        );

        self.tx()
            .to(project_address)
            .egld(&*payment)
            .raw_call("donate")
            .sync_call();

        let sc_address = self.blockchain().get_sc_address();
        self.donation_event(&sc_address, &*payment, &TokenIdentifier::from(""), 0);
    }

    #[endpoint(setNftTokenId)]
    fn set_nft_token_id(&self, token_id: TokenIdentifier) {
        self.nft_token_id().set(&token_id);
    }

    #[endpoint(setProjectTemplate)]
    fn set_project_template(&self, project_template: ManagedAddress) {
        self.project_template().set(project_template);
    }

    #[endpoint(setOwner)]
    fn set_owner(&self, new_owner: ManagedAddress) {
        self.owner().set(new_owner);
    }

    fn format_charity_name(&self, name: &ManagedBuffer) -> ManagedBuffer {
        let suffix = ManagedBuffer::new_from_bytes(b" - Philanthrify Foundation");
        let mut branded_name = ManagedBuffer::new();
        branded_name.append(name);
        branded_name.append(&suffix);
        branded_name
    }

    // Helper function to convert u64 to ManagedBuffer without dynamic allocation
    fn u64_to_managed_buffer(&self, mut num: u64) -> ManagedBuffer {
        if num == 0 {
            return ManagedBuffer::new_from_bytes(b"0");
        }

        let mut result = ManagedBuffer::new();
        while num > 0 {
            let digit = (num % 10) as u8;
            // Prepend the digit to the result (reverses the order naturally)
            let mut new_result = ManagedBuffer::new_from_bytes(&[digit + b'0']); // Convert to ASCII (e.g., 0 -> '0')
            new_result.append(&result);
            result = new_result;
            num /= 10;
        }

        result
    }

    #[event("donationEvent")]
    fn donation_event(
        &self,
        #[indexed] donor: &ManagedAddress,
        #[indexed] amount: &BigUint,
        #[indexed] token_id: &TokenIdentifier,
        #[indexed] nonce: u64,
    );

    #[event("projectDeployed")]
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

    #[view(getDeployedProjects)]
    #[storage_mapper("deployed_projects")]
    fn deployed_projects(&self) -> SingleValueMapper<ManagedVec<ManagedAddress>>;

    #[view(getProjectTemplate)]
    #[storage_mapper("project_template")]
    fn project_template(&self) -> SingleValueMapper<ManagedAddress>;

    #[view(getNftTokenId)]
    #[storage_mapper("nft_token_id")]
    fn nft_token_id(&self) -> SingleValueMapper<TokenIdentifier>;

    #[view(getDonationCount)]
    #[storage_mapper("donation_count")]
    fn donation_count(&self) -> SingleValueMapper<u64>;
}