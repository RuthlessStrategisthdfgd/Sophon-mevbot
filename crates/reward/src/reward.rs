//FEATURE TAG(S): Rewards, Block Structure
use accountable::accountable::Accountable;
use rand::{
    distributions::{Distribution, WeightedIndex},
    thread_rng,
    Rng,
};
use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;

/// This module declares the Categories and algorithms for randomly selecting
/// amounts within the categories of the rewards.
//TODO: Replace this entire module with the new monetary policy.
// https://www.notion.so/vrrb/Token-Emission-Methodology-4b6277403a3f4ad28653a651f0ff2995
use crate::decay::decay_calculator;

// UNITS
pub const SPECK: u128 = 1;
pub const TRIXIMO: u128 = 1000 * SPECK;
pub const NIFADA: u128 = 1000 * TRIXIMO;
pub const RIMA: u128 = 1000 * NIFADA;
pub const SITARI: u128 = 1000 * RIMA;
pub const PSIGMA: u128 = 1000 * SITARI;
pub const VRRB: u128 = 1000 * PSIGMA;

// Generate a random variable reward to include in new blocks
pub const TOTAL_NUGGETS: u128 = 80_000_000;
pub const TOTAL_VEINS: u128 = 1_400_000;
pub const TOTAL_MOTHERLODES: u128 = 20_000;
pub const N_BLOCKS_PER_EPOCH: u128 = 16000000;
pub const NUGGET_FINAL_EPOCH: u128 = 300;
pub const VEIN_FINAL_EPOCH: u128 = 200;
pub const MOTHERLODE_FINAL_EPOCH: u128 = 100;
pub const FLAKE_REWARD_RANGE: (u128, u128) = (1, 8);
pub const GRAIN_REWARD_RANGE: (u128, u128) = (8, 64);
pub const NUGGET_REWARD_RANGE: (u128, u128) = (64, 512);
pub const VEIN_REWARD_RANGE: (u128, u128) = (512, 4096);
pub const MOTHERLODE_REWARD_RANGE: (u128, u128) = (4096, 32769);
pub const GENESIS_REWARD: u128 = 200_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, EnumIter)]
pub enum Category {
    Flake(Option<u128>),
    Grain(Option<u128>),
    Nugget(Option<u128>),
    Vein(Option<u128>),
    Motherlode(Option<u128>),
    Genesis(Option<u128>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub struct RewardState {
    pub epoch: u128,
    pub next_epoch_block: u128,
    pub current_block: u128,
    pub n_nuggets_remaining: u128,
    pub n_veins_remaining: u128,
    pub n_motherlodes_remaining: u128,
    pub n_nuggets_current_epoch: u128,
    pub n_veins_current_epoch: u128,
    pub n_motherlodes_current_epoch: u128,
    pub n_flakes_current_epoch: u128,
    pub n_grains_current_epoch: u128,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reward {
    pub miner: Option<String>,
    pub amount: u128,
}

impl RewardState {
    pub fn start() -> RewardState {
        let n_nuggets_ce: u128 =
            (decay_calculator(TOTAL_NUGGETS, NUGGET_FINAL_EPOCH) * TOTAL_NUGGETS as f64) as u128;
        let n_veins_ce: u128 =
            (decay_calculator(TOTAL_VEINS, VEIN_FINAL_EPOCH) * TOTAL_VEINS as f64) as u128;
        let n_motherlodes_ce: u128 = (decay_calculator(TOTAL_MOTHERLODES, MOTHERLODE_FINAL_EPOCH)
            * TOTAL_MOTHERLODES as f64) as u128;
        let remaining_blocks = N_BLOCKS_PER_EPOCH - (n_nuggets_ce + n_veins_ce + n_motherlodes_ce);
        let n_flakes_ce: u128 = (remaining_blocks as f64 * 0.6f64) as u128;
        let n_grains_ce: u128 = (remaining_blocks as f64 * 0.4f64) as u128;

        RewardState {
            current_block: 0,
            epoch: 1,
            next_epoch_block: 16000000,
            n_nuggets_remaining: TOTAL_NUGGETS,
            n_veins_remaining: TOTAL_VEINS,
            n_motherlodes_remaining: TOTAL_MOTHERLODES,
            n_nuggets_current_epoch: n_nuggets_ce,
            n_veins_current_epoch: n_veins_ce,
            n_motherlodes_current_epoch: n_motherlodes_ce,
            n_flakes_current_epoch: n_flakes_ce,
            n_grains_current_epoch: n_grains_ce,
        }
    }

    pub fn update(&mut self, last_reward: Category) {
        let remaining_blocks_in_ce: u128 = self.next_epoch_block - (self.current_block + 1);

        if remaining_blocks_in_ce != 0 {
            match last_reward {
                Category::Nugget(_) => {
                    self.n_nuggets_current_epoch -= 1;
                    self.n_nuggets_remaining -= 1;
                },
                Category::Vein(_) => {
                    self.n_veins_current_epoch -= 1;
                    self.n_veins_remaining -= 1;
                },
                Category::Motherlode(_) => {
                    self.n_motherlodes_current_epoch -= 1;
                    self.n_motherlodes_remaining -= 1;
                },
                Category::Flake(_) => {
                    self.n_flakes_current_epoch -= 1;
                },
                Category::Grain(_) => {
                    self.n_grains_current_epoch -= 1;
                },
                _ => {},
            }
        } else {
            self.new_epoch();
        }
    }

    pub fn new_epoch(&mut self) {
        self.n_nuggets_current_epoch = (decay_calculator(TOTAL_NUGGETS, NUGGET_FINAL_EPOCH)
            * self.n_nuggets_remaining as f64) as u128;
        self.n_veins_current_epoch = (decay_calculator(TOTAL_NUGGETS, NUGGET_FINAL_EPOCH)
            * self.n_veins_remaining as f64) as u128;
        self.n_motherlodes_current_epoch = (decay_calculator(TOTAL_NUGGETS, NUGGET_FINAL_EPOCH)
            * self.n_motherlodes_remaining as f64)
            as u128;
        let remaining_blocks = N_BLOCKS_PER_EPOCH
            - (self.n_nuggets_current_epoch
                + self.n_veins_current_epoch
                + self.n_motherlodes_current_epoch);
        self.n_flakes_current_epoch = (remaining_blocks as f64 * 0.6f64) as u128;
        self.n_grains_current_epoch = (remaining_blocks as f64 * 0.4f64) as u128;
        self.epoch += 1;
        self.next_epoch_block += N_BLOCKS_PER_EPOCH;
    }

    pub fn as_bytes(&self) -> Vec<u8> {
        self.to_string().as_bytes().to_vec()
    }

    pub fn from_bytes(data: &[u8]) -> RewardState {
        let mut buffer: Vec<u8> = vec![];

        data.iter().for_each(|x| buffer.push(*x));

        let to_string = String::from_utf8(buffer).unwrap();

        serde_json::from_str::<RewardState>(&to_string).unwrap()
    }

    //TODO: Can we rename that?
    #[allow(clippy::inherent_to_string)]
    pub fn to_string(&self) -> String {
        // TODO: Is this unwrap safe?
        serde_json::to_string(self).unwrap()
    }

    pub fn valid_reward(&self, category: Category) -> bool {
        match category {
            Category::Flake(amount) => match amount {
                Some(amt) => {
                    if amt < FLAKE_REWARD_RANGE.0 || amt > FLAKE_REWARD_RANGE.1 {
                        return false;
                    }
                    if self.n_flakes_current_epoch == 0 {
                        return false;
                    }
                },
                None => return false,
            },
            Category::Grain(amount) => match amount {
                Some(amt) => {
                    if amt < GRAIN_REWARD_RANGE.0 || amt > GRAIN_REWARD_RANGE.1 {
                        return false;
                    }

                    if self.n_grains_current_epoch == 0 {
                        return false;
                    }
                },
                None => return false,
            },
            Category::Nugget(amount) => match amount {
                Some(amt) => {
                    if amt < NUGGET_REWARD_RANGE.0 || amt > NUGGET_REWARD_RANGE.1 {
                        return false;
                    }

                    if self.n_nuggets_current_epoch == 0 {
                        return false;
                    }

                    if self.n_nuggets_remaining == 0 {
                        return false;
                    }

                    if self.epoch > NUGGET_FINAL_EPOCH {
                        return false;
                    }

                    if self.epoch == NUGGET_FINAL_EPOCH && self.n_nuggets_remaining > 1 {
                        return false;
                    }
                },
                None => return false,
            },
            Category::Vein(amount) => match amount {
                Some(amt) => {
                    if amt < VEIN_REWARD_RANGE.0 || amt > VEIN_REWARD_RANGE.1 {
                        return false;
                    }
                    if self.n_veins_current_epoch == 0 {
                        return false;
                    }

                    if self.n_veins_remaining == 0 {
                        return false;
                    }

                    if self.epoch > VEIN_FINAL_EPOCH {
                        return false;
                    }

                    if self.epoch == VEIN_FINAL_EPOCH && self.n_veins_remaining > 1 {
                        return false;
                    }
                },
                None => return false,
            },
            Category::Motherlode(amount) => match amount {
                Some(amt) => {
                    if amt < MOTHERLODE_REWARD_RANGE.0 || amt > MOTHERLODE_REWARD_RANGE.1 {
                        return false;
                    }
                    if self.n_motherlodes_current_epoch == 0 {
                        return false;
                    }

                    if self.n_motherlodes_remaining == 0 {
                        return false;
                    }

                    if self.epoch > MOTHERLODE_FINAL_EPOCH {
                        return false;
                    }

                    if self.epoch == MOTHERLODE_FINAL_EPOCH && self.n_motherlodes_remaining > 1 {
                        return false;
                    }
                },
                None => return false,
            },
            Category::Genesis(amount) => match amount {
                Some(amt) => {
                    if amt != GENESIS_REWARD {
                        return false;
                    }
                },
                None => return false,
            },
        }
        true
    }
}

impl Reward {
    pub fn new(miner: Option<String>, _reward_state: &RewardState) -> Reward {
        Reward {
            miner,
            amount: 0, // Add error handling, as this should NEVER happen.
        }
    }

    pub fn genesis(miner: Option<String>) -> Reward {
        let category = Category::Genesis(Some(GENESIS_REWARD as u128));
        Reward {
            miner,
            amount: match category {
                Category::Genesis(Some(amount)) => amount,
                // TODO: set amount to base line reward by default
                _ => 0,
            },
        }
    }

    pub fn as_bytes(&self) -> Vec<u8> {
        // TODO: Is this unwrap safe?
        let as_string = serde_json::to_string(self).unwrap();

        as_string.as_bytes().to_vec()
    }

    pub fn from_bytes(data: &[u8]) -> Reward {
        let mut buffer: Vec<u8> = vec![];

        data.iter().for_each(|x| buffer.push(*x));

        let to_string = String::from_utf8(buffer).unwrap();

        serde_json::from_str::<Reward>(&to_string).unwrap()
    }
}

impl Category {
    pub fn new(reward_state: &RewardState) -> Category {
        Category::generate_category(reward_state).amount()
    }

    pub fn generate_category(reward_state: &RewardState) -> Category {
        let n_flakes_current_epoch = (*reward_state).n_flakes_current_epoch;
        let n_grains_current_epoch = (*reward_state).n_grains_current_epoch;
        let n_nuggets_current_epoch = (*reward_state).n_grains_current_epoch;
        let n_veins_current_epoch = (*reward_state).n_veins_current_epoch;
        let n_motherlodes_current_epoch = (*reward_state).n_motherlodes_current_epoch;

        let items = vec![
            (Category::Flake(None), n_flakes_current_epoch),
            (Category::Grain(None), n_grains_current_epoch),
            (Category::Nugget(None), n_nuggets_current_epoch),
            (Category::Vein(None), n_veins_current_epoch),
            (Category::Motherlode(None), n_motherlodes_current_epoch),
        ];
        let dist = WeightedIndex::new(items.iter().map(|item| item.1)).unwrap();
        let mut rng = rand::thread_rng();
        items[dist.sample(&mut rng)].0
    }

    pub fn amount(&self) -> Category {
        let mut rng = thread_rng();
        match self {
            Self::Genesis(None) => Category::Genesis(None),
            Self::Flake(None) => Category::Flake(Some(
                rng.gen_range(FLAKE_REWARD_RANGE.0, FLAKE_REWARD_RANGE.1),
            )),
            Self::Grain(None) => Category::Grain(Some(
                rng.gen_range(GRAIN_REWARD_RANGE.0, GRAIN_REWARD_RANGE.1),
            )),
            Self::Nugget(None) => Category::Nugget(Some(
                rng.gen_range(NUGGET_REWARD_RANGE.0, NUGGET_REWARD_RANGE.1),
            )),
            Self::Vein(None) => Category::Vein(Some(
                rng.gen_range(VEIN_REWARD_RANGE.0, VEIN_REWARD_RANGE.1),
            )),
            Self::Motherlode(None) => Category::Motherlode(Some(
                rng.gen_range(MOTHERLODE_REWARD_RANGE.0, MOTHERLODE_REWARD_RANGE.1),
            )),
            Self::Genesis(Some(amount)) => Self::Genesis(Some(*amount)),
            Self::Flake(Some(amount)) => Self::Flake(Some(*amount)),
            Self::Grain(Some(amount)) => Self::Grain(Some(*amount)),
            Self::Nugget(Some(amount)) => Self::Nugget(Some(*amount)),
            Self::Vein(Some(amount)) => Self::Vein(Some(*amount)),
            Self::Motherlode(Some(amount)) => Self::Motherlode(Some(*amount)),
        }
    }

    pub fn as_bytes(&self) -> Vec<u8> {
        let as_string = serde_json::to_string(self).unwrap();

        as_string.as_bytes().to_vec()
    }

    pub fn from_bytes(data: &[u8]) -> Category {
        let mut buffer: Vec<u8> = vec![];

        data.iter().for_each(|x| buffer.push(*x));

        let to_string = String::from_utf8(buffer).unwrap();

        serde_json::from_str::<Category>(&to_string).unwrap()
    }
}

impl Accountable for Reward {
    type Category = Category;

    fn receivable(&self) -> String {
        self.miner.clone().unwrap()
    }

    fn payable(&self) -> Option<String> {
        None
    }

    fn get_amount(&self) -> u128 {
        self.amount
    }

    fn get_category(&self) -> Option<Category> {
        None
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_reward_state_starting_point() {}

    #[test]
    fn test_reward_state_updates_after_mined_block() {}

    #[test]
    fn test_restored_reward_state() {}

    #[test]
    fn test_reward_category_valid_amount() {}

    #[test]
    fn test_reward_category_invalid_amount() {}
}
