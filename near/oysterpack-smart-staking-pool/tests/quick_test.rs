use oysterpack_smart_near::domain::BasisPoints;
use oysterpack_smart_near::YOCTO;

#[test]
fn quick_test() {
    let current_contract_managed_total_balance: u128 = 350209990595092277689499999999;
    let last_contract_managed_total_balance: u128 = 350209991710169784696199999999;

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

    let fee = BasisPoints(80);
    println!("{}", fee * (10 * YOCTO));

    let total_staked_balance: u128 = 350205275301062033780999999999;
    let locked_balance: u128 = 350205274368298429283699999999;

    print!("total_staked_balance - locked_balance = ");
    if locked_balance > total_staked_balance {
        println!("-{}", locked_balance - total_staked_balance);
    } else {
        println!("{}", total_staked_balance - locked_balance);
    }

    let total_staked_balance: u128 = 350205275301062033780999999999;
    let locked_balance: u128 = 350208273156378890985777488829;

    print!("total_staked_balance - locked_balance = ");
    if locked_balance > total_staked_balance {
        println!("-{}", locked_balance - total_staked_balance);
    } else {
        println!("{}", total_staked_balance - locked_balance);
    }
}
