#![no_std]

multiversx_sc::imports!();
multiversx_sc::derive_imports!();

#[multiversx_sc::contract]
pub trait PhilanthrifyFactory {
    #[init]
    fn init(&self) {
        self.owner().set(self.blockchain().get_caller());
    }

    #[upgrade]
    fn upgrade(&self) {
        require!(
            self.blockchain().get_caller() == self.owner().get(),
            "Only the owner can upgrade the contract"
        );
    }

    #[endpoint(deployCharity)]
    fn deploy_charity(&self, charity_name: ManagedBuffer) -> ManagedAddress<Self::Api> {
        require!(
            self.blockchain().get_caller() == self.owner().get(),
            "Only the owner can deploy charities"
        );

        let charity_template = self.charity_template().get();
        require!(!charity_template.is_zero(), "Charity template not set");

        let gas_for_deploy = 15_000_000u64;

        let new_charity_address: ManagedAddress<Self::Api> = self
            .tx()
            .raw_deploy()
            .from_source(charity_template)
            .code_metadata(
                multiversx_sc::types::CodeMetadata::PAYABLE
                    | multiversx_sc::types::CodeMetadata::PAYABLE_BY_SC
                    | multiversx_sc::types::CodeMetadata::UPGRADEABLE
                    | multiversx_sc::types::CodeMetadata::READABLE,
            )
            .argument(&charity_name)
            .argument(&self.blockchain().get_sc_address())
            .argument(&self.blockchain().get_caller())
            .gas(gas_for_deploy)
            .returns(multiversx_sc::types::ReturnsNewAddress)
            .sync_call()
            .into();

        let mut charities = self.deployed_charities().get();
        charities.push(new_charity_address.clone());
        self.deployed_charities().set(charities);

        self.charity_deployed_event(&charity_name, &new_charity_address);

        new_charity_address
    }

    #[endpoint(setCharityTemplate)]
    fn set_charity_template(&self, charity_template: ManagedAddress) {
        require!(
            self.blockchain().get_caller() == self.owner().get(),
            "Only the owner can set the charity template"
        );
        self.charity_template().set(charity_template);
    }

    #[endpoint(setOwner)]
    fn set_owner(&self, new_owner: ManagedAddress) {
        require!(
            self.blockchain().get_caller() == self.owner().get(),
            "Only the owner can set a new owner"
        );
        self.owner().set(new_owner);
    }

    #[event("charityDeployed")]
    fn charity_deployed_event(
        &self,
        #[indexed] charity_name: &ManagedBuffer,
        #[indexed] address: &ManagedAddress,
    );

    #[view(getOwner)]
    #[storage_mapper("owner")]
    fn owner(&self) -> SingleValueMapper<ManagedAddress>;

    #[view(getCharityTemplate)]
    #[storage_mapper("charity_template")]
    fn charity_template(&self) -> SingleValueMapper<ManagedAddress>;

    #[view(getDeployedCharities)]
    #[storage_mapper("deployed_charities")]
    fn deployed_charities(&self) -> SingleValueMapper<ManagedVec<ManagedAddress>>;
}