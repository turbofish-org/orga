# Deterministic Concurrent State Machines

**Matt Bell ([@mappum](https://twitter.com/mappum))** • [Nomic Hodlings, Inc.](https://nomic.io)

v0.0.0 - *November 5, 2019*

As data moves through a distributed computer network, each node sees the data in a different order. Blockchains are a tool to solve this problem by letting network nodes agree on a canonical ordering - solving the "double spend problem" in payments and guaranteeing all nodes have the same state.

Modern blockchain engineers typically stop here and build systems to simply process the data serially, one transaction after the next, in this canonical order. This simple model is easy to reason about, but severely limits performance since CPU utilization is bound to a single core due to the lack of concurrency.

There are many opportunities to scale blockchain processing to use all available compute resources by concurrently breaking up the work in a way where each node still achieves the same deterministic results.

This document explores a simple model for a framework that processes data concurrently but deterministically. Our concurrent processing is done without the need for complicated concurrency primitives such as mutexes and atomics at the application level, but instead coordinates purely through execution scheduling. Many of these concepts can be generalized (for instance, to a compiler which provides concurrency automatically without cognitive load on the developer), but we specifically focus on typical blockchain applications (payments, financial primitives, and other smart contracts).

We'll be taking the model as reasoned about in this document to implement a blockchain state machine framework in Rust, integrated with Tendermint consensus. An additional goal of our implementation will be for logic to be portable to other environments, including centralized web servers and peer-to-peer payment channels.

Concretely, my short-term goal is to build a blockchain system that can sustain the processing of 50,000 typical payment transactions per second on an average server (roughly the same throughput level as the WeChat payment network).

## Concurrency

Our model creates a *state machine*, logic which takes in *transitions* as input data, and processes them in a deterministic way. This logic reads and writes from a *store*, a mutable state which stores data with a key/value interface.

The store interface will be the same as typically found in embedded databases. Concretely, these are the methods `get(key) -> value`, `put(key, value)`, and `delete(key)` where keys and values are variable-length binary data. We assume that side-effects made by transitions are applied atomically after processing the logic of the whole transition (e.g. we collect the writes in a map then flush them to the state later).

As we process data through this interface, we can reason about the state observed or affected by each transition in terms of this key/value interface. For each transition, we track the set of keys it reads from, and the set of keys it writes to. In this document, we'll use the notation `Xr` to refer to the set of keys read by transition `X` for the current state, and `Xw` to refer to the set of keys written to (`put` and `delete`). Additionally, we will perform standard set operators on these, namely intersection (`&`) and union (`|`). Note that these sets only apply to the keys in the operations - if a transition calls `put('a', 'foo')` and another calls `put('a', 'bar')` we consider this an intersection for key `'a'` even though the values differ.

All processing that happens within our state machine logic is within the handling of transitions, so the concurrency model deals with using relations between transitions' read and write sets to guarantee transitions execute deterministically.

XXXXXX: move to more concrete section - Note that for simplicity we apply our concurrency rules to the coarse level of atomic transactions and do not reason about the order that reads and writes happen within a transaction - further optimization could be made in the future if needed by breaking down transactions into sub-transactions which allow concurrency on a more granular level.

### Axioms

Our reasoning starts from the following axioms:

1. **Transitions which do not intersect in reads or writes can be processed concurrently with no coordination.** In other words, transitions `A` and `B` can be processed in parallel if the following is true: `(Ar | Aw) & (Br | Bw) = []` (`[]` means an empty set). For example, if Alice sends a payment to Bob, and Carol sends a payment to Dave, these transitions can be processed in any order and still affect the state the same way.
2. **Transitions which intersect in reads but not writes, and do not write to the intersecting reads can be processed concurrently with no coordination.** In other words, transitions `A` and `B` can be processed in parallel if the following is true: `(Ar & Br) & (Aw | Bw) = []`. For example, if Alice and Bob each send transactions which are calculated based on a current market price, but do not change the market price, these transactions can be processed in any order and still affect the state the same way.
3. **Transitions where writes of `A` intersect with with writes of `B`, but do not read from the intersecting writes can be processed concurrently, but must have their writes applied in a canonical order.** In other words, transitiions `A` and `B` can be processed in parallel if the following is true, but their side-effects to the state must be written in-order: `Aw & Bw != []` and `(Ar | Br) & (Aw & Bw) = []`. For example, if there is a global variable `last_sender` and Alice and Bob each send transactions which set this variable to their own identity but do not read from this variable, we may execute the transactions in any order, but must flush their writes to the state in-order.
4. **Transitions where writes of `A` intersect with with the reads of `B` must be processed serially in a canonical order.** In other words, transitions `A` and `B` must be processed in-order if the following is true: `Aw & Br != []`. For example, if Bob has a balance of 0, Alice sends a payment of 1 to Bob, and Bob sends a payment of 1 to Carol, the second payment will fail if processed before the first. Note that this rule is commutative so `A` and `B` can be switched in the above notation.

### Concrete Observations

The above axioms are abstract and general, but we also make some practical observations that let us apply them to our real-world applications.

1. **Many transitions will be fully disjoint and can be trivially parallelized based on axiom #1.** Since this rule applies to significant amount of typical blockchain transactions (e.g. payments), our implementation should support all of these transactions in parallel by default.
2. **Many transitions will read and write the same keys when processed during different states, so their execution results can be cached and re-applied.** For example, we will process transactions first when they enter the mempool (e.g. ABCI `CheckTx`) and again when processed in a consensus context (e.g. ABCI `DeliverTx`). Even though the state before processing will be slightly different during these two executions because they likely do not have the same transition ordering, for a significant amount of transactions we will be able to simply re-apply the side-effects as long as none of the keys read by the transition have been written to by other transitions. Additionally, even if the transition's read have been written to by other transitions, we will be able to re-use the read and write key sets for execution scheduling for the second execution.
3. **A signifcant amount of transactions in a block will already be in the mempool of nodes receiving the block, so all steps that can be computed and cached during the initial execution will reduce the amount of real time to validate a proposed block.** This essentially just lets us apply #2 to increase blockchain throughput, since nodes can do expensive computations as early as possible and reduce the amount of verification time that blocks the concensus process. An obvious concrete example is signature verfication, one of the most expensive parts of payment processing.
4. **Scaling initial transaction processing (`CheckTx`) to N parallel mempools will increase throughput, due to observations #1 and #2.** This will let us scale up our utilized resources as high as we can, even if this sometimes breaks our concurrency rules and creates diverging mempool states, because many transition results and read/write key sets will still be able to be re-used when performing the consensus execution. Benchmarks will have to be made on real-world data and realistic simulations to find a level of separate mempools that finds the right tradeoff of increased throughput vs. wasted resources.
