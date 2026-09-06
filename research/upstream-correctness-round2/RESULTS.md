# Five additional bioinformatics correctness findings

Audit date: 6 September 2026. These are distinct from the three first-round PRs, now published as BEDTools #1145 and HTSJDK #1799 / #1800.

## Evidence and scope

[Native execution run 34029187758](https://github.com/dnncha/turbo-picard/actions/runs/34029187758) completed successfully in both jobs. The executed harness commit is `97ec8db81454d601ab681bc8872ec820d4aa8ce7`; `audit.py` Git blob is `f14bc91b8adc6b9ae485740c5408c21700fdc24f`.

The Python job executed released Biopython 1.88, Scanpy 1.12.4 and GSEApy 1.3.1 through their public APIs. Its supplementary exhaustive distance check called the underlying `_pairwise` method. BEDTools was compiled and executed from upstream commit `614e9a5c5935ab86e873dab9072fbbaf003c1b7e`.

All inputs are synthetic. These results establish concrete failure cases, not the fraction of real datasets or published papers affected. Full upstream regression suites were not run in this second round. No second-round fixes, upstream issues or pull requests are claimed as submitted.

Native artifacts:

- Python artifact `9988043000`, SHA256 `8d6be640e1806fc2e9f8ed384260872795e6c78f55e4aed7471c38196fae686b`.
- BEDTools artifact `9988059638`, SHA256 `3dc0f3bfd6917f5fa4ec114cee379d3aca3660815dcd7210fb5e4f96dbc9e70a`.

They contain machine-readable results, source snapshots, dependency versions, and BEDTools build logs. Copies were downloaded and their hashes verified. GitHub artifact retention is 30 days; the companion downloadable bundle preserves copies of the contents.

## 1. GSEApy: selecting overlapping pathways before BH correction

**Executed package:** GSEApy 1.3.1. **Scope:** local Enrichr-style over-representation analysis with fixed custom gene sets, not the remote Enrichr service or all GSEA methods.

`calc_pvalues` discards a pathway when its observed overlap is zero. `enrich_local` then applies Benjamini-Hochberg (BH) correction only to the returned p-values. This selects the correction family using the outcome of the same test, removing the p=1 hypotheses before correction.

A public `gseapy.enrichr` call used 1,000 background genes partitioned into 100 fixed disjoint pathways of 10 genes. The five-gene query selected one gene from each of five pathways.

| Quantity, for each returned pathway | Observed / reference |
| --- | ---: |
| Raw p-value | 0.04910629542831942 |
| Reported adjusted p-value | 0.04910629542831942 |
| BH-adjusted p-value across the 100-pathway family | 0.9821259085663884 |
| Pathways returned | 5 of 100 |

At alpha=0.05, the reported result declares all five significant while full-family BH declares none. A second fixture with two target hits gives reported adjusted p=0.0008865335332037756 versus full-family adjusted p=0.08865335332037756.

### Complete-null experiment

With the same fixed partition, the native API was called for 100 uniformly sampled five-gene queries without replacement (NumPy seed 20260906). Every hypothesis is null under this exchangeable sampling design.

- Native runs with at least one reported adjusted p<0.05: **100/100**.
- Full-family BH runs with any discovery on the same queries: **0/100**.

This is not a real-world error-rate estimate. The failure can also be derived: every retained set has at least one hit, whose upper-tail p is at most 0.0491063. BH over only those retained sets rejects them all. Since the partition covers the background, every query returns at least one discovery. Thus the procedure has false discovery rate 1 for this constructed complete-null design.

An independent integer-combinatorics calculation, `1 - C(990,5)/C(1000,5)`, agrees with the raw one-hit p-value. The issue is the correction family, not hypergeometric numerical precision or the BH routine itself.

**Correction direction:** retain all pre-specified eligible nonempty pathways during correction, including zero-overlap p=1 hypotheses; hide zero-hit rows afterward if desired. Filtering by background membership or pre-specified size is a different operation from filtering by observed overlap. Full-family BH arithmetic is the reference here; no claim is made that arbitrary pathway dependence automatically satisfies every FDR theorem.

Source: [stats.py](https://github.com/zqfang/GSEApy/blob/6e6f0e29ce3b407a7fb19bc6a9a73ee0015263fa/gseapy/stats.py), [enrichr.py](https://github.com/zqfang/GSEApy/blob/6e6f0e29ce3b407a7fb19bc6a9a73ee0015263fa/gseapy/enrichr.py).

## 2. Biopython: conflicting trees yield false unanimity

**Executed package:** Biopython 1.88. **Scope:** `Bio.Phylo.Consensus`.

Inputs:

```text
((A:1,B:1):1,(C:1,D:1):1);
((A:1,C:1):1,(B:1,D:1):1);
((A:1,D:1):1,(B:1,C:1):1);
```

Every nontrivial clade occurs in just one of the three trees. At a unanimity threshold there should be no nontrivial consensus clade.

Actual public-API results:

- `majority_consensus(..., cutoff=1.0)` returns clade A+B with confidence **100.0**, despite its actual frequency of **1/3**.
- `strict_consensus(...)` returns A+B and C+D instead of an unresolved result.
- All six input-order permutations were tested. The spurious strict-consensus result follows the first input tree.

`_tree_to_bitstrs` assigns bit positions using each tree's own traversal order. Different taxa therefore acquire the same positional bit mask, which `_count_clades` treats as the same clade across trees. This is a taxon-identity error, not rounding or legitimate phylogenetic uncertainty.

**Known report:** [Biopython issue #3345](https://github.com/biopython/biopython/issues/3345), opened 10 November 2020 and still open when checked. This is not claimed as a new discovery. The new contribution here is a current native reproduction showing false unanimity and strict-consensus input-order dependence.

**Correction direction:** use one shared, validated taxon index for all input trees and decode with that same index. Test topology invariance under both tree and child order, identical tip sets, and distinct clades that share traversal positions.

Source: [Consensus.py](https://github.com/biopython/biopython/blob/dc262b5c437e07a8cc1cfb8a734c0d84a4434b23/Bio/Phylo/Consensus.py).

## 3. Scanpy: binary logistic marker scores assigned to the opposite group

**Executed package:** Scanpy 1.12.4. **Scope:** `rank_genes_groups(method="logreg")` with two fitted groups; not the t-test or Wilcoxon methods.

Synthetic expression before log1p: 12 group-A cells express marker_A=10 and marker_B=0; 12 group-B cells express marker_A=0 and marker_B=10. A third near-constant gene supplies a neutral control. No scientific interpretation of the neutral control is needed.

With category order A,B, the output labelled A ranks marker_B first:

| Gene | Reported score in output labelled A |
| --- | ---: |
| marker_B | +1.2794039249420166 |
| marker_A | -1.2791163921356201 |

The explicit `groups=["A"], reference="B"` call reproduces the same reversal. Dense and CSR inputs, both category orders, and three group/reference selections were tested: eight of twelve variants return the wrong group's marker as the positive top marker. That is a fixture count, not an estimate of user exposure.

In binary logistic regression, scikit-learn's coefficient row is oriented to `classes_[1]`. Scanpy assigns that row to its first output group without orienting the sign to the group's encoded class. Reordering categories changes which named group is affected.

**Correction direction:** map the target to `clf.classes_`, negate the binary coefficient vector for the other class, and explicitly test name/sign consistency under category and requested-group permutations. Returning both groups rather than one is a separate API-contract decision, not needed to demonstrate the sign error.

Source: [_RankGenes.logreg](https://github.com/scverse/scanpy/blob/ec374022343eb7ef80bbe3139264e37552cb79b4/src/scanpy/tools/_rank_genes_groups.py). Historical multiclass/group-order fixes exist; this audit does not claim the binary finding is previously unknown.

## 4. Biopython: explicitly skipped columns inflate identity distance

**Executed package:** Biopython 1.88. **Scope:** `DistanceCalculator("identity", skip_letters=("N", "-"))`.

| Identical sequence on both sides | Observed distance | Distance over comparable columns |
| --- | ---: | ---: |
| ACGT | 0.0 | 0.0 |
| ACGTNNNN | 0.5 | 0.0 |
| ACGT-------- | 0.6666666666666667 | 0.0 |

The numerator skips configured characters, while the denominator remains the entire sequence length. Adding identical excluded columns therefore increases distance between otherwise identical sequences.

The three public `get_distance` examples were supplemented by exhaustive `_pairwise` checks over three-character strings from A/C/N: **330 of 604** eligible ordered pairs disagree with an independent comparable-column identity-distance oracle. Pairs with no comparable positions were deliberately excluded; their policy was not judged.

**Correction direction:** use the same valid-column mask in numerator and denominator. Preserve or explicitly define all-missing behavior. The audit does not claim the default identity configuration, which has no skipped letters, is affected by this particular defect.

Source: [TreeConstruction.py](https://github.com/biopython/biopython/blob/dc262b5c437e07a8cc1cfb8a734c0d84a4434b23/Bio/Phylo/TreeConstruction.py).

## 5. BEDTools: -split loses the -e either-fraction rule

**Executed source:** upstream `614e9a5c5935ab86e873dab9072fbbaf003c1b7e`.

A is one BED12 block [0,100); B is [40,60). A's covered fraction is 0.2 and B's is 1.0.

```bash
bedtools intersect -a A.bed -b B.bed -f 0.9 -F 0.9 -e -wa -wb
```

The documented `-e` contract is that either threshold may pass. B passes, so the pair must be returned.

Observed output line counts:

| Extra flags | Lines |
| --- | ---: |
| none | 1 |
| -split | 0 |
| -sorted | 1 |
| -split -sorted | 0 |

There is only one block and one database interval: this fixture does not depend on the previously reported merged-union arithmetic or multi-candidate reciprocal-denominator bugs. The split path checks both fraction conditions separately and does not honor the either-fraction option.

**Correction direction:** carry the either-fraction setting into block-aware filtering and preserve its Boolean contract. Coordinate with the existing reciprocal-filtering work rather than submitting conflicting or duplicate fixes. Simply dropping `-split` is not a valid general workaround for genuinely spliced inputs.

Sources: [official intersect documentation](https://bedtools.readthedocs.io/en/latest/content/tools/intersect.html), [BlockMgr.cpp](https://github.com/arq5x/bedtools2/blob/614e9a5c5935ab86e873dab9072fbbaf003c1b7e/src/utils/FileRecordTools/Records/BlockMgr.cpp).

## Current-source check and next validation gates

The executed Biopython functions and GSEApy `calc_pvalues` are AST-identical to the downloaded current default-branch versions. Scanpy's coefficient-assignment loop is AST-identical; the surrounding method changes only its return annotation and `.values` to `.to_numpy()`. GSEApy's current caller adds flooring for the Combined Score but leaves the tested-family correction unchanged. These comparisons are source checks, not claims that the entire Python default branches were executed.

The strongest next submissions are GSEApy's test-family error and Scanpy's binary score orientation; the Biopython consensus issue should receive a properly attributed current reproduction rather than a duplicate discovery claim. Before proposing fixes, run focused failing-then-passing tests and the relevant upstream regression suites, check contribution policies, and quantify representative-data effects separately.

All audit implementation and analysis were prepared with AI assistance. Native package outputs and transparent mathematical counterexamples, not an assertion of novelty, support these findings.
