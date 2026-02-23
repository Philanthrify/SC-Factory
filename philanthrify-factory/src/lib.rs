#![no_std]

extern crate alloc;

multiversx_sc::imports!();
multiversx_sc::derive_imports!();

#[type_abi]
#[derive(TopEncode, TopDecode, NestedEncode, NestedDecode, Clone)]
pub struct DonationRecord<M: ManagedTypeApi> {
    pub amount: BigUint<M>,
    pub timestamp: u64,
    pub entity_name: ManagedBuffer<M>,
    pub entity_type: ManagedBuffer<M>,
    pub nft_nonce: u64,
}

#[type_abi]
#[derive(TopEncode, TopDecode, NestedEncode, NestedDecode, Clone)]
pub struct GlobalStats<M: ManagedTypeApi> {
    pub total_donations_amount: BigUint<M>,
    pub total_donations_count: u64,
    pub total_nfts_minted: u64,
}

#[type_abi]
#[derive(TopEncode, TopDecode, NestedEncode, NestedDecode, Clone)]
pub struct NftMetadataRecord<M: ManagedTypeApi> {
    pub nft_nonce: u64,
    pub donor_address: ManagedAddress<M>,
    pub entity_name: ManagedBuffer<M>,
    pub entity_type: ManagedBuffer<M>,
    pub donation_count: u64,
    pub tier_level: u64,
    pub total_amount: BigUint<M>,
    pub last_updated: u64,
    pub is_on_contract: bool,
}

#[type_abi]
#[derive(TopEncode, TopDecode, NestedEncode, NestedDecode, Clone)]
pub struct PatronRecord<M: ManagedTypeApi> {
    pub donor_address: ManagedAddress<M>,
    pub total_amount: BigUint<M>,
    pub patron_rank: u64,  // 1-10
    pub since_timestamp: u64,
}

#[type_abi]
#[derive(TopEncode, TopDecode, NestedEncode, NestedDecode, Clone)]
pub struct RecurringPattern {
    pub monthly_streak: u64,
    pub quarterly_streak: u64,
    pub last_donation_month: u64,  // Format: YYYYMM (e.g., 202601)
    pub last_donation_quarter: u64,  // Format: YYYYQ (e.g., 20261 for Q1)
}

impl RecurringPattern {
    pub fn default() -> Self {
        RecurringPattern {
            monthly_streak: 0,
            quarterly_streak: 0,
            last_donation_month: 0,
            last_donation_quarter: 0,
        }
    }
}

#[multiversx_sc::contract]
pub trait PhilanthrifyFactory {
    #[init]
    fn init(&self, global_admin: ManagedAddress) {
        self.global_admin_address().set(&global_admin);
        self.charity_count().set(0u64);
        self.project_count().set(0u64);
        self.nft_nonce().set(0u64);
        
        let stats = GlobalStats {
            total_donations_amount: BigUint::zero(),
            total_donations_count: 0u64,
            total_nfts_minted: 0u64,
        };
        self.global_statistics().set(&stats);
    }

    #[upgrade]
    fn upgrade(&self) {
        self.only_owner();
    }

    fn only_owner(&self) {
        let caller = self.blockchain().get_caller();
        let owner = self.global_admin_address().get();
        require!(caller == owner, "Only global admin allowed");
    }

    #[endpoint(deployCharity)]
    fn deploy_charity(&self, charity_name: ManagedBuffer) -> ManagedAddress {
        self.only_owner();

        let charity_template = self.charity_template().get();
        require!(!charity_template.is_zero(), "Charity template not set");

        let admin_address = self.global_admin_address().get();
        let factory_address = self.blockchain().get_sc_address();
        let project_template = self.project_template().get();

        let new_charity: ManagedAddress = self
            .tx()
            .raw_deploy()
            .from_source(charity_template)
            .code_metadata(
                CodeMetadata::PAYABLE
                    | CodeMetadata::PAYABLE_BY_SC
                    | CodeMetadata::UPGRADEABLE
                    | CodeMetadata::READABLE,
            )
            .argument(&charity_name)
            .argument(&factory_address)
            .argument(&admin_address)
            .argument(&project_template)
            .gas(80_000_000)
            .returns(ReturnsNewAddress)
            .sync_call()
            .into();

        let current_count = self.charity_count().get();
        self.charity_count().set(current_count + 1);

        self.charity_deployed(&charity_name, &new_charity);
        new_charity
    }

    #[endpoint(setCharityTemplate)]
    fn set_charity_template(&self, template_address: ManagedAddress) {
        self.only_owner();
        require!(!template_address.is_zero(), "Invalid template address");
        self.charity_template().set(&template_address);
    }

    #[endpoint(setProjectTemplate)]
    fn set_project_template(&self, template_address: ManagedAddress) {
        self.only_owner();
        require!(!template_address.is_zero(), "Invalid template address");
        self.project_template().set(&template_address);
    }

    #[payable("EGLD")]
    #[endpoint(issuePhilanthrifyNft)]
    fn issue_philanthrify_nft(&self) {
        self.only_owner();

        let issue_cost = BigUint::from(50_000_000_000_000_000u64);
        let payment = self.call_value().egld();

        require!(*payment == issue_cost, "Incorrect issue cost");

        // Issue NFT collection with properties that allow it to become dynamic
        // Note: Due to MultiversX cross-shard limitations, we cannot call registerDynamic
        // directly from the contract. The collection must be made dynamic manually after issuance
        // using: mxpy tx new --receiver="erd1qqqqqqqqqqqqqqqpqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqzllls8a5w6u" \
        //        --data="registerDynamic@<token_name_hex>@<token_ticker_hex>@4e4654@00" \
        //        --value=50000000000000000 --pem=./sohail.pem --send
        // 
        // However, we issue with can_upgrade and can_add_special_roles enabled
        // so that once dynamic, the factory can update NFTs even when they're in donor wallets
        self.send()
            .esdt_system_sc_proxy()
            .issue_non_fungible(
                issue_cost,
                &ManagedBuffer::from("PhilanthrifyDonor"),
                &ManagedBuffer::from("PHILXY"),
                NonFungibleTokenProperties {
                    can_freeze: true,
                    can_wipe: true,
                    can_pause: true,
                    can_change_owner: true,
                    can_upgrade: true,  // Required for making it dynamic
                    can_add_special_roles: true,  // Required for NFT roles
                    can_transfer_create_role: true,
                },
            )
            .async_call_and_exit();
    }

    // Callback for issue_non_fungible - stores the token ID automatically
    #[callback]
    fn issue_non_fungible_callback(&self, token_identifier: TokenIdentifier) {
        // Store the token ID automatically after issuance
        self.global_nft_collection().set(&token_identifier);
        
        // Emit event to notify owner that collection is issued
        // Roles will be granted manually from wallet using the deploy script
        // The factory is the token manager, but cannot grant roles to itself
        // due to MultiversX cross-shard limitations
        self.nft_collection_issued(&token_identifier);
    }

    #[endpoint(setGlobalNftCollection)]
    fn set_global_nft_collection(&self, nft_token_id: TokenIdentifier) {
        self.only_owner();
        self.global_nft_collection().set(&nft_token_id);
    }

    #[endpoint(grantNftRoleToFactory)]
    fn grant_nft_role_to_factory(&self) {
        self.only_owner();

        let nft_collection = self.global_nft_collection().get();
        require!(nft_collection.is_valid_esdt_identifier(), "NFT collection not set");

        let factory_address = self.blockchain().get_sc_address();
        let roles = [
            EsdtLocalRole::NftCreate,
            EsdtLocalRole::NftUpdateAttributes,  // Required for updating NFT attributes (works even if NFT is in donor's wallet)
            EsdtLocalRole::NftAddUri,
        ];

        self.send()
            .esdt_system_sc_proxy()
            .set_special_roles(&factory_address, &nft_collection, roles.iter().cloned())
            .async_call_and_exit();
    }

    #[endpoint(transferTokenManagerRole)]
    fn transfer_token_manager_role(&self, new_manager: ManagedAddress) {
        self.only_owner();

        let nft_collection = self.global_nft_collection().get();
        require!(nft_collection.is_valid_esdt_identifier(), "NFT collection not set");
        require!(!new_manager.is_zero(), "Invalid manager address");

        // Transfer token manager role to new manager
        // This allows the new manager to call changeToDynamic
        self.send()
            .esdt_system_sc_proxy()
            .transfer_ownership(&nft_collection, &new_manager)
            .async_call_and_exit();
    }

    #[endpoint(updateNftUri)]
    fn update_nft_uri(
        &self,
        nft_token_id: TokenIdentifier,
        nft_nonce: u64,
        uri: ManagedBuffer,
    ) {
        self.only_owner();

        require!(nft_token_id.is_valid_esdt_identifier(), "Invalid NFT token ID");

        // nft_add_uri expects a single ManagedBuffer by value, not a reference
        self.send()
            .nft_add_uri(&nft_token_id, nft_nonce, uri);
    }

    #[endpoint(updateNftAttributes)]
    fn update_nft_attributes(
        &self,
        nft_token_id: TokenIdentifier,
        nft_nonce: u64,
        attributes: ManagedBuffer,
    ) {
        self.only_owner();

        require!(nft_token_id.is_valid_esdt_identifier(), "Invalid NFT token ID");

        self.send()
            .nft_update_attributes(&nft_token_id, nft_nonce, &attributes);
    }

    // ============================================================
    // IPFS METADATA CID STORAGE (for tags format)
    // ============================================================

    #[storage_mapper("donor_nft_ipfs_cid")]
    fn donor_nft_ipfs_cid(&self) -> SingleValueMapper<ManagedBuffer>;

    #[storage_mapper("transaction_nft_ipfs_cid")]
    fn transaction_nft_ipfs_cid(&self) -> SingleValueMapper<ManagedBuffer>;

    #[storage_mapper("default_donor_image_uri")]
    fn default_donor_image_uri(&self) -> SingleValueMapper<ManagedBuffer>;

    #[endpoint(setDonorNftMetadata)]
    fn set_donor_nft_metadata(&self, ipfs_cid: ManagedBuffer) {
        self.only_owner();
        require!(!ipfs_cid.is_empty(), "IPFS CID cannot be empty");
        self.donor_nft_ipfs_cid().set(&ipfs_cid);
    }

    /// Clear donor metadata CID so Attributes/Tags come only from on-chain (tags:...;traits:[...]). Call this if you want no Pinata JSON.
    #[endpoint(clearDonorNftMetadata)]
    fn clear_donor_nft_metadata(&self) {
        self.only_owner();
        self.donor_nft_ipfs_cid().clear();
    }

    #[endpoint(setTransactionNftMetadata)]
    fn set_transaction_nft_metadata(&self, ipfs_cid: ManagedBuffer) {
        self.only_owner();
        require!(!ipfs_cid.is_empty(), "IPFS CID cannot be empty");
        self.transaction_nft_ipfs_cid().set(&ipfs_cid);
    }

    /// First URI for new donor NFTs when set. Use a static image URL so we never put tier images in Assets
    /// (MultiversX cannot remove URIs). Current tier image is only in attributes ;image:.
    #[endpoint(setDefaultDonorImageUri)]
    fn set_default_donor_image_uri(&self, uri: ManagedBuffer) {
        self.only_owner();
        self.default_donor_image_uri().set(&uri);
    }

    // ============================================================
    // DONATION NFT MINTING
    // ============================================================
    #[endpoint(mintNft)]
    fn mint_nft(
        &self,
        donor_address: ManagedAddress,
        display_amount: BigUint,  // Display amount only (for NFT name), not actual EGLD
        entity_name: ManagedBuffer,
        entity_type: ManagedBuffer,
        user_image_uri: ManagedBuffer,  // Optional user image (CID or full URL) - empty string means no image
        custom_tags: MultiValueEncoded<ManagedBuffer>,  // Must be last (var-args)
    ) {
        let nft_token_id = self.global_nft_collection().get();
        require!(nft_token_id.is_valid_esdt_identifier(), "NFT collection not set");

        let current_donation_count = self.donor_donations_to_entity(&donor_address, &entity_name).get();
        let new_donation_count = current_donation_count + 1;

        // Track total amount per donor-entity
        let current_total_amount = self.donor_total_amount_to_entity(&donor_address, &entity_name).get();
        let new_total_amount = current_total_amount + &display_amount;
        self.donor_total_amount_to_entity(&donor_address, &entity_name).set(&new_total_amount);

        let tier_level = self.calculate_tier_for_entity(new_donation_count);
        let tier_name = self.get_tier_name(tier_level);

        // GAMIFICATION: Check patron status and update recurring patterns
        let patron_rank = self.check_and_update_patrons(&donor_address, &entity_name, &new_total_amount);
        let recurring_pattern = self.update_recurring_patterns(&donor_address, &entity_name);

        let mut user_tags = ManagedVec::new();
        for tag in custom_tags.into_iter() {
            if !tag.is_empty() && user_tags.len() < 10 {
                user_tags.push(tag);
            }
        }

        // Get registry - VecMapper uses 1-based indexing!
        let mut registry = self.donor_nft_registry_for_entity(&donor_address, &entity_name);
        
        // Check if registry has any NFTs - VecMapper.len() returns the count
        // VecMapper indices are 1-based, so valid indices are 1 to len()
        let registry_len = registry.len();
        
        // Determine if we need to create new NFT or update existing one
        let target_nonce = if registry_len > 0 {
            // Registry has NFT - update the existing one
            // VecMapper uses 1-based indexing: get the latest NFT (last in registry)
            let existing_nonce = registry.get(registry_len);
            
            // Read old tier before we overwrite metadata (for tier-change check)
            let metadata_mapper_pre = self.nft_metadata_record(existing_nonce);
            let old_tier = if metadata_mapper_pre.is_empty() {
                0u64
            } else {
                metadata_mapper_pre.get().tier_level
            };
            
            let tier_image_uri = self.get_tier_base_image_uri(tier_level);
            let tier_changed = old_tier != tier_level;
            
            // Create updated attributes with new donation count, gamification, and ;image: tier URI
            let updated_attributes = self.create_donation_nft_attributes(
                &entity_name,
                &entity_type,
                new_donation_count,
                tier_level,
                &tier_name,
                &user_tags,
                &new_total_amount,
                patron_rank,
                Some(&recurring_pattern),
                tier_image_uri.clone(),
            );

            // Always update NFT (it stays on contract for dynamic updates)
            // Verify NFT is on contract before updating
            let contract_address = self.blockchain().get_sc_address();
            let contract_balance = self.blockchain().get_esdt_balance(
                &contract_address,
                &nft_token_id,
                existing_nonce,
            );
            
            if contract_balance > 0u32 {
                // NFT is on contract - update on-chain attributes so tags/traits/tier image upgrade
                self.send().nft_update_attributes(
                    &nft_token_id,
                    existing_nonce,
                    &updated_attributes,
                );
                self.nft_attributes_updated(&donor_address, &entity_name, existing_nonce);

                // When tier changes, add the new tier image as a URI so the explorer can show it (many use last URI for display).
                if tier_changed {
                    if let Some(ref uri) = tier_image_uri {
                        if !uri.is_empty() {
                            self.send().nft_add_uri(&nft_token_id, existing_nonce, uri.clone());
                        }
                    }
                }

                // Patron image: when rank 1 → add top1; when rank 2–10 → add rest. When patron #2 becomes #1, add top1 so it shows.
                if let Some(rank) = patron_rank {
                    let want_type = if rank == 1 { 1u64 } else { 2u64 };
                    let added_type = self.patron_image_type_added(existing_nonce).get();
                    let legacy_has_image = added_type == 0 && self.has_patron_uri(existing_nonce).get();
                    let should_add = added_type != want_type
                        && (want_type == 1 || !legacy_has_image);  // always add top1 when now #1; for rest skip only if legacy
                    if should_add {
                        if let Some(patron_uri) = self.get_patron_badge_uri(rank) {
                            self.send().nft_add_uri(&nft_token_id, existing_nonce, patron_uri);
                            self.patron_image_type_added(existing_nonce).set(want_type);
                            self.has_patron_uri(existing_nonce).set(true);
                        }
                    }
                }

                // Add user image URI if provided (appends to history)
                if !user_image_uri.is_empty() {
                    if let Some(formatted_uri) = self.format_user_image_uri(&user_image_uri) {
                        self.send().nft_add_uri(&nft_token_id, existing_nonce, formatted_uri);
                    }
                }
            }
            // If NFT is not on contract (in wallet), it cannot be updated dynamically
            // This is expected for old NFTs that were sent to wallets before the update

            // Update or create metadata record with fresh data
            let is_on_contract = contract_balance > 0u32;
            let metadata = if metadata_mapper_pre.is_empty() {
                // Metadata doesn't exist yet (for old NFTs) - create it
                NftMetadataRecord {
                    nft_nonce: existing_nonce,
                    donor_address: donor_address.clone(),
                    entity_name: entity_name.clone(),
                    entity_type: entity_type.clone(),
                    donation_count: new_donation_count,
                    tier_level,
                    total_amount: display_amount.clone(),
                    last_updated: self.blockchain().get_block_timestamp(),
                    is_on_contract,
                }
            } else {
                // Metadata exists - update it
                let mut existing_metadata = metadata_mapper_pre.get();
                existing_metadata.donation_count = new_donation_count;
                existing_metadata.tier_level = tier_level;
                existing_metadata.total_amount = new_total_amount.clone();
                existing_metadata.last_updated = self.blockchain().get_block_timestamp();
                existing_metadata.is_on_contract = is_on_contract;
                existing_metadata
            };
            metadata_mapper_pre.set(&metadata);

            // When ranks change, update other patrons' NFTs (tags + patron image) so e.g. old #1 shows #patron_2
            self.update_other_patrons_nfts(&donor_address, &entity_name, &entity_type, &nft_token_id);

            existing_nonce
        } else {
            // Registry is empty - create new NFT
            let current_nonce = self.nft_nonce().get();
            let new_nonce = current_nonce + 1;

            // NFT name: "Philanthrify Donor Badge"
            let nft_name = ManagedBuffer::from(b"Philanthrify Donor Badge");

            // Tier image URI required for ;image: in attributes (display); we do NOT add it to URIs
            // so that after upgrade to Silver, Bronze IPFS is not in Assets (only current tier in attributes).
            let tier_image_uri = self.get_tier_base_image_uri(tier_level)
                .unwrap_or_else(|| sc_panic!("Tier image URI not configured. Run SET_IMAGE_URIS.sh first."));

            let attrs = self.create_donation_nft_attributes(
                &entity_name,
                &entity_type,
                new_donation_count,
                tier_level,
                &tier_name,
                &user_tags,
                &display_amount,
                patron_rank,
                Some(&recurring_pattern),
                Some(tier_image_uri.clone()),
            );

            let royalties = BigUint::from(500u32);
            let hash = ManagedBuffer::new();

            // URIs: first = tier (or default), then patron image if donor is in top 10 (so it shows in explorer).
            let mut uris = ManagedVec::new();
            let default_uri = self.default_donor_image_uri().get();
            let first_uri = if !default_uri.is_empty() {
                default_uri
            } else {
                tier_image_uri.clone()
            };
            uris.push(first_uri);
            // Patron image: when donor is in top 10 for this entity, add patron badge image (rank 1 = top1, 2–10 = rest).
            if let Some(rank) = patron_rank {
                if let Some(patron_uri) = self.get_patron_badge_uri(rank) {
                    uris.push(patron_uri);
                }
            }
            if !user_image_uri.is_empty() {
                if let Some(formatted_uri) = self.format_user_image_uri(&user_image_uri) {
                    uris.push(formatted_uri);
                }
            }

            let created_nonce = self.send().esdt_nft_create(
                &nft_token_id,
                &BigUint::from(1u32),
                &nft_name,
                &royalties,
                &hash,
                &attrs,
                &uris,
            );

            if let Some(rank) = patron_rank {
                let t = if rank == 1 { 1u64 } else { 2u64 };
                self.patron_image_type_added(created_nonce).set(t);
                self.has_patron_uri(created_nonce).set(true);
            }

            // Add to registry
            registry.push(&created_nonce);
            self.nft_nonce().set(new_nonce);
            self.nft_minted(&donor_address, &entity_name, created_nonce);

            // Store metadata record for new NFT
            let metadata = NftMetadataRecord {
                nft_nonce: created_nonce,
                donor_address: donor_address.clone(),
                entity_name: entity_name.clone(),
                entity_type: entity_type.clone(),
                donation_count: new_donation_count,
                tier_level,
                total_amount: display_amount.clone(),
                last_updated: self.blockchain().get_block_timestamp(),
                is_on_contract: true,
            };
            self.nft_metadata_record(created_nonce).set(&metadata);

            // DON'T send NFT to donor - it stays on contract for dynamic updates
            
            created_nonce
        };

        let donation_record = DonationRecord {
            amount: display_amount.clone(),  // Store display amount for records
            timestamp: self.blockchain().get_block_timestamp(),
            entity_name: entity_name.clone(),
            entity_type: entity_type.clone(),
            nft_nonce: target_nonce,
        };

        self.donor_donation_history(&donor_address).push(&donation_record);
        self.entity_donation_history(&entity_name).push(&donation_record);
        self.entity_type_donation_history(&entity_type).push(&donation_record);

        self.donor_donations_to_entity(&donor_address, &entity_name).set(new_donation_count);

        // Don't update stats with display amount (it's not real EGLD)
        // self.update_donation_stats(&display_amount, minted_new_nft);

        self.donation_recorded(&donor_address, &display_amount, &entity_name);
    }

    // ============================================================
    // TRANSACTION NFT MINTING (NEW)
    // ============================================================
    #[endpoint(mintTransactionNft)]
    fn mint_transaction_nft(
        &self,
        entity_owner: ManagedAddress,
        display_amount: BigUint,  // Display amount only (for NFT name), not actual EGLD
        entity_name: ManagedBuffer,
        entity_type: ManagedBuffer,
        category: ManagedBuffer,
        description: ManagedBuffer,
        user_image_uri: ManagedBuffer,  // Optional user image (CID or full URL) - empty string means no image
    ) {
        let nft_token_id = self.global_nft_collection().get();
        require!(nft_token_id.is_valid_esdt_identifier(), "NFT collection not set");

        // Track transaction statistics per entity
        let current_total_amount = self.entity_transaction_total(&entity_name).get();
        let new_total_amount = current_total_amount + &display_amount;
        self.entity_transaction_total(&entity_name).set(&new_total_amount);

        let current_transaction_count = self.entity_transaction_count(&entity_name).get();
        let new_transaction_count = current_transaction_count + 1;
        self.entity_transaction_count(&entity_name).set(new_transaction_count);

        // Get or create transaction NFT for this entity (ONE NFT PER ENTITY)
        let entity_transaction_nft = self.entity_transaction_nft(&entity_name);
        let existing_nonce_opt = entity_transaction_nft.get();
        
        let target_nonce = if existing_nonce_opt == 0 {
            // No NFT exists - create new one
        let current_nonce = self.nft_nonce().get();
        let new_nonce = current_nonce + 1;

            // NFT name: "Philanthrify Transaction Receipt"
            let nft_name = ManagedBuffer::from(b"Philanthrify Transaction Receipt");

            // Create attributes with aggregated data
            let attrs = self.create_transaction_nft_attributes_aggregated(
            &entity_name,
            &entity_type,
                &new_total_amount,
                new_transaction_count,
            &category,
            &description,
        );

        let royalties = BigUint::from(500u32);
        let hash = ManagedBuffer::new();

        // Add image URIs: first = display image on explorer. User upload = first if provided; else default badge.
        let default_transaction_image = ManagedBuffer::from(b"https://ipfs.io/ipfs/bafybeicqtbhfnonjy7hfddbsd6cpbeu3vbjk3ysjaddy7m2dnpng52hmae");
        let mut uris = ManagedVec::new();
        if !user_image_uri.is_empty() {
            if let Some(formatted_uri) = self.format_user_image_uri(&user_image_uri) {
                uris.push(formatted_uri);
            }
        }
        if uris.is_empty() {
            uris.push(default_transaction_image);
        }

        let created_nonce = self.send().esdt_nft_create(
            &nft_token_id,
            &BigUint::from(1u32),
            &nft_name,
            &royalties,
            &hash,
            &attrs,
            &uris,
        );

            // Store the nonce
            entity_transaction_nft.set(&created_nonce);
        self.nft_nonce().set(new_nonce);

        let mut stats = self.global_statistics().get();
        stats.total_nfts_minted += 1;
        self.global_statistics().set(&stats);

            // Store metadata record for new transaction NFT
            let metadata = NftMetadataRecord {
                nft_nonce: created_nonce,
                donor_address: entity_owner.clone(),
                entity_name: entity_name.clone(),
                entity_type: entity_type.clone(),
                donation_count: new_transaction_count,
                tier_level: 0,
                total_amount: new_total_amount.clone(),
                last_updated: self.blockchain().get_block_timestamp(),
                is_on_contract: true,
            };
            self.nft_metadata_record(created_nonce).set(&metadata);

            created_nonce
        } else {
            // NFT exists - update it
            let existing_nonce = existing_nonce_opt;

            // Create updated attributes with new aggregated data
            let updated_attrs = self.create_transaction_nft_attributes_aggregated(
                &entity_name,
                &entity_type,
                &new_total_amount,
                new_transaction_count,
                &category,
                &description,
            );

            // Always update NFT (it stays on contract for dynamic updates)
            // Verify NFT is on contract before updating
            let contract_address = self.blockchain().get_sc_address();
            let contract_balance = self.blockchain().get_esdt_balance(
                &contract_address,
                &nft_token_id,
                existing_nonce,
            );
            
            // Only update if NFT is on contract (avoid errors if NFT was transferred)
            if contract_balance > 0u32 {
                // NFT is on contract - update it immediately
                self.send().nft_update_attributes(
                    &nft_token_id,
                    existing_nonce,
                    &updated_attrs,
                );
                
                // Add user image URI if provided (appends to history)
                if !user_image_uri.is_empty() {
                    if let Some(formatted_uri) = self.format_user_image_uri(&user_image_uri) {
                        self.send().nft_add_uri(&nft_token_id, existing_nonce, formatted_uri);
                    }
                }
            }
            // If NFT is not on contract, skip update (it may have been transferred to wallet)

            // Update or create metadata record
            let metadata_mapper = self.nft_metadata_record(existing_nonce);
            let is_on_contract = contract_balance > 0u32;
            let metadata = if metadata_mapper.is_empty() {
                // Metadata doesn't exist yet (for old NFTs) - create it
                NftMetadataRecord {
                    nft_nonce: existing_nonce,
                    donor_address: entity_owner.clone(),
                    entity_name: entity_name.clone(),
                    entity_type: entity_type.clone(),
                    donation_count: new_transaction_count,
                    tier_level: 0,
                    total_amount: new_total_amount.clone(),
                    last_updated: self.blockchain().get_block_timestamp(),
                    is_on_contract,
                }
            } else {
                // Metadata exists - update it
                let mut existing_metadata = metadata_mapper.get();
                existing_metadata.donation_count = new_transaction_count;
                existing_metadata.total_amount = new_total_amount.clone();
                existing_metadata.last_updated = self.blockchain().get_block_timestamp();
                existing_metadata.is_on_contract = is_on_contract;
                existing_metadata
            };
            metadata_mapper.set(&metadata);

            // DON'T send NFT to owner - it stays on contract for dynamic updates

            existing_nonce
        };

        self.transaction_nft_minted(&entity_name, &entity_type, target_nonce);
    }

    // ============================================================
    // HELPER: Format User Image URI
    // ============================================================
    // Accepts both full URL (https://ipfs.io/ipfs/CID) and CID only
    // Returns formatted URI ready for NFT
    fn format_user_image_uri(&self, user_image_input: &ManagedBuffer) -> Option<ManagedBuffer> {
        if user_image_input.is_empty() {
            return None;
        }

        let input_bytes = user_image_input.to_boxed_bytes();
        let input_slice = input_bytes.as_slice();
        
        // Check if it's already a full URL (starts with http:// or https://)
        if input_slice.starts_with(b"http://") || input_slice.starts_with(b"https://") {
            // Already a full URL, return as-is (clone)
            return Some(user_image_input.clone());
        }
        
        // Otherwise, treat as CID and format as IPFS URL
        let ipfs_url = ManagedBuffer::from(b"https://ipfs.io/ipfs/");
        let mut formatted = ManagedBuffer::new();
        formatted.append(&ipfs_url);
        formatted.append(user_image_input);
        Some(formatted)
    }

    /// Returns the donor metadata JSON URL (https://ipfs.io/ipfs/CID) if donor_nft_ipfs_cid is set.
    /// Used as first NFT URI so we don't put tier images in Assets (no Bronze left after Silver).
    fn get_donor_metadata_url(&self) -> Option<ManagedBuffer> {
        let cid = self.donor_nft_ipfs_cid().get();
        if cid.is_empty() {
            return None;
        }
        let base = ManagedBuffer::from(b"https://ipfs.io/ipfs/");
        let mut out = ManagedBuffer::new();
        out.append(&base);
        out.append(&cid);
        Some(out)
    }

    // ============================================================
    // NFT ATTRIBUTE CREATORS
    // ============================================================

    fn create_donation_nft_attributes(
        &self,
        entity_name: &ManagedBuffer,
        entity_type: &ManagedBuffer,
        donation_count_to_entity: u64,
        tier_level: u64,
        tier_name: &ManagedBuffer,
        user_tags: &ManagedVec<Self::Api, ManagedBuffer>,
        total_amount: &BigUint,  // Used for donated amount tag
        patron_rank: Option<u64>,
        recurring_pattern: Option<&RecurringPattern>,
        tier_image_uri: Option<ManagedBuffer>,  // Optional; when set, add ;image:url for explorers/frontends
    ) -> ManagedBuffer {
        let mut attributes = ManagedBuffer::new();

        // ALWAYS include metadata reference - NEVER removed on updates
        // Format: metadata:ipfsCID;tags:tag1,tag2,tag3
        // Note: CID points directly to the JSON file (no /metadata.json needed)
        let ipfs_cid = self.donor_nft_ipfs_cid().get();
        if !ipfs_cid.is_empty() {
            attributes.append(&ManagedBuffer::from(b"metadata:"));
            attributes.append(&ipfs_cid);
            attributes.append(&ManagedBuffer::from(b";tags:"));
        } else {
            // Fallback: if IPFS CID not set, use tags: format only
            attributes.append(&ManagedBuffer::from(b"tags:"));
        }

        // Build tags list (comma-separated)
        let mut needs_comma = false;

        // Add user tags if any
        if user_tags.len() > 0 {
            for tag in user_tags.iter() {
                if needs_comma {
                    attributes.append(&ManagedBuffer::from(b","));
                }
                attributes.append(&tag);
                needs_comma = true;
            }
        }

        // Add dynamic tags based on donation info
        if needs_comma {
            attributes.append(&ManagedBuffer::from(b","));
        }
        attributes.append(&ManagedBuffer::from(b"donation"));
        attributes.append(&self.u64_to_buffer(donation_count_to_entity));

        attributes.append(&ManagedBuffer::from(b",donated$"));
        attributes.append(&self.u64_to_buffer(total_amount.to_u64().unwrap_or(0)));

        attributes.append(&ManagedBuffer::from(b","));
        attributes.append(entity_name);

        attributes.append(&ManagedBuffer::from(b","));
        attributes.append(entity_type);

        // Tier (position 5): #bronze so it shows
        attributes.append(&ManagedBuffer::from(b","));
        attributes.append(&self.get_tier_tag_lower(tier_level));

        // Patron as 6th tag (explorer only shows first 6): #patron_1 or #supporter so patron shows
        if let Some(rank) = patron_rank {
            attributes.append(&ManagedBuffer::from(b",patron"));
            attributes.append(&ManagedBuffer::from(b","));
            attributes.append(&self.get_patron_rank_tag(rank));
            attributes.append(&ManagedBuffer::from(b",patron_rank:"));
            attributes.append(&self.u64_to_buffer(rank));
        } else {
            attributes.append(&ManagedBuffer::from(b",supporter"));
            attributes.append(&ManagedBuffer::from(b",patron_rank:0"));
        }

        // tier:Name after patron (still in string for API / metadata JSON)
        attributes.append(&ManagedBuffer::from(b",tier:"));
        attributes.append(tier_name);

        // Add recurring pattern tags
        if let Some(pattern) = recurring_pattern {
            if pattern.monthly_streak > 1 {
                attributes.append(&ManagedBuffer::from(b",recurring_monthly:"));
                attributes.append(&self.u64_to_buffer(pattern.monthly_streak));
            }
            if pattern.quarterly_streak > 1 {
                attributes.append(&ManagedBuffer::from(b",recurring_quarterly:"));
                attributes.append(&self.u64_to_buffer(pattern.quarterly_streak));
            }
        }

        // Always add default tags
        attributes.append(&ManagedBuffer::from(b",philanthrify"));
        attributes.append(&ManagedBuffer::from(b",charity"));
        attributes.append(&ManagedBuffer::from(b",blockchain"));
        attributes.append(&ManagedBuffer::from(b",transparency"));
        attributes.append(&ManagedBuffer::from(b",impact"));

        // Gamification: add image URI for UIs that read attributes (tier-specific display image)
        if let Some(uri) = tier_image_uri {
            if !uri.is_empty() {
                attributes.append(&ManagedBuffer::from(b";image:"));
                attributes.append(&uri);
            }
        }

        // All traits on-chain: Patron and Patron Rank right after Tier so explorer shows them in Attributes
        let total_str = self.u64_to_buffer(total_amount.to_u64().unwrap_or(0));
        let monthly = recurring_pattern.map(|p| p.monthly_streak).unwrap_or(0);
        let quarterly = recurring_pattern.map(|p| p.quarterly_streak).unwrap_or(0);
        // No website link; platform/project attributes: Platform, Blockchain, Impact, Transparency, Badge, Status, Tier, Patron...
        attributes.append(&ManagedBuffer::from(b";traits:[{\"trait_type\":\"Platform\",\"value\":\"Philanthrify\"},{\"trait_type\":\"Blockchain\",\"value\":\"MultiversX\"},{\"trait_type\":\"Badge Type\",\"value\":\"Donor Badge\"},{\"trait_type\":\"Status\",\"value\":\"Active\"},{\"trait_type\":\"Impact\",\"value\":\"Verified\"},{\"trait_type\":\"Transparency\",\"value\":\"On-chain\"},{\"trait_type\":\"Tier\",\"value\":\""));
        attributes.append(tier_name);
        // Patron and Patron Rank immediately after Tier so they show in explorer Attributes
        if let Some(rank) = patron_rank {
            attributes.append(&ManagedBuffer::from(b"\"},{\"trait_type\":\"Patron\",\"value\":\"Yes\"},{\"trait_type\":\"Patron Rank\",\"value\":\""));
            attributes.append(&self.u64_to_buffer(rank));
            attributes.append(&ManagedBuffer::from(b"\"},{\"trait_type\":\"Donation Count\",\"value\":\""));
        } else {
            attributes.append(&ManagedBuffer::from(b"\"},{\"trait_type\":\"Patron\",\"value\":\"No\"},{\"trait_type\":\"Patron Rank\",\"value\":\"-\"},{\"trait_type\":\"Donation Count\",\"value\":\""));
        }
        attributes.append(&self.u64_to_buffer(donation_count_to_entity));
        attributes.append(&ManagedBuffer::from(b"\"},{\"trait_type\":\"Total Donated\",\"value\":\""));
        attributes.append(&total_str);
        attributes.append(&ManagedBuffer::from(b"\"},{\"trait_type\":\"Monthly Streak\",\"value\":\""));
        attributes.append(&self.u64_to_buffer(monthly));
        attributes.append(&ManagedBuffer::from(b"\"},{\"trait_type\":\"Quarterly Streak\",\"value\":\""));
        attributes.append(&self.u64_to_buffer(quarterly));
        attributes.append(&ManagedBuffer::from(b"\"}]"));

        attributes
    }

    fn create_transaction_nft_attributes(
        &self,
        entity_name: &ManagedBuffer,
        entity_type: &ManagedBuffer,
        amount: &BigUint,
        category: &ManagedBuffer,
        description: &ManagedBuffer,
    ) -> ManagedBuffer {
        let mut attributes = ManagedBuffer::new();

        attributes.append(&ManagedBuffer::from(b"["));

        attributes.append(&ManagedBuffer::from(b"{\"trait_type\":\"Type\",\"value\":\"Transaction\"},"));

        attributes.append(&ManagedBuffer::from(b"{\"trait_type\":\"Entity\",\"value\":\""));
        attributes.append(entity_name);
        attributes.append(&ManagedBuffer::from(b"\"},"));

        attributes.append(&ManagedBuffer::from(b"{\"trait_type\":\"EntityType\",\"value\":\""));
        attributes.append(entity_type);
        attributes.append(&ManagedBuffer::from(b"\"},"));

        attributes.append(&ManagedBuffer::from(b"{\"trait_type\":\"Amount\",\"value\":\""));
        attributes.append(&self.u64_to_buffer(amount.to_u64().unwrap_or(0)));
        attributes.append(&ManagedBuffer::from(b"\"},"));

        attributes.append(&ManagedBuffer::from(b"{\"trait_type\":\"Category\",\"value\":\""));
        attributes.append(category);
        attributes.append(&ManagedBuffer::from(b"\"},"));

        attributes.append(&ManagedBuffer::from(b"{\"trait_type\":\"Description\",\"value\":\""));
        attributes.append(description);
        attributes.append(&ManagedBuffer::from(b"\"}"));

        attributes.append(&ManagedBuffer::from(b"]"));

        attributes
    }

    fn create_transaction_nft_attributes_aggregated(
        &self,
        entity_name: &ManagedBuffer,
        entity_type: &ManagedBuffer,
        total_amount: &BigUint,
        transaction_count: u64,
        _latest_category: &ManagedBuffer,  // Not used - removed from attributes per user request
        _latest_description: &ManagedBuffer,  // Not used - removed from attributes per user request
    ) -> ManagedBuffer {
        let mut attributes = ManagedBuffer::new();

        // ALWAYS include metadata reference - NEVER removed on updates
        // Format: metadata:ipfsCID;tags:tag1,tag2,tag3
        // Note: CID points directly to the JSON file (no /metadata.json needed)
        let ipfs_cid = self.transaction_nft_ipfs_cid().get();
        if !ipfs_cid.is_empty() {
            attributes.append(&ManagedBuffer::from(b"metadata:"));
            attributes.append(&ipfs_cid);
            attributes.append(&ManagedBuffer::from(b";tags:"));
        } else {
            // Fallback: if IPFS CID not set, use tags: format only
            attributes.append(&ManagedBuffer::from(b"tags:"));
        }

        // Build tags: dynamic (count, amount, name) + platform (receipt, philanthrify, entity type, transparency, impact)
        attributes.append(&ManagedBuffer::from(b"transaction"));
        attributes.append(&self.u64_to_buffer(transaction_count));
        attributes.append(&ManagedBuffer::from(b",spent$"));
        attributes.append(&self.u64_to_buffer(total_amount.to_u64().unwrap_or(0)));
        attributes.append(&ManagedBuffer::from(b","));
        attributes.append(entity_name);
        attributes.append(&ManagedBuffer::from(b",receipt"));
        attributes.append(&ManagedBuffer::from(b",philanthrify"));
        attributes.append(&ManagedBuffer::from(b","));
        attributes.append(entity_type);
        attributes.append(&ManagedBuffer::from(b",transparency"));
        attributes.append(&ManagedBuffer::from(b",impact"));

        attributes
    }

    // ============================================================
    // TIER SYSTEM (SIMPLIFIED - 4 TIERS ONLY)
    // ============================================================

    fn calculate_tier_for_entity(&self, donation_count: u64) -> u64 {
        // Simplified tier system with 4 tiers (no Legendary)
        if donation_count >= 11 {
            4u64  // Platinum (11+ donations)
        } else if donation_count >= 6 {
            3u64  // Gold (6-10 donations)
        } else if donation_count >= 3 {
            2u64  // Silver (3-5 donations)
        } else {
            1u64  // Bronze (1-2 donations)
        }
    }

    fn get_tier_name(&self, tier: u64) -> ManagedBuffer {
        match tier {
            4 => ManagedBuffer::from(b"Platinum"),
            3 => ManagedBuffer::from(b"Gold"),
            2 => ManagedBuffer::from(b"Silver"),
            _ => ManagedBuffer::from(b"Bronze"),
        }
    }

    /// Lowercase tier tag for explorer Tags (#bronze, #silver, #gold, #platinum)
    fn get_tier_tag_lower(&self, tier: u64) -> ManagedBuffer {
        match tier {
            4 => ManagedBuffer::from(b"platinum"),
            3 => ManagedBuffer::from(b"gold"),
            2 => ManagedBuffer::from(b"silver"),
            _ => ManagedBuffer::from(b"bronze"),
        }
    }

    /// Patron rank as single tag so explorer shows #patron_1 .. #patron_10 (like #bronze)
    fn get_patron_rank_tag(&self, rank: u64) -> ManagedBuffer {
        match rank {
            1 => ManagedBuffer::from(b"patron_1"),
            2 => ManagedBuffer::from(b"patron_2"),
            3 => ManagedBuffer::from(b"patron_3"),
            4 => ManagedBuffer::from(b"patron_4"),
            5 => ManagedBuffer::from(b"patron_5"),
            6 => ManagedBuffer::from(b"patron_6"),
            7 => ManagedBuffer::from(b"patron_7"),
            8 => ManagedBuffer::from(b"patron_8"),
            9 => ManagedBuffer::from(b"patron_9"),
            _ => ManagedBuffer::from(b"patron_10"),
        }
    }

    /// Hardcoded default tier image URLs (ipfs.io) so contract works with zero config. Owner can override via setTierImageUri.
    fn default_tier_image_uri(&self, tier: u64) -> ManagedBuffer {
        match tier {
            1 => ManagedBuffer::from(b"https://ipfs.io/ipfs/bafybeiabacxg5gtzrrobsgnc4ghpln2urz7hfnxalwwnnvuvo3figzurgy"),
            2 => ManagedBuffer::from(b"https://ipfs.io/ipfs/bafkreigwv5olvqvofh7mxfdv62owz5oomvkg7uljgsi5lpu6lt25uwkoau"),
            3 => ManagedBuffer::from(b"https://ipfs.io/ipfs/bafybeid3vl2h3jmnlrus77zly3tzymb7c7rmem4tuwmeazk2ffwv4jqsnu"),
            4 => ManagedBuffer::from(b"https://ipfs.io/ipfs/bafybeia5ghx4ahml7z4o3uu3z5kdou3ldxilgso7m5kncuhi5uqjwb5y3e"),
            _ => ManagedBuffer::new(),
        }
    }

    /// Hardcoded default patron image URLs. Owner can override via setPatronImageUri.
    fn default_patron_image_uri(&self, rank: u64) -> ManagedBuffer {
        if rank == 1 {
            ManagedBuffer::from(b"https://ipfs.io/ipfs/bafybeic4uhivpvz2ohbvg6nqh3dnt6vqfz7mxmh7uikhdeumg6t3mcfsay")
        } else {
            ManagedBuffer::from(b"https://ipfs.io/ipfs/bafkreiecqk4qxbxvwmkp37b4nwx6evax2e6rxho4hcekpjlrxd4gmuoe6q")
        }
    }

    fn get_tier_base_image_uri(&self, tier: u64) -> Option<ManagedBuffer> {
        match tier {
            1 => {
                let mapper = self.tier_bronze_image_uri();
                if mapper.is_empty() { Some(self.default_tier_image_uri(1)) } else { Some(mapper.get()) }
            },
            2 => {
                let mapper = self.tier_silver_image_uri();
                if mapper.is_empty() { Some(self.default_tier_image_uri(2)) } else { Some(mapper.get()) }
            },
            3 => {
                let mapper = self.tier_gold_image_uri();
                if mapper.is_empty() { Some(self.default_tier_image_uri(3)) } else { Some(mapper.get()) }
            },
            4 => {
                let mapper = self.tier_platinum_image_uri();
                if mapper.is_empty() { Some(self.default_tier_image_uri(4)) } else { Some(mapper.get()) }
            },
            _ => None,
        }
    }

    fn get_patron_badge_uri(&self, rank: u64) -> Option<ManagedBuffer> {
        if rank == 1 {
            let mapper = self.patron_top1_image_uri();
            if mapper.is_empty() { Some(self.default_patron_image_uri(1)) } else { Some(mapper.get()) }
        } else {
            let mapper = self.patron_rest_image_uri();
            if mapper.is_empty() { Some(self.default_patron_image_uri(2)) } else { Some(mapper.get()) }
        }
    }

    // ============================================================
    // PROJECT PATRON SYSTEM
    // ============================================================

    fn check_and_update_patrons(
        &self,
        donor_address: &ManagedAddress,
        entity_name: &ManagedBuffer,
        total_amount: &BigUint,
    ) -> Option<u64> {
        // Get current patron list for this project
        let mut patrons = self.project_patrons(&entity_name);
        
        // Check if donor is already a patron
        let mut existing_index: Option<usize> = None;
        for i in 1..=patrons.len() {
            let patron = patrons.get(i);
            if patron.donor_address == *donor_address {
                existing_index = Some(i);
                break;
            }
        }

        // Update or add patron
        let current_timestamp = self.blockchain().get_block_timestamp();
        
        if let Some(index) = existing_index {
            // Update existing patron
            let mut patron = patrons.get(index);
            patron.total_amount = total_amount.clone();
            // Rank will be recalculated below
            patrons.set(index, &patron);
        } else {
            // Add new patron if list is not full
            if patrons.len() < 10 {
                let new_patron = PatronRecord {
                    donor_address: donor_address.clone(),
                    total_amount: total_amount.clone(),
                    patron_rank: 0,  // Will be set after sorting
                    since_timestamp: current_timestamp,
                };
                patrons.push(&new_patron);
            } else {
                // Check if new donor qualifies (has more than lowest patron)
                // Find the minimum amount patron
                let mut min_amount_opt: Option<BigUint> = None;
                let mut min_index = 0usize;
                
                for i in 1..=patrons.len() {
                    let patron = patrons.get(i);
                    if min_amount_opt.is_none() || patron.total_amount < *min_amount_opt.as_ref().unwrap() {
                        min_amount_opt = Some(patron.total_amount.clone());
                        min_index = i;
                    }
                }
                
                // Replace lowest patron if new donor has more
                if let Some(min_amount) = min_amount_opt {
                    if total_amount > &min_amount {
                        let new_patron = PatronRecord {
                            donor_address: donor_address.clone(),
                            total_amount: total_amount.clone(),
                            patron_rank: 0,
                            since_timestamp: current_timestamp,
                        };
                        patrons.set(min_index, &new_patron);
                    } else {
                        // Don't qualify for patron status
                        return None;
                    }
                } else {
                    // No patrons yet, but list is full (shouldn't happen)
                    return None;
                }
            }
        }

        // Sort patrons by total_amount (descending) and assign ranks
        // Since we can't use Vec or ManagedVec with PatronRecord, we'll rebuild in sorted order
        // by repeatedly finding the max and rebuilding the list directly
        let patron_count = patrons.len();
        
        // Use temporary storage for sorted patrons
        let mut sorted_patrons = self.temp_sorted_patrons();
        sorted_patrons.clear();
        
        // Track which indices we've already processed
        let mut processed: ManagedVec<Self::Api, u64> = ManagedVec::new();
        
        // Find max patron each time and add to sorted list
        for rank in 1..=patron_count {
            let mut max_idx = 0usize;
            let mut max_amount_opt: Option<BigUint> = None;
            
            // Find max from unprocessed patrons
            for i in 1..=patron_count {
                // Check if already processed (ManagedVec is 0-based)
                let mut already_processed = false;
                let i_as_u64 = i as u64;
                for j in 0..processed.len() {
                    if processed.get(j) == i_as_u64 {
                        already_processed = true;
                        break;
                    }
                }
                
                if !already_processed {
                    let patron = patrons.get(i);
                    if max_amount_opt.is_none() || patron.total_amount > *max_amount_opt.as_ref().unwrap() {
                        max_amount_opt = Some(patron.total_amount.clone());
                        max_idx = i;
                    }
                }
            }
            
            // Add max patron to sorted list with rank
            if max_idx > 0 {
                let mut max_patron = patrons.get(max_idx);
                max_patron.patron_rank = rank as u64;
                sorted_patrons.push(&max_patron);
                processed.push(max_idx as u64);
            }
        }
        
        // Clear original and rebuild from sorted
        patrons.clear();
        for i in 1..=sorted_patrons.len() {
            let patron = sorted_patrons.get(i);
            patrons.push(&patron);
        }
        
        // Clear temporary storage
        sorted_patrons.clear();

        // Find and return rank for this donor
        for i in 1..=patrons.len() {
            let patron = patrons.get(i);
            if patron.donor_address == *donor_address {
                return Some(patron.patron_rank);
            }
        }

        None
    }

    fn get_donor_patron_rank(&self, donor_address: &ManagedAddress, entity_name: &ManagedBuffer) -> Option<u64> {
        let patrons = self.project_patrons(&entity_name);
        for i in 1..=patrons.len() {
            let patron = patrons.get(i);
            if patron.donor_address == *donor_address {
                return Some(patron.patron_rank);
            }
        }
        None
    }

    /// When ranks change, update other patrons' NFTs (tags + patron image) so e.g. old #1 shows #patron_2.
    fn update_other_patrons_nfts(
        &self,
        current_donor: &ManagedAddress,
        entity_name: &ManagedBuffer,
        entity_type: &ManagedBuffer,
        nft_token_id: &TokenIdentifier<Self::Api>,
    ) {
        let patrons = self.project_patrons(entity_name);
        let contract_address = self.blockchain().get_sc_address();
        let empty_tags = ManagedVec::new();

        for i in 1..=patrons.len() {
            let patron = patrons.get(i);
            if &patron.donor_address == current_donor {
                continue;
            }
            let other_registry = self.donor_nft_registry_for_entity(&patron.donor_address, entity_name);
            if other_registry.len() == 0 {
                continue;
            }
            let other_nonce = other_registry.get(other_registry.len());
            let balance = self.blockchain().get_esdt_balance(&contract_address, nft_token_id, other_nonce);
            if balance == 0u32 {
                continue;
            }
            let other_donation_count = self.donor_donations_to_entity(&patron.donor_address, entity_name).get();
            let other_total = self.donor_total_amount_to_entity(&patron.donor_address, entity_name).get();
            let other_tier = self.calculate_tier_for_entity(other_donation_count);
            let other_tier_name = self.get_tier_name(other_tier);
            let pattern_mapper = self.donor_recurring_patterns(&patron.donor_address);
            let other_pattern = if pattern_mapper.is_empty() {
                RecurringPattern { monthly_streak: 0, quarterly_streak: 0, last_donation_month: 0, last_donation_quarter: 0 }
            } else {
                pattern_mapper.get()
            };
            let tier_image_uri = self.get_tier_base_image_uri(other_tier);

            let attrs = self.create_donation_nft_attributes(
                entity_name,
                entity_type,
                other_donation_count,
                other_tier,
                &other_tier_name,
                &empty_tags,
                &other_total,
                Some(patron.patron_rank),
                Some(&other_pattern),
                tier_image_uri.clone(),
            );
            self.send().nft_update_attributes(nft_token_id, other_nonce, &attrs);

            let want_type = if patron.patron_rank == 1 { 1u64 } else { 2u64 };
            let added_type = self.patron_image_type_added(other_nonce).get();
            let legacy_has_image = added_type == 0 && self.has_patron_uri(other_nonce).get();
            let should_add = added_type != want_type
                && (want_type == 1 || !legacy_has_image);
            if should_add {
                if let Some(patron_uri) = self.get_patron_badge_uri(patron.patron_rank) {
                    self.send().nft_add_uri(nft_token_id, other_nonce, patron_uri);
                    self.patron_image_type_added(other_nonce).set(want_type);
                    self.has_patron_uri(other_nonce).set(true);
                }
            }
        }
    }

    // ============================================================
    // RECURRING DONATION TRACKING
    // ============================================================

    fn update_recurring_patterns(
        &self,
        donor_address: &ManagedAddress,
        _entity_name: &ManagedBuffer,
    ) -> RecurringPattern {
        let current_timestamp = self.blockchain().get_block_timestamp();
        
        // Calculate current month (YYYYMM format)
        // Note: This is simplified - you may want to use a proper date library
        // For now, we'll use timestamp-based calculation
        let current_month = self.timestamp_to_month(current_timestamp);
        let current_quarter = self.timestamp_to_quarter(current_timestamp);

        let pattern_mapper = self.donor_recurring_patterns(donor_address);
        let mut pattern = if pattern_mapper.is_empty() {
            RecurringPattern {
                monthly_streak: 0,
                quarterly_streak: 0,
                last_donation_month: 0,
                last_donation_quarter: 0,
            }
        } else {
            pattern_mapper.get()
        };

        // Check monthly streak
        if pattern.last_donation_month == 0 {
            // First donation
            pattern.monthly_streak = 1;
            pattern.quarterly_streak = 1;
        } else {
            let month_diff = if current_month > pattern.last_donation_month {
                current_month - pattern.last_donation_month
            } else {
                0
            };

            if month_diff == 1 {
                // Consecutive month
                pattern.monthly_streak += 1;
            } else if month_diff > 1 {
                // Streak broken
                pattern.monthly_streak = 1;
            }
            // If month_diff == 0, same month, keep streak

            // Check quarterly streak
            let quarter_diff = if current_quarter > pattern.last_donation_quarter {
                current_quarter - pattern.last_donation_quarter
            } else {
                0
            };

            if quarter_diff == 1 {
                pattern.quarterly_streak += 1;
            } else if quarter_diff > 1 {
                pattern.quarterly_streak = 1;
            }
        }

        pattern.last_donation_month = current_month;
        pattern.last_donation_quarter = current_quarter;

        self.donor_recurring_patterns(donor_address).set(&pattern);
        pattern
    }

    fn timestamp_to_month(&self, timestamp: u64) -> u64 {
        // Convert timestamp to YYYYMM format
        // Unix epoch: Jan 1, 1970
        // Approximate: 1 month = 2,592,000 seconds (30 days)
        // This is simplified - for production, use proper date library
        let seconds_per_month = 2_592_000u64;
        let months_since_epoch = timestamp / seconds_per_month;
        // Assuming we start counting from Jan 2024 (month 0)
        // Adjust base_month based on when your contract was deployed
        let base_month = 202401u64; // Jan 2024
        base_month + (months_since_epoch % 12) + ((months_since_epoch / 12) * 100)
    }

    fn timestamp_to_quarter(&self, timestamp: u64) -> u64 {
        // Convert timestamp to YYYYQ format (e.g., 20241 for Q1 2024)
        let seconds_per_month = 2_592_000u64;
        let months_since_epoch = timestamp / seconds_per_month;
        let month_in_year = months_since_epoch % 12;
        let quarter = (month_in_year / 3) + 1;
        let year = 2024u64 + (months_since_epoch / 12);
        year * 10 + quarter
    }

    // ============================================================
    // STATISTICS UPDATE
    // ============================================================

    fn update_donation_stats(&self, donation_amount: &BigUint, minted_new_nft: bool) {
        let mut stats = self.global_statistics().get();
        stats.total_donations_amount += donation_amount;
        stats.total_donations_count += 1;
        if minted_new_nft {
            stats.total_nfts_minted += 1;
        }
        self.global_statistics().set(&stats);
    }

    fn u64_to_buffer(&self, mut value: u64) -> ManagedBuffer {
        if value == 0 {
            return ManagedBuffer::new_from_bytes(b"0");
        }
        let mut buffer = ManagedBuffer::new();
        let mut digits = [0u8; 20];
        let mut len = 0;
        while value > 0 {
            digits[len] = b'0' + (value % 10) as u8;
            value /= 10;
            len += 1;
        }
        for i in (0..len).rev() {
            buffer.append(&ManagedBuffer::new_from_bytes(&[digits[i]]));
        }
        buffer
    }

    // ============================================================
    // QUERY FUNCTIONS
    // ============================================================

    #[view(getGlobalStatistics)]
    fn get_global_statistics(&self) -> GlobalStats<Self::Api> {
        self.global_statistics().get()
    }

    #[view(getDonorDonations)]
    fn get_donor_donations(&self, donor: ManagedAddress) -> MultiValueEncoded<DonationRecord<Self::Api>> {
        let mut result = MultiValueEncoded::new();
        for item in self.donor_donation_history(&donor).iter() {
            result.push(item);
        }
        result
    }

    #[view(getDonationsByEntity)]
    fn get_donations_by_entity(&self, entity_name: ManagedBuffer) -> MultiValueEncoded<DonationRecord<Self::Api>> {
        let mut result = MultiValueEncoded::new();
        for item in self.entity_donation_history(&entity_name).iter() {
            result.push(item);
        }
        result
    }

    #[view(getDonorNftsForEntity)]
    fn get_donor_nfts_for_entity(&self, donor: ManagedAddress, entity_name: ManagedBuffer) -> MultiValueEncoded<u64> {
        let mut result = MultiValueEncoded::new();
        for item in self.donor_nft_registry_for_entity(&donor, &entity_name).iter() {
            result.push(item);
        }
        result
    }

    #[view(getNftNonce)]
    fn get_nft_nonce(&self) -> u64 {
        self.nft_nonce().get()
    }

    #[view(getDonationCountToEntity)]
    fn get_donation_count_to_entity(&self, donor: ManagedAddress, entity_name: ManagedBuffer) -> u64 {
        self.donor_donations_to_entity(&donor, &entity_name).get()
    }

    /// Returns the attributes string that should be on-chain for this donor+entity (for verifying tags/traits update).
    #[view(getDonorNftAttributesPreview)]
    fn get_donor_nft_attributes_preview(&self, donor: ManagedAddress, entity_name: ManagedBuffer) -> ManagedBuffer {
        let registry = self.donor_nft_registry_for_entity(&donor, &entity_name);
        if registry.len() == 0 {
            return ManagedBuffer::new();
        }
        let nonce = registry.get(registry.len());
        let donation_count = self.donor_donations_to_entity(&donor, &entity_name).get();
        let tier_level = self.calculate_tier_for_entity(donation_count);
        let tier_name = self.get_tier_name(tier_level);
        let total_amount = self.donor_total_amount_to_entity(&donor, &entity_name).get();
        let patron_rank = self.get_donor_patron_rank(&donor, &entity_name);
        let pattern = self.donor_recurring_patterns(&donor).get();
        let recurring_pattern = if pattern.last_donation_month == 0 && pattern.last_donation_quarter == 0 {
            None
        } else {
            Some(&pattern)
        };
        let entity_type = if self.nft_metadata_record(nonce).is_empty() {
            ManagedBuffer::from(b"charity")
        } else {
            self.nft_metadata_record(nonce).get().entity_type
        };
        let user_tags = ManagedVec::new();
        let tier_image_uri = self.get_tier_base_image_uri(tier_level);
        self.create_donation_nft_attributes(
            &entity_name,
            &entity_type,
            donation_count,
            tier_level,
            &tier_name,
            &user_tags,
            &total_amount,
            patron_rank,
            recurring_pattern,
            tier_image_uri,
        )
    }

    #[view(getProjectTemplate)]
    fn get_project_template(&self) -> ManagedAddress {
        self.project_template().get()
    }

    #[view(getCharityTemplate)]
    fn get_charity_template(&self) -> ManagedAddress {
        self.charity_template().get()
    }

    // ============================================================
    // NFT METADATA VIEW FUNCTIONS - Query live NFT data
    // ============================================================

    #[view(getNftMetadata)]
    fn get_nft_metadata(&self, nft_nonce: u64) -> NftMetadataRecord<Self::Api> {
        self.nft_metadata_record(nft_nonce).get()
    }

    #[view(getDonorNftMetadataForEntity)]
    fn get_donor_nft_metadata_for_entity(
        &self,
        donor: ManagedAddress,
        entity_name: ManagedBuffer,
    ) -> OptionalValue<NftMetadataRecord<Self::Api>> {
        let registry = self.donor_nft_registry_for_entity(&donor, &entity_name);
        let registry_len = registry.len();
        
        if registry_len == 0 {
            OptionalValue::None
        } else {
            let nft_nonce = registry.get(registry_len);
            let metadata = self.nft_metadata_record(nft_nonce).get();
            OptionalValue::Some(metadata)
        }
    }

    #[view(getTransactionNftForEntity)]
    fn get_transaction_nft_for_entity(
        &self,
        entity_name: ManagedBuffer,
    ) -> OptionalValue<NftMetadataRecord<Self::Api>> {
        let nft_nonce_opt = self.entity_transaction_nft(&entity_name).get();
        
        if nft_nonce_opt == 0 {
            OptionalValue::None
        } else {
            let metadata = self.nft_metadata_record(nft_nonce_opt).get();
            OptionalValue::Some(metadata)
        }
    }

    // ============================================================
    // NFT RETRIEVAL - Allow donor to get their NFT from contract
    // ============================================================
    #[endpoint(retrieveDonorNft)]
    fn retrieve_donor_nft(&self, entity_name: ManagedBuffer) {
        let caller = self.blockchain().get_caller();
        let nft_token_id = self.global_nft_collection().get();
        require!(nft_token_id.is_valid_esdt_identifier(), "NFT collection not set");

        let registry = self.donor_nft_registry_for_entity(&caller, &entity_name);
        let registry_len = registry.len();
        
        require!(registry_len > 0, "No NFT found for this entity");

        // Get the latest NFT (last in registry, 1-based indexing)
        let nft_nonce = registry.get(registry_len);

        // Check if NFT is on contract
        let contract_balance = self.blockchain().get_esdt_balance(
            &self.blockchain().get_sc_address(),
            &nft_token_id,
            nft_nonce,
        );

        require!(contract_balance > 0u32, "NFT is not on contract (may have been retrieved already)");

        // Send NFT to donor
        self.send().direct_esdt(
            &caller,
            &nft_token_id,
            nft_nonce,
            &BigUint::from(1u32),
        );
    }

    // ============================================================
    // EVENTS
    // ============================================================

    #[event("nft_minted")]
    fn nft_minted(&self, #[indexed] donor: &ManagedAddress, #[indexed] entity: &ManagedBuffer, #[indexed] nonce: u64);

    #[event("transaction_nft_minted")]
    fn transaction_nft_minted(&self, #[indexed] entity: &ManagedBuffer, #[indexed] entity_type: &ManagedBuffer, #[indexed] nonce: u64);

    #[event("donation_recorded")]
    fn donation_recorded(&self, #[indexed] donor: &ManagedAddress, #[indexed] amount: &BigUint, #[indexed] entity: &ManagedBuffer);

    #[event("nft_attributes_updated")]
    fn nft_attributes_updated(&self, #[indexed] donor: &ManagedAddress, #[indexed] entity: &ManagedBuffer, #[indexed] nonce: u64);

    #[event("charity_deployed")]
    fn charity_deployed(&self, #[indexed] name: &ManagedBuffer, #[indexed] address: &ManagedAddress);

    #[event("nft_collection_issued")]
    fn nft_collection_issued(&self, #[indexed] token_identifier: &TokenIdentifier);

    // ============================================================
    // GAMIFICATION ENDPOINTS - Image URI Management
    // ============================================================

    #[endpoint(setTierImageUri)]
    fn set_tier_image_uri(&self, tier: u64, uri: ManagedBuffer) {
        self.only_owner();
        require!(!uri.is_empty(), "URI cannot be empty");
        
        match tier {
            1 => self.tier_bronze_image_uri().set(&uri),
            2 => self.tier_silver_image_uri().set(&uri),
            3 => self.tier_gold_image_uri().set(&uri),
            4 => self.tier_platinum_image_uri().set(&uri),
            _ => sc_panic!("Invalid tier number (1-4)"),
        }
    }

    #[endpoint(setPatronImageUri)]
    fn set_patron_image_uri(&self, patron_type: u64, uri: ManagedBuffer) {
        self.only_owner();
        require!(!uri.is_empty(), "URI cannot be empty");
        
        match patron_type {
            1 => self.patron_top1_image_uri().set(&uri),      // Rank #1 only
            2 => self.patron_rest_image_uri().set(&uri),      // Ranks 2-10
            _ => sc_panic!("Invalid patron type (1=Top1, 2=Rest)"),
        }
    }

    #[view(getProjectPatrons)]
    fn get_project_patrons(&self, project_name: ManagedBuffer) -> MultiValueEncoded<PatronRecord<Self::Api>> {
        let mut result = MultiValueEncoded::new();
        for item in self.project_patrons(&project_name).iter() {
            result.push(item);
        }
        result
    }

    #[view(getDonorPatronRank)]
    fn get_donor_patron_rank_view(&self, donor: ManagedAddress, project_name: ManagedBuffer) -> OptionalValue<u64> {
        let patrons = self.project_patrons(&project_name);
        for i in 1..=patrons.len() {
            let patron = patrons.get(i);
            if patron.donor_address == donor {
                return OptionalValue::Some(patron.patron_rank);
            }
        }
        OptionalValue::None
    }

    #[view(getDonorRecurringPattern)]
    fn get_donor_recurring_pattern(&self, donor: ManagedAddress) -> RecurringPattern {
        let pattern_mapper = self.donor_recurring_patterns(&donor);
        if pattern_mapper.is_empty() {
            RecurringPattern {
                monthly_streak: 0,
                quarterly_streak: 0,
                last_donation_month: 0,
                last_donation_quarter: 0,
            }
        } else {
            pattern_mapper.get()
        }
    }

    // ============================================================
    // STORAGE
    // ============================================================

    #[storage_mapper("global_admin_address")]
    fn global_admin_address(&self) -> SingleValueMapper<ManagedAddress>;

    #[storage_mapper("charity_template")]
    fn charity_template(&self) -> SingleValueMapper<ManagedAddress>;

    #[storage_mapper("project_template")]
    fn project_template(&self) -> SingleValueMapper<ManagedAddress>;

    #[storage_mapper("global_nft_collection")]
    fn global_nft_collection(&self) -> SingleValueMapper<TokenIdentifier>;

    #[storage_mapper("charity_count")]
    fn charity_count(&self) -> SingleValueMapper<u64>;

    #[storage_mapper("project_count")]
    fn project_count(&self) -> SingleValueMapper<u64>;

    #[storage_mapper("nft_nonce")]
    fn nft_nonce(&self) -> SingleValueMapper<u64>;

    #[storage_mapper("donor_donations_to_entity")]
    fn donor_donations_to_entity(&self, donor: &ManagedAddress, entity: &ManagedBuffer) -> SingleValueMapper<u64>;

    #[storage_mapper("donor_total_amount_to_entity")]
    fn donor_total_amount_to_entity(&self, donor: &ManagedAddress, entity: &ManagedBuffer) -> SingleValueMapper<BigUint>;

    #[storage_mapper("donor_donation_history")]
    fn donor_donation_history(&self, donor: &ManagedAddress) -> VecMapper<DonationRecord<Self::Api>>;

    #[storage_mapper("donor_nft_registry_for_entity")]
    fn donor_nft_registry_for_entity(&self, donor: &ManagedAddress, entity: &ManagedBuffer) -> VecMapper<u64>;

    #[storage_mapper("entity_donation_history")]
    fn entity_donation_history(&self, entity: &ManagedBuffer) -> VecMapper<DonationRecord<Self::Api>>;

    #[storage_mapper("entity_type_donation_history")]
    fn entity_type_donation_history(&self, entity_type: &ManagedBuffer) -> VecMapper<DonationRecord<Self::Api>>;

    #[storage_mapper("global_statistics")]
    fn global_statistics(&self) -> SingleValueMapper<GlobalStats<Self::Api>>;

    // Transaction NFT tracking (ONE NFT PER ENTITY)
    #[storage_mapper("entity_transaction_nft")]
    fn entity_transaction_nft(&self, entity_name: &ManagedBuffer) -> SingleValueMapper<u64>;

    #[storage_mapper("entity_transaction_total")]
    fn entity_transaction_total(&self, entity_name: &ManagedBuffer) -> SingleValueMapper<BigUint>;

    #[storage_mapper("entity_transaction_count")]
    fn entity_transaction_count(&self, entity_name: &ManagedBuffer) -> SingleValueMapper<u64>;

    // NFT Metadata tracking for dynamic updates
    #[storage_mapper("nft_metadata_record")]
    fn nft_metadata_record(&self, nft_nonce: u64) -> SingleValueMapper<NftMetadataRecord<Self::Api>>;

    // ============================================================
    // GAMIFICATION STORAGE
    // ============================================================

    // Project Patron System
    #[storage_mapper("project_patrons")]
    fn project_patrons(&self, project_name: &ManagedBuffer) -> VecMapper<PatronRecord<Self::Api>>;

    // Recurring Donation Patterns
    #[storage_mapper("donor_recurring_patterns")]
    fn donor_recurring_patterns(&self, donor: &ManagedAddress) -> SingleValueMapper<RecurringPattern>;

    // Tier Base Image URIs (Full images for each tier)
    #[storage_mapper("tier_bronze_image_uri")]
    fn tier_bronze_image_uri(&self) -> SingleValueMapper<ManagedBuffer>;

    #[storage_mapper("tier_silver_image_uri")]
    fn tier_silver_image_uri(&self) -> SingleValueMapper<ManagedBuffer>;

    #[storage_mapper("tier_gold_image_uri")]
    fn tier_gold_image_uri(&self) -> SingleValueMapper<ManagedBuffer>;

    #[storage_mapper("tier_platinum_image_uri")]
    fn tier_platinum_image_uri(&self) -> SingleValueMapper<ManagedBuffer>;

    // Patron Image URIs (2 images only)
    #[storage_mapper("patron_top1_image_uri")]
    fn patron_top1_image_uri(&self) -> SingleValueMapper<ManagedBuffer>;

    #[storage_mapper("patron_rest_image_uri")]
    fn patron_rest_image_uri(&self) -> SingleValueMapper<ManagedBuffer>;

    // Temporary storage for sorting patrons
    #[storage_mapper("temp_sorted_patrons")]
    fn temp_sorted_patrons(&self) -> VecMapper<PatronRecord<Self::Api>>;

    // Patron badge URI added (0=none, 1=top1 image, 2=rest image) so we add the other when rank changes
    #[storage_mapper("patron_image_type_added")]
    fn patron_image_type_added(&self, nft_nonce: u64) -> SingleValueMapper<u64>;

    #[storage_mapper("has_patron_uri")]
    fn has_patron_uri(&self, nft_nonce: u64) -> SingleValueMapper<bool>;
}