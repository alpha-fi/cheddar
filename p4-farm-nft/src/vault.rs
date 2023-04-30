//! Vault is information per user about their balances in the exchange.
use crate::*;

#[derive(BorshSerialize, BorshDeserialize, Serialize)]
#[cfg_attr(feature = "test", derive(Default, Debug, Clone))]
pub struct Vault {
    /// Contract.reward_acc value when the last ping was called and rewards calculated
    pub reward_acc: Balance,
    /// Staked NFTs in this vault
    /// index - contract id
    /// value - token ids - []
    pub staked: Vec<TokenIds>,
    pub min_stake: Balance,
    /// Amount of accumulated, not withdrawn farmed units. When withdrawing the
    /// farmed units are translated to all `Contract.farm_tokens` based on
    /// `Contract.farm_token_rates`
    pub farmed: Balance,
    /// farmed tokens which failed to withdraw to the user.
    pub farmed_recovered: Vec<Balance>,
    /// NFTs deposited to get an extra boost. Only one NFT can be deposited to a
    /// single acocunt.
    /// Storing like `nft_contract@token_id`
    pub boost_nft: ContractNftTokenId,
    /// Staked Cheddar. Equals to `Contract.cheddar_rate` * total_staked_tokens.
    /// not depends on which NFT contract staked more or less tokens, rate used as a const
    pub cheddar_staked: Balance,
}

impl Vault {
    pub fn new(staked_len: usize, farmed_len: usize, reward_acc: Balance) -> Self {
        Self {
            reward_acc,
            staked: vec![TokenIds::new(); staked_len],
            min_stake: 0,
            farmed: 0,
            farmed_recovered: vec![0; farmed_len],
            boost_nft: TokenId::new(),
            cheddar_staked: 0,
        }
    }

    /**
    Update rewards for locked tokens in past epochs
    Arguments:
     - `reward_acc`: Contract.reward_acc value
     - `round`: current round
     */
    pub fn ping(&mut self, reward_acc: Balance, round: u64) {
        // note: the last round is at self.farming_end
        // if farming didn't start, ignore the rewards update
        if round == 0 {
            return;
        }
        // no new rewards
        if self.reward_acc >= reward_acc {
            return; // self.farmed;
        }
        self.farmed += self.min_stake * (reward_acc - self.reward_acc) / ACC_OVERFLOW;
        self.reward_acc = reward_acc;
    }

    /// If all vault's units is empty returns true
    #[inline]
    pub fn is_empty(&self) -> bool {
        check_all_empty(&self.staked)
            && self.farmed == 0
            && self.boost_nft.is_empty()
            && self.cheddar_staked == 0
    }
    /// Returns amount of user NFT tokens staked (from all supported NFT contracts).
    pub fn get_number_of_staked_tokens(&self) -> usize {
        self.staked
            .iter()
            .map(|contract_tokens| contract_tokens.len())
            .sum()
    }
}

impl Contract {
    /// Returns the registered vault.
    /// Panics if the account is not registered.
    #[inline]
    pub(crate) fn get_vault(&self, account_id: &AccountId) -> Vault {
        self.vaults.get(account_id).expect(ERR10_NO_ACCOUNT)
    }

    pub(crate) fn ping_all(&mut self, vault: &mut Vault) {
        let r = self.current_round();
        self.update_reward_acc(r);
        vault.ping(self.reward_acc, r);
    }

    /// updates the rewards accumulator
    pub(crate) fn update_reward_acc(&mut self, round: u64) {
        let new_acc = self.compute_reward_acc(round);
        // we should advance with rounds if self.t is zero, otherwise we have a jump and
        // don't compute properly the accumulator.
        if self.staked_units == 0 || new_acc != self.reward_acc {
            self.reward_acc = new_acc;
            self.reward_acc_round = round;
        }
    }

    /// computes the rewards accumulator.
    /// NOTE: the current, optimized algorithm will not farm anything if
    /// `self.rate * ACC_OVERFLOW / self.t < 1`
    pub(crate) fn compute_reward_acc(&self, round: u64) -> u128 {
        // covers also when round == 0
        if self.reward_acc_round == round || self.staked_units == 0 {
            return self.reward_acc;
        }

        self.reward_acc
            + u128::from(round - self.reward_acc_round) * self.farm_unit_emission * ACC_OVERFLOW
                / u128::from(self.staked_units)
    }

    pub(crate) fn _recompute_stake(&mut self, vault: &mut Vault) {
        let mut s = min_stake(&vault.staked, &self.stake_rates);

        if !vault.boost_nft.is_empty() {
            let (boost_contract, _) = extract_contract_token_ids(&vault.boost_nft);
            let nft_boost_rate = if boost_contract == self.cheddy {
                self.cheddy_boost
            } else {
                self.nft_boost
            };
            s += s * u128::from(nft_boost_rate) / BASIS_P;
        }

        if s > vault.min_stake {
            let diff = s - vault.min_stake;
            self.staked_units += diff; // must be called after ping_s
            vault.min_stake = s;
        } else if s < vault.min_stake {
            let diff = vault.min_stake - s;
            self.staked_units -= diff; // must be called after ping_s
            vault.min_stake = s;
        }
    }

    /// Returns stake operation status.
    /// Stake works only for 1 NFT token coming at the moment.
    /// Revert transfer if nft_contract (`predecessor_account_id`) not in `Contract.stake_tokens`
    /// We expect for user who stake enough cheddar stake in the `Vault`.
    /// For example - if user have `5 * cheddar_charge` Cheddar staked
    /// he can stake `5 NFT tokens`.
    /// so, if user have `5 staked NFT` now and `5 * cheddar_charge` Cheddar staked
    /// he cannot stake more NFT before `1 * cheddar_charge` will be deposited
    pub(crate) fn _nft_stake(
        &mut self,
        user: &AccountId,
        nft_contract_id: &NftContractId,
        token_id: TokenId,
    ) -> bool {
        // find index for staking token into Contract.stake_tokens
        let nft_ctr_idx = find_acc_idx(nft_contract_id, &self.stake_nft_tokens);
        let mut vault = self.get_vault(&user);

        // firstly check cheddar stake
        let total_staked_tokens = vault.get_number_of_staked_tokens();

        // we expect for user who stake one more token have enough cheddar staked
        let expected = expected_cheddar_stake(total_staked_tokens, self.cheddar_rate);
        assert!(
            vault.cheddar_staked >= expected,
            "You need to stake {} yoctoCheddar more to stake one more NFT token",
            expected - vault.cheddar_staked,
        );

        // then update the past rewards
        self.ping_all(&mut vault);
        // after that add "token" to staked into vault
        vault.staked[nft_ctr_idx].push(token_id.clone());
        // update total staked info about this token
        self.total_stake[nft_ctr_idx] += 1;

        self._recompute_stake(&mut vault);
        self.vaults.insert(user, &vault);
        log!(
            "Staked {}@{}, stake_units: {}",
            nft_contract_id,
            token_id.clone(),
            vault.min_stake
        );

        true
    }
    /// Returns boost stake operation status.
    /// Stake works only for 1 NFT token coming at the moment.
    /// Revert transfer if nft_contract (`predecessor_account_id`) not in `Contract.boost_nft_contracts`
    pub(crate) fn _boost_stake(
        &mut self,
        user: &AccountId,
        nft_contract_id: &NftContractId,
        token_id: TokenId,
    ) -> bool {
        // find index for boost token into Contract.boost_nft_contracts
        // Option to refund if `nft_contract_i` not in required for stake NFT contracts
        let nft_ctr_idx = find_acc_idx(&nft_contract_id, &self.boost_nft_contracts);
        let mut vault = self.get_vault(&user);

        if !vault.boost_nft.is_empty() {
            log!("Account already has boost NFT deposited. You can only deposit one");
            return false;
        }
        let contract_token_id: ContractNftTokenId =
            format!("{}{}{}", nft_ctr_idx, NFT_DELIMETER, token_id);
        log!(
            "Staking {} NFT - you will obtain a special farming boost",
            contract_token_id
        );

        self.ping_all(&mut vault);
        vault.boost_nft = contract_token_id.clone();

        // update total staked info about this token
        self.total_boost[nft_ctr_idx] += 1;

        self._recompute_stake(&mut vault);
        self.vaults.insert(&user, &vault);
        log!(
            "Added boost to user @{} with {}",
            user,
            contract_token_id.clone()
        );
        true
    }

    /// Returns remaining amount of NFTs from `nft_contract_id` which user has staked after function call.    
    /// Panics if `token_id` is not supported or not staked by a user.
    pub(crate) fn _nft_unstake(
        &mut self,
        user: &AccountId,
        nft_contract_id: &NftContractId,
        token_id: TokenId,
    ) -> Vec<String> {
        // getting contract, token and user vault
        let nft_ctr_idx = find_acc_idx(nft_contract_id, &self.stake_nft_tokens);
        let mut vault = self.get_vault(user);
        let token_idx = find_token_idx(&token_id, &vault.staked[nft_ctr_idx]);

        // check if we are withdraw last staked token
        // todo - double check for total_stake and total_cheddar_staked
        if vault.get_number_of_staked_tokens() == 1 {
            log!("unstaked last staked token - closing account");
            self.close();
            return vec![];
        }

        self.ping_all(&mut vault);
        // remove token from vault
        let removed_token_id = vault.staked[nft_ctr_idx].remove(token_idx);
        let remaining_tokens = vault.staked[nft_ctr_idx].clone();

        self._recompute_stake(&mut vault);

        // staked cheddar keeps on vault
        // v.total_cheddar_staked -= self.cheddar_rate;
        self.vaults.insert(user, &vault);

        self.transfer_staked_nft(user.clone(), nft_ctr_idx, removed_token_id);

        // staked cheddar keeps on vault
        // self.transfer_staked_cheddar(receiver_id.clone(), None);

        return remaining_tokens;
    }

    pub(crate) fn _withdraw_boost_nft(&mut self, user: &AccountId, vault: &mut Vault) {
        assert!(!vault.boost_nft.is_empty(), "Sender has no NFT deposit");
        self.ping_all(vault);

        let (boost_nft_contract_id, boost_nft_token_id) =
            extract_contract_token_ids(&vault.boost_nft);
        let nft_ctr_idx = find_acc_idx(&boost_nft_contract_id, &self.boost_nft_contracts);

        self.total_boost[nft_ctr_idx] -= 1;

        ext_nft::ext(boost_nft_contract_id.clone())
            .with_attached_deposit(ONE_YOCTO)
            .with_static_gas(GAS_FOR_NFT_TRANSFER)
            .nft_transfer(
                user.clone(),
                boost_nft_token_id.clone(),
                None,
                Some("Boost withdraw".to_string()),
            )
            .then(
                Self::ext(env::current_account_id())
                    .with_static_gas(GAS_FOR_CALLBACK)
                    .withdraw_boost_nft_callback(
                        user.clone(),
                        vault.boost_nft.clone(),
                        nft_ctr_idx,
                    ),
            );

        vault.boost_nft = "".into();
        self._recompute_stake(vault);
        self.vaults.insert(&user, &vault);
    }
}
