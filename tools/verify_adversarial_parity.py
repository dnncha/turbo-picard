#!/usr/bin/env python3
"""Gate scientific edge cases against an explicitly selected upstream Picard.

Runs real executables, preserves every failed run, and records exact argv,
versions, source/binary/input hashes and exit status. CI runs this in both
Picard version jobs; ordinary unittest discovery does not substitute for it.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shlex
import subprocess
import sys
import tempfile

if __package__:
    from .sam_contract import VERSION, DESCRIPTION, sam_records
else:
    from sam_contract import VERSION, DESCRIPTION, sam_records

ROOT = Path(__file__).resolve().parents[1]
HEADER = "@HD\tVN:1.6\tSO:coordinate\n@SQ\tSN:chr1\tLN:1000\n@RG\tID:rg\tSM:sample\tLB:lib\tPL:ILLUMINA\n"
FRAGMENT = "frag\t0\tchr1\t10\t60\t4M\t*\t0\t0\tACGT\tIIII\tRG:Z:rg\n"
PAIR = "pair\t99\tchr1\t10\t60\t4M\t=\t30\t24\tACGT\tIIII\tRG:Z:rg\n"
PAIR_END = "pair\t147\tchr1\t30\t60\t4M\t=\t10\t-24\tACGT\tIIII\tRG:Z:rg\n"


def sha256(path):
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def stable(path):
    return [line for line in path.read_text().splitlines() if line.strip() and not line.startswith("#")]


class ParityRun:
    def __init__(self, root, native, upstream):
        self.root, self.native, self.upstream = root, [str(native)], upstream
        self.executions, self.cases = [], []

    def write(self, name, text):
        path = self.root / name
        path.write_text(text)
        return path

    def run(self, label, prefix, args, success=True):
        # Do not let either explicit or auto-discovered fallback certify a run.
        env = {key: value for key, value in os.environ.items() if key not in {"PICARD_JAR", "TURBO_PICARD_PICARD_JAR", "TURBO_PICARD_FALLBACK_COMMAND"}}
        env["TURBO_PICARD_REQUIRE_NATIVE"] = "1"
        argv = [*prefix, *map(str, args)]
        result = subprocess.run(argv, capture_output=True, text=True, env=env, timeout=180)
        (self.root / f"{label}.stdout").write_text(result.stdout)
        (self.root / f"{label}.stderr").write_text(result.stderr)
        self.executions.append({"label": label, "argv": argv, "exit_code": result.returncode})
        self.save()
        if success is not None and (result.returncode == 0) != success:
            raise AssertionError(f"{label}: unexpected exit {result.returncode}; see retained stderr")
        return result

    def save(self):
        (self.root / "commands.json").write_text(json.dumps(self.executions, indent=2) + "\n")

    def bam(self, sam, label):
        path = self.root / f"{label}.bam"
        self.run(label, self.upstream, ["SamFormatConverter", f"I={sam}", f"O={path}", "QUIET=true", "VALIDATION_STRINGENCY=SILENT"])
        return path

    def markdup(self, label, inputs, options=()):
        paths = []
        for name, prefix in [("native", self.native), ("picard", self.upstream)]:
            output, metrics = self.root / f"{label}-{name}.sam", self.root / f"{label}-{name}.metrics"
            self.run(f"{label}-{name}", prefix, ["MarkDuplicates", *[f"I={p}" for p in inputs], f"O={output}", f"M={metrics}", "READ_NAME_REGEX=null", "VALIDATION_STRINGENCY=SILENT", "QUIET=true", *options])
            paths.append((output, metrics))
        self.run(f"{label}-compare-v2", [sys.executable, str(ROOT / "tools/compare_markduplicates.py")], ["--picard-bam", paths[1][0], "--turbo-picard-bam", paths[0][0], "--picard-metrics", paths[1][1], "--turbo-picard-metrics", paths[0][1], "--json"])
        return paths[0][0]

    def case(self, name, operation):
        operation()
        self.cases.append(name)
        print(f"PASS {name}", flush=True)

    def verify(self):
        a = self.write("a.sam", HEADER + FRAGMENT.replace("frag", "a"))
        b = self.write("b.sam", (HEADER + FRAGMENT.replace("frag", "b")).replace("chr1", "chr2"))
        a_bam, b_bam = self.bam(a, "a-convert"), self.bam(b, "b-convert")

        def dictionaries():
            for name, prefix in [("native", self.native), ("picard", self.upstream)]:
                self.run(f"dictionary-{name}", prefix, ["MarkDuplicates", f"I={a_bam}", f"I={b_bam}", f"O={self.root}/dictionary-{name}.bam", f"M={self.root}/dictionary-{name}.metrics", "READ_NAME_REGEX=null", "QUIET=true"], success=False)
            checksummed = []
            for index in [1, 2]:
                checksummed.append(self.write(f"checksum-{index}.sam", HEADER.replace("LN:1000", f"LN:1000\tM5:{index:032x}") + FRAGMENT.replace("frag", f"checksum-{index}")))
            for name, prefix in [("native", self.native), ("picard", self.upstream)]:
                self.run(f"dictionary-md5-{name}", prefix, ["MarkDuplicates", *[f"I={input}" for input in checksummed], f"O={self.root}/dictionary-md5-{name}.bam", f"M={self.root}/dictionary-md5-{name}.metrics", "READ_NAME_REGEX=null", "QUIET=true"], success=False)
        self.case("F01 dictionary mismatch rejects", dictionaries)

        mixed = self.write("mixed.sam", HEADER + PAIR + FRAGMENT + PAIR_END)
        mixed_bam = self.bam(mixed, "mixed-convert")
        external = self.write("external.sam", HEADER + "@RG\tID:other\tSM:sample\tLB:lib\tPL:ILLUMINA\n" + PAIR + FRAGMENT + PAIR_END)
        external_bam = self.bam(external, "external-convert")
        def fragment_paths():
            for index, input in enumerate([mixed, mixed_bam, external_bam]):
                for tag in ["true", "false"]:
                    path = self.markdup(f"fragments-{index}-{tag}", [input], [f"TAG_DUPLICATE_SET_MEMBERS={tag}"])
                    assert next(record for record in sam_records(path) if record.query_name == "frag").duplicate
        self.case("F02 fragments agree across formats and plans", fragment_paths)

        score = self.write("score.sam", HEADER + FRAGMENT.replace("frag", "low").replace("IIII", "!!!!") + FRAGMENT.replace("frag", "high"))
        score_bam = self.bam(score, "score-convert")
        def selection():
            for index, input in enumerate([score, score_bam]):
                for remove in ["true", "false"]:
                    path = self.markdup(f"selection-{index}-{remove}", [input], [f"REMOVE_DUPLICATES={remove}"])
                    records = list(sam_records(path))
                    assert next(record for record in records if record.query_name == "high").duplicate is False
                    assert len(records) == (1 if remove == "true" else 2)
        self.case("F03 quality representative and removal agree", selection)

        noqual = self.write("noqual.sam", HEADER + "read\t4\t*\t0\t0\t*\t*\t0\t0\tACGT\t*\tRG:Z:rg\n")
        noqual_bam = self.bam(noqual, "noqual-convert")
        def quality():
            for index, input in enumerate([noqual, noqual_bam]):
                for combined in [False, True]:
                    outputs = []
                    for name, prefix in [("native", self.native), ("picard", self.upstream)]:
                        path = self.root / f"noqual-{index}-{combined}-{name}.metrics"
                        command = "CollectMultipleMetrics" if combined else "CollectQualityYieldMetrics"
                        self.run(f"noqual-{index}-{combined}-{name}", prefix, [command, f"I={input}", f"O={path}", "QUIET=true", *(["PROGRAM=null", "PROGRAM=CollectQualityYieldMetrics"] if combined else [])])
                        if combined: path = Path(f"{path}.quality_yield_metrics")
                        outputs.append(stable(path))
                    assert outputs[0] == outputs[1]
                    assert outputs[0][1] == "1\t1\t4\t4\t4\t0\t0\t0\t0\t0\t0"
        self.case("F04 absent qualities preserve bases without inventing yield", quality)

        rg_a = self.write("rg-a.sam", (HEADER + FRAGMENT.replace("frag", "a")).replace("LB:lib", "LB:libA"))
        rg_b = self.write("rg-b.sam", (HEADER + FRAGMENT.replace("frag", "b").replace("\t10\t", "\t20\t")).replace("LB:lib", "LB:libB"))
        rg_inputs = [self.bam(rg_a, "rg-a-convert"), self.bam(rg_b, "rg-b-convert")]
        self.case("F05 conflicting read groups preserve complete definitions", lambda: self.markdup("rg-identity", rg_inputs))

        vcf_header = "##fileformat=VCFv4.2\n##contig=<ID=chr1,length=1000>\n##FORMAT=<ID=GT,Number=1,Type=String,Description=\"genotype\">\n"
        columns = "#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\tFORMAT\tsample\n"
        extra = "##INFO=<ID=PRIVATE,Number=1,Type=Integer,Description=\"second, shard\">\n##FORMAT=<ID=EXTRA,Number=1,Type=Integer,Description=\"unused\">\n##FILTER=<ID=Low,Description=\"low\">\n"
        vcf_a = self.write("a.vcf", vcf_header + columns + "chr1\t10\t.\tA\tC\t50\tPASS\t.\tGT\t0/0\n")
        vcf_b = self.write("b.vcf", vcf_header + extra + columns + "chr1\t20\t.\tA\tT\t50\tLow\tPRIVATE=7\tGT\t0/1\n")
        def vcf_union():
            for command in ["MergeVcfs", "SortVcf"]:
                for spill in [1, 500_000]:
                    outputs = []
                    for name, prefix in [("native", self.native), ("picard", self.upstream)]:
                        path = self.root / f"{command}-{spill}-{name}.vcf"
                        self.run(f"{command}-{spill}-{name}", prefix, [command, f"I={vcf_a}", f"I={vcf_b}", f"O={path}", f"MAX_RECORDS_IN_RAM={spill}", "CREATE_INDEX=false", "QUIET=true"])
                        text = path.read_text()
                        for definition in extra.splitlines(): assert definition in text
                        outputs.append(stable(path))
                    assert outputs[0] == outputs[1]
        self.case("F06 VCF header union protects later field definitions", vcf_union)

        unsorted = self.write("unsorted.sam", HEADER + FRAGMENT.replace("frag", "later").replace("\t10\t", "\t20\t") + FRAGMENT.replace("frag", "earlier"))
        # Picard's writer rejects a false SO declaration before it can create
        # this input. Serialize it independently with samtools, which preserves
        # the deliberately stale header without invoking a native fast path.
        unsorted_bam = self.root / "unsorted.bam"
        self.run("unsorted-convert", ["samtools"], ["view", "-b", "-o", unsorted_bam, unsorted])
        def sort_records():
            for extension in ["sam", "bam"]:
                outputs = []
                for name, prefix in [("native", self.native), ("picard", self.upstream)]:
                    path = self.root / f"sorted-{extension}-{name}.{extension}"
                    self.run(f"sort-{extension}-{name}", prefix, ["SortSam", f"I={unsorted_bam}", f"O={path}", "SO=coordinate", "QUIET=true"])
                    if extension == "bam":
                        sam = path.with_suffix(".view.sam")
                        self.run(f"sort-view-{name}", self.native, ["ViewSam", f"I={path}", f"O={sam}"])
                        path = sam
                    records = list(sam_records(path))
                    assert [record.position for record in records] == [10, 20]
                    outputs.append(records)
                assert outputs[0] == outputs[1]
        self.case("F07 sort verifies records despite stale SO metadata", sort_records)

        tie_rows = [
            "z\t0\tchr1\t10\t60\t4M\t*\t0\t0\tACGT\tIIII",
            "a\t16\tchr1\t10\t60\t4M\t*\t0\t0\tACGT\tIIII",
            "pair\t147\tchr1\t1\t60\t4M\t=\t90\t-93\tACGT\tIIII",
            "pair\t99\tchr1\t90\t60\t4M\t=\t1\t93\tACGT\tIIII",
            "hit\t256\tchr1\t10\t40\t4M\t*\t0\t0\tACGT\tIIII\tHI:i:1",
            "hit\t256\tchr1\t10\t60\t4M\t*\t0\t0\tACGT\tIIII\tHI:i:0",
            "u0\t20\t*\t0\t0\t*\t*\t0\t0\tACGT\tIIII",
            "u1\t4\t*\t0\t0\t*\t*\t0\t0\tACGT\tIIII",
        ]
        ties = self.write("sort-ties.sam", HEADER + "\n".join(tie_rows) + "\n")
        ties_bam = self.root / "sort-ties.bam"
        self.run("sort-ties-convert", ["samtools"], ["view", "-b", "-o", ties_bam, ties])
        def sort_ties():
            for index, input in enumerate([ties, ties_bam]):
                for order in ["coordinate", "queryname"]:
                    for spill in [1, 500_000]:
                        outputs = []
                        for name, prefix in [("native", self.native), ("picard", self.upstream)]:
                            label = f"sort-ties-{index}-{order}-{spill}-{name}"
                            path = self.root / f"{label}.sam"
                            self.run(label, prefix, ["SortSam", f"I={input}", f"O={path}", f"SO={order}", f"MAX_RECORDS_IN_RAM={spill}", "VALIDATION_STRINGENCY=SILENT", "QUIET=true"])
                            outputs.append(list(sam_records(path)))
                        assert outputs[0] == outputs[1]
        self.case("sorting tie rules agree across formats and spill paths", sort_ties)

        reference = self.write("reference.fa", ">chr1\n" + "ACGT" * 250 + "\n")
        self.write("reference.fa.fai", "chr1\t1000\t6\t1000\t1001\n")
        self.run("reference-dictionary", self.upstream, ["CreateSequenceDictionary", f"R={reference}", f"O={self.root}/reference.dict", "QUIET=true"])
        invalid_nm = self.write("invalid-nm.sam", HEADER + "read\t0\tchr1\t1\t60\t4M\t*\t0\t0\tTTTT\tIIII\tRG:Z:rg\tNM:i:0\n")
        invalid_nm_bam = self.bam(invalid_nm, "invalid-nm-convert")
        def validate_nm():
            for index, input in enumerate([invalid_nm, invalid_nm_bam]):
                for mode in ["SUMMARY", "VERBOSE"]:
                    for name, prefix in [("native", self.native), ("picard", self.upstream)]:
                        output = self.root / f"validation-{index}-{mode}-{name}.txt"
                        result = self.run(f"validation-{index}-{mode}-{name}", prefix, ["ValidateSamFile", f"I={input}", f"R={reference}", f"O={output}", f"MODE={mode}", "QUIET=true"], success=False)
                        assert result.returncode == 3
                        assert "INVALID_TAG_NM" in output.read_text()
        self.case("F08 supplied reference protects NM in both formats and modes", validate_nm)

        def validation_categories():
            documents = [
                HEADER + FRAGMENT.rstrip("\n") + "\tNM:i:0\n",
                HEADER + FRAGMENT,
                HEADER.replace("@RG\tID:rg\tSM:sample\tLB:lib\tPL:ILLUMINA\n", "") + FRAGMENT.replace("\tRG:Z:rg", ""),
                HEADER + "invalid\t4\t*\t0\t60\t*\t*\t0\t0\tACGT\tIIII\tRG:Z:rg\n",
            ]
            for expected, document in enumerate(documents):
                input = self.write(f"validation-category-{expected}.sam", document)
                outputs = []
                for name, prefix in [("native", self.native), ("picard", self.upstream)]:
                    label = f"validation-category-{expected}-{name}"
                    output = self.root / f"{label}.txt"
                    result = self.run(label, prefix, ["ValidateSamFile", f"I={input}", f"O={output}", "MODE=SUMMARY", "QUIET=true"], success=None)
                    assert result.returncode == expected, f"{label}: expected exit {expected}, got {result.returncode}"
                    outputs.append(stable(output))
                assert outputs[0] == outputs[1]
        self.case("validation exits preserve clean, warnings, mixed and errors categories", validation_categories)

        mate_rows = [
            "complete\t99\tchr1\t10\t60\t4M\t*\t0\t0\tACGT\tIIII",
            "complete\t355\tchr1\t20\t20\t4M\t*\t0\t0\tGGGG\tIIII",
            "complete\t147\tchr1\t30\t60\t4M\t*\t0\t0\tACGT\tIIII",
            "mixed-map\t73\tchr1\t10\t60\t4M\t*\t0\t0\tACGT\tIIII\tMQ:i:55\tMC:Z:stale",
            "mixed-map\t133\t*\t0\t0\t*\t*\t0\t0\tACGT\tIIII\tMQ:i:55\tMC:Z:stale",
            "orphan\t99\tchr1\t10\t60\t4M\t=\t30\t24\tACGT\tIIII",
            "orphan\t2147\tchr1\t100\t60\t4M\t=\t30\t24\tGGGG\tIIII",
            "supponly\t2147\tchr1\t10\t60\t4M\t=\t30\t24\tACGT\tIIII",
            "supponly\t2195\tchr1\t30\t60\t4M\t=\t10\t-24\tGGGG\tIIII",
            "tandem\t67\tchr1\t10\t60\t4M\t*\t0\t0\tACGT\tIIII",
            "tandem\t131\tchr1\t30\t60\t4M\t*\t0\t0\tACGT\tIIII",
            "unmapped\t77\tchr1\t10\t0\t*\t=\t10\t0\tACGT\tIIII\tMQ:i:55\tMC:Z:stale",
            "unmapped\t141\tchr1\t10\t0\t*\t=\t10\t0\tACGT\tIIII\tMQ:i:55\tMC:Z:stale",
        ]
        mates = self.write("mate-boundaries.sam", HEADER.replace("SO:coordinate", "SO:queryname") + "\n".join(mate_rows) + "\n")
        def mate_boundaries():
            for cigar in ["true", "false"]:
                outputs = []
                for name, prefix in [("native", self.native), ("picard", self.upstream)]:
                    label = f"mate-boundaries-{cigar}-{name}"
                    path = self.root / f"{label}.sam"
                    self.run(label, prefix, ["FixMateInformation", f"I={mates}", f"O={path}", "SO=queryname", f"ADD_MATE_CIGAR={cigar}", "VALIDATION_STRINGENCY=SILENT", "QUIET=true"])
                    outputs.append(list(sam_records(path)))
                assert outputs[0] == outputs[1]
        self.case("mate repair handles missing primaries without Java fallback", mate_boundaries)

        noise_mixed = self.write("noise-mixed.sam", HEADER + PAIR + FRAGMENT + PAIR_END + "noise\t4\t*\t0\t0\t*\t*\t0\t0\tACGT\tIIII\tRG:Z:rg\tXN:i:1\n")
        noise_bam = self.bam(noise_mixed, "noise-convert")
        def alignment():
            for index, input in enumerate([noise_mixed, noise_bam]):
                for combined in [False, True]:
                    outputs = []
                    for name, prefix in [("native", self.native), ("picard", self.upstream)]:
                        path = self.root / f"alignment-{index}-{combined}-{name}.metrics"
                        command = "CollectMultipleMetrics" if combined else "CollectAlignmentSummaryMetrics"
                        self.run(f"alignment-{index}-{combined}-{name}", prefix, [command, f"I={input}", f"O={path}", "QUIET=true", *(["PROGRAM=null", "PROGRAM=CollectAlignmentSummaryMetrics"] if combined else [])])
                        if combined: path = Path(f"{path}.alignment_summary_metrics")
                        outputs.append(stable(path))
                    assert outputs[0] == outputs[1]
                    row = next(line.split("\t") for line in outputs[0] if line.startswith("UNPAIRED\t"))
                    assert row[1] == "2" and row[4] == "1"
                    assert "4\t2\t2\t2\t1" in outputs[0]
        self.case("F09/F10 mixed populations and noise retain complete metrics", alignment)

        g_header = "##fileformat=VCFv4.2\n##contig=<ID=chr1,length=1000>\n#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\n"
        g1 = self.write("g1.vcf", g_header + "chr1\t20\t.\tA\tC\t50\tPASS\t.\n")
        def gather():
            for position in [10, 20, 30]:
                g2 = self.write(f"g2-{position}.vcf", g_header + f"chr1\t{position}\t.\tA\tT\t50\tPASS\t.\n")
                for name, prefix in [("native", self.native), ("picard", self.upstream)]:
                    self.run(f"gather-{position}-{name}", prefix, ["GatherVcfs", f"I={g1}", f"I={g2}", f"O={self.root}/gather-{position}-{name}.vcf", "CREATE_INDEX=false", "QUIET=true"], success=position > 20)
        self.case("F11 gather rejects reversed and overlapping shards", gather)

        def comparator_mutation():
            left = self.write("mutation-before.sam", HEADER + FRAGMENT)
            right = self.write("mutation-after.sam", HEADER + FRAGMENT.replace("\t0\tchr1", "\t16\tchr1").replace("ACGT\tIIII", "TTTT\t!!!!").replace("RG:Z:rg", "RG:Z:wrong"))
            before = self.write("mutation-before.metrics", "## METRICS CLASS\tpicard.sam.DuplicationMetrics\nLIBRARY\tREAD_PAIRS_EXAMINED\nlib\t1\n\n## HISTOGRAM\tjava.lang.Double\nset_size\tall_sets\n1.0\t1\n")
            after = self.write("mutation-after.metrics", before.read_text().split("## HISTOGRAM")[0])
            self.run("comparator-rejects-corruption", [sys.executable, str(ROOT / "tools/compare_markduplicates.py")], ["--picard-bam", left, "--turbo-picard-bam", right, "--picard-metrics", before, "--turbo-picard-metrics", after, "--json"], success=False)
        self.case("F12 scientific corruption fails the parity gate", comparator_mutation)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--turbo-picard", type=Path, default=ROOT / "target/debug/turbo-picard")
    upstream = parser.add_mutually_exclusive_group(required=True)
    upstream.add_argument("--picard-jar", type=Path)
    upstream.add_argument("--picard-command", help="Upstream argv prefix parsed by shlex; never executed through a shell")
    parser.add_argument("--expected-picard-version")
    parser.add_argument("--output-dir", type=Path, help="Must not already exist; all evidence is retained")
    args = parser.parse_args()
    directory = args.output_dir
    if directory is None:
        directory = Path(tempfile.mkdtemp(prefix="turbo-picard-adversarial-"))
    else:
        directory.mkdir(parents=True, exist_ok=False)
    print(f"Retaining evidence in {directory}", flush=True)
    prefix = ["java", "-Xmx512m", "-jar", str(args.picard_jar.resolve())] if args.picard_jar else shlex.split(args.picard_command)
    run = ParityRun(directory, args.turbo_picard.resolve(), prefix)
    metadata = {"schema_version": 1, "comparator": VERSION, "contract": DESCRIPTION, "status": "FAIL"}
    try:
        version = run.run("picard-version", prefix, ["MarkDuplicates", "--version"], success=None)
        text = version.stdout + version.stderr
        found = re.search(r"Version:([0-9]+\.[0-9]+\.[0-9]+)", text)
        if not found or (args.expected_picard_version and found[1] != args.expected_picard_version):
            raise AssertionError(f"upstream Picard version did not match: {text}")
        metadata["picard_version"] = found[1]
        metadata["native_version"] = run.run("native-version", run.native, ["--version"]).stdout.strip()
        run.verify()
        metadata["status"] = "PASS"
        return 0
    except (AssertionError, ValueError, OSError, subprocess.TimeoutExpired) as error:
        metadata["error"] = str(error)
        print(f"FAIL: {error}; evidence retained in {directory}", file=sys.stderr)
        return 1
    finally:
        metadata["cases"] = run.cases
        metadata["native_sha256"] = sha256(args.turbo_picard.resolve())
        metadata["source_commit"] = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
        metadata["source_state"] = subprocess.check_output(["git", "status", "--porcelain"], cwd=ROOT, text=True).splitlines()
        metadata["source_diff_sha256"] = hashlib.sha256(subprocess.check_output(["git", "diff", "HEAD"], cwd=ROOT)).hexdigest()
        sources = subprocess.check_output(["git", "ls-files", "--cached", "--others", "--exclude-standard"], cwd=ROOT, text=True).splitlines()
        metadata["source_files_sha256"] = {name: sha256(ROOT / name) for name in sorted(set(sources)) if (ROOT / name).is_file() and Path(name).suffix in {".rs", ".py", ".toml", ".lock", ".yml"}}
        metadata["files"] = {path.name: sha256(path) for path in sorted(directory.iterdir()) if path.is_file() and path.name != "result.json"}
        (directory / "result.json").write_text(json.dumps(metadata, indent=2) + "\n")


if __name__ == "__main__":
    raise SystemExit(main())
