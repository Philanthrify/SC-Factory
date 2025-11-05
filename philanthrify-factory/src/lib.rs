#![no_std]

extern crate alloc;

multiversx_sc::imports!();
multiversx_sc::derive_imports!();

#[type_abi]
#[derive(TopEncode, TopDecode, NestedEncode, NestedDecode, Clone)]
pub struct DonorProfile<M: ManagedTypeApi> {
    pub total_donated: BigUint<M>,
    pub donation_count: u64,
    pub first_donation_timestamp: u64,
    pub last_donation_timestamp: u64,
    pub highest_single_donation: BigUint<M>,
    pub favorite_charity: ManagedBuffer<M>,
    pub favorite_project: ManagedBuffer<M>,
}

#[type_abi]
#[derive(TopEncode, TopDecode, NestedEncode, NestedDecode, Clone, ManagedVecItem)]
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
    pub total_unique_donors: u64,
    pub total_nfts_minted: u64,
}

#[multiversx_sc::contract]
pub trait PhilanthrifyFactory {
    #[init]
    fn init(&self, global_admin: ManagedAddress) {
        self.global_admin_address().set(&global_admin);
        self.charity_count().set(0u64);
        self.project_count().set(0u64);
        
        let stats = GlobalStats {
            total_donations_amount: BigUint::zero(),
            total_donations_count: 0u64,
            total_unique_donors: 0u64,
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

        self.charity_deployed_event(&charity_name, &new_charity);
        new_charity
    }

    #[endpoint(setCharityTemplate)]
    fn set_charity_template(&self, template_address: ManagedAddress) {
        self.only_owner();
        require!(!template_address.is_zero(), "Invalid template address");
        self.charity_template().set(&template_address);
        self.template_updated_event(&ManagedBuffer::from("charity"), &template_address);
    }

    #[endpoint(setProjectTemplate)]
    fn set_project_template(&self, template_address: ManagedAddress) {
        self.only_owner();
        require!(!template_address.is_zero(), "Invalid template address");
        self.project_template().set(&template_address);
        self.template_updated_event(&ManagedBuffer::from("project"), &template_address);
    }

    #[payable("EGLD")]
    #[endpoint(issuePhilanthrifyNft)]
    fn issue_philanthrify_nft(&self) {
        self.only_owner();

        let issue_cost = BigUint::from(50_000_000_000_000_000u64);
        let payment = self.call_value().egld();

        require!(*payment == issue_cost, "Incorrect issue cost");

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
                    can_upgrade: true,
                    can_add_special_roles: true,
                    can_transfer_create_role: true,
                },
            )
            .async_call_and_exit();
    }

    #[endpoint(setGlobalNftCollection)]
    fn set_global_nft_collection(&self, nft_token_id: TokenIdentifier) {
        self.only_owner();
        self.global_nft_collection().set(&nft_token_id);
        self.nft_collection_updated_event(&nft_token_id);
    }

    #[endpoint(grantNftRoleToFactory)]
    fn grant_nft_role_to_factory(&self) {
        self.only_owner();

        let nft_collection = self.global_nft_collection().get();
        require!(nft_collection.is_valid_esdt_identifier(), "NFT collection not set");

        let factory_address = self.blockchain().get_sc_address();
        let roles = [
            EsdtLocalRole::NftCreate,
            EsdtLocalRole::NftUpdateAttributes,
            EsdtLocalRole::NftAddUri,
        ];

        self.send()
            .esdt_system_sc_proxy()
            .set_special_roles(&factory_address, &nft_collection, roles.iter().cloned())
            .async_call_and_exit();
    }

    #[endpoint(mintNft)]
    fn mint_nft(
        &self,
        donor_address: ManagedAddress,
        donation_amount: BigUint,
        entity_name: ManagedBuffer,
        entity_type: ManagedBuffer,
        custom_tags: MultiValueEncoded<ManagedBuffer>,
    ) {
        let nft_token_id = self.global_nft_collection().get();
        require!(nft_token_id.is_valid_esdt_identifier(), "NFT collection not set");

        let mut profile = self.get_or_create_donor_profile(&donor_address);
        let was_new_donor = profile.donation_count == 0;

        let current_timestamp = self.blockchain().get_block_timestamp();
        self.update_donor_profile(&mut profile, &donation_amount, &entity_name, &entity_type, current_timestamp);

        let mut user_tags = ManagedVec::new();
        for tag in custom_tags.into_iter() {
            if !tag.is_empty() && user_tags.len() < 10 {
                user_tags.push(tag);
            }
        }

        let mut minted_new_nft = false;
        let target_nonce: u64;

        let mut registry = self.donor_nft_registry(&donor_address);
        let has_nft = !registry.is_empty();

        if !has_nft {
            let current_nonce = self.nft_nonce().get();
            let new_nonce = current_nonce + 1;

            let mut nft_name = ManagedBuffer::new();
            nft_name.append(&ManagedBuffer::from(b"PHILXY #"));
            nft_name.append(&self.u64_to_buffer(new_nonce));
            nft_name.append(&ManagedBuffer::from(b" \xe2\x80\xa2 "));
            nft_name.append(&entity_name);
            nft_name.append(&ManagedBuffer::from(b" \xe2\x80\xa2 "));
            nft_name.append(&self.biguint_to_egld_string(&donation_amount));

            let attrs = self.create_nft_attributes(
                &entity_name,
                &entity_type,
                &donation_amount,
                new_nonce,
                &user_tags,
            );

            let royalties = BigUint::from(500u32);
            let hash = ManagedBuffer::new();

            let created_nonce = self.send().esdt_nft_create(
                &nft_token_id,
                &BigUint::from(1u32),
                &nft_name,
                &royalties,
                &hash,
                &attrs,
                &ManagedVec::new(),
            );

            self.send().direct_esdt(
                &donor_address,
                &nft_token_id,
                created_nonce,
                &BigUint::from(1u32),
            );

            registry.push(&created_nonce);
            self.nft_nonce().set(new_nonce);
            self.nft_minted_event(&donor_address, &entity_name, created_nonce);

            minted_new_nft = true;
            target_nonce = created_nonce;

        } else {
            let existing_nonce = registry.get(0);
            let updated_attributes = self.create_nft_attributes(
                &entity_name,
                &entity_type,
                &donation_amount,
                existing_nonce,
                &user_tags,
            );

            self.send().nft_update_attributes(
                &nft_token_id,
                existing_nonce,
                &updated_attributes,
            );

            target_nonce = existing_nonce;
        }

        let donation_record = DonationRecord {
            amount: donation_amount.clone(),
            timestamp: current_timestamp,
            entity_name: entity_name.clone(),
            entity_type: entity_type.clone(),
            nft_nonce: target_nonce,
        };
        self.donor_donation_history(&donor_address).push(&donation_record);

        self.donor_profiles(&donor_address).set(&profile);

        self.update_global_stats(&donation_amount, was_new_donor, minted_new_nft);

        self.donation_recorded_event(&donor_address, &donation_amount, &entity_name);
    }

    fn get_or_create_donor_profile(&self, donor: &ManagedAddress) -> DonorProfile<Self::Api> {
        if self.donor_profiles(donor).is_empty() {
            let current_timestamp = self.blockchain().get_block_timestamp();
            DonorProfile {
                total_donated: BigUint::zero(),
                donation_count: 0u64,
                first_donation_timestamp: current_timestamp,
                last_donation_timestamp: current_timestamp,
                highest_single_donation: BigUint::zero(),
                favorite_charity: ManagedBuffer::new(),
                favorite_project: ManagedBuffer::new(),
            }
        } else {
            self.donor_profiles(donor).get()
        }
    }

    fn update_donor_profile(
        &self,
        profile: &mut DonorProfile<Self::Api>,
        amount: &BigUint,
        entity_name: &ManagedBuffer,
        entity_type: &ManagedBuffer,
        current_timestamp: u64,
    ) {
        profile.total_donated += amount;
        profile.donation_count += 1;

        if amount > &profile.highest_single_donation {
            profile.highest_single_donation = amount.clone();
        }

        profile.last_donation_timestamp = current_timestamp;

        if entity_type == &ManagedBuffer::from(b"charity") {
            profile.favorite_charity = entity_name.clone();
        } else {
            profile.favorite_project = entity_name.clone();
        }
    }

    fn create_nft_attributes(
        &self,
        entity_name: &ManagedBuffer,
        entity_type: &ManagedBuffer,
        donation_amount: &BigUint,
        nft_nonce: u64,
        user_tags: &ManagedVec<Self::Api, ManagedBuffer>,
    ) -> ManagedBuffer {
        let mut attributes = ManagedBuffer::new();

        attributes.append(&ManagedBuffer::from(b"["));

        if user_tags.len() > 0 {
            for (idx, tag) in user_tags.iter().enumerate() {
                if idx > 0 {
                    attributes.append(&ManagedBuffer::from(b","));
                }
                attributes.append(&ManagedBuffer::from(b"{\"trait_type\":\"Tag\",\"value\":\""));
                attributes.append(&tag);
                attributes.append(&ManagedBuffer::from(b"\"}"));
            }

            if user_tags.len() > 0 {
                attributes.append(&ManagedBuffer::from(b","));
            }
        }

        attributes.append(&ManagedBuffer::from(b"{\"trait_type\":\"Charity\",\"value\":\""));
        attributes.append(entity_name);
        attributes.append(&ManagedBuffer::from(b"\"},"));

        attributes.append(&ManagedBuffer::from(b"{\"trait_type\":\"Type\",\"value\":\""));
        attributes.append(entity_type);
        attributes.append(&ManagedBuffer::from(b"\"},"));

        attributes.append(&ManagedBuffer::from(b"{\"trait_type\":\"Amount\",\"value\":\""));
        attributes.append(&self.biguint_to_egld_string(donation_amount));
        attributes.append(&ManagedBuffer::from(b" EGLD\"},"));

        attributes.append(&ManagedBuffer::from(b"{\"trait_type\":\"NFT_ID\",\"value\":\""));
        attributes.append(&self.u64_to_buffer(nft_nonce));
        attributes.append(&ManagedBuffer::from(b"\"}"));

        attributes.append(&ManagedBuffer::from(b"]"));

        attributes
    }

    fn update_global_stats(&self, donation_amount: &BigUint, was_new_donor: bool, minted_new_nft: bool) {
        let mut stats = self.global_statistics().get();
        stats.total_donations_amount += donation_amount;
        stats.total_donations_count += 1;
        if minted_new_nft {
            stats.total_nfts_minted += 1;
        }
        if was_new_donor {
            stats.total_unique_donors += 1;
        }
        self.global_statistics().set(&stats);
    }

    #[view(getDonorProfile)]
    fn get_donor_profile(&self, donor: ManagedAddress) -> OptionalValue<DonorProfile<Self::Api>> {
        if self.donor_profiles(&donor).is_empty() {
            OptionalValue::None
        } else {
            OptionalValue::Some(self.donor_profiles(&donor).get())
        }
    }

    #[view(getDonorDonations)]
    fn get_donor_donations(&self, donor: ManagedAddress) -> MultiValueEncoded<DonationRecord<Self::Api>> {
        let mut result = MultiValueEncoded::new();
        for item in self.donor_donation_history(&donor).iter() {
            result.push(item);
        }
        result
    }

    #[view(getDonorNfts)]
    fn get_donor_nfts(&self, donor: ManagedAddress) -> MultiValueEncoded<u64> {
        let mut result = MultiValueEncoded::new();
        for item in self.donor_nft_registry(&donor).iter() {
            result.push(item);
        }
        result
    }

    #[view(getGlobalStatistics)]
    fn get_global_statistics(&self) -> GlobalStats<Self::Api> {
        self.global_statistics().get()
    }

    #[view(getNftNonce)]
    fn get_nft_nonce(&self) -> u64 {
        self.nft_nonce().get()
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

    fn biguint_to_egld_string(&self, value: &BigUint) -> ManagedBuffer {
        let one_egld = BigUint::from(1_000_000_000_000_000_000u64);
        let egld = value / &one_egld;
        let mut result = ManagedBuffer::new();
        result.append(&self.u64_to_buffer(egld.to_u64().unwrap_or(0)));

        let remainder = value % &one_egld;
        if remainder > BigUint::zero() {
            result.append(&ManagedBuffer::from(b"."));
            let mut frac = remainder;
            let ten = BigUint::from(10u32);
            for _ in 0..2 {
                if frac == BigUint::zero() {
                    break;
                }
                let temp = frac.clone() * &ten;
                let digit = (temp.clone() / &one_egld).to_u64().unwrap_or(0);
                frac = temp % &one_egld;
                if digit > 0 || result.len() > 0 {
                    result.append(&self.u64_to_buffer(digit));
                }
            }
        }
        result
    }

    #[event("nft_minted")]
    fn nft_minted_event(
        &self,
        #[indexed] donor: &ManagedAddress,
        #[indexed] entity: &ManagedBuffer,
        data: u64,
    );

    #[event("donation_recorded")]
    fn donation_recorded_event(
        &self,
        #[indexed] donor: &ManagedAddress,
        #[indexed] amount: &BigUint,
        #[indexed] entity: &ManagedBuffer,
    );

    #[event("charity_deployed")]
    fn charity_deployed_event(
        &self,
        #[indexed] name: &ManagedBuffer,
        #[indexed] address: &ManagedAddress,
    );

    #[event("template_updated")]
    fn template_updated_event(
        &self,
        #[indexed] template_type: &ManagedBuffer,
        #[indexed] address: &ManagedAddress,
    );

    #[event("nft_collection_updated")]
    fn nft_collection_updated_event(
        &self,
        #[indexed] token_id: &TokenIdentifier,
    );

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

    #[storage_mapper("donor_profiles")]
    fn donor_profiles(&self, donor: &ManagedAddress) -> SingleValueMapper<DonorProfile<Self::Api>>;

    #[storage_mapper("donor_donation_history")]
    fn donor_donation_history(&self, donor: &ManagedAddress) -> VecMapper<DonationRecord<Self::Api>>;

    #[storage_mapper("donor_nft_registry")]
    fn donor_nft_registry(&self, donor: &ManagedAddress) -> VecMapper<u64>;

    #[storage_mapper("global_statistics")]
    fn global_statistics(&self) -> SingleValueMapper<GlobalStats<Self::Api>>;
}