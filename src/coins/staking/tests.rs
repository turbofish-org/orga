use super::*;
use crate::coins::MultiShare;
use crate::context::Context;
use crate::orga;
use crate::plugins::Time;
use crate::Result;
use rust_decimal_macros::dec;
use serial_test::serial;
use std::cell::RefCell;
use std::rc::Rc;

#[orga]
#[derive(Debug, Clone)]
struct Simp;
impl Symbol for Simp {
    const INDEX: u8 = 0;
    const NAME: &'static str = "SIMP";
}

fn simp_balance(multishare: &MultiShare) -> Amount {
    Balance::<Simp, Amount>::balance(multishare).unwrap()
}

#[cfg(feature = "abci")]
fn alt_balance(multishare: &MultiShare) -> Amount {
    Balance::<Alt, Amount>::balance(multishare).unwrap()
}

#[cfg(feature = "abci")]
fn setup_state() -> Result<Staking<Simp>> {
    let staking: Staking<Simp> = Staking {
        downtime_jail_seconds: 5,
        unbonding_seconds: UNBONDING_SECONDS,
        max_validators: 100,
        max_offline_blocks: 50_000,
        slash_fraction_downtime: (Amount::new(1) / Amount::new(2))?,
        slash_fraction_double_sign: (Amount::new(1) / Amount::new(2))?,
        min_self_delegation_min: 1,
        ..Default::default()
    };

    let val_ctx = Validators::new(
        Rc::new(RefCell::new(Some(EntryMap::new()))),
        Rc::new(RefCell::new(Some(Default::default()))),
    );
    Context::add(val_ctx);
    Context::add(Time::from_seconds(0));
    Context::add(Events::default());

    Ok(staking)
}

#[test]
#[serial]
fn staking() -> Result<()> {
    let mut staking = Staking::<Simp> {
        downtime_jail_seconds: 5,
        slash_fraction_downtime: (Amount::new(1) / Amount::new(2))?,
        ..Default::default()
    };

    let alice = Address::from_pubkey([0; 33]);
    let alice_con = [4; 32];
    let bob = Address::from_pubkey([1; 33]);
    let bob_con = [5; 32];
    let carol = Address::from_pubkey([2; 33]);
    let dave = Address::from_pubkey([3; 33]);
    let dave_con = [6; 32];

    let val_ctx = Validators::new(
        Rc::new(RefCell::new(Some(EntryMap::new()))),
        Rc::new(RefCell::new(Some(Default::default()))),
    );
    Context::add(val_ctx);
    Context::add(Time::from_seconds(0));

    staking
        .give(Simp::mint(100))
        .expect_err("Cannot give to empty validator set");
    assert_eq!(staking.staked()?, 0);
    staking
        .delegate(alice, alice, Simp::mint(100))
        .expect_err("Should not be able to delegate to an undeclared validator");
    staking.declare(
        alice,
        Declaration {
            consensus_key: alice_con,
            commission: Commission {
                rate: dec!(0.0).into(),
                max: dec!(1.0).into(),
                max_change: dec!(0.1).into(),
            },
            amount: 50.into(),
            min_self_delegation: 1.into(),
            validator_info: vec![].try_into()?,
        },
        50.into(),
    )?;
    staking
        .declare(
            alice,
            Declaration {
                consensus_key: alice_con,
                commission: Commission {
                    rate: dec!(0.0).into(),
                    max: dec!(1.0).into(),
                    max_change: dec!(0.1).into(),
                },
                amount: 50.into(),
                min_self_delegation: 1.into(),
                validator_info: vec![].try_into()?,
            },
            50.into(),
        )
        .expect_err("Should not be able to redeclare validator");
    staking
        .declare(
            carol,
            Declaration {
                consensus_key: alice_con,
                commission: Commission {
                    rate: dec!(0.0).into(),
                    max: dec!(1.0).into(),
                    max_change: dec!(0.1).into(),
                },
                amount: 50.into(),
                min_self_delegation: 1.into(),
                validator_info: vec![].try_into()?,
            },
            50.into(),
        )
        .expect_err("Should not be able to declare using an existing consensus key");

    staking.end_block_step(&Default::default())?;
    assert_eq!(staking.staked()?, 50);
    staking.delegate(alice, alice, Simp::mint(50))?;
    assert_eq!(staking.staked()?, 100);
    staking.declare(
        bob,
        Declaration {
            consensus_key: bob_con,
            commission: Commission {
                rate: dec!(0.0).into(),
                max: dec!(1.0).into(),
                max_change: dec!(0.1).into(),
            },
            amount: 50.into(),
            min_self_delegation: 1.into(),
            validator_info: vec![].try_into()?,
        },
        50.into(),
    )?;
    staking.end_block_step(&Default::default())?;
    assert_eq!(staking.staked()?, 150);

    staking.delegate(bob, bob, Simp::mint(250))?;
    staking.delegate(bob, carol, Simp::mint(100))?;
    staking.delegate(bob, carol, Simp::mint(200))?;
    staking.delegate(bob, dave, Simp::mint(400))?;
    assert_eq!(staking.staked()?, 1100);

    let ctx = Context::resolve::<Validators>().unwrap();
    staking.end_block_step(&Default::default())?;
    assert_eq!(ctx.consensus_key(alice)?.unwrap(), alice_con);

    let alice_vp = ctx.updates.get(&alice_con).unwrap().power;
    assert_eq!(alice_vp, 100);

    let bob_vp = ctx.updates.get(&bob_con).unwrap().power;
    assert_eq!(bob_vp, 1000);

    let alice_self_delegation = staking.get(alice)?.get(alice)?.staked.amount()?;
    assert_eq!(alice_self_delegation, 100);

    let bob_self_delegation = staking.get(bob)?.get(bob)?.staked.amount()?;
    assert_eq!(bob_self_delegation, 300);

    let carol_to_bob_delegation = staking.get(bob)?.get(carol)?.staked.amount()?;
    assert_eq!(carol_to_bob_delegation, 300);

    let alice_val_balance = staking.get_mut(alice)?.staked()?;
    assert_eq!(alice_val_balance, 100);

    let bob_val_balance = staking.get_mut(bob)?.staked()?;
    assert_eq!(bob_val_balance, 1000);

    // Big block rewards, doubling all balances
    staking.give(Simp::mint(600))?;
    staking.give(Simp::mint(500))?;
    assert_eq!(staking.staked()?, 1100);

    let alice_liquid = simp_balance(&staking.get(alice)?.get(alice)?.liquid);
    assert_eq!(alice_liquid, 100);

    let carol_to_bob_delegation = staking.get(bob)?.get(carol)?.staked.amount()?;
    assert_eq!(carol_to_bob_delegation, 300);
    let carol_to_bob_liquid = simp_balance(&staking.get(bob)?.get(carol)?.liquid);
    assert_eq!(carol_to_bob_liquid, 300);

    let bob_val_balance = staking.get_mut(bob)?.staked()?;
    assert_eq!(bob_val_balance, 1000);

    let bob_vp = ctx.updates.get(&bob_con).unwrap().power;
    assert_eq!(bob_vp, 1000);

    // Bob gets slashed 50%
    staking.punish_downtime(bob)?;

    staking.end_block_step(&Default::default())?;
    // Bob has been jailed and should no longer have any voting power
    let bob_vp = ctx.updates.get(&bob_con).unwrap().power;
    assert_eq!(bob_vp, 0);

    staking
        .deduct(bob, dave, 401, Simp::INDEX)
        .expect_err("Dave has not unbonded coins yet");
    // Bob's staked coins should no longer be present in the global staking
    // balance
    assert_eq!(staking.staked()?, 100);

    // Carol can still withdraw her 300 coins from Bob's jailed validator
    {
        staking.unbond(bob, carol, 150)?;
        assert_eq!(staking.staked()?, 100);
        staking
            .deduct(bob, carol, 450, Simp::INDEX)
            .expect_err("Should not be able to take coins before unbonding period has elapsed");
        assert_eq!(staking.staked()?, 100);
        Context::add(Time::from_seconds(10));
        staking.deduct(bob, carol, 450, Simp::INDEX)?;
    }

    {
        // Bob withdraws a third of his self-delegation
        staking.unbond(bob, bob, 100)?;
        Context::add(Time::from_seconds(20));
        staking.deduct(bob, bob, 100, Simp::INDEX)?;
        staking
            .unbond(bob, bob, 201)
            .expect_err("Should not be able to unbond more than we have staked");

        staking.unbond(bob, bob, 50)?;
        Context::add(Time::from_seconds(30));
        staking
            .deduct(bob, bob, 501, Simp::INDEX)
            .expect_err("Should not be able to take more than we have unbonded");
        staking.deduct(bob, bob, 350, Simp::INDEX)?;
    }

    assert_eq!(staking.staked()?, 100);
    let alice_liquid = simp_balance(&staking.get(alice)?.get(alice)?.liquid);
    assert_eq!(alice_liquid, 100);
    let alice_staked = staking.get(alice)?.get(alice)?.staked.amount()?;
    assert_eq!(alice_staked, 100);

    // More block reward, but bob's delegators are jailed and should not
    // earn from it
    staking.give(Simp::mint(200))?;
    assert_eq!(staking.staked()?, 100);
    let alice_val_balance = staking.get_mut(alice)?.staked()?;
    assert_eq!(alice_val_balance, 100);
    let alice_liquid = simp_balance(&staking.get(alice)?.get(alice)?.liquid);
    assert_eq!(alice_liquid, 300);

    staking
        .unbond(bob, dave, 401)
        .expect_err("Dave should only have 400 unbondable coins");

    staking.unbond(bob, dave, 200)?;
    // Bob slashed another 50% while Dave unbonds
    staking.punish_downtime(bob)?;

    Context::add(Time::from_seconds(40));
    staking.deduct(bob, dave, 500, Simp::INDEX)?;

    assert_eq!(staking.staked()?, 100);
    staking.declare(
        dave,
        Declaration {
            consensus_key: dave_con,
            commission: Commission {
                rate: dec!(0.0).into(),
                max: dec!(1.0).into(),
                max_change: dec!(0.1).into(),
            },
            amount: 300.into(),
            min_self_delegation: 1.into(),
            validator_info: vec![].try_into()?,
        },
        300.into(),
    )?;
    staking.end_block_step(&Default::default())?;
    assert_eq!(staking.staked()?, 400);
    staking.end_block_step(&Default::default())?;
    assert_eq!(ctx.updates.get(&alice_con).unwrap().power, 100);
    assert_eq!(ctx.updates.get(&dave_con).unwrap().power, 300);
    staking.delegate(dave, carol, 300.into())?;
    assert_eq!(staking.staked()?, 700);

    staking.end_block_step(&Default::default())?;
    assert_eq!(ctx.updates.get(&dave_con).unwrap().power, 600);
    staking.unbond(dave, dave, 150)?;
    assert_eq!(staking.staked()?, 550);
    staking.end_block_step(&Default::default())?;
    assert_eq!(ctx.updates.get(&dave_con).unwrap().power, 450);

    // Test commissions
    let edith = Address::from_pubkey([7; 33]);
    let edith_con = [201; 32];

    staking.declare(
        edith,
        Declaration {
            consensus_key: edith_con,
            commission: Commission {
                rate: dec!(0.5).into(),
                max: dec!(1.0).into(),
                max_change: dec!(0.1).into(),
            },
            amount: 550.into(),
            min_self_delegation: 1.into(),
            validator_info: vec![].try_into()?,
        },
        550.into(),
    )?;

    staking.delegate(edith, carol, 550.into())?;

    staking.get_mut(edith)?.give(Simp::mint(500))?;

    let edith_liquid = simp_balance(&staking.get(edith)?.get(edith)?.liquid);
    assert_eq!(edith_liquid, 375);
    let carol_liquid = simp_balance(&staking.get(edith)?.get(carol)?.liquid);
    assert_eq!(carol_liquid, 125);

    staking.punish_double_sign(dave)?;
    staking.end_block_step(&Default::default())?;
    assert_eq!(ctx.updates.get(&dave_con).unwrap().power, 0);

    Ok(())
}

#[cfg(feature = "abci")]
#[test]
#[serial]
fn val_size_limit() -> Result<()> {
    let mut staking: Staking<Simp> = Default::default();

    Context::add(Validators::new(
        Rc::new(RefCell::new(Some(EntryMap::new()))),
        Rc::new(RefCell::new(Some(Default::default()))),
    ));

    Context::add(Time::from_seconds(0));
    let ctx = Context::resolve::<Validators>().unwrap();
    staking.max_validators = 2;

    for i in 1..10 {
        staking.declare(
            Address::from_pubkey([i; 33]),
            Declaration {
                consensus_key: [i; 32],
                commission: Commission {
                    rate: dec!(0.0).into(),
                    max: dec!(1.0).into(),
                    max_change: dec!(0.1).into(),
                },
                amount: Amount::new(i as u64 * 100),
                min_self_delegation: 1.into(),
                validator_info: vec![].try_into()?,
            },
            Amount::new(i as u64 * 100).into(),
        )?;
    }
    staking.end_block_step(&Default::default())?;
    assert_eq!(staking.staked()?, 1700);
    assert!(!ctx.updates.contains_key(&[7; 32]));
    assert_eq!(ctx.updates.get(&[8; 32]).unwrap().power, 800);
    assert_eq!(ctx.updates.get(&[9; 32]).unwrap().power, 900);
    staking.give(Simp::mint(3400))?;
    assert_eq!(
        simp_balance(
            &staking
                .get(Address::from_pubkey([4; 33]))?
                .get(Address::from_pubkey([4; 33]))?
                .liquid
        ),
        0
    );
    assert_eq!(
        simp_balance(
            &staking
                .get(Address::from_pubkey([8; 33]))?
                .get(Address::from_pubkey([8; 33]))?
                .liquid
        ),
        1600
    );
    assert_eq!(
        simp_balance(
            &staking
                .get(Address::from_pubkey([9; 33]))?
                .get(Address::from_pubkey([9; 33]))?
                .liquid
        ),
        1800
    );

    staking.declare(
        Address::from_pubkey([10; 33]),
        Declaration {
            consensus_key: [10; 32],
            commission: Commission {
                rate: dec!(0.0).into(),
                max: dec!(1.0).into(),
                max_change: dec!(0.1).into(),
            },
            amount: 1000.into(),
            min_self_delegation: 1.into(),
            validator_info: vec![].try_into()?,
        },
        1000.into(),
    )?;

    staking.end_block_step(&Default::default())?;

    assert_eq!(ctx.updates.get(&[8; 32]).unwrap().power, 0);
    assert_eq!(ctx.updates.get(&[9; 32]).unwrap().power, 900);
    assert_eq!(ctx.updates.get(&[10; 32]).unwrap().power, 1000);
    staking.give(Simp::mint(1900))?;

    let balance: Amount = simp_balance(
        &staking
            .get(Address::from_pubkey([8; 33]))?
            .get(Address::from_pubkey([8; 33]))?
            .liquid,
    );
    assert_eq!(balance, 1600);

    let balance: Amount = simp_balance(
        &staking
            .get(Address::from_pubkey([9; 33]))?
            .get(Address::from_pubkey([9; 33]))?
            .liquid,
    );
    assert_eq!(balance, 2700);

    let balance: Amount = simp_balance(
        &staking
            .get(Address::from_pubkey([10; 33]))?
            .get(Address::from_pubkey([10; 33]))?
            .liquid,
    );
    assert_eq!(balance, 1000);

    Ok(())
}

#[cfg(feature = "abci")]
#[test]
#[serial]
fn undelegate() -> Result<()> {
    let mut staking = setup_state()?;

    let val_0 = Address::from_pubkey([0; 33]);

    staking.declare(
        val_0,
        Declaration {
            consensus_key: [0; 32],
            commission: Commission {
                rate: dec!(0.0).into(),
                max: dec!(1.0).into(),
                max_change: dec!(0.1).into(),
            },
            amount: Amount::new(100),
            min_self_delegation: 1.into(),
            validator_info: vec![].try_into()?,
        },
        Amount::new(100).into(),
    )?;

    let ctx = Context::resolve::<Validators>().unwrap();
    let staker = Address::from_pubkey([1; 33]);

    staking.delegate(val_0, staker, 100.into())?;

    staking.end_block_step(&Default::default())?;

    assert_eq!(ctx.updates.get(&[0; 32]).unwrap().power, 200);

    staking.unbond(Address::from_pubkey([0; 33]), staker, Amount::from(100))?;

    staking.end_block_step(&Default::default())?;
    assert_eq!(ctx.updates.get(&[0; 32]).unwrap().power, 100);
    assert_eq!(staking.get(val_0)?.get(staker)?.staked.amount()?, 0);

    Ok(())
}

#[cfg(feature = "abci")]
#[test]
#[serial]
fn undelegate_slash_before_unbond() -> Result<()> {
    let mut staking = setup_state()?;

    let val_0 = Address::from_pubkey([0; 33]);

    staking.declare(
        val_0,
        Declaration {
            consensus_key: [0; 32],
            commission: Commission {
                rate: dec!(0.0).into(),
                max: dec!(1.0).into(),
                max_change: dec!(0.1).into(),
            },
            amount: Amount::new(100),
            min_self_delegation: 1.into(),
            validator_info: vec![].try_into()?,
        },
        Amount::new(100).into(),
    )?;

    let ctx = Context::resolve::<Validators>().unwrap();
    let staker = Address::from_pubkey([1; 33]);

    staking.delegate(val_0, staker, 100.into())?;

    staking.end_block_step(&Default::default())?;

    assert_eq!(ctx.updates.get(&[0; 32]).unwrap().power, 200);

    staking.unbond(Address::from_pubkey([0; 33]), staker, Amount::from(100))?;

    staking.end_block_step(&Default::default())?;

    staking.punish_double_sign(Address::from_pubkey([0; 33]))?;
    staking.end_block_step(&Default::default())?;

    assert_eq!(ctx.updates.get(&[0; 32]).unwrap().power, 0);
    assert_eq!(staking.get(val_0)?.get(staker)?.staked.amount()?, 0);

    assert_eq!(staking.get_mut(val_0)?.delegators.balance()?.amount()?, 50);

    Context::add(Time::from_seconds(10));
    staking.end_block_step(&Default::default())?;
    let balance: Amount = simp_balance(&staking.get(val_0)?.get(staker)?.liquid);
    assert_eq!(balance, 50);

    Ok(())
}

#[cfg(feature = "abci")]
#[test]
#[serial]
fn undelegate_slash_after_unbond() -> Result<()> {
    let mut staking = setup_state()?;

    let val_0 = Address::from_pubkey([0; 33]);

    staking.declare(
        val_0,
        Declaration {
            consensus_key: [0; 32],
            commission: Commission {
                rate: dec!(0.0).into(),
                max: dec!(1.0).into(),
                max_change: dec!(0.1).into(),
            },
            amount: Amount::new(100),
            min_self_delegation: 1.into(),
            validator_info: vec![].try_into()?,
        },
        Amount::new(100).into(),
    )?;

    let ctx = Context::resolve::<Validators>().unwrap();
    let staker = Address::from_pubkey([1; 33]);

    staking.delegate(val_0, staker, 100.into())?;

    staking.end_block_step(&Default::default())?;

    assert_eq!(ctx.updates.get(&[0; 32]).unwrap().power, 200);

    staking.unbond(Address::from_pubkey([0; 33]), staker, Amount::from(100))?;

    Context::add(Time::from_seconds(10));
    staking.end_block_step(&Default::default())?;

    staking.punish_double_sign(Address::from_pubkey([0; 33]))?;
    staking.end_block_step(&Default::default())?;

    assert_eq!(ctx.updates.get(&[0; 32]).unwrap().power, 0);
    assert_eq!(staking.get(val_0)?.get(staker)?.staked.amount()?, 0);

    assert_eq!(staking.get_mut(val_0)?.delegators.balance()?.amount()?, 50);

    staking.end_block_step(&Default::default())?;
    let balance: Amount = simp_balance(&staking.get(val_0)?.get(staker)?.liquid);
    assert_eq!(balance, 100);

    Ok(())
}

#[cfg(feature = "abci")]
#[test]
#[serial]
fn redelegate() -> Result<()> {
    let mut staking = setup_state()?;

    for i in 0..2 {
        staking.declare(
            Address::from_pubkey([i; 33]),
            Declaration {
                consensus_key: [i; 32],
                commission: Commission {
                    rate: dec!(0.0).into(),
                    max: dec!(1.0).into(),
                    max_change: dec!(0.1).into(),
                },
                amount: Amount::new(100),
                min_self_delegation: 1.into(),
                validator_info: vec![].try_into()?,
            },
            Amount::new(100).into(),
        )?;
    }

    let ctx = Context::resolve::<Validators>().unwrap();
    let staker = Address::from_pubkey([2; 33]);

    staking.delegate(Address::from_pubkey([0; 33]), staker, 100.into())?;

    staking.end_block_step(&Default::default())?;

    assert_eq!(ctx.updates.get(&[0; 32]).unwrap().power, 200);

    staking.redelegate(
        Address::from_pubkey([0; 33]),
        Address::from_pubkey([1; 33]),
        staker,
        Amount::from(100),
    )?;

    staking.end_block_step(&Default::default())?;
    assert_eq!(ctx.updates.get(&[0; 32]).unwrap().power, 100);
    assert_eq!(ctx.updates.get(&[1; 32]).unwrap().power, 200);

    Ok(())
}

#[cfg(feature = "abci")]
#[test]
#[serial]
fn redelegate_slash_before_unbond() -> Result<()> {
    let mut staking = setup_state()?;

    for i in 0..2 {
        staking.declare(
            Address::from_pubkey([i; 33]),
            Declaration {
                consensus_key: [i; 32],
                commission: Commission {
                    rate: dec!(0.0).into(),
                    max: dec!(1.0).into(),
                    max_change: dec!(0.1).into(),
                },
                amount: Amount::new(100),
                min_self_delegation: 1.into(),
                validator_info: vec![].try_into()?,
            },
            Amount::new(100).into(),
        )?;
    }

    let ctx = Context::resolve::<Validators>().unwrap();
    let staker = Address::from_pubkey([2; 33]);

    staking.delegate(Address::from_pubkey([0; 33]), staker, 100.into())?;

    staking.end_block_step(&Default::default())?;

    assert_eq!(ctx.updates.get(&[0; 32]).unwrap().power, 200);

    staking.redelegate(
        Address::from_pubkey([0; 33]),
        Address::from_pubkey([1; 33]),
        staker,
        Amount::from(100),
    )?;

    staking.end_block_step(&Default::default())?;
    assert_eq!(ctx.updates.get(&[0; 32]).unwrap().power, 100);
    assert_eq!(ctx.updates.get(&[1; 32]).unwrap().power, 200);

    staking.punish_double_sign(Address::from_pubkey([0; 33]))?;
    staking.end_block_step(&Default::default())?;

    let val_0 = Address::from_pubkey([0; 33]);
    let val_1 = Address::from_pubkey([1; 33]);

    assert_eq!(staking.get(val_0)?.get(staker)?.staked.amount()?, 0);
    assert_eq!(staking.get(val_1)?.get(staker)?.staked.amount()?, 50);

    assert_eq!(staking.get_mut(val_0)?.delegators.balance()?.amount()?, 50);
    assert_eq!(staking.get_mut(val_1)?.delegators.balance()?.amount()?, 150);

    Ok(())
}

#[cfg(feature = "abci")]
#[test]
#[serial]
fn redelegate_slash_after_unbond() -> Result<()> {
    let mut staking = setup_state()?;

    for i in 0..2 {
        staking.declare(
            Address::from_pubkey([i; 33]),
            Declaration {
                consensus_key: [i; 32],
                commission: Commission {
                    rate: dec!(0.0).into(),
                    max: dec!(1.0).into(),
                    max_change: dec!(0.1).into(),
                },
                amount: Amount::new(100),
                min_self_delegation: 1.into(),
                validator_info: vec![].try_into()?,
            },
            Amount::new(100).into(),
        )?;
    }

    let ctx = Context::resolve::<Validators>().unwrap();
    let staker = Address::from_pubkey([2; 33]);

    staking.delegate(Address::from_pubkey([0; 33]), staker, 100.into())?;

    staking.end_block_step(&Default::default())?;

    assert_eq!(ctx.updates.get(&[0; 32]).unwrap().power, 200);

    staking.redelegate(
        Address::from_pubkey([0; 33]),
        Address::from_pubkey([1; 33]),
        staker,
        Amount::from(100),
    )?;

    staking.end_block_step(&Default::default())?;
    assert_eq!(ctx.updates.get(&[0; 32]).unwrap().power, 100);
    assert_eq!(ctx.updates.get(&[1; 32]).unwrap().power, 200);

    Context::add(Time::from_seconds(10));
    staking.end_block_step(&Default::default())?;

    staking.punish_double_sign(Address::from_pubkey([0; 33]))?;
    staking.end_block_step(&Default::default())?;

    let val_0 = Address::from_pubkey([0; 33]);
    let val_1 = Address::from_pubkey([1; 33]);

    assert_eq!(staking.get(val_0)?.get(staker)?.staked.amount()?, 0);
    assert_eq!(staking.get(val_1)?.get(staker)?.staked.amount()?, 100);

    assert_eq!(staking.get_mut(val_0)?.delegators.balance()?.amount()?, 50);
    assert_eq!(staking.get_mut(val_1)?.delegators.balance()?.amount()?, 200);

    Ok(())
}

#[cfg(feature = "abci")]
#[test]
#[serial]
fn redelegation_slash() -> Result<()> {
    let mut staking = setup_state()?;

    for i in 0..3 {
        staking.declare(
            Address::from_pubkey([i; 33]),
            Declaration {
                consensus_key: [i; 32],
                commission: Commission {
                    rate: dec!(0.0).into(),
                    max: dec!(1.0).into(),
                    max_change: dec!(0.1).into(),
                },
                amount: Amount::new(100),
                min_self_delegation: 1.into(),
                validator_info: vec![].try_into()?,
            },
            Amount::new(100).into(),
        )?;
    }

    let ctx = Context::resolve::<Validators>().unwrap();
    let staker = Address::from_pubkey([3; 33]);

    staking.delegate(Address::from_pubkey([0; 33]), staker, 100.into())?;
    staking.delegate(Address::from_pubkey([1; 33]), staker, 100.into())?;
    staking.delegate(Address::from_pubkey([2; 33]), staker, 100.into())?;

    staking.end_block_step(&Default::default())?;

    staking.redelegate(
        Address::from_pubkey([0; 33]),
        Address::from_pubkey([2; 33]),
        staker,
        Amount::from(50),
    )?;
    staking.end_block_step(&Default::default())?;

    assert_eq!(ctx.updates.get(&[0; 32]).unwrap().power, 150);
    assert_eq!(ctx.updates.get(&[1; 32]).unwrap().power, 200);
    assert_eq!(ctx.updates.get(&[2; 32]).unwrap().power, 250);

    staking.redelegate(
        Address::from_pubkey([1; 33]),
        Address::from_pubkey([0; 33]),
        staker,
        Amount::from(30),
    )?;
    staking.end_block_step(&Default::default())?;

    assert_eq!(ctx.updates.get(&[0; 32]).unwrap().power, 180);
    assert_eq!(ctx.updates.get(&[1; 32]).unwrap().power, 170);
    assert_eq!(ctx.updates.get(&[2; 32]).unwrap().power, 250);

    staking.redelegate(
        Address::from_pubkey([1; 33]),
        Address::from_pubkey([2; 33]),
        staker,
        Amount::from(30),
    )?;
    staking.end_block_step(&Default::default())?;

    assert_eq!(ctx.updates.get(&[0; 32]).unwrap().power, 180);
    assert_eq!(ctx.updates.get(&[1; 32]).unwrap().power, 140);
    assert_eq!(ctx.updates.get(&[2; 32]).unwrap().power, 280);

    staking.punish_double_sign(Address::from_pubkey([1; 33]))?;
    staking.end_block_step(&Default::default())?;

    let val_0 = Address::from_pubkey([0; 33]);
    let val_1 = Address::from_pubkey([1; 33]);
    let val_2 = Address::from_pubkey([2; 33]);

    assert_eq!(staking.get(val_0)?.get(staker)?.staked.amount()?, 65);
    assert_eq!(staking.get(val_1)?.get(staker)?.staked.amount()?, 20);
    assert_eq!(staking.get(val_2)?.get(staker)?.staked.amount()?, 165);

    assert_eq!(staking.get_mut(val_0)?.delegators.balance()?.amount()?, 165);
    assert_eq!(staking.get_mut(val_1)?.delegators.balance()?.amount()?, 70);
    assert_eq!(staking.get_mut(val_2)?.delegators.balance()?.amount()?, 265);

    staking.punish_double_sign(Address::from_pubkey([0; 33]))?;
    staking.end_block_step(&Default::default())?;
    assert_eq!(staking.get(val_0)?.get(staker)?.staked.amount()?, 32);
    assert_eq!(staking.get(val_1)?.get(staker)?.staked.amount()?, 20);
    assert_eq!(staking.get(val_2)?.get(staker)?.staked.amount()?, 140);

    assert_eq!(staking.get_mut(val_0)?.delegators.balance()?.amount()?, 82);
    assert_eq!(staking.get_mut(val_1)?.delegators.balance()?.amount()?, 70);
    assert_eq!(staking.get_mut(val_2)?.delegators.balance()?.amount()?, 240);

    Ok(())
}

#[cfg(feature = "abci")]
#[test]
#[serial]
fn redelegation_double_slash() -> Result<()> {
    let mut staking = setup_state()?;

    for i in 0..2 {
        staking.declare(
            Address::from_pubkey([i; 33]),
            Declaration {
                consensus_key: [i; 32],
                commission: Commission {
                    rate: dec!(0.0).into(),
                    max: dec!(1.0).into(),
                    max_change: dec!(0.1).into(),
                },
                amount: Amount::new(100),
                min_self_delegation: 1.into(),
                validator_info: vec![].try_into()?,
            },
            Amount::new(100).into(),
        )?;
    }

    let staker = Address::from_pubkey([2; 33]);
    let val_0 = Address::from_pubkey([0; 33]);
    let val_1 = Address::from_pubkey([1; 33]);

    staking.delegate(Address::from_pubkey([0; 33]), staker, 100.into())?;

    staking.end_block_step(&Default::default())?;

    staking.redelegate(
        Address::from_pubkey([0; 33]),
        Address::from_pubkey([1; 33]),
        staker,
        Amount::from(100),
    )?;
    staking.end_block_step(&Default::default())?;

    staking.punish_double_sign(Address::from_pubkey([0; 33]))?;
    staking.punish_double_sign(Address::from_pubkey([1; 33]))?;

    staking.end_block_step(&Default::default())?;

    assert_eq!(staking.get(val_0)?.get(staker)?.staked.amount()?, 0);
    assert_eq!(staking.get(val_1)?.get(staker)?.staked.amount()?, 25);

    Ok(())
}

#[cfg(feature = "abci")]
#[test]
#[serial]
fn redelegation_slash_with_unbond() -> Result<()> {
    let mut staking = setup_state()?;

    for i in 0..3 {
        staking.declare(
            Address::from_pubkey([i; 33]),
            Declaration {
                consensus_key: [i; 32],
                commission: Commission {
                    rate: dec!(0.0).into(),
                    max: dec!(1.0).into(),
                    max_change: dec!(0.1).into(),
                },
                amount: Amount::new(100),
                min_self_delegation: 1.into(),
                validator_info: vec![].try_into()?,
            },
            Amount::new(100).into(),
        )?;
    }

    let ctx = Context::resolve::<Validators>().unwrap();
    let staker = Address::from_pubkey([3; 33]);

    staking.delegate(Address::from_pubkey([0; 33]), staker, 100.into())?;
    staking.delegate(Address::from_pubkey([1; 33]), staker, 100.into())?;
    staking.delegate(Address::from_pubkey([2; 33]), staker, 100.into())?;

    staking.end_block_step(&Default::default())?;

    staking.redelegate(
        Address::from_pubkey([0; 33]),
        Address::from_pubkey([2; 33]),
        staker,
        Amount::from(50),
    )?;
    staking.end_block_step(&Default::default())?;

    assert_eq!(ctx.updates.get(&[0; 32]).unwrap().power, 150);
    assert_eq!(ctx.updates.get(&[1; 32]).unwrap().power, 200);
    assert_eq!(ctx.updates.get(&[2; 32]).unwrap().power, 250);

    staking.redelegate(
        Address::from_pubkey([1; 33]),
        Address::from_pubkey([0; 33]),
        staker,
        Amount::from(30),
    )?;
    staking.end_block_step(&Default::default())?;

    assert_eq!(ctx.updates.get(&[0; 32]).unwrap().power, 180);
    assert_eq!(ctx.updates.get(&[1; 32]).unwrap().power, 170);
    assert_eq!(ctx.updates.get(&[2; 32]).unwrap().power, 250);

    staking.redelegate(
        Address::from_pubkey([1; 33]),
        Address::from_pubkey([2; 33]),
        staker,
        Amount::from(30),
    )?;
    staking.end_block_step(&Default::default())?;

    assert_eq!(ctx.updates.get(&[0; 32]).unwrap().power, 180);
    assert_eq!(ctx.updates.get(&[1; 32]).unwrap().power, 140);
    assert_eq!(ctx.updates.get(&[2; 32]).unwrap().power, 280);

    staking.punish_double_sign(Address::from_pubkey([1; 33]))?;
    staking.end_block_step(&Default::default())?;

    let val_0 = Address::from_pubkey([0; 33]);
    let val_1 = Address::from_pubkey([1; 33]);
    let val_2 = Address::from_pubkey([2; 33]);

    assert_eq!(staking.get(val_0)?.get(staker)?.staked.amount()?, 65);
    assert_eq!(staking.get(val_1)?.get(staker)?.staked.amount()?, 20);
    assert_eq!(staking.get(val_2)?.get(staker)?.staked.amount()?, 165);

    assert_eq!(staking.get_mut(val_0)?.delegators.balance()?.amount()?, 165);
    assert_eq!(staking.get_mut(val_1)?.delegators.balance()?.amount()?, 70);
    assert_eq!(staking.get_mut(val_2)?.delegators.balance()?.amount()?, 265);

    staking.unbond(val_2, staker, Amount::from(100))?;
    staking.end_block_step(&Default::default())?;

    staking.punish_double_sign(Address::from_pubkey([0; 33]))?;
    staking.end_block_step(&Default::default())?;

    assert_eq!(staking.get(val_0)?.get(staker)?.staked.amount()?, 32);
    assert_eq!(staking.get(val_1)?.get(staker)?.staked.amount()?, 20);
    assert_eq!(staking.get(val_2)?.get(staker)?.staked.amount()?, 40);

    assert_eq!(staking.get_mut(val_0)?.delegators.balance()?.amount()?, 82);
    assert_eq!(staking.get_mut(val_1)?.delegators.balance()?.amount()?, 70);
    assert_eq!(staking.get_mut(val_2)?.delegators.balance()?.amount()?, 140);

    Context::add(Time::from_seconds(10));
    staking.end_block_step(&Default::default())?;

    assert_eq!(simp_balance(&staking.get(val_2)?.get(staker)?.liquid), 100);

    Ok(())
}

#[cfg(feature = "abci")]
#[test]
#[serial]
fn redelegation_slash_with_slash_unbond_overflow() -> Result<()> {
    let mut staking = setup_state()?;

    for i in 0..3 {
        staking.declare(
            Address::from_pubkey([i; 33]),
            Declaration {
                consensus_key: [i; 32],
                commission: Commission {
                    rate: dec!(0.0).into(),
                    max: dec!(1.0).into(),
                    max_change: dec!(0.1).into(),
                },
                amount: Amount::new(100),
                min_self_delegation: 1.into(),
                validator_info: vec![].try_into()?,
            },
            Amount::new(100).into(),
        )?;
    }

    let ctx = Context::resolve::<Validators>().unwrap();
    let staker = Address::from_pubkey([3; 33]);

    staking.delegate(Address::from_pubkey([0; 33]), staker, 100.into())?;
    staking.delegate(Address::from_pubkey([1; 33]), staker, 100.into())?;
    staking.delegate(Address::from_pubkey([2; 33]), staker, 100.into())?;

    staking.end_block_step(&Default::default())?;

    staking.redelegate(
        Address::from_pubkey([0; 33]),
        Address::from_pubkey([2; 33]),
        staker,
        Amount::from(50),
    )?;
    staking.end_block_step(&Default::default())?;

    assert_eq!(ctx.updates.get(&[0; 32]).unwrap().power, 150);
    assert_eq!(ctx.updates.get(&[1; 32]).unwrap().power, 200);
    assert_eq!(ctx.updates.get(&[2; 32]).unwrap().power, 250);

    staking.redelegate(
        Address::from_pubkey([1; 33]),
        Address::from_pubkey([0; 33]),
        staker,
        Amount::from(30),
    )?;
    staking.end_block_step(&Default::default())?;

    assert_eq!(ctx.updates.get(&[0; 32]).unwrap().power, 180);
    assert_eq!(ctx.updates.get(&[1; 32]).unwrap().power, 170);
    assert_eq!(ctx.updates.get(&[2; 32]).unwrap().power, 250);

    staking.redelegate(
        Address::from_pubkey([1; 33]),
        Address::from_pubkey([2; 33]),
        staker,
        Amount::from(30),
    )?;
    staking.end_block_step(&Default::default())?;

    assert_eq!(ctx.updates.get(&[0; 32]).unwrap().power, 180);
    assert_eq!(ctx.updates.get(&[1; 32]).unwrap().power, 140);
    assert_eq!(ctx.updates.get(&[2; 32]).unwrap().power, 280);

    staking.punish_double_sign(Address::from_pubkey([1; 33]))?;
    staking.end_block_step(&Default::default())?;

    let val_0 = Address::from_pubkey([0; 33]);
    let val_1 = Address::from_pubkey([1; 33]);
    let val_2 = Address::from_pubkey([2; 33]);

    assert_eq!(staking.get(val_0)?.get(staker)?.staked.amount()?, 65);
    assert_eq!(staking.get(val_1)?.get(staker)?.staked.amount()?, 20);
    assert_eq!(staking.get(val_2)?.get(staker)?.staked.amount()?, 165);

    assert_eq!(staking.get_mut(val_0)?.delegators.balance()?.amount()?, 165);
    assert_eq!(staking.get_mut(val_1)?.delegators.balance()?.amount()?, 70);
    assert_eq!(staking.get_mut(val_2)?.delegators.balance()?.amount()?, 265);

    for _ in 0..15 {
        staking.unbond(val_2, staker, Amount::from(10))?;
    }

    staking.end_block_step(&Default::default())?;

    assert_eq!(staking.get(val_2)?.get(staker)?.staked.amount()?, 15);

    staking.punish_double_sign(Address::from_pubkey([0; 33]))?;
    staking.end_block_step(&Default::default())?;

    assert_eq!(staking.get(val_0)?.get(staker)?.staked.amount()?, 32);
    assert_eq!(staking.get(val_1)?.get(staker)?.staked.amount()?, 20);
    assert_eq!(staking.get(val_2)?.get(staker)?.staked.amount()?, 0);

    assert_eq!(staking.get_mut(val_0)?.delegators.balance()?.amount()?, 82);
    assert_eq!(staking.get_mut(val_1)?.delegators.balance()?.amount()?, 70);
    assert_eq!(staking.get_mut(val_2)?.delegators.balance()?.amount()?, 100);

    Context::add(Time::from_seconds(10));
    staking.end_block_step(&Default::default())?;

    assert_eq!(simp_balance(&staking.get(val_2)?.get(staker)?.liquid), 140);

    Ok(())
}

#[cfg(feature = "abci")]
#[test]
#[serial]
#[should_panic]
fn delegate_slashed_fail() {
    let mut staking = setup_state().unwrap();

    staking
        .declare(
            Address::from_pubkey([0; 33]),
            Declaration {
                consensus_key: [0; 32],
                commission: Commission {
                    rate: dec!(0.0).into(),
                    max: dec!(1.0).into(),
                    max_change: dec!(0.1).into(),
                },
                amount: Amount::new(0),
                min_self_delegation: 1.into(),
                validator_info: vec![].try_into().unwrap(),
            },
            Amount::new(100).into(),
        )
        .unwrap();

    let staker = Address::from_pubkey([3; 33]);

    staking.end_block_step(&Default::default()).unwrap();

    staking
        .punish_double_sign(Address::from_pubkey([0; 33]))
        .unwrap();
    staking.end_block_step(&Default::default()).unwrap();

    staking
        .delegate(Address::from_pubkey([0; 33]), staker, 100.into())
        .unwrap();
}

#[cfg(feature = "abci")]
#[test]
#[serial]
fn min_delegation_fall_below() -> Result<()> {
    let mut staking = setup_state()?;

    staking.declare(
        Address::from_pubkey([0; 33]),
        Declaration {
            consensus_key: [0; 32],
            commission: Commission {
                rate: dec!(0.0).into(),
                max: dec!(1.0).into(),
                max_change: dec!(0.1).into(),
            },
            amount: Amount::new(0),
            min_self_delegation: 75.into(),
            validator_info: vec![].try_into()?,
        },
        Amount::new(100).into(),
    )?;

    let ctx = Context::resolve::<Validators>().unwrap();
    let val_0 = Address::from_pubkey([0; 33]);

    staking.end_block_step(&Default::default())?;

    staking.punish_downtime(Address::from_pubkey([0; 33]))?;
    assert_eq!(staking.get_mut(val_0)?.delegators.balance()?.amount()?, 50);
    Context::add(Time::from_seconds(10));

    staking.end_block_step(&Default::default())?;

    staking.get_mut(val_0)?.try_unjail()?;
    staking.update_vp(val_0)?;

    staking.end_block_step(&Default::default())?;

    assert_eq!(ctx.updates.get(&[0; 32]).unwrap().power, 0);
    staking.delegate(val_0, val_0, 25.into())?;

    staking.end_block_step(&Default::default())?;

    assert_eq!(staking.get_mut(val_0)?.delegators.balance()?.amount()?, 75);
    assert_eq!(ctx.updates.get(&[0; 32]).unwrap().power, 75);

    Ok(())
}

#[cfg(feature = "abci")]
#[test]
#[serial]
fn min_delegation_fall_below_unbond() -> Result<()> {
    let mut staking = setup_state()?;

    for i in 0..2 {
        staking.declare(
            Address::from_pubkey([i; 33]),
            Declaration {
                consensus_key: [i; 32],
                commission: Commission {
                    rate: dec!(0.0).into(),
                    max: dec!(1.0).into(),
                    max_change: dec!(0.1).into(),
                },
                amount: Amount::new(0),
                min_self_delegation: 75.into(),
                validator_info: vec![].try_into()?,
            },
            Amount::new(100).into(),
        )?;
    }

    let ctx = Context::resolve::<Validators>().unwrap();
    let val_0 = Address::from_pubkey([0; 33]);
    let val_1 = Address::from_pubkey([1; 33]);

    staking.end_block_step(&Default::default())?;
    staking.unbond(val_0, val_0, Amount::from(50))?;
    staking.end_block_step(&Default::default())?;

    assert_eq!(staking.get_mut(val_0)?.delegators.balance()?.amount()?, 50);

    staking.end_block_step(&Default::default())?;

    assert_eq!(ctx.updates.get(&[0; 32]).unwrap().power, 0);
    staking.delegate(val_0, val_0, 25.into())?;

    staking.end_block_step(&Default::default())?;

    assert_eq!(staking.get_mut(val_0)?.delegators.balance()?.amount()?, 75);
    assert_eq!(ctx.updates.get(&[0; 32]).unwrap().power, 75);

    staking.redelegate(val_0, val_1, val_0, Amount::from(25))?;

    staking.end_block_step(&Default::default())?;

    assert_eq!(staking.get_mut(val_0)?.delegators.balance()?.amount()?, 50);

    staking.end_block_step(&Default::default())?;

    assert_eq!(ctx.updates.get(&[0; 32]).unwrap().power, 0);
    staking.delegate(val_0, val_0, 25.into())?;

    staking.end_block_step(&Default::default())?;

    assert_eq!(staking.get_mut(val_0)?.delegators.balance()?.amount()?, 75);
    assert_eq!(ctx.updates.get(&[0; 32]).unwrap().power, 75);

    Ok(())
}

#[cfg(feature = "abci")]
#[test]
#[serial]
fn punish_downtime_jailed() -> Result<()> {
    let mut staking = setup_state()?;

    staking.declare(
        Address::from_pubkey([0; 33]),
        Declaration {
            consensus_key: [0; 32],
            commission: Commission {
                rate: dec!(0.0).into(),
                max: dec!(1.0).into(),
                max_change: dec!(0.1).into(),
            },
            amount: Amount::new(0),
            min_self_delegation: 75.into(),
            validator_info: vec![].try_into()?,
        },
        Amount::new(100).into(),
    )?;

    let val_0 = Address::from_pubkey([0; 33]);
    staking.end_block_step(&Default::default())?;

    staking.punish_downtime(Address::from_pubkey([0; 33]))?;
    assert_eq!(staking.get_mut(val_0)?.delegators.balance()?.amount()?, 50);
    staking.end_block_step(&Default::default())?;

    staking.punish_double_sign(val_0)?;
    staking.end_block_step(&Default::default())?;

    assert_eq!(staking.get_mut(val_0)?.delegators.balance()?.amount()?, 25);

    Ok(())
}

#[cfg(feature = "abci")]
#[test]
#[serial]
fn unclaimed_rewards_slash() -> Result<()> {
    let mut staking = setup_state()?;

    staking.declare(
        Address::from_pubkey([0; 33]),
        Declaration {
            consensus_key: [0; 32],
            commission: Commission {
                rate: dec!(0.0).into(),
                max: dec!(1.0).into(),
                max_change: dec!(0.1).into(),
            },
            amount: Amount::new(100),
            min_self_delegation: 1.into(),
            validator_info: vec![].try_into()?,
        },
        Amount::new(100).into(),
    )?;

    let val_0 = Address::from_pubkey([0; 33]);
    let staker = Address::from_pubkey([1; 33]);

    staking.end_block_step(&Default::default())?;

    staking.delegate(val_0, staker, 100.into())?;
    staking.give(Simp::mint(100))?;

    staking.end_block_step(&Default::default())?;

    assert_eq!(simp_balance(&staking.get(val_0)?.get(staker)?.liquid), 50);
    staking.end_block_step(&Default::default())?;

    staking.punish_downtime(Address::from_pubkey([0; 33]))?;

    staking.end_block_step(&Default::default())?;
    assert_eq!(simp_balance(&staking.get(val_0)?.get(staker)?.liquid), 50);

    Ok(())
}

#[cfg(feature = "abci")]
#[test]
#[serial]
fn reward_with_unbond() -> Result<()> {
    let mut staking = setup_state()?;

    for i in 0..2 {
        staking.declare(
            Address::from_pubkey([i; 33]),
            Declaration {
                consensus_key: [i; 32],
                commission: Commission {
                    rate: dec!(0.0).into(),
                    max: dec!(1.0).into(),
                    max_change: dec!(0.1).into(),
                },
                amount: Amount::new(100),
                min_self_delegation: 1.into(),
                validator_info: vec![].try_into()?,
            },
            Amount::new(100).into(),
        )?;
    }

    let ctx = Context::resolve::<Validators>().unwrap();
    let val_0 = Address::from_pubkey([0; 33]);
    let val_1 = Address::from_pubkey([1; 33]);
    staking.end_block_step(&Default::default())?;

    staking.give(Simp::mint(100))?;
    staking.end_block_step(&Default::default())?;

    assert_eq!(simp_balance(&staking.get(val_0)?.get(val_0)?.liquid), 50);
    assert_eq!(simp_balance(&staking.get(val_1)?.get(val_1)?.liquid), 50);
    staking.end_block_step(&Default::default())?;

    staking.unbond(val_0, val_0, Amount::from(100))?;
    staking.end_block_step(&Default::default())?;

    assert_eq!(ctx.updates.get(&[0; 32]).unwrap().power, 0);
    staking.give(Simp::mint(100))?;
    staking.end_block_step(&Default::default())?;

    assert_eq!(simp_balance(&staking.get(val_0)?.get(val_0)?.liquid), 50);
    assert_eq!(simp_balance(&staking.get(val_1)?.get(val_1)?.liquid), 150);

    Ok(())
}

#[cfg(feature = "abci")]
#[test]
#[serial]
#[should_panic(expected = "Cannot redelegate from validator with inbound redelegations")]
fn redelegate_from_to_failure() {
    let mut staking = setup_state().unwrap();

    for i in 0..2 {
        staking
            .declare(
                Address::from_pubkey([i; 33]),
                Declaration {
                    consensus_key: [i; 32],
                    commission: Commission {
                        rate: dec!(0.0).into(),
                        max: dec!(1.0).into(),
                        max_change: dec!(0.1).into(),
                    },
                    amount: Amount::new(100),
                    min_self_delegation: 1.into(),
                    validator_info: vec![].try_into().unwrap(),
                },
                Amount::new(100).into(),
            )
            .unwrap();
    }

    let val_0 = Address::from_pubkey([0; 33]);
    let val_1 = Address::from_pubkey([1; 33]);
    let staker = Address::from_pubkey([2; 33]);

    staking.delegate(val_0, staker, 100.into()).unwrap();
    staking.delegate(val_1, staker, 100.into()).unwrap();

    staking.end_block_step(&Default::default()).unwrap();

    staking
        .redelegate(val_0, val_1, staker, Amount::from(100))
        .unwrap();

    staking.end_block_step(&Default::default()).unwrap();

    staking
        .redelegate(val_1, val_0, staker, Amount::from(100))
        .unwrap();
}

#[cfg(feature = "abci")]
#[test]
#[serial]
#[should_panic(expected = "Cannot redelegate from validator with inbound redelegations")]
fn redelegate_from_to_two_stakers() {
    let mut staking = setup_state().unwrap();

    for i in 0..2 {
        staking
            .declare(
                Address::from_pubkey([i; 33]),
                Declaration {
                    consensus_key: [i; 32],
                    commission: Commission {
                        rate: dec!(0.0).into(),
                        max: dec!(1.0).into(),
                        max_change: dec!(0.1).into(),
                    },
                    amount: Amount::new(100),
                    min_self_delegation: 1.into(),
                    validator_info: vec![].try_into().unwrap(),
                },
                Amount::new(100).into(),
            )
            .unwrap();
    }

    let val_0 = Address::from_pubkey([0; 33]);
    let val_1 = Address::from_pubkey([1; 33]);
    let staker_0 = Address::from_pubkey([2; 33]);
    let staker_1 = Address::from_pubkey([2; 33]);

    staking.delegate(val_0, staker_0, 100.into()).unwrap();
    staking.delegate(val_1, staker_1, 100.into()).unwrap();

    staking.end_block_step(&Default::default()).unwrap();

    staking
        .redelegate(val_0, val_1, staker_0, Amount::from(100))
        .unwrap();

    staking.end_block_step(&Default::default()).unwrap();

    staking
        .redelegate(val_1, val_0, staker_1, Amount::from(100))
        .unwrap();
}

#[orga]
#[derive(Clone, Debug)]
struct Alt;
impl Symbol for Alt {
    const INDEX: u8 = 1;
    const NAME: &'static str = "ALT";
}

#[cfg(feature = "abci")]
#[test]
#[serial]
fn alt_coin_rewards() -> Result<()> {
    let mut staking = setup_state().unwrap();

    for i in 0..2 {
        staking
            .declare(
                Address::from_pubkey([i; 33]),
                Declaration {
                    consensus_key: [i; 32],
                    commission: Commission {
                        rate: dec!(0.0).into(),
                        max: dec!(1.0).into(),
                        max_change: dec!(0.1).into(),
                    },
                    amount: Amount::new(100),
                    min_self_delegation: 1.into(),
                    validator_info: vec![].try_into()?,
                },
                Amount::new(100).into(),
            )
            .unwrap();
    }

    let val_0 = Address::from_pubkey([0; 33]);
    let val_1 = Address::from_pubkey([1; 33]);
    let staker_0 = Address::from_pubkey([2; 33]);

    staking.delegate(val_0, staker_0, 100.into()).unwrap();
    staking.delegate(val_1, staker_0, 100.into()).unwrap();

    staking.end_block_step(&Default::default()).unwrap();

    staking.give(Alt::mint(100)).unwrap();
    staking.end_block_step(&Default::default()).unwrap();
    let balance = alt_balance(&staking.get(val_0)?.get(val_0)?.liquid);
    assert_eq!(balance, 25);

    let balance = alt_balance(&staking.get(val_0)?.get(staker_0)?.liquid);
    assert_eq!(balance, 25);

    let balance = alt_balance(&staking.get(val_1)?.get(val_1)?.liquid);
    assert_eq!(balance, 25);

    let balance = alt_balance(&staking.get(val_1)?.get(staker_0)?.liquid);
    assert_eq!(balance, 25);

    staking.end_block_step(&Default::default()).unwrap();

    Ok(())
}
