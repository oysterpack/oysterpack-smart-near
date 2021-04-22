#[test]
fn quick_test() {
    let left: u128 =  `356693490636844421135431859863;
    let right: u128 = 356685089509449107666631859864;
    // 000008401127395313468799999999
    // 000006736804002786064100000005

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
