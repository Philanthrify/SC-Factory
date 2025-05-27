#![no_std]

extern crate alloc;

multiversx_sc::imports!();
multiversx_sc::derive_imports!();

#[multiversx_sc::contract]
pub trait PhilanthrifyProject {
    #[init]
    fn init(&self, project_name: ManagedBuffer, charity_address: ManagedAddress) {
        self.project_name().set(&project_name);
        self.charity_address().set(&charity_address);
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
        let charity_address = self.charity_address().get();
        require!(
            caller == charity_address,
            "Only the owning Charity contract can donate"
        );

        let token_id = self.nft_token_id().get();
        require!(!token_id.as_managed_buffer().is_empty(), "NFT token not set");

        // Increment donation count
        let mut count = self.donation_count().get();
        count += 1;
        self.donation_count().set(count);

        // Create dynamic NFT name: "Philanthrify Impact Token - <project_name> - Donation #<count>"
        let project_name = self.project_name().get();
        let mut token_name = ManagedBuffer::new_from_bytes(b"Philanthrify Impact Token - ");
        token_name.append(&project_name);
        token_name.append(&ManagedBuffer::new_from_bytes(b" - Donation #"));
        token_name.append(&self.u64_to_managed_buffer(count));

        let amount = BigUint::from(1u32);
        let royalties = BigUint::from(1000u32); // 10% royalties (1000 basis points)
        let attributes = ManagedBuffer::new_from_bytes(b"tags:project-donation,philanthrify");
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

        self.donation_event(&charity_address, &*payment, &token_id, nonce);
    }

    #[endpoint(setNftTokenId)]
    fn set_nft_token_id(&self, token_id: TokenIdentifier) {
        self.nft_token_id().set(&token_id);
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

    #[view(getProjectName)]
    #[storage_mapper("project_name")]
    fn project_name(&self) -> SingleValueMapper<ManagedBuffer>;

    #[view(getCharityAddress)]
    #[storage_mapper("charity_address")]
    fn charity_address(&self) -> SingleValueMapper<ManagedAddress>;

    #[view(getNftTokenId)]
    #[storage_mapper("nft_token_id")]
    fn nft_token_id(&self) -> SingleValueMapper<TokenIdentifier>;

    #[view(getDonationCount)]
    #[storage_mapper("donation_count")]
    fn donation_count(&self) -> SingleValueMapper<u64>;
}