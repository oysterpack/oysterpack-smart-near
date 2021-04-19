#[test]
fn quick_test() {
    let last_contract_managed_total_balance: u128 = 350209991710169784696199999999;
    let current_contract_managed_total_balance: u128 = 350209991292164780970499999999;

    print!("current_contract_managed_total_balance - last_contract_managed_total_balance = ");
    if last_contract_managed_total_balance > current_contract_managed_total_balance {
        println!(
            "-{}",
            last_contract_managed_total_balance - current_contract_managed_total_balance
        );
    } else {
        println!(
            "{}",
            current_contract_managed_total_balance - last_contract_managed_total_balance
        );
    }

    let last_total_staked: u128 = 350195274368298429283700000000;
    let total_staked: u128 = 350205275301062033780999999999;
    print!("total_staked - last_total_staked = ");
    if last_total_staked > total_staked {
        println!("-{}", last_total_staked - total_staked);
    } else {
        println!("{}", total_staked - last_total_staked);
    }
}
