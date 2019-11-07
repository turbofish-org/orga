# Deterministic Concurrent State Machines

**Matt Bell ([@mappum](https://twitter.com/mappum))** • [Nomic Hodlings, Inc.](https://nomic.io)

v0.0.0 - *November 5, 2019*

As data moves through a distributed computer network, each node sees the data in a different order. Blockchains are a tool to solve this problem by letting network nodes agree on a canonical ordering - solving the "double spend problem" in payments and guaranteeing all nodes have the same state.

Modern blockchain engineers typically stop here and build systems to simply process the data serially, one transaction after the next, in this canonical order. This simple model is easy to reason about, but severely limits performance since CPU utilization is bound to a single core due to the lack of concurrency.

There are many opportunities to scale blockchain processing to use all available compute resources by concurrently breaking up the work in a way where each node still achieves the same deterministic results.

This document explores a simple model for a framework that processes data concurrently but deterministically. Our concurrent processing is done without the need for concurrency primitives such as mutexes and atomics at the application level, but instead synchronizes purely through execution scheduling. Many of these concepts can be generalized (for instance, to a compiler which provides concurrency automatically without cognitive load on the developer), but we specifically focus on typical blockchain applications (payments, financial primitives, and other smart contracts).

We'll be taking the model as reasoned about in this document to implement a blockchain state machine framework in Rust, integrated with Tendermint consensus. An additional goal of our implementation will be for logic to be portable to other environments, including centralized web servers and peer-to-peer payment channels.

Concretely, my short-term goal is to build a blockchain system that can sustain the processing of 50,000 typical payment transactions per second on an average server (roughly the same throughput level as the WeChat payment network).

## Concurrency

Our model creates a *state machine*, logic which takes in *transitions* as input data, and processes them in a deterministic way. This logic reads and writes from a *store*, a mutable state which stores data with a key/value interface.

The store interface will be the same as typically found in embedded databases. Concretely, these are the methods `get(key) -> value`, `put(key, value)`, and `delete(key)` where keys and values are variable-length binary data. We assume that side-effects made by transitions are applied atomically after processing the logic of the whole transition (e.g. we collect the writes in a map then flush them to the state later).

As we process data through this interface, we can reason about the state observed or affected by each transition in terms of this key/value interface. For each transition, we track the set of keys it reads from, and the set of keys it writes to. In this document, we'll use the notation `Xr` to refer to the set of keys read by transition `X` for the current state, and `Xw` to refer to the set of keys written to (`put` and `delete`). Additionally, we will perform standard set operators on these, namely intersection (`&`) and union (`|`). Note that these sets only apply to the keys in the operations - if a transition calls `put('a', 'foo')` and another calls `put('a', 'bar')` we consider this an intersection for key `'a'` even though the values differ.

All processing that happens within our state machine logic is within the handling of transitions, so the concurrency model deals with using relations between transitions' read and write sets to guarantee transitions execute deterministically.

### Axioms

Our reasoning starts from the following axioms:

1. **Transitions which do not intersect in reads or writes can be processed concurrently with no coordination.** In other words, transitions `A` and `B` can be processed in parallel if the following is true: `(Ar | Aw) & (Br | Bw) = []` (`[]` means an empty set). For example, if Alice sends a payment to Bob, and Carol sends a payment to Dave, these transitions can be processed in any order and still affect the state the same way.
2. **Transitions where writes of `A` intersect with with the reads of `B` must be processed serially in a canonical order.** In other words, transitions `A` and `B` must be processed in-order if the following is true: `Aw & Br != []`. For example, if Bob has a balance of 0, Alice sends a payment of 1 to Bob, and Bob sends a payment of 1 to Carol, the second payment will fail if processed before the first. Note that this rule is commutative and for any two transitions it applies both ways.

And also, an optionally-used axiom which is possible due to our implementation:

3. **Transitions where writes of `A` intersect with with writes of `B`, but do not read from the intersecting writes can be processed concurrently, but must have their writes applied in a canonical order.** In other words, transitions `A` and `B` can be processed in parallel if the following is true, but their side-effects to the state must be written in-order: `Aw & Bw != []` and `(Ar | Br) & (Aw & Bw) = []`. For example, if there is a global variable `last_sender` and Alice and Bob each send transactions which set this variable to their own identity but do not read from this variable, we may execute the transactions in any order, but must flush their writes to the state in-order.

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

To service incoming state queries, query handlers can be created spanning across N threads (e.g. in a Tokio thread pool) which resolve queries against a snapshot of the latest committed state. At a low-level this can use RocksDB's consistent snapshot feature, allowing us to resolve queries against the state at height `H` even while committing a a new state for `H + 1`, meaning requests do not need to block waiting for the consensus process.

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


##### Read/Write Key Sets

Since at this step we are seeing transactions for the first time, we don't necessarily yet know the read and write keys which we will need for scheduling or correlation. However, there are a few strategies that let us figure these out:

- *Run-first* - If we just start executing the transition, we can simply run it first with no knowledge of the reads and writes, then at the end we will know the full sets of keys. This limits our ability to schedule transitions at the `CheckTx` step but has the benefit of simplicity since the developer doesn't have to write any code other than their core logic.
- *Derive in framework* - A different strategy is to require application developers to write a function which inspects the transition data and cheaply derives the expected write keys, or a superset of them. For example, a payment from Alice to Bob can cheaply derive the read and write sets which will both be Alice's account key and Bob's account key. This may be harder in some types of transactions but makes scheduling easier.
- *Conflict detection* - Another method is to proceed with running the transition, and on each read and write operation check against other workers to detect conflicts mid-execution. On detection of a conflict, we may need to halt execution and start again on the relevant worker which we conflicted with, adding some cost but possibly giving us a net-reduction in wasted work.

##### Result Cache

As we process these transactions and we flush their state changes to mempool states, we can persist their sets of read keys and their side-effects to the state (a map of written keys and values).

Later, when we see the same transactions during block processing (`DeliverTx`), we will read from this result cache for concurrency scheduling, and also to possibly replay the writes if none of the read dependencies have been written to.

#### `BeginBlock`, `DeliverTx`, `EndBlock`

This is the block verification cycle.

Our system will start processing the block by first preparing a new working state based on the most recently committed state (e.g. without the mempool transitions).

When processing the block, any tasks for the beginning of the block can be treated as individual transitions, then all of the `DeliverTx` transactions, then finally any tasks for the end of the block. This ordered queue will run based on our concurrency rules, scheduled since now we must guarantee that these transitions run deterministically. The scheduling algorithm is described in the *Scheduling Algorithm* section below.

#### `Commit`

After processing the transitions for a block cycle, we can commit the changes by flushing all writes to a backing store and computing the new state hash.

A possible throughput optimization here is to compute the new state hash before writing to disk, since then we block the consensus process for the lowest possible amount of time. The disk writes can happen in a background thread concurrently after sending the ABCI response as we wait for the next block cycle.

### Scheduling Algorithm

When we process transitions deterministically, e.g. within a single `CheckTx` mempool or during block processing (`DeliverTx`), we need to assign transitions to different worker threads based on a deterministic algorithm which optimizes for utilization while also not breaking any of the concurrency rules (based on the *Axioms* section in this document). A network will need to choose a *concurrency factor*, the number of *virtual workers* to group transitions into, where these virtual workers are guaranteed to be able to be run concurrently but may also be run serially on machines with less available CPUs with no difference in determinism.

Similar to operating systems, there are many possible scheduling algorithms we can use that will have different performance tradeoffs. However, unlike traditional OS schedulers we don't have any context-switching costs since we run transitions atomically, so we can optimize more for utilization. Another difference is that our scheduling algorithm needs to be deterministic (based on a canonically-orderered queue of transitions).

We can assume all transitions have a known set of read and write keys which we can use to ensure the concurrency rules are obeyed. Ideally these sets are exactly correct or a superset of the keys actually used by the transition so that scheduling is correct. However, we may also encouter transitions where the keys have changed between the first execution and the scheduled execution, in which case we should either fail the transaction or have the algorithm re-schedule it based on the updated set of keys.

#### Pseudocode

##### Epoch Scheduler

This is a simple formulation of the algorithm for demonstration, the actual implementation will be slightly more complex in order to optimize for higher utilization.

Note that this scheduling algorithm runs in a single thread and sends work to other threads.

- Given: N virtual workers `W0` to `WN` which each have sets of read and write keys `WXr` and `WXw`, an *input-queue* of transitions with known read and write keys, and an empty *wait-queue* which can hold transitions
- For every transition *T* in the queue:
  - Compute intersections between read and write keys of `T` with each virtual worker:
    - Set of write intersections: `Tw & WXw`
    - Set of read dependencies: `Tr & WXw`
    - Set of write dependents: `Tw & WXr`
  - If all of these sets are empty, our transition is concurrent to all the other transitions being processed (axiom #1):
    - If no workers are available:
      - Block until all workers have finished executing then clear their sets of read and write keys (wait until the next epoch)
      - Iterate through all transitions in the wait-queue (if not-empty) and process them the same way we process the input-queue (the outer loop)
    - Send the transition to the first idle worker and set its read and write keys to `Tr` and `Tw`
  - If any of the sets are non-empty, push the transition to the wait-queue
- If there are still any transitions in the wait-queue, iterate through them and process them the same way we process the input-queue (the loop above)

This algorithm is not particularly efficient since workers have to synchronize between the execution of each transition, so many may be idle while waiting for the last worker to finish (even while there may be more transitions that could be run in parallel).

##### Enhanced Schedulers

In the future, we can design more efficient schedulers. A few possible enhancements are:
- *Larger epochs* - A simple enhancement is to let each worker contain an ordered queue of N transitions for each epoch, rather than a single transition per worker.
- *Synchronization barriers* - Rather than epochs, workers could each have their own wait-queues which will also contain *barriers* which reference barriers in other workers as dependencies. When a barrier is encountered, the worker will only block if any of its dependencies have not completed, otherwise will continue. This can allow for higher utilization while maintaining determinism. Ordering the transitions and barriers can be done many ways for further optimization.
- *Time estimates* - If we can estimate the runtime of a transition, (e.g. based on recording its initial `CheckTx` runtime or using heuristics which inspect the transition), we can queue the transitions among workers more efficiently with bin-packing.
- *Execute and flush* - We currently treat transition execution and flushing its writes to our working store as a single atomic operation. If we separate these two phases, we can utilize axiom #3 to execute some transactions in parallel but flush their writes in-order for a slight gain in concurrency.

#### Optimized Set Operations

Since we are doing so many set intersection and union operations, it may make sense to optimize these operations with more efficient data structures. In a significant number of cases, there will no intersection (observation #1 above), so if we can optimize detecting this case then we can have a significant speedup. We can assume the naive implementation probably uses hash or tree-based sets, but it is likely helpful to also use Bloom filters.

If two Bloom filters were computed with the same hash functions, their intersection can be computed with simple bitwise AND - a fast operation that can be applied in constant-time for any number of elements in the set (whereas sets require `O(N)` time to find all intersections, where N is the number of elements in the smaller of the two sets). Likewise, union is computed with bitwise OR. This can give us false positives about intersections (and we can fall back to the full set intersection to check these), but no false negatives.
