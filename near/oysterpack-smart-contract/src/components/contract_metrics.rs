use crate::ContractMetrics;
use oysterpack_smart_account_management::GetAccountMetrics;

pub struct ContractMetricsComponent;

impl ContractMetrics for ContractMetricsComponent {}

impl GetAccountMetrics for ContractMetricsComponent {}

#[cfg(test)]
mod tests {
    use super::*;
    use near_sdk::env;
    use oysterpack_smart_account_management::components::account_management::{
        AccountManagementComponent, UnregisterAccount, UnregisterAccountNOOP,
    };
    use oysterpack_smart_account_management::{StorageManagement, StorageUsageBounds};
    use oysterpack_smart_near::component::*;
    use oysterpack_smart_near::YOCTO;
    use oysterpack_smart_near_test::near_vm_logic::VMContext;
    use oysterpack_smart_near_test::*;
    use teloc::*;

    pub type AccountManager = AccountManagementComponent<()>;

    fn deploy_account_service() {
        AccountManager::deploy(Some(StorageUsageBounds {
            min: 1000.into(),
            max: None,
        }));
    }

    fn run_test<F>(test: F)
    where
        F: FnOnce(VMContext, AccountManager),
    {
        // Arrange
        let account_id = "bob";
        let ctx = new_context(account_id);
        testing_env!(ctx.clone());

        // Act
        deploy_account_service();

        let container = ServiceProvider::new()
            .add_transient_c::<Box<dyn UnregisterAccount>, Box<UnregisterAccountNOOP>>()
            .add_transient::<AccountManager>();

        let service: AccountManager = container.resolve();
        test(ctx, service);
    }

    #[test]
    fn total_registered_accounts() {
        run_test(|mut ctx, mut account_manager| {
            let component = ContractMetricsComponent;

            // Assert - no accounts registered
            assert_eq!(component.total_registered_accounts(), 0.into());

            // Arrange - register an account
            ctx.attached_deposit = YOCTO;
            testing_env!(ctx.clone());
            account_manager.storage_deposit(None, None);

            // Assert
            assert_eq!(component.total_registered_accounts(), 1.into());
        });
    }

    #[test]
    fn storage_usage() {
        run_test(|mut ctx, mut account_manager| {
            let component = ContractMetricsComponent;

            // Act - no accounts registered
            let storage_usage = component.storage_usage();
            println!("{:?}", storage_usage);
            assert_eq!(storage_usage.accounts(), 0.into());

            // Arrange - register an account
            ctx.attached_deposit = YOCTO;
            testing_env!(ctx.clone());
            account_manager.storage_deposit(None, None);

            // Act
            let storage_usage = component.storage_usage();
            println!("{:?}", storage_usage);
            // Assert
            assert!(storage_usage.accounts().value() > 0);

            let storage_usage_costs = component.storage_usage_costs();
            assert_eq!(storage_usage_costs, storage_usage.into());
        });
    }

    #[test]
    fn near_balances() {
        run_test(|mut ctx, mut account_manager| {
            let component = ContractMetricsComponent;

            // Act - no accounts registered
            let balances1 = component.near_balances();
            println!("{:?}", balances1);
            assert_eq!(balances1.total(), env::account_balance().into());
            assert_eq!(balances1.owner(), env::account_balance().into());
            assert!(balances1.balances().is_none());
            assert_eq!(balances1.accounts(), 0.into());
            assert_eq!(balances1.total(), balances1.owner() + balances1.accounts());

            // Arrange - register an account
            ctx.attached_deposit = YOCTO;
            testing_env!(ctx.clone());
            account_manager.storage_deposit(None, None);

            // Act
            let balances2 = component.near_balances();
            println!("{:?}", balances2);
            // Assert
            assert_eq!(balances2.total(), env::account_balance().into());
            assert_eq!(balances2.owner(), balances1.owner());
            assert!(balances2.balances().is_none());
            assert_eq!(balances2.accounts(), YOCTO.into());
            assert_eq!(balances2.total(), balances2.owner() + balances2.accounts());
        });
    }

    #[test]
    fn metrics() {
        run_test(|mut ctx, mut account_manager| {
            let component = ContractMetricsComponent;

            let metrics = component.metrics();
            println!("{:#?}", metrics);

            // Arrange - register an account
            ctx.attached_deposit = YOCTO;
            ctx.block_timestamp = 1;
            ctx.block_index = 2;
            ctx.epoch_height = 3;
            testing_env!(ctx.clone());
            account_manager.storage_deposit(None, None);

            let metrics = component.metrics();
            println!("{:#?}", metrics);
            assert_eq!(metrics.block_time.timestamp.value(), 1);
            assert_eq!(metrics.block_time.height.value(), 2);
            assert_eq!(metrics.block_time.epoch.value(), 3);
        });
    }
}
