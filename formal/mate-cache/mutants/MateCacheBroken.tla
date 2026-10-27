---------------------------- MODULE MateCacheBroken ----------------------------
\* Model of ExternalMateCache (crates/turbo-picard-markdup/src/external_mates.rs).
\* Property: every completed pair is the canonical first/second occurrence
\* pairing for that template key, under every cache eviction and filter
\* false-positive schedule.
EXTENDS Naturals, Sequences, FiniteSets

CONSTANTS Keys, MaxPending, MaxBytes, MaxRecords

VARIABLES next, input, pending, pendingBytes, spilled, may, spillSeq, completed, finished

vars == <<next, input, pending, pendingBytes, spilled, may, spillSeq, completed, finished>>

None == 0

TypeOK ==
  /\ next \in 0..MaxRecords
  /\ input \in [1..next -> [key: Keys, size: {1, 2, 4}]]
  /\ pending \in [Keys -> 0..MaxRecords]
  /\ pendingBytes \in 0..(MaxRecords * 4)
  /\ spilled \in SUBSET Keys
  /\ may \in SUBSET Keys
  /\ spillSeq \in Seq(0..MaxRecords)
  /\ completed \subseteq SUBSET (1..MaxRecords)
  /\ \A p \in completed: Cardinality(p) = 2
  /\ finished \in BOOLEAN

PendingKeys == {k \in Keys : pending[k] # None}
PendingCount == Cardinality(PendingKeys)

\* Ordinals in input order for one key, and for the spilled set of one key.
OrdsOf(k) == SelectSeq([i \in 1..next |-> i], LAMBDA o: input[o].key = k)
SeqSet(seq) == {seq[i] : i \in 1..Len(seq)}
SpilledOrdsOf(k) ==
  SelectSeq([i \in 1..next |-> i], LAMBDA o: o \in SeqSet(spillSeq) /\ input[o].key = k)

RECURSIVE PairUp(_, _)
PairUp(ords, i) ==
  IF i + 1 <= Len(ords)
  THEN {{ords[i], ords[i + 1]}} \union PairUp(ords, i + 2)
  ELSE {}

CanonicalPairs == UNION {PairUp(OrdsOf(k), 1) : k \in Keys}
ExactPairsOf(seq) ==
  UNION {PairUp(SelectSeq([i \in 1..next |-> i],
                          LAMBDA o: o \in SeqSet(seq) /\ input[o].key = k), 1)
         : k \in Keys}

SpilledMay == spilled \subseteq may
PairsCanonical == completed \subseteq CanonicalPairs
FinishedComplete == finished => completed = CanonicalPairs

Init ==
  /\ next = 0
  /\ input = <<>>
  /\ pending = [k \in Keys |-> None]
  /\ pendingBytes = 0
  /\ spilled = {}
  /\ may = {}
  /\ spillSeq = <<>>
  /\ completed = {}
  /\ finished = FALSE

\* Common frame for one arriving record.
Arrive(k, s, o) ==
  /\ next' = o
  /\ input' = Append(input, [key |-> k, size |-> s])
  /\ finished' = finished

Input(k, s) ==
  /\ ~finished
  /\ next < MaxRecords
  /\ k \in Keys
  /\ s \in {1, 2, 4}
  /\ LET o == next + 1 IN
     \/ \* external_mates.rs: was_spilled -> spill pending, then the record
        /\ FALSE \* MUTANT: the was_spilled check is deleted
        /\ spilled' = spilled \union {k}
        /\ may' = may
        /\ spillSeq' =
             IF pending[k] # None THEN Append(Append(spillSeq, pending[k]), o)
             ELSE Append(spillSeq, o)
        /\ pending' = [pending EXCEPT ![k] = None]
        /\ pendingBytes' =
             pendingBytes - (IF pending[k] # None THEN input[pending[k]].size ELSE 0)
        /\ completed' = completed
        /\ Arrive(k, s, o)
     \/ \* external_mates.rs: cached occurrence pairs with the incoming one
        /\ TRUE \* MUTANT: was_spilled guard deleted
        /\ pending[k] # None
        /\ completed' = completed \union {{pending[k], o}}
        /\ pending' = [pending EXCEPT ![k] = None]
        /\ pendingBytes' = pendingBytes - input[pending[k]].size
        /\ spillSeq' = spillSeq
        /\ spilled' = spilled
        /\ may' = may
        /\ Arrive(k, s, o)
     \/ \* external_mates.rs: oversized record is marked and spilled
        /\ TRUE \* MUTANT: was_spilled guard deleted
        /\ pending[k] = None
        /\ s > MaxBytes
        /\ spilled' = spilled \union {k}
        /\ may' = may \union {k}
        /\ spillSeq' = Append(spillSeq, o)
        /\ pending' = pending
        /\ pendingBytes' = pendingBytes
        /\ completed' = completed
        /\ Arrive(k, s, o)
     \/ \* external_mates.rs: capacity flush, then insert the record
        /\ TRUE \* MUTANT: was_spilled guard deleted
        /\ pending[k] = None
        /\ s <= MaxBytes
        /\ PendingCount = MaxPending \/ pendingBytes + s > MaxBytes
        /\ LET flushSeq == SelectSeq([i \in 1..next |-> i],
                                     LAMBDA x: \E kk \in PendingKeys: pending[kk] = x)
               newSpill == spillSeq \o flushSeq
           IN
           /\ spilled' = spilled \union PendingKeys
           /\ may' = may \union PendingKeys
           /\ spillSeq' = newSpill
           /\ pending' = [x \in Keys |-> IF x = k THEN o ELSE None]
           /\ pendingBytes' = s
           /\ completed' = completed
           /\ Arrive(k, s, o)
     \/ \* external_mates.rs: cache the record
        /\ TRUE \* MUTANT: was_spilled guard deleted
        /\ pending[k] = None
        /\ s <= MaxBytes
        /\ ~(PendingCount = MaxPending \/ pendingBytes + s > MaxBytes)
        /\ pending' = [pending EXCEPT ![k] = o]
        /\ pendingBytes' = pendingBytes + s
        /\ spillSeq' = spillSeq
        /\ spilled' = spilled
        /\ may' = may
        /\ completed' = completed
        /\ Arrive(k, s, o)

\* A bloom-filter false positive: an unspilled key may report spilled.
FalsePositive(k) ==
  /\ ~finished
  /\ k \in Keys
  /\ TRUE \* MUTANT: was_spilled guard deleted
  /\ may' = may \union {k}
  /\ UNCHANGED <<next, input, pending, pendingBytes, spilled, spillSeq, completed, finished>>

\* End of input: flush the cache, then pair spilled occurrences exactly.
Finish ==
  /\ ~finished
  /\ LET flushSeq == SelectSeq([i \in 1..next |-> i],
                               LAMBDA x: \E kk \in PendingKeys: pending[kk] = x)
         newSpill == spillSeq \o flushSeq
     IN
     /\ spilled' = spilled \union PendingKeys
     /\ may' = may \union PendingKeys
     /\ spillSeq' = newSpill
     /\ pending' = [k \in Keys |-> None]
     /\ pendingBytes' = 0
     /\ completed' = completed \union ExactPairsOf(newSpill)
     /\ finished' = TRUE
     /\ UNCHANGED <<next, input>>

Next ==
  \/ \E k \in Keys, s \in {1, 2, 4}: Input(k, s)
  \/ \E k \in Keys: FalsePositive(k)
  \/ Finish

Spec == Init /\ [][Next]_vars
=============================================================================
