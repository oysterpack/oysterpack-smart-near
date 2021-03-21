use enumflags2::{bitflags, make_bitflags, BitFlags};

#[bitflags]
#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialEq)]
enum Test {
    A = 0b0001,
    B = 0b0010,
    C, // unspecified variants pick unused bits automatically
    D = 0b1000,
}

#[test]
fn test() {
    let a_b: BitFlags<Test> = Test::A | Test::B;
    let a_c = Test::A | Test::C;
    let b_c_d = make_bitflags!(Test::{B | C | D});

    let bits: u8 = a_b.bits();
    println!("a_b {}", bits);
    println!("a_c {:?}", a_c);
    println!("b_c_d {}", b_c_d.bits());

    let bit_flags: BitFlags<Test> = BitFlags::from_bits(14).unwrap();
    assert_eq!(b_c_d, bit_flags);
    let bit_flags: BitFlags<Test> =
        BitFlags::from_bits(1).unwrap() | BitFlags::from_bits(2).unwrap();
    assert_eq!(a_b, bit_flags);

    let x: u8 = 0b0001 | 0b0010;
    assert_eq!(0b0011, x);
}
