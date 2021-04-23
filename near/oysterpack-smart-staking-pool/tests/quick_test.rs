#[test]
fn quick_test() {
    let left: u128 = 992366412213740458015268;
    let right: u128 = 992319794883748033331391;

    if left > right {
        println!(
            r#"
  {:0>30} 
- {:0>30}
  {:0>30}"#,
            left,
            right,
            left - right
        );
    } else {
        println!(
            r#"
  {:0>30} 
- {:0>30}
  {:0>30}"#,
            left,
            right,
            right - left
        );
    }
}
