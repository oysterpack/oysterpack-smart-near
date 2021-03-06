use crate::domain::YoctoNear;
use crate::{ErrCode, ErrorConst};
use near_sdk::env;

const ERR_YOCTONEAR_DEPOSIT_REQUIRED: ErrorConst = ErrorConst(
    ErrCode("YOCTONEAR_DEPOSIT_REQUIRED"),
    "exactly 1 yoctoNEAR must be attached",
);

const ERR_INSUFFICIENT_NEAR_DEPOSIT: ErrCode = ErrCode("INSUFFICIENT_NEAR_DEPOSIT");

/// used to protect functions that transfer value against FCAK calls
pub fn assert_yocto_near_attached() {
    if env::attached_deposit() != 1 {
        env::panic(ERR_YOCTONEAR_DEPOSIT_REQUIRED.to_string().as_bytes())
    }
}

/// used to protect functions that transfer value against FCAK calls
pub fn assert_min_near_attached(min: YoctoNear) {
    assert!(
        env::attached_deposit() >= *min,
        "{} attached NEAR amount is insufficient - minimum required amount is: {} yoctoNEAR",
        ERR_INSUFFICIENT_NEAR_DEPOSIT,
        min
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use oysterpack_smart_near_test::*;

    #[test]
    fn assert_yocto_near_attached_check_passes() {
        let mut ctx = new_context("bob");
        ctx.attached_deposit = 1;
        testing_env!(ctx);

        assert_yocto_near_attached();
    }

    #[test]
    #[should_panic(
        expected = "[ERR] [YOCTONEAR_DEPOSIT_REQUIRED] exactly 1 yoctoNEAR must be attached"
    )]
    fn assert_yocto_near_attached_with_zero_deposit() {
        let ctx = new_context("bob");
        testing_env!(ctx);

        assert_yocto_near_attached();
    }

    #[test]
    #[should_panic(
        expected = "[ERR] [YOCTONEAR_DEPOSIT_REQUIRED] exactly 1 yoctoNEAR must be attached"
    )]
    fn assert_yocto_near_attached_with_2_deposit() {
        let mut ctx = new_context("bob");
        ctx.attached_deposit = 2;
        testing_env!(ctx);

        assert_yocto_near_attached();
    }

    #[test]
    fn assert_min_near_attached_check_passes() {
        let mut ctx = new_context("bob");
        ctx.attached_deposit = 100;
        testing_env!(ctx);

        assert_min_near_attached(100.into());
        assert_min_near_attached(50.into());
    }

    #[test]
    #[should_panic(
        expected = "[ERR] [INSUFFICIENT_NEAR_DEPOSIT] attached NEAR amount is insufficient - minimum required amount is: 200 yoctoNEAR"
    )]
    fn assert_min_near_attached_insufficient_depoist() {
        let mut ctx = new_context("bob");
        ctx.attached_deposit = 199;
        testing_env!(ctx);

        assert_min_near_attached(200.into());
    }
}
