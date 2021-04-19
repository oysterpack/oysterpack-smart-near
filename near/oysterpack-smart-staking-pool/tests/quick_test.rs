#[test]
fn quick_test() {
    let left: u128 = 2010000056473772592579318987;
    let right: u128 = 2009000385891747347835578875;

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
