use lazy_static::lazy_static;
use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    serde::{Deserialize, Serialize},
};
use oysterpack_smart_near::{
    data::Object,
    domain::{YoctoNear, ZERO_NEAR},
    eventbus::{self, Event, EventHandlers},
    Level, LogEvent,
};
use std::{
    collections::HashMap,
    fmt::{self, Display, Formatter},
    ops::Deref,
    sync::Mutex,
};

#[derive(
    BorshSerialize,
    BorshDeserialize,
    Deserialize,
    Serialize,
    Clone,
    Copy,
    Debug,
    PartialOrd,
    PartialEq,
    Eq,
    Hash,
    Default,
)]
#[serde(crate = "near_sdk::serde")]
pub struct BalanceId(pub u8);

/// used to track NEAR balances that are outside registered accounts - examples
/// - liquidity
/// - profit sharing fund
pub type NearBalances = HashMap<BalanceId, YoctoNear>;

#[derive(
    BorshSerialize, BorshDeserialize, Deserialize, Serialize, Clone, Debug, PartialEq, Default,
)]
#[serde(crate = "near_sdk::serde")]
pub struct ContractNearBalances {
    total: YoctoNear,
    accounts: YoctoNear,
    balances: Option<NearBalances>,
    owner: YoctoNear,
}

impl ContractNearBalances {
    pub fn new(total: YoctoNear, accounts: YoctoNear, balances: Option<NearBalances>) -> Self {
        let owner = total
            - accounts
            - balances.as_ref().map_or(ZERO_NEAR, |balances| {
                balances
                    .values()
                    .map(|balance| balance.value())
                    .sum::<u128>()
                    .into()
            });
        Self {
            total,
            accounts,
            balances,
            owner,
        }
    }

    pub fn total(&self) -> YoctoNear {
        self.total
    }

    pub fn accounts(&self) -> YoctoNear {
        self.accounts
    }

    /// NEAR balances that are not owned by registered accounts and not by the contract owner, e.g.,
    /// - liquidity pools
    /// - batched funds, e.g., STAKE batches
    /// - profit sharing funds
    pub fn balances(&self) -> Option<NearBalances> {
        self.balances.as_ref().map(|balances| balances.clone())
    }

    /// returns portion of total contract NEAR balance that is owned by the contract owner, which is
    /// computed as: `total - accounts - balances`
    pub fn owner(&self) -> YoctoNear {
        self.owner
    }
}

const NEAR_BALANCES_KEY: u128 = 1953121181530803691069739592144632957;

type DAO = Object<u128, NearBalances>;

pub fn load_near_balances() -> NearBalances {
    DAO::load(&NEAR_BALANCES_KEY).map_or_else(NearBalances::new, |object| object.deref().clone())
}

/// Increments the balance by the specified amount and returns the updated balance
pub fn incr_balance(id: BalanceId, amount: YoctoNear) -> YoctoNear {
    let mut balances = DAO::load(&NEAR_BALANCES_KEY)
        .unwrap_or_else(|| DAO::new(NEAR_BALANCES_KEY, NearBalances::new()));
    let mut balance = balances.get(&id).cloned().unwrap_or(ZERO_NEAR);
    balance += amount;
    balances.insert(id, balance);
    balances.save();
    balance
}

/// Decrements the balance by the specified amount and returns the updated balance
pub fn decr_balance(id: BalanceId, amount: YoctoNear) -> YoctoNear {
    let mut balances = DAO::load(&NEAR_BALANCES_KEY)
        .unwrap_or_else(|| DAO::new(NEAR_BALANCES_KEY, NearBalances::new()));
    let mut balance = balances.get(&id).cloned().unwrap_or(ZERO_NEAR);
    balance -= amount;
    if balance == ZERO_NEAR {
        balances.remove(&id);
    } else {
        balances.insert(id, balance);
    }
    balances.save();
    balance
}

/// Sets the balance to the specified amount and returns the updated balance
pub fn set_balance(id: BalanceId, amount: YoctoNear) {
    let mut balances = DAO::load(&NEAR_BALANCES_KEY)
        .unwrap_or_else(|| DAO::new(NEAR_BALANCES_KEY, NearBalances::new()));
    if amount == ZERO_NEAR {
        balances.remove(&id);
    } else {
        balances.insert(id, amount);
    }
    balances.save();
}

/// Clears the balance and removes the record from storage
pub fn clear_balance(id: BalanceId) {
    let mut balances = DAO::load(&NEAR_BALANCES_KEY)
        .unwrap_or_else(|| DAO::new(NEAR_BALANCES_KEY, NearBalances::new()));
    balances.remove(&id);
    balances.save();
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum NearBalanceChangeEvent {
    Increment(BalanceId, YoctoNear),
    Decrement(BalanceId, YoctoNear),
    Update(BalanceId, YoctoNear),
    Clear(BalanceId),
}

impl Display for NearBalanceChangeEvent {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

const NEAR_BALANCE_CHANGE_EVENT: LogEvent = LogEvent(Level::INFO, "NearBalanceChangeEvent");

impl NearBalanceChangeEvent {
    pub fn log(&self) {
        NEAR_BALANCE_CHANGE_EVENT.log(self.to_string());
    }
}

// TODO: create macro to generate boilerplate code for event: #[event]
lazy_static! {
    static ref NEAR_BALANCE_CHANGE_EVENTS: Mutex<EventHandlers<NearBalanceChangeEvent>> =
        Mutex::new(EventHandlers::new());
    static ref EVENT_HANDLER_REGISTERED: Mutex<bool> = Mutex::new(false);
}

impl Event for NearBalanceChangeEvent {
    fn handlers<F>(f: F)
    where
        F: FnOnce(&EventHandlers<Self>),
    {
        f(&*NEAR_BALANCE_CHANGE_EVENTS.lock().unwrap())
    }

    fn handlers_mut<F>(f: F)
    where
        F: FnOnce(&mut EventHandlers<Self>),
    {
        f(&mut *NEAR_BALANCE_CHANGE_EVENTS.lock().unwrap())
    }
}

/// can be safely called multiple times and will only register the event handler once
pub(crate) fn register_event_handler() {
    let mut registered = EVENT_HANDLER_REGISTERED.lock().unwrap();
    if !*registered {
        eventbus::register(on_near_balance_change_event);
        *registered = true;
    }
}

fn on_near_balance_change_event(event: &NearBalanceChangeEvent) {
    event.log();
    match *event {
        NearBalanceChangeEvent::Increment(id, amount) => {
            incr_balance(id, amount);
        }
        NearBalanceChangeEvent::Decrement(id, amount) => {
            decr_balance(id, amount);
        }
        NearBalanceChangeEvent::Update(id, amount) => set_balance(id, amount),
        NearBalanceChangeEvent::Clear(id) => clear_balance(id),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use near_sdk::test_utils::{self, test_env};
    use oysterpack_smart_near::YOCTO;

    const LIQUIDITY_BALANCE_ID: BalanceId = BalanceId(0);
    const EARNINGS_BALANCE_ID: BalanceId = BalanceId(1);

    #[test]
    fn near_balance_change_event_handling() {
        // Arrange
        test_env::setup();
        register_event_handler();

        let balances = load_near_balances();
        assert!(balances.is_empty());

        // Act - increment balances
        eventbus::post(&NearBalanceChangeEvent::Increment(
            LIQUIDITY_BALANCE_ID,
            YOCTO.into(),
        ));
        eventbus::post(&NearBalanceChangeEvent::Increment(
            LIQUIDITY_BALANCE_ID,
            YOCTO.into(),
        ));
        eventbus::post(&NearBalanceChangeEvent::Increment(
            LIQUIDITY_BALANCE_ID,
            YOCTO.into(),
        ));
        eventbus::post(&NearBalanceChangeEvent::Increment(
            EARNINGS_BALANCE_ID,
            (2 * YOCTO).into(),
        ));

        // Assert
        let balances = load_near_balances();
        assert_eq!(balances.len(), 2);
        assert_eq!(
            balances.get(&LIQUIDITY_BALANCE_ID).unwrap().value(),
            3 * YOCTO
        );
        assert_eq!(
            balances.get(&EARNINGS_BALANCE_ID).unwrap().value(),
            2 * YOCTO
        );

        let logs = test_utils::get_logs();
        assert_eq!(logs.len(), 4);
        println!("{:#?}", logs);
    }
}
