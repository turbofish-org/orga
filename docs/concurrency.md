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
2. **Transitions which intersect in reads but not writes, and do not write to the intersecting reads can be processed concurrently with no coordination.** In other words, transitions `A` and `B` can be processed in parallel if the following is true: `(Ar & Br) & (Aw | Bw) = []`. For example, if Alice and Bob each send transactions which are calculated based on a current market price, but do not change the market price, these transactions can be processed in any order and still affect the state the same way. This is a similar concept to Rust's `RWLock`, or its borrowing rules which allow multiple immutable borrows.
3. **Transitions where writes of `A` intersect with with writes of `B`, but do not read from the intersecting writes can be processed concurrently, but must have their writes applied in a canonical order.** In other words, transitions `A` and `B` can be processed in parallel if the following is true, but their side-effects to the state must be written in-order: `Aw & Bw != []` and `(Ar | Br) & (Aw & Bw) = []`. For example, if there is a global variable `last_sender` and Alice and Bob each send transactions which set this variable to their own identity but do not read from this variable, we may execute the transactions in any order, but must flush their writes to the state in-order.
4. **Transitions where writes of `A` intersect with with the reads of `B` must be processed serially in a canonical order.** In other words, transitions `A` and `B` must be processed in-order if the following is true: `Aw & Br != []`. For example, if Bob has a balance of 0, Alice sends a payment of 1 to Bob, and Bob sends a payment of 1 to Carol, the second payment will fail if processed before the first. Note that this rule is commutative and for any two transitions it applies both ways.

### Concrete Observations

The above axioms are abstract and general, but we also make some practical observations that let us apply them to our real-world applications.

1. **Many transitions will be fully disjoint and can be trivially parallelized based on axiom #1.** Since this rule applies to significant amount of typical blockchain transactions (e.g. payments), our implementation should support all of these transactions in parallel by default.
2. **Many transitions will read and write the same keys when processed during different states, so their execution results can be cached and re-applied.** For example, we will process transactions first when they enter the mempool (e.g. ABCI `CheckTx`) and again when processed in a consensus context (e.g. ABCI `DeliverTx`). Even though the state before processing will be slightly different during these two executions because they likely do not have the same transition ordering, for a significant amount of transactions we will be able to simply re-apply the side-effects as long as none of the keys read by the transition have been written to by other transitions. Additionally, even if the transition's reads have been written to by other transitions, we will be able to re-use the read and write key sets to schedule execution for the second execution.
3. **A signifcant amount of transactions in a block will already be in the mempool of nodes receiving the block, so all steps that can be computed and cached during the initial execution will reduce the amount of real time to validate a proposed block.** This essentially just lets us apply #2 to increase blockchain throughput, since nodes can do expensive computations as early as possible and reduce the amount of verification time that blocks the concensus process. An obvious concrete example is signature verfication, one of the most expensive parts of payment processing.
4. **Scaling initial transaction processing (`CheckTx`) to N parallel mempools will increase throughput, due to observations #1 and #2.** This will let us scale up our utilized resources as high as we can, even if this sometimes breaks our concurrency rules and creates diverging mempool states, because many transition results and read/write key sets will still be able to be re-used when performing the consensus execution. Benchmarks will have to be made on real-world data and realistic simulations to find a level of separate mempools that finds the right tradeoff of increased throughput vs. wasted resources.

## Architecture

### ABCI Pipeline

This system will be integrated with Tendermint consensus via the ABCI protocol. This serves as the core of our blockchain framework, handling efficient processing of transitions in the context of transactions in a gossip network and with a consensus-defined order to create cross-network determinism.

Each ABCI message will be handled in the pipeline as follows:

#### `Query`

To service incoming state queries, query handlers can be created spanning across N threads (e.g. in a Tokio thread pool) which resolve queries against a snapshot of the latest committed state. At a low-level this can use RocksDB's consistent snapshot feature, allowing us to resolve queries against the state at height `H` even while committing a a new state for `H + 1`, increasing query throughput.

#### `CheckTx`

N mempool workers will be maintained to handle transactions as they stream in, spanning across at least N threads. These separate mempools will diverge somewhat since they are processing different sets of transactions, but determinism is not important in this step (mempools across nodes diverge anyways).

Although we can accept some divergence, there is a tradeoff between wasted computation and worker utilization - divergence may mean we have to re-process a transaction later if its dependencies on the state have changed, whereas coordinating based on our concurrency rules means we may need to sometimes block and leave a CPU idle.

##### Worker Selection

Different strategies exist when assigning a worker for a given transaction, spanning the divergence/utilization tradeoff spectrum.

- *Simple load-balancing* - It may make sense to simply balance the load with traditional techniques, e.g. sending the transaction to the worker with the least amount of queued work. However, this results in the highest amount of divergence between the separate mempool states since there could be e.g. transactions which double-spend from the same funds each being processed in separate workers with different states.
- *Roughly correlated* - A strategy with less divergence is to use a watered-down form of our concurrency rules, e.g. probabalistically sending transactions to a different worker if there are some amount of intersecting state changes, while sometimes allowing different workers to process conflicting transactions.
- *Scheduled* - For the least amount of divergence but also the least efficient utilization, we can use a scheduling algorithm similar to processing consensus-ordered transactions to split work across threads.

##### Periodic Syncronization

A possible optimization to reduce mempool state divergence is to periodically (e.g. every 250ms) synchronize mempools. This may be as simple as discarding `N - 1` mempool states and replacing them with the remaining mempool state, so that further transactions share a common state (which will once again diverge). If it can be done cheaply and without violating concurrency rules, it could also make sense to more intelligently merge different mempools' state changes.

##### Transaction Fees

A concrete case where this matters is in ensuring transactions pay their required fee. Even if the state machine logic verifies the fee rules, the simple load-balancing model can potentially allow N conflicting spends to be checked against the same fee funds, letting the attacker propagate N transactions for the price of 1.

This may be acceptable since it is bounded by the number of mempool workers (e.g. 16) and not unlimited, so has a limited impact in denial-of-service attacks. However, a possible solution is to require the account to have a balance of at least `N * fee_amount` to pass the fee check, so that a separate fee can be paid for each conflicting transaction.

##### Result Cache

As we process these transactions and we flush their state changes to mempool states, we can persist their sets of read keys and their side-effects to the state (a map of written keys and values).

Later, when we see the same transactions during block processing (`DeliverTx`), we will read from this result cache for concurrency scheduling, and also to possibly replay the writes if none of the read dependencies have been written to.
