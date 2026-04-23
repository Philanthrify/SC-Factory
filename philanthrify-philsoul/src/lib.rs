#![no_std]

multiversx_sc::imports!();
multiversx_sc::derive_imports!();

/// Soulbound-style credits: no transfer; PlatformDAO uses `balanceOf` + admin mint/burn.
#[multiversx_sc::contract]
pub trait PhilanthrifyPhilSoul {
    /// Deployer becomes the only address allowed to `mint` / `burn` (`only_admin`).
    #[init]
    fn init(&self) {
        self.admin().set(self.blockchain().get_caller());
        self.total_supply().set(0u64);
    }

    fn only_admin(&self) {
        require!(self.blockchain().get_caller() == self.admin().get(), "A");
    }

    #[endpoint(mint)]
    fn mint(&self, to: ManagedAddress, amount: u64) {
        self.only_admin();
        require!(!to.is_zero() && amount > 0, "B");
        let n = self
            .balances()
            .get(&to)
            .unwrap_or(0u64)
            .saturating_add(amount);
        self.balances().insert(to, n);
        self.total_supply()
            .set(self.total_supply().get().saturating_add(amount));
    }

    #[endpoint(burn)]
    fn burn(&self, from: ManagedAddress, amount: u64) {
        self.only_admin();
        require!(!from.is_zero() && amount > 0, "B");
        let cur = self.balances().get(&from).unwrap_or(0u64);
        require!(cur >= amount, "C");
        let n = cur - amount;
        if n == 0 {
            self.balances().remove(&from);
        } else {
            self.balances().insert(from, n);
        }
        self.total_supply()
            .set(self.total_supply().get().saturating_sub(amount));
    }

    #[view(balanceOf)]
    fn balance_of(&self, a: ManagedAddress) -> u64 {
        self.balances().get(&a).unwrap_or(0u64)
    }

    #[view(totalSupply)]
    fn total_supply_v(&self) -> u64 {
        self.total_supply().get()
    }

    #[view(getAdmin)]
    fn get_admin(&self) -> ManagedAddress {
        self.admin().get()
    }

    #[storage_mapper("a")]
    fn admin(&self) -> SingleValueMapper<ManagedAddress>;
    #[storage_mapper("b")]
    fn balances(&self) -> MapMapper<ManagedAddress, u64>;
    #[storage_mapper("c")]
    fn total_supply(&self) -> SingleValueMapper<u64>;
}
