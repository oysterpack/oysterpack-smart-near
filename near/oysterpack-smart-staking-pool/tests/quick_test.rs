#[test]
fn quick_test() {
    let left: u128 = 352309736455025543172304217827;
    let right: u128 = 350961419839521556191903529545;

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
