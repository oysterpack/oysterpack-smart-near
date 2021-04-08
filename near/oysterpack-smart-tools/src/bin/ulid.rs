use rusty_ulid::Ulid;

fn main() {
    let ulid = Ulid::generate();
    println!("{}", ulid);
    let ulid_u128: u128 = ulid.into();
    println!("{}", ulid_u128);
}
