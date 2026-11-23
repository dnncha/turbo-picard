import subprocess
from pathlib import Path

SCRIPT = Path(__file__).with_name("duel_vm.sh")
GENOME_BASES = 3.1e9
READ_LENGTH = 151


def run_check(tmp_path: Path, depth: float, target: str) -> subprocess.CompletedProcess[str]:
    reads = int(depth * GENOME_BASES / READ_LENGTH)
    stub = tmp_path / "bin"
    stub.mkdir()
    samtools = stub / "samtools"
    samtools.write_text(f"#!/bin/sh\nprintf 'chr1\\t248956422\\t{reads}\\t0\\n'\n")
    samtools.chmod(0o755)
    data = tmp_path / "data"
    data.mkdir()
    (data / "4x.bam").write_text("")
    function = subprocess.run(
        ["sed", "-n", "/^check_depth()/,/^}/p", str(SCRIPT)], capture_output=True, text=True, check=True
    ).stdout
    return subprocess.run(
        ["bash", "-c", f'{function}\ncheck_depth 4x "{target}"'],
        env={"BIN": str(stub), "DATA": str(data), "PATH": "/usr/bin:/bin"},
        capture_output=True,
        text=True,
        check=False,
    )


def test_a_subsample_at_its_named_depth_passes(tmp_path: Path) -> None:
    result = run_check(tmp_path, 4.1, "4")
    assert result.returncode == 0, result.stderr
    assert "prepare 4x: 4.10x (target 4x)" in result.stdout


def test_a_4x_subsample_that_came_out_near_10x_is_refused_and_removed(tmp_path: Path) -> None:
    result = run_check(tmp_path, 7.6, "4")
    assert result.returncode == 1
    assert "expected about 4x but the subsample is 7.60x" in result.stderr
    assert not (tmp_path / "data" / "4x.bam").exists()
