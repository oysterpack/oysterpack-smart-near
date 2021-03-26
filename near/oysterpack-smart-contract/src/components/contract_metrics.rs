use crate::{ContractMetrics, ContractMetricsSnapshot};
use crate::{ContractNearBalances, ContractStorageUsage, ContractStorageUsageCosts};
use oysterpack_smart_account_management::AccountMetrics;
use oysterpack_smart_near::data::numbers::U128;
use oysterpack_smart_near::domain::BlockTime;
use oysterpack_smart_near::near_sdk::env;

pub struct ContractMetricsComponent;

impl ContractMetrics for ContractMetricsComponent {
    fn ops_metrics_total_registered_accounts(&self) -> U128 {
        self.ops_metrics_accounts().total_registered_accounts
    }

    fn ops_metrics_contract_storage_usage(&self) -> ContractStorageUsage {
        let account_metrics = self.ops_metrics_accounts();
        ContractStorageUsage::new(account_metrics.total_storage_usage)
    }

    fn ops_metrics_near_balances(&self) -> ContractNearBalances {
        let account_metrics = self.ops_metrics_accounts();
        let near_balances = ContractNearBalances::load_near_balances();
        let near_balances = if near_balances.is_empty() {
            None
        } else {
            Some(near_balances)
        };
        ContractNearBalances::new(
            env::account_balance().into(),
            account_metrics.total_near_balance,
            near_balances,
        )
    }

    fn ops_metrics_storage_usage_costs(&self) -> ContractStorageUsageCosts {
        self.ops_metrics_contract_storage_usage().into()
    }

    fn ops_metrics(&self) -> ContractMetricsSnapshot {
        let storage_usage = self.ops_metrics_contract_storage_usage();
        ContractMetricsSnapshot {
            block_time: BlockTime::from_env(),
            total_registered_accounts: self.ops_metrics_total_registered_accounts(),
            storage_usage,
            near_balances: self.ops_metrics_near_balances(),
            storage_usage_costs: storage_usage.into(),
        }
    }

    fn ops_metrics_accounts(&self) -> AccountMetrics {
        AccountMetrics::load()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use near_sdk::env;
    use oysterpack_smart_account_management::components::account_management::{
        AccountManagementComponent, AccountManagementComponentConfig, ContractPermissions,
        UnregisterAccount, UnregisterAccountNOOP,
    };
    use oysterpack_smart_account_management::{
        AccountRepository, StorageManagement, StorageUsageBounds,
    };
    use oysterpack_smart_near::component::*;
    use oysterpack_smart_near::domain::YoctoNear;
    use oysterpack_smart_near::YOCTO;
    use oysterpack_smart_near_test::near_vm_logic::VMContext;
    use oysterpack_smart_near_test::*;
    use teloc::*;

    pub type AccountManager = AccountManagementComponent<()>;

    const ADMIN: &str = "admin";

    fn deploy_account_service() {
        AccountManager::deploy(AccountManagementComponentConfig {
            storage_usage_bounds: Some(StorageUsageBounds {
                min: 1000.into(),
                max: None,
            }),
            admin_account: to_valid_account_id(ADMIN),
            component_account_storage_mins: None,
        });
    }

    const ACCOUNT: &str = "bob";

    fn run_test<F>(test: F)
    where
        F: FnOnce(VMContext, AccountManager),
    {
        // Arrange
        let ctx = new_context(ACCOUNT);
        testing_env!(ctx.clone());

        // Act
        deploy_account_service();

        let container = ServiceProvider::new()
            .add_transient_c::<Box<dyn UnregisterAccount>, Box<UnregisterAccountNOOP>>()
            .add_instance(ContractPermissions::default())
            .add_transient::<AccountManager>();

        let service: AccountManager = container.resolve();
        test(ctx, service);
    }

    #[test]
    fn total_registered_accounts() {
        run_test(|mut ctx, mut account_manager| {
            // Assert - no accounts registered
            assert_eq!(
                ContractMetricsComponent.ops_metrics_total_registered_accounts(),
                1.into()
            );

            // Arrange - register an account
            ctx.attached_deposit = YOCTO;
            testing_env!(ctx.clone());
            account_manager.storage_deposit(None, None);

            // Assert
            assert_eq!(
                ContractMetricsComponent.ops_metrics_total_registered_accounts(),
                2.into()
            );
        });
    }

    #[test]
    fn storage_usage() {
        run_test(|mut ctx, mut account_manager| {
            // Act - no accounts registered besides admin
            let admin = account_manager.registered_account_near_data(ADMIN);
            let storage_usage = ContractMetricsComponent.ops_metrics_contract_storage_usage();
            println!("{:?}", storage_usage);
            assert_eq!(storage_usage.accounts(), admin.storage_usage());

            // Arrange - register an account
            ctx.attached_deposit = YOCTO;
            testing_env!(ctx.clone());
            account_manager.storage_deposit(None, None);

            let account = account_manager.registered_account_near_data(ACCOUNT);

            // Act
            let storage_usage = ContractMetricsComponent.ops_metrics_contract_storage_usage();
            println!("{:?}", storage_usage);
            // Assert
            assert_eq!(
                storage_usage.accounts(),
                account.storage_usage() + admin.storage_usage()
            );

            let storage_usage_costs = ContractMetricsComponent.ops_metrics_storage_usage_costs();
            assert_eq!(storage_usage_costs, storage_usage.into());
        });
    }

    #[test]
    fn near_balances() {
        run_test(|mut ctx, mut account_manager| {
            // Act - no accounts registered besides admin
            let admin = account_manager.registered_account_near_data(ADMIN);
            let balances1 = ContractMetricsComponent.ops_metrics_near_balances();
            println!("{:#?}", balances1);
            assert_eq!(balances1.total(), env::account_balance().into());
            assert_eq!(
                balances1.owner(),
                (env::account_balance() - admin.near_balance().value()).into()
            );
            assert!(balances1.balances().is_none());
            assert_eq!(balances1.accounts(), admin.near_balance());
            assert_eq!(balances1.total(), balances1.owner() + balances1.accounts());

            // Arrange - register an account
            ctx.attached_deposit = YOCTO;
            testing_env!(ctx.clone());
            account_manager.storage_deposit(None, None);

            // Act
            let balances2 = ContractMetricsComponent.ops_metrics_near_balances();
            println!("{:?}", balances2);
            // Assert
            assert_eq!(balances2.total(), env::account_balance().into());
            assert_eq!(balances2.owner(), balances1.owner());
            assert!(balances2.balances().is_none());
            assert_eq!(
                balances2.accounts(),
                admin.near_balance() + YoctoNear(YOCTO)
            );
            assert_eq!(balances2.total(), balances2.owner() + balances2.accounts());
        });
    }

    #[test]
    fn metrics() {
        run_test(|mut ctx, mut account_manager| {
            let metrics = ContractMetricsComponent.ops_metrics();
            println!("{:#?}", metrics);

            // Arrange - register an account
            ctx.attached_deposit = YOCTO;
            ctx.block_timestamp = 1;
            ctx.block_index = 2;
            ctx.epoch_height = 3;
            testing_env!(ctx.clone());
            account_manager.storage_deposit(None, None);

            let metrics = ContractMetricsComponent.ops_metrics();
            println!("{:#?}", metrics);
            assert_eq!(metrics.block_time.timestamp.value(), 1);
            assert_eq!(metrics.block_time.height.value(), 2);
            assert_eq!(metrics.block_time.epoch.value(), 3);
        });
    }
}
