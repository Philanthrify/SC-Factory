#![no_std]

multiversx_sc::imports!();
multiversx_sc::derive_imports!();

#[multiversx_sc::contract]
pub trait PhilanthrifyProject {
    #[init]
    fn init(&self, project_name: ManagedBuffer, charity_address: ManagedAddress) {
        self.project_name().set(&project_name);
        self.charity_address().set(&charity_address);
    }

    #[upgrade]
    fn upgrade(&self) {
        require!(
            self.blockchain().get_caller() == self.charity_address().get(),
            "Only the Charity contract can upgrade the contract"
        );
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

        let amount = BigUint::from(1u32);
        let token_name = ManagedBuffer::new_from_bytes(b"Philanthrify Impact Token - Project Donation");
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
        require!(
            self.blockchain().get_caller() == self.charity_address().get(),
            "Only the Charity contract can set the NFT token ID"
        );
        self.nft_token_id().set(&token_id);
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
}