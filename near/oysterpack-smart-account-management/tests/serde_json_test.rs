use oysterpack_smart_near::near_sdk::{
    serde::Deserialize,
    serde_json::{self, *},
};

#[derive(Deserialize, Debug)]
#[serde(crate = "oysterpack_smart_near::near_sdk::serde")]
struct User {
    fingerprint: String,
    location: String,
}

#[test]
fn test() {
    // The type of `j` is `serde_json::Value`
    let j = json!({
        "fingerprint": "0xF9BA143B95FF6D82",
        "location": "Menlo Park, CA",
        "age": 2
    });

    let u: User = serde_json::from_value(j).unwrap();
    println!("{:#?}", u);
}
