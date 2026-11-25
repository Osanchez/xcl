# Resume here (2026-11-25)

Handoff for continuing this fork's work on another PC. Read this first, then
[telemetry.md](telemetry.md) and the last entry of `.flowestra/decisions.md`.

## Goal

Run the runtime as fast as possible on a 32-lane NVIDIA card (RTX 5080): reach 30 fps, then go
beyond it. First step: full, machine-readable instrumentation, so a run explains itself without
anyone watching it.

## Done

- **Synced 154 upstream commits** into `dev` (merge `052fddf`).
- **Security review of those commits:** four AI reviewers read all 19,001 diff lines; findings were
  checked against the code. No malicious code found. Older code and dependencies were not reviewed.
- **Hourly sync is off.** The `fork-sync` workflow is disabled on GitHub. Syncing is manual.
- **Telemetry spec rewritten** (`telemetry.md`). Awaiting the user's final review; nothing is built.
- **`CLAUDE.md` hard limits** added (no game data, gated merges, commit only when asked).
- **Merge protection, first version** (`scripts/dev/pr-gate.py`, `merge-pr.py`, `merge-guard.py`,
  `.githooks/pre-merge-commit`, `test_merge_protection.py`): 60 tests passed on 2026-11-25, but
  review found gaps (below).

## Left to do, in order

1. **Finish merge protection.** Do not merge outside code until this is done. The seven fixes are in
   `.flowestra/decisions.md`, "Protection review refinements". The pushed files
   are the first version (60 tests pass) and contain none of
   them. A stopped attempt at them is not in the repository. Main ones:
   - merges must run with an empty hooks folder (an incoming `.githooks` file could run mid-merge);
   - refuse changes to the protection files unless explicitly allowed;
   - the hook must read `GITHEAD_<id>` variables (git writes `MERGE_HEAD` only after the hook);
   - ask before shell commands write review or approval records, or edit the guard;
   - cover `timeout`, PowerShell `iex` / `Start-Process`, and `patch`;
   - binary files must be named in the review.
   Then repeat an adversarial review before relying on it.
2. **User reviews `telemetry.md`.** No telemetry code before approval.
3. **Build telemetry parts 1 to 6** from the spec, in order. Tests use `tests/testtitle` and
   synthetic shaders only.
4. **Split-wave exchange report:** build it cleanly on a new
   branch, `git switch -c shaders/wave64-exchange-report origin/main` (the old one was local and
   empty), so it could be shared. Decide later whether to send it.

## Open questions for the user

1. Approve `telemetry.md`, or change it?
2. Which title, if any, will be measured? Unanswered. It does not block parts 1 to 6.
3. Publish the exchange report upstream once its numbers are known?
4. Report the two test findings below upstream? Nothing has been posted.

Settled: D1 to D8 are in `telemetry.md`. Code goes in `log.rs` and `report.rs`; overhead is checked
from part 2; every outside change needs a full diff review plus the user's approval; building
reviewed code locally is fine.

## Known test state

At `a67c592` on an RTX 5080, driver 621.01, Rust 1.100.0:
**1,285 passed, 5 failed, 46 ignored, 26 skip messages.**

- `a_bridged_mask_write_where_one_half_is_elsewhere_meets_no_barrier`: stale expectation. #2074's
  test met #2203's change when merged. It also fails in upstream's CI. Not run: turning
  `can_wave_branch` off to confirm. Matters only with `XCL_PER_LANE=1`.
- `fuzz_validate_and_gpu_0` to `_3` (seeds 70, 545, 1019, 1507): the NVIDIA driver refuses the
  pipeline (`0x8007000E`), alone and with `XCL_DXIL_NO_OPT=all`. Likely cause: groupshared
  subtraction, where upstream's workaround does not hold on this driver. Not memory exhaustion.

Bugs found in review, none looking deliberate, none fixed here:

| Priority | Bug | From |
|---|---|---|
| Medium | `XCL_NETWORK=0` lets `127.<anything>` names reach real DNS (`netoff.rs:53`) | older; #2312 extended it |
| Low | a huge split dispatch can exhaust memory (`pso.rs` `dispatch_slabs`) | `e9ddf17` |
| Low | sampler copies can pass the end of a short page (`console_heap.rs` `publish_range`) | #1991 |
| Low | video memory may be freed while another queue uses it (`vram.rs`); unconfirmed in practice | #1985 |
| Low | fresh translations skip the container check (`pso.rs` `cached_rerouted`) | #2120 |
| Low | shader cache entries are not checked against tampering | older; #2080 widened it |
| Low | `XMemVirtualQuery` writes 16 bytes at +48; real size unknown | #2048 |

## Facts not to get wrong

- Nothing has been measured on this hardware. Upstream's README says about 5 fps in the 3D world on
  an RTX 5090. The often-quoted 1 fps is another project on another card.
- The loop cap is 65,536, not 4 million.
- `XCL_WAVE64_SPIN=0` affects only groups of one console wave, not the multi-wave case.
- None of these has merged (checked 2026-11-25): #1193, #2343, #1596, #1597, #1111, #2377, #2097,
  #2210, #917.
- Telemetry must not reuse the timestamp pool in `list_x.rs`.

## Installed on the old PC

| Tool | Version | Installed by us |
|---|---|---|
| Rust (through rustup 1.29.3) | 1.100.0, `stable-x86_64-pc-windows-msvc` | yes, with winget |
| Gaming Services | 39.118.30001.0, gives `xgameruntime.dll` | yes: first attempted with winget, present later |
| Visual Studio C++ x64 tools | Visual Studio 18 Community | already there |
| Windows SDK / DXC | 10.0.26100.0 / 1.8.2502.11 | already there |
| Python, GitHub CLI | 3.14.2, signed in | already there |
| `core.hooksPath` | `.githooks` | yes, for that clone only |

Cargo is in `%USERPROFILE%\.cargo\bin`; Git Bash needed it added to `PATH`.

## Setting up the other PC

Windows 10 or 11 x64. These do not travel with git:

1. Visual Studio with the C++ x64 tools, the Windows SDK (DXC 1.8 or newer), Python 3.12 or newer,
   and the GitHub CLI signed in (`gh auth login`).
2. `winget install --id Rustlang.Rustup --exact`, toolchain `stable-x86_64-pc-windows-msvc`.
3. Microsoft Store: Gaming Services (gives `xgameruntime.dll`; needed only to run a title).
4. `git clone https://github.com/Osanchez/xcl.git`, then `git switch dev`.
5. `git config core.hooksPath .githooks` (set per clone).
6. `python -I scripts/dev/test_merge_protection.py`, then
   `cargo test --workspace --no-fail-fast -- --test-threads=4` (do not pipe it through `tail`: that
   hides the exit code). A different graphics card gives different skips and failures.

Review records (`.git/pr-gate/`), logs and Claude's session history and memory stay on the old PC.
Nothing needed depends on them.

To resume, tell Claude: "Read docs/guide/resume-here.md and continue with item 1."
