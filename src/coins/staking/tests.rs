use super::*;
#[cfg(feature = "abci")]
use crate::plugins::Time;
use crate::{
    context::Context,
    store::{MapStore, Shared, Store},
};
use rust_decimal_macros::dec;
use serial_test::serial;

#[derive(State, Debug, Clone)]
struct Simp(());
impl Symbol for Simp {}

fn setup_state() -> Result<Staking<Simp>> {
    let store = Store::new(Shared::new(MapStore::new()).into());
    let mut staking: Staking<Simp> = Staking::create(store, Default::default())?;
    staking.downtime_jail_seconds = 5;
    staking.slash_fraction_downtime = (Amount::new(1) / Amount::new(2))?;
    staking.slash_fraction_double_sign = (Amount::new(1) / Amount::new(2))?;

    Context::add(Validators::default());
    Context::add(Time::from_seconds(0));

    Ok(staking)
}

#[cfg(feature = "abci")]
#[test]
#[serial]
fn staking() -> Result<()> {
    let store = Store::new(Shared::new(MapStore::new()).into());
    let mut staking: Staking<Simp> = Staking::create(store, Default::default())?;
    staking.downtime_jail_seconds = 5;
    staking.slash_fraction_downtime = (Amount::new(1) / Amount::new(2))?;

    let alice = Address::from_pubkey([0; 33]);
    let alice_con = [4; 32];
    let bob = Address::from_pubkey([1; 33]);
    let bob_con = [5; 32];
    let carol = Address::from_pubkey([2; 33]);
    let dave = Address::from_pubkey([3; 33]);
    let dave_con = [6; 32];

    Context::add(Validators::default());
    Context::add(Time::from_seconds(0));

    staking
        .give(100.into())
        .expect_err("Cannot give to empty validator set");
    assert_eq!(staking.staked()?, 0);
    staking
        .delegate(alice, alice, Coin::mint(100))
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
            min_self_delegation: 0.into(),
            validator_info: vec![].into(),
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
                min_self_delegation: 0.into(),
                validator_info: vec![].into(),
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
                min_self_delegation: 0.into(),
                validator_info: vec![].into(),
            },
            50.into(),
        )
        .expect_err("Should not be able to declare using an existing consensus key");

    staking.end_block_step()?;
    assert_eq!(staking.staked()?, 50);
    staking.delegate(alice, alice, Coin::mint(50))?;
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
            min_self_delegation: 0.into(),
            validator_info: vec![].into(),
        },
        50.into(),
    )?;
    staking.end_block_step()?;
    assert_eq!(staking.staked()?, 150);

    staking.delegate(bob, bob, Coin::mint(250))?;
    staking.delegate(bob, carol, Coin::mint(100))?;
    staking.delegate(bob, carol, Coin::mint(200))?;
    staking.delegate(bob, dave, Coin::mint(400))?;
    assert_eq!(staking.staked()?, 1100);

    let ctx = Context::resolve::<Validators>().unwrap();
    staking.end_block_step()?;
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
    staking.give(Coin::mint(600))?;
    staking.give(Coin::mint(500))?;
    assert_eq!(staking.staked()?, 1100);

    let alice_liquid = staking.get(alice)?.get(alice)?.liquid.amount()?;
    assert_eq!(alice_liquid, 100);

    let carol_to_bob_delegation = staking.get(bob)?.get(carol)?.staked.amount()?;
    assert_eq!(carol_to_bob_delegation, 300);
    let carol_to_bob_liquid = staking.get(bob)?.get(carol)?.liquid.amount()?;
    assert_eq!(carol_to_bob_liquid, 300);

    let bob_val_balance = staking.get_mut(bob)?.staked()?;
    assert_eq!(bob_val_balance, 1000);

    let bob_vp = ctx.updates.get(&bob_con).unwrap().power;
    assert_eq!(bob_vp, 1000);

    // Bob gets slashed 50%
    staking.punish_downtime(bob)?;

    staking.end_block_step()?;
    // Bob has been jailed and should no longer have any voting power
    let bob_vp = ctx.updates.get(&bob_con).unwrap().power;
    assert_eq!(bob_vp, 0);

    staking
        .withdraw(bob, dave, 401)
        .expect_err("Dave has not unbonded coins yet");
    // Bob's staked coins should no longer be present in the global staking
    // balance
    assert_eq!(staking.staked()?, 100);

    // Carol can still withdraw her 300 coins from Bob's jailed validator
    {
        staking.unbond(bob, carol, 150)?;
        assert_eq!(staking.staked()?, 100);
        staking
            .withdraw(bob, carol, 450)
            .expect_err("Should not be able to take coins before unbonding period has elapsed");
        assert_eq!(staking.staked()?, 100);
        Context::add(Time::from_seconds(10));
        let carol_recovered_coins = staking.withdraw(bob, carol, 450)?;

        assert_eq!(carol_recovered_coins.amount, 450);
    }

    {
        // Bob withdraws a third of his self-delegation
        staking.unbond(bob, bob, 100)?;
        Context::add(Time::from_seconds(20));
        let bob_recovered_coins = staking.withdraw(bob, bob, 100)?;
        assert_eq!(bob_recovered_coins.amount, 100);
        staking
            .unbond(bob, bob, 201)
            .expect_err("Should not be able to unbond more than we have staked");

        staking.unbond(bob, bob, 50)?;
        Context::add(Time::from_seconds(30));
        staking
            .withdraw(bob, bob, 501)
            .expect_err("Should not be able to take more than we have unbonded");
        staking.withdraw(bob, bob, 350)?.burn();
    }

    assert_eq!(staking.staked()?, 100);
    let alice_liquid = staking.get(alice)?.get(alice)?.liquid.amount()?;
    assert_eq!(alice_liquid, 100);
    let alice_staked = staking.get(alice)?.get(alice)?.staked.amount()?;
    assert_eq!(alice_staked, 100);

    // More block reward, but bob's delegators are jailed and should not
    // earn from it
    staking.give(Coin::mint(200))?;
    assert_eq!(staking.staked()?, 100);
    let alice_val_balance = staking.get_mut(alice)?.staked()?;
    assert_eq!(alice_val_balance, 100);
    let alice_liquid = staking.get(alice)?.get(alice)?.liquid.amount()?;
    assert_eq!(alice_liquid, 300);

    staking
        .unbond(bob, dave, 401)
        .expect_err("Dave should only have 400 unbondable coins");

    staking.unbond(bob, dave, 200)?;
    // Bob slashed another 50% while Dave unbonds
    staking.punish_downtime(bob)?;

    Context::add(Time::from_seconds(40));
    staking.withdraw(bob, dave, 500)?.burn();

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
            min_self_delegation: 0.into(),
            validator_info: vec![].into(),
        },
        300.into(),
    )?;
    staking.end_block_step()?;
    assert_eq!(staking.staked()?, 400);
    staking.end_block_step()?;
    assert_eq!(ctx.updates.get(&alice_con).unwrap().power, 100);
    assert_eq!(ctx.updates.get(&dave_con).unwrap().power, 300);
    staking.delegate(dave, carol, 300.into())?;
    assert_eq!(staking.staked()?, 700);

    staking.end_block_step()?;
    assert_eq!(ctx.updates.get(&dave_con).unwrap().power, 600);
    staking.unbond(dave, dave, 150)?;
    assert_eq!(staking.staked()?, 550);
    staking.end_block_step()?;
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
            min_self_delegation: 0.into(),
            validator_info: vec![].into(),
        },
        550.into(),
    )?;

    staking.delegate(edith, carol, 550.into())?;

    staking.get_mut(edith)?.give(500.into())?;

    let edith_liquid = staking.get(edith)?.get(edith)?.liquid.amount()?;
    assert_eq!(edith_liquid, 375);
    let carol_liquid = staking.get(edith)?.get(carol)?.liquid.amount()?;
    assert_eq!(carol_liquid, 125);

    staking.punish_double_sign(dave)?;
    staking.end_block_step()?;
    assert_eq!(ctx.updates.get(&dave_con).unwrap().power, 0);

    Ok(())
}

#[cfg(feature = "abci")]
#[test]
#[serial]
fn val_size_limit() -> Result<()> {
    let store = Store::new(Shared::new(MapStore::new()).into());
    let mut staking: Staking<Simp> = Staking::create(store, Default::default())?;

    Context::add(Validators::default());
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
                min_self_delegation: 0.into(),
                validator_info: vec![].into(),
            },
            Amount::new(i as u64 * 100).into(),
        )?;
    }
    staking.end_block_step()?;
    assert_eq!(staking.staked()?, 1700);
    assert!(ctx.updates.get(&[7; 32]).is_none());
    assert_eq!(ctx.updates.get(&[8; 32]).unwrap().power, 800);
    assert_eq!(ctx.updates.get(&[9; 32]).unwrap().power, 900);
    staking.give(3400.into())?;
    assert_eq!(
        staking
            .get(Address::from_pubkey([4; 33]))?
            .get(Address::from_pubkey([4; 33]))?
            .liquid
            .amount()?,
        0
    );
    assert_eq!(
        staking
            .get(Address::from_pubkey([8; 33]))?
            .get(Address::from_pubkey([8; 33]))?
            .liquid
            .amount()?,
        1600
    );
    assert_eq!(
        staking
            .get(Address::from_pubkey([9; 33]))?
            .get(Address::from_pubkey([9; 33]))?
            .liquid
            .amount()?,
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
            min_self_delegation: 0.into(),
            validator_info: vec![].into(),
        },
        1000.into(),
    )?;

    staking.end_block_step()?;

    assert_eq!(ctx.updates.get(&[8; 32]).unwrap().power, 0);
    assert_eq!(ctx.updates.get(&[9; 32]).unwrap().power, 900);
    assert_eq!(ctx.updates.get(&[10; 32]).unwrap().power, 1000);
    staking.give(1900.into())?;

    assert_eq!(
        staking
            .get(Address::from_pubkey([8; 33]))?
            .get(Address::from_pubkey([8; 33]))?
            .liquid
            .amount()?,
        1600
    );
    assert_eq!(
        staking
            .get(Address::from_pubkey([9; 33]))?
            .get(Address::from_pubkey([9; 33]))?
            .liquid
            .amount()?,
        2700
    );
    assert_eq!(
        staking
            .get(Address::from_pubkey([10; 33]))?
            .get(Address::from_pubkey([10; 33]))?
            .liquid
            .amount()?,
        1000
    );

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
            min_self_delegation: 0.into(),
            validator_info: vec![].into(),
        },
        Amount::new(100).into(),
    )?;

    let ctx = Context::resolve::<Validators>().unwrap();
    let staker = Address::from_pubkey([1; 33]);

    staking.delegate(val_0, staker, 100.into())?;

    staking.end_block_step()?;

    assert_eq!(ctx.updates.get(&[0; 32]).unwrap().power, 200);

    staking.unbond(Address::from_pubkey([0; 33]), staker, Amount::from(100))?;

    staking.end_block_step()?;
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
            min_self_delegation: 0.into(),
            validator_info: vec![].into(),
        },
        Amount::new(100).into(),
    )?;

    let ctx = Context::resolve::<Validators>().unwrap();
    let staker = Address::from_pubkey([1; 33]);

    staking.delegate(val_0, staker, 100.into())?;

    staking.end_block_step()?;

    assert_eq!(ctx.updates.get(&[0; 32]).unwrap().power, 200);

    staking.unbond(Address::from_pubkey([0; 33]), staker, Amount::from(100))?;

    staking.end_block_step()?;

    staking.punish_double_sign(Address::from_pubkey([0; 33]))?;
    staking.end_block_step()?;

    assert_eq!(ctx.updates.get(&[0; 32]).unwrap().power, 0);
    assert_eq!(staking.get(val_0)?.get(staker)?.staked.amount()?, 0);

    assert_eq!(staking.get_mut(val_0)?.delegators.balance()?.amount()?, 50);

    Context::add(Time::from_seconds(10));
    staking.end_block_step()?;
    assert_eq!(staking.get(val_0)?.get(staker)?.liquid.amount()?, 50);

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
            min_self_delegation: 0.into(),
            validator_info: vec![].into(),
        },
        Amount::new(100).into(),
    )?;

    let ctx = Context::resolve::<Validators>().unwrap();
    let staker = Address::from_pubkey([1; 33]);

    staking.delegate(val_0, staker, 100.into())?;

    staking.end_block_step()?;

    assert_eq!(ctx.updates.get(&[0; 32]).unwrap().power, 200);

    staking.unbond(Address::from_pubkey([0; 33]), staker, Amount::from(100))?;

    Context::add(Time::from_seconds(10));
    staking.end_block_step()?;

    staking.punish_double_sign(Address::from_pubkey([0; 33]))?;
    staking.end_block_step()?;

    assert_eq!(ctx.updates.get(&[0; 32]).unwrap().power, 0);
    assert_eq!(staking.get(val_0)?.get(staker)?.staked.amount()?, 0);

    assert_eq!(staking.get_mut(val_0)?.delegators.balance()?.amount()?, 50);

    staking.end_block_step()?;
    assert_eq!(staking.get(val_0)?.get(staker)?.liquid.amount()?, 100);

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
                min_self_delegation: 0.into(),
                validator_info: vec![].into(),
            },
            Amount::new(100).into(),
        )?;
    }

    let ctx = Context::resolve::<Validators>().unwrap();
    let staker = Address::from_pubkey([2; 33]);

    staking.delegate(Address::from_pubkey([0; 33]), staker, 100.into())?;

    staking.end_block_step()?;

    assert_eq!(ctx.updates.get(&[0; 32]).unwrap().power, 200);

    staking.redelegate(
        Address::from_pubkey([0; 33]),
        Address::from_pubkey([1; 33]),
        staker,
        Amount::from(100),
    )?;

    staking.end_block_step()?;
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
                min_self_delegation: 0.into(),
                validator_info: vec![].into(),
            },
            Amount::new(100).into(),
        )?;
    }

    let ctx = Context::resolve::<Validators>().unwrap();
    let staker = Address::from_pubkey([2; 33]);

    staking.delegate(Address::from_pubkey([0; 33]), staker, 100.into())?;

    staking.end_block_step()?;

    assert_eq!(ctx.updates.get(&[0; 32]).unwrap().power, 200);

    staking.redelegate(
        Address::from_pubkey([0; 33]),
        Address::from_pubkey([1; 33]),
        staker,
        Amount::from(100),
    )?;

    staking.end_block_step()?;
    assert_eq!(ctx.updates.get(&[0; 32]).unwrap().power, 100);
    assert_eq!(ctx.updates.get(&[1; 32]).unwrap().power, 200);

    staking.punish_double_sign(Address::from_pubkey([0; 33]))?;
    staking.end_block_step()?;

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
                min_self_delegation: 0.into(),
                validator_info: vec![].into(),
            },
            Amount::new(100).into(),
        )?;
    }

    let ctx = Context::resolve::<Validators>().unwrap();
    let staker = Address::from_pubkey([2; 33]);

    staking.delegate(Address::from_pubkey([0; 33]), staker, 100.into())?;

    staking.end_block_step()?;

    assert_eq!(ctx.updates.get(&[0; 32]).unwrap().power, 200);

    staking.redelegate(
        Address::from_pubkey([0; 33]),
        Address::from_pubkey([1; 33]),
        staker,
        Amount::from(100),
    )?;

    staking.end_block_step()?;
    assert_eq!(ctx.updates.get(&[0; 32]).unwrap().power, 100);
    assert_eq!(ctx.updates.get(&[1; 32]).unwrap().power, 200);

    Context::add(Time::from_seconds(10));
    staking.end_block_step()?;

    staking.punish_double_sign(Address::from_pubkey([0; 33]))?;
    staking.end_block_step()?;

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
                amount: Amount::new(0),
                min_self_delegation: 0.into(),
                validator_info: vec![].into(),
            },
            Amount::new(0).into(),
        )?;
    }

    let ctx = Context::resolve::<Validators>().unwrap();
    let staker = Address::from_pubkey([3; 33]);

    staking.delegate(Address::from_pubkey([0; 33]), staker, 100.into())?;
    staking.delegate(Address::from_pubkey([1; 33]), staker, 100.into())?;
    staking.delegate(Address::from_pubkey([2; 33]), staker, 100.into())?;

    staking.end_block_step()?;

    staking.redelegate(
        Address::from_pubkey([0; 33]),
        Address::from_pubkey([2; 33]),
        staker,
        Amount::from(50),
    )?;
    staking.end_block_step()?;

    assert_eq!(ctx.updates.get(&[0; 32]).unwrap().power, 50);
    assert_eq!(ctx.updates.get(&[1; 32]).unwrap().power, 100);
    assert_eq!(ctx.updates.get(&[2; 32]).unwrap().power, 150);

    staking.redelegate(
        Address::from_pubkey([1; 33]),
        Address::from_pubkey([0; 33]),
        staker,
        Amount::from(30),
    )?;
    staking.end_block_step()?;

    assert_eq!(ctx.updates.get(&[0; 32]).unwrap().power, 80);
    assert_eq!(ctx.updates.get(&[1; 32]).unwrap().power, 70);
    assert_eq!(ctx.updates.get(&[2; 32]).unwrap().power, 150);

    staking.redelegate(
        Address::from_pubkey([1; 33]),
        Address::from_pubkey([2; 33]),
        staker,
        Amount::from(30),
    )?;
    staking.end_block_step()?;

    assert_eq!(ctx.updates.get(&[0; 32]).unwrap().power, 80);
    assert_eq!(ctx.updates.get(&[1; 32]).unwrap().power, 40);
    assert_eq!(ctx.updates.get(&[2; 32]).unwrap().power, 180);

    staking.punish_double_sign(Address::from_pubkey([1; 33]))?;
    staking.end_block_step()?;

    let val_0 = Address::from_pubkey([0; 33]);
    let val_1 = Address::from_pubkey([1; 33]);
    let val_2 = Address::from_pubkey([2; 33]);

    assert_eq!(staking.get(val_0)?.get(staker)?.staked.amount()?, 65);
    assert_eq!(staking.get(val_1)?.get(staker)?.staked.amount()?, 20);
    assert_eq!(staking.get(val_2)?.get(staker)?.staked.amount()?, 165);

    assert_eq!(staking.get_mut(val_0)?.delegators.balance()?.amount()?, 65);
    assert_eq!(staking.get_mut(val_1)?.delegators.balance()?.amount()?, 20);
    assert_eq!(staking.get_mut(val_2)?.delegators.balance()?.amount()?, 165);

    staking.punish_double_sign(Address::from_pubkey([0; 33]))?;
    staking.end_block_step()?;
    assert_eq!(staking.get(val_0)?.get(staker)?.staked.amount()?, 32);
    assert_eq!(staking.get(val_1)?.get(staker)?.staked.amount()?, 20);
    assert_eq!(staking.get(val_2)?.get(staker)?.staked.amount()?, 140);

    assert_eq!(staking.get_mut(val_0)?.delegators.balance()?.amount()?, 32);
    assert_eq!(staking.get_mut(val_1)?.delegators.balance()?.amount()?, 20);
    assert_eq!(staking.get_mut(val_2)?.delegators.balance()?.amount()?, 140);

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
                amount: Amount::new(0),
                min_self_delegation: 0.into(),
                validator_info: vec![].into(),
            },
            Amount::new(0).into(),
        )?;
    }

    let staker = Address::from_pubkey([2; 33]);
    let val_0 = Address::from_pubkey([0; 33]);
    let val_1 = Address::from_pubkey([1; 33]);

    staking.delegate(Address::from_pubkey([0; 33]), staker, 100.into())?;

    staking.end_block_step()?;

    staking.redelegate(
        Address::from_pubkey([0; 33]),
        Address::from_pubkey([1; 33]),
        staker,
        Amount::from(100),
    )?;
    staking.end_block_step()?;

    staking.punish_double_sign(Address::from_pubkey([0; 33]))?;
    staking.punish_double_sign(Address::from_pubkey([1; 33]))?;

    staking.end_block_step()?;

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
                amount: Amount::new(0),
                min_self_delegation: 0.into(),
                validator_info: vec![].into(),
            },
            Amount::new(0).into(),
        )?;
    }

    let ctx = Context::resolve::<Validators>().unwrap();
    let staker = Address::from_pubkey([3; 33]);

    staking.delegate(Address::from_pubkey([0; 33]), staker, 100.into())?;
    staking.delegate(Address::from_pubkey([1; 33]), staker, 100.into())?;
    staking.delegate(Address::from_pubkey([2; 33]), staker, 100.into())?;

    staking.end_block_step()?;

    staking.redelegate(
        Address::from_pubkey([0; 33]),
        Address::from_pubkey([2; 33]),
        staker,
        Amount::from(50),
    )?;
    staking.end_block_step()?;

    assert_eq!(ctx.updates.get(&[0; 32]).unwrap().power, 50);
    assert_eq!(ctx.updates.get(&[1; 32]).unwrap().power, 100);
    assert_eq!(ctx.updates.get(&[2; 32]).unwrap().power, 150);

    staking.redelegate(
        Address::from_pubkey([1; 33]),
        Address::from_pubkey([0; 33]),
        staker,
        Amount::from(30),
    )?;
    staking.end_block_step()?;

    assert_eq!(ctx.updates.get(&[0; 32]).unwrap().power, 80);
    assert_eq!(ctx.updates.get(&[1; 32]).unwrap().power, 70);
    assert_eq!(ctx.updates.get(&[2; 32]).unwrap().power, 150);

    staking.redelegate(
        Address::from_pubkey([1; 33]),
        Address::from_pubkey([2; 33]),
        staker,
        Amount::from(30),
    )?;
    staking.end_block_step()?;

    assert_eq!(ctx.updates.get(&[0; 32]).unwrap().power, 80);
    assert_eq!(ctx.updates.get(&[1; 32]).unwrap().power, 40);
    assert_eq!(ctx.updates.get(&[2; 32]).unwrap().power, 180);

    staking.punish_double_sign(Address::from_pubkey([1; 33]))?;
    staking.end_block_step()?;

    let val_0 = Address::from_pubkey([0; 33]);
    let val_1 = Address::from_pubkey([1; 33]);
    let val_2 = Address::from_pubkey([2; 33]);

    assert_eq!(staking.get(val_0)?.get(staker)?.staked.amount()?, 65);
    assert_eq!(staking.get(val_1)?.get(staker)?.staked.amount()?, 20);
    assert_eq!(staking.get(val_2)?.get(staker)?.staked.amount()?, 165);

    assert_eq!(staking.get_mut(val_0)?.delegators.balance()?.amount()?, 65);
    assert_eq!(staking.get_mut(val_1)?.delegators.balance()?.amount()?, 20);
    assert_eq!(staking.get_mut(val_2)?.delegators.balance()?.amount()?, 165);

    staking.unbond(val_2, staker, Amount::from(100))?;
    staking.end_block_step()?;

    staking.punish_double_sign(Address::from_pubkey([0; 33]))?;
    staking.end_block_step()?;

    assert_eq!(staking.get(val_0)?.get(staker)?.staked.amount()?, 32);
    assert_eq!(staking.get(val_1)?.get(staker)?.staked.amount()?, 20);
    assert_eq!(staking.get(val_2)?.get(staker)?.staked.amount()?, 40);

    assert_eq!(staking.get_mut(val_0)?.delegators.balance()?.amount()?, 32);
    assert_eq!(staking.get_mut(val_1)?.delegators.balance()?.amount()?, 20);
    assert_eq!(staking.get_mut(val_2)?.delegators.balance()?.amount()?, 40);

    Context::add(Time::from_seconds(10));
    staking.end_block_step()?;

    assert_eq!(staking.get(val_2)?.get(staker)?.liquid.amount()?, 100);

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
                amount: Amount::new(0),
                min_self_delegation: 0.into(),
                validator_info: vec![].into(),
            },
            Amount::new(0).into(),
        )?;
    }

    let ctx = Context::resolve::<Validators>().unwrap();
    let staker = Address::from_pubkey([3; 33]);

    staking.delegate(Address::from_pubkey([0; 33]), staker, 100.into())?;
    staking.delegate(Address::from_pubkey([1; 33]), staker, 100.into())?;
    staking.delegate(Address::from_pubkey([2; 33]), staker, 100.into())?;

    staking.end_block_step()?;

    staking.redelegate(
        Address::from_pubkey([0; 33]),
        Address::from_pubkey([2; 33]),
        staker,
        Amount::from(50),
    )?;
    staking.end_block_step()?;

    assert_eq!(ctx.updates.get(&[0; 32]).unwrap().power, 50);
    assert_eq!(ctx.updates.get(&[1; 32]).unwrap().power, 100);
    assert_eq!(ctx.updates.get(&[2; 32]).unwrap().power, 150);

    staking.redelegate(
        Address::from_pubkey([1; 33]),
        Address::from_pubkey([0; 33]),
        staker,
        Amount::from(30),
    )?;
    staking.end_block_step()?;

    assert_eq!(ctx.updates.get(&[0; 32]).unwrap().power, 80);
    assert_eq!(ctx.updates.get(&[1; 32]).unwrap().power, 70);
    assert_eq!(ctx.updates.get(&[2; 32]).unwrap().power, 150);

    staking.redelegate(
        Address::from_pubkey([1; 33]),
        Address::from_pubkey([2; 33]),
        staker,
        Amount::from(30),
    )?;
    staking.end_block_step()?;

    assert_eq!(ctx.updates.get(&[0; 32]).unwrap().power, 80);
    assert_eq!(ctx.updates.get(&[1; 32]).unwrap().power, 40);
    assert_eq!(ctx.updates.get(&[2; 32]).unwrap().power, 180);

    staking.punish_double_sign(Address::from_pubkey([1; 33]))?;
    staking.end_block_step()?;

    let val_0 = Address::from_pubkey([0; 33]);
    let val_1 = Address::from_pubkey([1; 33]);
    let val_2 = Address::from_pubkey([2; 33]);

    assert_eq!(staking.get(val_0)?.get(staker)?.staked.amount()?, 65);
    assert_eq!(staking.get(val_1)?.get(staker)?.staked.amount()?, 20);
    assert_eq!(staking.get(val_2)?.get(staker)?.staked.amount()?, 165);

    assert_eq!(staking.get_mut(val_0)?.delegators.balance()?.amount()?, 65);
    assert_eq!(staking.get_mut(val_1)?.delegators.balance()?.amount()?, 20);
    assert_eq!(staking.get_mut(val_2)?.delegators.balance()?.amount()?, 165);

    for _ in 0..15 {
        staking.unbond(val_2, staker, Amount::from(10))?;
    }

    staking.end_block_step()?;

    assert_eq!(staking.get(val_2)?.get(staker)?.staked.amount()?, 15);

    staking.punish_double_sign(Address::from_pubkey([0; 33]))?;
    staking.end_block_step()?;

    assert_eq!(staking.get(val_0)?.get(staker)?.staked.amount()?, 32);
    assert_eq!(staking.get(val_1)?.get(staker)?.staked.amount()?, 20);
    assert_eq!(staking.get(val_2)?.get(staker)?.staked.amount()?, 0);

    assert_eq!(staking.get_mut(val_0)?.delegators.balance()?.amount()?, 32);
    assert_eq!(staking.get_mut(val_1)?.delegators.balance()?.amount()?, 20);
    assert_eq!(staking.get_mut(val_2)?.delegators.balance()?.amount()?, 0);

    Context::add(Time::from_seconds(10));
    staking.end_block_step()?;

    assert_eq!(staking.get(val_2)?.get(staker)?.liquid.amount()?, 140);

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
                min_self_delegation: 0.into(),
                validator_info: vec![].into(),
            },
            Amount::new(100).into(),
        )
        .unwrap();

    let staker = Address::from_pubkey([3; 33]);

    staking.end_block_step().unwrap();

    staking
        .punish_double_sign(Address::from_pubkey([0; 33]))
        .unwrap();
    staking.end_block_step().unwrap();

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
            validator_info: vec![].into(),
        },
        Amount::new(100).into(),
    )?;

    let ctx = Context::resolve::<Validators>().unwrap();
    let val_0 = Address::from_pubkey([0; 33]);

    staking.end_block_step()?;

    staking.punish_downtime(Address::from_pubkey([0; 33]))?;
    assert_eq!(staking.get_mut(val_0)?.delegators.balance()?.amount()?, 50);
    Context::add(Time::from_seconds(10));

    staking.end_block_step()?;

    staking.get_mut(val_0)?.try_unjail()?;
    staking.update_vp(val_0)?;

    staking.end_block_step()?;

    assert_eq!(ctx.updates.get(&[0; 32]).unwrap().power, 0);
    staking.delegate(val_0, val_0, 25.into())?;

    staking.end_block_step()?;

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
                validator_info: vec![].into(),
            },
            Amount::new(100).into(),
        )?;
    }

    let ctx = Context::resolve::<Validators>().unwrap();
    let val_0 = Address::from_pubkey([0; 33]);
    let val_1 = Address::from_pubkey([1; 33]);

    staking.end_block_step()?;
    staking.unbond(val_0, val_0, Amount::from(50))?;
    staking.end_block_step()?;

    assert_eq!(staking.get_mut(val_0)?.delegators.balance()?.amount()?, 50);

    staking.end_block_step()?;

    assert_eq!(ctx.updates.get(&[0; 32]).unwrap().power, 0);
    staking.delegate(val_0, val_0, 25.into())?;

    staking.end_block_step()?;

    assert_eq!(staking.get_mut(val_0)?.delegators.balance()?.amount()?, 75);
    assert_eq!(ctx.updates.get(&[0; 32]).unwrap().power, 75);

    staking.redelegate(val_0, val_1, val_0, Amount::from(25))?;

    staking.end_block_step()?;

    assert_eq!(staking.get_mut(val_0)?.delegators.balance()?.amount()?, 50);

    staking.end_block_step()?;

    assert_eq!(ctx.updates.get(&[0; 32]).unwrap().power, 0);
    staking.delegate(val_0, val_0, 25.into())?;

    staking.end_block_step()?;

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
            validator_info: vec![].into(),
        },
        Amount::new(100).into(),
    )?;

    let val_0 = Address::from_pubkey([0; 33]);
    staking.end_block_step()?;

    staking.punish_downtime(Address::from_pubkey([0; 33]))?;
    assert_eq!(staking.get_mut(val_0)?.delegators.balance()?.amount()?, 50);
    staking.end_block_step()?;

    staking.punish_double_sign(val_0)?;
    staking.end_block_step()?;

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
            min_self_delegation: 0.into(),
            validator_info: vec![].into(),
        },
        Amount::new(100).into(),
    )?;

    let val_0 = Address::from_pubkey([0; 33]);
    let staker = Address::from_pubkey([1; 33]);

    staking.end_block_step()?;

    staking.delegate(val_0, staker, 100.into())?;
    staking.give(100.into())?;

    staking.end_block_step()?;

    assert_eq!(staking.get(val_0)?.get(staker)?.liquid.amount()?, 50);
    staking.end_block_step()?;

    staking.punish_downtime(Address::from_pubkey([0; 33]))?;

    staking.end_block_step()?;
    assert_eq!(staking.get(val_0)?.get(staker)?.liquid.amount()?, 50);

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
                min_self_delegation: 0.into(),
                validator_info: vec![].into(),
            },
            Amount::new(100).into(),
        )?;
    }

    let ctx = Context::resolve::<Validators>().unwrap();
    let val_0 = Address::from_pubkey([0; 33]);
    let val_1 = Address::from_pubkey([1; 33]);
    staking.end_block_step()?;

    staking.give(100.into())?;
    staking.end_block_step()?;

    assert_eq!(staking.get(val_0)?.get(val_0)?.liquid.amount()?, 50);
    assert_eq!(staking.get(val_1)?.get(val_1)?.liquid.amount()?, 50);
    staking.end_block_step()?;

    staking.unbond(val_0, val_0, Amount::from(100))?;
    staking.end_block_step()?;

    assert_eq!(ctx.updates.get(&[0; 32]).unwrap().power, 0);
    staking.give(100.into())?;
    staking.end_block_step()?;

    assert_eq!(staking.get(val_0)?.get(val_0)?.liquid.amount()?, 50);
    assert_eq!(staking.get(val_1)?.get(val_1)?.liquid.amount()?, 150);

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
                    min_self_delegation: 0.into(),
                    validator_info: vec![].into(),
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

    staking.end_block_step().unwrap();

    staking
        .redelegate(val_0, val_1, staker, Amount::from(100))
        .unwrap();

    staking.end_block_step().unwrap();

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
                    min_self_delegation: 0.into(),
                    validator_info: vec![].into(),
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

    staking.end_block_step().unwrap();

    staking
        .redelegate(val_0, val_1, staker_0, Amount::from(100))
        .unwrap();

    staking.end_block_step().unwrap();

    staking
        .redelegate(val_1, val_0, staker_1, Amount::from(100))
        .unwrap();
}
