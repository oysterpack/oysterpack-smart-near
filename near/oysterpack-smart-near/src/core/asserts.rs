use crate::domain::YoctoNear;
use crate::{ErrCode, ErrorConst};
use near_sdk::env;

const ERR_YOCTONEAR_DEPOSIT_REQUIRED: ErrorConst = ErrorConst(
    ErrCode("ERR_YOCTONEAR_DEPOSIT_REQUIRED"),
    "exactly 1 yoctoNEAR must be attached",
);

const ERR_INSUFFICIENT_NEAR_DEPOSIT: ErrCode = ErrCode("ERR_INSUFFICIENT_NEAR_DEPOSIT");

/// used to protect functions that transfer value against FCAK calls
pub fn assert_yocto_near_attached() {
    assert_eq!(
        env::attached_deposit(),
        1,
        "{}",
        ERR_YOCTONEAR_DEPOSIT_REQUIRED
    )
}

/// used to protect functions that transfer value against FCAK calls
pub fn assert_min_near_attached(min: YoctoNear) {
    assert!(
        env::attached_deposit() >= *min,
        "{} attached NEAR amount is insufficient - min required amount is: {}",
        ERR_INSUFFICIENT_NEAR_DEPOSIT,
        min
    )
}
