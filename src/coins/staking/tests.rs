
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
